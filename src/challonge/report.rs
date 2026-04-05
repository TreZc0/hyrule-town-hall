use crate::prelude::*;
use super::client::{self, Error};

/// Report a match result to Challonge.
///
/// Uses `PUT /v2/matches/{match_id}/report` with `winner_id` and `scores_csv`.
pub(crate) async fn report_result(
    http_client: &reqwest::Client,
    api_key: &str,
    match_id: &str,
    winner_challonge_id: &str,
    game_scores: &[(u8, u8)],
) -> Result<(), Error> {
    let scores_csv = game_scores.iter()
        .map(|(w, l)| format!("{w}-{l}"))
        .collect::<Vec<_>>()
        .join(",");
    let endpoint = format!("https://api.challonge.com/v2/matches/{match_id}/report");
    let payload = serde_json::json!({
        "match": {
            "winner_id": winner_challonge_id,
            "scores_csv": scores_csv,
        }
    });
    client::rate_limited_request(|| async {
        client::api_request(http_client, reqwest::Method::PUT, &endpoint, api_key)
            .json(&payload)
            .send().await?
            .detailed_error_for_status().await?;
        Ok(())
    }).await
}
