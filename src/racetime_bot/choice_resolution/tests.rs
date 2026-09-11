use super::*;

fn fixture(timing: Timing) -> OwrEventConfig {
    OwrEventConfig {
        choice_resolution: timing,
        base_settings: json!({"goal": "crystals"}),
        choices: json!({
            "option": {"label": "Option", "settings": {"goal": "dungeons"}},
            "delay": {"label": "No delay", "hidden_for_async": true, "value_labels": {"always": "No delay agreed", "never": "Use stream delay"}}
        }),
        ..OwrEventConfig::default()
    }
}

#[test]
fn configuration_defaults_and_named_baselines_preserve_timing() {
    assert_eq!(
        OwrEventConfig::parse(&json!({"base_settings": {}}))
            .unwrap()
            .choice_resolution,
        Timing::SeedRolling
    );
    for (value, timing) in [
        ("race_creation", Timing::RaceCreation),
        ("room_opening", Timing::RoomOpening),
        ("seed_rolling", Timing::SeedRolling),
    ] {
        let config = OwrEventConfig::parse(&json!({"choice_resolution": value, "baselines": {"a": {"label": "A", "base_settings": {}}}, "default_baseline": "a"})).unwrap();
        assert_eq!(config.select(None).unwrap().choice_resolution, timing);
    }
    assert!(
        OwrEventConfig::parse(&json!({"base_settings": {}, "choice_resolution": "typo"})).is_err()
    );
}

#[test]
fn saved_outcomes_preserve_random_reveal_and_async_rule_filtering() {
    let config = fixture(Timing::RaceCreation);
    let snapshot = Snapshot {
        teams: vec![1, 2],
        definitions: config.choices.clone(),
        preferences: [
            ("option".into(), ChoiceValue::Random),
            ("delay".into(), ChoiceValue::Random),
        ]
        .into(),
        resolved: [("option".into(), false), ("delay".into(), true)].into(),
        timing: Timing::RaceCreation,
    };
    let restored: Snapshot =
        serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap();
    assert_eq!(restored.values()["option"], ChoiceValue::Never);
    assert!(
        restored
            .reveal(&config, &[])
            .unwrap()
            .contains("Option: Disabled")
    );
    assert!(restored.display(false).contains("No delay agreed"));
    assert!(!restored.display(true).contains("No delay agreed"));
    assert!(restored.validate(&[1, 3], &config).is_err());
    let mut changed = config.clone();
    changed.choices["option"]["settings"]["goal"] = json!("other");
    assert!(restored.validate(&[1, 2], &changed).is_err());
    changed = config;
    changed.choice_resolution = Timing::SeedRolling;
    assert!(restored.validate(&[1, 2], &changed).is_ok());
}

#[test]
fn display_identifies_suppressed_patches() {
    let mut config = fixture(Timing::RaceCreation);
    config.choices["stronger"] =
        json!({"label": "Stronger", "settings": {"goal": "other"}, "supercedes": ["option"]});
    let snapshot = Snapshot {
        teams: vec![1, 2],
        definitions: config.choices,
        preferences: HashMap::new(),
        resolved: [("option".into(), true), ("stronger".into(), true)].into(),
        timing: Timing::RaceCreation,
    };
    assert!(
        snapshot
            .display(false)
            .contains("Option: not applied (superseded by Stronger)")
    );
}

fn race(id: i64) -> Race {
    let mut first = Team::dummy();
    first.id = 1_i64.into();
    let mut second = Team::dummy();
    second.id = 2_i64.into();
    Race {
        id: id.into(),
        series: Series::AlttprDe,
        event: "choice-test".into(),
        source: cal::Source::Manual,
        entrants: Entrants::Two([
            Entrant::MidosHouseTeam(first),
            Entrant::MidosHouseTeam(second),
        ]),
        is_qualifier: false,
        qualifier_number: None,
        phase: None,
        round: None,
        game: Some(id as i16),
        scheduling_thread: None,
        schedule: RaceSchedule::Unscheduled,
        schedule_updated_at: None,
        fpa_invoked: false,
        breaks_used: false,
        draft: None,
        seed: seed::Data::default(),
        video_urls: HashMap::new(),
        restreamers: HashMap::new(),
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
        scheduling_deadline: None,
        restream_consent_required: false,
        custom_title: None,
        custom_create_room: true,
        companion_race_id: None,
    }
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a *_test database; uses an isolated schema"]
async fn database_timing_retries_concurrency_and_per_game_storage() {
    let admin = event::configuration::test_pool().await;
    let schema = format!("choice_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = format!("SET search_path TO {schema}");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |conn, _| {
            let sql = search_path.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&std::env::var("HTH_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE races (id BIGINT PRIMARY KEY, team1 BIGINT, team2 BIGINT, team3 BIGINT); CREATE TABLE teams (id BIGINT PRIMARY KEY, custom_choices JSONB); CREATE TABLE race_entrants (race BIGINT, team BIGINT);")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/113_resolved_race_settings.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO teams VALUES (1, '{\"option\":\"random\",\"delay\":\"random\"}'), (2, '{\"option\":\"always\",\"delay\":\"random\"}')").execute(&pool).await.unwrap();
    for (id, timing) in [
        (1, Timing::RaceCreation),
        (2, Timing::RoomOpening),
        (3, Timing::SeedRolling),
    ] {
        sqlx::query("INSERT INTO races (id, team1, team2) VALUES ($1, 1, 2)")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let race = race(id);
        let config = fixture(timing);
        let mut first = None;
        for stage in [
            Timing::RaceCreation,
            Timing::RoomOpening,
            Timing::SeedRolling,
        ] {
            let mut transaction = pool.begin().await.unwrap();
            let result = ensure(&mut transaction, &race, &config, stage)
                .await
                .unwrap();
            transaction.commit().await.unwrap();
            assert_eq!(result.is_some(), stage >= timing);
            if let Some(result) = result {
                let original = first.get_or_insert_with(|| result.resolved.clone());
                assert_eq!(&result.resolved, original);
            }
        }
        // Simulate a new process and both async handlers requesting the same seed at once.
        let (a, b) = tokio::join!(
            for_seed(&pool, &race, &config),
            for_seed(&pool, &race, &config)
        );
        assert_eq!(a.unwrap().unwrap().resolved, first.clone().unwrap());
        assert_eq!(b.unwrap().unwrap().resolved, first.unwrap());
    }
    // A concurrent first resolution must also choose only once.
    sqlx::query("INSERT INTO races (id, team1, team2) VALUES (4, 1, 2)")
        .execute(&pool)
        .await
        .unwrap();
    let game4 = race(4);
    let config = fixture(Timing::SeedRolling);
    let (a, b) = tokio::join!(
        for_seed(&pool, &game4, &config),
        for_seed(&pool, &game4, &config)
    );
    assert_eq!(a.unwrap().unwrap().resolved, b.unwrap().unwrap().resolved);
    let original = read(&pool, game4.id).await.unwrap().unwrap();
    sqlx::query("UPDATE teams SET custom_choices = '{}'::jsonb")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        for_seed(&pool, &game4, &config)
            .await
            .unwrap()
            .unwrap()
            .resolved,
        original.resolved
    );
    // The next game uses the new preferences; existing games remain untouched.
    sqlx::query("INSERT INTO races (id, team1, team2) VALUES (5, 1, 2)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        for_seed(&pool, &race(5), &config)
            .await
            .unwrap()
            .unwrap()
            .resolved
            .values()
            .all(|enabled| !enabled)
    );
    sqlx::query("UPDATE races SET team2 = 3 WHERE id = 4")
        .execute(&pool)
        .await
        .unwrap();
    assert!(for_seed(&pool, &game4, &config).await.is_err());
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
