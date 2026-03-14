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
        let req = self.apply_auth(self.client.post(&url));
        let result: AvianartEnvelope<AvianartGenerateResponse> =
            req.json(&body).send().await?.json().await?;
        Ok(result.response.hash)
    }

    pub(crate) async fn wait_for_seed(&self, hash: &str) -> Result<AvianartPermlinkResponse, AvianartError> {
        let url = format!("{}?action=permlink&hash={}", API_URL, hash);
        for _ in 0..MAX_POLL_ATTEMPTS {
            sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            let req = self.apply_auth(self.client.get(&url));
            let envelope: AvianartEnvelope<AvianartPermlinkResponse> =
                req.send().await?.json().await?;
            // Inner status "failure" means generation failed
            if envelope.response.status.as_deref() == Some("failure") {
                return Err(AvianartError::GenerationFailed(envelope.response.message));
            }
            if envelope.status == 200 {
                return Ok(envelope.response);
            }
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
