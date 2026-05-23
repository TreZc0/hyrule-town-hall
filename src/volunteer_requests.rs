use {
    chrono::{DateTime, Duration, Utc},
    serenity::all::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage, EditMessage},
    serenity::http::HttpError,
    serenity::model::{ModelError, id::{ChannelId, MessageId}},
    serenity_utils::message::TimestampStyle,
    sqlx::{PgPool, Postgres, Transaction},
    std::collections::BTreeMap,
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

fn is_unknown_message(e: &serenity::Error) -> bool {
    matches!(
        e,
        serenity::Error::Http(HttpError::UnsuccessfulRequest(res)) if res.error.code == 10008
    )
}

fn is_message_too_long(e: &serenity::Error) -> bool {
    matches!(e, serenity::Error::Model(ModelError::MessageTooLong(_)))
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
    language: Language,
    is_full: bool,
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
/// This function is called every 10 minutes from the main loop.
pub(crate) async fn check_and_post_volunteer_requests(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
) -> Result<(), Error> {
    // Get all events with volunteer requests enabled
    let enabled_events = sqlx::query!(
        r#"SELECT
            series AS "series: Series",
            event,
            volunteer_request_lead_time_hours,
            discord_volunteer_info_channel AS "discord_volunteer_info_channel: PgSnowflake<ChannelId>"
        FROM events
        WHERE volunteer_requests_enabled = true
          AND discord_volunteer_info_channel IS NOT NULL"#
    )
    .fetch_all(pool)
    .await?;

    for event_row in enabled_events {
        let lead_time = Duration::hours(event_row.volunteer_request_lead_time_hours as i64);
        let channel_id = match event_row.discord_volunteer_info_channel {
            Some(PgSnowflake(id)) => id,
            None => continue,
        };

        // Get event data for display name
        let event_data = {
            let mut transaction = pool.begin().await?;
            let data = event::Data::new(&mut transaction, event_row.series, &event_row.event).await?;
            transaction.commit().await?;
            match data {
                Some(data) => data,
                None => continue,
            }
        };

        let _ = post_volunteer_requests_for_event(
            pool,
            discord_ctx,
            &event_data,
            channel_id,
            lead_time,
        ).await;

        // Always refresh existing posts to remove races that have since started
        let _ = update_volunteer_posts_for_event(
            pool,
            discord_ctx,
            event_row.series,
            &event_row.event,
        ).await;
    }

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
    // Get event settings
    let event_settings = sqlx::query!(
        r#"SELECT
            volunteer_requests_enabled,
            volunteer_request_lead_time_hours,
            discord_volunteer_info_channel AS "discord_volunteer_info_channel: PgSnowflake<ChannelId>"
        FROM events
        WHERE series = $1 AND event = $2"#,
        series as _,
        event
    )
    .fetch_optional(pool)
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

    let event_data = {
        let mut transaction = pool.begin().await?;
        let data = event::Data::new(&mut transaction, series, event).await?;
        transaction.commit().await?;
        match data {
            Some(data) => data,
            None => return Ok(CheckResult::NotEnabled),
        }
    };

    let result = post_volunteer_requests_for_event(
        pool,
        discord_ctx,
        &event_data,
        channel_id,
        lead_time,
    ).await?;

    Ok(result)
}

/// Posts volunteer requests for a specific event. Used by both the background task and manual trigger.
async fn post_volunteer_requests_for_event(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    event_data: &event::Data<'_>,
    channel_id: ChannelId,
    lead_time: Duration,
) -> Result<CheckResult, Error> {
    const MAX_RACES_PER_POST: usize = 5;

    let now = Utc::now();
    let cutoff = now + lead_time;
    let mut transaction = pool.begin().await?;

    // Find races needing volunteer announcements
    let races_needing_volunteers = get_races_needing_announcements(
        &mut transaction,
        event_data.series,
        &event_data.event,
        lead_time,
    ).await?;

    let count = races_needing_volunteers.len();

    // Find all existing active posts with their current race counts, ordered by earliest race.
    // We'll fill these up to MAX_RACES_PER_POST before creating new posts.
    let existing_messages = sqlx::query!(
        r#"SELECT
            volunteer_request_message_id AS "message_id: PgSnowflake<MessageId>",
            COUNT(*) AS "race_count!: i64"
        FROM races
        WHERE series = $1
          AND event = $2
          AND volunteer_request_message_id IS NOT NULL
          AND volunteer_request_sent = true
          AND start > $3
          AND ignored = false
        GROUP BY volunteer_request_message_id
        ORDER BY MIN(start) ASC"#,
        event_data.series as _,
        &*event_data.event,
        now
    )
    .fetch_all(&mut *transaction)
    .await?;

    if races_needing_volunteers.is_empty() && existing_messages.is_empty() {
        transaction.commit().await?;
        return Ok(CheckResult::NoRacesNeeded);
    }

    struct ExistingPostPlan {
        message_id: MessageId,
        add_race_ids: Vec<Id<Races>>,
        content: String,
        components: Vec<CreateActionRow>,
    }

    let http_client = reqwest::Client::new();
    let mut existing_post_plans = Vec::new();
    let mut remaining = races_needing_volunteers.as_slice();

    // Plan updates for existing posts while the transaction is open.
    for existing in &existing_messages {
        let existing_id = match existing.message_id {
            Some(PgSnowflake(id)) => id,
            None => continue,
        };

        let mut add_race_ids = Vec::new();
        if !remaining.is_empty() {
            let current_count = existing.race_count as usize;
            if current_count < MAX_RACES_PER_POST {
                let slots = MAX_RACES_PER_POST - current_count;
                let to_add = remaining.len().min(slots);
                let (chunk, rest) = remaining.split_at(to_add);
                remaining = rest;
                add_race_ids.extend(chunk.iter().map(|need| need.race.id));
            }
        }

        let mut all_race_ids = sqlx::query_scalar!(
            r#"SELECT id AS "id: Id<Races>"
            FROM races
            WHERE volunteer_request_message_id = $1
              AND ignored = false
            ORDER BY start ASC NULLS LAST"#,
            PgSnowflake(existing_id) as _
        )
        .fetch_all(&mut *transaction)
        .await?;
        all_race_ids.extend(add_race_ids.iter().copied());

        let all_needs = build_volunteer_needs_for_race_ids(
            &mut transaction,
            event_data,
            &all_race_ids,
            &http_client,
        ).await?;

        let (content, components) = build_announcement_content(&all_needs, event_data, now, cutoff);
        existing_post_plans.push(ExistingPostPlan {
            message_id: existing_id,
            add_race_ids,
            content,
            components,
        });
    }

    let mut new_posts = Vec::new();
    for chunk in remaining.chunks(MAX_RACES_PER_POST) {
        let race_ids = chunk.iter().map(|need| need.race.id).collect::<Vec<_>>();
        let message = build_announcement_message(chunk, event_data, now, cutoff);
        new_posts.push((race_ids, message));
    }

    transaction.commit().await?;

    for plan in existing_post_plans {
        if let Err(e) = channel_id.edit_message(
            discord_ctx,
            plan.message_id,
            EditMessage::new().content(plan.content).components(plan.components),
        ).await {
            eprintln!("volunteer post update: failed to edit message {}: {e}", plan.message_id);
            if is_unknown_message(&e) || is_message_too_long(&e) {
                if is_message_too_long(&e) {
                    let _ = channel_id.delete_message(discord_ctx, plan.message_id).await;
                }
                let mut cleanup_tx = pool.begin().await?;
                sqlx::query!(
                    "UPDATE races SET volunteer_request_sent = false, volunteer_request_message_id = NULL \
                     WHERE volunteer_request_message_id = $1",
                    PgSnowflake(plan.message_id) as _
                )
                .execute(&mut *cleanup_tx)
                .await?;
                cleanup_tx.commit().await?;
            }
            continue;
        }

        if !plan.add_race_ids.is_empty() {
            let mut update_tx = pool.begin().await?;
            for race_id in plan.add_race_ids {
                sqlx::query!(
                    "UPDATE races SET volunteer_request_sent = true, volunteer_request_message_id = $2 WHERE id = $1",
                    race_id as _,
                    PgSnowflake(plan.message_id) as _
                )
                .execute(&mut *update_tx)
                .await?;
            }
            update_tx.commit().await?;
        }
    }

    for (race_ids, message) in new_posts {
        let posted_message = match channel_id.send_message(discord_ctx, message).await {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to post volunteer request to Discord: {}", e);
                return Err(e.into());
            }
        };
        let mut update_tx = pool.begin().await?;
        for race_id in race_ids {
            sqlx::query!(
                "UPDATE races SET volunteer_request_sent = true, volunteer_request_message_id = $2 WHERE id = $1",
                race_id as _,
                PgSnowflake(posted_message.id) as _
            )
            .execute(&mut *update_tx)
            .await?;
        }
        update_tx.commit().await?;
    }

    if count == 0 {
        Ok(CheckResult::NoRacesNeeded)
    } else {
        Ok(CheckResult::Posted(count))
    }
}

async fn build_volunteer_needs_for_race_ids(
    transaction: &mut Transaction<'_, Postgres>,
    event_data: &event::Data<'_>,
    race_ids: &[Id<Races>],
    http_client: &reqwest::Client,
) -> Result<Vec<RaceVolunteerNeed>, Error> {
    let role_bindings = EffectiveRoleBinding::for_event(
        &mut *transaction,
        event_data.series,
        &event_data.event,
    ).await?;
    let mut needs = Vec::new();

    for rid in race_ids {
        let race = match Race::from_id(&mut *transaction, http_client, *rid).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("volunteer post update: failed to load race {rid:?}: {e}");
                continue;
            }
        };
        let matchup = get_matchup_description(&mut *transaction, &race).await?;
        let signups = Signup::for_race(&mut *transaction, *rid).await?;
        let (role_needs, has_any_need) = collect_role_needs_for_bindings(
            &mut *transaction,
            &role_bindings,
            &signups,
        ).await?;

        if has_any_need {
            needs.push(RaceVolunteerNeed { race, matchup, role_needs });
        }
    }

    needs.sort_by_key(|need| match need.race.schedule {
        RaceSchedule::Live { start, .. } => start,
        _ => Utc::now(),
    });

    Ok(needs)
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
        // Skip this check for open-entry races and qualifiers
        if !matches!(race.entrants, Entrants::Open) && !race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
            if !race.restream_consent_required {
                if let Some(mut teams) = race.teams_opt() {
                    if !teams.all(|team| team.restream_consent) {
                        continue;
                    }
                } else {
                    // Not all entrants are Mido's House teams, skip
                    continue;
                }
            }
        }

        // Get matchup description
        let matchup = get_matchup_description(&mut *transaction, &race).await?;

        // Get role bindings and check volunteer counts
        let role_bindings = EffectiveRoleBinding::for_event(&mut *transaction, series, event).await?;
        let signups = Signup::for_race(&mut *transaction, race_id).await?;

        let (role_needs, has_any_need) = collect_role_needs_for_bindings(&mut *transaction, &role_bindings, &signups).await?;

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
    // For qualifier races, use "Qualifier <round>" (e.g., "Qualifier 1")
    if race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
        return Ok(race.round.as_ref().map(|r| format!("Qualifier {}", r)).unwrap_or_else(|| "Qualifier".to_string()));
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
        Entrants::Open => "Open Signup Race".to_string(),
        _ => "Unknown matchup".to_string(),
    };

    let draft_mode = race.draft.as_ref().and_then(|draft| {
        let game = race.game.unwrap_or(1);
        let preset = draft.settings.get(&*format!("game{game}_preset"))?;
        racetime_bot::Goal::for_event(race.series, &race.event)
            .and_then(|g| g.draft_kind())
            .and_then(|kind: draft::Kind| kind.preset_display_name(preset.as_ref()))
    });

    // Add round and/or phase info if available, then draft mode
    let mut result = match (&race.round, &race.phase) {
        (Some(round), Some(phase)) => format!("{} ({}, {})", matchup, round, phase),
        (Some(round), None) => format!("{} ({})", matchup, round),
        (None, Some(phase)) => format!("{} ({})", matchup, phase),
        (None, None) => matchup,
    };

    // Append draft mode if present
    if let Some(mode) = draft_mode {
        result = format!("{} [{}]", result, mode);
    }

    Ok(result)
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

/// Builds the role needs list for a set of role bindings and signups.
/// Always includes all non-disabled roles; `is_full` marks roles at capacity.
/// Returns `(role_needs, has_any_need)` where `has_any_need` is true if any role is not full.
async fn collect_role_needs_for_bindings(
    transaction: &mut Transaction<'_, Postgres>,
    role_bindings: &[EffectiveRoleBinding],
    signups: &[Signup],
) -> Result<(Vec<RoleNeed>, bool), sqlx::Error> {
    let mut role_needs = Vec::new();
    let mut has_any_need = false;

    for binding in role_bindings {
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

        let is_full = confirmed_count >= binding.max_count;

        role_needs.push(RoleNeed {
            role_name: binding.role_type_name.clone(),
            discord_role_id: binding.discord_role_id,
            confirmed_names,
            confirmed_count,
            pending_count,
            min_count: binding.min_count,
            max_count: binding.max_count,
            language: binding.language,
            is_full,
        });

        if !is_full {
            has_any_need = true;
        }
    }

    Ok((role_needs, has_any_need))
}

/// Builds the content and components for a volunteer announcement message.
/// Returns (content_string, components_vec).
fn build_announcement_content(
    needs: &[RaceVolunteerNeed],
    event_data: &event::Data<'_>,
    time_window_start: DateTime<Utc>,
    time_window_end: DateTime<Utc>,
) -> (String, Vec<CreateActionRow>) {
    let mut msg = MessageBuilder::default();

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
            // Sort restreams so the event's default language appears first
            let mut sorted_urls: Vec<_> = need.race.video_urls.iter().collect();
            sorted_urls.sort_by_key(|(lang, _)| {
                if **lang == event_data.default_volunteer_language {
                    0 // Default language first
                } else {
                    1 // Others after (order doesn't matter)
                }
            });

            for (language, video_url) in sorted_urls {
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
                if role_need.is_full {
                    if !role_need.confirmed_names.is_empty() {
                        msg.push(&role_need.confirmed_names.join(", "));
                        msg.push(" ");
                    }
                    msg.push("(**Full**)");
                } else {
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

        // Button label: truncate matchup to fit Discord's 80 char limit.
        // If multiple races share the same matchup string, add the date to disambiguate.
        let has_duplicate_matchup = needs.iter()
            .filter(|n| n.matchup == need.matchup)
            .count() > 1;
        let label = if has_duplicate_matchup {
            if let RaceSchedule::Live { start, .. } = need.race.schedule {
                format!("Sign up: {} - {}", truncate_string(&need.matchup, 60), start.format("%b %d"))
            } else {
                format!("Sign up: {}", truncate_string(&need.matchup, 60))
            }
        } else {
            format!("Sign up: {}", truncate_string(&need.matchup, 60))
        };
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
    time_window_start: DateTime<Utc>,
    time_window_end: DateTime<Utc>,
) -> CreateMessage {
    let (content, components) = build_announcement_content(
        needs,
        event_data,
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
    let mut needs = build_volunteer_needs_for_race_ids(
        &mut transaction,
        &event_data,
        &race_ids,
        &http_client,
    ).await?;

    // Calculate current time
    let now = Utc::now();

    // Filter out races that have already started
    needs.retain(|need| {
        match need.race.schedule {
            RaceSchedule::Live { start, .. } => start > now,
            _ => false,
        }
    });

    // All races in this post have started - delete the now-empty Discord message
    if needs.is_empty() {
        transaction.commit().await?;
        let _ = channel_id.delete_message(discord_ctx, message_id).await;
        let mut cleanup_tx = pool.begin().await?;
        sqlx::query!(
            "UPDATE races SET volunteer_request_message_id = NULL, volunteer_request_sent = false WHERE volunteer_request_message_id = $1",
            PgSnowflake(message_id) as _
        )
        .execute(&mut *cleanup_tx)
        .await?;
        cleanup_tx.commit().await?;
        return Ok(());
    }

    // Calculate time window (use lead time from event)
    let lead_time = Duration::hours(race_info.volunteer_request_lead_time_hours as i64);
    let cutoff = now + lead_time;

    // Build the updated message content
    let (content, components) = build_announcement_content(
        &needs,
        &event_data,
        now,
        cutoff,
    );

    transaction.commit().await?;

    // Edit the message
    if let Err(e) = channel_id.edit_message(
        discord_ctx,
        message_id,
        EditMessage::new()
            .content(content)
            .components(components)
    ).await {
        if is_unknown_message(&e) || is_message_too_long(&e) {
            if is_message_too_long(&e) {
                let _ = channel_id.delete_message(discord_ctx, message_id).await;
            }
            let mut cleanup_tx = pool.begin().await?;
            sqlx::query!(
                "UPDATE races SET volunteer_request_sent = false, volunteer_request_message_id = NULL \
                 WHERE volunteer_request_message_id = $1",
                PgSnowflake(message_id) as _
            )
            .execute(&mut *cleanup_tx)
            .await?;
            cleanup_tx.commit().await?;
        }
    }

    Ok(())
}

/// Updates all volunteer posts for an event (call when role bindings change)
pub(crate) async fn update_volunteer_posts_for_event(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    series: Series,
    event: &str,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;

    // Find all unique message IDs for races in this event that haven't started yet, plus
    // races that started recently. The recent window ensures posts for just-finished races
    // get cleaned up even when all races in the post have started.
    let message_ids = sqlx::query_scalar!(
        r#"SELECT DISTINCT volunteer_request_message_id AS "message_id: PgSnowflake<MessageId>"
        FROM races
        WHERE series = $1
          AND event = $2
          AND volunteer_request_message_id IS NOT NULL
          AND ignored = false
          AND start > NOW() - interval '8 hours'"#,
        series as _,
        event
    )
    .fetch_all(&mut *transaction)
    .await?;

    transaction.commit().await?;

    // Update each unique message (which may contain multiple races)
    for message_id in message_ids {
        // Find any race with this message ID to trigger the update
        let race_id = sqlx::query_scalar!(
            r#"SELECT id AS "id: Id<Races>"
            FROM races
            WHERE volunteer_request_message_id = $1
            LIMIT 1"#,
            message_id as _
        )
        .fetch_optional(pool)
        .await?;

        if let Some(race_id) = race_id {
            // Update the message (this will update all races sharing the same message)
            let _ = update_volunteer_post_for_race(pool, discord_ctx, race_id).await;
        }
    }

    Ok(())
}

/// Updates or deletes a volunteer post identified by its Discord message ID.
///
/// Used when races are deleted outright (e.g. pausing/deleting a weekly schedule),
/// so the race rows are gone and we can't look up the message ID from a race.
pub(crate) async fn update_volunteer_post_by_message_id(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    series: Series,
    event: &str,
    message_id: MessageId,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;
    let http_client = reqwest::Client::new();

    // Look up the volunteer info channel and lead time from the event
    let event_config = sqlx::query!(
        r#"SELECT
            discord_volunteer_info_channel AS "channel: PgSnowflake<ChannelId>",
            volunteer_request_lead_time_hours
        FROM events WHERE series = $1 AND event = $2"#,
        series as _,
        event,
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let event_config = match event_config {
        Some(c) => c,
        None => return Ok(()),
    };
    let channel_id = match event_config.channel {
        Some(PgSnowflake(id)) => id,
        None => return Ok(()),
    };

    // Find remaining non-ignored races that still reference this message
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
        transaction.commit().await?;
        let _ = channel_id.delete_message(discord_ctx, message_id).await;
        let mut cleanup_tx = pool.begin().await?;
        sqlx::query!(
            "UPDATE races SET volunteer_request_message_id = NULL WHERE volunteer_request_message_id = $1",
            PgSnowflake(message_id) as _
        )
        .execute(&mut *cleanup_tx)
        .await?;
        cleanup_tx.commit().await?;
        return Ok(());
    }

    let event_data = match event::Data::new(&mut transaction, series, event.to_owned()).await? {
        Some(data) => data,
        None => return Ok(()),
    };

    // Build volunteer needs for remaining races
    let mut needs = build_volunteer_needs_for_race_ids(
        &mut transaction,
        &event_data,
        &race_ids,
        &http_client,
    ).await?;

    let now = Utc::now();
    needs.retain(|need| matches!(need.race.schedule, RaceSchedule::Live { start, .. } if start > now));

    if needs.is_empty() {
        transaction.commit().await?;
        let _ = channel_id.delete_message(discord_ctx, message_id).await;
        let mut cleanup_tx = pool.begin().await?;
        sqlx::query!(
            "UPDATE races SET volunteer_request_message_id = NULL WHERE volunteer_request_message_id = $1",
            PgSnowflake(message_id) as _
        )
        .execute(&mut *cleanup_tx)
        .await?;
        cleanup_tx.commit().await?;
        return Ok(());
    }

    let lead_time = Duration::hours(event_config.volunteer_request_lead_time_hours as i64);
    let (content, components) = build_announcement_content(&needs, &event_data, now, now + lead_time);

    transaction.commit().await?;

    if let Err(e) = channel_id.edit_message(
        discord_ctx,
        message_id,
        EditMessage::new().content(content).components(components),
    ).await {
        if is_unknown_message(&e) || is_message_too_long(&e) {
            if is_message_too_long(&e) {
                let _ = channel_id.delete_message(discord_ctx, message_id).await;
            }
            let mut cleanup_tx = pool.begin().await?;
            sqlx::query!(
                "UPDATE races SET volunteer_request_sent = false, volunteer_request_message_id = NULL \
                 WHERE volunteer_request_message_id = $1",
                PgSnowflake(message_id) as _
            )
            .execute(&mut *cleanup_tx)
            .await?;
            cleanup_tx.commit().await?;
        }
    }

    Ok(())
}
