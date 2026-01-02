use crate::prelude::*;

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

/// Returns a list of races to import. The `phase`, `round`, and `game` fields will be left blank since the data required to fill them in is not
/// provided by the Challonge API, and only one race for each match is imported. The caller is expected to fill in the values for `phase` and
/// `round`, duplicate this race to get as many different games as there should be in the match, and create a single scheduling thread for the match.
pub(crate) async fn races_to_import(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, config: &Config, event: &event::Data<'_>, community: Option<&str>, tournament: &str) -> Result<(Vec<Race>, Vec<(String, ImportSkipReason)>), cal::Error> {
    #[derive(Deserialize)]
    struct Matches {
        data: Vec<Match>,
        links: MatchesLinks,
    }

    #[derive(Deserialize)]
    struct MatchesLinks {
        next: Url,
    }

    #[derive(Deserialize)]
    struct Match {
        id: String,
        relationships: Relationships,
    }

    #[derive(Deserialize)]
    struct Relationships {
        player1: Option<Player>,
        player2: Option<Player>,
    }

    #[derive(Deserialize)]
    struct Player {
        data: PlayerData,
    }

    #[derive(Deserialize)]
    struct PlayerData {
        id: String,
    }

    let mut races = Vec::default();
    let mut skips = Vec::default();
    let mut next_endpoint = if let Some(community) = community {
        format!("https://api.challonge.com/v2/communities/{community}/tournaments/{tournament}/matches.json")
    } else {
        format!("https://api.challonge.com/v2/tournaments/{tournament}/matches.json")
    }.parse()?;
    loop {
        println!("Challonge: Requesting API endpoint {next_endpoint}");
        let Matches { data, links } = http_client.get(next_endpoint)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::CONTENT_TYPE, "application/vnd.api+json")
            .header("Authorization-Type", "v1")
            .header(reqwest::header::AUTHORIZATION, &config.challonge_api_key)
            .send().await?
            .detailed_error_for_status().await?
            .json_with_text_in_error().await?;
        println!("Challonge: Got response from API");
        if data.is_empty() { break }
        for set in data {
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
                    round: None,
                    game: None,
                    scheduling_thread: None,
                    schedule: RaceSchedule::Unscheduled,
                    schedule_updated_at: None,
                    fpa_invoked: false,
                    breaks_used: false,
                    draft: if let Some(draft_kind) = event.draft_kind() {
                        Some(Draft::for_game1(transaction, http_client, draft_kind, event, None, [&team1, &team2]).await?)
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
                });
            }
        }
        next_endpoint = links.next;
    }
    Ok((races, skips))
}
