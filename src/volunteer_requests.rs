use {
    chrono::{DateTime, Duration, Utc},
    serenity::all::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage, EditMessage},
    serenity::model::id::{ChannelId, MessageId, RoleId},
    serenity_utils::message::TimestampStyle,
    sqlx::{PgPool, Postgres, Transaction},
    std::collections::{BTreeMap, HashMap},
    std::mem,
    crate::{
        cal::{Entrant, Entrants, Race, RaceSchedule},
        discord_bot::PgSnowflake,
        event::{self, roles::{EffectiveRoleBinding, Signup, VolunteerSignupStatus}},
        id::{Id, Races},
        lang::Language,
        prelude::*,
        series::Series,
    },
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] Event(#[from] event::DataError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Serenity(#[from] serenity::Error),
}

/// Details about a volunteer role that needs filling.
#[allow(dead_code)]
struct RoleNeed {
    role_name: String,
    discord_role_id: Option<i64>,
    confirmed_names: Vec<String>,
    confirmed_count: i32,
    pending_count: i32,
    min_count: i32,
    max_count: i32,
    needs_ping: bool,
    language: Language,
}

/// Represents a match that needs volunteers, with details about which roles need filling.
struct RaceVolunteerNeed {
    race: Race,
    /// The matchup description (e.g., "Team A vs Team B")
    matchup: String,
    role_needs: Vec<RoleNeed>,
}

/// Result of a manual volunteer request check.
pub(crate) enum CheckResult {
    /// No races needed volunteer announcements.
    NoRacesNeeded,
    /// Posted announcement for the given number of races.
    Posted(usize),
    /// Volunteer requests are not enabled for this event.
    NotEnabled,
    /// No volunteer info channel configured.
    NoChannel,
}

/// Checks for upcoming races needing volunteers and posts announcements to Discord.
/// This function is called every 30 minutes from the main loop.
pub(crate) async fn check_and_post_volunteer_requests(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;

    // Get all events with volunteer requests enabled
    let enabled_events = sqlx::query!(
        r#"SELECT
            series AS "series: Series",
            event,
            volunteer_request_lead_time_hours,
            volunteer_request_ping_enabled,
            discord_volunteer_info_channel AS "discord_volunteer_info_channel: PgSnowflake<ChannelId>"
        FROM events
        WHERE volunteer_requests_enabled = true
          AND discord_volunteer_info_channel IS NOT NULL"#
    )
    .fetch_all(&mut *transaction)
    .await?;

    for event_row in enabled_events {
        let lead_time = Duration::hours(event_row.volunteer_request_lead_time_hours as i64);
        let ping_enabled = event_row.volunteer_request_ping_enabled;
        let channel_id = match event_row.discord_volunteer_info_channel {
            Some(PgSnowflake(id)) => id,
            None => continue,
        };

        // Get event data for display name
        let event_data = match event::Data::new(&mut transaction, event_row.series, &event_row.event).await? {
            Some(data) => data,
            None => continue,
        };

        let _ = post_volunteer_requests_for_event(
            &mut transaction,
            discord_ctx,
            &event_data,
            channel_id,
            lead_time,
            ping_enabled,
        ).await;
    }

    transaction.commit().await?;
    Ok(())
}

/// Checks and posts volunteer requests for a single event.
/// Returns the result of the check.
pub(crate) async fn check_and_post_for_event(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    series: Series,
    event: &str,
) -> Result<CheckResult, Error> {
    let mut transaction = pool.begin().await?;

    // Get event settings
    let event_settings = sqlx::query!(
        r#"SELECT
            volunteer_requests_enabled,
            volunteer_request_lead_time_hours,
            volunteer_request_ping_enabled,
            discord_volunteer_info_channel AS "discord_volunteer_info_channel: PgSnowflake<ChannelId>"
        FROM events
        WHERE series = $1 AND event = $2"#,
        series as _,
        event
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let settings = match event_settings {
        Some(s) => s,
        None => return Ok(CheckResult::NotEnabled),
    };

    if !settings.volunteer_requests_enabled {
        return Ok(CheckResult::NotEnabled);
    }

    let channel_id = match settings.discord_volunteer_info_channel {
        Some(PgSnowflake(id)) => id,
        None => return Ok(CheckResult::NoChannel),
    };

    let lead_time = Duration::hours(settings.volunteer_request_lead_time_hours as i64);
    let ping_enabled = settings.volunteer_request_ping_enabled;

    let event_data = match event::Data::new(&mut transaction, series, event).await? {
        Some(data) => data,
        None => return Ok(CheckResult::NotEnabled),
    };

    let result = post_volunteer_requests_for_event(
        &mut transaction,
        discord_ctx,
        &event_data,
        channel_id,
        lead_time,
        ping_enabled,
    ).await?;

    transaction.commit().await?;
    Ok(result)
}

/// Posts volunteer requests for a specific event. Used by both the background task and manual trigger.
async fn post_volunteer_requests_for_event(
    transaction: &mut Transaction<'_, Postgres>,
    discord_ctx: &DiscordCtx,
    event_data: &event::Data<'_>,
    channel_id: ChannelId,
    lead_time: Duration,
    ping_enabled: bool,
) -> Result<CheckResult, Error> {
    let now = Utc::now();
    let cutoff = now + lead_time;

    // Find races needing volunteer announcements
    let races_needing_volunteers = get_races_needing_announcements(
        transaction,
        event_data.series,
        &event_data.event,
        lead_time,
    ).await?;

    if races_needing_volunteers.is_empty() {
        return Ok(CheckResult::NoRacesNeeded);
    }

    let count = races_needing_volunteers.len();

    // Check if there's an existing active post we can add to
    // Find a message ID from a race that hasn't started yet in this event
    let existing_message = sqlx::query_scalar!(
        r#"SELECT volunteer_request_message_id AS "volunteer_request_message_id: PgSnowflake<MessageId>"
        FROM races
        WHERE series = $1
          AND event = $2
          AND volunteer_request_message_id IS NOT NULL
          AND start > $3
          AND ignored = false
        ORDER BY start ASC
        LIMIT 1"#,
        event_data.series as _,
        &*event_data.event,
        now
    )
    .fetch_optional(&mut **transaction)
    .await?
    .flatten();

    let message_id = if let Some(PgSnowflake(existing_id)) = existing_message {
        // Add new races to the existing post
        // First, mark them as notified with the existing message ID
        for need in &races_needing_volunteers {
            sqlx::query!(
                "UPDATE races SET volunteer_request_sent = true, volunteer_request_message_id = $2 WHERE id = $1",
                need.race.id as _,
                PgSnowflake(existing_id) as _
            )
            .execute(&mut **transaction)
            .await?;
        }

        // Now update the post using the existing update logic
        // Get all races that share this message ID
        let http_client = reqwest::Client::new();
        let all_race_ids = sqlx::query_scalar!(
            r#"SELECT id AS "id: Id<Races>"
            FROM races
            WHERE volunteer_request_message_id = $1
              AND ignored = false
            ORDER BY start ASC NULLS LAST"#,
            PgSnowflake(existing_id) as _
        )
        .fetch_all(&mut **transaction)
        .await?;

        // Build volunteer needs for all races in this post
        let mut all_needs = Vec::new();
        for rid in &all_race_ids {
            let race = Race::from_id(&mut *transaction, &http_client, *rid).await?;

            let matchup = get_matchup_description(&mut *transaction, &race).await?;
            let role_bindings = EffectiveRoleBinding::for_event(&mut *transaction, event_data.series, &event_data.event).await?;
            let signups = Signup::for_race(&mut *transaction, *rid).await?;

            let mut role_needs = Vec::new();
            for binding in &role_bindings {
                if binding.is_disabled {
                    continue;
                }

                let confirmed_signups: Vec<_> = signups.iter()
                    .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Confirmed))
                    .collect();
                let confirmed_count = confirmed_signups.len() as i32;

                let mut confirmed_names = Vec::new();
                for signup in &confirmed_signups {
                    if let Ok(Some(user)) = User::from_id(&mut **transaction, signup.user_id).await {
                        confirmed_names.push(user.display_name().to_owned());
                    }
                }

                let pending_count = signups.iter()
                    .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Pending))
                    .count() as i32;

                if confirmed_count < binding.max_count || pending_count > 0 {
                    role_needs.push(RoleNeed {
                        role_name: binding.role_type_name.clone(),
                        discord_role_id: binding.discord_role_id,
                        confirmed_names,
                        confirmed_count,
                        pending_count,
                        min_count: binding.min_count,
                        max_count: binding.max_count,
                        needs_ping: confirmed_count < binding.min_count,
                        language: binding.language,
                    });
                }
            }

            if !role_needs.is_empty() {
                all_needs.push(RaceVolunteerNeed {
                    race,
                    matchup,
                    role_needs,
                });
            }
        }

        // Build and edit the message (ping only for new races)
        let (content, components) = build_announcement_content(
            &all_needs,
            event_data,
            ping_enabled,
            now,
            cutoff,
        );

        if let Err(e) = channel_id.edit_message(
            discord_ctx,
            existing_id,
            EditMessage::new()
                .content(content)
                .components(components)
        ).await {
            eprintln!("Failed to update volunteer request message with new races: {}", e);
        }

        existing_id
    } else {
        // Create a new post
        let message = build_announcement_message(
            &races_needing_volunteers,
            event_data,
            ping_enabled,
            now,
            cutoff,
        );

        let posted_message = match channel_id.send_message(discord_ctx, message).await {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to post volunteer request to Discord: {}", e);
                return Err(e.into());
            }
        };

        // Mark all races as notified and store the message ID
        for need in &races_needing_volunteers {
            sqlx::query!(
                "UPDATE races SET volunteer_request_sent = true, volunteer_request_message_id = $2 WHERE id = $1",
                need.race.id as _,
                PgSnowflake(posted_message.id) as _
            )
            .execute(&mut **transaction)
            .await?;
        }

        posted_message.id
    };

    let _ = message_id; // Suppress unused warning
    Ok(CheckResult::Posted(count))
}

/// Finds races that need volunteer announcements for a specific event.
async fn get_races_needing_announcements(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    lead_time: Duration,
) -> Result<Vec<RaceVolunteerNeed>, Error> {
    let now = Utc::now();
    let cutoff = now + lead_time;
    let http_client = reqwest::Client::new();

    // Get all races for this event that haven't been announced yet
    let race_ids = sqlx::query_scalar!(
        r#"SELECT id AS "id: crate::id::Id<crate::id::Races>"
        FROM races
        WHERE series = $1
          AND event = $2
          AND volunteer_request_sent = false
          AND ignored = false
          AND start IS NOT NULL
          AND start > $3
          AND start <= $4
          AND async_start1 IS NULL"#,
        series as _,
        event,
        now,
        cutoff
    )
    .fetch_all(&mut **transaction)
    .await?;

    let mut needs = Vec::new();

    for race_id in race_ids {
        let race = Race::from_id(&mut *transaction, &http_client, race_id).await?;

        // Skip if not a live scheduled race
        let _start_time = match race.schedule {
            RaceSchedule::Live { start, .. } => start,
            _ => continue,
        };

        // Check restream consent - all teams must have consented
        // Skip this check for qualifier races since they're open to all entrants
        let is_qualifier = race.phase.as_ref().is_some_and(|p| p == "Qualifier");
        if !is_qualifier {
            if let Some(mut teams) = race.teams_opt() {
                if !teams.all(|team| team.restream_consent) {
                    continue;
                }
            } else {
                // Not all entrants are Mido's House teams, skip
                continue;
            }
        }

        // Get matchup description
        let matchup = get_matchup_description(&mut *transaction, &race).await?;

        // Get role bindings and check volunteer counts
        let role_bindings = EffectiveRoleBinding::for_event(&mut *transaction, series, event).await?;
        let signups = Signup::for_race(&mut *transaction, race_id).await?;

        let mut role_needs = Vec::new();
        let mut has_any_need = false;

        for binding in &role_bindings {
            if binding.is_disabled {
                continue;
            }

            // Get confirmed signups for this role
            let confirmed_signups: Vec<_> = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Confirmed))
                .collect();
            let confirmed_count = confirmed_signups.len() as i32;

            // Get names of confirmed volunteers
            let mut confirmed_names = Vec::new();
            for signup in &confirmed_signups {
                if let Ok(Some(user)) = User::from_id(&mut **transaction, signup.user_id).await {
                    confirmed_names.push(user.display_name().to_owned());
                }
            }

            // Count pending volunteers for this role
            let pending_count = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Pending))
                .count() as i32;

            // Check if volunteers are needed
            if confirmed_count < binding.max_count {
                let needs_ping = confirmed_count < binding.min_count;
                role_needs.push(RoleNeed {
                    role_name: binding.role_type_name.clone(),
                    discord_role_id: binding.discord_role_id,
                    confirmed_names,
                    confirmed_count,
                    pending_count,
                    min_count: binding.min_count,
                    max_count: binding.max_count,
                    needs_ping,
                    language: binding.language,
                });
                has_any_need = true;
            }
        }

        if has_any_need {
            needs.push(RaceVolunteerNeed {
                race,
                matchup,
                role_needs,
            });
        }
    }

    // Sort by start time
    needs.sort_by_key(|n| match n.race.schedule {
        RaceSchedule::Live { start, .. } => start,
        _ => Utc::now(), // shouldn't happen
    });

    Ok(needs)
}

/// Gets a human-readable matchup description for a race.
async fn get_matchup_description(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
) -> Result<String, Error> {
    // For qualifier races, just use the round name (e.g., "Live 1")
    if race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
        return Ok(race.round.clone().unwrap_or_else(|| "Qualifier".to_string()));
    }

    let matchup = match &race.entrants {
        Entrants::Two([e1, e2]) => {
            let name1 = get_entrant_name(transaction, e1).await?;
            let name2 = get_entrant_name(transaction, e2).await?;
            format!("{} vs {}", name1, name2)
        }
        Entrants::Three([e1, e2, e3]) => {
            let name1 = get_entrant_name(transaction, e1).await?;
            let name2 = get_entrant_name(transaction, e2).await?;
            let name3 = get_entrant_name(transaction, e3).await?;
            format!("{} vs {} vs {}", name1, name2, name3)
        }
        _ => "Unknown matchup".to_string(),
    };

    // Add round and/or phase info if available
    match (&race.round, &race.phase) {
        (Some(round), Some(phase)) => Ok(format!("{} ({}, {})", matchup, round, phase)),
        (Some(round), None) => Ok(format!("{} ({})", matchup, round)),
        (None, Some(phase)) => Ok(format!("{} ({})", matchup, phase)),
        (None, None) => Ok(matchup),
    }
}

/// Gets a display name for an entrant.
async fn get_entrant_name(
    transaction: &mut Transaction<'_, Postgres>,
    entrant: &Entrant,
) -> Result<String, Error> {
    Ok(match entrant {
        Entrant::MidosHouseTeam(team) => {
            team.name(transaction).await
                .ok()
                .flatten()
                .map(|n| n.into_owned())
                .unwrap_or_else(|| "Unknown Team".to_string())
        }
        Entrant::Named { name, .. } => name.clone(),
        Entrant::Discord { .. } => "Discord User".to_string(),
    })
}

/// Truncates a string to the specified maximum length, adding "..." if truncated.
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut cutoff = max_len.saturating_sub(3);
        while cutoff > 0 && !s.is_char_boundary(cutoff) {
            cutoff -= 1;
        }
        format!("{}...", &s[..cutoff])
    }
}

/// Formats a date range for display in Discord.
fn format_date_range(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    if start.month() != end.month() {
        format!("{} - {}", start.format("%b %-d"), end.format("%b %-d"))
    } else if start.day() != end.day() {
        format!("{} - {}", start.format("%b %-d"), end.format("%-d"))
    } else {
        start.format("%b %-d").to_string()
    }
}

/// Builds the content and components for a volunteer announcement message.
/// Returns (content_string, components_vec).
fn build_announcement_content(
    needs: &[RaceVolunteerNeed],
    event_data: &event::Data<'_>,
    ping_enabled: bool,
    time_window_start: DateTime<Utc>,
    time_window_end: DateTime<Utc>,
) -> (String, Vec<CreateActionRow>) {
    let mut msg = MessageBuilder::default();

    // Collect all roles that need pinging
    let mut roles_to_ping: HashMap<i64, String> = HashMap::new();
    if ping_enabled {
        for need in needs {
            for role_need in &need.role_needs {
                if role_need.needs_ping {
                    if let Some(role_id) = role_need.discord_role_id {
                        roles_to_ping.insert(role_id, role_need.role_name.clone());
                    }
                }
            }
        }
    }

    // Add role pings at the top
    if !roles_to_ping.is_empty() {
        for role_id in roles_to_ping.keys() {
            msg.role(RoleId::new(*role_id as u64));
            msg.push(" ");
        }
        msg.push("\n\n");
    }

    msg.push("**Open Races for ");
    msg.push(&event_data.display_name);
    msg.push(" (");
    msg.push(&format_date_range(time_window_start, time_window_end));
    msg.push(")**\n\n");

    for need in needs {
        // Race matchup and time
        msg.push("**");
        msg.push(&need.matchup);
        msg.push("** - ");

        if let RaceSchedule::Live { start, .. } = need.race.schedule {
            msg.push_timestamp(start, TimestampStyle::LongDateTime);
        }
        msg.push("\n");

        // Restream info
        if need.race.video_urls.is_empty() {
            msg.push("Restream TBD\n");
        } else {
            for (language, video_url) in &need.race.video_urls {
                msg.push("Restream (");
                msg.push(&language.to_string());
                msg.push("): <");
                msg.push(&video_url.to_string());
                msg.push(">\n");
            }
        }

        // Group role needs by language
        let mut roles_by_language: BTreeMap<Language, Vec<&RoleNeed>> = BTreeMap::new();
        for role_need in &need.role_needs {
            roles_by_language
                .entry(role_need.language)
                .or_default()
                .push(role_need);
        }

        // Display roles grouped by language
        for (language, roles) in &roles_by_language {
            msg.push("**");
            msg.push(&language.to_string());
            msg.push(":**\n");

            for role_need in roles {
                msg.push("- ");
                msg.push(&role_need.role_name);
                msg.push(": ");
                if !role_need.confirmed_names.is_empty() {
                    msg.push(&role_need.confirmed_names.join(", "));
                    msg.push(" ");
                } else {
                    msg.push("none yet ");
                }
                msg.push("(");
                msg.push(&role_need.confirmed_count.to_string());
                msg.push("/");
                msg.push(&role_need.max_count.to_string());
                msg.push(")");
                if role_need.pending_count > 0 {
                    msg.push(" (");
                    msg.push(&role_need.pending_count.to_string());
                    msg.push(" pending)");
                }
                msg.push("\n");
            }
        }
        msg.push("\n");
    }

    // Add signup link
    msg.push(format!("Sign up through the website or the buttons below: <{}/event/{}/{}/volunteer-roles>", base_uri(), event_data.series.slug(), &*event_data.event));

    // Build buttons for each race (max 5 buttons per row, max 5 rows = 25 buttons)
    // Skip races that have already started
    let now = Utc::now();
    let mut components = Vec::new();
    let mut current_row = Vec::new();

    for need in needs {
        // Skip races that have already started - no signup button needed
        let race_started = match need.race.schedule {
            RaceSchedule::Live { start, .. } => start <= now,
            _ => false,
        };
        if race_started {
            continue;
        }

        // Button label: truncate matchup to fit Discord's 80 char limit
        let label = format!("Sign up: {}", truncate_string(&need.matchup, 60));
        let button = CreateButton::new(format!("volunteer_signup_{}", u64::from(need.race.id)))
            .label(label)
            .style(ButtonStyle::Primary);

        current_row.push(button);

        // Discord allows max 5 buttons per row
        if current_row.len() >= 5 {
            components.push(CreateActionRow::Buttons(mem::take(&mut current_row)));
            // Max 5 rows total
            if components.len() >= 5 {
                break;
            }
        }
    }

    // Add any remaining buttons
    if !current_row.is_empty() && components.len() < 5 {
        components.push(CreateActionRow::Buttons(current_row));
    }

    (msg.build(), components)
}

/// Builds the Discord announcement message with signup buttons.
fn build_announcement_message(
    needs: &[RaceVolunteerNeed],
    event_data: &event::Data<'_>,
    ping_enabled: bool,
    time_window_start: DateTime<Utc>,
    time_window_end: DateTime<Utc>,
) -> CreateMessage {
    let (content, components) = build_announcement_content(
        needs,
        event_data,
        ping_enabled,
        time_window_start,
        time_window_end,
    );
    CreateMessage::new()
        .content(content)
        .components(components)
}

/// Updates the volunteer request post for a race when signups change.
/// This will update the entire post since one post can contain multiple races.
pub(crate) async fn update_volunteer_post_for_race(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    race_id: Id<Races>,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;
    let http_client = reqwest::Client::new();

    // Get the race's message ID and event info
    let race_info = sqlx::query!(
        r#"SELECT
            r.series AS "series: Series",
            r.event,
            r.volunteer_request_message_id AS "volunteer_request_message_id: PgSnowflake<MessageId>",
            e.discord_volunteer_info_channel AS "discord_volunteer_info_channel: PgSnowflake<ChannelId>",
            e.volunteer_request_lead_time_hours
        FROM races r
        JOIN events e ON r.series = e.series AND r.event = e.event
        WHERE r.id = $1"#,
        race_id as _
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let race_info = match race_info {
        Some(info) => info,
        None => return Ok(()), // Race not found
    };

    // If no message ID, the post wasn't sent yet - nothing to update
    let message_id = match race_info.volunteer_request_message_id {
        Some(PgSnowflake(id)) => id,
        None => return Ok(()),
    };

    let channel_id = match race_info.discord_volunteer_info_channel {
        Some(PgSnowflake(id)) => id,
        None => return Ok(()), // No channel configured
    };

    // Find ALL races that share the same message ID
    let race_ids = sqlx::query_scalar!(
        r#"SELECT id AS "id: Id<Races>"
        FROM races
        WHERE volunteer_request_message_id = $1
          AND ignored = false
        ORDER BY start ASC NULLS LAST"#,
        PgSnowflake(message_id) as _
    )
    .fetch_all(&mut *transaction)
    .await?;

    if race_ids.is_empty() {
        return Ok(());
    }

    // Get event data for display name
    let event_data = match event::Data::new(&mut transaction, race_info.series, &race_info.event).await? {
        Some(data) => data,
        None => return Ok(()),
    };

    // Build volunteer needs for all races in this post
    let mut needs = Vec::new();
    for rid in &race_ids {
        let race = Race::from_id(&mut transaction, &http_client, *rid).await?;

        // Get matchup description
        let matchup = get_matchup_description(&mut transaction, &race).await?;

        // Get role bindings and check volunteer counts
        let role_bindings = EffectiveRoleBinding::for_event(&mut transaction, race_info.series, &race_info.event).await?;
        let signups = Signup::for_race(&mut transaction, *rid).await?;

        let mut role_needs = Vec::new();
        let mut has_any_need = false;

        for binding in &role_bindings {
            if binding.is_disabled {
                continue;
            }

            // Get confirmed signups for this role
            let confirmed_signups: Vec<_> = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Confirmed))
                .collect();
            let confirmed_count = confirmed_signups.len() as i32;

            // Get names of confirmed volunteers
            let mut confirmed_names = Vec::new();
            for signup in &confirmed_signups {
                if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                    confirmed_names.push(user.display_name().to_owned());
                }
            }

            // Count pending volunteers for this role
            let pending_count = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Pending))
                .count() as i32;

            // Check if volunteers are needed (or if there are any signups to show)
            if confirmed_count < binding.max_count || pending_count > 0 {
                let needs_ping = confirmed_count < binding.min_count;
                role_needs.push(RoleNeed {
                    role_name: binding.role_type_name.clone(),
                    discord_role_id: binding.discord_role_id,
                    confirmed_names,
                    confirmed_count,
                    pending_count,
                    min_count: binding.min_count,
                    max_count: binding.max_count,
                    needs_ping,
                    language: binding.language,
                });
                has_any_need = true;
            }
        }

        if has_any_need || !role_needs.is_empty() {
            needs.push(RaceVolunteerNeed {
                race,
                matchup,
                role_needs,
            });
        }
    }

    // If no needs remain, we still want to show the races with their filled status
    // But if there are truly no races, just return
    if needs.is_empty() {
        return Ok(());
    }

    // Calculate time window (use lead time from event)
    let now = Utc::now();
    let lead_time = Duration::hours(race_info.volunteer_request_lead_time_hours as i64);
    let cutoff = now + lead_time;

    // Build the updated message content (without pings on updates)
    let (content, components) = build_announcement_content(
        &needs,
        &event_data,
        false, // Don't ping on updates
        now,
        cutoff,
    );

    // Edit the message
    if let Err(e) = channel_id.edit_message(
        discord_ctx,
        message_id,
        EditMessage::new()
            .content(content)
            .components(components)
    ).await {
        eprintln!("Failed to update volunteer request message: {}", e);
        // Don't return error - this is a best-effort update
    }

    transaction.commit().await?;
    Ok(())
}
