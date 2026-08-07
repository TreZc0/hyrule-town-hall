//! Per-language SpeedGaming schedule and volunteer exports.

use {
    lazy_regex::Regex,
    reqwest::header::{COOKIE, ORIGIN, REFERER, SET_COOKIE},
    tokio::time::sleep,
    crate::{
        cal::{Entrant, Entrants, Race, RaceSchedule},
        event::{self, roles::{Signup, VolunteerSignupStatus}},
        id::{Races, Signups},
        prelude::*,
        racetime_bot,
        series::Series,
        user::User,
        zsr_export,
    },
};

const BASE_URL: &str = "https://speedgaming.org";
const SCHEDULE_BATCH_SIZE: usize = 5;
// Keep the old, privately communicated schedule API limit until SpeedGaming confirms otherwise.
const SCHEDULE_BATCH_PAUSE: Duration = Duration::from_secs(60);
pub(crate) const LEGACY_IMPORT_ENABLED: bool = false;

pub(crate) static SYNC_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Calendar(#[from] cal::Error),
    #[error(transparent)] Event(#[from] event::DataError),
    #[error(transparent)] Http(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("SpeedGaming form did not contain {0}")] MissingFormField(&'static str),
    #[error("SpeedGaming rejected the {0} submission")] Rejected(&'static str),
    #[error("SpeedGaming rejected the {form} submission with HTTP {status}")]
    HttpRejected {
        form: &'static str,
        status: reqwest::StatusCode,
    },
    #[error("SpeedGaming returned an invalid episode ID")] InvalidEpisodeId,
    #[error("SpeedGaming may have accepted the submission: {0}")] AmbiguousSubmission(String),
    #[error("event not found")] EventNotFound,
    #[error("SpeedGaming exports only support 1v1 races")] NotOneVsOne,
    #[error("runner does not have a current Discord username")] MissingDiscordUsername,
    #[error("team does not have exactly one racing member")] InvalidTeam,
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        matches!(self, Self::Http(_)) || matches!(self, Self::Calendar(e) if e.is_network_error())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "speedgaming_export_trigger", rename_all = "snake_case")]
pub(crate) enum ExportTrigger {
    WhenScheduled,
    WhenRestreamChannelSet,
    WhenVolunteerSignedUp,
}

impl fmt::Display for ExportTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WhenScheduled => write!(f, "When Scheduled"),
            Self::WhenRestreamChannelSet => write!(f, "When Restream Channel Set"),
            Self::WhenVolunteerSignedUp => write!(f, "When Volunteer Signed Up"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "speedgaming_delivery_state", rename_all = "snake_case")]
pub(crate) enum DeliveryState {
    Pending,
    InProgress,
    Succeeded,
    Failed,
    Ambiguous,
}

#[derive(Debug, Clone)]
pub(crate) struct ExportConfig {
    pub(crate) id: i32,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) slug: String,
    pub(crate) trigger_condition: ExportTrigger,
    pub(crate) delay_minutes: i32,
    pub(crate) export_volunteers: bool,
    pub(crate) enabled: bool,
    pub(crate) volunteer_languages: Vec<Language>,
}

impl ExportConfig {
    pub(crate) async fn from_id(transaction: &mut Transaction<'_, Postgres>, id: i32) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"
            SELECT e.id, e.series AS "series: Series", e.event, e.slug,
                   e.trigger_condition AS "trigger_condition: ExportTrigger", e.delay_minutes,
                   e.export_volunteers, e.enabled,
                   ARRAY(SELECT language FROM speedgaming_export_languages WHERE export_id = e.id ORDER BY language)
                       AS "volunteer_languages!: Vec<Language>"
            FROM speedgaming_exports e
            WHERE e.id = $1 AND e.archived_at IS NULL
        "#, id)
        .fetch_optional(&mut **transaction)
        .await
    }

    pub(crate) async fn from_id_for_update(transaction: &mut Transaction<'_, Postgres>, id: i32) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"
            SELECT e.id, e.series AS "series: Series", e.event, e.slug,
                   e.trigger_condition AS "trigger_condition: ExportTrigger", e.delay_minutes,
                   e.export_volunteers, e.enabled,
                   ARRAY(SELECT language FROM speedgaming_export_languages WHERE export_id = e.id ORDER BY language)
                       AS "volunteer_languages!: Vec<Language>"
            FROM speedgaming_exports e
            WHERE e.id = $1 AND e.archived_at IS NULL
            FOR UPDATE
        "#, id)
        .fetch_optional(&mut **transaction)
        .await
    }

    pub(crate) async fn for_event(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(Self, r#"
            SELECT e.id, e.series AS "series: Series", e.event, e.slug,
                   e.trigger_condition AS "trigger_condition: ExportTrigger", e.delay_minutes,
                   e.export_volunteers, e.enabled,
                   ARRAY(SELECT language FROM speedgaming_export_languages WHERE export_id = e.id ORDER BY language)
                       AS "volunteer_languages!: Vec<Language>"
            FROM speedgaming_exports e
            WHERE e.series = $1 AND e.event = $2 AND e.archived_at IS NULL
        "#, series as _, event)
        .fetch_all(&mut **transaction)
        .await
    }

    pub(crate) async fn all_enabled(transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(Self, r#"
            SELECT e.id, e.series AS "series: Series", e.event, e.slug,
                   e.trigger_condition AS "trigger_condition: ExportTrigger", e.delay_minutes,
                   e.export_volunteers, e.enabled,
                   ARRAY(SELECT language FROM speedgaming_export_languages WHERE export_id = e.id ORDER BY language)
                       AS "volunteer_languages!: Vec<Language>"
            FROM speedgaming_exports e
            WHERE e.enabled = true AND e.archived_at IS NULL
            ORDER BY e.series, e.event
        "#)
        .fetch_all(&mut **transaction)
        .await
    }

    pub(crate) async fn create(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        slug: &str,
        trigger_condition: ExportTrigger,
        delay_minutes: i32,
        export_volunteers: bool,
        volunteer_languages: &[Language],
    ) -> sqlx::Result<i32> {
        let archived_id = sqlx::query_scalar!(r#"
            SELECT id FROM speedgaming_exports
            WHERE series = $1 AND event = $2 AND slug = $3
              AND archived_at IS NOT NULL
            ORDER BY archived_at DESC
            LIMIT 1
            FOR UPDATE
        "#, series as _, event, slug)
        .fetch_optional(&mut **transaction)
        .await?;
        let id = if let Some(id) = archived_id {
            sqlx::query!(r#"
                UPDATE speedgaming_exports SET
                    trigger_condition = $2, delay_minutes = $3, export_volunteers = $4,
                    enabled = true, archived_at = NULL, updated_at = NOW()
                WHERE id = $1
            "#, id, trigger_condition as _, delay_minutes, export_volunteers)
            .execute(&mut **transaction)
            .await?;
            id
        } else {
            sqlx::query_scalar!(r#"
                INSERT INTO speedgaming_exports
                    (series, event, slug, trigger_condition, delay_minutes, export_volunteers)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING id
            "#, series as _, event, slug, trigger_condition as _, delay_minutes, export_volunteers)
            .fetch_one(&mut **transaction)
            .await?
        };
        Self::set_volunteer_languages(transaction, id, volunteer_languages).await?;
        Ok(id)
    }

    pub(crate) async fn update(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
        slug: &str,
        trigger_condition: ExportTrigger,
        delay_minutes: i32,
        export_volunteers: bool,
        enabled: bool,
        volunteer_languages: &[Language],
    ) -> sqlx::Result<()> {
        sqlx::query!(r#"
            UPDATE speedgaming_exports SET
                slug = $2, trigger_condition = $3, delay_minutes = $4,
                export_volunteers = $5, enabled = $6, updated_at = NOW()
            WHERE id = $1 AND archived_at IS NULL
        "#, id, slug, trigger_condition as _, delay_minutes, export_volunteers, enabled)
        .execute(&mut **transaction)
        .await?;
        Self::set_volunteer_languages(transaction, id, volunteer_languages).await?;
        Ok(())
    }

    async fn set_volunteer_languages(
        transaction: &mut Transaction<'_, Postgres>,
        id: i32,
        volunteer_languages: &[Language],
    ) -> sqlx::Result<()> {
        sqlx::query!("DELETE FROM speedgaming_export_languages WHERE export_id = $1", id)
            .execute(&mut **transaction)
            .await?;
        for language in volunteer_languages.iter().copied().unique() {
            sqlx::query!("INSERT INTO speedgaming_export_languages (export_id, language) VALUES ($1, $2)", id, language as _)
                .execute(&mut **transaction)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn archive(transaction: &mut Transaction<'_, Postgres>, id: i32) -> sqlx::Result<()> {
        sqlx::query!(r#"
            UPDATE speedgaming_exports
            SET enabled = false, archived_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND archived_at IS NULL
        "#, id)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }
}

struct FormState {
    csrf: String,
    cookie: String,
    episode_id: Option<i64>,
}

fn input_value(html: &str, name: &'static str) -> Result<String, Error> {
    static INPUT_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<input\b[^>]*>").expect("valid regex"));
    static NAME_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)\bname\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).expect("valid regex"));
    static VALUE_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)\bvalue\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).expect("valid regex"));

    INPUT_TAG.find_iter(html).find_map(|tag| {
        let tag = tag.as_str();
        let attributes = NAME_ATTRIBUTE.captures(tag)?;
        let input_name = attributes.get(1).or_else(|| attributes.get(2)).or_else(|| attributes.get(3))?.as_str();
        if input_name != name {
            return None
        }
        let attributes = VALUE_ATTRIBUTE.captures(tag)?;
        Some(attributes.get(1).or_else(|| attributes.get(2)).or_else(|| attributes.get(3))?.as_str().to_owned())
    }).ok_or(Error::MissingFormField(name))
}

fn csrf_cookie(response: &reqwest::Response) -> Result<String, Error> {
    response.headers().get_all(SET_COOKIE).iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| value.split(';').find_map(|part| part.trim().strip_prefix("csrftoken=")))
        .map(|value| format!("csrftoken={value}"))
        .ok_or(Error::MissingFormField("csrftoken cookie"))
}

async fn get_form(http_client: &reqwest::Client, url: &str, expect_episode_id: bool) -> Result<FormState, Error> {
    let response = http_client.get(url).send().await?.error_for_status()?;
    let cookie = csrf_cookie(&response)?;
    let html = response.text().await?;
    let csrf = input_value(&html, "csrfmiddlewaretoken")?;
    let episode_id = expect_episode_id.then(|| input_value(&html, "episodeid"))
        .transpose()?
        .map(|value| value.parse().map_err(|_| Error::InvalidEpisodeId))
        .transpose()?;
    Ok(FormState { csrf, cookie, episode_id })
}

#[derive(Debug)]
struct RunnerIdentity {
    discord_username: Option<String>,
    display_name: String,
    twitch_name: Option<String>,
}

async fn user_identity(http_client: &reqwest::Client, user: &User) -> Result<RunnerIdentity, Error> {
    let twitch_name = user.racetime_user_data(http_client).await?
        .flatten()
        .and_then(|profile| profile.twitch_name);
    let discord_username = user.discord.as_ref().and_then(|discord| {
        discord.username_or_discriminator.as_ref().left().cloned()
    });
    Ok(RunnerIdentity {
        discord_username,
        display_name: user.display_name().to_owned(),
        twitch_name,
    })
}

async fn runner_identity(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    event_data: &event::Data<'_>,
    entrant: &Entrant,
) -> Result<RunnerIdentity, Error> {
    match entrant {
        Entrant::MidosHouseTeam(team) => {
            let user = team.members_roles(transaction).await?.into_iter()
                .filter(|(_, role)| event_data.team_config.role_is_racing(*role))
                .map(|(user, _)| user)
                .exactly_one()
                .map_err(|_| Error::InvalidTeam)?;
            user_identity(http_client, &user).await
        }
        Entrant::Discord { id, racetime_id, twitch_username } => {
            let user = User::from_discord(&mut **transaction, *id).await?;
            let mut identity = if let Some(user) = user {
                user_identity(http_client, &user).await?
            } else {
                RunnerIdentity { discord_username: None, display_name: id.to_string(), twitch_name: None }
            };
            if identity.twitch_name.is_none() {
                identity.twitch_name = if let Some(twitch_username) = twitch_username {
                    Some(twitch_username.clone())
                } else if let Some(racetime_id) = racetime_id {
                    racetime_bot::user_data(http_client, racetime_id).await?.and_then(|profile| profile.twitch_name)
                } else {
                    None
                };
            }
            Ok(identity)
        }
        Entrant::Named { name, racetime_id, twitch_username } => {
            let twitch_name = if let Some(twitch_username) = twitch_username {
                Some(twitch_username.clone())
            } else if let Some(racetime_id) = racetime_id {
                racetime_bot::user_data(http_client, racetime_id).await?.and_then(|profile| profile.twitch_name)
            } else {
                None
            };
            Ok(RunnerIdentity {
                discord_username: None,
                display_name: name.clone(),
                twitch_name,
            })
        }
    }
}

struct MatchSubmission {
    slug: String,
    runner1: RunnerIdentity,
    runner2: RunnerIdentity,
    start: DateTime<Utc>,
    note: String,
}

fn format_race_note(round: Option<&str>, game: Option<i16>) -> String {
    round.into_iter().map(|round| {
        if round.chars().all(|character| character.is_ascii_digit()) {
            format!("Round {round}")
        } else {
            round.to_owned()
        }
    })
        .chain(iter::once(format!("Game {}", game.unwrap_or(1))))
        .join(" ")
}

fn race_note(race: &Race) -> String {
    format_race_note(race.round.as_deref(), race.game)
}

fn confirmation_episode_id(html: &str) -> Result<i64, Error> {
    let (_, episode_id) = regex_captures!(r"Episode ID:\s*([0-9]+)", html).ok_or(Error::InvalidEpisodeId)?;
    episode_id.parse().map_err(|_| Error::InvalidEpisodeId)
}

fn speedgaming_form_time(start: DateTime<Utc>) -> (String, String, String) {
    let start = start.with_timezone(&America::New_York);
    (
        start.format("%m/%d/%Y").to_string(),
        start.format("%I:%M").to_string(),
        start.format("%P").to_string(),
    )
}

async fn build_match_submission(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    race: &Race,
    export: &ExportConfig,
    event_data: &event::Data<'_>,
) -> Result<MatchSubmission, Error> {
    let Entrants::Two(entrants) = &race.entrants else { return Err(Error::NotOneVsOne) };
    let mut runner1 = runner_identity(transaction, http_client, event_data, &entrants[0]).await?;
    let mut runner2 = runner_identity(transaction, http_client, event_data, &entrants[1]).await?;
    if runner1.discord_username.is_none() && runner2.discord_username.is_some() {
        mem::swap(&mut runner1, &mut runner2);
    }
    if runner1.discord_username.is_none() {
        return Err(Error::MissingDiscordUsername)
    }
    let RaceSchedule::Live { start, .. } = race.schedule else { return Err(Error::NotOneVsOne) };
    Ok(MatchSubmission {
        slug: export.slug.clone(),
        runner1,
        runner2,
        start: start + TimeDelta::minutes(export.delay_minutes.into()),
        note: race_note(race),
    })
}

async fn submit_match(http_client: &reqwest::Client, submission: &MatchSubmission) -> Result<i64, Error> {
    let url = format!("{BASE_URL}/{}/submit/", submission.slug);
    let form = get_form(http_client, &url, false).await?;
    let discord_username = submission.runner1.discord_username.as_deref().ok_or(Error::MissingDiscordUsername)?;
    let (date, time, am_pm) = speedgaming_form_time(submission.start);
    let fields = [
        ("csrfmiddlewaretoken", form.csrf),
        ("eventslug", submission.slug.clone()),
        ("person1id", "0".to_owned()),
        ("discordtag1", discord_username.to_owned()),
        ("displayname1", submission.runner1.display_name.clone()),
        ("publicstream1", submission.runner1.twitch_name.clone().unwrap_or_default()),
        ("person2id", "0".to_owned()),
        ("displayname2", submission.runner2.display_name.clone()),
        ("whendate", date),
        ("whentime", time),
        ("whenampm", am_pm),
        ("whentimezone", String::new()),
        ("note", submission.note.clone()),
        ("submit", "Submit Match".to_owned()),
    ];
    let response = http_client.post(&url).header(COOKIE, form.cookie).form(&fields).send().await
        .map_err(|error| Error::AmbiguousSubmission(error.to_string()))?
        .error_for_status()
        .map_err(|error| Error::AmbiguousSubmission(error.to_string()))?;
    let html = response.text().await.map_err(|error| Error::AmbiguousSubmission(error.to_string()))?;
    if !html.contains("Match Submission Confirmed") {
        return Err(Error::Rejected("match"))
    }
    confirmation_episode_id(&html)
}

async fn should_export_race(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    export: &ExportConfig,
) -> Result<bool, Error> {
    let RaceSchedule::Live { start, .. } = race.schedule else { return Ok(false) };
    if race.ignored || start <= Utc::now() {
        return Ok(false)
    }
    let entrant_consent = race.teams_opt().map(|mut teams| teams.all(|team| team.restream_consent));
    if !restream_consent_allows_export(race.restream_consent_required, entrant_consent) {
        return Ok(false)
    }
    match export.trigger_condition {
        ExportTrigger::WhenScheduled => Ok(true),
        ExportTrigger::WhenRestreamChannelSet => Ok(!race.video_urls.is_empty()),
        ExportTrigger::WhenVolunteerSignedUp => Ok(Signup::for_race(transaction, race.id).await?.iter().any(|signup| {
            export.volunteer_languages.contains(&signup.language)
                && matches!(signup.status, VolunteerSignupStatus::Pending | VolunteerSignupStatus::Confirmed)
        })),
    }
}

fn restream_consent_allows_export(forced_consent: bool, entrant_consent: Option<bool>) -> bool {
    forced_consent || entrant_consent == Some(true)
}

async fn claim_race_export(
    transaction: &mut Transaction<'_, Postgres>,
    race_id: Id<Races>,
    export_id: i32,
) -> sqlx::Result<bool> {
    let enabled = sqlx::query_scalar!("SELECT enabled FROM speedgaming_exports WHERE id = $1 AND archived_at IS NULL FOR KEY SHARE", export_id)
        .fetch_optional(&mut **transaction)
        .await?;
    if enabled != Some(true) {
        return Ok(false)
    }
    Ok(sqlx::query_scalar!(r#"
        INSERT INTO speedgaming_race_exports (race_id, export_id, state, attempt_count, last_attempt_at)
        VALUES ($1, $2, 'in_progress', 1, NOW())
        ON CONFLICT (race_id, export_id) DO UPDATE SET
            state = 'in_progress', attempt_count = speedgaming_race_exports.attempt_count + 1,
            last_attempt_at = NOW(), last_error = NULL
        WHERE speedgaming_race_exports.state IN ('pending', 'failed')
        RETURNING true AS "claimed!"
    "#, race_id as _, export_id)
    .fetch_optional(&mut **transaction)
    .await?
    .unwrap_or(false))
}

async fn record_race_failure(pool: &PgPool, race_id: Id<Races>, export_id: i32, error: &Error) -> sqlx::Result<()> {
    let state = if matches!(error, Error::AmbiguousSubmission(_) | Error::InvalidEpisodeId) { DeliveryState::Ambiguous } else { DeliveryState::Failed };
    sqlx::query!(r#"
        UPDATE speedgaming_race_exports SET state = $3, last_error = $4
        WHERE race_id = $1 AND export_id = $2
    "#, race_id as _, export_id, state as _, error.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

async fn sync_races_for_export(pool: &PgPool, http_client: &reqwest::Client, export: &ExportConfig) -> Result<(), Error> {
    let race_ids = sqlx::query_scalar!(r#"
        SELECT id AS "id: Id<Races>" FROM races
        WHERE series = $1 AND event = $2 AND ignored = false AND start > NOW()
        ORDER BY start, id
    "#, export.series as _, &export.event)
    .fetch_all(pool)
    .await?;

    for race_id in race_ids {
        let (submission, claimed) = {
            let mut transaction = pool.begin().await?;
            let race = Race::from_id(&mut transaction, http_client, race_id).await?;
            if !should_export_race(&mut transaction, &race, export).await? {
                transaction.rollback().await?;
                continue
            }
            let event_data = event::Data::new(&mut transaction, export.series, &export.event).await?.ok_or(Error::EventNotFound)?;
            let submission = build_match_submission(&mut transaction, http_client, &race, export, &event_data).await;
            let claimed = claim_race_export(&mut transaction, race_id, export.id).await?;
            transaction.commit().await?;
            (submission, claimed)
        };
        if !claimed {
            continue
        }
        match submission {
            Ok(submission) => match submit_match(http_client, &submission).await {
                Ok(episode_id) => {
                    sqlx::query!(r#"
                        UPDATE speedgaming_race_exports SET state = 'succeeded', episode_id = $3,
                            exported_at = NOW(), last_error = NULL
                        WHERE race_id = $1 AND export_id = $2
                    "#, race_id as _, export.id, episode_id)
                    .execute(pool)
                    .await?;
                }
                Err(error) => record_race_failure(pool, race_id, export.id, &error).await?,
            },
            Err(error) => record_race_failure(pool, race_id, export.id, &error).await?,
        }
    }
    Ok(())
}

async fn claim_volunteer_export(
    transaction: &mut Transaction<'_, Postgres>,
    signup_id: Id<Signups>,
    export_id: i32,
) -> sqlx::Result<bool> {
    let enabled = sqlx::query_scalar!("SELECT enabled FROM speedgaming_exports WHERE id = $1 AND archived_at IS NULL FOR KEY SHARE", export_id)
        .fetch_optional(&mut **transaction)
        .await?;
    if enabled != Some(true) {
        return Ok(false)
    }
    Ok(sqlx::query_scalar!(r#"
        INSERT INTO speedgaming_volunteer_exports (signup_id, export_id, state, attempt_count, last_attempt_at)
        VALUES ($1, $2, 'in_progress', 1, NOW())
        ON CONFLICT (signup_id, export_id) DO UPDATE SET
            state = 'in_progress', attempt_count = speedgaming_volunteer_exports.attempt_count + 1,
            last_attempt_at = NOW(), last_error = NULL
        WHERE speedgaming_volunteer_exports.state IN ('pending', 'failed')
           OR (speedgaming_volunteer_exports.state = 'ambiguous'
               AND speedgaming_volunteer_exports.last_error LIKE '%403 Forbidden%')
        RETURNING true AS "claimed!"
    "#, signup_id as _, export_id)
    .fetch_optional(&mut **transaction)
    .await?
    .unwrap_or(false))
}

async fn submit_volunteer(
    http_client: &reqwest::Client,
    episode_id: i64,
    language: Language,
    role_type_name: &str,
    discord_username: &str,
    display_name: &str,
) -> Result<(), Error> {
    let (path, success_marker) = match role_type_name {
        "Commentary" => ("commentator", "Commentator Signup Submitted"),
        "Tracking" => ("tracker", "Tracker Signup Submitted"),
        _ => return Err(Error::Rejected("unsupported volunteer role")),
    };
    let url = volunteer_signup_url(language, path, episode_id);
    let form = get_form(http_client, &url, true).await?;
    if form.episode_id != Some(episode_id) {
        return Err(Error::InvalidEpisodeId)
    }
    let fields = [
        ("csrfmiddlewaretoken", form.csrf),
        ("episodeid", episode_id.to_string()),
        ("personid", "0".to_owned()),
        ("discordtag", discord_username.to_owned()),
        ("displayname", display_name.to_owned()),
        ("publicstream", String::new()),
        ("submit", "Submit New/Updated Info".to_owned()),
    ];
    let response = http_client.post(&url)
        .header(COOKIE, form.cookie)
        .header(ORIGIN, BASE_URL)
        .header(REFERER, &url)
        .form(&fields)
        .send().await
        .map_err(|error| Error::AmbiguousSubmission(error.to_string()))?;
    if response.status().is_client_error() {
        return Err(Error::HttpRejected { form: "volunteer", status: response.status() })
    }
    let response = response.error_for_status()
        .map_err(|error| Error::AmbiguousSubmission(error.to_string()))?;
    let html = response.text().await.map_err(|error| Error::AmbiguousSubmission(error.to_string()))?;
    if !html.contains(success_marker) {
        return Err(Error::Rejected("volunteer"))
    }
    Ok(())
}

fn volunteer_signup_url(language: Language, role_path: &str, episode_id: i64) -> String {
    format!("{BASE_URL}/{}/{role_path}/signup/{episode_id}/", language.short_code())
}

async fn sync_volunteers_for_export(pool: &PgPool, http_client: &reqwest::Client, export: &ExportConfig) -> Result<(), Error> {
    if !export.export_volunteers {
        return Ok(())
    }
    let candidates = sqlx::query!(r#"
        SELECT s.id AS "signup_id: Id<Signups>", s.user_id AS "user_id: crate::id::Id<crate::id::Users>",
               rt.name AS role_type_name, rb.language AS "language: Language", re.episode_id AS "episode_id!"
        FROM signups s
        JOIN role_bindings rb ON rb.id = s.role_binding_id
        JOIN role_types rt ON rt.id = rb.role_type_id
        JOIN speedgaming_race_exports re ON re.race_id = s.race_id AND re.export_id = $1
        WHERE re.state = 'succeeded' AND rb.language = ANY($2)
          AND s.status IN ('pending', 'confirmed') AND rt.name IN ('Commentary', 'Tracking')
        ORDER BY s.created_at, s.id
    "#, export.id, &export.volunteer_languages as _)
    .fetch_all(pool)
    .await?;

    for candidate in candidates {
        let (claimed, identity) = {
            let mut transaction = pool.begin().await?;
            let claimed = claim_volunteer_export(&mut transaction, candidate.signup_id, export.id).await?;
            let identity = match User::from_id(&mut *transaction, candidate.user_id).await? {
                Some(User { discord: Some(discord), .. }) => match discord.username_or_discriminator.left() {
                    Some(discord_username) => Ok((discord_username, discord.display_name)),
                    None => Err(Error::MissingDiscordUsername),
                },
                _ => Err(Error::MissingDiscordUsername),
            };
            transaction.commit().await?;
            (claimed, identity)
        };
        if !claimed {
            continue
        }
        let (discord_username, display_name) = match identity {
            Ok(identity) => identity,
            Err(error) => {
                sqlx::query!(r#"
                    UPDATE speedgaming_volunteer_exports SET state = 'failed', last_error = $3
                    WHERE signup_id = $1 AND export_id = $2
                "#, candidate.signup_id as _, export.id, error.to_string())
                .execute(pool)
                .await?;
                eprintln!("SpeedGaming volunteer export for signup {} failed: {error}", candidate.signup_id);
                continue
            }
        };
        let result = submit_volunteer(
            http_client,
            candidate.episode_id,
            candidate.language,
            &candidate.role_type_name,
            &discord_username,
            &display_name,
        ).await;
        match result {
            Ok(()) => {
                sqlx::query!(r#"
                    UPDATE speedgaming_volunteer_exports SET state = 'succeeded', submitted_at = NOW(), last_error = NULL
                    WHERE signup_id = $1 AND export_id = $2
                "#, candidate.signup_id as _, export.id)
                .execute(pool)
                .await?;
            }
            Err(error) => {
                let state = if matches!(error, Error::AmbiguousSubmission(_)) { DeliveryState::Ambiguous } else { DeliveryState::Failed };
                sqlx::query!(r#"
                    UPDATE speedgaming_volunteer_exports SET state = $3, last_error = $4
                    WHERE signup_id = $1 AND export_id = $2
                "#, candidate.signup_id as _, export.id, state as _, error.to_string())
                .execute(pool)
                .await?;
                eprintln!("SpeedGaming volunteer export for signup {} failed: {error}", candidate.signup_id);
            }
        }
    }
    Ok(())
}

pub(crate) async fn sync_export(pool: &PgPool, http_client: &reqwest::Client, export: &ExportConfig) -> Result<(), Error> {
    sync_races_for_export(pool, http_client, export).await?;
    sync_volunteers_for_export(pool, http_client, export).await?;
    Ok(())
}

async fn sync_outbound_exports(pool: &PgPool, http_client: &reqwest::Client) -> Result<Vec<ExportConfig>, Error> {
    sqlx::query!(r#"
        UPDATE speedgaming_race_exports SET state = 'ambiguous', last_error = 'export process stopped during submission'
        WHERE state = 'in_progress' AND last_attempt_at < NOW() - INTERVAL '15 minutes'
    "#).execute(pool).await?;
    sqlx::query!(r#"
        UPDATE speedgaming_volunteer_exports SET state = 'ambiguous', last_error = 'export process stopped during submission'
        WHERE state = 'in_progress' AND last_attempt_at < NOW() - INTERVAL '15 minutes'
    "#).execute(pool).await?;
    let exports = {
        let mut transaction = pool.begin().await?;
        let exports = ExportConfig::all_enabled(&mut transaction).await?;
        transaction.commit().await?;
        exports
    };
    for export in &exports {
        if let Err(error) = sync_export(pool, http_client, export).await {
            eprintln!("SpeedGaming export {}/{} failed: {error}", export.series.slug(), export.event);
        }
    }
    Ok(exports)
}

pub(crate) async fn check_and_sync_all_exports(pool: &PgPool, http_client: &reqwest::Client) -> Result<(), Error> {
    let Ok(_guard) = SYNC_LOCK.try_lock() else { return Ok(()) };
    let exports = sync_outbound_exports(pool, http_client).await?;
    poll_all_exports(pool, http_client, &exports).await?;
    Ok(())
}

pub(crate) fn schedule_sync(pool: PgPool, http_client: reqwest::Client) {
    tokio::spawn(async move {
        let _guard = SYNC_LOCK.lock().await;
        if let Err(error) = sync_outbound_exports(&pool, &http_client).await {
            eprintln!("SpeedGaming export sync failed: {error}");
        }
    });
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleVolunteer {
    #[serde(default)]
    approved: bool,
    #[serde(default)]
    discord_id: String,
    #[serde(default)]
    discord_tag: String,
    language: String,
}

#[derive(Clone, Deserialize)]
struct ScheduleChannel {
    language: String,
    slug: String,
}

#[derive(Clone, Deserialize)]
struct ScheduleEpisode {
    id: i64,
    commentators: Vec<ScheduleVolunteer>,
    trackers: Vec<ScheduleVolunteer>,
    channels: Vec<ScheduleChannel>,
}

fn poll_window(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    (now - TimeDelta::hours(3), now + TimeDelta::hours(24))
}

/// Heuristic for whether a `video_urls` entry was set by a previous SpeedGaming poll (and is
/// therefore safe for a later poll to update or clear) rather than by an organizer.
fn looks_speedgaming_owned(url: &Url) -> bool {
    url.as_str().to_lowercase().contains("twitch.tv/speedgaming")
}

async fn poll_export(pool: &PgPool, http_client: &reqwest::Client, export: &ExportConfig) -> Result<(), Error> {
    let (from, to) = poll_window(Utc::now());
    let delay = TimeDelta::minutes(export.delay_minutes.into());
    let has_races = sqlx::query_scalar!(r#"
        SELECT EXISTS (
            SELECT 1
            FROM speedgaming_race_exports re
            JOIN races r ON r.id = re.race_id
            WHERE re.export_id = $1 AND re.state = 'succeeded'
              AND r.start >= $2 AND r.start <= $3
        ) AS "exists!"
    "#, export.id, from - delay, to - delay)
    .fetch_one(pool)
    .await?;
    if !has_races {
        return Ok(())
    }
    let episodes = http_client.get(format!("{BASE_URL}/api/schedule/"))
        .query(&[
            ("event", export.slug.clone()),
            ("from", from.to_rfc3339()),
            ("to", to.to_rfc3339()),
        ])
        .send().await?
        .error_for_status()?
        .json::<Vec<ScheduleEpisode>>().await?;

    for episode in episodes {
        let race_id = sqlx::query_scalar!(r#"
            SELECT race_id AS "race_id: Id<Races>" FROM speedgaming_race_exports
            WHERE export_id = $1 AND episode_id = $2
        "#, export.id, episode.id)
        .fetch_optional(pool)
        .await?;
        let Some(race_id) = race_id else { continue };
        let approved = episode.commentators.iter().map(|volunteer| ("Commentary", volunteer))
            .chain(episode.trackers.iter().map(|volunteer| ("Tracking", volunteer)))
            .filter_map(|(role_type_name, volunteer)| {
                export.volunteer_languages.iter().copied()
                    .find(|language| language.short_code() == volunteer.language)
                    .filter(|_| volunteer.approved)
                    .map(|language| (role_type_name, volunteer, language))
            })
            .collect_vec();
        let mut transaction = pool.begin().await?;
        for (role_type_name, volunteer, language) in approved {
            let discord_id = volunteer.discord_id.parse::<i64>().ok();
            let confirmed = sqlx::query!(r#"
                UPDATE signups s SET status = 'confirmed', updated_at = NOW()
                FROM role_bindings rb, role_types rt, users u
                WHERE s.race_id = $1 AND s.role_binding_id = rb.id AND rb.role_type_id = rt.id
                  AND s.user_id = u.id AND s.status = 'pending' AND rb.language = $2 AND rt.name = $3
                  AND (($4::BIGINT IS NOT NULL AND u.discord_id = $4)
                       OR LOWER(u.discord_username) = LOWER($5))
                RETURNING s.id AS "signup_id: Id<Signups>", s.user_id AS "user_id: crate::id::Id<crate::id::Users>"
            "#, race_id as _, language as _, role_type_name, discord_id, &volunteer.discord_tag)
            .fetch_all(&mut *transaction)
            .await?;
            for signup in confirmed {
                Signup::auto_reject_overlapping_signups(&mut transaction, signup.signup_id, signup.user_id).await?;
            }
        }
        let mut race = Race::from_id(&mut transaction, http_client, race_id).await?;
        let mut changed = false;
        for language in export.volunteer_languages.iter().copied() {
            let channel = episode.channels.iter().find(|channel| channel.language == language.short_code());
            match (channel, race.video_urls.get(&language)) {
                (Some(channel), existing) => {
                    let new_url = Url::parse(&format!("https://twitch.tv/{}", channel.slug))?;
                    if existing.is_none_or(looks_speedgaming_owned) && existing != Some(&new_url) {
                        race.video_urls.insert(language, new_url);
                        changed = true;
                    }
                }
                (None, Some(existing)) if looks_speedgaming_owned(existing) => {
                    race.video_urls.remove(&language);
                    changed = true;
                }
                (None, _) => {}
            }
        }
        if changed {
            race.save(&mut transaction).await?;
        }
        sqlx::query!(r#"
            UPDATE speedgaming_race_exports SET last_polled_at = NOW()
            WHERE race_id = $1 AND export_id = $2
        "#, race_id as _, export.id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        zsr_export::schedule_volunteer_api_call(pool.clone(), http_client.clone(), race_id);
    }
    Ok(())
}

async fn poll_all_exports(pool: &PgPool, http_client: &reqwest::Client, exports: &[ExportConfig]) -> Result<(), Error> {
    let exports = exports.to_vec();
    let batch_count = exports.len().div_ceil(SCHEDULE_BATCH_SIZE);
    for (batch_index, batch) in exports.chunks(SCHEDULE_BATCH_SIZE).enumerate() {
        for (export, result) in batch.iter().zip(future::join_all(batch.iter().map(|export| poll_export(pool, http_client, export))).await) {
            if let Err(error) = result {
                eprintln!("SpeedGaming status poll {}/{} failed: {error}", export.series.slug(), export.event);
            }
        }
        if batch_index + 1 < batch_count {
            sleep(SCHEDULE_BATCH_PAUSE).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_csrf_shapes() {
        assert_eq!(input_value(r#"<input type='hidden' name='csrfmiddlewaretoken' value='abc' />"#, "csrfmiddlewaretoken").unwrap(), "abc");
        assert_eq!(input_value(r#"<input name="csrfmiddlewaretoken" value="def">"#, "csrfmiddlewaretoken").unwrap(), "def");
        assert_eq!(input_value(r#"<INPUT value="ghi" type="hidden" name="csrfmiddlewaretoken">"#, "csrfmiddlewaretoken").unwrap(), "ghi");
        assert_eq!(input_value(r#"<input name="other" value="wrong"><input name="episodeid" value="74585">"#, "episodeid").unwrap(), "74585");
    }

    #[test]
    fn parses_episode_id_from_confirmation() {
        let html = "<h1>Match Submission Confirmed</h1> Episode ID: 74585<br/>";
        assert_eq!(confirmation_episode_id(html).unwrap(), 74585);
    }

    #[test]
    fn formats_round_and_game_note() {
        assert_eq!(format_race_note(Some("1"), Some(2)), "Round 1 Game 2");
        assert_eq!(format_race_note(Some("Grand Finals"), None), "Grand Finals Game 1");
        assert_eq!(format_race_note(None, None), "Game 1");
    }

    #[test]
    fn converts_speedgaming_form_time_to_eastern_time() {
        let winter = Utc.with_ymd_and_hms(2026, 1, 15, 18, 30, 0).unwrap();
        assert_eq!(speedgaming_form_time(winter), ("01/15/2026".to_owned(), "01:30".to_owned(), "pm".to_owned()));

        let summer = Utc.with_ymd_and_hms(2026, 7, 15, 18, 30, 0).unwrap();
        assert_eq!(speedgaming_form_time(summer), ("07/15/2026".to_owned(), "02:30".to_owned(), "pm".to_owned()));
    }

    #[test]
    fn requires_restream_consent_before_export() {
        assert!(restream_consent_allows_export(false, Some(true)));
        assert!(!restream_consent_allows_export(false, Some(false)));
        assert!(!restream_consent_allows_export(false, None));
        assert!(restream_consent_allows_export(true, Some(false)));
        assert!(restream_consent_allows_export(true, None));
    }

    #[test]
    fn limits_schedule_poll_to_three_hours_ago_and_the_next_24_hours() {
        let now = Utc.with_ymd_and_hms(2026, 8, 4, 15, 30, 0).unwrap();
        assert_eq!(
            poll_window(now),
            (
                Utc.with_ymd_and_hms(2026, 8, 4, 12, 30, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 8, 5, 15, 30, 0).unwrap(),
            ),
        );
    }

    #[test]
    fn localizes_volunteer_signup_urls() {
        assert_eq!(
            volunteer_signup_url(German, "commentator", 74597),
            "https://speedgaming.org/de/commentator/signup/74597/",
        );
        assert_eq!(
            volunteer_signup_url(French, "tracker", 74597),
            "https://speedgaming.org/fr/tracker/signup/74597/",
        );
    }

    #[test]
    fn accepts_unconfigured_speedgaming_languages_in_schedule_response() {
        let episode: ScheduleEpisode = serde_json::from_str(r#"{
            "id": 74597,
            "commentators": [{"language": "es", "approved": true}],
            "trackers": [],
            "channels": [{"language": "es", "slug": "speedgaminges"}]
        }"#).unwrap();
        assert_eq!(episode.commentators[0].language, "es");
        assert_eq!(episode.channels[0].language, "es");
    }
}
