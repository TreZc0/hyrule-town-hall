use super::*;
use crate::racetime_bot::seed_gen_type::OwrEventConfig;

fn modes() -> OwrEventConfig {
    OwrEventConfig::parse(&serde_json::json!({
        "default_baseline": "a",
        "baselines": {
            "a": {"label": "Mode A", "base_settings": {}},
            "b": {"label": "Mode B", "base_settings": {}},
            "c": {"label": "Mode C", "base_settings": {}}
        }
    }))
    .unwrap()
}

#[test]
fn custom_mode_accepts_event_defaults_and_valid_overrides() {
    let config = modes();
    for key in ["a", "b", "c"] {
        assert_eq!(
            validate_custom_baseline(Some(&config), true, key)
                .unwrap()
                .as_deref(),
            Some(key)
        );
    }
    assert_eq!(
        validate_custom_baseline(Some(&config), true, "").unwrap(),
        None
    );
    for key in ["missing", " b "] {
        assert!(validate_custom_baseline(Some(&config), true, key).is_err());
    }
}

#[test]
fn custom_mode_cannot_override_a_normal_match_or_an_unconfigured_event() {
    let config = modes();
    assert_eq!(
        validate_custom_baseline(Some(&config), false, "").unwrap(),
        None
    );
    assert!(validate_custom_baseline(Some(&config), false, "b").is_err());
    assert_eq!(validate_custom_baseline(None, true, "").unwrap(), None);
    assert!(validate_custom_baseline(None, true, "b").is_err());
    assert!(validate_custom_baseline(Some(&OwrEventConfig::default()), true, "b").is_err());
}

#[test]
fn custom_mode_form_keeps_selection_and_defaults_disabled_game_count_to_one() {
    let form = Form::<CreateRaceForm>::parse(
        "custom_title=Tiebreaker&multi_teams=1&multi_teams=2&custom_baselines=b",
    )
    .unwrap();
    assert_eq!(form.custom_baselines, vec!["b"]);
    assert_eq!(form.game_count, 1);
    assert_eq!(form.multi_teams.len(), 2);
    let regular = Form::<CreateRaceForm>::parse("team1=1&team2=2&game_count=3").unwrap();
    assert_eq!(regular.game_count, 3);
    assert!(regular.custom_baselines.is_empty());
}

#[test]
fn custom_match_modes_support_bo2_bo3_and_optional_overrides() {
    let config = modes();
    for count in [1, 2, 3] {
        assert_eq!(
            validate_custom_modes(Some(&config), true, count, &[]).unwrap(),
            vec![None; count as usize]
        );
    }
    assert_eq!(
        validate_custom_modes(Some(&config), true, 3, &["".into(), "b".into(), "".into()]).unwrap(),
        vec![None, Some("b".into()), None]
    );
    assert_eq!(
        validate_custom_modes(Some(&config), true, 2, &["a".into(), "a".into()]).unwrap(),
        vec![Some("a".into()); 2]
    );
    assert!(
        validate_custom_modes(
            Some(&config),
            true,
            2,
            &["a".into(), "b".into(), "c".into()]
        )
        .is_err()
    );
    assert!(validate_custom_modes(Some(&config), true, 3, &["missing".into()]).is_err());
    for count in [0, -1, 256] {
        assert!(validate_custom_modes(Some(&config), true, count, &[]).is_err());
    }
    let form = Form::<CreateRaceForm>::parse("custom_title=Final&multi_teams=1&multi_teams=2&game_count=3&custom_baselines=&custom_baselines=b&custom_baselines=").unwrap();
    assert_eq!(form.game_count, 3);
    assert_eq!(form.custom_baselines, vec!["", "b", ""]);
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
async fn database_custom_matches_group_games_and_preserve_draft_overrides() {
    let pool = event::configuration::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let client = reqwest::Client::new();
    let slug = format!("match-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let config = json!({"baselines": modes().baselines, "default_baseline": "a"});
    let draft_config = json!({"label":"mode", "options":[
        {"display_name":"Mode A", "preset":"a"},
        {"display_name":"Mode B", "preset":"b"},
        {"display_name":"Mode C", "preset":"c"}
    ], "order":[{"phase":"pick", "team":"high_seed"},{"phase":"pick", "team":"low_seed"}]});
    sqlx::query("INSERT INTO events (series,event,display_name,team_config,seed_gen_type,seed_config,draft_kind,draft_config) VALUES ('alttprmain',$1,'Custom match test','solo','owr',$2,'ban_pick',$3)")
        .bind(&slug).bind(config).bind(draft_config).execute(&mut *tx).await.unwrap();
    let event = event::Data::new(&mut tx, Series::AlttprMain, slug.as_str())
        .await
        .unwrap()
        .unwrap();
    let kind = event.draft_kind().unwrap();
    let mut teams = Vec::new();
    for rank in 1..=2 {
        let id = Id::<Teams>::new(&mut tx).await.unwrap();
        sqlx::query(
            "INSERT INTO teams (id,series,event,qualifier_rank) VALUES ($1,'alttprmain',$2,$3)",
        )
        .bind(i64::from(id))
        .bind(&slug)
        .bind(rank as i16)
        .execute(&mut *tx)
        .await
        .unwrap();
        teams.push(Team::from_id(&mut tx, id).await.unwrap().unwrap());
    }
    let draft = Draft::for_game1(
        &mut tx,
        &client,
        &kind,
        &event,
        None,
        [&teams[0], &teams[1]],
    )
    .await
    .unwrap();
    let id = Id::<Races>::new(&mut tx).await.unwrap();
    sqlx::query("INSERT INTO races (id,series,event) VALUES ($1,'alttprmain',$2)")
        .bind(i64::from(id))
        .bind(&slug)
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut template = Race::from_id(&mut tx, &client, id).await.unwrap();
    template.custom_title = Some("Tiebreaker".into());
    template.entrants = Entrants::Two([
        Entrant::MidosHouseTeam(teams[0].clone()),
        Entrant::MidosHouseTeam(teams[1].clone()),
    ]);
    template.draft = Some(draft.clone());
    let mut first = custom_match_games(
        &mut tx,
        template.clone(),
        vec![None, Some("c".into()), None],
    )
    .await
    .unwrap();
    template.id = Id::<Races>::new(&mut tx).await.unwrap();
    template.draft = None;
    let second = custom_match_games(&mut tx, template, vec![Some("a".into()); 2])
        .await
        .unwrap();
    assert_ne!(first[0].custom_match_id, second[0].custom_match_id);
    for race in first.iter().chain(&second) {
        race.save(&mut tx).await.unwrap();
    }
    for games in [&first, &second] {
        for (index, race) in games.iter().enumerate() {
            let loaded = Race::from_id(&mut tx, &client, race.id).await.unwrap();
            assert_eq!(loaded.game, Some(index as i16 + 1));
            assert_eq!(loaded.custom_match_id, games[0].custom_match_id);
            assert_eq!(
                loaded.game_count(&mut tx).await.unwrap(),
                games.len() as i16
            );
            assert_eq!(
                loaded
                    .next_game(&mut tx, &client)
                    .await
                    .unwrap()
                    .map(|race| race.id),
                games.get(index + 1).map(|race| race.id)
            );
        }
    }
    let start = Utc::now() + TimeDelta::hours(4);
    assert!(
        first[1]
            .live_schedule_order_error(&mut tx, start)
            .await
            .unwrap()
            .is_some()
    );
    first[0]
        .schedule
        .set_live_start(start - TimeDelta::hours(1));
    first[0].save(&mut tx).await.unwrap();
    assert!(
        first[1]
            .live_schedule_order_error(&mut tx, start)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        second[1]
            .live_schedule_order_error(&mut tx, start)
            .await
            .unwrap()
            .is_some()
    );

    let mut completed = draft;
    for mode in ["a", "b"] {
        completed
            .apply(
                &kind,
                Some(1),
                &mut draft::MessageContext::None,
                draft::Action::Pick {
                    setting: mode.into(),
                    value: String::new(),
                },
            )
            .await
            .unwrap()
            .unwrap();
    }
    first[0].draft = Some(completed.clone());
    first[0]
        .copy_draft_to_remaining_games(&mut tx, &completed)
        .await
        .unwrap();
    let config =
        racetime_bot::choice_resolution::config(event.seed_gen_type.as_ref().unwrap()).unwrap();
    for (race, expected) in first.iter().zip(["a", "c", "c"]) {
        let loaded = Race::from_id(&mut tx, &client, race.id).await.unwrap();
        assert_eq!(
            config
                .for_race(&loaded, &event)
                .await
                .unwrap()
                .selected_baseline
                .unwrap()
                .0,
            expected
        );
    }
    for race in &second {
        let loaded = Race::from_id(&mut tx, &client, race.id).await.unwrap();
        assert!(loaded.draft.is_none());
        assert_eq!(loaded.custom_baseline.as_deref(), Some("a"));
    }
    let mut step = completed
        .next_step(&kind, Some(1), &mut draft::MessageContext::None)
        .await
        .unwrap();
    racetime_bot::baselines::format_draft_step(&event, &first[0], &mut step, &mut tx)
        .await
        .unwrap();
    assert!(step.message.contains("Game 2: Mode C"));

    let thread = ChannelId::new(42);
    for race in &mut first {
        race.scheduling_thread = Some(thread);
        race.save(&mut tx).await.unwrap();
    }
    let linked = Race::for_scheduling_channel(&mut tx, &client, thread, None, true)
        .await
        .unwrap();
    assert_eq!(linked.len(), 3);
    let game2 = Race::for_scheduling_channel(&mut tx, &client, thread, Some(2), true)
        .await
        .unwrap();
    assert_eq!(game2.len(), 1);
    assert_eq!(game2[0].id, first[1].id);

    // Ordinary manual matches must not count, draft, or delete custom games
    // with the same entrants and empty phase/round fields.
    let mut normal = first[0].clone();
    normal.id = Id::<Races>::new(&mut tx).await.unwrap();
    normal.custom_title = None;
    normal.custom_match_id = None;
    normal.custom_baseline = None;
    normal.scheduling_thread = None;
    normal.schedule = RaceSchedule::Unscheduled;
    normal.save(&mut tx).await.unwrap();
    let mut normal_second = normal.clone();
    normal_second.id = Id::<Races>::new(&mut tx).await.unwrap();
    normal_second.game = Some(2);
    normal_second.save(&mut tx).await.unwrap();
    assert_eq!(normal.game_count(&mut tx).await.unwrap(), 2);
    assert_eq!(
        normal
            .next_game(&mut tx, &client)
            .await
            .unwrap()
            .unwrap()
            .id,
        normal_second.id
    );
    assert!(
        normal_second
            .live_schedule_order_error(&mut tx, start)
            .await
            .unwrap()
            .is_some()
    );
    normal
        .copy_draft_to_remaining_games(&mut tx, &completed)
        .await
        .unwrap();
    assert!(
        Race::from_id(&mut tx, &client, second[1].id)
            .await
            .unwrap()
            .draft
            .is_none()
    );
    assert_eq!(
        normal.ignore_remaining_games(&mut tx).await.unwrap(),
        vec![normal_second.id]
    );
    assert_eq!(
        first[1].ignore_remaining_games(&mut tx).await.unwrap(),
        vec![first[2].id]
    );
    assert!(
        !Race::from_id(&mut tx, &client, second[1].id)
            .await
            .unwrap()
            .ignored
    );
    tx.rollback().await.unwrap();
}
