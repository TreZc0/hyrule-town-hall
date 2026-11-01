use super::*;

#[test]
fn seed_fallback_uses_one_ranking_source_for_both_players() {
    let mut team1 = team::Team::dummy();
    let mut team2 = team::Team::dummy();
    team1.id = 1_i64.into();
    team2.id = 2_i64.into();
    let forward = [team1.id, team2.id];
    let reversed = [team2.id, team1.id];

    // Both local ranks take precedence over conflicting bracket seeds.
    team1.qualifier_rank = Some(63);
    team2.qualifier_rank = Some(21);
    assert_eq!(seeded_team_order([&team1, &team2], Some([1, 64])), reversed);

    team1.qualifier_rank = None;
    team2.qualifier_rank = None;
    assert_eq!(
        seeded_team_order([&team1, &team2], Some([63, 21])),
        reversed
    );
    assert_eq!(
        seeded_team_order([&team2, &team1], Some([21, 63])),
        reversed
    );

    // A local rank of 1 must not be compared to the other player's bracket seed.
    team1.qualifier_rank = Some(1);
    assert_eq!(
        seeded_team_order([&team1, &team2], Some([63, 21])),
        reversed
    );
    team1.qualifier_rank = None;
    team2.qualifier_rank = Some(1);
    assert_eq!(seeded_team_order([&team1, &team2], Some([21, 63])), forward);

    // start.gg seeds need not fit the smallint used for stored qualifier ranks.
    assert_eq!(
        seeded_team_order([&team1, &team2], Some([40_000, 50_000])),
        forward
    );
}

#[test]
fn missing_invalid_and_tied_seed_fallbacks_keep_distinct_teams() {
    let mut team1 = team::Team::dummy();
    let mut team2 = team::Team::dummy();
    team1.id = 1_i64.into();
    team2.id = 2_i64.into();
    let forward = [team1.id, team2.id];
    let reversed = [team2.id, team1.id];
    for fallback in [None, Some([0, 1]), Some([1, -1]), Some([21, 21])] {
        assert_eq!(seeded_team_order([&team1, &team2], fallback), forward);
    }
    // Without a usable fallback, an unknown rank cannot outrank a known one.
    team2.qualifier_rank = Some(21);
    for fallback in [None, Some([0, 1]), Some([1, -1])] {
        assert_eq!(seeded_team_order([&team1, &team2], fallback), reversed);
    }
    team1.qualifier_rank = Some(21);
    assert_eq!(seeded_team_order([&team1, &team2], Some([63, 1])), forward);
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
async fn pooled_standings_seed_new_and_reset_drafts_without_stored_ranks() {
    let pool = event::configuration::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let client = reqwest::Client::new();
    let series = Series::from_str("casboots").unwrap();
    let event_name = "draftest";
    let lower_id = 9_000_000_000_002_001_i64;
    let higher_id = lower_id + 1;
    sqlx::query("INSERT INTO events(series,event,display_name,team_config,qualifier_mode) VALUES($1,$2,'Draft seeding test','solo','pooled_by_mode')")
        .bind(series).bind(event_name).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO pooled_qualifier_configs(series,event,required_mode_count,submissions_close_at,results_release_at) VALUES($1,$2,3,NOW()-INTERVAL '1 hour',NOW()+INTERVAL '1 hour')")
        .bind(series).bind(event_name).execute(&mut *tx).await.unwrap();
    for (id, name) in [(lower_id, "A lower seed"), (higher_id, "Z higher seed")] {
        sqlx::query("INSERT INTO users(id,racetime_id,racetime_display_name) VALUES($1,$2,$3)")
            .bind(id)
            .bind(format!("draft-seeding-{id}"))
            .bind(name)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO teams(id,series,event) VALUES($1,$2,$3)")
            .bind(id)
            .bind(series)
            .bind(event_name)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO team_members(team,member,status,role) VALUES($1,$1,'created','none')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    for position in 1..=3_i16 {
        let mode: i64 = sqlx::query_scalar("INSERT INTO qualifier_modes(series,event,position,slug,display_name,seed_gen_type,seed_config,generator_profile,settings_fingerprint) VALUES($1,$2,$3,$4,$4,'owr','{}','default','test') RETURNING id")
            .bind(series).bind(event_name).bind(position).bind(format!("mode-{position}"))
            .fetch_one(&mut *tx).await.unwrap();
        let seed: i64 = sqlx::query_scalar("INSERT INTO qualifier_seeds(series,event,mode_id,source,pool_position,generation_state,seed_data,generator_profile,physical_seed_identity,settings_fingerprint) VALUES($1,$2,$3,'async_pool',1,'ready','{}','default',$4,'test') RETURNING id")
            .bind(series).bind(event_name).bind(mode).bind(format!("draft-seeding-{position}"))
            .fetch_one(&mut *tx).await.unwrap();
        for (team, seconds) in [(lower_id, 5400_i32), (higher_id, 3600)] {
            sqlx::query("INSERT INTO qualifier_attempts(series,event,mode_id,seed_id,team_id,attempt_sequence,source,state,official_outcome,official_time,verified_at) VALUES($1,$2,$3,$4,$5,1,'async','finalized','finished',make_interval(secs => $6),NOW())")
                .bind(series).bind(event_name).bind(mode).bind(seed).bind(team).bind(f64::from(seconds))
                .execute(&mut *tx).await.unwrap();
        }
    }
    let mut event = event::Data::new(&mut tx, series, event_name)
        .await
        .unwrap()
        .unwrap();
    let mut lower = team::Team::from_id(&mut tx, lower_id.into())
        .await
        .unwrap()
        .unwrap();
    let mut higher = team::Team::from_id(&mut tx, higher_id.into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lower.qualifier_rank, None);
    assert_eq!(higher.qualifier_rank, None);
    let qualifier_kind = event.qualifier_kind(&mut tx, None).await.unwrap();
    let standings = teams::signups_sorted(
        &mut tx,
        &mut teams::Cache::new(client.clone()),
        None,
        &event,
        true,
        qualifier_kind,
        None,
        true,
        false,
    )
    .await
    .unwrap();
    assert_eq!(standings[0].team.as_ref().unwrap().id, higher.id);

    let options = vec![
        PresetOption {
            preset: "mcboss".into(),
            display_name: "McBoss".into(),
        },
        PresetOption {
            preset: "inverted".into(),
            display_name: "Inverted".into(),
        },
        PresetOption {
            preset: "cabookey".into(),
            display_name: "Cabookey".into(),
        },
    ];
    let kinds = [
        Kind::BanPick {
            options: options.clone(),
            can_defer: false,
            order: vec![
                DraftPhase::Pick(Team::HighSeed),
                DraftPhase::Pick(Team::LowSeed),
            ],
            label: "mode".into(),
        },
        Kind::PickOnly {
            options: options.clone(),
            who_starts: Team::HighSeed,
            picks_per_player: 1,
            unique: true,
            label: "mode".into(),
        },
        Kind::BanOnly {
            options,
            order: vec![
                DraftPhase::Ban(Team::HighSeed),
                DraftPhase::Ban(Team::LowSeed),
            ],
            label: "mode".into(),
        },
    ];
    // Public visibility must not replace sporting order with alphabetical order,
    // even when results have not been released yet.
    for hiding in [
        event::QualifierScoreHiding::None,
        event::QualifierScoreHiding::FullComplete,
    ] {
        event.qualifier_score_hiding = hiding;
        for kind in &kinds {
            for entrants in [[&lower, &higher], [&higher, &lower]] {
                let state = Draft::for_game1(&mut tx, &client, kind, &event, None, entrants)
                    .await
                    .unwrap();
                assert_eq!(state.high_seed, higher.id);
                let step = state
                    .next_step(kind, Some(1), &mut MessageContext::None)
                    .await
                    .unwrap();
                assert!(matches!(
                    step.kind,
                    StepKind::PickPreset {
                        team: Team::HighSeed,
                        ..
                    } | StepKind::Ban {
                        team: Team::HighSeed,
                        ..
                    }
                ));
            }
            // The fallback must not replace the configured pooled standings.
            let state = Draft::for_game1_with_seed_fallback(
                &mut tx,
                &client,
                kind,
                &event,
                None,
                [&lower, &higher],
                Some([1, 64]),
            )
            .await
            .unwrap();
            assert_eq!(state.high_seed, higher.id);
            let stale = Draft {
                high_seed: lower.id,
                went_first: Some(true),
                skipped_bans: 1,
                settings: HashMap::from_iter([(Cow::Borrowed("pick1"), Cow::Borrowed("mcboss"))]),
            };
            let reset = stale
                .reset(&mut tx, &client, kind, &event, [&lower, &higher])
                .await
                .unwrap();
            assert_eq!(reset.high_seed, higher.id);
            assert_eq!(reset.went_first, None);
            assert_eq!(reset.skipped_bans, 0);
            assert!(reset.settings.is_empty());
        }
    }

    // Old manually stored ranks must not override the event's pooled results.
    lower.qualifier_rank = Some(1);
    higher.qualifier_rank = Some(63);
    let state = Draft::for_game1(&mut tx, &client, &kinds[0], &event, None, [&lower, &higher])
        .await
        .unwrap();
    assert_eq!(state.high_seed, higher.id);

    // Formats that assign priority to the previous game's loser keep it on reset.
    let loser_first = Draft::for_next_game(&mut tx, &Kind::S7, lower.id, higher.id)
        .await
        .unwrap();
    let reset = loser_first
        .reset(&mut tx, &client, &Kind::S7, &event, [&lower, &higher])
        .await
        .unwrap();
    assert_eq!(reset.high_seed, lower.id);

    // Completing all required modes outranks a better average in only two modes.
    sqlx::query("UPDATE qualifier_attempts SET state='void',official_outcome=NULL,official_time=NULL WHERE team_id=$1 AND mode_id=(SELECT MAX(id) FROM qualifier_modes WHERE series=$2 AND event=$3)")
        .bind(higher_id).bind(series).bind(event_name).execute(&mut *tx).await.unwrap();
    let state = Draft::for_game1(&mut tx, &client, &kinds[0], &event, None, [&higher, &lower])
        .await
        .unwrap();
    assert_eq!(state.high_seed, lower.id);

    // Missing entrants must be reported, not silently assigned bracket-slot order.
    sqlx::query("UPDATE teams SET resigned=TRUE WHERE id=$1")
        .bind(higher_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = Draft::for_game1(&mut tx, &client, &kinds[0], &event, None, [&lower, &higher])
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing from pooled qualifier standings")
    );

    // Events using stored ranks retain their existing seeding behavior.
    event.qualifier_mode = "rank".into();
    lower.qualifier_rank = Some(63);
    higher.qualifier_rank = Some(21);
    let state = Draft::for_game1(&mut tx, &client, &kinds[0], &event, None, [&lower, &higher])
        .await
        .unwrap();
    assert_eq!(state.high_seed, higher.id);

    // An imported event without local ranks uses the supplied start.gg seeds,
    // and resets preserve that assignment after the import data is gone.
    lower.qualifier_rank = None;
    higher.qualifier_rank = None;
    for kind in &kinds {
        let state = Draft::for_game1_with_seed_fallback(
            &mut tx,
            &client,
            kind,
            &event,
            None,
            [&lower, &higher],
            Some([63, 21]),
        )
        .await
        .unwrap();
        assert_eq!(state.high_seed, higher.id);
        let reset = state
            .reset(&mut tx, &client, kind, &event, [&lower, &higher])
            .await
            .unwrap();
        assert_eq!(reset.high_seed, higher.id);
    }
    tx.rollback().await.unwrap();
}
