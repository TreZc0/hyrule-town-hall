use super::*;

fn report_responses(previous: &[&str], fail: bool) -> startgg::MockQueries {
    startgg::MockQueries {
        responses: std::collections::VecDeque::from([
            (
                "SetQuery".into(),
                json!({"data": {"set": {
                    "id": "1", "setGamesType": 1, "round": 1,
                    "phaseGroup": {"bracketType": "SINGLE_ELIMINATION", "rounds": [{"number": 1, "bestOf": 3}]},
                    "games": previous.iter().enumerate().map(|(i, winner)| json!({
                        "id": i.to_string(), "orderNum": i + 1, "winnerId": winner.parse::<i64>().unwrap(),
                    })).collect::<Vec<_>>(),
                }}}),
            ),
            (
                "ReportBracketSetMutation".into(),
                if fail {
                    json!({"errors": [{"message": "Result reporting rejected"}]})
                } else {
                    json!({"data": {"reportBracketSet": [{"id": "1"}]}})
                },
            ),
        ]),
        requests: Vec::new(),
    }
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
async fn database_async_bo3_reporting_and_mixed_live_games() {
    let pool = event::configuration::test_pool().await;
    let client = reqwest::Client::new();
    let mut tx = pool.begin().await.unwrap();
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM races ORDER BY id LIMIT 3")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(ids.len(), 3);
    let base = Race::from_id(&mut tx, &client, ids[0].into())
        .await
        .unwrap();
    let mut event = base.event(&mut tx).await.unwrap();
    event.swiss_standings = false;
    let mut winner = Team::dummy();

    // Exercise the Discord entry point and the shared live entry point in both
    // orders, including elimination rounds in an event with double RR enabled.
    for double_rr in [false, true] {
        event.startgg_double_rr = double_rr;
        for live_games in [
            [false, false, false],
            [false, true, false],
            [true, false, true],
        ] {
            for winners in [&["10", "10"][..], &["10", "20", "20"][..]] {
                let phase = format!("async-bo3-test-{}", Uuid::new_v4());
                let mut games = Vec::new();
                for game in 1..=3 {
                    let mut race = base.clone();
                    race.id = ids[game - 1].into();
                    race.source = cal::Source::StartGG {
                        event: "test/event".into(),
                        set: startgg::ID("1".into()),
                    };
                    race.phase = Some(phase.clone());
                    race.round = Some("Round 1".into());
                    race.game = Some(game as i16);
                    race.ignored = false;
                    race.save(&mut tx).await.unwrap();
                    games.push(race);
                }
                for (index, winning_id) in winners.iter().enumerate() {
                    winner.startgg_id = Some(startgg::ID((*winning_id).into()));
                    let decided = index + 1 == winners.len();
                    startgg::MOCK_QUERIES
                        .scope(
                            std::cell::RefCell::new(report_responses(&winners[..index], false)),
                            async {
                                if live_games[index] {
                                    let (actual_decided, _) =
                                        racetime_bot::report::report_startgg_result(
                                            &mut tx,
                                            &client,
                                            "unused",
                                            &games[index],
                                            winner.startgg_id.as_ref().unwrap(),
                                            double_rr,
                                        )
                                        .await
                                        .unwrap();
                                    assert_eq!(actual_decided, decided);
                                } else {
                                    report_async_race_to_external_platforms(
                                        &mut tx,
                                        &client,
                                        "unused",
                                        "unused",
                                        &games[index],
                                        &event,
                                        &winner,
                                    )
                                    .await
                                    .unwrap();
                                }
                                startgg::MOCK_QUERIES.with(|mock| {
                                    let mock = mock.borrow();
                                    assert!(mock.responses.is_empty());
                                    let variables = &mock.requests.last().unwrap()["variables"];
                                    assert_eq!(variables["setID"], "1");
                                    assert_eq!(
                                        variables["winnerID"],
                                        if decided {
                                            json!(winning_id)
                                        } else {
                                            json!(null)
                                        }
                                    );
                                    let data = variables["gameData"].as_array().unwrap();
                                    assert_eq!(data.len(), index + 1);
                                    for (i, game) in data.iter().enumerate() {
                                        assert_eq!(game["gameNum"], i + 1);
                                        assert_eq!(game["winnerId"], winners[i]);
                                    }
                                });
                            },
                        )
                        .await;
                    let ignored: Vec<bool> = sqlx::query_scalar(
                        "SELECT ignored FROM races WHERE id = ANY($1) ORDER BY id",
                    )
                    .bind(&ids)
                    .fetch_all(&mut *tx)
                    .await
                    .unwrap();
                    assert_eq!(ignored, vec![false, false, decided && index == 1]);
                }
            }
        }
    }

    // A rejected deciding result must propagate back to the confirmation
    // transaction without retiring game 3.
    sqlx::query("UPDATE races SET ignored = false WHERE id = ANY($1)")
        .bind(&ids)
        .execute(&mut *tx)
        .await
        .unwrap();
    let race = Race::from_id(&mut tx, &client, ids[1].into())
        .await
        .unwrap();
    winner.startgg_id = Some(startgg::ID("10".into()));
    startgg::MOCK_QUERIES
        .scope(
            std::cell::RefCell::new(report_responses(&["10"], true)),
            async {
                let error = report_async_race_to_external_platforms(
                    &mut tx, &client, "unused", "unused", &race, &event, &winner,
                )
                .await
                .unwrap_err();
                assert!(error.to_string().contains("Result reporting rejected"));
                startgg::MOCK_QUERIES.with(|mock| assert!(mock.borrow().responses.is_empty()));
            },
        )
        .await;
    let ignored: Vec<bool> =
        sqlx::query_scalar("SELECT ignored FROM races WHERE id = ANY($1) ORDER BY id")
            .bind(&ids)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(ignored, vec![false; 3]);

    // Missing entrant IDs must fail before any external request.
    winner.startgg_id = None;
    startgg::MOCK_QUERIES
        .scope(
            std::cell::RefCell::new(startgg::MockQueries::default()),
            async {
                assert!(
                    report_async_race_to_external_platforms(
                        &mut tx, &client, "unused", "unused", &race, &event, &winner,
                    )
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("no start.gg entrant ID")
                );
            },
        )
        .await;
    winner.startgg_id = Some(startgg::ID("10".into()));

    // Retrying a game already accepted remotely replaces that game's result;
    // it must not count the same win twice or invent a third game.
    startgg::MOCK_QUERIES
        .scope(
            std::cell::RefCell::new(report_responses(&["10", "10"], false)),
            async {
                let ignored = report_async_race_to_external_platforms(
                    &mut tx, &client, "unused", "unused", &race, &event, &winner,
                )
                .await
                .unwrap();
                assert_eq!(ignored, vec![Id::from(ids[2])]);
                startgg::MOCK_QUERIES.with(|mock| {
                    let mock = mock.borrow();
                    assert!(mock.responses.is_empty());
                    let variables = &mock.requests.last().unwrap()["variables"];
                    assert_eq!(variables["gameData"].as_array().unwrap().len(), 2);
                    assert_eq!(variables["winnerID"], "10");
                });
            },
        )
        .await;

    // Single-game async matches still finish immediately.
    let mut race = race.clone();
    race.game = None;
    startgg::MOCK_QUERIES
        .scope(
            std::cell::RefCell::new(startgg::MockQueries {
                responses: std::collections::VecDeque::from([(
                    "ReportOneGameResultMutation".into(),
                    json!({"data": {"reportBracketSet": [{"id": "1"}]}}),
                )]),
                requests: Vec::new(),
            }),
            async {
                report_async_race_to_external_platforms(
                    &mut tx, &client, "unused", "unused", &race, &event, &winner,
                )
                .await
                .unwrap();
                startgg::MOCK_QUERIES.with(|mock| {
                    let mock = mock.borrow();
                    assert!(mock.responses.is_empty());
                    assert_eq!(mock.requests[0]["variables"]["winnerEntrantID"], "10");
                });
            },
        )
        .await;
    tx.rollback().await.unwrap();
}
