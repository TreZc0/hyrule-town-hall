use super::*;

fn entrants(race: &Race) -> Vec<Entrant> {
    match &race.entrants {
        Entrants::Two(entrants) => entrants.to_vec(),
        Entrants::Three(entrants) => entrants.to_vec(),
        Entrants::Many(teams) => teams.iter().cloned().map(Entrant::MidosHouseTeam).collect(),
        Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => Vec::new(),
    }
}

// Previous opponents are context, not a second set of profile links.
async fn opponent_name(
    entrant: &Entrant,
    transaction: &mut Transaction<'_, Postgres>,
) -> sqlx::Result<String> {
    Ok(match entrant {
        Entrant::MidosHouseTeam(team) => team
            .name(transaction)
            .await?
            .map_or_else(|| "(unnamed)".to_owned(), |name| name.into_owned()),
        Entrant::Discord { id, .. } => User::from_discord(&mut **transaction, *id)
            .await?
            .map_or_else(|| id.to_string(), |user| user.display_name().to_owned()),
        Entrant::Named { name, .. } => name.clone(),
    })
}

// Names alone are not identities: two unlinked entrants can have the same name.
fn same_entrant(a: &Entrant, b: &Entrant) -> bool {
    match (a, b) {
        (Entrant::MidosHouseTeam(a), Entrant::MidosHouseTeam(b)) => a.id == b.id,
        (Entrant::Discord { id: a, .. }, Entrant::Discord { id: b, .. }) if a == b => true,
        (
            Entrant::Discord {
                racetime_id: Some(a),
                ..
            }
            | Entrant::Named {
                racetime_id: Some(a),
                ..
            },
            Entrant::Discord {
                racetime_id: Some(b),
                ..
            }
            | Entrant::Named {
                racetime_id: Some(b),
                ..
            },
        ) => !a.is_empty() && a == b,
        _ => false,
    }
}

fn same_match(a: &Race, b: &Race) -> bool {
    if a.series != b.series || a.event != b.event {
        return false;
    }
    match (&a.source, &b.source) {
        (Source::StartGG { event: ae, set: a }, Source::StartGG { event: be, set: b }) => {
            ae == be && a == b
        }
        (Source::Challonge { id: a }, Source::Challonge { id: b }) => a == b,
        (Source::Manual, Source::Manual) => {
            a.game.is_some()
                && b.game.is_some()
                && a.phase == b.phase
                && a.round == b.round
                && match (a.scheduling_thread, b.scheduling_thread) {
                    (Some(a), Some(b)) => a == b,
                    _ => a.phase.is_some() || a.round.is_some(),
                }
                && a.entrants == b.entrants
        }
        _ => false,
    }
}

fn public_parts(race: &Race) -> Vec<Event> {
    race.cal_events()
        .filter(|part| race.is_ended() || !part.is_private_async_part())
        .collect()
}

fn previous_race<'a>(
    race: &Race,
    entrant: &Entrant,
    races: &'a [Race],
    now: DateTime<Utc>,
) -> Option<&'a Race> {
    let before = race
        .cal_events()
        .filter_map(|part| part.start())
        .min()
        .unwrap_or(now)
        .min(now);
    races
        .iter()
        .filter(|other| {
            other.id != race.id
                && !other.ignored
                && !other.is_qualifier
                && other.series == race.series
                && other.event == race.event
                && !same_match(race, other)
                && other
                    .schedule
                    .end_time(&other.entrants)
                    .is_some_and(|end| end < before)
                && entrants(other)
                    .iter()
                    .any(|other| same_entrant(entrant, other))
        })
        .max_by_key(|other| (other.schedule.end_time(&other.entrants), other.id))
}

// Only known Twitch channel and VOD URLs become players. Other links remain links.
fn twitch_embed(url: &Url, ended: bool) -> Option<String> {
    if url.scheme() != "https" || !matches!(url.host_str(), Some("twitch.tv" | "www.twitch.tv")) {
        return None;
    }
    let segments = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        ["videos", id] if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => {
            Some(format!("video=v{id}"))
        }
        [channel]
            if !ended
                && !matches!(
                    *channel,
                    "directory" | "downloads" | "settings" | "subscriptions" | "videos"
                )
                && channel
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_') =>
        {
            Some(format!("channel={channel}"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_detail_does_not_collide_with_neighboring_routes() {
        let routes = rocket::routes![
            race_detail,
            create_race,
            import_races,
            import_races_status,
            practice_seed,
            edit_race,
            add_file_hash,
        ];
        for (index, route) in routes.iter().enumerate() {
            for other in &routes[index + 1..] {
                assert!(
                    !route.collides_with(other),
                    "{:?} collides with {:?}",
                    route.name,
                    other.name
                );
            }
        }
        for named in &routes[1..3] {
            assert!(
                named.rank < routes[0].rank,
                "named routes must take precedence"
            );
        }
    }

    #[tokio::test]
    async fn race_detail_and_named_race_routes_ignite_together() {
        // Only ignition is exercised: no real credentials, database, or Discord connection.
        let oauth = serde_json::json!({"clientID": "test", "clientSecret": "test"});
        let config: Config = serde_json::from_value(serde_json::json!({
            "challonge": oauth,
            "challongeApiKey": "test",
            "discord": {"clientID": "1", "clientSecret": "test", "botToken": "test"},
            "leagueApiKey": "test",
            "ootrApiKey": "test",
            "ootrApiKeyEncryption": "test",
            "racetimeBot": oauth,
            "racetimeOAuth": oauth,
            "secretKey": "test",
            "startgg": "test",
            "startggOAuth": oauth
        }))
        .unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1/unused")
            .unwrap();
        rocket::build()
            .manage(pool)
            .manage(config)
            .manage(reqwest::Client::new())
            .manage(RwFuture::new(std::future::pending::<DiscordCtx>()))
            .mount("/", rocket::routes![race_detail, create_race, import_races])
            .ignite()
            .await
            .expect("race detail must coexist with /races/new and /races/import at startup");
    }
}

// Rocket gives both this dynamic path and /races/import the same default rank.
// Keep named race routes ahead of the detail fallback so ignition cannot collide.
#[rocket::get("/event/<series>/<event>/races/<id>", rank = 1)]
pub(crate) async fn race_detail(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: Option<User>,
    uri: Origin<'_>,
    series: Series,
    event: &str,
    id: Id<Races>,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = event::Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    // Check existence and ownership before loading; a mismatched event URL is not a second permalink.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND series = $2 AND event = $3)",
    )
    .bind(id)
    .bind(series.to_string())
    .bind(event)
    .fetch_one(&mut *transaction)
    .await?;
    if id == Id::dummy() || !exists {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    let race = Race::from_id(&mut transaction, http_client, id).await?;
    if race.ignored && !race.is_ended() {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Races, true)
        .await?;
    let now = Utc::now();
    let participants = entrants(&race);
    let mut event_races = Vec::new();
    let mut team_cache = HashMap::new();
    // Local records only: opening a detail page never needs a start.gg request.
    let ids: Vec<Id<Races>> =
        sqlx::query_scalar("SELECT id FROM races WHERE series = $1 AND event = $2 AND NOT ignored")
            .bind(series.to_string())
            .bind(event)
            .fetch_all(&mut *transaction)
            .await?;
    for other_id in ids {
        if other_id != id {
            event_races.push(
                Race::from_id_with_team_cache(&mut transaction, other_id, &mut team_cache).await?,
            );
        }
    }
    let primary = event_races
        .iter()
        .find(|other| other.companion_race_id == Some(id));
    let companion = race
        .companion_race_id
        .and_then(|id| event_races.iter().find(|other| other.id == id));
    let broadcast_race = primary.unwrap_or(&race);
    let mut broadcasts = broadcast_race.video_urls.iter().collect::<Vec<_>>();
    broadcasts.sort_by_key(|(language, _)| **language);
    let is_async = matches!(race.schedule, RaceSchedule::Async { .. });
    let multistream = if broadcasts.is_empty()
        && !is_async
        && !matches!(broadcast_race.schedule, RaceSchedule::Async { .. })
    {
        if matches!(
            broadcast_race.schedule,
            RaceSchedule::Live { room: Some(_), .. }
        ) {
            broadcast_race
                .multistream_url(&mut transaction, http_client, &data)
                .await?
        } else {
            broadcast_race
                .multistream_url_prerace(&mut transaction, http_client, &data)
                .await?
        }
    } else {
        None
    };
    let signups = Signup::for_race(&mut transaction, broadcast_race.id).await?;
    let bindings = EffectiveRoleBinding::for_event(&mut transaction, series, event).await?;
    let mut volunteers = Vec::new();
    for binding in &bindings {
        let mut names = Vec::new();
        for signup in signups.iter().filter(|signup| {
            signup.role_binding_id == binding.id
                && matches!(signup.status, VolunteerSignupStatus::Confirmed)
        }) {
            if let Some(user) = User::from_id(&mut *transaction, signup.user_id).await? {
                names.push(user.to_string());
            }
        }
        if !names.is_empty() {
            volunteers.push((binding, names));
        }
    }
    let settings = if let Some(generator) = data.seed_gen_type.as_ref() {
        generator
            .settings_display_str(&mut transaction, &race, &data.choice_requirements())
            .await
    } else {
        None
    };
    let preset = race.draft.as_ref().and_then(|draft| {
        let preset = draft
            .settings
            .get(&*format!("game{}_preset", race.game.unwrap_or(1)))?;
        data.draft_kind()?
            .preset_display_name(preset.as_ref())
            .map(str::to_owned)
    });
    let drafted_settings = if data.single_settings.is_none() && race.draft.is_some() {
        race.single_settings(&mut transaction).await?
    } else {
        None
    };
    let round_deadlines: Vec<(String, DateTime<Utc>)> = sqlx::query_as("SELECT round, scheduling_deadline FROM event_round_configs WHERE series = $1 AND event = $2 AND scheduling_deadline IS NOT NULL ORDER BY scheduling_deadline, round")
        .bind(series.to_string()).bind(event).fetch_all(&mut *transaction).await?;
    let mut related_games = event_races
        .iter()
        .filter(|other| same_match(&race, other))
        .collect::<Vec<_>>();
    related_games.sort_by_key(|other| (other.game, other.id));
    let rooms = broadcast_race.rooms().collect::<Vec<_>>();
    let bracket = race.startgg_set_url()?;
    // Schedules are public; private room URLs still go through Race::rooms().
    let parts = broadcast_race.cal_events().collect::<Vec<_>>();
    let show_seed = race.show_seed(&mut transaction).await?;
    let title = race.custom_title.as_deref().unwrap_or("Race details");
    let content = html! {
        : header;
        article(class = "race-detail") {
            p { a(href = uri!(event::races(series, event))) : "← All races"; }
            h1 : title;
            p(class = "race-detail-context") {
                : data.display_name;
                @if let Some(phase) = &race.phase { : " · "; : phase; }
                @if let Some(round) = &race.round { : " · "; : round; }
                @if let Some(game) = race.game { : " · Game "; : game; }
            }
            @if is_async {
                div(class = "race-async-notice") {
                    strong : "Async race";
                    p : "Played separately, with an individual schedule for each entrant.";
                }
            }
            div(class = if participants.len() == 2 { "race-matchup race-matchup-two" } else { "race-matchup" }) {
                @for (index, entrant) in participants.iter().enumerate() {
                    @if participants.len() == 2 && index == 1 { span(class = "race-versus") : "vs."; }
                    div(class = "race-entrant") {
                        h2 : entrant.to_html_with_optional_discord(&mut transaction, None, None, false).await?;
                        @if let Some(previous) = previous_race(&race, entrant, &event_races, now) {
                            div(class = "race-previous") {
                                a(href = uri!(race_detail(series, event, previous.id))) : "Previous race";
                                @let previous_entrants = entrants(previous);
                                @let opponents = previous_entrants.iter().filter(|other| !same_entrant(entrant, other)).collect::<Vec<_>>();
                                @let previous_start = public_parts(previous).iter().filter_map(|part| part.start()).min();
                                div(class = "race-previous-meta") {
                                    @for (index, opponent) in opponents.iter().enumerate() {
                                        : if index == 0 { "vs. " } else { ", " };
                                        bdi : opponent_name(opponent, &mut transaction).await?;
                                    }
                                    @if let Some(start) = previous_start {
                                        @if !opponents.is_empty() { : " · "; }
                                        : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                    }
                                    @if let Some(game) = previous.game {
                                        @if !opponents.is_empty() || previous_start.is_some() { : " · "; }
                                        : "Game "; : game;
                                    }
                                }
                            }
                        }
                    }
                }
                @if participants.is_empty() {
                    @match &race.entrants {
                        Entrants::Named(name) => h2 : name;
                        Entrants::Count { total, .. } => h2 : format!("{total} entrants");
                        _ => h2 : "Open race";
                    }
                }
            }
            section(class = "race-detail-section", aria_label = "Schedule") {
                h2 : "Schedule";
                @if is_async {
                    p(class = "race-detail-status") : format!("{} of {} parts scheduled · {} of {} parts completed",
                        parts.iter().filter(|part| part.start().is_some()).count(), parts.len(),
                        parts.iter().filter(|part| part.end().is_some()).count(), parts.len());
                } else {
                    p(class = "race-detail-status") : if race.is_ended() { "Finished" } else if parts.iter().all(|part| part.start().is_none()) { "Not scheduled yet" } else if parts.iter().any(|part| part.start().is_some_and(|start| start <= now)) { "Awaiting completion" } else { "Scheduled" };
                }
                @for part in &parts {
                    p {
                        @if is_async {
                            @let index = match part.kind { EventKind::Async1 => 0, EventKind::Async2 => 1, EventKind::Async3 => 2, EventKind::Normal => 0 };
                            strong {
                                : format!("Async part {}", index + 1);
                                @if let Some(entrant) = participants.get(index) {
                                    : " — "; bdi : opponent_name(entrant, &mut transaction).await?;
                                }
                                : ": ";
                            }
                        }
                        @if let Some(start) = part.start() {
                            : format_datetime(start, DateTimeFormat { long: true, running_text: false });
                        } else { : "Not scheduled yet"; }
                    }
                }
                @if let Some(primary) = primary {
                    p { : "Shared room and broadcast with "; a(href = uri!(race_detail(series, event, primary.id))) : primary.custom_title.as_deref().unwrap_or("the combined race"); : "."; }
                }
                @if let Some(companion) = companion {
                    p { : "Combined with "; a(href = uri!(race_detail(series, event, companion.id))) : companion.custom_title.as_deref().unwrap_or("another race"); : "."; }
                }
                @if let Some(deadline) = race.scheduling_deadline {
                    p { strong : "Scheduling deadline: "; : format_datetime(deadline, DateTimeFormat { long: true, running_text: false }); }
                }
                @if !round_deadlines.is_empty() {
                    details {
                        summary : "Round deadlines";
                        dl {
                            @for (round, deadline) in &round_deadlines {
                                dt : round;
                                dd : format_datetime(*deadline, DateTimeFormat { long: true, running_text: false });
                            }
                        }
                    }
                }
            }
            @if !broadcasts.is_empty() || multistream.is_some() || !rooms.is_empty() || bracket.is_some() {
                section(class = "race-detail-section") {
                    h2 : "Watch & links";
                    div(class = "race-detail-links") {
                        @for (language, url) in &broadcasts {
                            a(class = "button", href = url.as_str(), target = "_blank", rel = "noopener noreferrer") : format!("{language} restream");
                        }
                        @if let Some(url) = &multistream {
                            a(class = "button", href = url.as_str(), target = "_blank", rel = "noopener noreferrer") : "Watch multistream";
                        }
                        @for (index, room) in rooms.iter().enumerate() {
                            a(class = "button", href = room.as_str(), target = "_blank", rel = "noopener noreferrer") : if rooms.len() == 1 { "Race room".to_owned() } else { format!("Race room {}", index + 1) };
                        }
                        @if let Some(url) = &bracket {
                            a(class = "button", href = url.as_str(), target = "_blank", rel = "noopener noreferrer") : "start.gg match";
                        }
                    }
                    @for (language, url) in &broadcasts {
                        @if let Some(query) = twitch_embed(url, race.is_ended()) {
                            details(class = "race-stream", name = "race-restream", data_twitch_query = query) {
                                summary : format!("Watch {language} here");
                                div(class = "race-stream-player", data_player_title = format!("{language} restream")) {}
                            }
                        }
                    }
                }
            }
            @if settings.is_some() || race.draft.is_some() || data.single_settings.is_some() {
                section(class = "race-detail-section") {
                    h2 : "Settings";
                    @if let Some(preset) = &preset { p { strong : "Drafted preset: "; : preset; } }
                    @if let Some(settings) = &settings { p(class = "race-settings-summary") : settings; }
                    @if let Some(settings) = &drafted_settings {
                        details {
                            summary : "Drafted settings";
                            pre : serde_json::to_string_pretty(settings)?;
                        }
                    }
                    @if race.draft.is_some() && settings.is_none() && preset.is_none() && drafted_settings.is_none() {
                        p : "Draft settings are not available yet.";
                    }
                    @if let Some(settings) = &data.single_settings {
                        details {
                            summary : "Event settings";
                            p(class = "help") : "Current event configuration. A race’s generated seed may use an earlier configuration.";
                            pre : serde_json::to_string_pretty(settings)?;
                        }
                    }
                }
            }
            section(class = "race-detail-section") {
                h2 : "Seed";
                @if show_seed {
                    @let game_id = data.game(&mut transaction).await?.map(|game| game.id).unwrap_or(1);
                    @if race.seed.file_hash.is_some() || race.seed.files().is_some() {
                        : seed::table_cell(now, &race.seed, true, None, &mut transaction, game_id, preset.as_deref()).await?;
                    } else { p : "No seed recorded."; }
                } else if is_async {
                    p : "The seed stays hidden until every entrant has finished or received it.";
                } else { p : "The seed has not been revealed publicly yet."; }
            }
            @if !volunteers.is_empty() {
                section(class = "race-detail-section") {
                    h2 : "Confirmed volunteers";
                    dl {
                        @for (binding, names) in &volunteers {
                            dt : format!("{} · {}", binding.role_type_name, binding.language);
                            dd : names.join(", ");
                        }
                    }
                }
            }
            @if !related_games.is_empty() {
                section(class = "race-detail-section") {
                    h2 : "Other games in this match";
                    ul {
                        @for other in related_games {
                            li {
                                a(href = uri!(race_detail(series, event, other.id))) : other.game.map_or("Race details".to_owned(), |game| format!("Game {game}"));
                                @if let Some(start) = public_parts(other).iter().filter_map(|part| part.start()).min() {
                                    : " · "; : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                }
                            }
                        }
                    }
                }
            }
        }
        script(src = static_url!("race-detail.js"), defer = "") {}
    };
    Ok(page(
        transaction,
        &me,
        &uri,
        PageStyle::default(),
        &format!("{title} — {}", data.display_name),
        content,
    )
    .await?)
}
