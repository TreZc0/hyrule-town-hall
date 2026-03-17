use {
    reqwest::Client,
    crate::prelude::*,
};

const API_URL: &str = "https://avianart.games/api.php";
const POLL_INTERVAL_SECS: u64 = 5;
const MAX_POLL_ATTEMPTS: u32 = 60; // 5 minutes max

#[derive(Debug, Clone)]
pub(crate) struct AvianartClient {
    api_key: Option<String>,
    client: Client,
    verbose: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartEnvelope<T> {
    #[allow(dead_code)]
    status: u16,
    pub(crate) response: T,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartGenerateResponse {
    pub(crate) hash: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartPermlinkResponse {
    pub(crate) status: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) spoiler: Option<AvianartSpoiler>,
    // Present when generation is complete; absence means still generating
    #[allow(dead_code)]
    patch: Option<serde_json::Value>, // present when generation is complete
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartSpoiler {
    pub(crate) meta: SpoilerMeta,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpoilerMeta {
    pub(crate) hash: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AvianartError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Seed generation failed: {0}")]
    GenerationFailed(String),
    #[error("Seed generation timed out after {0} poll attempts")]
    Timeout(u32),
    #[error("Expected 5 hash icons, got {count} in: {raw}")]
    HashParse { count: usize, raw: String },
}

impl AvianartClient {
    pub(crate) fn new(api_key: Option<String>, client: Client, verbose: bool) -> Self {
        Self { api_key, client, verbose }
    }

    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref key) = self.api_key {
            builder.header("Authorization", key)
        } else {
            builder
        }
    }

    pub(crate) async fn generate_seed(&self, preset: &str) -> Result<String, AvianartError> {
        let url = format!("{}?action=generate&preset={}", API_URL, preset);
        let body = json!([{"args": {"race": true}}]);
        if cfg!(debug_assertions) {
            eprintln!("[avianart] generate_seed: POST {url}");
            eprintln!("[avianart] generate_seed: body = {body}");
            eprintln!("[avianart] generate_seed: auth header present = {}", self.api_key.is_some());
        }
        let req = self.apply_auth(self.client.post(&url));
        let result: AvianartEnvelope<AvianartGenerateResponse> = if self.verbose {
            let text = req.json(&body).send().await?.text().await?;
            eprintln!("[avianart] generate POST {url} body={body}");
            eprintln!("[avianart] generate response: {text}");
            serde_json::from_str(&text)?
        } else {
            req.json(&body).send().await?.json().await?
        };
        if self.verbose { eprintln!("[avianart] generate hash={}", result.response.hash); }
        Ok(result.response.hash)
    }

    pub(crate) async fn wait_for_seed(&self, hash: &str) -> Result<AvianartPermlinkResponse, AvianartError> {
        let url = format!("{}?action=permlink&hash={}", API_URL, hash);
        for attempt in 1..=MAX_POLL_ATTEMPTS {
            sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            if cfg!(debug_assertions) {
                eprintln!("[avianart] wait_for_seed: attempt {}/{MAX_POLL_ATTEMPTS}", attempt + 1);
            }
            let req = self.apply_auth(self.client.get(&url));
            let envelope: AvianartEnvelope<AvianartPermlinkResponse> = if self.verbose {
                let text = req.send().await?.text().await?;
                eprintln!("[avianart] permlink attempt {attempt}/{MAX_POLL_ATTEMPTS}: {text}");
                serde_json::from_str(&text)?
            } else {
                req.send().await?.json().await?
            };
            match envelope.response.status.as_deref() {
                Some("failure") => return Err(AvianartError::GenerationFailed(envelope.response.message.unwrap_or_default())),
                None => {
                    if self.verbose { eprintln!("[avianart] seed ready after {attempt} attempt(s), spoiler={:?}", envelope.response.spoiler.as_ref().map(|s| &s.meta.hash)); }
                    return Ok(envelope.response); // status absent = generation complete
                }
                Some(s) => {
                    if self.verbose { eprintln!("[avianart] still generating: status={s:?}"); }
                }
            }
        }
        if cfg!(debug_assertions) {
            eprintln!("[avianart] wait_for_seed: timed out after {MAX_POLL_ATTEMPTS} attempts");
        }
        Err(AvianartError::Timeout(MAX_POLL_ATTEMPTS))
    }
}

pub(crate) fn parse_file_hash(hash_str: &str) -> Result<[String; 5], AvianartError> {
    let parts: Vec<String> = hash_str
        .split(", ")
        .map(|s| s.trim().replace(' ', "")) // "Bug Net" → "BugNet"
        .collect();
    parts.try_into().map_err(|v: Vec<_>| AvianartError::HashParse {
        count: v.len(),
        raw: hash_str.to_owned(),
    })
}
