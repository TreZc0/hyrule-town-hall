use {
    chrono::{DateTime, Duration, NaiveTime, Utc},
    serenity::all::CreateMessage,
    serenity::model::id::{ChannelId, MessageId, RoleId},
    sqlx::PgPool,
    std::collections::HashSet,
    crate::{
        cal::{Entrant, Entrants, Race, RaceSchedule},
        event::{roles::{EffectiveRoleBinding, Signup, VolunteerSignupStatus}},
        game::Game,
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

#[derive(Debug, Clone, Copy, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "ping_interval", rename_all = "lowercase")]
pub(crate) enum PingInterval {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Copy, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "ping_workflow_type", rename_all = "snake_case")]
pub(crate) enum PingWorkflowTypeDb {
    Scheduled,
    PerRace,
}

pub(crate) enum PingWorkflowType {
    Scheduled {
        interval: PingInterval,
        schedule_time: NaiveTime,
        schedule_day_of_week: Option<i16>,
    },
    PerRace {
        lead_times: Vec<i32>,
    },
}

#[allow(dead_code)]
pub(crate) struct PingWorkflow {
    pub(crate) id: i32,
    pub(crate) language: Language,
    pub(crate) discord_ping_channel: Option<i64>,
    pub(crate) delete_after_race: bool,
    pub(crate) workflow_type: PingWorkflowType,
}


/// Resolves which workflows apply to the given event. If any event-level workflows exist, those
/// are returned exclusively. Otherwise, game-level workflows for the associated game are returned.
async fn resolve_workflows_for_event(
    pool: &PgPool,
    series: Series,
    event: &str,
) -> Result<Vec<PingWorkflow>, Error> {
    let mut transaction = pool.begin().await?;

    // Check for event-level workflows
    let event_rows = sqlx::query!(
        r#"SELECT
            w.id,
            w.language AS "language: Language",
            w.discord_ping_channel,
            w.delete_after_race,
            w.workflow_type AS "workflow_type: PingWorkflowTypeDb",
            w.ping_interval AS "ping_interval: PingInterval",
            w.schedule_time,
            w.schedule_day_of_week
        FROM volunteer_ping_workflows w
        WHERE w.series = $1 AND w.event = $2
        ORDER BY w.id"#,
        series.slug(),
        event,
    )
    .fetch_all(&mut *transaction)
    .await?;

    let workflows = if !event_rows.is_empty() {
        let mut out = Vec::new();
        for row in event_rows {
            let wf_type = match row.workflow_type {
                PingWorkflowTypeDb::Scheduled => {
                    let interval = row.ping_interval.unwrap_or(PingInterval::Daily);
                    let schedule_time = row.schedule_time.unwrap_or_else(|| NaiveTime::from_hms_opt(18, 0, 0).unwrap());
                    PingWorkflowType::Scheduled {
                        interval,
                        schedule_time,
                        schedule_day_of_week: row.schedule_day_of_week,
                    }
                }
                PingWorkflowTypeDb::PerRace => {
                    let lead_times = sqlx::query_scalar!(
                        "SELECT lead_time_hours FROM volunteer_ping_lead_times WHERE workflow_id = $1 ORDER BY lead_time_hours",
                        row.id
                    )
                    .fetch_all(&mut *transaction)
                    .await?;
                    PingWorkflowType::PerRace { lead_times }
                }
            };
            out.push(PingWorkflow {
                id: row.id,
                language: row.language,
                discord_ping_channel: row.discord_ping_channel,
                delete_after_race: row.delete_after_race,
                workflow_type: wf_type,
            });
        }
        out
    } else {
        // Fall back to game-level workflows
        let game = Game::from_series(&mut transaction, series).await.map_err(|_| sqlx::Error::RowNotFound)?;
        let Some(game) = game else { return Ok(Vec::new()) };

        let game_rows = sqlx::query!(
            r#"SELECT
                w.id,
                w.language AS "language: Language",
                w.discord_ping_channel,
                w.delete_after_race,
                w.workflow_type AS "workflow_type: PingWorkflowTypeDb",
                w.ping_interval AS "ping_interval: PingInterval",
                w.schedule_time,
                w.schedule_day_of_week
            FROM volunteer_ping_workflows w
            WHERE w.game_id = $1
            ORDER BY w.id"#,
            game.id,
        )
        .fetch_all(&mut *transaction)
        .await?;

        let mut out = Vec::new();
        for row in game_rows {
            let wf_type = match row.workflow_type {
                PingWorkflowTypeDb::Scheduled => {
                    let interval = row.ping_interval.unwrap_or(PingInterval::Daily);
                    let schedule_time = row.schedule_time.unwrap_or_else(|| NaiveTime::from_hms_opt(18, 0, 0).unwrap());
                    PingWorkflowType::Scheduled {
                        interval,
                        schedule_time,
                        schedule_day_of_week: row.schedule_day_of_week,
                    }
                }
                PingWorkflowTypeDb::PerRace => {
                    let lead_times = sqlx::query_scalar!(
                        "SELECT lead_time_hours FROM volunteer_ping_lead_times WHERE workflow_id = $1 ORDER BY lead_time_hours",
                        row.id
                    )
                    .fetch_all(&mut *transaction)
                    .await?;
                    PingWorkflowType::PerRace { lead_times }
                }
            };
            out.push(PingWorkflow {
                id: row.id,
                language: row.language,
                discord_ping_channel: row.discord_ping_channel,
                delete_after_race: row.delete_after_race,
                workflow_type: wf_type,
            });
        }
        out
    };

    transaction.commit().await?;
    Ok(workflows)
}

/// Returns the effective ping channel: workflow-specific if set, else the event's volunteer info channel.
fn resolve_ping_channel(workflow: &PingWorkflow, event_info_channel: Option<i64>) -> Option<ChannelId> {
    workflow.discord_ping_channel
        .or(event_info_channel)
        .map(|id| ChannelId::new(id as u64))
}

/// Checks whether a scheduled workflow should fire right now.
fn scheduled_workflow_should_fire(
    interval: PingInterval,
    schedule_time: NaiveTime,
    schedule_day_of_week: Option<i16>,
    last_sent_at: Option<DateTime<Utc>>,
) -> bool {
    let now = Utc::now();
    let now_time = now.time();

    // Window: fire if we're within 10 minutes after the scheduled time
    let window_minutes = 10i64;
    let target = schedule_time;
    let target_seconds = target.num_seconds_from_midnight() as i64;
    let now_seconds = now_time.num_seconds_from_midnight() as i64;
    let diff_seconds = now_seconds - target_seconds;

    // Must be within [0, window_minutes * 60) seconds past schedule_time
    if diff_seconds < 0 || diff_seconds >= window_minutes * 60 {
        return false;
    }

    // For weekly: also check day of week
    if interval == PingInterval::Weekly {
        if let Some(day) = schedule_day_of_week {
            // 0=Mon..6=Sun, chrono weekday: Mon=0..Sun=6
            let current_day = now.weekday().num_days_from_monday() as i16;
            if current_day != day {
                return false;
            }
        }
    }

    // Dedup: if we already fired within the last [interval] period, skip
    if let Some(last) = last_sent_at {
        let min_gap = match interval {
            PingInterval::Daily => Duration::hours(23),
            PingInterval::Weekly => Duration::days(6),
        };
        if now - last < min_gap {
            return false;
        }
    }

    true
}

/// Builds a ping message for a set of role IDs needing pings and a list of races.
fn build_scheduled_ping_message(
    role_ids: &HashSet<i64>,
    race_summaries: &[(String, String, Option<i64>, Option<i64>)], // (matchup, discord_msg_link, volunteer_request_message_id, channel_id)
    series: Series,
    event: &str,
    volunteer_page_url: &str,
) -> CreateMessage {
    let mut msg = MessageBuilder::default();

    // Role pings at top
    for role_id in role_ids {
        msg.role(RoleId::new(*role_id as u64));
        msg.push(" ");
    }
    if !role_ids.is_empty() {
        msg.push("\n\n");
    }

    msg.push("**Volunteer ping — ");
    msg.push(series.slug());
    msg.push("/");
    msg.push(event);
    msg.push("**\n");

    msg.push("Races needing volunteers:\n");
    for (matchup, start_ts, msg_id, chan_id) in race_summaries {
        msg.push("• ");
        msg.push(matchup);
        msg.push(" — ");
        msg.push(start_ts);
        if let (Some(msg_id), Some(chan_id)) = (msg_id, chan_id) {
            // Guild ID is not easily available here; omit it and just post channel+message link
            msg.push(&format!(" [signup post](https://discord.com/channels/0/{}/{})", chan_id, msg_id));
        }
        msg.push("\n");
    }

    msg.push("\nSign up: ");
    msg.push(volunteer_page_url);

    CreateMessage::new().content(msg.build())
}

fn build_per_race_ping_message(
    role_ids: &HashSet<i64>,
    matchup: &str,
    start_ts: &str,
    volunteer_request_message_id: Option<i64>,
    volunteer_request_channel_id: Option<i64>,
    volunteer_page_url: &str,
) -> CreateMessage {
    let mut msg = MessageBuilder::default();

    for role_id in role_ids {
        msg.role(RoleId::new(*role_id as u64));
        msg.push(" ");
    }
    if !role_ids.is_empty() {
        msg.push("\n\n");
    }

    msg.push("**Volunteer needed — ");
    msg.push(matchup);
    msg.push("**\n");
    msg.push("Race starts: ");
    msg.push(start_ts);
    msg.push("\n");

    if let (Some(msg_id), Some(chan_id)) = (volunteer_request_message_id, volunteer_request_channel_id) {
        msg.push(&format!("Signup post: https://discord.com/channels/0/{}/{}\n", chan_id, msg_id));
    }

    msg.push("Sign up: ");
    msg.push(volunteer_page_url);

    CreateMessage::new().content(msg.build())
}

/// Top-level function called every 10 minutes to send scheduled and per-race pings.
pub(crate) async fn check_and_send_volunteer_pings(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
) -> Result<(), Error> {
    // Get all events with volunteer requests enabled
    let enabled_events = sqlx::query!(
        r#"SELECT
            series,
            event,
            discord_volunteer_info_channel,
            volunteer_request_lead_time_hours
        FROM events
        WHERE volunteer_requests_enabled = true"#,
    )
    .fetch_all(pool)
    .await?;

    for event_row in enabled_events {
        let series: Series = match event_row.series.parse() {
            Ok(s) => s,
            Err(()) => continue,
        };
        let event = &event_row.event;
        let info_channel = event_row.discord_volunteer_info_channel;
        let lead_time_hours = event_row.volunteer_request_lead_time_hours;

        let workflows = match resolve_workflows_for_event(pool, series, event).await {
            Ok(w) => w,
            Err(e) => { eprintln!("Error resolving ping workflows for {}/{}: {}", series.slug(), event, e); continue; }
        };

        for workflow in &workflows {
            let volunteer_page_url = format!("{}/event/{}/{}/volunteer-roles", base_uri(), series.slug(), event);

            match &workflow.workflow_type {
                PingWorkflowType::Scheduled { interval, schedule_time, schedule_day_of_week } => {
                    if let Err(e) = check_scheduled_workflow(
                        pool,
                        discord_ctx,
                        workflow,
                        series,
                        event,
                        info_channel,
                        lead_time_hours,
                        *interval,
                        *schedule_time,
                        *schedule_day_of_week,
                        &volunteer_page_url,
                    ).await {
                        eprintln!("Error in scheduled ping workflow {} for {}/{}: {}", workflow.id, series.slug(), event, e);
                    }
                }
                PingWorkflowType::PerRace { lead_times } => {
                    for &lead_time_h in lead_times {
                        if let Err(e) = check_per_race_workflow(
                            pool,
                            discord_ctx,
                            workflow,
                            series,
                            event,
                            info_channel,
                            lead_time_h,
                            &volunteer_page_url,
                        ).await {
                            eprintln!("Error in per-race ping workflow {} lt={} for {}/{}: {}", workflow.id, lead_time_h, series.slug(), event, e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn check_scheduled_workflow(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    workflow: &PingWorkflow,
    series: Series,
    event: &str,
    info_channel: Option<i64>,
    lead_time_hours: i32,
    interval: PingInterval,
    schedule_time: NaiveTime,
    schedule_day_of_week: Option<i16>,
    volunteer_page_url: &str,
) -> Result<(), Error> {
    // Check when we last sent a scheduled ping for this workflow
    let last_sent = sqlx::query_scalar!(
        r#"SELECT sent_at FROM volunteer_ping_messages
        WHERE workflow_id = $1 AND race_id IS NULL
        ORDER BY sent_at DESC LIMIT 1"#,
        workflow.id
    )
    .fetch_optional(pool)
    .await?;

    if !scheduled_workflow_should_fire(interval, schedule_time, schedule_day_of_week, last_sent) {
        return Ok(());
    }

    let channel_id = match resolve_ping_channel(workflow, info_channel) {
        Some(id) => id,
        None => return Ok(()),
    };

    // Find races within the lead_time window that need volunteers for this workflow's language
    let now = Utc::now();
    let cutoff = now + Duration::hours(lead_time_hours as i64);

    let race_ids: Vec<i64> = sqlx::query_scalar!(
        r#"SELECT id FROM races
        WHERE series = $1
          AND event = $2
          AND ignored = false
          AND start IS NOT NULL
          AND start > $3
          AND start <= $4
          AND async_start1 IS NULL
        ORDER BY start ASC"#,
        series.slug(),
        event,
        now,
        cutoff,
    )
    .fetch_all(pool)
    .await?;

    if race_ids.is_empty() {
        return Ok(());
    }

    // Build role_ids and race summaries
    let http_client = reqwest::Client::new();
    let mut role_ids_to_ping: HashSet<i64> = HashSet::new();
    let mut race_summaries = Vec::new();

    let mut transaction = pool.begin().await?;

    for race_id_raw in &race_ids {
        let race_id: Id<Races> = Id::from(*race_id_raw);
        let race = match Race::from_id(&mut transaction, &http_client, race_id).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Determine role needs for this workflow's language
        let role_bindings = EffectiveRoleBinding::for_event(&mut transaction, series, event).await?;
        let signups = Signup::for_race(&mut transaction, race_id).await?;

        let mut needs_ping = false;
        for binding in &role_bindings {
            if binding.is_disabled || binding.language != workflow.language {
                continue;
            }
            let confirmed = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Confirmed))
                .count() as i32;
            let pending = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Pending))
                .count() as i32;
            if confirmed + pending < binding.min_count {
                needs_ping = true;
                if let Some(role_id) = binding.discord_role_id {
                    role_ids_to_ping.insert(role_id);
                }
            }
        }

        if !needs_ping {
            continue;
        }

        // Build race summary
        let start_ts = match race.schedule {
            RaceSchedule::Live { start, .. } => format!("<t:{}:F>", start.timestamp()),
            _ => continue,
        };

        let matchup = build_matchup_label(&race);

        // Get volunteer_request_message_id and channel_id for link
        let msg_info = sqlx::query!(
            r#"SELECT
                r.volunteer_request_message_id,
                e.discord_volunteer_info_channel
            FROM races r
            JOIN events e ON r.series = e.series AND r.event = e.event
            WHERE r.id = $1"#,
            *race_id_raw
        )
        .fetch_optional(&mut *transaction)
        .await?;

        let (vmsg_id, vchan_id) = msg_info
            .map(|m| (m.volunteer_request_message_id, m.discord_volunteer_info_channel))
            .unwrap_or((None, None));

        race_summaries.push((matchup, start_ts, vmsg_id, vchan_id));
    }

    transaction.commit().await?;

    if role_ids_to_ping.is_empty() && race_summaries.is_empty() {
        return Ok(());
    }

    let message = build_scheduled_ping_message(
        &role_ids_to_ping,
        &race_summaries,
        series,
        event,
        volunteer_page_url,
    );

    let posted = match channel_id.send_message(discord_ctx, message).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to send scheduled ping for workflow {}: {}", workflow.id, e);
            return Ok(());
        }
    };

    sqlx::query!(
        "INSERT INTO volunteer_ping_messages (workflow_id, race_id, lead_time_hours, message_id, channel_id) VALUES ($1, NULL, NULL, $2, $3)",
        workflow.id,
        posted.id.get() as i64,
        channel_id.get() as i64,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn check_per_race_workflow(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
    workflow: &PingWorkflow,
    series: Series,
    event: &str,
    info_channel: Option<i64>,
    lead_time_hours: i32,
    volunteer_page_url: &str,
) -> Result<(), Error> {
    let channel_id = match resolve_ping_channel(workflow, info_channel) {
        Some(id) => id,
        None => return Ok(()),
    };

    let now = Utc::now();
    let cutoff = now + Duration::hours(lead_time_hours as i64);

    // Find races starting within this lead time window
    let race_ids: Vec<i64> = sqlx::query_scalar!(
        r#"SELECT id FROM races
        WHERE series = $1
          AND event = $2
          AND ignored = false
          AND start IS NOT NULL
          AND start > $3
          AND start <= $4
          AND async_start1 IS NULL
        ORDER BY start ASC"#,
        series.slug(),
        event,
        now,
        cutoff,
    )
    .fetch_all(pool)
    .await?;

    let http_client = reqwest::Client::new();

    for race_id_raw in race_ids {
        // Check dedup: already sent a ping for this workflow + race + lead_time?
        let already_sent: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM volunteer_ping_messages
                WHERE workflow_id = $1 AND race_id = $2 AND lead_time_hours = $3
            ) AS "exists!""#,
            workflow.id,
            race_id_raw,
            lead_time_hours,
        )
        .fetch_one(pool)
        .await?;

        if already_sent {
            continue;
        }

        let race_id: Id<Races> = Id::from(race_id_raw);
        let mut transaction = pool.begin().await?;

        let race = match Race::from_id(&mut transaction, &http_client, race_id).await {
            Ok(r) => r,
            Err(_) => { let _ = transaction.rollback().await; continue; }
        };

        // Check if any role for this language needs a ping
        let role_bindings = EffectiveRoleBinding::for_event(&mut transaction, series, event).await?;
        let signups = Signup::for_race(&mut transaction, race_id).await?;

        let mut role_ids_to_ping: HashSet<i64> = HashSet::new();
        for binding in &role_bindings {
            if binding.is_disabled || binding.language != workflow.language {
                continue;
            }
            let confirmed = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Confirmed))
                .count() as i32;
            let pending = signups.iter()
                .filter(|s| s.role_binding_id == binding.id && matches!(s.status, VolunteerSignupStatus::Pending))
                .count() as i32;
            if confirmed + pending < binding.min_count {
                if let Some(role_id) = binding.discord_role_id {
                    role_ids_to_ping.insert(role_id);
                }
            }
        }

        if role_ids_to_ping.is_empty() {
            let _ = transaction.rollback().await;
            continue;
        }

        let start_ts = match race.schedule {
            RaceSchedule::Live { start, .. } => format!("<t:{}:F>", start.timestamp()),
            _ => { let _ = transaction.rollback().await; continue; }
        };

        let matchup = build_matchup_label(&race);

        let msg_info = sqlx::query!(
            r#"SELECT
                r.volunteer_request_message_id,
                e.discord_volunteer_info_channel
            FROM races r
            JOIN events e ON r.series = e.series AND r.event = e.event
            WHERE r.id = $1"#,
            race_id_raw
        )
        .fetch_optional(&mut *transaction)
        .await?;

        let (vmsg_id, vchan_id) = msg_info
            .map(|m| (m.volunteer_request_message_id, m.discord_volunteer_info_channel))
            .unwrap_or((None, None));

        transaction.commit().await?;

        let message = build_per_race_ping_message(
            &role_ids_to_ping,
            &matchup,
            &start_ts,
            vmsg_id,
            vchan_id,
            volunteer_page_url,
        );

        let posted = match channel_id.send_message(discord_ctx, message).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to send per-race ping for workflow {} race {}: {}", workflow.id, race_id_raw, e);
                continue;
            }
        };

        if let Err(e) = sqlx::query!(
            "INSERT INTO volunteer_ping_messages (workflow_id, race_id, lead_time_hours, message_id, channel_id) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT DO NOTHING",
            workflow.id,
            race_id_raw,
            lead_time_hours,
            posted.id.get() as i64,
            channel_id.get() as i64,
        )
        .execute(pool)
        .await {
            eprintln!("Failed to record per-race ping message: {}", e);
        }
    }

    Ok(())
}

/// Deletes Discord ping messages for races that have already started, when `delete_after_race` is set.
pub(crate) async fn delete_stale_ping_messages(
    pool: &PgPool,
    discord_ctx: &DiscordCtx,
) -> Result<(), Error> {
    let now = Utc::now();

    let stale = sqlx::query!(
        r#"SELECT pm.id, pm.message_id, pm.channel_id
        FROM volunteer_ping_messages pm
        JOIN volunteer_ping_workflows w ON pm.workflow_id = w.id
        LEFT JOIN races r ON pm.race_id = r.id
        WHERE w.delete_after_race = true
          AND pm.deleted_at IS NULL
          AND (
              -- per-race pings: delete when the specific race has started
              (pm.race_id IS NOT NULL AND r.start IS NOT NULL AND r.start <= $1)
              OR
              -- scheduled pings: delete if older than 24h (cleanup)
              (pm.race_id IS NULL AND pm.sent_at <= $1 - interval '24 hours')
          )"#,
        now,
    )
    .fetch_all(pool)
    .await?;

    for row in stale {
        let channel_id = ChannelId::new(row.channel_id as u64);
        let message_id = MessageId::new(row.message_id as u64);

        // Attempt deletion — ignore error if already gone
        let _ = channel_id.delete_message(discord_ctx, message_id).await;

        sqlx::query!(
            "UPDATE volunteer_ping_messages SET deleted_at = $1 WHERE id = $2",
            now,
            row.id
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

fn build_matchup_label(race: &Race) -> String {
    let base = match &race.entrants {
        Entrants::Two([e1, e2]) => format!("{} vs {}", entrant_short_name(e1), entrant_short_name(e2)),
        Entrants::Three([e1, e2, e3]) => format!("{} vs {} vs {}", entrant_short_name(e1), entrant_short_name(e2), entrant_short_name(e3)),
        Entrants::Open => "Open Race".to_string(),
        _ => "Race".to_string(),
    };
    match (&race.round, &race.phase) {
        (Some(r), Some(p)) => format!("{} ({}, {})", base, r, p),
        (Some(r), None) => format!("{} ({})", base, r),
        (None, Some(p)) => format!("{} ({})", base, p),
        (None, None) => base,
    }
}

fn entrant_short_name(entrant: &Entrant) -> &str {
    match entrant {
        Entrant::Named { name, .. } => name,
        _ => "?",
    }
}
