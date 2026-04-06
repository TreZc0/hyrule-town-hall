use crate::prelude::*;
use super::client::{self, Error};
use super::types::*;

pub(crate) enum ImportSkipReason {
    Exists,
    Player1,
    Player2,
    // can't include more info because showCommunityParticipant endpoint returns 404
    UnknownTeam(String),
}

impl fmt::Display for ImportSkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists => write!(f, "already exists"),
            Self::Player1 => write!(f, "no player 1"),
            Self::Player2 => write!(f, "no player 2"),
            Self::UnknownTeam(id) => write!(f, "Challonge team ID {id} is not associated with a Hyrule Town Hall team"),
        }
    }
}

/// Fetches all participants for a tournament, handling pagination.
pub(crate) async fn fetch_participants(
    http_client: &reqwest::Client,
    config: &Config,
    community: Option<&str>,
    tournament: &str,
) -> Result<Vec<Participant>, Error> {
    if let Some(cached) = client::cached_participants(community, tournament).await {
        return Ok(cached);
    }
    let mut all = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut next_url: Option<Url> = Some(client::tournament_url(community, tournament, "participants").parse()?);
    for _ in 0..10 {
        let Some(url) = next_url.take() else { break };
        let resp: ParticipantsResponse = client::rate_limited_request(|| async {
            Ok(client::api_request(http_client, reqwest::Method::GET, url.clone(), &config.challonge_api_key)
                .send().await?
                .detailed_error_for_status().await?
                .json_with_text_in_error().await?)
        }).await?;
        if resp.data.is_empty() { break }
        let mut duplicate = false;
        for item in resp.data {
            if !seen_ids.insert(item.id.clone()) {
                duplicate = true;
                break;
            }
            all.push(item);
        }
        if duplicate { break }
        next_url = resp.links.next.filter(|next| next != &url);
    }
    client::store_participants(community, tournament, all.clone()).await;
    Ok(all)
}

/// Fetches all matches for a tournament, optionally filtered by state.
pub(crate) async fn fetch_matches(
    http_client: &reqwest::Client,
    config: &Config,
    community: Option<&str>,
    tournament: &str,
    state: Option<&str>,
) -> Result<Vec<Match>, Error> {
    if let Some(cached) = client::cached_matches(community, tournament, state).await {
        return Ok(cached);
    }
    let mut all = Vec::new();
    let mut seen_ids = HashSet::new();
    let base = client::tournament_url(community, tournament, "matches");
    let mut next_url: Option<Url> = Some({
        let mut url: Url = base.parse()?;
        if let Some(state) = state {
            url.query_pairs_mut().append_pair("state", state);
        }
        url
    });
    for _ in 0..10 {
        let Some(url) = next_url.take() else { break };
        let resp: MatchesResponse = client::rate_limited_request(|| async {
            Ok(client::api_request(http_client, reqwest::Method::GET, url.clone(), &config.challonge_api_key)
                .send().await?
                .detailed_error_for_status().await?
                .json_with_text_in_error().await?)
        }).await?;
        if resp.data.is_empty() { break }
        let mut duplicate = false;
        for item in resp.data {
            if !seen_ids.insert(item.id.clone()) {
                duplicate = true;
                break;
            }
            all.push(item);
        }
        if duplicate { break }
        next_url = resp.links.next.filter(|next| next != &url);
    }
    client::store_matches(community, tournament, state, all.clone()).await;
    Ok(all)
}

/// Creates a participant in a Challonge tournament and returns the participant ID.
///
/// If `challonge_username` is provided, the participant is linked to that Challonge account.
pub(crate) async fn create_participant(
    http_client: &reqwest::Client,
    config: &Config,
    community: Option<&str>,
    tournament: &str,
    name: &str,
    challonge_username: Option<&str>,
) -> Result<String, Error> {
    let url = client::tournament_url(community, tournament, "participants");
    let mut attributes = serde_json::json!({ "name": name });
    if let Some(username) = challonge_username {
        attributes["username"] = serde_json::Value::String(username.to_owned());
    }
    let payload = serde_json::json!({
        "data": {
            "type": "Participants",
            "attributes": attributes,
        }
    });
    let resp: serde_json::Value = client::rate_limited_request(|| async {
        Ok(client::api_request(http_client, reqwest::Method::POST, &url, &config.challonge_api_key)
            .json(&payload)
            .send().await?
            .detailed_error_for_status().await?
            .json_with_text_in_error().await?)
    }).await?;
    // Extract participant ID from JSONAPI response: { "data": { "id": "123", ... } }
    match &resp["data"]["id"] {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => panic!("Challonge create participant returned unexpected id format: {resp}"),
    }
}

/// Auto-link teams by matching Challonge participant usernames to users' OAuth-linked accounts.
///
/// For each participant with a Challonge `username`, finds the Mido's House user whose
/// `challonge_id` matches that username, then finds that user's team in this event and
/// sets the team's `challonge_id` to the participant's tournament-specific ID.
///
/// Returns the number of teams that were linked.
pub(crate) async fn sync_team_challonge_ids(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    config: &Config,
    series: Series,
    event: &str,
    community: Option<&str>,
    tournament: &str,
) -> Result<usize, Error> {
    let participants = fetch_participants(http_client, config, community, tournament).await?;
    let mut updated = 0;
    for participant in &participants {
        let Some(ref username) = participant.attributes.username else { continue };
        // Find the Mido's House user whose challonge_id matches this participant's Challonge username
        let Some(user_id) = sqlx::query_scalar!(
            r#"SELECT id AS "id: Id<Users>" FROM users WHERE challonge_id = $1"#,
            username,
        ).fetch_optional(&mut **transaction).await? else { continue };
        // Find that user's team in this event and set the team's challonge_id
        let result = sqlx::query!(
            "UPDATE teams SET challonge_id = $1
             WHERE series = $2 AND event = $3 AND challonge_id IS NULL
             AND id IN (SELECT team FROM team_members WHERE member = $4)",
            participant.id,
            series as _,
            event,
            user_id as _,
        ).execute(&mut **transaction).await?;
        if result.rows_affected() > 0 {
            updated += 1;
        }
    }
    Ok(updated)
}

/// Returns a list of races to import. The `phase` and `round` fields are pre-filled from the
/// Challonge API when match attributes are available. The `game` field is left blank since only
/// one race per match is imported. The caller is expected to duplicate the race for multi-game
/// matches and create a single scheduling thread for each match.
pub(crate) async fn races_to_import(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, config: &Config, event: &event::Data<'_>, community: Option<&str>, tournament: &str) -> Result<(Vec<Race>, Vec<(String, ImportSkipReason)>), cal::Error> {
    let matches = fetch_matches(http_client, config, community, tournament, None).await?;
    let mut races = Vec::default();
    let mut skips = Vec::default();
    for set in matches {
        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE challonge_match = $1) AS "exists!""#, set.id).fetch_one(&mut **transaction).await? {
            skips.push((set.id, ImportSkipReason::Exists));
        } else {
            let Some(player1) = set.relationships.player1 else { skips.push((set.id, ImportSkipReason::Player1)); continue };
            let Some(player2) = set.relationships.player2 else { skips.push((set.id, ImportSkipReason::Player2)); continue };
            let Some(team1) = Team::from_challonge(&mut *transaction, &player1.data.id).await? else { skips.push((set.id, ImportSkipReason::UnknownTeam(player1.data.id))); continue };
            let Some(team2) = Team::from_challonge(&mut *transaction, &player2.data.id).await? else { skips.push((set.id, ImportSkipReason::UnknownTeam(player2.data.id))); continue };
            races.push(Race {
                id: Id::new(transaction).await?,
                series: event.series,
                event: event.event.to_string(),
                source: cal::Source::Challonge { id: set.id },
                entrants: Entrants::Two([
                    Entrant::MidosHouseTeam(team1.clone()),
                    Entrant::MidosHouseTeam(team2.clone()),
                ]),
                phase: None,
                round: set.attributes.as_ref().map(|a| {
                    if a.round < 0 {
                        format!("LB Round {}", a.round.abs())
                    } else {
                        format!("WB Round {}", a.round)
                    }
                }),
                game: None,
                scheduling_thread: None,
                schedule: RaceSchedule::Unscheduled,
                schedule_updated_at: None,
                fpa_invoked: false,
                breaks_used: false,
                draft: if let Some(draft_kind) = event.draft_kind() {
                    Some(Draft::for_game1(transaction, http_client, &draft_kind, event, None, [&team1, &team2]).await?)
                } else {
                    None
                },
                seed: seed::Data::default(),
                video_urls: HashMap::default(),
                restreamers: HashMap::default(),
                last_edited_by: None,
                last_edited_at: None,
                ignored: false,
                schedule_locked: false,
                notified: false,
                async_notified_1: false,
                async_notified_2: false,
                async_notified_3: false,
                discord_scheduled_event_id: None,
                volunteer_request_sent: false,
                volunteer_request_message_id: None,
                racetime_goal_slug: None,
            });
        }
    }
    Ok((races, skips))
}
