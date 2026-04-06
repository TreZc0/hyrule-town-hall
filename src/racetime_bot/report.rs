use {
    std::collections::HashMap,
    tokio::sync::RwLockReadGuard,
    serenity::all::{
        CreateActionRow,
        CreateButton,
        CreateMessage,
    },
    crate::{
        prelude::*,
        racetime_bot::*,
    },
};

trait Score {
    type SortKey: Ord;

    fn is_dnf(&self) -> bool;
    fn sort_key(&self) -> Self::SortKey;
    fn time_window(&self, other: &Self) -> Option<Duration>;
    fn format(&self, language: Language) -> Cow<'_, str>;
    fn as_duration(&self) -> Option<Option<Duration>>;
}

impl Score for Option<Duration> {
    type SortKey = (bool, Option<Duration>);

    fn is_dnf(&self) -> bool {
        self.is_none()
    }

    fn sort_key(&self) -> Self::SortKey {
        (
            self.is_none(), // sort DNF last
            *self,
        )
    }

    fn time_window(&self, other: &Self) -> Option<Duration> {
        Some((*self)? - (*other)?)
    }

    fn format(&self, language: Language) -> Cow<'_, str> {
        match language {
            French => self.map_or(Cow::Borrowed("forfait"), |time| Cow::Owned(French.format_duration(time, false))),
            _ => self.map_or(Cow::Borrowed("DNF"), |time| Cow::Owned(English.format_duration(time, false))),
        }
    }

    fn as_duration(&self) -> Option<Option<Duration>> {
        Some(*self)
    }
}


/// Queries start.gg for current set state and builds complete game results including the new game
async fn collect_completed_game_results(
    http_client: &reqwest::Client,
    startgg_token: &str,
    set_id: &startgg::ID,
    current_game: i16,
    current_winner_id: &startgg::ID,
) -> Result<Vec<startgg::GameResult>, Error> {
    let set_data = startgg::query_uncached::<startgg::SetQuery>(
        http_client,
        startgg_token,
        startgg::set_query::Variables {
            set_id: set_id.clone(),
        }
    ).await.to_racetime()?;

    let mut results = Vec::new();

    if let Some(set) = set_data.set {
        if let Some(games) = set.games {
            for game in games.into_iter().flatten() {
                if let (Some(order_num), Some(winner_id)) = (game.order_num, game.winner_id) {
                    results.push(startgg::GameResult {
                        game_num: order_num,
                        winner_entrant_id: startgg::ID(winner_id.to_string()),
                    });
                }
            }
        }
    }

    let current_game_num = current_game as i64;
    if let Some(existing) = results.iter_mut().find(|r| r.game_num == current_game_num) {
        // Update existing game (shouldn't normally happen, but handle it)
        existing.winner_entrant_id = current_winner_id.clone();
    } else {
        results.push(startgg::GameResult {
            game_num: current_game_num,
            winner_entrant_id: current_winner_id.clone(),
        });
    }

    results.sort_by_key(|r| r.game_num);

    Ok(results)
}

fn is_match_decided(game_results: &[startgg::GameResult], total_games: i16) -> bool {
    let games_to_win = (total_games / 2) + 1;

    let mut win_counts: HashMap<&startgg::ID, i16> = HashMap::new();
    for result in game_results {
        *win_counts.entry(&result.winner_entrant_id).or_insert(0) += 1;
    }

    win_counts.values().any(|&wins| wins >= games_to_win)
}

fn determine_overall_winner(game_results: &[startgg::GameResult]) -> startgg::ID {
    let mut win_counts: HashMap<startgg::ID, i16> = HashMap::new();
    for result in game_results {
        *win_counts.entry(result.winner_entrant_id.clone()).or_insert(0) += 1;
    }

    win_counts.into_iter()
        .max_by_key(|(_, wins)| *wins)
        .map(|(id, _)| id)
        .expect("No games completed")
}

async fn report_1v1<'a, S: Score>(mut transaction: Transaction<'a, Postgres>, ctx: &RaceContext<GlobalState>, cal_event: &cal::Event, event: &event::Data<'_>, mut entrants: [(Entrant, S, Url); 2]) -> Result<(Transaction<'a, Postgres>, Vec<Id<Races>>), Error> {
    entrants.sort_unstable_by_key(|(_, time, _)| time.sort_key());
    let [(winner, winning_time, winning_room), (loser, losing_time, losing_room)] = entrants;
    let ignored_race_ids: Vec<Id<Races>> = vec![];
    if winning_time.is_dnf() && losing_time.is_dnf() {
        if let Some(results_channel) = event.discord_race_results_channel.or(event.discord_organizer_channel) {
            let msg = if_chain! {
                if let French = event.language;
                if let Some(phase_round) = match (&cal_event.race.phase, &cal_event.race.round) {
                    (Some(phase), Some(round)) => if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut *transaction).await.to_racetime()? {
                        Some(Some(phase_round))
                    } else {
                        None // no translation
                    },
                    (Some(_), None) | (None, Some(_)) => None, // no translation
                    (None, None) => Some(None), // no phase/round
                };
                if cal_event.race.game.is_none();
                then {
                    let mut builder = MessageBuilder::default();
                    if let Some(phase_round) = phase_round {
                        builder.push_safe(phase_round);
                        builder.push(" : ");
                    }
                    builder.push("Ni ");
                    builder.mention_entrant(&mut transaction, event.discord_guild, &winner).await.to_racetime()?;
                    if winning_room != losing_room {
                        builder.push(" [<");
                        builder.push(winning_room.to_string());
                        builder.push(">]");
                    }
                    builder.push(" ni ");
                    builder.mention_entrant(&mut transaction, event.discord_guild, &loser).await.to_racetime()?;
                    if winning_room != losing_room {
                        builder.push(" [<");
                        builder.push(losing_room.to_string());
                        builder.push(">]");
                    }
                    builder.push(" n'ont fini");
                    if winning_room == losing_room {
                        builder.push(" <");
                        builder.push(winning_room);
                        builder.push('>');
                    }
                    builder.build()
                } else {
                    let mut builder = MessageBuilder::default();
                    let info_prefix = match (&cal_event.race.phase, &cal_event.race.round) {
                        (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
                        (Some(phase), None) => Some(phase.clone()),
                        (None, Some(round)) => Some(round.clone()),
                        (None, None) => None,
                    };
                    match (info_prefix, cal_event.race.game) {
                        (Some(prefix), Some(game)) => {
                            builder.push_safe(prefix);
                            builder.push(", game ");
                            builder.push(game.to_string());
                            builder.push(": ");
                        }
                        (Some(prefix), None) => {
                            builder.push_safe(prefix);
                            builder.push(": ");
                        }
                        (None, Some(game)) => {
                            builder.push("game ");
                            builder.push(game.to_string());
                            builder.push(": ");
                        }
                        (None, None) => {}
                    }
                    builder.mention_entrant(&mut transaction, event.discord_guild, &winner).await.to_racetime()?;
                    if winning_room != losing_room {
                        builder.push(" [<");
                        builder.push(winning_room.to_string());
                        builder.push(">]");
                    }
                    builder.push(" and ");
                    builder.mention_entrant(&mut transaction, event.discord_guild, &loser).await.to_racetime()?;
                    if winning_room != losing_room {
                        builder.push(" [<");
                        builder.push(losing_room.to_string());
                        builder.push(">]");
                    }
                    builder.push(" both did not finish");
                    if winning_room == losing_room {
                        builder.push(" <");
                        builder.push(winning_room);
                        builder.push('>');
                    }
                    builder.build()
                }
            };
            results_channel.say(&*ctx.global_state.discord_ctx.read().await, msg).await.to_racetime()?;
        }
    } else if losing_time.time_window(&winning_time).is_some_and(|time_window| time_window <= event.retime_window) {
        if let Some(organizer_channel) = event.discord_organizer_channel {
            let mut msg = MessageBuilder::default();
            msg.push("Race");
            // Add matchup info: (Player A vs. Player B, Phase - Round)
            let mut matchup_parts = Vec::new();
            let discord_ctx = ctx.global_state.discord_ctx.read().await;
            // Get entrant names
            if let (Some(winner_name), Some(loser_name)) = (
                winner.name(&mut transaction, &*discord_ctx).await.to_racetime()?,
                loser.name(&mut transaction, &*discord_ctx).await.to_racetime()?
            ) {
                matchup_parts.push(format!("{} vs. {}", winner_name, loser_name));
            }
            // Add phase/round info
            let phase_round = match (&cal_event.race.phase, &cal_event.race.round) {
                (Some(phase), Some(round)) => Some(format!("{} - {}", phase, round)),
                (Some(phase), None) => Some(phase.clone()),
                (None, Some(round)) => Some(round.clone()),
                (None, None) => None,
            };
            if let Some(phase_round) = phase_round {
                matchup_parts.push(phase_round);
            }
            if !matchup_parts.is_empty() {
                msg.push(" (");
                msg.push(matchup_parts.join(", "));
                msg.push(")");
            }
            msg.push(" finished too close for automatic reporting (potential draw): <");
            msg.push(winning_room.to_string());
            if winning_room != losing_room {
                msg.push("> and <");
                msg.push(losing_room);
            }
            msg.push('>');
            let discord_ctx = ctx.global_state.discord_ctx.read().await;
            if winning_time.as_duration().is_some() {
                msg.push("\nPlease decide how to proceed. You can either trigger a rematch or check the results and frame count the VoDs if necessary (<https://somewes.com/frame-count/>), then report the finalized results via the buttons below.");
                organizer_channel.send_message(&*discord_ctx, CreateMessage::new()
                    .content(msg.build())
                    .components(vec![
                        CreateActionRow::Buttons(vec![
                            CreateButton::new(format!("draw_report_result_{}", cal_event.race.id))
                                .label("Report final result")
                                .style(ButtonStyle::Primary),
                            CreateButton::new(format!("draw_restart_race_{}", cal_event.race.id))
                                .label("Restart race")
                                .style(ButtonStyle::Danger),
                        ])
                    ])
                ).await.to_racetime()?;
            } else {
                // TFB or other non-time score: text only
                if event.discord_race_results_channel.is_some() || matches!(cal_event.race.source, cal::Source::StartGG { .. }) {
                    msg.push(" — please manually ");
                    if let Some(results_channel) = event.discord_race_results_channel {
                        msg.push("post the announcement in ");
                        msg.mention(&results_channel);
                    }
                    if let Some(startgg_set_url) = cal_event.race.startgg_set_url().to_racetime()? {
                        if event.discord_race_results_channel.is_some() {
                            msg.push(" and ");
                        }
                        msg.push_named_link_no_preview("report the result on start.gg", startgg_set_url);
                    }
                    msg.push(" after adjusting the times");
                }
                organizer_channel.say(&*discord_ctx, msg.build()).await.to_racetime()?;
            }
        }
    } else if let (Some(winner_time), Some(loser_time)) = (winning_time.as_duration(), losing_time.as_duration()) {
        return complete_1v1_result(transaction, &*ctx.global_state, &cal_event.race, event, winner, winner_time, winning_room, loser, loser_time, losing_room).await;
    } else {
        // Non-duration score (e.g. TFB piece count): announce result and report to start.gg/draft as applicable.
        if let Some(results_channel) = event.discord_race_results_channel.or(event.discord_organizer_channel) {
            let msg = if_chain! {
                if let French = event.language;
                if let Some(phase_round) = match (&cal_event.race.phase, &cal_event.race.round) {
                    (Some(phase), Some(round)) => if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut *transaction).await.to_racetime()? {
                        Some(Some(phase_round))
                    } else {
                        None // no translation
                    },
                    (Some(_), None) | (None, Some(_)) => None, // no translation
                    (None, None) => Some(None), // no phase/round
                };
                if cal_event.race.game.is_none();
                then {
                    let mut builder = MessageBuilder::default();
                    if let Some(phase_round) = phase_round {
                        builder.push_safe(phase_round);
                        builder.push(" : ");
                    }
                    builder.mention_entrant(&mut transaction, event.discord_guild, &winner).await.to_racetime()?;
                    builder.push(" (");
                    builder.push(winning_time.format(French));
                    builder.push(')');
                    if winning_room != losing_room {
                        builder.push(" [<");
                        builder.push(winning_room.to_string());
                        builder.push(">]");
                    }
                    builder.push(if winner.name_is_plural() { " ont battu " } else { " a battu " });
                    builder.mention_entrant(&mut transaction, event.discord_guild, &loser).await.to_racetime()?;
                    builder.push(" (");
                    builder.push(losing_time.format(French));
                    builder.push(if winning_room == losing_room { ") <" } else { ") [<" });
                    builder.push(losing_room.to_string());
                    builder.push(if winning_room == losing_room { ">" } else { ">]" });
                    builder.build()
                } else {
                    let mut builder = MessageBuilder::default();
                    let info_prefix = match (&cal_event.race.phase, &cal_event.race.round) {
                        (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
                        (Some(phase), None) => Some(phase.clone()),
                        (None, Some(round)) => Some(round.clone()),
                        (None, None) => None,
                    };
                    match (info_prefix, cal_event.race.game) {
                        (Some(prefix), Some(game)) => {
                            builder.push_safe(prefix);
                            builder.push(", game ");
                            builder.push(game.to_string());
                            builder.push(": ");
                        }
                        (Some(prefix), None) => {
                            builder.push_safe(prefix);
                            builder.push(": ");
                        }
                        (None, Some(game)) => {
                            builder.push("game ");
                            builder.push(game.to_string());
                            builder.push(": ");
                        }
                        (None, None) => {}
                    }
                    builder.mention_entrant(&mut transaction, event.discord_guild, &winner).await.to_racetime()?;
                    builder.push(" (");
                    builder.push(winning_time.format(English));
                    builder.push(')');
                    if winning_room != losing_room {
                        builder.push(" [<");
                        builder.push(winning_room.to_string());
                        builder.push(">]");
                    }
                    builder.push(if winner.name_is_plural() { " defeat " } else { " defeats " });
                    builder.mention_entrant(&mut transaction, event.discord_guild, &loser).await.to_racetime()?;
                    builder.push(" (");
                    builder.push(losing_time.format(English));
                    builder.push(if winning_room == losing_room { ") <" } else { ") [<" });
                    builder.push(losing_room.to_string());
                    builder.push(if winning_room == losing_room { ">" } else { ">]" });
                    builder.build()
                }
            };
            results_channel.say(&*ctx.global_state.discord_ctx.read().await, msg).await.to_racetime()?;
        }
        return report_external_and_init_draft(transaction, &*ctx.global_state, &cal_event.race, event, winner, None, winning_room, loser, None).await;
    }
    Ok((transaction, ignored_race_ids))
}

pub(crate) async fn complete_1v1_result<'a>(
    mut transaction: Transaction<'a, Postgres>,
    global_state: &GlobalState,
    race: &Race,
    event: &event::Data<'_>,
    winner: Entrant,
    winner_time: Option<Duration>,
    winning_room: Url,
    loser: Entrant,
    loser_time: Option<Duration>,
    losing_room: Url,
) -> Result<(Transaction<'a, Postgres>, Vec<Id<Races>>), Error> {
    let fmt_time = |time: Option<Duration>, language: Language| -> Cow<'static, str> {
        match language {
            French => time.map_or(Cow::Borrowed("forfait"), |t| Cow::Owned(French.format_duration(t, false))),
            _ => time.map_or(Cow::Borrowed("DNF"), |t| Cow::Owned(English.format_duration(t, false))),
        }
    };

    // 1. Post Discord announcement
    if let Some(results_channel) = event.discord_race_results_channel.or(event.discord_organizer_channel) {
        let msg = if_chain! {
            if let French = event.language;
            if let Some(phase_round) = match (&race.phase, &race.round) {
                (Some(phase), Some(round)) => if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut *transaction).await.to_racetime()? {
                    Some(Some(phase_round))
                } else {
                    None
                },
                (Some(_), None) | (None, Some(_)) => None,
                (None, None) => Some(None),
            };
            if race.game.is_none();
            then {
                let mut builder = MessageBuilder::default();
                if let Some(phase_round) = phase_round {
                    builder.push_safe(phase_round);
                    builder.push(" : ");
                }
                builder.mention_entrant(&mut transaction, event.discord_guild, &winner).await.to_racetime()?;
                builder.push(" (");
                builder.push(fmt_time(winner_time, French));
                builder.push(')');
                if winning_room != losing_room {
                    builder.push(" [<");
                    builder.push(winning_room.to_string());
                    builder.push(">]");
                }
                builder.push(if winner.name_is_plural() { " ont battu " } else { " a battu " });
                builder.mention_entrant(&mut transaction, event.discord_guild, &loser).await.to_racetime()?;
                builder.push(" (");
                builder.push(fmt_time(loser_time, French));
                builder.push(if winning_room == losing_room { ") <" } else { ") [<" });
                builder.push(losing_room.to_string());
                builder.push(if winning_room == losing_room { ">" } else { ">]" });
                builder.build()
            } else {
                let mut builder = MessageBuilder::default();
                let info_prefix = match (&race.phase, &race.round) {
                    (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
                    (Some(phase), None) => Some(phase.clone()),
                    (None, Some(round)) => Some(round.clone()),
                    (None, None) => None,
                };
                match (info_prefix, race.game) {
                    (Some(prefix), Some(game)) => {
                        builder.push_safe(prefix);
                        builder.push(", game ");
                        builder.push(game.to_string());
                        builder.push(": ");
                    }
                    (Some(prefix), None) => {
                        builder.push_safe(prefix);
                        builder.push(": ");
                    }
                    (None, Some(game)) => {
                        builder.push("game ");
                        builder.push(game.to_string());
                        builder.push(": ");
                    }
                    (None, None) => {}
                }
                builder.mention_entrant(&mut transaction, event.discord_guild, &winner).await.to_racetime()?;
                builder.push(" (");
                builder.push(fmt_time(winner_time, English));
                builder.push(')');
                if winning_room != losing_room {
                    builder.push(" [<");
                    builder.push(winning_room.to_string());
                    builder.push(">]");
                }
                builder.push(if winner.name_is_plural() { " defeat " } else { " defeats " });
                builder.mention_entrant(&mut transaction, event.discord_guild, &loser).await.to_racetime()?;
                builder.push(" (");
                builder.push(fmt_time(loser_time, English));
                builder.push(if winning_room == losing_room { ") <" } else { ") [<" });
                builder.push(losing_room.to_string());
                builder.push(if winning_room == losing_room { ">" } else { ">]" });
                builder.build()
            }
        };
        results_channel.say(&*global_state.discord_ctx.read().await, msg).await.to_racetime()?;
    }

    report_external_and_init_draft(transaction, global_state, race, event, winner, winner_time, winning_room, loser, loser_time).await
}

async fn report_external_and_init_draft<'a>(
    mut transaction: Transaction<'a, Postgres>,
    global_state: &GlobalState,
    race: &Race,
    event: &event::Data<'_>,
    winner: Entrant,
    winner_time: Option<Duration>,
    winning_room: Url,
    loser: Entrant,
    loser_time: Option<Duration>,
) -> Result<(Transaction<'a, Postgres>, Vec<Id<Races>>), Error> {
    let mut ignored_race_ids: Vec<Id<Races>> = vec![];
    let mut series_decided = false;
    match race.source {
        cal::Source::Manual | cal::Source::Sheet { .. } => {}
        cal::Source::Challonge { .. } => {} //TODO
        cal::Source::League { id } => if let (Some(winner_rt), Some(loser_rt)) = (
            match &winner {
                Entrant::MidosHouseTeam(team) => team.members(&mut transaction).await.to_racetime()?.into_iter().exactly_one().ok().and_then(|member| member.racetime).map(|racetime| racetime.id),
                Entrant::Discord { racetime_id, .. } | Entrant::Named { racetime_id, .. } => racetime_id.clone(),
            },
            match &loser {
                Entrant::MidosHouseTeam(team) => team.members(&mut transaction).await.to_racetime()?.into_iter().exactly_one().ok().and_then(|member| member.racetime).map(|racetime| racetime.id),
                Entrant::Discord { racetime_id, .. } | Entrant::Named { racetime_id, .. } => racetime_id.clone(),
            },
        ) {
            let mut form = collect![as HashMap<_, _>:
                "id" => id.to_string(),
                "racetimeRoom" => winning_room.to_string(),
                "fpa" => "0".to_owned(),
                "winner" => winner_rt,
                "loser" => loser_rt,
            ];
            if let Some(t) = winner_time {
                form.insert("winningTime", t.as_secs().to_string());
            }
            if let Some(t) = loser_time {
                form.insert("losingTime", t.as_secs().to_string());
            }
            let request = global_state.http_client.post("https://league.ootrandomizer.com/reportResultFromMidoHouse")
                .bearer_auth(&global_state.league_api_key)
                .form(&form);
            println!("reporting draw-resolved result to League website: {:?}", serde_urlencoded::to_string(&form));
            request.send().await?.detailed_error_for_status().await.to_racetime()?;
        },
        cal::Source::StartGG { ref set, .. } => {
            if let Entrant::MidosHouseTeam(Team { startgg_id: Some(winner_entrant_id), .. }) = &winner {
                if let Some(game) = race.game {
                    let total_games = race.game_count(&mut transaction).await.to_racetime()?;
                    let completed_game_results = collect_completed_game_results(
                        &global_state.http_client,
                        &global_state.startgg_token,
                        set,
                        game,
                        winner_entrant_id,
                    ).await.to_racetime()?;
                    let match_decided = if event.startgg_double_rr {
                        game as i16 == total_games
                    } else {
                        is_match_decided(&completed_game_results, total_games)
                    };
                    if match_decided {
                        if event.startgg_double_rr {
                            let score_data = startgg::query_uncached::<startgg::SetScoreQuery>(
                                &global_state.http_client,
                                &global_state.startgg_token,
                                startgg::set_score_query::Variables { set_id: set.clone() },
                            ).await.to_racetime()?;
                            let game1_winner_id = score_data.set
                                .and_then(|s| s.slots)
                                .into_iter()
                                .flatten()
                                .flatten()
                                .find(|slot| slot.standing.as_ref().and_then(|st| st.placement) == Some(1))
                                .and_then(|slot| slot.entrant)
                                .and_then(|e| e.id)
                                .expect("double-RR set score query: no slot with placement=1");
                            let all_game_results = vec![
                                startgg::GameResult { game_num: 1, winner_entrant_id: game1_winner_id },
                                startgg::GameResult { game_num: 2, winner_entrant_id: winner_entrant_id.clone() },
                            ];
                            startgg::query_uncached::<startgg::ResetSetMutation>(
                                &global_state.http_client,
                                &global_state.startgg_token,
                                startgg::reset_set_mutation::Variables { set_id: set.clone() },
                            ).await.to_racetime()?;
                            let overall_winner_id = if is_match_decided(&all_game_results, total_games) {
                                Some(determine_overall_winner(&all_game_results))
                            } else {
                                None
                            };
                            startgg::query_uncached::<startgg::ReportBracketSetMutation>(
                                &global_state.http_client,
                                &global_state.startgg_token,
                                startgg::report_bracket_set_mutation::Variables {
                                    set_id: set.clone(),
                                    winner_id: overall_winner_id,
                                    game_data: Some(all_game_results.iter().map(|gr| Some(gr.to_game_data_input())).collect()),
                                },
                            ).await.to_racetime()?;
                        } else {
                            let overall_winner = determine_overall_winner(&completed_game_results);
                            startgg::query_uncached::<startgg::ReportBracketSetMutation>(
                                &global_state.http_client,
                                &global_state.startgg_token,
                                startgg::report_bracket_set_mutation::Variables {
                                    set_id: set.clone(),
                                    winner_id: Some(overall_winner),
                                    game_data: Some(completed_game_results.iter().map(|gr| Some(gr.to_game_data_input())).collect()),
                                },
                            ).await.to_racetime()?;
                        }
                        ignored_race_ids = race.ignore_remaining_games(&mut transaction).await.to_racetime()?;
                        series_decided = true;
                    } else if event.startgg_double_rr {
                        startgg::query_uncached::<startgg::ReportBracketSetMutation>(
                            &global_state.http_client,
                            &global_state.startgg_token,
                            startgg::report_bracket_set_mutation::Variables {
                                set_id: set.clone(),
                                winner_id: Some(winner_entrant_id.clone()),
                                game_data: Some(completed_game_results.iter().map(|gr| Some(gr.to_game_data_input())).collect()),
                            },
                        ).await.to_racetime()?;
                    } else {
                        startgg::query_uncached::<startgg::ReportBracketSetMutation>(
                            &global_state.http_client,
                            &global_state.startgg_token,
                            startgg::report_bracket_set_mutation::Variables {
                                set_id: set.clone(),
                                winner_id: None,
                                game_data: Some(completed_game_results.iter().map(|gr| Some(gr.to_game_data_input())).collect()),
                            },
                        ).await.to_racetime()?;
                    }
                } else {
                    startgg::query_uncached::<startgg::ReportOneGameResultMutation>(
                        &global_state.http_client,
                        &global_state.startgg_token,
                        startgg::report_one_game_result_mutation::Variables {
                            set_id: set.clone(),
                            winner_entrant_id: winner_entrant_id.clone(),
                        },
                    ).await.to_racetime()?;
                }
            } else if let Some(organizer_channel) = event.discord_organizer_channel {
                let mut msg = MessageBuilder::default();
                msg.push("failed to report race result to start.gg: <");
                msg.push(winning_room.to_string());
                msg.push("> (winner has no start.gg entrant ID)");
                organizer_channel.say(&*global_state.discord_ctx.read().await, msg.build()).await.to_racetime()?;
            }
        },
        cal::Source::SpeedGaming { .. } => {}
    }

    if_chain! {
        if !series_decided;
        if let Entrant::MidosHouseTeam(winner) = winner;
        if let Entrant::MidosHouseTeam(loser) = loser;
        if let Some(draft_kind) = event.draft_kind();
        if let Some(next_game) = race.next_game(&mut transaction, &global_state.http_client).await.to_racetime()?;
        then {
            let draft = match draft_kind {
                draft::Kind::PickOnly { .. }
                | draft::Kind::BanPick { .. }
                | draft::Kind::BanOnly { .. } => {
                    sqlx::query_scalar!(
                        r#"SELECT draft_state AS "draft_state: sqlx::types::Json<Draft>" FROM races WHERE id = $1"#,
                        race.id as _,
                    ).fetch_one(&mut *transaction).await.to_racetime()?
                        .expect("series-draft race should have draft state")
                        .0
                },
                _ => Draft::for_next_game(&mut transaction, &draft_kind, loser.id, winner.id).await.to_racetime()?,
            };
            sqlx::query!("UPDATE races SET draft_state = $1 WHERE id = $2", sqlx::types::Json(&draft) as _, next_game.id as _).execute(&mut *transaction).await.to_racetime()?;
            if_chain! {
                if let Some(guild_id) = event.discord_guild;
                if let Some(scheduling_thread) = next_game.scheduling_thread;
                let discord_ctx = global_state.discord_ctx.read().await;
                let data = discord_ctx.data.read().await;
                if let Some(Some(command_ids)) = data.get::<CommandIds>().and_then(|command_ids| command_ids.get(&guild_id).copied());
                then {
                    let mut msg_ctx = draft::MessageContext::Discord {
                        teams: next_game.teams().cloned().collect(),
                        team: Team::dummy(),
                        transaction, guild_id, command_ids,
                    };
                    let step = draft.next_step(&draft_kind, next_game.game, &mut msg_ctx).await.to_racetime()?;
                    if !step.message.is_empty() {
                        scheduling_thread.say(&*discord_ctx, step.message).await.to_racetime()?;
                    }
                    transaction = msg_ctx.into_transaction();
                }
            }
        }
    }

    Ok((transaction, ignored_race_ids))
}

async fn report_ffa(ctx: &RaceContext<GlobalState>, cal_event: &cal::Event, event: &event::Data<'_>, room: Url) -> Result<(), Error> {
    if let Some(results_channel) = event.discord_race_results_channel.or(event.discord_organizer_channel) {
        let mut builder = MessageBuilder::default();
        let info_prefix = match (&cal_event.race.phase, &cal_event.race.round) {
            (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
            (Some(phase), None) => Some(phase.clone()),
            (None, Some(round)) => Some(round.clone()),
            (None, None) => None,
        };
        match (info_prefix, cal_event.race.game) {
            (Some(prefix), Some(game)) => {
                builder.push_safe(prefix);
                builder.push(", game ");
                builder.push(game.to_string());
                builder.push(": ");
            }
            (Some(prefix), None) => {
                builder.push_safe(prefix);
                builder.push(": ");
            }
            (None, Some(game)) => {
                builder.push("game ");
                builder.push(game.to_string());
                builder.push(": ");
            }
            (None, None) => {}
        }
        builder.push("race finished: <");
        builder.push(room.to_string());
        builder.push('>');
        results_channel.say(&*ctx.global_state.discord_ctx.read().await, builder.build()).await.to_racetime()?;
    }
    Ok(())
}

impl Handler {
    pub(super) async fn official_race_finished(&self, ctx: &RaceContext<GlobalState>, data: RwLockReadGuard<'_, RaceData>, cal_event: &cal::Event, event: &event::Data<'_>, fpa_invoked: bool, breaks_used: bool) -> Result<(), Error> {
        let stream_delay = match cal_event.race.entrants {
            Entrants::Open | Entrants::Count { .. } => event.open_stream_delay,
            Entrants::Two(_) | Entrants::Three(_) | Entrants::Named(_) => event.invitational_stream_delay,
        };
        sleep(stream_delay).await;
        let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
        if let Some(ended_at) = data.ended_at {
            match cal_event.kind {
                cal::EventKind::Normal => sqlx::query!("UPDATE races SET end_time = $1 WHERE id = $2", ended_at, cal_event.race.id as _).execute(&mut *transaction).await.to_racetime()?,
                cal::EventKind::Async1 => sqlx::query!("UPDATE races SET async_end1 = $1 WHERE id = $2", ended_at, cal_event.race.id as _).execute(&mut *transaction).await.to_racetime()?,
                cal::EventKind::Async2 => sqlx::query!("UPDATE races SET async_end2 = $1 WHERE id = $2", ended_at, cal_event.race.id as _).execute(&mut *transaction).await.to_racetime()?,
                cal::EventKind::Async3 => sqlx::query!("UPDATE races SET async_end3 = $1 WHERE id = $2", ended_at, cal_event.race.id as _).execute(&mut *transaction).await.to_racetime()?,
            };
        }
        if cal_event.is_private_async_part() {
            ctx.say("@entrants Please remember to send the videos of your run to a tournament organizer.").await?;
            if fpa_invoked {
                sqlx::query!("UPDATE races SET fpa_invoked = TRUE WHERE id = $1", cal_event.race.id as _).execute(&mut *transaction).await.to_racetime()?;
            }
            if breaks_used {
                sqlx::query!("UPDATE races SET breaks_used = TRUE WHERE id = $1", cal_event.race.id as _).execute(&mut *transaction).await.to_racetime()?;
            }
            if let Some(organizer_channel) = event.discord_organizer_channel {
                organizer_channel.say(&*ctx.global_state.discord_ctx.read().await, MessageBuilder::default()
                    .push("first half of async finished")
                    .push(if fpa_invoked { " with FPA call" } else if event.manual_reporting_with_breaks && breaks_used { " with breaks" } else { "" })
                    .push(": <https://")
                    .push(racetime_host())
                    .push(&ctx.data().await.url)
                    .push('>')
                    .build()
                ).await.to_racetime()?;
            }
        } else if fpa_invoked {
            if let Some(organizer_channel) = event.discord_organizer_channel {
                let mut msg = MessageBuilder::default();
                msg.push("race finished with FPA call: <https://");
                msg.push(racetime_host());
                msg.push(&ctx.data().await.url);
                msg.push('>');
                if event.discord_race_results_channel.is_some() || matches!(cal_event.race.source, cal::Source::StartGG { .. }) {
                    msg.push(" — please manually ");
                    if let Some(results_channel) = event.discord_race_results_channel {
                        msg.push("post the announcement in ");
                        msg.mention(&results_channel);
                    }
                    if let Some(startgg_set_url) = cal_event.race.startgg_set_url().to_racetime()? {
                        if event.discord_race_results_channel.is_some() {
                            msg.push(" and ");
                        }
                        msg.push_named_link_no_preview("report the result on start.gg", startgg_set_url);
                    }
                    msg.push(" after adjusting the times");
                }
                //TODO note to manually initialize high seed for next game's draft (if any) and use `/post-status`
                organizer_channel.say(&*ctx.global_state.discord_ctx.read().await, msg.build()).await.to_racetime()?;
            }
        } else if event.manual_reporting_with_breaks && breaks_used {
            if let Some(organizer_channel) = event.discord_organizer_channel {
                let mut msg = MessageBuilder::default();
                msg.push("race finished with breaks: <https://");
                msg.push(racetime_host());
                msg.push(&ctx.data().await.url);
                msg.push('>');
                if event.discord_race_results_channel.is_some() || matches!(cal_event.race.source, cal::Source::StartGG { .. }) {
                    msg.push(" — please manually ");
                    if let Some(results_channel) = event.discord_race_results_channel {
                        msg.push("post the announcement in ");
                        msg.mention(&results_channel);
                    }
                    if let Some(startgg_set_url) = cal_event.race.startgg_set_url().to_racetime()? {
                        if event.discord_race_results_channel.is_some() {
                            msg.push(" and ");
                        }
                        msg.push_named_link_no_preview("report the result on start.gg", startgg_set_url);
                    }
                    msg.push(" after adjusting the times");
                }
                //TODO note to manually initialize high seed for next game's draft (if any) and use `/post-status`
                organizer_channel.say(&*ctx.global_state.discord_ctx.read().await, msg.build()).await.to_racetime()?;
            }
        } else if cal_event.race.phase.as_deref() == Some("Seeding") {
            // Seeding race: assign qualifier_rank based on finish order
            let mut entrants: Vec<_> = data.entrants.iter()
                .filter_map(|e| e.user.as_ref().map(|u| (&u.id, e.finish_time)))
                .collect();
            entrants.sort_by_key(|(_, t)| (t.is_none(), *t));
            for (rank, (rt_id, _)) in entrants.iter().enumerate() {
                if let Some(user) = User::from_racetime(&mut *transaction, rt_id).await.to_racetime()? {
                    if let Some(team) = Team::from_event_and_member(&mut transaction, event.series, &event.event, user.id).await.to_racetime()? {
                        sqlx::query!("UPDATE teams SET qualifier_rank = $1 WHERE id = $2", rank as i16 + 1, team.id as _)
                            .execute(&mut *transaction).await.to_racetime()?;
                    }
                }
            }
            if let Some(organizer_channel) = event.discord_organizer_channel {
                let room = Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?;
                organizer_channel.say(&*ctx.global_state.discord_ctx.read().await, format!("Seeding race finished — qualifier ranks assigned: <{room}>")).await.to_racetime()?;
            }
            transaction.commit().await.to_racetime()?;
            return Ok(());
        } else {
            let mut ignored_race_ids = Vec::new();
            match event.team_config {
                TeamConfig::Solo => match cal_event.race.entrants {
                    Entrants::Open | Entrants::Count { .. } => {
                        let room = Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?;
                        report_ffa(ctx, cal_event, event, room).await?;
                    }
                    Entrants::Named(_) => unimplemented!(),
                    Entrants::Two(_) | Entrants::Three(_) => {
                        let room = Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?;
                            let mut teams = Vec::with_capacity(data.entrants.len());
                            for entrant in &data.entrants {
                                if let Some(rt_user) = &entrant.user {
                                    teams.push((if_chain! {
                                        if let Some(user) = User::from_racetime(&mut *transaction, &rt_user.id).await.to_racetime()?;
                                        if let Some(team) = Team::from_event_and_member(&mut transaction, event.series, &event.event, user.id).await.to_racetime()?;
                                        then {
                                            Entrant::MidosHouseTeam(team)
                                        } else {
                                            Entrant::Named {
                                                name: rt_user.full_name.clone(),
                                                racetime_id: Some(rt_user.id.clone()),
                                                twitch_username: rt_user.twitch_name.clone(),
                                            }
                                        }
                                    }, entrant.finish_time, room.clone()));
                                }
                            }
                            if let Ok(teams) = teams.try_into() {
                                let (t, ids) = report_1v1(transaction, ctx, cal_event, event, teams).await?;
                                transaction = t;
                                ignored_race_ids = ids;
                            } else { //TODO separate function for reporting 3-entrant results
                                report_ffa(ctx, cal_event, event, room).await?;
                            }
                    }
                },
                TeamConfig::Pictionary => unimplemented!(), //TODO calculate like solo but report as teams
                _ => match cal_event.race.entrants {
                    Entrants::Open | Entrants::Count { .. } => {
                        let room = Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?;
                        report_ffa(ctx, cal_event, event, room).await?;
                    }
                    Entrants::Named(_) => unimplemented!(),
                    Entrants::Two(_) | Entrants::Three(_) => {
                        let mut team_times = HashMap::<_, Vec<_>>::default();
                        let mut team_rooms = HashMap::new();
                        if cal_event.is_public_async_part() {
                            #[derive(Debug, thiserror::Error)]
                            #[error("ExactlyOneError while formatting result of last async half")]
                            struct ExactlyOneError;

                            for private_async_part in cal_event.race.cal_events().filter(|cal_event| cal_event.is_private_async_part()) {
                                if let Some(ref room) = private_async_part.room() {
                                    let nonactive_team = private_async_part.active_teams().exactly_one().map_err(|_| Error::Custom(Box::new(ExactlyOneError)))?;
                                    let data = ctx.global_state.http_client.get(format!("{}/data", room.to_string()))
                                        .send().await?
                                        .detailed_error_for_status().await.to_racetime()?
                                        .json_with_text_in_error::<RaceData>().await.to_racetime()?;
                                    team_rooms.insert(nonactive_team.racetime_slug.clone().expect("non-racetime.gg team"), Url::clone(room));
                                    for entrant in &data.entrants {
                                        team_times.entry(nonactive_team.racetime_slug.clone().expect("non-racetime.gg team")).or_default().push(entrant.finish_time);
                                    }
                                }
                            }
                            let active_team = cal_event.active_teams().exactly_one().map_err(|_| Error::Custom(Box::new(ExactlyOneError)))?;
                            team_rooms.insert(active_team.racetime_slug.clone().expect("non-racetime.gg team"), Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?);
                            for entrant in &data.entrants {
                                team_times.entry(active_team.racetime_slug.clone().expect("non-racetime.gg team")).or_default().push(entrant.finish_time);
                            }
                        } else {
                            for entrant in &data.entrants {
                                if let Some(ref team) = entrant.team {
                                    if let hash_map::Entry::Vacant(entry) = team_rooms.entry(team.slug.clone()) {
                                        entry.insert(Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?);
                                    }
                                    team_times.entry(team.slug.clone()).or_default().push(entrant.finish_time);
                                } else {
                                    unimplemented!("solo runner in team race") //TODO report error in organizer channel
                                }
                            }
                        }
                            let mut all_teams_found = true;
                            let mut teams = Vec::with_capacity(team_times.len());
                            for (team_slug, times) in team_times {
                                if let Some(team) = Team::from_racetime(&mut transaction, event.series, &event.event, &team_slug).await.to_racetime()? {
                                    teams.push((
                                        Entrant::MidosHouseTeam(team),
                                        times.iter().try_fold(Duration::default(), |acc, &time| Some(acc + time?)).map(|total| total / u32::try_from(times.len()).expect("too many team members")),
                                        team_rooms.remove(&team_slug).expect("each team should have a room"),
                                    ));
                                } else {
                                    all_teams_found = false;
                                }
                            }
                            if_chain! {
                                if all_teams_found;
                                if let Ok(teams) = teams.try_into();
                                then {
                                    let (t, ids) = report_1v1(transaction, ctx, cal_event, event, teams).await?;
                                    transaction = t;
                                    ignored_race_ids = ids;
                                } else { //TODO separate function for reporting 3-entrant results
                                    let room = Url::parse(&format!("https://{}{}", racetime_host(), data.url)).to_racetime()?;
                                    report_ffa(ctx, cal_event, event, room).await?;
                                }
                            }
                    }
                },
            }
            transaction.commit().await.to_racetime()?;
            if !ignored_race_ids.is_empty() {
                let discord_ctx = ctx.global_state.discord_ctx.read().await;
                for race_id in ignored_race_ids {
                    let _ = crate::volunteer_requests::update_volunteer_post_for_race(
                        &ctx.global_state.db_pool,
                        &discord_ctx,
                        race_id,
                    ).await;
                }
            }
            return Ok(());
        }
        transaction.commit().await.to_racetime()?;
        Ok(())
    }
}
