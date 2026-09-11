//! Reconciliation using existing export records, which survive local deletion.
use super::*;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use sqlx::Row as _;
use std::sync::OnceLock;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static SCHEDULE_REQUESTS: LazyLock<tokio::sync::Mutex<Vec<Instant>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(Vec::new()));

pub(super) async fn reserve_schedule_request() {
    let mut recent = SCHEDULE_REQUESTS.lock().await;
    recent.retain(|sent| sent.elapsed() < SCHEDULE_BATCH_PAUSE);
    if recent.len() >= SCHEDULE_BATCH_SIZE {
        sleep_until(recent[0] + SCHEDULE_BATCH_PAUSE).await;
        recent.retain(|sent| sent.elapsed() < SCHEDULE_BATCH_PAUSE);
    }
    recent.push(Instant::now());
}

fn authorization(username: &str, password: &str) -> Result<HeaderValue, Error> {
    if username.is_empty() || password.is_empty() {
        return Err(Error::Credentials);
    }
    // SpeedGaming explicitly expects the literal pair, without Basic or Base64.
    let mut value =
        HeaderValue::from_str(&format!("{username}:{password}")).map_err(|_| Error::Credentials)?;
    value.set_sensitive(true);
    Ok(value)
}

pub(crate) fn initialize(config: Option<&crate::config::ConfigSpeedGaming>) -> Result<(), Error> {
    if let Some(config) = config {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            authorization(&config.username, &config.password)?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .user_agent("HyruleTownHall SpeedGaming integration")
            .build()?;
        let _ = CLIENT.set(client);
    }
    Ok(())
}

pub(crate) fn configured() -> bool {
    CLIENT.get().is_some()
}

pub(super) fn client() -> Result<&'static reqwest::Client, Error> {
    CLIENT.get().ok_or(Error::Credentials)
}

#[derive(sqlx::FromRow)]
struct RemoteRace {
    race_id: i64,
    export_id: i32,
    slug: String,
    episode_id: Option<i64>,
    match_id: Option<i64>,
    synced_start: Option<DateTime<Utc>>,
    state: String,
}

pub(super) async fn prepare_create(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    export: &ExportConfig,
    start: DateTime<Utc>,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE speedgaming_race_exports SET synced_start=$3, operation='create'
        WHERE race_id=$1 AND export_id=$2",
    )
    .bind(i64::from(race.id))
    .bind(export.id)
    .bind(start)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
struct RemoteMatch {
    id: i64,
}

#[derive(Deserialize)]
struct Episode {
    id: i64,
    when: DateTime<Utc>,
    match1: Option<RemoteMatch>,
    match2: Option<RemoteMatch>,
    #[serde(default)]
    commentators: Vec<Volunteer>,
    #[serde(default)]
    trackers: Vec<Volunteer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Volunteer {
    id: i64,
    language: String,
    #[serde(default)]
    discord_tag: String,
}

async fn lookup_episode(
    remote: &RemoteRace,
    desired: Option<DateTime<Utc>>,
) -> Result<Option<Episode>, Error> {
    let old = remote
        .synced_start
        .ok_or_else(|| Error::Attention("Missing previous schedule time".into()))?;
    let new = desired.unwrap_or(old);
    reserve_schedule_request().await;
    let response = client()?
        .get(format!("{BASE_URL}/api/schedule/"))
        .query(&[
            ("event", remote.slug.clone()),
            ("from", (old.min(new) - TimeDelta::days(2)).to_rfc3339()),
            ("to", (old.max(new) + TimeDelta::days(2)).to_rfc3339()),
        ])
        .send()
        .await?
        .error_for_status()?;
    let episodes: Vec<Episode> = response.json().await?;
    Ok(episodes
        .into_iter()
        .find(|e| Some(e.id) == remote.episode_id))
}

async fn episode(remote: &RemoteRace, desired: Option<DateTime<Utc>>) -> Result<Episode, Error> {
    lookup_episode(remote, desired).await?.ok_or_else(|| {
        Error::Attention(
            "Episode absent from schedule window; inspect its location/approval before retrying"
                .into(),
        )
    })
}

fn single_match(episode: &Episode, expected: Option<i64>) -> Result<i64, Error> {
    match (&episode.match1, &episode.match2) {
        (Some(m), None) if expected.is_none_or(|id| id == m.id) => Ok(m.id),
        _ => Err(Error::Attention(
            "Combined episode or changed match slot; organizer action required".into(),
        )),
    }
}

fn forms(html: &str) -> impl Iterator<Item = &str> {
    static FORMS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<form\b[^>]*>.*?</form>").unwrap());
    FORMS.find_iter(html).map(|m| m.as_str())
}

fn action_form<'a>(html: &'a str, episode: i64, action: &str) -> Option<&'a str> {
    forms(html).find(|f| {
        input_value(f, "episodeid").ok().as_deref() == Some(&episode.to_string())
            && f.contains(&format!("name=\"{action}\""))
    })
}

struct CrewPage {
    url: String,
    html: String,
    cookie: String,
}

async fn post_page(page: &CrewPage, fields: &[(String, String)]) -> Result<CrewPage, Error> {
    let response = client()?
        .post(&page.url)
        .header(COOKIE, &page.cookie)
        .header(ORIGIN, BASE_URL)
        .header(REFERER, &page.url)
        .form(fields)
        .send()
        .await
        .map_err(|e| Error::AmbiguousSubmission(e.to_string()))?;
    reject_client_error("crew", response.status())?;
    if response.status().is_redirection() {
        return Err(Error::Credentials);
    }
    let response = response
        .error_for_status()
        .map_err(|e| Error::AmbiguousSubmission(e.to_string()))?;
    let cookie = csrf_cookie(&response).unwrap_or_else(|_| page.cookie.clone());
    let html = response
        .text()
        .await
        .map_err(|e| Error::AmbiguousSubmission(e.to_string()))?;
    Ok(CrewPage {
        url: page.url.clone(),
        html,
        cookie,
    })
}

fn hidden_fields(form: &str) -> Result<Vec<(String, String)>, Error> {
    Ok(vec![
        (
            "csrfmiddlewaretoken".into(),
            input_value(form, "csrfmiddlewaretoken")?,
        ),
        ("page".into(), input_value(form, "page")?),
    ])
}

async fn crew_page(remote: &RemoteRace, action: &str) -> Result<CrewPage, Error> {
    let url = format!("{BASE_URL}/{}/", remote.slug);
    let response = client()?.get(&url).send().await?.error_for_status()?;
    if response.status().is_redirection() {
        return Err(Error::Credentials);
    }
    let cookie = csrf_cookie(&response)?;
    let html = response.text().await?;
    let id = remote.episode_id.ok_or(Error::InvalidEpisodeId)?;
    if action_form(&html, id, action).is_some() {
        return Ok(CrewPage { url, html, cookie });
    }
    // Paging uses the site's own POST controls. Never infer absence from one page.
    for direction in ["later", "earlier"] {
        let mut page = CrewPage {
            url: url.clone(),
            html: html.clone(),
            cookie: cookie.clone(),
        };
        let mut seen = HashSet::new();
        for _ in 0..20 {
            let Some(form) =
                forms(&page.html).find(|f| f.contains(&format!("name=\"{direction}\"")))
            else {
                break;
            };
            let mut fields = hidden_fields(form)?;
            if !seen.insert(fields[1].1.clone()) {
                break;
            }
            fields.push((
                direction.into(),
                if direction == "later" {
                    "See later matches"
                } else {
                    "See earlier matches"
                }
                .into(),
            ));
            page = post_page(&page, &fields).await?;
            if action_form(&page.html, id, action).is_some() {
                return Ok(page);
            }
        }
    }
    Err(Error::Attention(
        "Crew action not found; check event permissions and pagination".into(),
    ))
}

async fn mutate(
    page: CrewPage,
    remote: &RemoteRace,
    action: &str,
    value: &str,
    extra: Vec<(String, String)>,
) -> Result<CrewPage, Error> {
    let form = action_form(&page.html, remote.episode_id.unwrap(), action)
        .ok_or(Error::InvalidEpisodeId)?;
    let mut fields = hidden_fields(form)?;
    fields.push(("episodeid".into(), remote.episode_id.unwrap().to_string()));
    if let Ok(confirm) = input_value(form, "confirm-delete-episode") {
        fields.push(("confirm-delete-episode".into(), confirm));
    }
    fields.extend(extra);
    fields.push((action.into(), value.into()));
    post_page(&page, &fields).await
}

fn crew_time(time: DateTime<Utc>) -> String {
    time.with_timezone(&America::New_York)
        .format("%b%d %Y %-I:%M%P")
        .to_string()
        .to_lowercase()
}

pub(super) fn minute_precision(time: DateTime<Utc>) -> DateTime<Utc> {
    time.with_second(0).unwrap().with_nanosecond(0).unwrap()
}

fn should_remove(
    requested: bool,
    exists: bool,
    scheduled: bool,
    ignored: bool,
    consent: bool,
    previous_start: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    requested
        || !exists
        || !scheduled
        || (previous_start.is_some_and(|time| time > now) && (ignored || !consent))
}

#[derive(PartialEq, Debug, Clone)]
struct Desired {
    start: Option<DateTime<Utc>>,
    remove: bool,
}

async fn desired(
    pool: &PgPool,
    http: &reqwest::Client,
    remote: &RemoteRace,
) -> Result<Option<Desired>, Error> {
    let mut tx = pool.begin().await?;
    let Some(export) = ExportConfig::from_id(&mut tx, remote.export_id).await? else {
        return Ok(None);
    };
    if !export.enabled {
        return Ok(None);
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM races WHERE id=$1)")
        .bind(remote.race_id)
        .fetch_one(&mut *tx)
        .await?;
    let race = if exists {
        Some(Race::from_id(&mut tx, http, Id::from(remote.race_id as u64)).await?)
    } else {
        None
    };
    let requested: bool = sqlx::query_scalar(
        "SELECT operation='delete' FROM speedgaming_race_exports WHERE race_id=$1 AND export_id=$2",
    )
    .bind(remote.race_id)
    .bind(remote.export_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let start = race.as_ref().and_then(|r| match r.schedule {
        RaceSchedule::Live { start, .. } => Some(minute_precision(
            start + TimeDelta::minutes(export.delay_minutes.into()),
        )),
        _ => None,
    });
    let consent = race.as_ref().is_some_and(|r| {
        restream_consent_allows_export(
            r.restream_consent_required,
            r.teams_opt()
                .map(|mut teams| teams.all(|t| t.restream_consent)),
        )
    });
    let remove = should_remove(
        requested,
        exists,
        start.is_some(),
        race.as_ref().is_some_and(|r| r.ignored),
        consent,
        remote.synced_start,
        Utc::now(),
    );
    Ok(Some(Desired { start, remove }))
}

async fn retire(pool: &PgPool, remote: &RemoteRace) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    // The episode identifies the remote race. Reset applications only
    // after remote deletion is verified, so a recreated episode gets new signups.
    sqlx::query("DELETE FROM speedgaming_volunteer_exports WHERE export_id=$1 AND episode_id=$2")
        .bind(remote.export_id)
        .bind(remote.episode_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE speedgaming_race_exports SET episode_id=NULL,match_id=NULL,
        synced_start=NULL,operation='delete',state='succeeded',last_error=NULL,attempt_count=0
        WHERE race_id=$1 AND export_id=$2",
    )
    .bind(remote.race_id)
    .bind(remote.export_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn reconciled(pool: &PgPool, remote: &RemoteRace, start: DateTime<Utc>) -> Result<(), Error> {
    // A concurrent unschedule may have changed the requested operation.
    sqlx::query("UPDATE speedgaming_race_exports SET synced_start=$3,
        state=CASE WHEN operation='delete' THEN 'pending'::speedgaming_delivery_state ELSE 'succeeded'::speedgaming_delivery_state END,
        last_error=NULL,attempt_count=0 WHERE race_id=$1 AND export_id=$2")
        .bind(remote.race_id).bind(remote.export_id).bind(start).execute(pool).await?;
    Ok(())
}

async fn reconcile_one(
    pool: &PgPool,
    http: &reqwest::Client,
    remote: &RemoteRace,
) -> Result<(), Error> {
    let Some(target) = desired(pool, http, remote).await? else {
        return Ok(());
    };
    // Clock passage and completed-race housekeeping do not remove SG history.
    if !target.remove
        && target.start == remote.synced_start
        && target.start.is_some_and(|t| t <= Utc::now())
    {
        return Ok(());
    }
    let needs_time = target.start != remote.synced_start;
    if !target.remove && !needs_time && remote.match_id.is_some() {
        reconcile_volunteers(pool, remote).await?;
        if remote.state != "succeeded" {
            reconciled(pool, remote, target.start.unwrap()).await?
        }
        return Ok(());
    }
    let observed = episode(remote, target.start).await?;
    let match_id = single_match(&observed, remote.match_id)?;
    // Old records have no trustworthy remote baseline until read back. Do not
    // overwrite a pre-existing manual time change just because of migration.
    if remote.match_id.is_none()
        && !target.remove
        && !needs_time
        && Some(observed.when) != remote.synced_start
    {
        return Err(Error::Attention(
            "Existing remote time differs; inspect the match before synchronization".into(),
        ));
    }
    sqlx::query(
        "UPDATE speedgaming_race_exports SET match_id=$3 WHERE race_id=$1 AND export_id=$2",
    )
    .bind(remote.race_id)
    .bind(remote.export_id)
    .bind(match_id)
    .execute(pool)
    .await?;
    let action = if target.remove {
        "Episode.delete-match1"
    } else {
        "Episode.update-time"
    };
    let page = if target.remove || (needs_time && Some(observed.when) != target.start) {
        Some(crew_page(remote, action).await?)
    } else {
        None
    };
    // Re-read local state after network reads. No revision counters are needed.
    if desired(pool, http, remote).await? != Some(target.clone()) {
        return Err(Error::Superseded);
    }
    if target.remove {
        sqlx::query(
            "UPDATE speedgaming_race_exports SET operation='delete',state='in_progress',
            last_attempt_at=NOW(),attempt_count=attempt_count+1 WHERE race_id=$1 AND export_id=$2",
        )
        .bind(remote.race_id)
        .bind(remote.export_id)
        .execute(pool)
        .await?;
        let result = mutate(page.unwrap(), remote, action, "Delete Match 1", vec![]).await?;
        if result.html.contains(&format!("#e{}", observed.id)) {
            return Err(Error::Attention(
                "Deletion response still contains the episode".into(),
            ));
        }
        if lookup_episode(remote, target.start).await?.is_some() {
            return Err(Error::Attention(
                "Deleted match still appears on schedule".into(),
            ));
        }
        retire(pool, remote).await?;
    } else {
        let start = target.start.unwrap();
        if let Some(page) = page {
            let claimed = sqlx::query("UPDATE speedgaming_race_exports SET operation='update',state='in_progress',
                last_attempt_at=NOW(),attempt_count=attempt_count+1 WHERE race_id=$1 AND export_id=$2 AND operation<>'delete'")
                .bind(remote.race_id).bind(remote.export_id).execute(pool).await?;
            if claimed.rows_affected() == 0 {
                return Err(Error::Superseded);
            }
            mutate(
                page,
                remote,
                action,
                "Update",
                vec![("when".into(), crew_time(start))],
            )
            .await?;
            if episode(remote, Some(start)).await?.when != start {
                return Err(Error::Attention(
                    "Time update did not match requested time".into(),
                ));
            }
        }
        reconciled(pool, remote, start).await?;
        reconcile_volunteers(pool, remote).await?;
    }
    Ok(())
}

pub(crate) async fn reconcile(pool: &PgPool, http: &reqwest::Client) -> Result<(), Error> {
    let remotes = sqlx::query_as::<_, RemoteRace>(
        "SELECT re.race_id,re.export_id,e.slug,
        re.episode_id,re.match_id,re.synced_start,re.state::text AS state
        FROM speedgaming_race_exports re JOIN speedgaming_exports e ON e.id=re.export_id
        WHERE re.episode_id IS NOT NULL AND e.enabled AND e.archived_at IS NULL
          AND (re.state IN ('pending','succeeded') OR re.last_attempt_at IS NULL
               OR re.last_attempt_at < NOW()-INTERVAL '5 minutes')
        ORDER BY re.export_id,re.race_id",
    )
    .fetch_all(pool)
    .await?;
    for remote in remotes {
        if let Err(error) = reconcile_one(pool, http, &remote).await {
            if matches!(error, Error::Superseded) {
                SYNC_NOTIFY.notify_one();
                continue;
            }
            sqlx::query(
                "UPDATE speedgaming_race_exports SET state=$3,last_error=$4,last_attempt_at=NOW()
                WHERE race_id=$1 AND export_id=$2",
            )
            .bind(remote.race_id)
            .bind(remote.export_id)
            .bind(if matches!(error, Error::AmbiguousSubmission(_)) {
                DeliveryState::Ambiguous
            } else {
                DeliveryState::Failed
            })
            .bind(error.to_string())
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

async fn reconcile_volunteers(pool: &PgPool, remote: &RemoteRace) -> Result<(), Error> {
    let candidates = sqlx::query(
        "SELECT v.signup_id,v.remote_id,v.role,u.discord_username,rb.language::text AS language,
            COALESCE(s.status IN ('pending','confirmed'),FALSE) AS active
        FROM speedgaming_volunteer_exports v LEFT JOIN signups s ON s.id=v.signup_id
        LEFT JOIN users u ON u.id=s.user_id LEFT JOIN role_bindings rb ON rb.id=s.role_binding_id
        WHERE v.export_id=$1 AND v.episode_id=$2 AND v.state IN ('succeeded','ambiguous')
          AND (v.remote_id IS NULL OR s.id IS NULL OR s.status NOT IN ('pending','confirmed'))",
    )
    .bind(remote.export_id)
    .bind(remote.episode_id)
    .fetch_all(pool)
    .await?;
    for row in candidates {
        let signup: i32 = row.try_get("signup_id")?;
        let role: Option<String> = row.try_get("role")?;
        let role =
            role.ok_or_else(|| Error::Attention("Volunteer role missing; remove manually".into()))?;
        let saved_id: Option<i64> = row.try_get("remote_id")?;
        let was_active: bool = row.try_get("active")?;
        let e = episode(remote, None).await?;
        let volunteers = match role.as_str() {
            "Commentary" => &e.commentators,
            "Tracking" => &e.trackers,
            _ => return Err(Error::Attention("Unsupported volunteer role".into())),
        };
        let tag: Option<String> = row.try_get("discord_username")?;
        let language: Option<String> = row.try_get("language")?;
        let matches = volunteers
            .iter()
            .filter(|v| {
                saved_id.map_or_else(
                    || {
                        tag.as_ref()
                            .is_some_and(|tag| v.discord_tag.eq_ignore_ascii_case(tag))
                            && language.as_ref() == Some(&v.language)
                    },
                    |id| id == v.id,
                )
            })
            .collect_vec();
        if matches.len() != 1 {
            if was_active {
                // A pending application may not be exposed by the schedule yet.
                continue;
            }
            return Err(Error::Attention(
                "Cannot uniquely identify withdrawn volunteer; inspect crew page".into(),
            ));
        }
        let id = matches[0].id;
        sqlx::query("UPDATE speedgaming_volunteer_exports SET remote_id=$3 WHERE signup_id=$1 AND export_id=$2")
            .bind(signup).bind(remote.export_id).bind(id).execute(pool).await?;
        if was_active {
            continue;
        }
        let page = crew_page(remote, "edit").await?;
        let (mut fields, action) =
            volunteer_delete_fields(&page.html, remote.episode_id.unwrap(), id, &role)?;
        let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM signups WHERE id=$1 AND status IN ('pending','confirmed'))")
            .bind(signup).fetch_one(pool).await?;
        if active {
            continue;
        }
        let enabled: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM speedgaming_exports WHERE id=$1 AND enabled AND archived_at IS NULL)")
            .bind(remote.export_id).fetch_one(pool).await?;
        if !enabled {
            continue;
        }
        fields.push(action);
        post_page(&page, &fields).await?;
        let after = episode(remote, None).await?;
        let still_present = if role == "Commentary" {
            after.commentators
        } else {
            after.trackers
        }
        .iter()
        .any(|v| v.id == id);
        if still_present {
            return Err(Error::Attention(
                "Volunteer removal was not reflected in schedule".into(),
            ));
        }
        sqlx::query("DELETE FROM speedgaming_volunteer_exports WHERE signup_id=$1 AND export_id=$2 AND episode_id=$3")
            .bind(signup).bind(remote.export_id).bind(remote.episode_id).execute(pool).await?;
    }
    Ok(())
}

type FormFields = Vec<(String, String)>;

fn volunteer_delete_fields(
    html: &str,
    episode: i64,
    id: i64,
    role: &str,
) -> Result<(FormFields, (String, String)), Error> {
    let (key, action) = match role {
        "Commentary" => ("cimid", "CommentatorInMatchDate.delete"),
        "Tracking" => ("timid", "TrackerInMatchDate.delete"),
        _ => return Err(Error::Attention("Unsupported volunteer role".into())),
    };
    let form = forms(html)
        .find(|f| {
            input_value(f, "episodeid").ok().as_deref() == Some(&episode.to_string())
                && input_value(f, key).ok().as_deref() == Some(&id.to_string())
                && f.contains(&format!("name=\"{action}\""))
        })
        .ok_or_else(|| {
            Error::Attention("Volunteer removal control missing; inspect crew permissions".into())
        })?;
    let mut fields = hidden_fields(form)?;
    fields.push(("episodeid".into(), episode.to_string()));
    fields.push((key.into(), id.to_string()));
    if let Ok(language) = input_value(form, "languageid") {
        fields.push(("languageid".into(), language))
    }
    Ok((fields, (action.into(), "X".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_races_are_preserved_but_explicit_removal_is_respected() {
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        let past = Some(now - TimeDelta::hours(9));
        let future = Some(now + TimeDelta::days(1));
        assert!(!should_remove(false, true, true, false, true, past, now));
        assert!(!should_remove(false, true, true, true, true, past, now));
        assert!(should_remove(true, true, true, true, true, past, now));
        assert!(should_remove(false, true, false, false, true, future, now));
        assert!(should_remove(false, false, false, false, true, future, now));
        assert!(should_remove(false, true, true, false, false, future, now));
        assert!(!should_remove(false, true, true, false, true, future, now));
    }
    #[test]
    fn literal_authorization_is_sensitive() {
        let header = authorization("hth", "test:password").unwrap();
        assert_eq!(header.to_str().unwrap(), "hth:test:password");
        assert!(header.is_sensitive());
        assert!(authorization("hth", "bad\r\nvalue").is_err());
        assert!(authorization("", "test").is_err());
    }
    #[test]
    fn only_the_owned_single_match_can_be_mutated() {
        let mut e: Episode = serde_json::from_str(
            r#"{"id":1,"when":"2026-09-13T18:45:00Z","match1":{"id":2},"match2":null}"#,
        )
        .unwrap();
        assert_eq!(single_match(&e, Some(2)).unwrap(), 2);
        assert!(single_match(&e, Some(3)).is_err());
        e.match2 = Some(RemoteMatch { id: 3 });
        assert!(single_match(&e, Some(2)).is_err());
    }
    #[test]
    fn crew_form_is_scoped_to_episode_and_action() {
        let html = r#"<form><input name="episodeid" value="12"><input name="Episode.delete-match1" value="Delete Match 1"></form><form><input name="episodeid" value="13"><input name="Episode.update-time" value="Update"></form>"#;
        assert!(action_form(html, 12, "Episode.update-time").is_none());
        assert!(action_form(html, 13, "Episode.update-time").is_some());
    }
    #[test]
    fn crew_times_use_eastern_time_and_minute_precision() {
        let summer = Utc.with_ymd_and_hms(2026, 9, 13, 18, 45, 32).unwrap();
        let winter = Utc.with_ymd_and_hms(2027, 1, 13, 19, 45, 0).unwrap();
        assert_eq!(crew_time(summer), "sep13 2026 2:45pm");
        assert_eq!(crew_time(winter), "jan13 2027 2:45pm");
        assert_eq!(minute_precision(summer).second(), 0);
        assert_eq!(minute_precision(summer).minute(), 45);
    }
    #[test]
    fn volunteer_removal_requires_the_exact_episode_identity_and_role() {
        let html = r#"<form><input name="csrfmiddlewaretoken" value="hth-token"><input name="page" value="2"><input name="episodeid" value="12"><input name="cimid" value="34"><input name="languageid" value="1"><input name="CommentatorInMatchDate.delete" value="X"></form>"#;
        let (fields, action) = volunteer_delete_fields(html, 12, 34, "Commentary").unwrap();
        assert!(fields.contains(&("cimid".into(), "34".into())));
        assert!(fields.contains(&("csrfmiddlewaretoken".into(), "hth-token".into())));
        assert_eq!(action.0, "CommentatorInMatchDate.delete");
        assert!(volunteer_delete_fields(html, 13, 34, "Commentary").is_err());
        assert!(volunteer_delete_fields(html, 12, 35, "Commentary").is_err());
        assert!(volunteer_delete_fields(html, 12, 34, "Tracking").is_err());
    }
}
