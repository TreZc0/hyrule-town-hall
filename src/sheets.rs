//! Utilities for working with Google Sheets.

// This module is only used for events which publish their race schedules as Google sheets.
// Allow it to remain unused between those events rather than deleting and restoring it each time.
#![allow(unused)]

use {
    yup_oauth2::{
        ServiceAccountAuthenticator,
        read_service_account_key,
    },
    crate::prelude::*,
};

/// from <https://developers.google.com/sheets/api/limits#quota>:
///
/// > Read requests […] Per minute per user per project […] 60
const RATE_LIMIT: Duration = Duration::from_secs(1);

static CACHE: LazyLock<Mutex<(Instant, HashMap<(String, String), (Instant, Vec<Vec<String>>)>)>> = LazyLock::new(|| Mutex::new((Instant::now() + RATE_LIMIT, HashMap::default())));

#[derive(Debug, thiserror::Error)]
enum UncachedError {
    #[error(transparent)] OAuth(#[from] yup_oauth2::Error),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("empty token is not valid")]
    EmptyToken,
    #[error("OAuth token is expired")]
    TokenExpired,
}

impl IsNetworkError for UncachedError {
    fn is_network_error(&self) -> bool {
        match self {
            Self::OAuth(_) => false,
            Self::Reqwest(e) => e.is_network_error(),
            Self::Wheel(e) => e.is_network_error(),
            Self::EmptyToken => false,
            Self::TokenExpired => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{source}")]
pub(crate) struct Error {
    source: UncachedError,
    cache: CacheMissReason,
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        self.source.is_network_error()
    }
}

#[derive(Debug)]
enum CacheMissReason {
    Elapsed,
    Vacant,
}

pub(crate) async fn values(http_client: reqwest::Client, sheet_id: &str, range: &str) -> Result<Vec<Vec<String>>, Error> {
    #[derive(Deserialize)]
    struct ValueRange {
        values: Vec<Vec<String>>,
    }

    async fn values_uncached(http_client: &reqwest::Client, sheet_id: &str, range: &str, next_request: &mut Instant) -> Result<Vec<Vec<String>>, UncachedError> {
        sleep_until(*next_request).await;
        let gsuite_secret = read_service_account_key("assets/google-client-secret.json").await.at("assets/google-client-secret.json")?;
        let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
            .build().await.at_unknown()?;
        let token = auth.token(&["https://www.googleapis.com/auth/spreadsheets"]).await?;
        if token.is_expired() { return Err(UncachedError::TokenExpired) }
        let Some(token) = token.token() else { return Err(UncachedError::EmptyToken) };
        if token.is_empty() { return Err(UncachedError::EmptyToken) }
        let ValueRange { values } = http_client.get(&format!("https://sheets.googleapis.com/v4/spreadsheets/{sheet_id}/values/{range}"))
            .bearer_auth(token)
            .query(&[
                ("valueRenderOption", "FORMATTED_VALUE"),
                ("dateTimeRenderOption", "FORMATTED_STRING"),
                ("majorDimension", "ROWS"),
            ])
            .send().await?
            .detailed_error_for_status().await?
            .json_with_text_in_error::<ValueRange>().await?;
        *next_request = Instant::now() + RATE_LIMIT;
        Ok(values)
    }

    let key = (sheet_id.to_owned(), range.to_owned());
    lock!(cache = CACHE; {
        let (ref mut next_request, ref mut cache) = *cache;
        Ok(match cache.entry(key) {
            hash_map::Entry::Occupied(mut entry) => {
                let (retrieved, values) = entry.get();
                if retrieved.elapsed() < Duration::from_secs(5 * 60) {
                    values.clone()
                } else {
                    match values_uncached(&http_client, sheet_id, range, next_request).await {
                        Ok(values) => {
                            entry.insert((Instant::now(), values.clone()));
                            values
                        }
                        Err(e) if e.is_network_error() && retrieved.elapsed() < Duration::from_secs(60 * 60) => values.clone(),
                        Err(source) => return Err(Error { cache: CacheMissReason::Elapsed, source }),
                    }
                }
            }
            hash_map::Entry::Vacant(entry) => {
                let values = values_uncached(&http_client, sheet_id, range, next_request).await.map_err(|source| Error { cache: CacheMissReason::Vacant, source })?;
                entry.insert((Instant::now(), values.clone()));
                values
            }
        })
    })
}

// ============================================================================
// Write Operations for ZSR Export
// ============================================================================

/// Rate limiter for write operations (separate from read cache)
static WRITE_RATE_LIMIT: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));

/// Error type for write operations (no caching involved)
#[derive(Debug, thiserror::Error)]
pub(crate) enum WriteError {
    #[error(transparent)] OAuth(#[from] yup_oauth2::Error),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("empty token is not valid")]
    EmptyToken,
    #[error("OAuth token is expired")]
    TokenExpired,
}

impl IsNetworkError for WriteError {
    fn is_network_error(&self) -> bool {
        match self {
            Self::OAuth(_) => false,
            Self::Reqwest(e) => e.is_network_error(),
            Self::Wheel(e) => e.is_network_error(),
            Self::EmptyToken => false,
            Self::TokenExpired => false,
        }
    }
}

/// Get OAuth token for Google Sheets API
async fn get_auth_token() -> Result<String, WriteError> {
    let gsuite_secret = read_service_account_key("assets/google-client-secret.json").await.at("assets/google-client-secret.json")?;
    let auth = ServiceAccountAuthenticator::builder(gsuite_secret)
        .build().await.at_unknown()?;
    let token = auth.token(&["https://www.googleapis.com/auth/spreadsheets"]).await?;
    if token.is_expired() { return Err(WriteError::TokenExpired) }
    let Some(token_str) = token.token() else { return Err(WriteError::EmptyToken) };
    if token_str.is_empty() { return Err(WriteError::EmptyToken) }
    Ok(token_str.to_owned())
}

/// Update values in a specific range (overwrites existing data)
pub(crate) async fn update_values(
    http_client: &reqwest::Client,
    sheet_id: &str,
    range: &str,
    values: Vec<Vec<String>>,
) -> Result<(), WriteError> {
    lock!(next_write = WRITE_RATE_LIMIT; {
        sleep_until(*next_write).await;

        let token = get_auth_token().await?;

        #[derive(Serialize)]
        struct ValueRange {
            range: String,
            values: Vec<Vec<String>>,
        }

        http_client.put(&format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{sheet_id}/values/{range}"
        ))
            .bearer_auth(&token)
            .query(&[("valueInputOption", "USER_ENTERED")])
            .json(&ValueRange {
                range: range.to_owned(),
                values,
            })
            .send().await?
            .detailed_error_for_status().await?;

        *next_write = Instant::now() + RATE_LIMIT;
        Ok(())
    })
}

/// Response from append operation
#[derive(Debug, Deserialize)]
pub(crate) struct AppendResponse {
    #[serde(rename = "updates")]
    pub(crate) updates: AppendUpdates,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AppendUpdates {
    #[serde(rename = "updatedRange")]
    pub(crate) updated_range: String,
    #[serde(rename = "updatedRows")]
    pub(crate) updated_rows: i32,
}

/// Append values to a sheet (adds new rows)
pub(crate) async fn append_values(
    http_client: &reqwest::Client,
    sheet_id: &str,
    range: &str,
    values: Vec<Vec<String>>,
) -> Result<AppendResponse, WriteError> {
    lock!(next_write = WRITE_RATE_LIMIT; {
        sleep_until(*next_write).await;

        let token = get_auth_token().await?;

        #[derive(Serialize)]
        struct ValueRange {
            values: Vec<Vec<String>>,
        }

        let response = http_client.post(&format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{sheet_id}/values/{range}:append"
        ))
            .bearer_auth(&token)
            .query(&[
                ("valueInputOption", "USER_ENTERED"),
                ("insertDataOption", "INSERT_ROWS"),
            ])
            .json(&ValueRange { values })
            .send().await?
            .detailed_error_for_status().await?
            .json_with_text_in_error::<AppendResponse>().await?;

        *next_write = Instant::now() + RATE_LIMIT;
        Ok(response)
    })
}

/// Read values without caching (for write operations that need fresh data)
pub(crate) async fn read_values_uncached(
    http_client: &reqwest::Client,
    sheet_id: &str,
    range: &str,
) -> Result<Vec<Vec<String>>, WriteError> {
    lock!(next_write = WRITE_RATE_LIMIT; {
        sleep_until(*next_write).await;

        let token = get_auth_token().await?;

        #[derive(Deserialize)]
        struct ValueRange {
            #[serde(default)]
            values: Vec<Vec<String>>,
        }

        let response = http_client.get(&format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{sheet_id}/values/{range}"
        ))
            .bearer_auth(&token)
            .query(&[
                ("valueRenderOption", "FORMATTED_VALUE"),
                ("dateTimeRenderOption", "FORMATTED_STRING"),
                ("majorDimension", "ROWS"),
            ])
            .send().await?
            .detailed_error_for_status().await?
            .json_with_text_in_error::<ValueRange>().await?;

        *next_write = Instant::now() + RATE_LIMIT;
        Ok(response.values)
    })
}

/// Batch update multiple ranges at once
pub(crate) async fn batch_update_values(
    http_client: &reqwest::Client,
    sheet_id: &str,
    data: Vec<(String, Vec<Vec<String>>)>, // (range, values) pairs
) -> Result<(), WriteError> {
    if data.is_empty() {
        return Ok(());
    }

    lock!(next_write = WRITE_RATE_LIMIT; {
        sleep_until(*next_write).await;

        let token = get_auth_token().await?;

        #[derive(Serialize)]
        struct BatchUpdateRequest {
            data: Vec<ValueRange>,
            #[serde(rename = "valueInputOption")]
            value_input_option: String,
        }

        #[derive(Serialize)]
        struct ValueRange {
            range: String,
            values: Vec<Vec<String>>,
        }

        let request = BatchUpdateRequest {
            data: data.into_iter().map(|(range, values)| ValueRange { range, values }).collect(),
            value_input_option: "USER_ENTERED".to_owned(),
        };

        http_client.post(&format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{sheet_id}/values:batchUpdate"
        ))
            .bearer_auth(&token)
            .json(&request)
            .send().await?
            .detailed_error_for_status().await?;

        *next_write = Instant::now() + RATE_LIMIT;
        Ok(())
    })
}
