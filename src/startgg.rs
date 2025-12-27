use {
    graphql_client::GraphQLQuery,
    typemap_rev::TypeMap,
    crate::prelude::*,
    std::collections::{HashMap, HashSet},
};

/// From https://dev.start.gg/docs/rate-limits:
///
/// > You may not average more than 80 requests per 60 seconds.
const RATE_LIMIT: Duration = Duration::from_millis(60_000 / 80);

static CACHE: LazyLock<Mutex<(Instant, TypeMap)>> = LazyLock::new(|| Mutex::new((Instant::now() + RATE_LIMIT, TypeMap::default())));

struct QueryCache<T: GraphQLQuery> {
    _phantom: PhantomData<T>,
}

impl<T: GraphQLQuery + 'static> TypeMapKey for QueryCache<T>
where T::Variables: Send + Sync, T::ResponseData: Send + Sync {
    type Value = HashMap<T::Variables, (Instant, T::ResponseData)>;
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("{} GraphQL errors", .0.len())]
    GraphQL(Vec<graphql_client::Error>),
    #[error("GraphQL response returned neither `data` nor `errors`")]
    NoDataNoErrors,
    #[error("no match on query, got {0:?}")]
    NoQueryMatch(event_sets_query::ResponseData),
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Reqwest(e) => e.is_network_error(),
            Self::Wheel(e) => e.is_network_error(),
            Self::GraphQL(errors) => errors.iter().all(|graphql_client::Error { message, .. }| message == "An unknown error has occurred"),
            Self::NoDataNoErrors | Self::NoQueryMatch(_) => false,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum IdInner {
    Number(serde_json::Number),
    String(String),
}

impl From<IdInner> for ID {
    fn from(inner: IdInner) -> Self {
        Self(match inner {
            IdInner::Number(n) => n.to_string(),
            IdInner::String(s) => s,
        })
    }
}

/// Workaround for <https://github.com/smashgg/developer-portal/issues/171>
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, sqlx::Type)]
#[serde(from = "IdInner", into = "String")]
#[sqlx(transparent)]
pub struct ID(pub(crate) String);

impl fmt::Display for ID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<ID> for String {
    fn from(ID(s): ID) -> Self {
        s
    }
}

type Int = i64;
type String = std::string::String;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "assets/graphql/startgg-schema.json",
    query_path = "assets/graphql/startgg-current-user-query.graphql",
    skip_default_scalars, // workaround for https://github.com/smashgg/developer-portal/issues/171
    variables_derives = "Clone, PartialEq, Eq, Hash",
    response_derives = "Debug, Clone",
)]
pub(crate) struct CurrentUserQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "assets/graphql/startgg-schema.json",
    query_path = "assets/graphql/startgg-event-sets-query.graphql",
    skip_default_scalars, // workaround for https://github.com/smashgg/developer-portal/issues/171
    variables_derives = "Clone, PartialEq, Eq, Hash",
    response_derives = "Debug, Clone",
)]
pub(crate) struct EventSetsQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "assets/graphql/startgg-schema.json",
    query_path = "assets/graphql/startgg-report-one-game-result-mutation.graphql",
    skip_default_scalars, // workaround for https://github.com/smashgg/developer-portal/issues/171
    variables_derives = "Clone, PartialEq, Eq, Hash",
    response_derives = "Debug, Clone",
)]
pub(crate) struct ReportOneGameResultMutation;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "assets/graphql/startgg-schema.json",
    query_path = "assets/graphql/startgg-user-slug-query.graphql",
    skip_default_scalars, // workaround for https://github.com/smashgg/developer-portal/issues/171
    variables_derives = "Clone, PartialEq, Eq, Hash",
    response_derives = "Debug, Clone",
)]
pub(crate) struct UserSlugQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "assets/graphql/startgg-schema.json",
    query_path = "assets/graphql/startgg-entrants-query.graphql",
    skip_default_scalars, // workaround for https://github.com/smashgg/developer-portal/issues/171
    variables_derives = "Clone, PartialEq, Eq, Hash",
    response_derives = "Debug, Clone",
)]
pub(crate) struct EntrantsQuery;

async fn query_inner<T: GraphQLQuery + 'static>(http_client: &reqwest::Client, auth_token: &str, variables: T::Variables, next_request: &mut Instant) -> Result<T::ResponseData, Error>
where T::Variables: Clone + Eq + Hash + Send + Sync, T::ResponseData: Clone + Send + Sync {
    sleep_until(*next_request).await;
    let graphql_client::Response { data, errors, extensions: _ } = http_client.post("https://api.start.gg/gql/alpha")
        .bearer_auth(auth_token)
        .json(&T::build_query(variables))
        .send().await?
        .detailed_error_for_status().await?
        .json_with_text_in_error::<graphql_client::Response<T::ResponseData>>().await?;
    *next_request = Instant::now() + RATE_LIMIT;
    match (data, errors) {
        (Some(_), Some(errors)) if !errors.is_empty() => Err(Error::GraphQL(errors)),
        (Some(data), _) => Ok(data),
        (None, Some(errors)) => Err(Error::GraphQL(errors)),
        (None, None) => Err(Error::NoDataNoErrors),
    }
}

pub(crate) async fn query_uncached<T: GraphQLQuery + 'static>(http_client: &reqwest::Client, auth_token: &str, variables: T::Variables) -> Result<T::ResponseData, Error>
where T::Variables: Clone + Eq + Hash + Send + Sync, T::ResponseData: Clone + Send + Sync {
    lock!(cache = CACHE; {
        let (ref mut next_request, _) = *cache;
        query_inner::<T>(http_client, auth_token, variables, next_request).await
    })
}

pub(crate) async fn query_cached<T: GraphQLQuery + 'static>(http_client: &reqwest::Client, auth_token: &str, variables: T::Variables) -> Result<T::ResponseData, Error>
where T::Variables: Clone + Eq + Hash + Send + Sync, T::ResponseData: Clone + Send + Sync {
    lock!(cache = CACHE; {
        let (ref mut next_request, ref mut cache) = *cache;
        Ok(match cache.entry::<QueryCache<T>>().or_default().entry(variables.clone()) {
            hash_map::Entry::Occupied(mut entry) => {
                let (retrieved, entry) = entry.get_mut();
                if retrieved.elapsed() >= Duration::from_secs(30 * 60) {
                    *entry = query_inner::<T>(http_client, auth_token, variables, next_request).await?;
                    *retrieved = Instant::now();
                }
                entry.clone()
            }
            hash_map::Entry::Vacant(entry) => {
                let data = query_inner::<T>(http_client, auth_token, variables, next_request).await?;
                entry.insert((Instant::now(), data.clone()));
                data
            }
        })
    })
}

pub(crate) enum ImportSkipReason {
    Exists,
    Preview,
    Slots,
    SetGamesType,
}

impl fmt::Display for ImportSkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists => write!(f, "already exists"),
            Self::Preview => write!(f, "is a preview"),
            Self::Slots => write!(f, "no match on slots"),
            Self::SetGamesType => write!(f, "unknown games type"),
        }
    }
}

/// Returns:
///
/// * A list of races to import. Only one race for each match is imported, with the `game` field specifying the total number of games in the match.
///   The caller is expected to duplicate this race to get the different games of the match, and create a single scheduling thread for the match.
///   A `game` value of `None` should be treated like `Some(1)`.
/// * A list of start.gg set IDs that were not imported, along with the reasons they were skipped.
pub(crate) async fn races_to_import(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, config: &Config, event: &event::Data<'_>, event_slug: &str) -> Result<(Vec<Race>, Vec<(ID, ImportSkipReason)>), cal::Error> {
    async fn process_set(
        transaction: &mut Transaction<'_, Postgres>,
        http_client: &reqwest::Client,
        event: &event::Data<'_>,
        races: &mut Vec<Race>,
        startgg_event: &str,
        set: ID,
        phase: Option<String>,
        round: Option<String>,
        team1: Team,
        team2: Team,
        set_games_type: Option<i64>,
        total_games: Option<i64>,
        best_of: Option<i64>,
    ) -> Result<Option<ImportSkipReason>, cal::Error> {
        races.push(Race {
            id: Id::new(&mut *transaction).await?,
            series: event.series,
            event: event.event.to_string(),
            source: cal::Source::StartGG {
                event: startgg_event.to_owned(),
                set,
            },
            entrants: Entrants::Two([
                Entrant::MidosHouseTeam(team1.clone()),
                Entrant::MidosHouseTeam(team2.clone()),
            ]),
            game: match set_games_type {
                Some(1) => best_of.map(|best_of| best_of.try_into().expect("too many games")),
                Some(2) => total_games.map(|total_games| total_games.try_into().expect("too many games")),
                _ => return Ok(Some(ImportSkipReason::SetGamesType)),
            },
            scheduling_thread: None,
            schedule: RaceSchedule::Unscheduled,
            schedule_updated_at: None,
            fpa_invoked: false,
            breaks_used: false,
            draft: if let Some(draft_kind) = event.draft_kind() {
                Some(Draft::for_game1(&mut *transaction, http_client, draft_kind, event, phase.as_deref(), [&team1, &team2]).await?)
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
            phase, 
            round,
        });
        Ok(None)
    }

    async fn process_page(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, config: &Config, event: &event::Data<'_>, event_slug: &str, page: i64, races: &mut Vec<Race>, skips: &mut Vec<(ID, ImportSkipReason)>) -> Result<i64, cal::Error> {
        let response = query_cached::<EventSetsQuery>(http_client, &config.startgg, event_sets_query::Variables { event_slug: event_slug.to_owned(), page }).await?;
        let event_sets_query::ResponseData {
            event: Some(event_sets_query::EventSetsQueryEvent {
                sets: Some(event_sets_query::EventSetsQueryEventSets {
                    page_info: Some(event_sets_query::EventSetsQueryEventSetsPageInfo { total_pages: Some(total_pages) }),
                    nodes: Some(sets),
                }),
            }),
        } = response else { return Err(Error::NoQueryMatch(response).into()) };
        for set in sets.into_iter().filter_map(identity) {
            let event_sets_query::EventSetsQueryEventSetsNodes { id: Some(id), phase_group, full_round_text, slots: Some(slots), set_games_type, total_games, round } = set else { panic!("unexpected set format") };
            if id.0.starts_with("preview") {
                skips.push((id, ImportSkipReason::Preview));
            } else if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE startgg_set = $1) AS "exists!""#, id as _).fetch_one(&mut **transaction).await? {
                skips.push((id, ImportSkipReason::Exists));
            } else if let [
                Some(event_sets_query::EventSetsQueryEventSetsNodesSlots { entrant: Some(event_sets_query::EventSetsQueryEventSetsNodesSlotsEntrant { id: Some(ref team1) }) }),
                Some(event_sets_query::EventSetsQueryEventSetsNodesSlots { entrant: Some(event_sets_query::EventSetsQueryEventSetsNodesSlotsEntrant { id: Some(ref team2) }) }),
            ] = *slots {
                let team1 = Team::from_startgg(&mut *transaction, team1).await?.ok_or_else(|| cal::Error::UnknownTeamStartGG(team1.clone()))?;
                let team2 = Team::from_startgg(&mut *transaction, team2).await?.ok_or_else(|| cal::Error::UnknownTeamStartGG(team2.clone()))?;
                let best_of = phase_group.as_ref()
                    .and_then(|event_sets_query::EventSetsQueryEventSetsNodesPhaseGroup { rounds, .. }| rounds.as_ref())
                    .and_then(|rounds| rounds.iter().filter_map(Option::as_ref).find(|event_sets_query::EventSetsQueryEventSetsNodesPhaseGroupRounds { number, .. }| *number == round))
                    .and_then(|event_sets_query::EventSetsQueryEventSetsNodesPhaseGroupRounds { best_of, .. }| *best_of);
                let phase = phase_group.as_ref()
                    .and_then(|event_sets_query::EventSetsQueryEventSetsNodesPhaseGroup { phase, .. }| phase.as_ref())
                    .and_then(|event_sets_query::EventSetsQueryEventSetsNodesPhaseGroupPhase { name }| name.clone());
                if let Some(reason) = process_set(&mut *transaction, http_client, event, races, event_slug, id.clone(), phase, full_round_text, team1, team2, set_games_type, total_games, best_of).await? {
                    skips.push((id, reason));
                }
            } else {
                skips.push((id, ImportSkipReason::Slots));
            }
        }
        Ok(total_pages)
    }

    let mut races = Vec::default();
    let mut skips = Vec::default();
    let total_pages = process_page(&mut *transaction, http_client, config, event, event_slug, 1, &mut races, &mut skips).await?;
    for page in 2..=total_pages {
        process_page(&mut *transaction, http_client, config, event, event_slug, page, &mut races, &mut skips).await?;
    }
    Ok((races, skips))
}

/// Fetches all entrants for a given event slug
pub(crate) async fn fetch_event_entrants(http_client: &reqwest::Client, config: &Config, event_slug: &str) -> Result<Vec<(ID, String, Vec<Option<ID>>)>, Error> {
    let startgg_token = &config.startgg;
    let mut all_entrants = Vec::new();
    let mut page = 1;
    
    loop {
        let response = query_uncached::<EntrantsQuery>(http_client, startgg_token, entrants_query::Variables { 
            slug: Some(event_slug.to_owned()), 
            page: Some(page)
        }).await?;
        
        let entrants_query::ResponseData {
            event: Some(entrants_query::EntrantsQueryEvent {
                entrants: Some(entrants_query::EntrantsQueryEventEntrants {
                    page_info: Some(entrants_query::EntrantsQueryEventEntrantsPageInfo { 
                        page: Some(current_page), 
                        total_pages: Some(total_pages) 
                    }),
                    nodes: Some(entrants),
                }), id: _,
            }),
        } = response else { return Err(Error::GraphQL(vec![graphql_client::Error { message: "Entrants query failed or returned no data".to_string(), locations: None, path: None, extensions: None }])); };
        
        for entrant in entrants.into_iter().filter_map(identity) {
            let entrants_query::EntrantsQueryEventEntrantsNodes { 
                id: Some(entrant_id), 
                name: Some(entrant_name), 
                participants: Some(participants),
                paginated_sets: _,
            } = entrant else { continue };
            
            let user_ids: Vec<Option<ID>> = participants.into_iter()
                .filter_map(identity)
                .map(|participant| {
                    let entrants_query::EntrantsQueryEventEntrantsNodesParticipants { 
                        user: Some(entrants_query::EntrantsQueryEventEntrantsNodesParticipantsUser { 
                            id: Some(user_id) 
                        }) 
                    } = participant else { return None };
                    Some(user_id)
                })
                .collect();
            
            all_entrants.push((entrant_id, entrant_name, user_ids));
        }
        
        if current_page >= total_pages {
            break;
        }
        page += 1;
    }
    
    Ok(all_entrants)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SwissStanding {
    pub placement: usize,
    pub name: String,
    pub wins: usize,
    pub losses: usize,
}

/// Computes Swiss standings for a Startgg Swiss event
pub(crate) async fn swiss_standings(
    http_client: &reqwest::Client,
    _config: &Config,
    event_slug: &str,
    startgg_token: &str,
    resigned_entrant_ids: Option<&std::collections::HashSet<String>>,
) -> Result<Vec<SwissStanding>, Error> {
    use entrants_query::EntrantsQueryEventEntrantsNodesPaginatedSetsNodes as SetNode;
    use entrants_query::EntrantsQueryEventEntrantsNodesPaginatedSetsNodesPhaseGroup as PhaseGroup;
    use entrants_query::EntrantsQueryEventEntrantsNodes as EntrantNode;
    use entrants_query::EntrantsQueryEventEntrantsPageInfo as PageInfo;
    use entrants_query::EntrantsQueryEventEntrants as Entrants;
    use entrants_query::EntrantsQueryEvent as Event;
    use entrants_query::ResponseData as ResponseData;
    use event_sets_query::EventSetsQueryEventSetsNodes as EventSetNode;
    use event_sets_query::EventSetsQueryEventSetsNodesPhaseGroup as EventSetPhaseGroup;
    use event_sets_query::EventSetsQueryEventSets as EventSets;
    use event_sets_query::EventSetsQueryEvent as EventSetsEvent;
    use event_sets_query::ResponseData as EventSetsResponseData;

    // Helper function to fetch remaining pages sequentially
    async fn fetch_remaining_pages<T>(
        http_client: &reqwest::Client,
        startgg_token: &str,
        total_pages: i64,
        page_vars_fn: impl Fn(i64) -> T::Variables + Send + Sync,
        mut process_response: impl FnMut(&T::ResponseData) + Send + Sync,
    ) -> Result<(), Error>
    where
        T: GraphQLQuery + 'static,
        T::Variables: Clone + Eq + Hash + Send + Sync,
        T::ResponseData: Clone + Send + Sync,
    {
        if total_pages > 1 {
            log::info!("Fetching {} additional pages sequentially", total_pages - 1);
            for page in 2..=total_pages {
                let response = query_cached::<T>(http_client, startgg_token, page_vars_fn(page)).await?;
                process_response(&response);
            }
        }
        Ok(())
    }
    
    // First, get tournament structure to understand what rounds should exist
    let mut swiss_rounds = HashSet::<String>::new();
    
    // Fetch all event sets pages
    let event_sets_response = query_cached::<EventSetsQuery>(
        http_client,
        startgg_token,
        event_sets_query::Variables {
            event_slug: event_slug.to_owned(),
            page: 1,
        },
    )
    .await?;
    
    let total_event_pages = if let EventSetsResponseData {
        event: Some(EventSetsEvent {
            sets: Some(EventSets {
                page_info: Some(event_sets_query::EventSetsQueryEventSetsPageInfo { total_pages: Some(tp) }),
                nodes: Some(sets),
            }),
            ..
        }),
        ..
    } = event_sets_response {
        // Process first page
        for set in sets.into_iter().filter_map(|s| s) {
            let EventSetNode {
                phase_group: Some(EventSetPhaseGroup { 
                    phase: Some(phase),
                    rounds: Some(_rounds),
                    ..
                }),
                full_round_text: Some(round_text),
                ..
            } = set else { continue };
            
            // Check if this is a Swiss phase
            if phase.name == Some("Swiss".to_string()) {
                swiss_rounds.insert(round_text);
            }
        }
        tp
    } else {
        return Ok(Vec::new());
    };

    // Fetch remaining event sets pages
    fetch_remaining_pages::<EventSetsQuery>(
        http_client,
        startgg_token,
        total_event_pages,
        |page| event_sets_query::Variables {
            event_slug: event_slug.to_owned(),
            page,
        },
        |response| {
            let EventSetsResponseData {
                event: Some(EventSetsEvent {
                    sets: Some(EventSets {
                        nodes: Some(sets),
                        ..
                    }),
                    ..
                }),
                ..
            } = response else { return };
            
            for set in sets.iter().filter_map(|s| s.as_ref()) {
                let EventSetNode {
                    phase_group: Some(EventSetPhaseGroup { 
                        phase: Some(phase),
                        rounds: Some(_rounds),
                        ..
                    }),
                    full_round_text: Some(round_text),
                    ..
                } = set else { continue };
                
                // Check if this is a Swiss phase
                if phase.name == Some("Swiss".to_string()) {
                    swiss_rounds.insert(round_text.clone());
                }
            }
        },
    )
    .await?;

    // Now get all entrants and their actual matches
    let mut all_entrants = Vec::new();
    let mut entrant_matches = std::collections::HashMap::new();
    
    // Fetch all entrants pages
    let entrants_response = query_cached::<EntrantsQuery>(
        http_client,
        startgg_token,
        entrants_query::Variables {
            slug: Some(event_slug.to_owned()),
            page: Some(1),
        },
    )
    .await?;
    
    let total_entrant_pages = if let ResponseData {
        event: Some(Event {
            entrants: Some(Entrants {
                page_info: Some(PageInfo { page: Some(_), total_pages: Some(tp) }),
                nodes: Some(entrants),
            }),
            ..
        }),
        ..
    } = entrants_response {
        // Process first page
        for entrant in entrants.into_iter().filter_map(|e| e) {
            let EntrantNode {
                id: Some(entrant_id),
                name: Some(entrant_name),
                paginated_sets: Some(paginated_sets),
                ..
            } = entrant else { continue };
            
            let mut wins = 0;
            let mut losses = 0;
            let mut total_matches = 0; // Track ALL matches (including null winnerId)
            
            if let Some(set_nodes) = paginated_sets.nodes {
                for set in set_nodes.into_iter().filter_map(|s| s) {
                    let SetNode {
                        winner_id,
                        phase_group: Some(PhaseGroup { bracket_type, .. }),
                        ..
                    } = set else { continue };
                    
                    if bracket_type != Some(entrants_query::BracketType::SWISS) { continue; }
                    
                    // Count ALL matches (including null winnerId)
                    total_matches += 1;
                    
                    // Count wins/losses based on winner_id
                    match winner_id {
                        Some(wid) if wid.to_string() == entrant_id.to_string() => wins += 1,
                        Some(_) => losses += 1,
                        None => {}, // winnerId null means the match hasn't been played yet
                    }
                }
            }
            
            // Store the matches for this entrant
            entrant_matches.insert(entrant_id.to_string(), (entrant_name.clone(), wins, losses, total_matches));
            all_entrants.push((entrant_name, wins, losses));
        }
        tp
    } else {
        return Ok(Vec::new());
    };

    // Fetch remaining entrants pages
    fetch_remaining_pages::<EntrantsQuery>(
        http_client,
        startgg_token,
        total_entrant_pages,
        |page| entrants_query::Variables {
            slug: Some(event_slug.to_owned()),
            page: Some(page),
        },
        |response| {
            let ResponseData {
                event: Some(Event {
                    entrants: Some(Entrants {
                        nodes: Some(entrants),
                        ..
                    }),
                    ..
                }),
                ..
            } = response else { return };
            
            for entrant in entrants.iter().filter_map(|e| e.as_ref()) {
                let EntrantNode {
                    id: Some(entrant_id),
                    name: Some(entrant_name),
                    paginated_sets: Some(paginated_sets),
                    ..
                } = entrant else { continue };
                
                let mut wins = 0;
                let mut losses = 0;
                let mut total_matches = 0; // Track ALL matches (including null winnerId)
                
                if let Some(set_nodes) = &paginated_sets.nodes {
                    for set in set_nodes.iter().filter_map(|s| s.as_ref()) {
                        let SetNode {
                            winner_id,
                            phase_group: Some(PhaseGroup { bracket_type, .. }),
                            ..
                        } = set else { continue };
                        
                        if *bracket_type != Some(entrants_query::BracketType::SWISS) { continue; }
                        
                        // Count ALL matches (including null winnerId)
                        total_matches += 1;
                        
                        // Count wins/losses based on winner_id
                        match winner_id {
                            Some(wid) if wid.to_string() == entrant_id.to_string() => wins += 1,
                            Some(_) => losses += 1,
                            None => {}, // winnerId null means the match hasn't been played yet
                        }
                    }
                }
                
                // Store the matches for this entrant
                entrant_matches.insert(entrant_id.to_string(), (entrant_name.clone(), wins, losses, total_matches));
                all_entrants.push((entrant_name.clone(), wins, losses));
            }
        },
    )
    .await?;

    // Now apply the correct bye detection logic
    let expected_rounds = swiss_rounds.len();
    if expected_rounds > 0 {
        for (entrant_id, (name, wins, losses, total_matches)) in &mut entrant_matches {
            // Skip bye prediction for resigned entrants
            if let Some(resigned_ids) = resigned_entrant_ids {
                if resigned_ids.contains(entrant_id) {
                    continue;
                }
            }
            
            // Only apply byes if the total number of matches is less than expected rounds
            // This indicates that some matches were wiped from the API
            if *total_matches < expected_rounds {
                let missing_matches = expected_rounds - *total_matches;
                *wins += missing_matches;
                
                // Update the all_entrants list
                if let Some(entrant) = all_entrants.iter_mut().find(|(n, _, _)| n == name) {
                    entrant.1 = *wins;
                    entrant.2 = *losses;
                }
            }
        }
    }

    // Sort: wins desc, losses asc, name asc
    all_entrants.sort_by(|a, b| {
        b.1.cmp(&a.1) // wins desc
            .then(a.2.cmp(&b.2)) // losses asc
            .then(a.0.cmp(&b.0)) // name asc
    });
    
    // Assign placements with ties
    let mut standings = Vec::new();
    let mut last_wins = None;
    let mut last_losses = None;
    let mut placement = 1;
    for (i, (name, wins, losses)) in all_entrants.iter().enumerate() {
        if i == 0 {
            last_wins = Some(*wins);
            last_losses = Some(*losses);
        } else if *wins != last_wins.unwrap() || *losses != last_losses.unwrap() {
            placement = i + 1;
            last_wins = Some(*wins);
            last_losses = Some(*losses);
        }
        standings.push(SwissStanding {
            placement,
            name: name.clone(),
            wins: *wins,
            losses: *losses,
        });
    }
    Ok(standings)
}
