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
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartEnvelope<T> {
    pub(crate) status: u16,
    pub(crate) response: T,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartGenerateResponse {
    pub(crate) hash: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AvianartPermlinkResponse {
    pub(crate) status: Option<String>,
    pub(crate) message: String,
    pub(crate) spoiler: Option<AvianartSpoiler>,
    // Present when generation is complete; absence means still generating
    pub(crate) patch: Option<serde_json::Value>,
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
    #[error("Seed generation failed: {0}")]
    GenerationFailed(String),
    #[error("Seed generation timed out after {0} poll attempts")]
    Timeout(u32),
    #[error("Expected 5 hash icons, got {count} in: {raw}")]
    HashParse { count: usize, raw: String },
}

impl AvianartClient {
    pub(crate) fn new(api_key: Option<String>, client: Client) -> Self {
        Self { api_key, client }
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
        let result: AvianartEnvelope<AvianartGenerateResponse> =
            req.json(&body).send().await?.json().await?;
        if cfg!(debug_assertions) {
            eprintln!("[avianart] generate_seed: response status = {}, hash = {:?}", result.status, result.response.hash);
        }
        Ok(result.response.hash)
    }

    pub(crate) async fn wait_for_seed(&self, hash: &str) -> Result<AvianartPermlinkResponse, AvianartError> {
        let url = format!("{}?action=permlink&hash={}", API_URL, hash);
        if cfg!(debug_assertions) {
            eprintln!("[avianart] wait_for_seed: polling for hash={hash}, url={url}");
            eprintln!("[avianart] wait_for_seed: poll interval={POLL_INTERVAL_SECS}s, max attempts={MAX_POLL_ATTEMPTS}");
        }
        for attempt in 0..MAX_POLL_ATTEMPTS {
            sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            if cfg!(debug_assertions) {
                eprintln!("[avianart] wait_for_seed: attempt {}/{MAX_POLL_ATTEMPTS}", attempt + 1);
            }
            let req = self.apply_auth(self.client.get(&url));
            let envelope: AvianartEnvelope<AvianartPermlinkResponse> =
                req.send().await?.json().await?;
            if cfg!(debug_assertions) {
                eprintln!(
                    "[avianart] wait_for_seed: outer status={}, inner status={:?}, message={:?}, has_spoiler={}, has_patch={}",
                    envelope.status,
                    envelope.response.status,
                    envelope.response.message,
                    envelope.response.spoiler.is_some(),
                    envelope.response.patch.is_some(),
                );
            }
            // Inner status "failure" means generation failed
            if envelope.response.status.as_deref() == Some("failure") {
                if cfg!(debug_assertions) {
                    eprintln!("[avianart] wait_for_seed: generation failed: {}", envelope.response.message);
                }
                return Err(AvianartError::GenerationFailed(envelope.response.message));
            }
            // Seed is complete when inner status is absent and patch data is present
            if envelope.response.status.is_none() && envelope.response.patch.is_some() {
                if cfg!(debug_assertions) {
                    eprintln!("[avianart] wait_for_seed: seed ready after {} attempt(s)", attempt + 1);
                    if let Some(ref spoiler) = envelope.response.spoiler {
                        eprintln!("[avianart] wait_for_seed: spoiler meta hash = {:?}", spoiler.meta.hash);
                    }
                }
                return Ok(envelope.response);
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
