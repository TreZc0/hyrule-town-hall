use {
    chrono::{DateTime, Utc},
    sqlx::{PgPool, Postgres, Transaction},
    std::time::Duration,
};

use crate::prelude::*;

pub(crate) mod generation;

pub(crate) fn physical_seed_identity(data: &serde_json::Value) -> Option<String> {
    match seed::Files::from_seed_data(data)? {
        seed::Files::AlttprDoorRando { uuid, is_owr } => Some(format!(
            "{}:{uuid}",
            if is_owr { "alttpr_owr" } else { "alttpr_dr" }
        )),
        seed::Files::AvianartSeed { hash, .. } => Some(format!("alttpr_avianart:{hash}")),
        seed::Files::OotrWeb { id, .. } => Some(format!("ootr_web:{id}")),
        seed::Files::TriforceBlitz { uuid, is_dev } => Some(format!(
            "ootr_tfb:{}:{uuid}",
            if is_dev { "dev" } else { "prod" }
        )),
        seed::Files::MidosHouse { file_stem, .. } => Some(format!("midos_house:{file_stem}")),
        seed::Files::TwwrPermalink { permalink, .. } => Some(format!("twwr:{permalink}")),
        seed::Files::TfbSotd { date, ordinal } => Some(format!("tfb_sotd:{date}:{ordinal}")),
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("pooled qualifiers are not configured for this event")]
    NotConfigured,
    #[error("pooled qualifier requests are paused")]
    Paused,
    #[error("the pooled qualifier request window is not open")]
    WindowClosed,
    #[error("this qualifier mode is unavailable")]
    ModeUnavailable,
    #[error("this entrant already has a counted attempt for the mode")]
    AlreadyAttempted,
    #[error("this entrant already has an active async qualifier")]
    ActiveAsync,
    #[error("no ready qualifier seed is available")]
    NoSeed,
    #[error("automated async delivery is not configured for this event")]
    DeliveryUnavailable,
    #[error("the event-wide re-attempt has already been used or reserved")]
    RetryUnavailable,
    #[error("this result is not eligible for a re-attempt")]
    RetryForbidden,
    #[error("no different physical seed is available for the re-attempt")]
    NoReplacementSeed,
    #[error("link a racetime.gg account before reserving a live qualifier retry")]
    RacetimeRequired,
    #[error("entry for this live qualifier has already closed")]
    LiveEntryClosed,
    #[error("invalid pooled qualifier transition from {0}")]
    InvalidTransition(String),
    #[error("this race is not a configured pooled live qualifier")]
    NotLiveQualifier,
}

/// Every pooled mutation takes this event lock before locking child rows.
/// Transactions must release it before performing external I/O.
pub(crate) async fn lock_event(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT 1 FROM pooled_qualifier_configs WHERE series = $1 AND event = $2 FOR UPDATE",
    )
    .bind(series)
    .bind(event)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(())
}

pub(crate) async fn lock_attempt(
    transaction: &mut Transaction<'_, Postgres>,
    attempt_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1 FROM pooled_qualifier_configs config WHERE (series, event) = (SELECT series, event FROM qualifier_attempts WHERE id = $1) FOR UPDATE OF config")
        .bind(attempt_id).fetch_optional(&mut **transaction).await?;
    Ok(())
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Config {
    pub(crate) series: String,
    pub(crate) event: String,
    pub(crate) required_mode_count: i16,
    pub(crate) pool_seed_count: i16,
    pub(crate) live_races_per_mode: i16,
    pub(crate) requests_open_at: Option<DateTime<Utc>>,
    pub(crate) requests_close_at: Option<DateTime<Utc>>,
    pub(crate) starts_close_at: Option<DateTime<Utc>>,
    pub(crate) submissions_close_at: Option<DateTime<Utc>>,
    pub(crate) retries_close_at: Option<DateTime<Utc>>,
    pub(crate) results_release_at: Option<DateTime<Utc>>,
    pub(crate) async_run_limit: sqlx::postgres::types::PgInterval,
    pub(crate) live_entry_close_lead: sqlx::postgres::types::PgInterval,
    pub(crate) retry_limit: i16,
    pub(crate) allocation_spread: i16,
    pub(crate) par_finishers: i16,
    pub(crate) score_scale: f64,
    pub(crate) score_offset: f64,
    pub(crate) score_minimum: f64,
    pub(crate) score_maximum: f64,
    pub(crate) requests_paused: bool,
    pub(crate) settings_locked_at: Option<DateTime<Utc>>,
}

impl Config {
    pub(crate) async fn load(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT series, event, required_mode_count, pool_seed_count,
                live_races_per_mode, requests_open_at, requests_close_at,
                starts_close_at, submissions_close_at, retries_close_at,
                results_release_at, async_run_limit, live_entry_close_lead,
                retry_limit, allocation_spread, par_finishers, score_scale,
                score_offset, score_minimum, score_maximum, requests_paused,
                settings_locked_at
            FROM pooled_qualifier_configs WHERE series = $1 AND event = $2"#,
        )
        .bind(series)
        .bind(event)
        .fetch_optional(&mut **transaction)
        .await
    }

    pub(crate) fn requests_open(&self, now: DateTime<Utc>, retry: bool) -> bool {
        self.requests_open_at.is_some_and(|start| start <= now)
            && self.requests_close_at.is_some_and(|end| now < end)
            && self.starts_close_at.is_some_and(|end| now < end)
            && self.submissions_close_at.is_some_and(|end| now < end)
            && self.results_release_at.is_some()
            && self.retries_close_at.is_some_and(|end| !retry || now < end)
    }

    pub(crate) fn run_limit(&self) -> chrono::Duration {
        pg_interval_duration(&self.async_run_limit)
    }
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Mode {
    pub(crate) id: i64,
    pub(crate) position: i16,
    pub(crate) slug: String,
    pub(crate) display_name: String,
    pub(crate) seed_gen_type: String,
    pub(crate) seed_config: serde_json::Value,
    pub(crate) generator_profile: String,
    pub(crate) settings_fingerprint: String,
    pub(crate) enabled: bool,
}

impl Mode {
    pub(crate) async fn for_event(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id, position, slug, display_name,
                seed_gen_type, seed_config, generator_profile, settings_fingerprint,
                enabled
            FROM qualifier_modes WHERE series = $1 AND event = $2
            ORDER BY position, id"#,
        )
        .bind(series)
        .bind(event)
        .fetch_all(&mut **transaction)
        .await
    }
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Attempt {
    pub(crate) id: i64,
    pub(crate) mode_id: i64,
    pub(crate) seed_id: i64,
    pub(crate) source: String,
    pub(crate) state: String,
    pub(crate) counts_for_entrant: bool,
    pub(crate) official_outcome: Option<String>,
    pub(crate) discord_thread: Option<i64>,
    pub(crate) retry_banned_at: Option<DateTime<Utc>>,
}

impl Attempt {
    const COLUMNS: &'static str = r#"id, mode_id, seed_id, source, state,
        counts_for_entrant, official_outcome, discord_thread, retry_banned_at"#;

    pub(crate) async fn for_team(
        transaction: &mut Transaction<'_, Postgres>,
        team_id: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(&format!(
            "SELECT {} FROM qualifier_attempts WHERE team_id = $1 AND state <> 'void' ORDER BY requested_at, id",
            Self::COLUMNS,
        ))
        .bind(team_id)
        .fetch_all(&mut **transaction)
        .await
    }

    pub(crate) fn is_active_async(&self) -> bool {
        self.source == "async"
            && matches!(
                self.state.as_str(),
                "assigned" | "revealed" | "starting" | "running"
            )
    }
}

pub(crate) struct RevealedSeed {
    pub(crate) data: serde_json::Value,
    pub(crate) next_control_version: i64,
    pub(crate) start_delay_minutes: i32,
}

/// Persist the reveal before Discord delivery. Repeated or stale controls cannot
/// expose another seed because an attempt always retains the same seed row.
pub(crate) async fn reveal(
    pool: &PgPool,
    attempt_id: i64,
    control_version: i64,
    discord_thread: i64,
) -> Result<RevealedSeed, Error> {
    type Row = (String, i64, Option<serde_json::Value>, String, Option<i32>);
    let mut transaction = pool.begin().await?;
    lock_attempt(&mut transaction, attempt_id).await?;
    let row: Row = sqlx::query_as(
        r#"SELECT attempt.state, attempt.control_version, seed.seed_data,
            seed.generation_state, event.async_start_delay
        FROM qualifier_attempts attempt
        JOIN qualifier_seeds seed ON seed.id = attempt.seed_id
        JOIN events event ON event.series = attempt.series AND event.event = attempt.event
        WHERE attempt.id = $1 AND attempt.discord_thread = $2 FOR UPDATE OF attempt"#,
    )
    .bind(attempt_id)
    .bind(discord_thread)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| Error::InvalidTransition("wrong qualifier thread".into()))?;
    if row.0 == "revealed" && row.1 == control_version + 1 {
        return Ok(RevealedSeed {
            data: row.2.ok_or(Error::NoSeed)?,
            next_control_version: row.1,
            start_delay_minutes: row.4.unwrap_or_default(),
        });
    }
    if row.0 != "assigned" || row.1 != control_version {
        return Err(Error::InvalidTransition("stale READY control".into()));
    }
    if row.3 != "ready" {
        return Err(Error::NoSeed);
    }
    let data = row.2.ok_or(Error::NoSeed)?;
    let next_control_version: i64 = sqlx::query_scalar(
        r#"UPDATE qualifier_attempts attempt SET state = 'revealed', revealed_at = NOW(),
            start_due_at = NOW() + make_interval(mins => $2::INT),
            control_version = attempt.control_version + 1
        FROM pooled_qualifier_configs config
        WHERE attempt.id = $1 AND config.series = attempt.series AND config.event = attempt.event
          AND NOW() < config.starts_close_at AND NOW() < config.submissions_close_at
        RETURNING attempt.control_version"#,
    )
    .bind(attempt_id)
    .bind(row.4.unwrap_or_default().max(0))
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(Error::WindowClosed)?;
    sqlx::query(
        r#"UPDATE qualifier_seeds SET released_at = COALESCE(released_at, NOW())
        WHERE id = (SELECT seed_id FROM qualifier_attempts WHERE id = $1)"#,
    )
    .bind(attempt_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"UPDATE pooled_qualifier_configs SET settings_locked_at = COALESCE(settings_locked_at, NOW())
        WHERE (series, event) = (SELECT series, event FROM qualifier_attempts WHERE id = $1)"#,
    )
    .bind(attempt_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(RevealedSeed {
        data,
        next_control_version,
        start_delay_minutes: row.4.unwrap_or_default(),
    })
}

pub(crate) async fn request_start(
    pool: &PgPool,
    attempt_id: i64,
    version: i64,
) -> Result<bool, Error> {
    let mut tx = pool.begin().await?;
    lock_attempt(&mut tx, attempt_id).await?;
    let changed = sqlx::query(r#"UPDATE qualifier_attempts attempt SET state = 'starting', start_due_at = NOW() + INTERVAL '6 seconds'
        FROM pooled_qualifier_configs config
        WHERE attempt.id = $1 AND attempt.control_version = $2 AND attempt.state = 'revealed'
          AND config.series = attempt.series AND config.event = attempt.event
          AND NOW() + INTERVAL '6 seconds' < config.starts_close_at
          AND NOW() + INTERVAL '6 seconds' < config.submissions_close_at"#)
        .bind(attempt_id).bind(version).execute(&mut *tx).await?.rows_affected() == 1;
    tx.commit().await?;
    Ok(changed)
}

pub(crate) async fn record_go(
    pool: &PgPool,
    attempt_id: i64,
    version: i64,
    at: DateTime<Utc>,
    delivery_claim: Option<&str>,
) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    lock_attempt(&mut tx, attempt_id).await?;
    let changed = sqlx::query(r#"UPDATE qualifier_attempts attempt SET state = 'running', started_at = $3,
        deadline_at = LEAST($3 + config.async_run_limit, config.submissions_close_at), control_version = control_version + 1
        FROM pooled_qualifier_configs config
        WHERE attempt.id = $1 AND attempt.control_version = $2 AND attempt.state IN ('revealed', 'starting')
          AND config.series = attempt.series AND config.event = attempt.event
          AND $3 < config.starts_close_at AND $3 < config.submissions_close_at
          AND ($4::TEXT IS NULL OR (attempt.delivery_claim = $4 AND attempt.delivery_claim_until > NOW()))"#)
        .bind(attempt_id).bind(version).bind(at).bind(delivery_claim).execute(&mut *tx).await?.rows_affected() == 1;
    if !changed {
        return Err(Error::InvalidTransition(
            "GO is stale or outside the start window".into(),
        ));
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn participant_finish(
    pool: &PgPool,
    attempt_id: i64,
    version: i64,
    at: DateTime<Utc>,
    forfeit: bool,
) -> Result<i64, Error> {
    let mut tx = pool.begin().await?;
    lock_attempt(&mut tx, attempt_id).await?;
    let next = sqlx::query_scalar::<_, i64>(r#"UPDATE qualifier_attempts SET state = 'awaiting_verification',
        player_finished_at = $3, participant_outcome = CASE WHEN $4 THEN 'forfeit' ELSE 'finished' END,
        undo_until = CASE WHEN $4 THEN NULL ELSE $3 + INTERVAL '30 seconds' END,
        control_version = control_version + 1
        WHERE id = $1 AND control_version = $2 AND state = 'running'
          AND $3 >= started_at AND $3 <= deadline_at RETURNING control_version"#)
        .bind(attempt_id).bind(version).bind(at).bind(forfeit).fetch_optional(&mut *tx).await?
        .ok_or_else(|| Error::InvalidTransition("finish is stale or outside the run window".into()))?;
    tx.commit().await?;
    Ok(next)
}

pub(crate) async fn revert_finish(
    pool: &PgPool,
    attempt_id: i64,
    control_version: i64,
) -> Result<i64, Error> {
    let mut transaction = pool.begin().await?;
    lock_attempt(&mut transaction, attempt_id).await?;
    let next = sqlx::query_scalar::<_, i64>(
        r#"UPDATE qualifier_attempts SET state = 'running', player_finished_at = NULL, participant_outcome = NULL, undo_until = NULL,
            control_version = control_version + 1
        WHERE id = $1 AND control_version = $2 AND state = 'awaiting_verification'
          AND official_outcome IS NULL AND NOW() <= deadline_at AND NOW() <= undo_until AND participant_outcome = 'finished'
        RETURNING control_version"#,
    )
    .bind(attempt_id)
    .bind(control_version)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| Error::InvalidTransition("finish can no longer be reverted".into()))?;
    transaction.commit().await?;
    Ok(next)
}

pub(crate) async fn finalize(
    transaction: &mut Transaction<'_, Postgres>,
    attempt_id: i64,
    control_version: i64,
    outcome: Outcome,
    vod: Option<&str>,
    verified_by: Option<i64>,
) -> Result<i64, Error> {
    lock_attempt(transaction, attempt_id).await?;
    if let Some(actor) = verified_by {
        check_verifier(transaction, attempt_id, actor).await?;
        validate_result(outcome, vod)?;
    }
    let (outcome_name, finish_time, par_eligible) = match outcome {
        Outcome::Finished(duration) => (
            "finished",
            Some(sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: i64::try_from(duration.as_micros())
                    .map_err(|_| Error::InvalidTransition("finish time is too large".into()))?,
            }),
            true,
        ),
        Outcome::Forfeit => ("forfeit", None, true),
        Outcome::Dq => ("dq", None, false),
        Outcome::Invalid => ("invalid", None, false),
    };
    let next = sqlx::query_scalar::<_, i64>(
        r#"UPDATE qualifier_attempts SET state = 'finalized',
            player_finished_at = COALESCE(player_finished_at, NOW()),
            official_outcome = $3, official_time = $4, vod = $5,
            par_eligible = $6, verified_at = NOW(), verified_by = $7,
            control_version = control_version + 1
        WHERE id = $1 AND control_version = $2
          AND state IN ('assigned', 'revealed', 'starting', 'running', 'awaiting_verification')
        RETURNING control_version"#,
    )
    .bind(attempt_id)
    .bind(control_version)
    .bind(outcome_name)
    .bind(finish_time)
    .bind(vod)
    .bind(par_eligible)
    .bind(verified_by)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| Error::InvalidTransition("result control is stale or already final".into()))?;
    Ok(next)
}

async fn check_verifier(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: i64,
    actor: i64,
) -> Result<(), Error> {
    let (organizer, participant): (bool, bool) = sqlx::query_as(r#"SELECT
        EXISTS(SELECT 1 FROM organizers WHERE organizer = $2 AND (series, event) = (SELECT series, event FROM qualifier_attempts WHERE id = $1)),
        EXISTS(SELECT 1 FROM team_members WHERE member = $2 AND team = (SELECT team_id FROM qualifier_attempts WHERE id = $1))"#)
        .bind(attempt_id).bind(actor).fetch_one(&mut **tx).await?;
    if participant || (!organizer && !User::GLOBAL_ADMIN_USER_IDS.contains(&(actor as u64))) {
        return Err(Error::InvalidTransition(
            "a different event organizer must verify this attempt".into(),
        ));
    }
    Ok(())
}

/// A result edit never changes seed, retry credit, counted status, or the run clock.
pub(crate) async fn correct_result(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: i64,
    version: i64,
    actor: i64,
    reason: &str,
    outcome: Outcome,
    vod: Option<&str>,
) -> Result<(), Error> {
    lock_attempt(tx, attempt_id).await?;
    check_verifier(tx, attempt_id, actor).await?;
    if reason.trim().is_empty() {
        return Err(Error::InvalidTransition(
            "a correction reason is required".into(),
        ));
    }
    let before: serde_json::Value = sqlx::query_scalar("SELECT to_jsonb(attempt) - 'correction_history' - 'delivery_messages' FROM qualifier_attempts attempt WHERE id = $1 AND control_version = $2 AND (state IN ('awaiting_verification', 'finalized') OR ($3 AND state IN ('assigned', 'revealed', 'starting', 'running'))) AND retry_banned_at IS NULL")
        .bind(attempt_id).bind(version).bind(!matches!(outcome, Outcome::Finished(_))).fetch_optional(&mut **tx).await?
        .ok_or_else(|| Error::InvalidTransition("stale result, unfinished attempt, or active disclosure sanction".into()))?;
    validate_result(outcome, vod)?;
    let (name, time, eligible) = outcome_values(outcome)?;
    sqlx::query(r#"UPDATE qualifier_attempts attempt SET state = 'finalized', official_outcome = $3,
        official_time = $4, vod = $5, par_eligible = $6, verified_at = NOW(), verified_by = $7,
        control_version = control_version + 1,
        correction_history = correction_history || jsonb_build_array(jsonb_build_object(
            'action', 'result', 'actor', $7::BIGINT, 'at', NOW(), 'reason', $8::TEXT, 'before', $9::JSONB,
            'after', jsonb_build_object('outcome', $3::TEXT, 'time', $4::INTERVAL, 'vod', $5::TEXT, 'par_eligible', $6::BOOLEAN)))
        WHERE id = $1 AND control_version = $2"#)
        .bind(attempt_id).bind(version).bind(name).bind(time).bind(vod).bind(eligible).bind(actor).bind(reason.trim()).bind(before)
        .execute(&mut **tx).await?;
    Ok(())
}

fn validate_result(outcome: Outcome, vod: Option<&str>) -> Result<(), Error> {
    if let Outcome::Finished(time) = outcome {
        if time.is_zero()
            || vod.is_none_or(|vod| {
                Url::parse(vod).map_or(true, |url| {
                    !matches!(url.scheme(), "https" | "http") || url.host_str().is_none()
                })
            })
        {
            return Err(Error::InvalidTransition(
                "a finish requires a positive time and a valid VOD URL".into(),
            ));
        }
    }
    Ok(())
}

fn outcome_values(
    outcome: Outcome,
) -> Result<
    (
        &'static str,
        Option<sqlx::postgres::types::PgInterval>,
        bool,
    ),
    Error,
> {
    Ok(match outcome {
        Outcome::Finished(time) => (
            "finished",
            Some(sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: i64::try_from(time.as_micros())
                    .map_err(|_| Error::InvalidTransition("finish time too large".into()))?,
            }),
            true,
        ),
        Outcome::Forfeit => ("forfeit", None, true),
        Outcome::Dq => ("dq", None, false),
        Outcome::Invalid => ("invalid", None, false),
    })
}

pub(crate) async fn disclosure(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: i64,
    version: i64,
    actor: i64,
    reason: &str,
    reverse: bool,
) -> Result<(), Error> {
    lock_attempt(tx, attempt_id).await?;
    check_verifier(tx, attempt_id, actor).await?;
    if reason.trim().is_empty() {
        return Err(Error::InvalidTransition(
            "a sanction reason is required".into(),
        ));
    }
    let (team, mode): (i64, i64) = sqlx::query_as("SELECT team_id, mode_id FROM qualifier_attempts WHERE id = $1 AND control_version = $2 AND state <> 'void'")
        .bind(attempt_id).bind(version).fetch_optional(&mut **tx).await?
        .ok_or_else(|| Error::InvalidTransition("stale sanction action".into()))?;
    if reverse {
        // The saved official result is restored; an unfinished run stays pending review.
        sqlx::query(r#"UPDATE qualifier_attempts attempt SET
            state = CASE WHEN saved.value->'before'->>'official_outcome' IS NULL THEN 'awaiting_verification' ELSE 'finalized' END,
            official_outcome = saved.value->'before'->>'official_outcome',
            official_time = (saved.value->'before'->>'official_time')::INTERVAL,
            vod = saved.value->'before'->>'vod', par_eligible = (saved.value->'before'->>'par_eligible')::BOOLEAN,
            retry_banned_at = NULL, retry_banned_by = NULL, retry_ban_reason = NULL,
            control_version = control_version + 1,
            correction_history = correction_history || jsonb_build_array(jsonb_build_object('action', 'disclosure_reversed', 'actor', $3::BIGINT, 'at', NOW(), 'reason', $4::TEXT,
                'before', to_jsonb(attempt) - 'correction_history' - 'delivery_messages', 'after', saved.value->'before'))
            FROM (SELECT id, (SELECT value FROM jsonb_array_elements(correction_history) WITH ORDINALITY AS history(value, position)
                WHERE value->>'action' = 'disclosure' ORDER BY position DESC LIMIT 1) AS value
                FROM qualifier_attempts WHERE team_id = $1 AND mode_id = $2 AND retry_banned_at IS NOT NULL AND state <> 'void') saved
            WHERE attempt.id = saved.id AND saved.value IS NOT NULL"#)
            .bind(team).bind(mode).bind(actor).bind(reason.trim()).execute(&mut **tx).await?;
    } else {
        sqlx::query(r#"UPDATE qualifier_attempts attempt SET state = 'finalized', official_outcome = 'dq', official_time = NULL,
            par_eligible = FALSE, verified_at = NOW(), verified_by = $3, retry_banned_at = NOW(), retry_banned_by = $3,
            retry_ban_reason = $4, player_finished_at = COALESCE(player_finished_at, NOW()), undo_until = NULL,
            control_version = control_version + 1,
            correction_history = correction_history || jsonb_build_array(jsonb_build_object('action', 'disclosure', 'actor', $3::BIGINT, 'at', NOW(), 'reason', $4::TEXT,
                'before', to_jsonb(attempt) - 'correction_history' - 'delivery_messages', 'after', jsonb_build_object('outcome', 'dq', 'par_eligible', FALSE)))
            WHERE team_id = $1 AND mode_id = $2 AND state <> 'void' AND retry_banned_at IS NULL"#)
            .bind(team).bind(mode).bind(actor).bind(reason.trim()).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn check_retry_ban(
    tx: &mut Transaction<'_, Postgres>,
    team: i64,
    mode: i64,
) -> Result<(), Error> {
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM qualifier_attempts WHERE team_id = $1 AND mode_id = $2 AND retry_banned_at IS NOT NULL)")
        .bind(team).bind(mode).fetch_one(&mut **tx).await? { return Err(Error::RetryForbidden); }
    Ok(())
}

pub(crate) async fn expire_overdue(pool: &PgPool) -> Result<Vec<(i64, Option<i64>)>, Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT 1 FROM pooled_qualifier_configs ORDER BY series, event FOR UPDATE")
        .fetch_all(&mut *transaction)
        .await?;
    sqlx::query(r#"UPDATE qualifier_attempts attempt SET state = 'awaiting_verification',
        participant_outcome = 'forfeit', player_finished_at = NOW(), control_version = control_version + 1,
        delivery_error = 'Last GO passed before this run started. Review service delivery before assigning a participant DNF.',
        correction_history = correction_history || jsonb_build_array(jsonb_build_object('action', 'unstarted_deadline', 'at', NOW(), 'previous_state', attempt.state))
        FROM pooled_qualifier_configs config WHERE attempt.series = config.series AND attempt.event = config.event
          AND attempt.source = 'async' AND attempt.state IN ('assigned', 'revealed', 'starting')
          AND NOT attempt.delivery_messages ? ('go-' || attempt.control_version)
          AND (NOW() >= config.starts_close_at OR NOW() >= config.submissions_close_at)"#)
        .execute(&mut *transaction).await?;
    let overdue: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
        r#"SELECT attempt.id, attempt.control_version, attempt.discord_thread
        FROM qualifier_attempts attempt
        JOIN pooled_qualifier_configs config USING (series, event)
        WHERE attempt.source = 'async' AND attempt.state = 'running'
          AND (attempt.deadline_at < NOW()
            OR (config.submissions_close_at IS NOT NULL AND config.submissions_close_at < NOW()))
        ORDER BY attempt.id FOR UPDATE OF attempt SKIP LOCKED"#,
    )
    .fetch_all(&mut *transaction)
    .await?;
    let mut expired = Vec::with_capacity(overdue.len());
    for (attempt_id, version, thread) in overdue {
        let next = sqlx::query_scalar::<_, i64>(
            r#"UPDATE qualifier_attempts attempt SET state = 'finalized', official_outcome = 'forfeit',
                official_time = NULL, player_finished_at = COALESCE(player_finished_at, deadline_at),
                verified_at = NOW(), control_version = control_version + 1
            FROM pooled_qualifier_configs config
            WHERE attempt.id = $1 AND attempt.control_version = $2 AND attempt.state = 'running'
              AND config.series = attempt.series AND config.event = attempt.event
              AND (attempt.deadline_at < NOW()
                OR (config.submissions_close_at IS NOT NULL AND config.submissions_close_at < NOW()))
            RETURNING attempt.control_version"#,
        )
        .bind(attempt_id)
        .bind(version)
        .fetch_optional(&mut *transaction)
        .await?;
        if next.is_some() {
            expired.push((attempt_id, thread));
        }
    }
    transaction.commit().await?;
    Ok(expired)
}

#[derive(Clone, Debug, sqlx::FromRow)]
struct SeedLoad {
    id: i64,
    assigned: i64,
}

fn pg_interval_duration(value: &sqlx::postgres::types::PgInterval) -> chrono::Duration {
    chrono::Duration::microseconds(value.microseconds)
        + chrono::Duration::days(i64::from(value.days))
        + chrono::Duration::days(i64::from(value.months) * 30)
}

fn choose_balanced_seed(seeds: &[SeedLoad], spread: i16, excluded: Option<i64>) -> Option<i64> {
    let min = seeds.iter().map(|seed| seed.assigned).min()?;
    let mut candidates = Vec::new();
    for seed in seeds {
        if Some(seed.id) == excluded {
            continue;
        }
        let prospective_max = seeds
            .iter()
            .map(|other| other.assigned + i64::from(other.id == seed.id))
            .max()
            .unwrap_or_default();
        let prospective_min = seeds
            .iter()
            .map(|other| other.assigned + i64::from(other.id == seed.id))
            .min()
            .unwrap_or_default();
        if prospective_max - prospective_min <= i64::from(spread) || seed.assigned == min {
            let weight = 100 / (1 + seed.assigned - min);
            candidates.push((seed.id, weight.max(1)));
        }
    }
    let total: i64 = candidates.iter().map(|(_, weight)| weight).sum();
    if total == 0 {
        return None;
    }
    let mut pick = rng().random_range(0..total);
    for (seed_id, weight) in candidates {
        if pick < weight {
            return Some(seed_id);
        }
        pick -= weight;
    }
    None
}

async fn allocation_candidates(
    transaction: &mut Transaction<'_, Postgres>,
    mode_id: i64,
) -> Result<Vec<SeedLoad>, sqlx::Error> {
    let rows: Vec<(i64, i64, serde_json::Value, String)> = sqlx::query_as(
        r#"SELECT seed.id,
            COUNT(attempt.id) FILTER (WHERE attempt.state <> 'void') AS assigned, seed.seed_data, seed.physical_seed_identity
        FROM qualifier_seeds seed
        LEFT JOIN qualifier_attempts attempt ON attempt.seed_id = seed.id
        WHERE seed.mode_id = $1 AND seed.source = 'async_pool'
          AND seed.generation_state = 'ready' AND seed.retired_at IS NULL
          AND (seed.generation_claim IS NOT NULL OR seed.settings_attested_at IS NOT NULL)
          AND seed.seed_data->>'type' = (SELECT CASE seed_gen_type WHEN 'owr' THEN 'alttpr_owr' ELSE seed_gen_type END FROM qualifier_modes WHERE id = $1)
          AND seed.generator_profile = (SELECT generator_profile FROM qualifier_modes WHERE id = $1)
          AND seed.settings_fingerprint = (SELECT settings_fingerprint FROM qualifier_modes WHERE id = $1)
        GROUP BY seed.id ORDER BY seed.id"#,
    )
    .bind(mode_id)
    .fetch_all(&mut **transaction)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, assigned, data, identity)| {
            (physical_seed_identity(&data).as_deref() == Some(identity.as_str()))
                .then_some(SeedLoad { id, assigned })
        })
        .collect())
}

async fn load_attempt_in(
    transaction: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<Attempt, sqlx::Error> {
    sqlx::query_as::<_, Attempt>(&format!(
        "SELECT {} FROM qualifier_attempts WHERE id = $1",
        Attempt::COLUMNS,
    ))
    .bind(id)
    .fetch_one(&mut **transaction)
    .await
}

async fn lock_request_context(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: i64,
    mode_id: i64,
) -> Result<Config, Error> {
    sqlx::query("SELECT 1 FROM pooled_qualifier_configs config WHERE (series, event) = (SELECT series, event FROM teams WHERE id = $1) FOR UPDATE OF config")
        .bind(team_id).fetch_optional(&mut **transaction).await?;
    let (series, event, team_config, qualifier_mode, delivery_ready): (
        String,
        String,
        String,
        String,
        bool,
    ) = sqlx::query_as(
        r#"SELECT team.series, team.event, event.team_config::TEXT, event.qualifier_mode,
                event.automated_asyncs AND event.discord_async_channel IS NOT NULL
            FROM teams team JOIN events event USING (series, event)
            WHERE team.id = $1 AND NOT team.resigned FOR UPDATE OF team"#,
    )
    .bind(team_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(Error::NotConfigured)?;
    if team_config != "solo" || qualifier_mode != "pooled_by_mode" {
        return Err(Error::NotConfigured);
    }
    if !delivery_ready {
        return Err(Error::DeliveryUnavailable);
    }
    let mode_event: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT series, event, enabled FROM qualifier_modes WHERE id = $1 FOR UPDATE",
    )
    .bind(mode_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if !matches!(mode_event, Some((ref s, ref e, true)) if s == &series && e == &event) {
        return Err(Error::ModeUnavailable);
    }
    let config = sqlx::query_as::<_, Config>(
        r#"SELECT series, event, required_mode_count, pool_seed_count,
            live_races_per_mode, requests_open_at, requests_close_at,
            starts_close_at, submissions_close_at, retries_close_at,
            results_release_at, async_run_limit, live_entry_close_lead,
            retry_limit, allocation_spread, par_finishers, score_scale,
            score_offset, score_minimum, score_maximum, requests_paused,
            settings_locked_at
        FROM pooled_qualifier_configs WHERE series = $1 AND event = $2 FOR UPDATE"#,
    )
    .bind(&series)
    .bind(&event)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(Error::NotConfigured)?;
    Ok(config)
}

fn check_requests(config: &Config, retry: bool) -> Result<(), Error> {
    if config.requests_paused {
        return Err(Error::Paused);
    }
    if !config.requests_open(Utc::now(), retry) {
        return Err(Error::WindowClosed);
    }
    Ok(())
}

async fn check_overlap(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: i64,
) -> Result<(), Error> {
    if sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(
        SELECT 1 FROM qualifier_attempts WHERE team_id = $1
          AND state IN ('assigned', 'revealed', 'starting', 'running')
        UNION ALL SELECT 1 FROM qualifier_live_entries WHERE team_id = $1
          AND eligible AND present_at_go IS NULL)"#,
    )
    .bind(team_id)
    .fetch_one(&mut **transaction)
    .await?
    {
        return Err(Error::ActiveAsync);
    }
    Ok(())
}

pub(crate) async fn request_async(
    pool: &PgPool,
    team_id: i64,
    mode_id: i64,
    created_by: i64,
) -> Result<Attempt, Error> {
    let mut transaction = pool.begin().await?;
    let config = lock_request_context(&mut transaction, team_id, mode_id).await?;
    if let Some(existing_id) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM qualifier_attempts WHERE team_id = $1 AND mode_id = $2 AND counts_for_entrant AND state <> 'void'",
    )
    .bind(team_id)
    .bind(mode_id)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let existing = load_attempt_in(&mut transaction, existing_id).await?;
        if existing.is_active_async() {
            transaction.commit().await?;
            return Ok(existing);
        }
        return Err(Error::AlreadyAttempted);
    }
    check_requests(&config, false)?;
    check_overlap(&mut transaction, team_id).await?;
    let seeds = allocation_candidates(&mut transaction, mode_id).await?;
    let seed_id =
        choose_balanced_seed(&seeds, config.allocation_spread, None).ok_or(Error::NoSeed)?;
    let attempt_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO qualifier_attempts
            (series, event, mode_id, seed_id, team_id, attempt_sequence, source, created_by)
        SELECT mode.series, mode.event, mode.id, $1, $2,
            (SELECT (COALESCE(MAX(attempt_sequence), 0) + 1)::SMALLINT FROM qualifier_attempts WHERE team_id = $2 AND mode_id = mode.id), 'async', $3
        FROM qualifier_modes mode WHERE mode.id = $4 RETURNING id"#,
    )
    .bind(seed_id)
    .bind(team_id)
    .bind(created_by)
    .bind(mode_id)
    .fetch_one(&mut *transaction)
    .await?;
    let attempt = load_attempt_in(&mut transaction, attempt_id).await?;
    transaction.commit().await?;
    Ok(attempt)
}

pub(crate) async fn request_async_retry(
    pool: &PgPool,
    team_id: i64,
    mode_id: i64,
    created_by: i64,
) -> Result<Attempt, Error> {
    let mut transaction = pool.begin().await?;
    let config = lock_request_context(&mut transaction, team_id, mode_id).await?;
    if config.retry_limit == 0 {
        return Err(Error::RetryUnavailable);
    }
    if let Some(replacement_id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT id FROM qualifier_attempts
        WHERE team_id = $1 AND retry_of IS NOT NULL AND state <> 'void'"#,
    )
    .bind(team_id)
    .fetch_optional(&mut *transaction)
    .await?
    {
        let replacement = load_attempt_in(&mut transaction, replacement_id).await?;
        if replacement.mode_id == mode_id && replacement.is_active_async() {
            transaction.commit().await?;
            return Ok(replacement);
        }
        return Err(Error::RetryUnavailable);
    }
    check_requests(&config, true)?;
    check_overlap(&mut transaction, team_id).await?;
    if sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM qualifier_live_entries WHERE team_id = $1
            AND retry_reserved_at IS NOT NULL AND retry_committed_at IS NULL
            AND retry_released_at IS NULL)"#,
    )
    .bind(team_id)
    .fetch_one(&mut *transaction)
    .await?
    {
        return Err(Error::RetryUnavailable);
    }
    check_retry_ban(&mut transaction, team_id, mode_id).await?;
    let original_id: i64 = sqlx::query_scalar(
        r#"SELECT id FROM qualifier_attempts
        WHERE team_id = $1 AND mode_id = $2 AND counts_for_entrant
          AND state IN ('awaiting_verification', 'finalized') FOR UPDATE"#,
    )
    .bind(team_id)
    .bind(mode_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(Error::RetryForbidden)?;
    let original = load_attempt_in(&mut transaction, original_id).await?;
    if original.retry_banned_at.is_some() {
        return Err(Error::RetryForbidden);
    }
    let seeds = allocation_candidates(&mut transaction, mode_id).await?;
    let seed_id = choose_balanced_seed(&seeds, config.allocation_spread, Some(original.seed_id))
        .or_else(|| {
            seeds
                .iter()
                .filter(|seed| seed.id != original.seed_id)
                .min_by_key(|seed| seed.assigned)
                .map(|seed| seed.id)
        })
        .ok_or(Error::NoReplacementSeed)?;
    let allocation_metadata = serde_json::json!({
        "excluded_seed": original.seed_id,
        "loads": seeds.iter().map(|seed| serde_json::json!({"seed": seed.id, "assigned": seed.assigned})).collect::<Vec<_>>(),
        "retry_exception": seeds.iter().find(|seed| seed.id == seed_id).is_some_and(|chosen| {
            let minimum = seeds.iter().map(|seed| seed.assigned).min().unwrap_or_default();
            let maximum = seeds.iter().map(|seed| seed.assigned + i64::from(seed.id == seed_id)).max().unwrap_or_default();
            chosen.assigned > minimum && maximum - minimum > i64::from(config.allocation_spread)
        }),
    });
    let next_sequence: i16 = sqlx::query_scalar(
        "SELECT (COALESCE(MAX(attempt_sequence), 0) + 1)::SMALLINT FROM qualifier_attempts WHERE team_id = $1 AND mode_id = $2",
    )
    .bind(team_id)
    .bind(mode_id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query("UPDATE qualifier_attempts SET counts_for_entrant = FALSE, control_version = control_version + 1 WHERE id = $1")
        .bind(original_id)
        .execute(&mut *transaction)
        .await?;
    let replacement_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO qualifier_attempts
            (series, event, mode_id, seed_id, team_id, attempt_sequence, source,
             retry_of, created_by, allocation_metadata)
        VALUES ($1, $2, $3, $4, $5, $6, 'async', $7, $8, $9) RETURNING id"#,
    )
    .bind(&config.series)
    .bind(&config.event)
    .bind(mode_id)
    .bind(seed_id)
    .bind(team_id)
    .bind(next_sequence)
    .bind(original_id)
    .bind(created_by)
    .bind(allocation_metadata)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query("UPDATE qualifier_attempts SET superseded_by = $1 WHERE id = $2")
        .bind(replacement_id)
        .bind(original_id)
        .execute(&mut *transaction)
        .await?;
    let replacement = load_attempt_in(&mut transaction, replacement_id).await?;
    transaction.commit().await?;
    Ok(replacement)
}

/// Reserve the event-wide retry for a particular live qualifier. The counted
/// result is left untouched until GO, so leaving beforehand releases the credit.
pub(crate) async fn reserve_live_retry(
    pool: &PgPool,
    team_id: i64,
    mode_id: i64,
    live_seed_id: i64,
    created_by: i64,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;
    let config = lock_request_context(&mut transaction, team_id, mode_id).await?;
    if config.retry_limit == 0 {
        return Err(Error::RetryUnavailable);
    }
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM qualifier_attempts WHERE team_id = $1 AND retry_of IS NOT NULL AND state <> 'void')",
    )
    .bind(team_id)
    .fetch_one(&mut *transaction)
    .await?
    {
        return Err(Error::RetryUnavailable);
    }
    if let Some(reserved_seed_id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT seed_id FROM qualifier_live_entries WHERE team_id = $1
            AND retry_reserved_at IS NOT NULL AND retry_committed_at IS NULL
            AND retry_released_at IS NULL"#,
    )
    .bind(team_id)
    .fetch_optional(&mut *transaction)
    .await?
    {
        if reserved_seed_id == live_seed_id {
            transaction.commit().await?;
            return Ok(());
        }
        return Err(Error::RetryUnavailable);
    }
    check_requests(&config, true)?;
    check_retry_ban(&mut transaction, team_id, mode_id).await?;
    let original_id: i64 = sqlx::query_scalar(
        r#"SELECT id FROM qualifier_attempts
        WHERE team_id = $1 AND mode_id = $2 AND counts_for_entrant
          AND retry_banned_at IS NULL AND state IN ('awaiting_verification', 'finalized')
        FOR UPDATE"#,
    )
    .bind(team_id)
    .bind(mode_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(Error::RetryForbidden)?;
    let racetime_id: String = sqlx::query_scalar(
        r#"SELECT users.racetime_id FROM users
        JOIN team_members ON team_members.member = users.id
        WHERE team_members.team = $1 AND users.id = $2 AND users.racetime_id IS NOT NULL"#,
    )
    .bind(team_id)
    .bind(created_by)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(Error::RacetimeRequired)?;
    let valid_seed: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM qualifier_seeds seed
            JOIN races race ON race.id = seed.live_race_id
            JOIN pooled_qualifier_configs config
              ON config.series = seed.series AND config.event = seed.event
            WHERE seed.id = $1 AND seed.mode_id = $2 AND seed.series = $3 AND seed.event = $4
              AND seed.source = 'live' AND seed.retired_at IS NULL
              AND race.start IS NOT NULL
              AND NOW() < race.start - config.live_entry_close_lead
              AND seed.entry_closed_at IS NULL)"#,
    )
    .bind(live_seed_id)
    .bind(mode_id)
    .bind(&config.series)
    .bind(&config.event)
    .fetch_one(&mut *transaction)
    .await?;
    if !valid_seed {
        return Err(Error::LiveEntryClosed);
    }
    let reserved = sqlx::query_scalar::<_, i64>(
        r#"INSERT INTO qualifier_live_entries
            (seed_id, series, event, mode_id, racetime_entrant_id, user_id, team_id,
             retry_original_attempt_id, retry_reserved_at, retry_declared_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), $6)
        ON CONFLICT (seed_id, racetime_entrant_id) DO UPDATE SET
            user_id = EXCLUDED.user_id, team_id = EXCLUDED.team_id,
            retry_original_attempt_id = EXCLUDED.retry_original_attempt_id,
            retry_reserved_at = NOW(), retry_committed_at = NULL,
            retry_released_at = NULL, retry_declared_by = EXCLUDED.retry_declared_by
        WHERE qualifier_live_entries.eligibility_frozen_at IS NULL
          AND qualifier_live_entries.retry_reserved_at IS NULL
        RETURNING id"#,
    )
    .bind(live_seed_id)
    .bind(&config.series)
    .bind(&config.event)
    .bind(mode_id)
    .bind(racetime_id)
    .bind(created_by)
    .bind(team_id)
    .bind(original_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(Error::LiveEntryClosed)?;
    let _ = reserved;
    transaction.commit().await?;
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct LiveEntrant {
    pub(crate) racetime_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LiveResult {
    pub(crate) racetime_id: String,
    pub(crate) outcome: Outcome,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LiveFreezeSummary {
    pub(crate) eligible: usize,
    pub(crate) excluded: usize,
    pub(crate) already_frozen: bool,
}

async fn live_seed_for_race(
    transaction: &mut Transaction<'_, Postgres>,
    race_id: i64,
) -> Result<(i64, String, String, i64, Option<DateTime<Utc>>), Error> {
    sqlx::query("SELECT 1 FROM pooled_qualifier_configs config WHERE (series, event) = (SELECT series, event FROM qualifier_seeds WHERE live_race_id = $1) FOR UPDATE OF config")
        .bind(race_id).fetch_optional(&mut **transaction).await?;
    sqlx::query_as(
        r#"SELECT seed.id, seed.series, seed.event, seed.mode_id, seed.entry_closed_at
        FROM qualifier_seeds seed
        JOIN pooled_qualifier_configs config USING (series, event)
        WHERE seed.live_race_id = $1 AND seed.source = 'live'
          AND seed.retired_at IS NULL
        FOR UPDATE OF seed"#,
    )
    .bind(race_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(Error::NotLiveQualifier)
}

/// Freeze the scoring roster at the configured generation cutoff. This is
/// idempotent so a bot restart can safely repeat the operation.
pub(crate) async fn freeze_live_eligibility(
    pool: &PgPool,
    race_id: i64,
    entrants: &[LiveEntrant],
) -> Result<LiveFreezeSummary, Error> {
    let mut transaction = pool.begin().await?;
    let (seed_id, series, event, mode_id, entry_closed_at) =
        live_seed_for_race(&mut transaction, race_id).await?;
    if entry_closed_at.is_some() {
        let (eligible, excluded): (i64, i64) = sqlx::query_as(
            r#"SELECT COUNT(*) FILTER (WHERE eligible), COUNT(*) FILTER (WHERE NOT eligible)
            FROM qualifier_live_entries WHERE seed_id = $1"#,
        )
        .bind(seed_id)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        return Ok(LiveFreezeSummary {
            eligible: eligible as usize,
            excluded: excluded as usize,
            already_frozen: true,
        });
    }

    let present_ids: HashSet<_> = entrants
        .iter()
        .map(|entrant| entrant.racetime_id.as_str())
        .collect();
    for entrant in entrants {
        let linked: Option<(i64, i64)> = sqlx::query_as(
            r#"SELECT users.id, teams.id
            FROM users
            JOIN team_members ON team_members.member = users.id
            JOIN teams ON teams.id = team_members.team
            WHERE users.racetime_id = $1 AND teams.series = $2 AND teams.event = $3
              AND NOT teams.resigned
            LIMIT 1"#,
        )
        .bind(&entrant.racetime_id)
        .bind(&series)
        .bind(&event)
        .fetch_optional(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO qualifier_live_entries
                (seed_id, series, event, mode_id, racetime_entrant_id, user_id, team_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (seed_id, racetime_entrant_id) DO UPDATE SET
                observed_at = NOW(), departed_at = NULL,
                user_id = EXCLUDED.user_id, team_id = EXCLUDED.team_id"#,
        )
        .bind(seed_id)
        .bind(&series)
        .bind(&event)
        .bind(mode_id)
        .bind(&entrant.racetime_id)
        .bind(linked.map(|row| row.0))
        .bind(linked.map(|row| row.1))
        .execute(&mut *transaction)
        .await?;
    }

    type EntryRow = (i64, String, Option<i64>, Option<i64>, Option<i64>);
    let entries: Vec<EntryRow> = sqlx::query_as(
        r#"SELECT id, racetime_entrant_id, user_id, team_id, retry_original_attempt_id
        FROM qualifier_live_entries WHERE seed_id = $1 ORDER BY id FOR UPDATE"#,
    )
    .bind(seed_id)
    .fetch_all(&mut *transaction)
    .await?;
    let mut summary = LiveFreezeSummary::default();
    for (entry_id, racetime_id, user_id, team_id, retry_original) in entries {
        let (eligible, reason) = if !present_ids.contains(racetime_id.as_str()) {
            (false, Some("not present at the entry cutoff"))
        } else if user_id.is_none() || team_id.is_none() {
            (
                false,
                Some("no active event registration linked to this racetime.gg account"),
            )
        } else {
            let team_id = team_id.expect("checked above");
            let active: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(SELECT 1 FROM qualifier_attempts
                WHERE team_id = $1 AND state IN ('assigned', 'revealed', 'starting', 'running'))"#,
            )
            .bind(team_id)
            .fetch_one(&mut *transaction)
            .await?;
            let counted: Option<i64> = sqlx::query_scalar(
                r#"SELECT id FROM qualifier_attempts
                WHERE team_id = $1 AND mode_id = $2 AND counts_for_entrant AND state <> 'void'"#,
            )
            .bind(team_id)
            .bind(mode_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if active {
                (
                    false,
                    Some("an async or live qualifier was active at the entry cutoff"),
                )
            } else if let Some(original_id) = retry_original {
                if counted == Some(original_id) {
                    (true, None)
                } else {
                    (
                        false,
                        Some("the declared retry no longer matches the counted result"),
                    )
                }
            } else if counted.is_some() {
                (false, Some("a counted result already exists for this mode"))
            } else {
                (true, None)
            }
        };
        sqlx::query(
            r#"UPDATE qualifier_live_entries SET eligibility_frozen_at = NOW(),
                eligible = $2, exclusion_reason = $3,
                departed_at = CASE WHEN $4 THEN NULL ELSE NOW() END
            WHERE id = $1"#,
        )
        .bind(entry_id)
        .bind(eligible)
        .bind(reason)
        .bind(present_ids.contains(racetime_id.as_str()))
        .execute(&mut *transaction)
        .await?;
        if eligible {
            summary.eligible += 1;
        } else {
            summary.excluded += 1;
        }
    }
    sqlx::query("UPDATE qualifier_seeds SET entry_closed_at = NOW() WHERE id = $1")
        .bind(seed_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        r#"UPDATE pooled_qualifier_configs
        SET settings_locked_at = COALESCE(settings_locked_at, NOW())
        WHERE series = $1 AND event = $2"#,
    )
    .bind(&series)
    .bind(&event)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(summary)
}

/// Commit the frozen live roster at GO. Retry reservations are consumed here;
/// reservations belonging to runners who left before GO are released.
pub(crate) async fn start_live(
    pool: &PgPool,
    race_id: i64,
    present_racetime_ids: &HashSet<String>,
) -> Result<usize, Error> {
    let mut transaction = pool.begin().await?;
    let (seed_id, series, event, mode_id, _) =
        live_seed_for_race(&mut transaction, race_id).await?;
    let ready: bool = sqlx::query_scalar("SELECT generation_state = 'ready' AND seed_data IS NOT NULL FROM qualifier_seeds WHERE id = $1")
        .bind(seed_id).fetch_one(&mut *transaction).await?;
    if !ready {
        return Err(Error::NoSeed);
    }
    for racetime_id in present_racetime_ids {
        sqlx::query(
            r#"INSERT INTO qualifier_live_entries
                (seed_id, series, event, mode_id, racetime_entrant_id,
                 eligibility_frozen_at, eligible, exclusion_reason)
            VALUES ($1, $2, $3, $4, $5, NOW(), FALSE, 'joined after the entry cutoff')
            ON CONFLICT (seed_id, racetime_entrant_id) DO NOTHING"#,
        )
        .bind(seed_id)
        .bind(&series)
        .bind(&event)
        .bind(mode_id)
        .bind(racetime_id)
        .execute(&mut *transaction)
        .await?;
    }
    type EntryRow = (
        i64,
        String,
        Option<i64>,
        bool,
        Option<bool>,
        Option<i64>,
        Option<i64>,
    );
    let entries: Vec<EntryRow> = sqlx::query_as(
        r#"SELECT id, racetime_entrant_id, team_id,
            retry_reserved_at IS NOT NULL AND retry_committed_at IS NULL AND retry_released_at IS NULL,
            present_at_go, retry_original_attempt_id, attempt_id
        FROM qualifier_live_entries WHERE seed_id = $1 ORDER BY id FOR UPDATE"#,
    )
    .bind(seed_id)
    .fetch_all(&mut *transaction)
    .await?;
    let mut started = 0;
    for (
        entry_id,
        racetime_id,
        team_id,
        retry_reserved,
        present_at_go,
        retry_original,
        attempt_id,
    ) in entries
    {
        if present_at_go.is_some() || attempt_id.is_some() {
            continue;
        }
        let present = present_racetime_ids.contains(&racetime_id);
        sqlx::query(
            r#"UPDATE qualifier_live_entries SET present_at_go = $2,
                departed_at = CASE WHEN $2 THEN departed_at ELSE COALESCE(departed_at, NOW()) END,
                retry_released_at = CASE WHEN NOT $2 AND retry_reserved_at IS NOT NULL
                    AND retry_committed_at IS NULL THEN NOW() ELSE retry_released_at END
            WHERE id = $1"#,
        )
        .bind(entry_id)
        .bind(present)
        .execute(&mut *transaction)
        .await?;
        if !present {
            continue;
        }
        let eligible: bool = sqlx::query_scalar(
            "SELECT COALESCE(eligible, FALSE) FROM qualifier_live_entries WHERE id = $1",
        )
        .bind(entry_id)
        .fetch_one(&mut *transaction)
        .await?;
        let Some(team_id) = team_id.filter(|_| eligible) else {
            continue;
        };

        let next_sequence: i16 = sqlx::query_scalar(
            "SELECT (COALESCE(MAX(attempt_sequence), 0) + 1)::SMALLINT FROM qualifier_attempts WHERE team_id = $1 AND mode_id = $2",
        )
        .bind(team_id)
        .bind(mode_id)
        .fetch_one(&mut *transaction)
        .await?;
        if retry_reserved {
            let Some(original_id) = retry_original else {
                continue;
            };
            let valid_original: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(SELECT 1 FROM qualifier_attempts WHERE id = $1
                    AND team_id = $2 AND mode_id = $3 AND counts_for_entrant
                    AND retry_banned_at IS NULL AND state IN ('awaiting_verification', 'finalized'))"#,
            )
            .bind(original_id)
            .bind(team_id)
            .bind(mode_id)
            .fetch_one(&mut *transaction)
            .await?;
            if !valid_original {
                sqlx::query(
                    r#"UPDATE qualifier_live_entries SET eligible = FALSE,
                        exclusion_reason = 'the declared retry was no longer valid at GO',
                        retry_released_at = NOW() WHERE id = $1"#,
                )
                .bind(entry_id)
                .execute(&mut *transaction)
                .await?;
                continue;
            }
            sqlx::query(
                "UPDATE qualifier_attempts SET counts_for_entrant = FALSE, control_version = control_version + 1 WHERE id = $1",
            )
            .bind(original_id)
            .execute(&mut *transaction)
            .await?;
        }
        let replacement_of = retry_reserved.then_some(retry_original).flatten();
        let attempt_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO qualifier_attempts
                (series, event, mode_id, seed_id, team_id, attempt_sequence, source,
                 state, requested_at, revealed_at, started_at, retry_of)
            VALUES ($1, $2, $3, $4, $5, $6, 'live', 'running', NOW(), NOW(), NOW(), $7)
            RETURNING id"#,
        )
        .bind(&series)
        .bind(&event)
        .bind(mode_id)
        .bind(seed_id)
        .bind(team_id)
        .bind(next_sequence)
        .bind(replacement_of)
        .fetch_one(&mut *transaction)
        .await?;
        if let Some(original_id) = replacement_of {
            sqlx::query("UPDATE qualifier_attempts SET superseded_by = $1 WHERE id = $2")
                .bind(attempt_id)
                .bind(original_id)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(
            r#"UPDATE qualifier_live_entries SET attempt_id = $2,
                retry_committed_at = CASE WHEN $3 THEN NOW() ELSE retry_committed_at END
            WHERE id = $1"#,
        )
        .bind(entry_id)
        .bind(attempt_id)
        .bind(retry_reserved)
        .execute(&mut *transaction)
        .await?;
        started += 1;
    }
    sqlx::query(
        r#"UPDATE qualifier_live_entries SET retry_released_at = NOW()
        WHERE seed_id = $1 AND retry_reserved_at IS NOT NULL
          AND retry_committed_at IS NULL AND retry_released_at IS NULL AND attempt_id IS NULL"#,
    )
    .bind(seed_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE pooled_qualifier_configs SET settings_locked_at = COALESCE(settings_locked_at, NOW()) WHERE series = $1 AND event = $2",
    )
    .bind(&series)
    .bind(&event)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(started)
}

/// Finalize all counted attempts from one live room using the authoritative
/// Racetime result. Attempts stay active until this whole-room transition.
pub(crate) async fn finish_live(
    pool: &PgPool,
    race_id: i64,
    results: &[LiveResult],
) -> Result<usize, Error> {
    let mut transaction = pool.begin().await?;
    let (seed_id, _, _, _, _) = live_seed_for_race(&mut transaction, race_id).await?;
    let outcomes: HashMap<_, _> = results
        .iter()
        .map(|result| (result.racetime_id.as_str(), result.outcome))
        .collect();
    let attempts: Vec<(i64, String, i64)> = sqlx::query_as(
        r#"SELECT attempt.id, entry.racetime_entrant_id, attempt.control_version
        FROM qualifier_live_entries entry
        JOIN qualifier_attempts attempt ON attempt.id = entry.attempt_id
        WHERE entry.seed_id = $1 AND attempt.source = 'live'
          AND attempt.state <> 'finalized' AND attempt.state <> 'void'
        ORDER BY attempt.id FOR UPDATE OF attempt"#,
    )
    .bind(seed_id)
    .fetch_all(&mut *transaction)
    .await?;
    let mut finalized = 0;
    for (attempt_id, racetime_id, control_version) in attempts {
        let outcome = outcomes
            .get(racetime_id.as_str())
            .copied()
            .unwrap_or(Outcome::Forfeit);
        finalize(
            &mut transaction,
            attempt_id,
            control_version,
            outcome,
            None,
            None,
        )
        .await?;
        finalized += 1;
    }
    transaction.commit().await?;
    Ok(finalized)
}

/// Cancel a live cohort without manufacturing forfeits. If GO already consumed a
/// retry, void its replacement and restore the original counted result.
pub(crate) async fn cancel_live(pool: &PgPool, race_id: i64) -> Result<usize, Error> {
    let mut transaction = pool.begin().await?;
    let (seed_id, _, _, _, _) = live_seed_for_race(&mut transaction, race_id).await?;
    let attempts: Vec<(i64, Option<i64>)> = sqlx::query_as(
        r#"SELECT attempt.id, attempt.retry_of FROM qualifier_attempts attempt
        JOIN qualifier_live_entries entry ON entry.attempt_id = attempt.id
        WHERE entry.seed_id = $1 AND attempt.source = 'live' AND attempt.state <> 'void'
        ORDER BY attempt.id FOR UPDATE OF attempt"#,
    )
    .bind(seed_id)
    .fetch_all(&mut *transaction)
    .await?;
    for (attempt_id, original_id) in &attempts {
        sqlx::query(
            r#"UPDATE qualifier_attempts SET state = 'void', counts_for_entrant = FALSE,
                official_outcome = NULL, official_time = NULL, verified_at = NULL,
                verified_by = NULL, voided_at = NOW(), void_reason = 'live race cancelled',
                control_version = control_version + 1 WHERE id = $1"#,
        )
        .bind(attempt_id)
        .execute(&mut *transaction)
        .await?;
        if let Some(original_id) = original_id {
            sqlx::query(
                "UPDATE qualifier_attempts SET counts_for_entrant = TRUE, superseded_by = NULL WHERE id = $1",
            )
            .bind(original_id)
            .execute(&mut *transaction)
            .await?;
        }
    }
    sqlx::query(
        r#"UPDATE qualifier_live_entries SET present_at_go = COALESCE(present_at_go, FALSE),
            departed_at = COALESCE(departed_at, NOW()),
            retry_committed_at = NULL,
            retry_released_at = CASE WHEN retry_reserved_at IS NOT NULL THEN NOW()
                ELSE retry_released_at END
        WHERE seed_id = $1"#,
    )
    .bind(seed_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(attempts.len())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Finished(Duration),
    Forfeit,
    Dq,
    Invalid,
}

#[derive(Clone, Debug)]
pub(crate) struct Performance {
    pub(crate) team_id: i64,
    pub(crate) mode_id: i64,
    pub(crate) seed_id: i64,
    pub(crate) counts_for_entrant: bool,
    pub(crate) par_eligible: bool,
    pub(crate) outcome: Outcome,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ModeScore {
    Pending,
    Score(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TeamScore {
    pub(crate) team_id: i64,
    pub(crate) modes: Vec<(i64, ModeScore)>,
    pub(crate) average: Option<f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Standing {
    pub(crate) team_id: i64,
    pub(crate) entered: usize,
    pub(crate) finished: usize,
    pub(crate) forfeited: usize,
    pub(crate) mode_scores: Vec<(i16, ModeScore, bool)>,
    pub(crate) average: Option<f64>,
}

pub(crate) fn score(config: &Config, performances: &[Performance]) -> Vec<TeamScore> {
    let mut pars = HashMap::new();
    for seed_id in performances
        .iter()
        .map(|run| run.seed_id)
        .collect::<HashSet<_>>()
    {
        let mut finishes: Vec<_> = performances
            .iter()
            .filter(|run| run.seed_id == seed_id && run.par_eligible)
            .filter_map(|run| match run.outcome {
                Outcome::Finished(time) => Some(time),
                _ => None,
            })
            .collect();
        finishes.sort_unstable();
        if finishes.len() >= config.par_finishers as usize {
            let count = config.par_finishers as usize;
            let seconds = finishes[..count]
                .iter()
                .map(Duration::as_secs_f64)
                .sum::<f64>()
                / count as f64;
            if seconds > 0.0 {
                pars.insert(seed_id, seconds);
            }
        }
    }
    let mut teams: HashMap<i64, Vec<(i64, ModeScore)>> = HashMap::new();
    for run in performances.iter().filter(|run| run.counts_for_entrant) {
        let mode_score = match run.outcome {
            Outcome::Forfeit | Outcome::Dq | Outcome::Invalid => ModeScore::Score(0.0),
            Outcome::Finished(time) => pars.get(&run.seed_id).map_or(ModeScore::Pending, |par| {
                ModeScore::Score(
                    (config.score_scale * (config.score_offset - time.as_secs_f64() / par))
                        .clamp(config.score_minimum, config.score_maximum),
                )
            }),
        };
        teams
            .entry(run.team_id)
            .or_default()
            .push((run.mode_id, mode_score));
    }
    teams
        .into_iter()
        .map(|(team_id, mut modes)| {
            modes.sort_by_key(|(mode_id, _)| *mode_id);
            let average = (modes.len() == config.required_mode_count as usize
                && modes
                    .iter()
                    .all(|(_, score)| matches!(score, ModeScore::Score(_))))
            .then(|| {
                modes
                    .iter()
                    .map(|(_, score)| match score {
                        ModeScore::Score(value) => *value,
                        ModeScore::Pending => unreachable!(),
                    })
                    .sum::<f64>()
                    / modes.len() as f64
            });
            TeamScore {
                team_id,
                modes,
                average,
            }
        })
        .collect()
}

pub(crate) fn hide_score(
    hiding: event::QualifierScoreHiding,
    released: bool,
    organizer: bool,
    is_async: bool,
) -> bool {
    !organizer
        && !released
        && match hiding {
            event::QualifierScoreHiding::None => false,
            event::QualifierScoreHiding::AsyncOnly => is_async,
            event::QualifierScoreHiding::FullPoints
            | event::QualifierScoreHiding::FullPointsCounts
            | event::QualifierScoreHiding::FullComplete => true,
        }
}

pub(crate) async fn standings(
    transaction: &mut Transaction<'_, Postgres>,
    config: &Config,
) -> Result<Vec<Standing>, sqlx::Error> {
    type Row = (
        i64,
        i64,
        i16,
        i64,
        bool,
        bool,
        String,
        Option<String>,
        Option<sqlx::postgres::types::PgInterval>,
        String,
    );
    let rows: Vec<Row> = sqlx::query_as(
        r#"SELECT attempt.team_id, attempt.mode_id, mode.position, attempt.seed_id,
            attempt.counts_for_entrant, attempt.par_eligible, attempt.state,
            attempt.official_outcome, attempt.official_time, attempt.source
        FROM qualifier_attempts attempt
        JOIN qualifier_modes mode ON mode.id = attempt.mode_id
        WHERE attempt.series = $1 AND attempt.event = $2 AND attempt.state <> 'void'
        ORDER BY attempt.team_id, mode.position, attempt.attempt_sequence"#,
    )
    .bind(&config.series)
    .bind(&config.event)
    .fetch_all(&mut **transaction)
    .await?;
    let performances: Vec<_> = rows
        .iter()
        .filter(|row| row.6 == "finalized")
        .filter_map(|row| {
            let outcome = match row.7.as_deref()? {
                "finished" => {
                    Outcome::Finished(pg_interval_duration(row.8.as_ref()?).to_std().ok()?)
                }
                "forfeit" => Outcome::Forfeit,
                "dq" => Outcome::Dq,
                "invalid" => Outcome::Invalid,
                _ => return None,
            };
            Some(Performance {
                team_id: row.0,
                mode_id: row.1,
                seed_id: row.3,
                counts_for_entrant: row.4,
                par_eligible: row.5,
                outcome,
            })
        })
        .collect();
    let scores: HashMap<_, _> = score(config, &performances)
        .into_iter()
        .map(|score| (score.team_id, score))
        .collect();
    let mode_positions: HashMap<_, _> = rows.iter().map(|row| (row.1, row.2)).collect();
    let team_ids: HashSet<_> = rows.iter().map(|row| row.0).collect();
    Ok(team_ids
        .into_iter()
        .map(|team_id| {
            let counted: Vec<_> = rows
                .iter()
                .filter(|row| row.0 == team_id && row.4)
                .collect();
            let scored = scores.get(&team_id);
            Standing {
                team_id,
                entered: counted.len(),
                finished: counted.iter().filter(|row| row.6 == "finalized").count(),
                forfeited: counted
                    .iter()
                    .filter(|row| matches!(row.7.as_deref(), Some("forfeit" | "dq" | "invalid")))
                    .count(),
                mode_scores: scored.map_or_else(Vec::new, |score| {
                    score
                        .modes
                        .iter()
                        .filter_map(|(mode_id, score)| {
                            Some((
                                *mode_positions.get(mode_id)?,
                                *score,
                                counted
                                    .iter()
                                    .any(|row| row.1 == *mode_id && row.9 == "async"),
                            ))
                        })
                        .collect()
                }),
                average: scored.and_then(|score| score.average),
            }
        })
        .collect())
}

pub(crate) async fn readiness(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let Some(config) = Config::load(transaction, series, event).await? else {
        return Ok(vec!["Pooled qualifier configuration is missing.".into()]);
    };
    let team_config: String =
        sqlx::query_scalar("SELECT team_config::TEXT FROM events WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .fetch_one(&mut **transaction)
            .await?;
    let mut errors = Vec::new();
    if team_config != "solo" {
        errors.push("Pooled qualifiers currently require solo entrants.".into());
    }
    let delivery_ready: bool = sqlx::query_scalar(
        r#"SELECT automated_asyncs AND discord_async_channel IS NOT NULL
        FROM events WHERE series = $1 AND event = $2"#,
    )
    .bind(series)
    .bind(event)
    .fetch_one(&mut **transaction)
    .await?;
    if !delivery_ready {
        errors.push(
            "Enable automated asyncs and select a Discord async channel before activation.".into(),
        );
    }
    if config.requests_open_at.is_none()
        || config.requests_close_at.is_none()
        || config.starts_close_at.is_none()
        || config.submissions_close_at.is_none()
        || config.retries_close_at.is_none()
        || config.results_release_at.is_none()
    {
        errors.push(
            "Set every request, GO, submission, retry, and publication timestamp before activation."
                .into(),
        );
    }
    if config
        .results_release_at
        .zip(config.submissions_close_at)
        .is_some_and(|(release, submissions)| release < submissions)
    {
        errors.push("Standings publication cannot precede the submission deadline.".into());
    }
    if config
        .requests_open_at
        .zip(config.requests_close_at)
        .is_some_and(|(open, close)| open >= close)
        || config
            .requests_close_at
            .zip(config.starts_close_at)
            .is_some_and(|(request, go)| request > go)
        || config
            .starts_close_at
            .zip(config.submissions_close_at)
            .is_some_and(|(go, submission)| go > submission)
        || config
            .retries_close_at
            .zip(config.requests_open_at)
            .is_some_and(|(retry, open)| retry <= open)
        || config
            .retries_close_at
            .zip(config.requests_close_at)
            .is_some_and(|(retry, close)| retry > close)
    {
        errors.push(
            "Request, retry, GO and submission deadlines must be in chronological order.".into(),
        );
    }
    let invalid_schedule: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM qualifier_seeds seed JOIN races race ON race.id = seed.live_race_id
        WHERE seed.series = $1 AND seed.event = $2 AND seed.retired_at IS NULL AND
          (NOT race.is_qualifier OR race.start IS NULL OR race.start < $3 OR race.start >= $4
            OR ($5::BOOLEAN AND race.start <= NOW()))"#,
    )
    .bind(series)
    .bind(event)
    .bind(config.requests_open_at)
    .bind(config.starts_close_at)
    .bind(config.settings_locked_at.is_none())
    .fetch_one(&mut **transaction)
    .await?;
    if invalid_schedule > 0 {
        errors.push("Linked live qualifiers must have valid start times within the qualifier window (and future times before first activation).".into());
    }
    let modes = Mode::for_event(transaction, series, event).await?;
    let enabled: Vec<_> = modes.iter().filter(|mode| mode.enabled).collect();
    if enabled.len() != config.required_mode_count as usize {
        errors.push(format!(
            "Expected {} enabled modes, found {}.",
            config.required_mode_count,
            enabled.len()
        ));
    }
    for mode in enabled {
        if mode.generator_profile != "default" {
            errors.push(format!(
                "{} uses generator profile {:?}, but this deployment only supports automatic profile \"default\".",
                mode.display_name, mode.generator_profile,
            ));
        }
        if racetime_bot::seed_gen_type::SeedGenType::from_db(
            Some(&mode.seed_gen_type),
            Some(&mode.seed_config),
        )
        .is_none_or(|kind| !generation::supported(&kind))
        {
            errors.push(format!(
                "{} has unsupported seed settings.",
                mode.display_name
            ));
        }
        let (ready_pool, live_races, mismatched_seeds): (i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                COUNT(*) FILTER (WHERE source = 'async_pool' AND generation_state = 'ready' AND retired_at IS NULL),
                COUNT(*) FILTER (WHERE source = 'live' AND retired_at IS NULL),
                COUNT(*) FILTER (WHERE retired_at IS NULL AND
                    (generator_profile <> $2 OR settings_fingerprint <> $3))
            FROM qualifier_seeds WHERE mode_id = $1"#,
        )
        .bind(mode.id)
        .bind(&mode.generator_profile)
        .bind(&mode.settings_fingerprint)
        .fetch_one(&mut **transaction)
        .await?;
        if let Some(kind) = racetime_bot::seed_gen_type::SeedGenType::from_db(
            Some(&mode.seed_gen_type),
            Some(&mode.seed_config),
        ) {
            let payloads: Vec<(Option<serde_json::Value>, bool)> = sqlx::query_as("SELECT seed_data, generation_claim IS NOT NULL OR settings_attested_at IS NOT NULL FROM qualifier_seeds WHERE mode_id = $1 AND source = 'async_pool' AND generation_state = 'ready' AND retired_at IS NULL")
                .bind(mode.id).fetch_all(&mut **transaction).await?;
            for (payload, trusted) in payloads {
                if !trusted {
                    errors.push(format!(
                        "{} has an imported seed without settings/build attestation.",
                        mode.display_name
                    ));
                }
                match payload {
                    Some(data) => {
                        if let Err(error) = generation::validate_payload(&kind, &data).await {
                            errors.push(format!(
                                "{} has an unusable private seed: {error}",
                                mode.display_name
                            ));
                        }
                    }
                    None => errors.push(format!(
                        "{} has a missing private seed payload.",
                        mode.display_name
                    )),
                }
            }
        }
        if ready_pool != i64::from(config.pool_seed_count) {
            errors.push(format!(
                "{} needs {} ready async seeds; found {}.",
                mode.display_name, config.pool_seed_count, ready_pool
            ));
        }
        if live_races != i64::from(config.live_races_per_mode) {
            errors.push(format!(
                "{} needs {} live qualifier races; found {}.",
                mode.display_name, config.live_races_per_mode, live_races
            ));
        }
        if mismatched_seeds != 0 {
            errors.push(format!(
                "{} has {} seeds from different settings or a different generator profile.",
                mode.display_name, mismatched_seeds
            ));
        }
    }
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            series: "test".into(),
            event: "test".into(),
            required_mode_count: 3,
            pool_seed_count: 3,
            live_races_per_mode: 3,
            requests_open_at: None,
            requests_close_at: None,
            starts_close_at: None,
            submissions_close_at: None,
            retries_close_at: None,
            results_release_at: None,
            async_run_limit: sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: 43_200_000_000,
            },
            live_entry_close_lead: sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: 600_000_000,
            },
            retry_limit: 1,
            allocation_spread: 2,
            par_finishers: 5,
            score_scale: 100.0,
            score_offset: 2.0,
            score_minimum: 0.0,
            score_maximum: 105.0,
            requests_paused: false,
            settings_locked_at: None,
        }
    }

    #[test]
    fn physical_seed_par_and_three_mode_average_match_tournament_rules() {
        let mut runs = Vec::new();
        for (team, minutes) in [50, 55, 60, 65, 70].into_iter().enumerate() {
            runs.push(Performance {
                team_id: team as i64,
                mode_id: 1,
                seed_id: 10,
                counts_for_entrant: team == 0,
                par_eligible: true,
                outcome: Outcome::Finished(Duration::from_secs(minutes * 60)),
            });
        }
        runs.extend([
            Performance {
                team_id: 0,
                mode_id: 2,
                seed_id: 20,
                counts_for_entrant: true,
                par_eligible: false,
                outcome: Outcome::Forfeit,
            },
            Performance {
                team_id: 0,
                mode_id: 3,
                seed_id: 30,
                counts_for_entrant: true,
                par_eligible: false,
                outcome: Outcome::Dq,
            },
        ]);
        let result = score(&config(), &runs)
            .into_iter()
            .find(|score| score.team_id == 0)
            .unwrap();
        assert_eq!(
            result.modes,
            vec![
                (1, ModeScore::Score(105.0)),
                (2, ModeScore::Score(0.0)),
                (3, ModeScore::Score(0.0))
            ]
        );
        assert_eq!(result.average, Some(35.0));
    }

    #[test]
    fn finish_is_pending_until_its_own_physical_seed_has_five_valid_finishes() {
        let runs = vec![
            Performance {
                team_id: 1,
                mode_id: 1,
                seed_id: 1,
                counts_for_entrant: true,
                par_eligible: true,
                outcome: Outcome::Finished(Duration::from_secs(3600)),
            },
            Performance {
                team_id: 2,
                mode_id: 1,
                seed_id: 2,
                counts_for_entrant: true,
                par_eligible: true,
                outcome: Outcome::Finished(Duration::from_secs(3600)),
            },
        ];
        assert!(
            score(&config(), &runs)
                .iter()
                .all(|team| team.modes[0].1 == ModeScore::Pending)
        );
    }

    #[test]
    fn allocator_respects_spread_and_can_select_every_candidate() {
        let equal = [
            SeedLoad { id: 1, assigned: 3 },
            SeedLoad { id: 2, assigned: 3 },
            SeedLoad { id: 3, assigned: 3 },
        ];
        let choices: HashSet<_> = (0..500)
            .filter_map(|_| choose_balanced_seed(&equal, 2, None))
            .collect();
        assert_eq!(choices, HashSet::from([1, 2, 3]));
        let skewed = [
            SeedLoad { id: 1, assigned: 5 },
            SeedLoad { id: 2, assigned: 3 },
            SeedLoad { id: 3, assigned: 3 },
        ];
        assert_ne!(choose_balanced_seed(&skewed, 2, None), Some(1));
    }

    #[test]
    fn allocation_recovers_retry_skew_and_tied_minima() {
        let mut loads = [
            SeedLoad {
                id: 1,
                assigned: 10,
            },
            SeedLoad {
                id: 2,
                assigned: 14,
            },
            SeedLoad {
                id: 3,
                assigned: 14,
            },
        ];
        for expected in 10..12 {
            assert_eq!(loads[0].assigned, expected);
            assert_eq!(choose_balanced_seed(&loads, 2, None), Some(1));
            loads[0].assigned += 1;
        }
        assert!(choose_balanced_seed(&loads, 2, None).is_some());
        let tied = [
            SeedLoad {
                id: 1,
                assigned: 10,
            },
            SeedLoad {
                id: 2,
                assigned: 10,
            },
            SeedLoad {
                id: 3,
                assigned: 14,
            },
        ];
        assert_eq!(choose_balanced_seed(&tied, 2, Some(1)), Some(2));
        let retry_skew = [
            SeedLoad {
                id: 1,
                assigned: 10,
            },
            SeedLoad {
                id: 2,
                assigned: 12,
            },
            SeedLoad {
                id: 3,
                assigned: 12,
            },
        ];
        assert_eq!(choose_balanced_seed(&retry_skew, 2, Some(1)), None);
        assert_eq!(choose_balanced_seed(&retry_skew[..1], 2, Some(1)), None);
        assert_eq!(choose_balanced_seed(&[], 2, None), None);
    }

    #[test]
    fn pooled_visibility_and_explicit_windows() {
        use event::QualifierScoreHiding::*;
        for hiding in [None, AsyncOnly, FullPoints, FullPointsCounts, FullComplete] {
            for is_async in [false, true] {
                assert!(!hide_score(hiding, true, false, is_async));
                assert!(!hide_score(hiding, false, true, is_async));
                assert_eq!(
                    hide_score(hiding, false, false, is_async),
                    match hiding {
                        None => false,
                        AsyncOnly => is_async,
                        _ => true,
                    }
                );
            }
        }
        assert!(!config().requests_open(Utc::now(), false));
        assert!(
            validate_result(
                Outcome::Finished(Duration::ZERO),
                Some("https://example.invalid/vod")
            )
            .is_err()
        );
        assert!(
            validate_result(
                Outcome::Finished(Duration::from_secs(1)),
                Some("javascript:alert(1)")
            )
            .is_err()
        );
        assert!(validate_result(Outcome::Forfeit, Option::None).is_ok());
    }

    async fn wait_for_lock_waiters(pool: &PgPool, count: i64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let waiting: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE '%pooled_qualifier_configs%'")
                    .fetch_one(pool).await.unwrap();
                if waiting >= count { break; }
                tokio::task::yield_now().await;
            }
        }).await.expect("pooled statements did not reach the shared event lock");
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn database_async_and_live_retry_lifecycle_is_transactional() {
        let pool = event::configuration::test_pool().await;
        let series = "casboots";
        let event = "pqtest";
        let team_ids = [-9_000_000_000_000_001_i64, -9_000_000_000_000_002_i64];
        let race_id = -9_000_000_000_000_003_i64;

        // A previous failed run may have left only these isolated fixture rows behind.
        sqlx::query("DELETE FROM qualifier_live_entries WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM qualifier_attempts WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM qualifier_seeds WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM qualifier_modes WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM pooled_qualifier_configs WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM team_members WHERE team = $1 OR team = $2")
            .bind(team_ids[0])
            .bind(team_ids[1])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM teams WHERE id = $1 OR id = $2")
            .bind(team_ids[0])
            .bind(team_ids[1])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM races WHERE id = $1")
            .bind(race_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM events WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        let result = std::panic::AssertUnwindSafe(async {
        let users: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, racetime_id FROM users WHERE racetime_id IS NOT NULL ORDER BY id LIMIT 2",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(users.len(), 2);
        let entrants = [
            (team_ids[0], users[0].0, users[0].1.clone()),
            (team_ids[1], users[1].0, users[1].1.clone()),
        ];
        sqlx::query(
            r#"INSERT INTO events
            (series, event, display_name, team_config, automated_asyncs,
             discord_async_channel, qualifier_mode)
            VALUES ($1, $2, 'Pooled qualifier integration test', 'solo', TRUE, 1,
                'pooled_by_mode')"#,
        )
        .bind(series)
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO teams (id, series, event) VALUES
            ($1, $3, $4), ($2, $3, $4)"#,
        )
        .bind(team_ids[0])
        .bind(team_ids[1])
        .bind(series)
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO team_members (team, member, status, role) VALUES
            ($1, $3, 'created', 'none'), ($2, $4, 'created', 'none')"#,
        )
        .bind(team_ids[0])
        .bind(team_ids[1])
        .bind(users[0].0)
        .bind(users[1].0)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO races (series, event, id, start, is_qualifier) VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', TRUE)",
        )
        .bind(series)
        .bind(event)
        .bind(race_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO pooled_qualifier_configs
            (series, event, required_mode_count, pool_seed_count, live_races_per_mode,
             requests_open_at, requests_close_at, starts_close_at,
             submissions_close_at, retries_close_at, results_release_at, requests_paused)
            VALUES ($1, $2, 1, 2, 1, NOW() - INTERVAL '1 hour',
                NOW() + INTERVAL '2 hours', NOW() + INTERVAL '3 hours',
                NOW() + INTERVAL '13 hours', NOW() + INTERVAL '2 hours',
                NOW() + INTERVAL '14 hours', FALSE)"#,
        )
        .bind(series)
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
        let mode_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO qualifier_modes
            (series, event, position, slug, display_name, seed_gen_type,
             seed_config, generator_profile, settings_fingerprint)
            VALUES ($1, $2, 1, 'test-mode', 'Test Mode', 'owr', '{}',
                'default', 'test-settings') RETURNING id"#,
        )
        .bind(series)
        .bind(event)
        .fetch_one(&pool)
        .await
        .unwrap();
        let seed_ids: Vec<i64> = sqlx::query_scalar(
            r#"INSERT INTO qualifier_seeds
            (series, event, mode_id, source, pool_position, generation_state,
             seed_data, generator_profile, physical_seed_identity, settings_fingerprint)
            VALUES
            ($1, $2, $3, 'async_pool', 1, 'ready', '{"type":"alttpr_owr","uuid":"00000000-0000-0000-0000-000000000001","hash1":"Bow","hash2":"Bow","hash3":"Bow","hash4":"Bow","hash5":"Bow"}', 'default', 'alttpr_owr:00000000-0000-0000-0000-000000000001', 'test-settings'),
            ($1, $2, $3, 'async_pool', 2, 'ready', '{"type":"alttpr_owr","uuid":"00000000-0000-0000-0000-000000000002","hash1":"Bow","hash2":"Bow","hash3":"Bow","hash4":"Bow","hash5":"Bow"}', 'default', 'alttpr_owr:00000000-0000-0000-0000-000000000002', 'test-settings')
            RETURNING id"#,
        )
        .bind(series)
        .bind(event)
        .bind(mode_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        let live_seed_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO qualifier_seeds
            (series, event, mode_id, source, live_race_id, generation_state,
             seed_data, generator_profile, physical_seed_identity, settings_fingerprint)
            VALUES ($1, $2, $3, 'live', $4, 'ready', '{"test":3}',
                'default', 'test-live-1', 'test-settings') RETURNING id"#,
        )
        .bind(series)
        .bind(event)
        .bind(mode_id)
        .bind(race_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query("UPDATE qualifier_seeds SET settings_attested_by=$3, settings_attested_at=NOW() WHERE series=$1 AND event=$2")
            .bind(series).bind(event).bind(entrants[0].1).execute(&pool).await.unwrap();
        let first = request_async(&pool, entrants[0].0, mode_id, entrants[0].1)
            .await
            .unwrap();
        let repeated = request_async(&pool, entrants[0].0, mode_id, entrants[0].1)
            .await
            .unwrap();
        assert_eq!(first.id, repeated.id);
        assert!(seed_ids.contains(&first.seed_id));
        crate::async_race::pooled::test_delivery_failures(&pool, first.id).await;
        let discord_thread = -first.id;
        sqlx::query("UPDATE qualifier_attempts SET discord_thread = $2 WHERE id = $1")
            .bind(first.id)
            .bind(discord_thread)
            .execute(&pool)
            .await
            .unwrap();
        let revealed = reveal(&pool, first.id, 1, discord_thread).await.unwrap();
        let repeated_reveal = reveal(&pool, first.id, 1, discord_thread).await.unwrap();
        assert_eq!(revealed.next_control_version, repeated_reveal.next_control_version);
        let revealed_run = crate::async_race::AsyncRun::PooledQualifier {
            attempt_id: first.id,
            control_version: revealed.next_control_version,
        };
        revealed_run.record_start_time(&pool).await.unwrap();
        let running_version = revealed.next_control_version + 1;
        let running_run = crate::async_race::AsyncRun::PooledQualifier {
            attempt_id: first.id,
            control_version: running_version,
        };
        running_run.check_finish_allowed(&pool).await.unwrap();
        running_run.set_player_finished_at(&pool).await.unwrap();
        let reverted_version = revert_finish(&pool, first.id, running_version + 1)
            .await
            .unwrap();
        let reverted_run = crate::async_race::AsyncRun::PooledQualifier {
            attempt_id: first.id,
            control_version: reverted_version,
        };
        reverted_run.check_finish_allowed(&pool).await.unwrap();
        reverted_run.set_player_finished_at(&pool).await.unwrap();
        let mut transaction = pool.begin().await.unwrap();
        finalize(
            &mut transaction,
            first.id,
            reverted_version + 1,
            Outcome::Finished(Duration::from_secs(3600)),
            Some("https://example.invalid/vod"),
            None,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        let replacement = request_async_retry(&pool, entrants[0].0, mode_id, entrants[0].1)
            .await
            .unwrap();
        let repeated_replacement =
            request_async_retry(&pool, entrants[0].0, mode_id, entrants[0].1)
                .await
                .unwrap();
        assert_eq!(replacement.id, repeated_replacement.id);
        assert_eq!(replacement.seed_id, *seed_ids.iter().find(|id| **id != first.seed_id).unwrap());
        let original_counts: bool = sqlx::query_scalar(
            "SELECT counts_for_entrant FROM qualifier_attempts WHERE id = $1",
        )
        .bind(first.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!original_counts);

        let live_original_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO qualifier_attempts
            (series, event, mode_id, seed_id, team_id, attempt_sequence, source,
             state, official_outcome, official_time, verified_at)
            VALUES ($1, $2, $3, $4, $5, 1, 'async', 'finalized',
                'finished', INTERVAL '1 hour', NOW()) RETURNING id"#,
        )
        .bind(series)
        .bind(event)
        .bind(mode_id)
        .bind(seed_ids[0])
        .bind(entrants[1].0)
        .fetch_one(&pool)
        .await
        .unwrap();
        reserve_live_retry(
            &pool,
            entrants[1].0,
            mode_id,
            live_seed_id,
            entrants[1].1,
        )
        .await
        .unwrap();
        reserve_live_retry(
            &pool,
            entrants[1].0,
            mode_id,
            live_seed_id,
            entrants[1].1,
        )
        .await
        .unwrap();
        let summary = freeze_live_eligibility(
            &pool,
            race_id,
            &[LiveEntrant {
                racetime_id: entrants[1].2.clone(),
            }],
        )
        .await
        .unwrap();
        assert_eq!(summary.eligible, 1);
        let repeated_freeze = freeze_live_eligibility(&pool, race_id, &[]).await.unwrap();
        assert!(repeated_freeze.already_frozen);
        assert_eq!(repeated_freeze.eligible, 1);
        assert_eq!(
            start_live(&pool, race_id, &HashSet::from([entrants[1].2.clone()]))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            start_live(&pool, race_id, &HashSet::from([entrants[1].2.clone()]))
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            finish_live(
                &pool,
                race_id,
                &[LiveResult {
                    racetime_id: entrants[1].2.clone(),
                    outcome: Outcome::Finished(Duration::from_secs(3900)),
                }],
            )
            .await
            .unwrap(),
            1
        );
        assert_eq!(finish_live(&pool, race_id, &[]).await.unwrap(), 0);
        assert_eq!(cancel_live(&pool, race_id).await.unwrap(), 1);
        assert_eq!(cancel_live(&pool, race_id).await.unwrap(), 0);
        let restored: (bool, Option<i64>) = sqlx::query_as(
            "SELECT counts_for_entrant, superseded_by FROM qualifier_attempts WHERE id = $1",
        )
        .bind(live_original_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(restored, (true, None));
        // Use another existing user as an isolated event organizer.
        let staff: i64 = sqlx::query_scalar("SELECT id FROM users WHERE id NOT IN ($1, $2) ORDER BY id LIMIT 1")
            .bind(entrants[0].1).bind(entrants[1].1).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO organizers(series, event, organizer) VALUES ($1,$2,$3)")
            .bind(series).bind(event).bind(staff).execute(&pool).await.unwrap();
        let version: i64 = sqlx::query_scalar("SELECT control_version FROM qualifier_attempts WHERE id=$1").bind(first.id).fetch_one(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(correct_result(&mut tx, first.id, version, entrants[0].1, "self verify", Outcome::Forfeit, None).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        correct_result(&mut tx, first.id, version, staff, "Correct timer from evidence", Outcome::Finished(Duration::from_secs(3550)), Some("https://example.invalid/corrected")).await.unwrap();
        assert!(correct_result(&mut tx, first.id, version, staff, "Stale editor", Outcome::Forfeit, None).await.is_err());
        tx.commit().await.unwrap();
        let unchanged: (bool, bool) = sqlx::query_as("SELECT counts_for_entrant, par_eligible FROM qualifier_attempts WHERE id=$1").bind(first.id).fetch_one(&pool).await.unwrap();
        assert_eq!(unchanged, (false, true));
        let mut tx = pool.begin().await.unwrap();
        disclosure(&mut tx, first.id, version + 1, staff, "Shared seed before release", false).await.unwrap();
        tx.commit().await.unwrap();
        let sanctioned: (bool, bool, bool) = sqlx::query_as("SELECT counts_for_entrant, par_eligible, retry_banned_at IS NOT NULL FROM qualifier_attempts WHERE id=$1").bind(replacement.id).fetch_one(&pool).await.unwrap();
        assert_eq!(sanctioned, (true, false, true));
        let mut tx = pool.begin().await.unwrap();
        assert!(check_retry_ban(&mut tx, entrants[0].0, mode_id).await.is_err());
        disclosure(&mut tx, first.id, version + 2, staff, "Evidence disproved disclosure", true).await.unwrap();
        tx.commit().await.unwrap();
        let restored: (bool, bool, String) = sqlx::query_as("SELECT counts_for_entrant, retry_banned_at IS NULL, state FROM qualifier_attempts WHERE id=$1").bind(replacement.id).fetch_one(&pool).await.unwrap();
        assert_eq!(restored, (true, true, "awaiting_verification".into()));
        assert!(matches!(request_async_retry(&pool, entrants[0].0, mode_id, entrants[0].1).await, Err(Error::RetryUnavailable)));

        // Two connections compete while an event lock keeps both pending.
        // Only one mode may allocate; the other observes the committed active run.
        let other_mode: i64 = sqlx::query_scalar(r#"INSERT INTO qualifier_modes(series,event,position,slug,display_name,seed_gen_type,seed_config,generator_profile,settings_fingerprint)
            VALUES ($1,$2,2,'other','Other','owr','{"base_settings":{}}','default','other') RETURNING id"#)
            .bind(series).bind(event).fetch_one(&pool).await.unwrap();
        let other_seed: i64 = sqlx::query_scalar(r#"INSERT INTO qualifier_seeds(series,event,mode_id,source,pool_position,generation_state,seed_data,generator_profile,physical_seed_identity,settings_fingerprint)
            VALUES ($1,$2,$3,'async_pool',1,'ready','{"type":"alttpr_owr","uuid":"00000000-0000-0000-0000-000000000004","hash1":"Bow","hash2":"Bow","hash3":"Bow","hash4":"Bow","hash5":"Bow"}','default','alttpr_owr:00000000-0000-0000-0000-000000000004','other') RETURNING id"#)
            .bind(series).bind(event).bind(other_mode).fetch_one(&pool).await.unwrap();
        sqlx::query("UPDATE qualifier_seeds SET settings_attested_by=$2, settings_attested_at=NOW() WHERE id=$1")
            .bind(other_seed).bind(staff).execute(&pool).await.unwrap();
        let mut blocker = pool.begin().await.unwrap();
        lock_event(&mut blocker, Series::from_str(series).unwrap(), event).await.unwrap();
        let gate = Arc::new(tokio::sync::Barrier::new(3));
        let launch = |retry| {
            let pool = pool.clone(); let gate = Arc::clone(&gate); let entrant = entrants[1].clone();
            tokio::spawn(async move {
                gate.wait().await;
                if retry { request_async_retry(&pool, entrant.0, mode_id, entrant.1).await }
                else { request_async(&pool, entrant.0, other_mode, entrant.1).await }
            })
        };
        let a = launch(false); let b = launch(true);
        gate.wait().await;
        blocker.commit().await.unwrap();
        let (a, b) = tokio::join!(a, b);
        let a = a.unwrap(); let b = b.unwrap();
        assert_ne!(a.is_ok(), b.is_ok());
        assert!(matches!(a.as_ref().err().or(b.as_ref().err()), Some(Error::ActiveAsync)));
        let winner = a.or(b).unwrap();
        sqlx::query("UPDATE qualifier_attempts SET state='awaiting_verification', participant_outcome='forfeit', player_finished_at=NOW(), control_version=control_version+1 WHERE id=$1").bind(winner.id).execute(&pool).await.unwrap();
        // Restore the test state for the excluded live reservation regression.
        sqlx::query("UPDATE qualifier_attempts SET superseded_by=NULL WHERE superseded_by=$1").bind(winner.id).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM qualifier_attempts WHERE id=$1").bind(winner.id).execute(&pool).await.unwrap();
        sqlx::query("UPDATE qualifier_attempts SET counts_for_entrant=TRUE, superseded_by=NULL WHERE id=$1").bind(live_original_id).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM qualifier_live_entries WHERE seed_id=$1").bind(live_seed_id).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM qualifier_attempts WHERE seed_id=$1 AND state='void'").bind(live_seed_id).execute(&pool).await.unwrap();
        sqlx::query("UPDATE qualifier_seeds SET entry_closed_at=NULL WHERE id=$1").bind(live_seed_id).execute(&pool).await.unwrap();
        // Exercise both lock arrival orders for request versus cutoff using
        // PostgreSQL's actual lock wait state, not sleeps standing in for concurrency.
        for request_first in [true, false] {
            sqlx::query("DELETE FROM qualifier_live_entries WHERE seed_id=$1").bind(live_seed_id).execute(&pool).await.unwrap();
            sqlx::query("DELETE FROM qualifier_attempts WHERE seed_id=$1 AND state='void'").bind(live_seed_id).execute(&pool).await.unwrap();
            sqlx::query("UPDATE qualifier_seeds SET entry_closed_at=NULL WHERE id=$1").bind(live_seed_id).execute(&pool).await.unwrap();
            reserve_live_retry(&pool, entrants[1].0, mode_id, live_seed_id, entrants[1].1).await.unwrap();
            let mut blocker = pool.begin().await.unwrap();
            lock_event(&mut blocker, Series::from_str(series).unwrap(), event).await.unwrap();
            let request = || { let pool = pool.clone(); let entrant = entrants[1].clone(); tokio::spawn(async move {
                request_async(&pool, entrant.0, other_mode, entrant.1).await
            }) };
            let cutoff = || { let pool = pool.clone(); let entrant = entrants[1].clone(); tokio::spawn(async move {
                freeze_live_eligibility(&pool, race_id, &[LiveEntrant { racetime_id: entrant.2 }]).await
            }) };
            let (request, cutoff) = if request_first {
                let request = request(); wait_for_lock_waiters(&pool, 1).await;
                let cutoff = cutoff(); wait_for_lock_waiters(&pool, 2).await; (request, cutoff)
            } else {
                let cutoff = cutoff(); wait_for_lock_waiters(&pool, 1).await;
                let request = request(); wait_for_lock_waiters(&pool, 2).await; (request, cutoff)
            };
            blocker.commit().await.unwrap();
            let result = request.await.unwrap(); let frozen = cutoff.await.unwrap().unwrap();
            assert_eq!(result.is_ok(), request_first);
            assert_eq!(frozen.eligible, usize::from(!request_first));
            if let Ok(attempt) = result { sqlx::query("DELETE FROM qualifier_attempts WHERE id=$1").bind(attempt.id).execute(&pool).await.unwrap(); }
            let started = start_live(&pool, race_id, &HashSet::from([entrants[1].2.clone()])).await.unwrap();
            assert_eq!(started, usize::from(!request_first));
            if !request_first { cancel_live(&pool, race_id).await.unwrap(); }
        }
        sqlx::query("DELETE FROM qualifier_live_entries WHERE seed_id=$1").bind(live_seed_id).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM qualifier_attempts WHERE seed_id=$1 AND state='void'").bind(live_seed_id).execute(&pool).await.unwrap();
        sqlx::query("UPDATE qualifier_seeds SET entry_closed_at=NULL WHERE id=$1").bind(live_seed_id).execute(&pool).await.unwrap();
        reserve_live_retry(&pool, entrants[1].0, mode_id, live_seed_id, entrants[1].1).await.unwrap();
        let active = request_async(&pool, entrants[1].0, other_mode, entrants[1].1).await.unwrap();
        let excluded = freeze_live_eligibility(&pool, race_id, &[LiveEntrant { racetime_id: entrants[1].2.clone() }]).await.unwrap();
        assert_eq!(excluded.excluded, 1);
        assert_eq!(start_live(&pool, race_id, &HashSet::from([entrants[1].2.clone()])).await.unwrap(), 0);
        let released: bool = sqlx::query_scalar("SELECT retry_released_at IS NOT NULL FROM qualifier_live_entries WHERE seed_id=$1 AND team_id=$2").bind(live_seed_id).bind(entrants[1].0).fetch_one(&pool).await.unwrap();
        assert!(released);
        assert!(matches!(request_async_retry(&pool, entrants[1].0, mode_id, entrants[1].1).await, Err(Error::ActiveAsync)));
        // GO and finish intent are immutable under repeated, stale controls.
        sqlx::query("UPDATE qualifier_attempts SET state='revealed', revealed_at=NOW(), discord_thread=$2 WHERE id=$1").bind(active.id).bind(-active.id).execute(&pool).await.unwrap();
        let (manual, forced) = tokio::join!(request_start(&pool, active.id, 1), request_start(&pool, active.id, 1));
        assert_ne!(manual.unwrap(), forced.unwrap());
        let go = Utc::now();
        record_go(&pool, active.id, 1, go, None).await.unwrap();
        assert!(record_go(&pool, active.id, 1, go + chrono::Duration::seconds(10), None).await.is_err());
        let stored: DateTime<Utc> = sqlx::query_scalar("SELECT started_at FROM qualifier_attempts WHERE id=$1").bind(active.id).fetch_one(&pool).await.unwrap();
        assert_eq!(stored.timestamp_micros(), go.timestamp_micros());
        let done = participant_finish(&pool, active.id, 2, Utc::now(), true).await.unwrap();
        assert!(revert_finish(&pool, active.id, done).await.is_err());
        let retry = request_async_retry(&pool, entrants[1].0, mode_id, entrants[1].1).await.unwrap();
        assert!(retry.is_active_async());
        sqlx::query("UPDATE pooled_qualifier_configs SET requests_paused=TRUE WHERE series=$1 AND event=$2").bind(series).bind(event).execute(&pool).await.unwrap();
        assert_eq!(request_async_retry(&pool, entrants[1].0, mode_id, entrants[1].1).await.unwrap().id, retry.id);
        // Generation completion uses its claim; stale workers cannot overwrite a ready seed.
        sqlx::query("UPDATE qualifier_seeds SET generation_state='generating', generation_claim='current', generation_claim_until=NOW()+INTERVAL '1 hour' WHERE id=$1").bind(other_seed).execute(&pool).await.unwrap();
        generation::complete(&pool, other_seed, "stale", Err(Error::NoSeed)).await.unwrap();
        let state: String = sqlx::query_scalar("SELECT generation_state FROM qualifier_seeds WHERE id=$1").bind(other_seed).fetch_one(&pool).await.unwrap();
        assert_eq!(state, "generating");
        generation::complete(&pool, other_seed, "current", Err(Error::NoSeed)).await.unwrap();
        let state: String = sqlx::query_scalar("SELECT generation_state FROM qualifier_seeds WHERE id=$1").bind(other_seed).fetch_one(&pool).await.unwrap();
        assert_eq!(state, "failed");

        event::qualifiers::route_tests::verify_pooled_routes(&pool, staff, entrants[0].1, series, event, other_mode).await;
        }).catch_unwind().await;

        sqlx::query("DELETE FROM qualifier_live_entries WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM qualifier_attempts WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM qualifier_seeds WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM qualifier_modes WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM pooled_qualifier_configs WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM organizers WHERE series=$1 AND event=$2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM team_members WHERE team = $1 OR team = $2")
            .bind(team_ids[0])
            .bind(team_ids[1])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM teams WHERE id = $1 OR id = $2")
            .bind(team_ids[0])
            .bind(team_ids[1])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM races WHERE id = $1")
            .bind(race_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM events WHERE series = $1 AND event = $2")
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}
