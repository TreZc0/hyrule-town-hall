use {
    chrono::{Duration, Utc},
    serenity::all::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage},
    serenity::model::id::{ChannelId, RoleId},
    serenity_utils::message::TimestampStyle,
    sqlx::{PgPool, Postgres, Transaction},
    std::{collections::HashMap, mem},
    crate::{
        cal::{Entrant, Entrants, Race, RaceSchedule},
        discord_bot::PgSnowflake,
        event::{self, roles::{EffectiveRoleBinding, Signup, VolunteerSignupStatus}},
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

    // Build and post the message
    let message = build_announcement_message(
        &races_needing_volunteers,
        event_data,
        ping_enabled,
    );

    // Post to Discord
    if let Err(e) = channel_id.send_message(discord_ctx, message).await {
        eprintln!("Failed to post volunteer request to Discord: {}", e);
        return Err(e.into());
    }

    // Mark all races as notified
    for need in &races_needing_volunteers {
        sqlx::query!(
            "UPDATE races SET volunteer_request_sent = true WHERE id = $1",
            need.race.id as _
        )
        .execute(&mut **transaction)
        .await?;
    }

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

    // Add round info if available
    if let Some(round) = &race.round {
        Ok(format!("{} ({})", matchup, round))
    } else if let Some(phase) = &race.phase {
        Ok(format!("{} ({})", matchup, phase))
    } else {
        Ok(matchup)
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

/// Builds the Discord announcement message with signup buttons.
fn build_announcement_message(
    needs: &[RaceVolunteerNeed],
    event_data: &event::Data<'_>,
    ping_enabled: bool,
) -> CreateMessage {
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

    msg.push("**Volunteers Needed for ");
    msg.push(&event_data.display_name);
    msg.push("**\n\n");

    for need in needs {
        // Race matchup and time
        msg.push("**");
        msg.push(&need.matchup);
        msg.push("** - ");

        if let RaceSchedule::Live { start, .. } = need.race.schedule {
            msg.push_timestamp(start, TimestampStyle::LongDateTime);
        }
        msg.push("\n");

        // Role needs
        for role_need in &need.role_needs {
            msg.push("  - ");
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
        msg.push("\n");
    }

    // Add signup link
    msg.push(format!("Sign up: <{}/event/{}/{}/volunteer-roles>", base_uri(), event_data.series.slug(), &*event_data.event));

    // Build buttons for each race (max 5 buttons per row, max 5 rows = 25 buttons)
    let mut components = Vec::new();
    let mut current_row = Vec::new();

    for need in needs {
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

    CreateMessage::new()
        .content(msg.build())
        .components(components)
}
