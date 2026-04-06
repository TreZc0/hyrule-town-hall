use {
    std::{
        collections::HashMap,
        future::Future,
        sync::LazyLock,
        time::{Duration, Instant},
    },
    tokio::sync::Mutex,
};
use crate::prelude::*;
use super::types::*;

/// Challonge has a 5000 requests/month limit.
/// Safe daily limit: ~160 requests (5000/31)
const SAFE_DAILY_LIMIT: u32 = 500;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(100);

const PARTICIPANTS_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const MATCHES_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

struct RateLimiter {
    daily_count: u32,
    daily_reset: Instant,
    last_request: Instant,
}

static RATE_LIMITER: LazyLock<Mutex<RateLimiter>> = LazyLock::new(|| {
    Mutex::new(RateLimiter {
        daily_count: 0,
        daily_reset: Instant::now() + Duration::from_secs(24 * 60 * 60),
        last_request: Instant::now() - MIN_REQUEST_INTERVAL,
    })
});

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("daily Challonge API budget exceeded ({0}/{SAFE_DAILY_LIMIT} requests)")]
    DailyBudgetExceeded(u32),
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Reqwest(e) => e.is_network_error(),
            Self::Wheel(e) => e.is_network_error(),
            Self::Sql(_) | Self::Url(_) | Self::DailyBudgetExceeded(_) => false,
        }
    }
}

pub(crate) async fn rate_limited_request<F, Fut, T>(request_fn: F) -> Result<T, Error>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut limiter = RATE_LIMITER.lock().await;
    if Instant::now() >= limiter.daily_reset {
        limiter.daily_count = 0;
        limiter.daily_reset = Instant::now() + Duration::from_secs(24 * 60 * 60);
    }
    if limiter.daily_count >= SAFE_DAILY_LIMIT {
        return Err(Error::DailyBudgetExceeded(limiter.daily_count));
    }
    let elapsed = limiter.last_request.elapsed();
    if elapsed < MIN_REQUEST_INTERVAL {
        sleep(MIN_REQUEST_INTERVAL - elapsed).await;
    }
    limiter.daily_count += 1;
    limiter.last_request = Instant::now();
    drop(limiter);
    request_fn().await
}

/// Build a Challonge API request with standard headers.
pub(crate) fn api_request(http_client: &reqwest::Client, method: reqwest::Method, url: impl reqwest::IntoUrl, api_key: &str) -> reqwest::RequestBuilder {
    http_client.request(method, url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/vnd.api+json")
        .header("Authorization-Type", "v1")
        .header(reqwest::header::AUTHORIZATION, api_key)
}

/// Build the base URL for a tournament-scoped API endpoint.
pub(crate) fn tournament_url(community: Option<&str>, tournament: &str, resource: &str) -> String {
    if let Some(community) = community {
        format!("https://api.challonge.com/v2/communities/{community}/tournaments/{tournament}/{resource}.json")
    } else {
        format!("https://api.challonge.com/v2/tournaments/{tournament}/{resource}.json")
    }
}

// === Cache ===

struct CacheEntry<T> {
    data: T,
    retrieved_at: Instant,
}

static PARTICIPANTS_CACHE: LazyLock<Mutex<HashMap<String, CacheEntry<Vec<Participant>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static MATCHES_CACHE: LazyLock<Mutex<HashMap<String, CacheEntry<Vec<Match>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache_key(community: Option<&str>, tournament: &str) -> String {
    match community {
        Some(c) => format!("{c}/{tournament}"),
        None => tournament.to_owned(),
    }
}

pub(crate) async fn cached_participants(community: Option<&str>, tournament: &str) -> Option<Vec<Participant>> {
    let key = cache_key(community, tournament);
    let cache = PARTICIPANTS_CACHE.lock().await;
    cache.get(&key).and_then(|entry| {
        if entry.retrieved_at.elapsed() < PARTICIPANTS_CACHE_TTL {
            Some(entry.data.clone())
        } else {
            None
        }
    })
}

pub(crate) async fn store_participants(community: Option<&str>, tournament: &str, data: Vec<Participant>) {
    let key = cache_key(community, tournament);
    PARTICIPANTS_CACHE.lock().await.insert(key, CacheEntry { data, retrieved_at: Instant::now() });
}

pub(crate) async fn cached_matches(community: Option<&str>, tournament: &str, state: Option<&str>) -> Option<Vec<Match>> {
    let key = format!("{}:{}", cache_key(community, tournament), state.unwrap_or("all"));
    let cache = MATCHES_CACHE.lock().await;
    cache.get(&key).and_then(|entry| {
        if entry.retrieved_at.elapsed() < MATCHES_CACHE_TTL {
            Some(entry.data.clone())
        } else {
            None
        }
    })
}

pub(crate) async fn store_matches(community: Option<&str>, tournament: &str, state: Option<&str>, data: Vec<Match>) {
    let key = format!("{}:{}", cache_key(community, tournament), state.unwrap_or("all"));
    MATCHES_CACHE.lock().await.insert(key, CacheEntry { data, retrieved_at: Instant::now() });
}
