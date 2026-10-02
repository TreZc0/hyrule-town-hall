//! API-only MMR generation: https://mmrandomizer.com/api/docs.
use crate::prelude::*;
use reqwest::{Method, StatusCode};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(unix, derive(async_proto::Protocol))]
pub(crate) struct Settings {
    #[serde(default = "master")]
    pub(crate) branch: String,
    pub(crate) version: String,
    pub(crate) settings: serde_json::Value,
}

fn master() -> String {
    "master".into()
}

impl Settings {
    pub(crate) fn parse(value: &serde_json::Value) -> Result<Self, String> {
        let config: Self = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.branch != "master"
            && (!self.branch.starts_with("dev")
                || !self.branch.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            return Err("MMR branch must be master or a dev branch name.".into());
        }
        semver::Version::parse(&self.version).map_err(|_| {
            "MMR version must be a pinned version from the branch's availableVersions list."
                .to_owned()
        })?;
        let settings = self
            .settings
            .as_object()
            .filter(|settings| !settings.is_empty())
            .ok_or("MMR settings must be a non-empty, flat JSON settings map exported by MMR.")?;
        for (key, value) in settings {
            let normalized = key.to_ascii_lowercase().replace(['_', '.'], "");
            if normalized == "seed" || normalized.ends_with("seedstring") {
                return Err("MMR race settings cannot specify a fixed seed.".into());
            }
            if (normalized.contains("multiworld")
                && !matches!(
                    value,
                    serde_json::Value::Bool(false) | serde_json::Value::Null
                ))
                || (normalized.contains("worldcount") && value.as_u64() != Some(1))
            {
                return Err("MMR multiworld is not supported.".into());
            }
            if !key.contains('.') || value.is_object() {
                return Err(format!(
                    "MMR setting {key} must use the flat API settings format."
                ));
            }
        }
        Ok(())
    }

    fn api_version(&self) -> String {
        if self.branch == "master" {
            self.version.clone()
        } else {
            format!("{}_{}", self.branch, self.version)
        }
    }

    fn body(&self) -> serde_json::Value {
        let mut settings = self.settings.clone();
        settings["OutputSettings.GenerateSpoilerLog"] = json!(true);
        settings
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Purpose {
    Practice,
    Competition,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("MMR API key is not configured")]
    MissingKey,
    #[error("MMR: {0}")]
    Invalid(String),
    #[error("MMR API {operation} returned HTTP {status}")]
    Http { operation: String, status: u16 },
    #[error("MMR API request failed: {0}")]
    Network(reqwest::Error),
    #[error("MMR generation timed out")]
    Timeout,
}

pub(crate) struct ApiClient {
    http: reqwest::Client,
    key: Option<String>,
    base: String,
    next_request: tokio::sync::Mutex<Instant>,
}

impl ApiClient {
    pub(crate) fn new(http: reqwest::Client, key: Option<String>) -> Self {
        Self {
            http,
            key: key.filter(|s| !s.trim().is_empty()),
            base: "https://mmrandomizer.com".into(),
            next_request: tokio::sync::Mutex::new(Instant::now()),
        }
    }

    fn create_query(&self, config: &Settings, purpose: Purpose) -> Vec<(String, String)> {
        let mut query = vec![
            ("version".into(), config.api_version()),
            ("locked".into(), "true".into()),
        ];
        // These are presence flags: encrypt=false would still enable encryption.
        if purpose == Purpose::Competition {
            query.push(("encrypt".into(), "true".into()));
        }
        query
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response, Error> {
        let key = self.key.as_ref().ok_or(Error::MissingKey)?;
        for attempt in 0..6 {
            let response = {
                let mut next = self.next_request.lock().await;
                sleep_until(*next).await;
                let mut request = self
                    .http
                    .request(method.clone(), format!("{}{path}", self.base))
                    .query(&[("key", key)])
                    .query(query)
                    .timeout(Duration::from_secs(30));
                if let Some(body) = body {
                    request = request.json(body);
                }
                let response = request.send().await;
                *next = Instant::now() + Duration::from_millis(600);
                response
            };
            match response {
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status() == StatusCode::LOCKED =>
                {
                    if attempt == 5 {
                        return Err(Error::Http {
                            operation: path.into(),
                            status: response.status().as_u16(),
                        });
                    }
                    let delay = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(2_u64.pow(attempt + 1))
                        .min(120);
                    sleep(Duration::from_secs(delay)).await;
                }
                Ok(response)
                    if response.status().is_server_error()
                        && method == Method::GET
                        && attempt < 5 =>
                {
                    sleep(Duration::from_secs(2_u64.pow(attempt + 1))).await
                }
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    return Err(Error::Http {
                        operation: path.into(),
                        status: response.status().as_u16(),
                    });
                }
                Err(_) if method == Method::GET && attempt < 5 => {
                    sleep(Duration::from_secs(2_u64.pow(attempt + 1))).await
                }
                Err(error) => return Err(Error::Network(error.without_url())),
            }
        }
        unreachable!()
    }

    async fn json(&self, path: &str, id: &str) -> Result<Option<serde_json::Value>, Error> {
        let response = self
            .request(Method::GET, path, &[("id".into(), id.into())], None)
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| Error::Network(e.without_url()))
    }

    pub(crate) async fn roll(
        &self,
        config: Settings,
        purpose: Purpose,
        updates: &mpsc::Sender<racetime_bot::SeedRollUpdate>,
    ) -> Result<seed::Data, Error> {
        config.validate().map_err(Error::Invalid)?;
        let response = self
            .request(
                Method::POST,
                "/api/v2/seed/create",
                &self.create_query(&config, purpose),
                Some(&config.body()),
            )
            .await?;
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Error::Network(e.without_url()))?;
        let id = seed_id(&value["id"])?;
        let deadline = Instant::now() + Duration::from_secs(1800);
        let mut last_position = None;
        loop {
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            let status = self
                .json("/api/v2/seed/status", &id)
                .await?
                .ok_or_else(|| Error::Invalid("empty status response".into()))?;
            match status["status"].as_u64() {
                Some(0) => {
                    let position = status["positionQueue"].as_u64().unwrap_or(0);
                    if last_position != Some(position) {
                        let update = if position == 0 {
                            racetime_bot::SeedRollUpdate::Started
                        } else {
                            racetime_bot::SeedRollUpdate::Queued(position)
                        };
                        let _ = updates.try_send(update);
                        last_position = Some(position);
                    }
                }
                Some(1) => {
                    if let Some(details) = self.json("/api/v2/seed/details", &id).await? {
                        let data = payload(&id, &details, purpose)?;
                        return Ok(seed::Data::from_seed_data_only(Some(data), None, false));
                    }
                }
                Some(3) => return Err(Error::Invalid(format!("seed {id} failed to generate"))),
                _ => return Err(Error::Invalid("unexpected generation status".into())),
            }
            sleep(Duration::from_secs(2)).await;
        }
    }

    pub(crate) async fn unlock(&self, id: &str) -> Result<bool, Error> {
        let response = self
            .request(
                Method::POST,
                "/api/v2/seed/unlock",
                &[("id".into(), id.into())],
                None,
            )
            .await?;
        Ok(response.status() != StatusCode::NO_CONTENT)
    }

    /// Uses the existing completion state. Shared/pooled seeds must never be
    /// unlocked just because one of their runners finished.
    pub(crate) async fn unlock_finished(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        if self.key.is_none() {
            return Ok(());
        }
        let ids: Vec<String> = sqlx::query_scalar(r#"
            WITH uses AS (
                SELECT r.seed_data->>'id' AS id,
                    NOT r.ignored AND e.spoiler_unlock = 'after'
                    AND CASE WHEN r.async_start1 IS NOT NULL OR r.async_start2 IS NOT NULL OR r.async_start3 IS NOT NULL
                        THEN r.async_end1 IS NOT NULL AND r.async_end2 IS NOT NULL
                            AND ((r.team3 IS NULL AND r.p3 IS NULL AND r.async_start3 IS NULL) OR r.async_end3 IS NOT NULL)
                        ELSE r.end_time IS NOT NULL END AS releasable,
                    r.seed_data->>'locked' IS DISTINCT FROM 'false' AS locked
                FROM races r JOIN events e ON e.series=r.series AND e.event=r.event
                WHERE r.seed_data->>'type'='mmr'
                UNION ALL
                SELECT a.seed_data->>'id', e.spoiler_unlock='after' AND a.end_time IS NOT NULL AND a.end_time <= NOW()
                    AND NOT EXISTS (SELECT 1 FROM async_teams at JOIN teams t ON t.id=at.team
                        WHERE t.series=a.series AND t.event=a.event AND at.kind=a.kind AND at.requested IS NOT NULL AND at.submitted IS NULL),
                    a.seed_data->>'locked' IS DISTINCT FROM 'false'
                FROM asyncs a JOIN events e ON e.series=a.series AND e.event=a.event
                WHERE a.seed_data->>'type'='mmr'
                UNION ALL
                SELECT seed_data->>'id', FALSE, TRUE FROM qualifier_seeds WHERE seed_data->>'type'='mmr'
            ) SELECT id FROM uses WHERE id IS NOT NULL GROUP BY id HAVING bool_and(COALESCE(releasable, FALSE)) AND bool_or(locked)
        "#).fetch_all(pool).await?;
        for id in ids {
            match self.unlock(&id).await {
                Ok(true) => {
                    let mut tx = pool.begin().await?;
                    for table in ["races", "asyncs"] {
                        sqlx::query(&format!("UPDATE {table} SET seed_data=jsonb_set(seed_data, '{{locked}}', 'false') WHERE seed_data->>'type'='mmr' AND seed_data->>'id'=$1"))
                            .bind(&id).execute(&mut *tx).await?;
                    }
                    tx.commit().await?;
                }
                Ok(false) => (),
                Err(error) => log::warn!("Could not release MMR spoiler for seed {id}: {error}"),
            }
        }
        Ok(())
    }
}

fn seed_id(value: &serde_json::Value) -> Result<String, Error> {
    let id = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(Error::Invalid("invalid seed ID".into()));
    }
    Ok(id)
}

fn payload(
    id: &str,
    details: &serde_json::Value,
    purpose: Purpose,
) -> Result<serde_json::Value, Error> {
    let log = match &details["settingsLog"] {
        serde_json::Value::String(log) => {
            serde_json::from_str(log).map_err(|_| Error::Invalid("invalid settings log".into()))?
        }
        serde_json::Value::Object(_) => details["settingsLog"].clone(),
        _ => {
            return Err(Error::Invalid(
                "missing settings log; check locked spoiler access scopes".into(),
            ));
        }
    };
    let hash: Vec<String> = serde_json::from_value(log["Hash"].clone())
        .map_err(|_| Error::Invalid("missing seed hash".into()))?;
    if hash.is_empty() || hash.iter().any(|s| s.trim().is_empty()) {
        return Err(Error::Invalid("empty seed hash".into()));
    }
    let version = details["version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Invalid("missing version".into()))?;
    let timestamp = details["creationTimestamp"]
        .as_str()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| Error::Invalid("invalid creation timestamp".into()))?;
    Ok(
        json!({"type":"mmr", "id":id, "version":version, "gen_time":timestamp, "hash":hash,
        "encrypted":purpose == Purpose::Competition, "locked":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    fn settings() -> Settings {
        Settings::parse(&json!({"version":"2.0.0-0", "settings": {
            "GameplaySettings.DrawHash": true,
            "OutputSettings.GenerateSpoilerLog": false
        }}))
        .unwrap()
    }

    fn details() -> serde_json::Value {
        json!({"version":"2.0.0-0", "creationTimestamp":"2026-10-02T12:00:00Z",
            "settingsLog": "{\"Hash\":[\"ITEM_BOW\",\"ITEM_BOMB\",\"0x61\",\"ITEM_MAP\",\"ITEM_OCARINA\"]}",
            "seed":"private-seed-string", "spoilerLog":"private-spoiler"})
    }

    /// Real HTTP requests against a local server; never contacts MMR.
    async fn server(
        replies: Vec<(u16, String)>,
    ) -> (ApiClient, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = ApiClient::new(reqwest::Client::new(), Some("test-api-secret".into()));
        client.base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let read = socket.read(&mut buffer).await.unwrap();
                    assert!(read > 0);
                    bytes.extend_from_slice(&buffer[..read]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length:")
                                    .and_then(|n| n.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nRetry-After: 0\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (client, task)
    }

    #[tokio::test]
    async fn mmr_competition_roll_is_encrypted_and_reads_encoded_hash() {
        let (client, requests) = server(vec![
            (200, "{\"id\":123}".into()),
            (200, "{\"status\":1}".into()),
            (200, details().to_string()),
        ])
        .await;
        let (tx, _rx) = mpsc::channel(16);
        let seed = client
            .roll(settings(), Purpose::Competition, &tx)
            .await
            .unwrap();
        let requests = requests.await.unwrap();
        assert!(requests[0].contains("locked=true"));
        assert!(requests[0].contains("encrypt=true"));
        assert!(requests[0].contains("version=2.0.0-0"));
        let body: serde_json::Value =
            serde_json::from_str(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["OutputSettings.GenerateSpoilerLog"], true);
        let data = seed.to_seed_data().unwrap();
        assert_eq!(data["encrypted"], true);
        assert_eq!(data["id"], "123");
        assert_eq!(data["hash"][2], "0x61");
        assert!(!data.to_string().contains("private"));
        assert!(
            matches!(seed.files(), Some(seed::Files::MmrWeb { id, hash }) if id == "123" && hash.len() == 5)
        );
    }

    #[tokio::test]
    async fn mmr_practice_omits_encryption_and_waits_for_details() {
        let (client, requests) = server(vec![
            (200, "{\"id\":\"321\"}".into()),
            (200, "{\"status\":1}".into()),
            (204, String::new()),
            (200, "{\"status\":1}".into()),
            (200, details().to_string()),
        ])
        .await;
        let (tx, _rx) = mpsc::channel(16);
        let data = client
            .roll(settings(), Purpose::Practice, &tx)
            .await
            .unwrap()
            .to_seed_data()
            .unwrap();
        let requests = requests.await.unwrap();
        assert!(requests[0].contains("locked=true"));
        assert!(!requests[0].contains("encrypt"));
        assert_eq!(data["encrypted"], false);
        assert_eq!(requests.len(), 5);
    }

    #[tokio::test]
    async fn mmr_queue_rejection_can_retry_but_failed_generation_does_not_reroll() {
        let (client, requests) = server(vec![
            (423, String::new()),
            (200, "{\"id\":42}".into()),
            (200, "{\"status\":3}".into()),
        ])
        .await;
        let (tx, _rx) = mpsc::channel(16);
        let error = client
            .roll(settings(), Purpose::Competition, &tx)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("failed to generate"));
        assert_eq!(requests.await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn mmr_uncertain_create_is_not_repeated_and_errors_do_not_expose_key() {
        let (client, requests) = server(vec![(500, "test-api-secret".into())]).await;
        let (tx, _rx) = mpsc::channel(16);
        let error = client
            .roll(settings(), Purpose::Competition, &tx)
            .await
            .unwrap_err();
        assert!(!format!("{error:?} {error}").contains("test-api-secret"));
        assert_eq!(requests.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn mmr_unlock_distinguishes_pending_and_already_unlocked() {
        let (client, requests) = server(vec![(204, String::new()), (208, String::new())]).await;
        assert!(!client.unlock("42").await.unwrap());
        assert!(client.unlock("42").await.unwrap());
        assert!(
            requests
                .await
                .unwrap()
                .iter()
                .all(|request| request.starts_with("POST /api/v2/seed/unlock?"))
        );
    }

    #[test]
    fn mmr_config_rejects_multiworld_fixed_seeds_and_unsupported_policies() {
        for (key, value) in [
            ("GameplaySettings.MultiWorld", json!(true)),
            ("world_count", json!(2)),
            ("seed", json!("fixed")),
        ] {
            let mut config = json!(settings());
            config["settings"][key] = value;
            assert!(Settings::parse(&config).is_err());
        }
        assert!(Settings::parse(&json!({"settings":{}})).is_err());
        let mut config = settings();
        config.branch = "devactorizer".into();
        assert_eq!(config.api_version(), "devactorizer_2.0.0-0");
        for policy in ["after", "never"] {
            for preroll in ["none", "short", "medium", "long"] {
                assert!(
                    event::configuration::validate_seed_policies(preroll, policy, Some("mmr"))
                        .is_ok()
                );
            }
        }
        assert!(
            event::configuration::validate_seed_policies("none", "immediately", Some("mmr"))
                .is_err()
        );
        assert!(event::configuration::validate_seed(Some("mmr"), Some(&json!(settings()))).is_ok());
    }

    #[test]
    fn mmr_requires_valid_metadata_and_keeps_unknown_hash_identifiers() {
        let mut details = details();
        details["settingsLog"] = json!({"Hash":["future-icon", "0x61"]});
        let data = payload("id", &details, Purpose::Competition).unwrap();
        let files = seed::Files::from_seed_data(&data).unwrap();
        assert!(seed::Files::from_seed_data(&files.to_seed_data_base()).is_some());
        assert_eq!(data["hash"][0], "future-icon");
        details["settingsLog"] = json!({"Hash":[]});
        assert!(payload("id", &details, Purpose::Competition).is_err());
        assert!(seed_id(&json!("bad&id=other")).is_err());
    }

    #[test]
    fn mmr_hash_catalog_has_every_mapping_and_image() {
        let sql = include_str!("../migrations/121_mmr_hash_icons.sql");
        let rows = sql
            .lines()
            .filter(|line| line.trim_start().starts_with("('"))
            .collect_vec();
        assert_eq!(rows.len(), 64);
        let mut assets = HashSet::new();
        for row in rows {
            let path = row.split('\'').nth(3).unwrap();
            let bytes = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("assets/static/hash-icon")
                    .join(path),
            )
            .unwrap();
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{path}");
            assets.insert(path);
        }
        assert_eq!(assets.len(), 63);
        assert!(sql.contains("('0x61', 'mmr/HashBombersNote.png', 'HashBombersNote')"));
    }

    #[tokio::test]
    #[ignore = "requires HTH_MMR_TEST_DATABASE_URL; uses connection-local temporary tables"]
    async fn mmr_spoilers_wait_for_every_race_part_and_shared_async_submission() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&std::env::var("HTH_MMR_TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        sqlx::raw_sql(r#"
            CREATE TEMP TABLE events (series TEXT, event TEXT, spoiler_unlock TEXT);
            CREATE TEMP TABLE races (
                series TEXT DEFAULT 'test', event TEXT DEFAULT 'after', seed_data JSONB,
                ignored BOOLEAN DEFAULT FALSE, end_time TIMESTAMPTZ,
                async_start1 TIMESTAMPTZ, async_start2 TIMESTAMPTZ, async_start3 TIMESTAMPTZ,
                async_end1 TIMESTAMPTZ, async_end2 TIMESTAMPTZ, async_end3 TIMESTAMPTZ,
                team3 BIGINT, p3 TEXT
            );
            CREATE TEMP TABLE asyncs (series TEXT DEFAULT 'test', event TEXT DEFAULT 'after', kind TEXT, seed_data JSONB, end_time TIMESTAMPTZ);
            CREATE TEMP TABLE teams (id BIGINT, series TEXT DEFAULT 'test', event TEXT DEFAULT 'after');
            CREATE TEMP TABLE async_teams (team BIGINT, kind TEXT, requested TIMESTAMPTZ, submitted TIMESTAMPTZ);
            CREATE TEMP TABLE qualifier_seeds (seed_data JSONB);
            CREATE TEMP TABLE games (id SERIAL PRIMARY KEY, name VARCHAR(255) UNIQUE NOT NULL, display_name VARCHAR(255) NOT NULL, description TEXT);
            CREATE TEMP TABLE hash_icons (game_id INTEGER REFERENCES games(id), name VARCHAR(255), file_name VARCHAR(255), racetime_emoji VARCHAR(100), UNIQUE(game_id,name));
            INSERT INTO events VALUES ('test','after','after'), ('test','never','never');
            INSERT INTO races (seed_data)
                SELECT jsonb_build_object('type','mmr','id',id::text,'locked',true) FROM generate_series(1,10) id;
            UPDATE races SET end_time=NOW() WHERE seed_data->>'id' IN ('1','7','8','9','10');
            UPDATE races SET async_start1=NOW(), async_end1=NOW() WHERE seed_data->>'id' IN ('3','4','5','6');
            UPDATE races SET async_end2=NOW() WHERE seed_data->>'id' IN ('4','5','6');
            UPDATE races SET team3=99 WHERE seed_data->>'id'='4';
            UPDATE races SET p3='third entrant' WHERE seed_data->>'id'='5';
            UPDATE races SET ignored=TRUE WHERE seed_data->>'id'='7';
            UPDATE races SET event='never' WHERE seed_data->>'id'='8';
            INSERT INTO races (seed_data) SELECT seed_data FROM races WHERE seed_data->>'id'='9';
            INSERT INTO qualifier_seeds SELECT seed_data FROM races WHERE seed_data->>'id'='10';
            INSERT INTO asyncs (kind,seed_data,end_time)
                SELECT id::text, jsonb_build_object('type','mmr','id',id::text,'locked',true), NOW()-INTERVAL '1 minute' FROM generate_series(11,13) id;
            UPDATE asyncs SET end_time=NOW()+INTERVAL '1 hour' WHERE kind='13';
            INSERT INTO teams (id) VALUES (1), (2);
            INSERT INTO async_teams VALUES (1,'11',NOW(),NULL), (2,'12',NOW(),NOW());
        "#).execute(&pool).await.unwrap();
        for _ in 0..2 {
            sqlx::raw_sql(include_str!("../migrations/120_mmr_seeds.sql"))
                .execute(&pool)
                .await
                .unwrap();
            sqlx::raw_sql(include_str!("../migrations/121_mmr_hash_icons.sql"))
                .execute(&pool)
                .await
                .unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM hash_icons")
                .fetch_one(&pool)
                .await
                .unwrap(),
            64
        );
        let (client, requests) = server(vec![(200, String::new()); 9]).await;
        client.unlock_finished(&pool).await.unwrap();
        let released = sqlx::query_scalar::<_, String>("SELECT seed_data->>'id' FROM races WHERE seed_data->>'locked'='false' UNION SELECT seed_data->>'id' FROM asyncs WHERE seed_data->>'locked'='false' ORDER BY 1").fetch_all(&pool).await.unwrap();
        assert_eq!(released, ["1", "12", "6"]);
        // A second pass must not call the API again for already released seeds.
        client.unlock_finished(&pool).await.unwrap();
        sqlx::raw_sql(r#"
            UPDATE races SET end_time=NOW() WHERE seed_data->>'id' IN ('2','9');
            UPDATE races SET async_end2=NOW(), async_end3=NOW() WHERE seed_data->>'id' IN ('3','4','5');
            UPDATE async_teams SET submitted=NOW() WHERE kind='11';
        "#).execute(&pool).await.unwrap();
        client.unlock_finished(&pool).await.unwrap();
        let mut unlocked = requests
            .await
            .unwrap()
            .into_iter()
            .map(|request| {
                let url = Url::parse(&format!(
                    "http://local{}",
                    request.split_whitespace().nth(1).unwrap()
                ))
                .unwrap();
                url.query_pairs()
                    .find(|(name, _)| name == "id")
                    .unwrap()
                    .1
                    .into_owned()
            })
            .collect_vec();
        unlocked.sort();
        assert_eq!(unlocked, ["1", "11", "12", "2", "3", "4", "5", "6", "9"]);
        pool.close().await;
    }
}
