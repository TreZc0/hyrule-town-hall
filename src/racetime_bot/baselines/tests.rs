use super::*;
use crate::{
    event::configuration,
    racetime_bot::{
        self, ChoiceValue, SeedRollUpdate, UnlockSpoilerLog,
        seed_gen_type::{AlttprDrSource, SeedGenType},
    },
};

fn fixture() -> serde_json::Value {
    json!({
        "baselines": {
            "a": {"label": "Mode A", "base_settings": {"goal": "crystals", "remove_me": true}, "base_placements": {"location": "A"}, "start_inventory": ["A"]},
            "b": {"label": "Mode B", "base_settings": {"goal": "dungeons", "remove_me": false}, "base_placements": {"location": "B"}, "start_inventory": ["B"]},
            "c": {"label": "Mode C", "base_settings": {"goal": "completionist"}, "base_placements": {}, "start_inventory": []}
        },
        "choices": {
            "option": {"label": "Option", "settings": {"goal": "other", "remove_me": null}, "placements": {"location": "patched"}, "start_inventory": ["Boots"]},
            "delay": {"label": "No delay", "hidden_for_async": true, "value_labels": {"always": "No delay agreed", "never": "Use stream delay"}}
        }
    })
}

fn draft_config() -> serde_json::Value {
    json!({"label": "mode", "options": [{"display_name": "Mode A", "preset": "a"}, {"display_name": "Mode B", "preset": "b"}, {"display_name": "Mode C", "preset": "c"}], "order": [{"phase": "pick", "team": "high_seed"}, {"phase": "pick", "team": "low_seed"}]})
}

#[test]
fn configuration_references_defaults_and_legacy_formats() {
    let config = fixture();
    for generator in ["owr", "owr_tourney", "alttpr_dr"] {
        let mut value = config.clone();
        if generator == "alttpr_dr" {
            value["source"] = json!("mutual_choices");
        }
        configuration::validate_seed(Some(generator), Some(&value)).unwrap();
        assert!(SeedGenType::from_db(Some(generator), Some(&value)).is_some());
        for count in [2, 3] {
            configuration::validate_draft(
                Some("ban_pick"),
                Some(&draft_config()),
                Some(generator),
                Some(&value),
                Some(count),
            )
            .unwrap();
        }
        assert!(
            configuration::validate_draft(
                Some("ban_pick"),
                Some(&draft_config()),
                Some(generator),
                Some(&value),
                Some(4)
            )
            .is_err()
        );
        assert!(
            configuration::validate_draft(None, None, Some(generator), Some(&value), None).is_err()
        );
        value["default_baseline"] = json!("b");
        configuration::validate_draft(None, None, Some(generator), Some(&value), None).unwrap();
        assert_eq!(
            OwrEventConfig::parse(&value)
                .unwrap()
                .select(None)
                .unwrap()
                .base_settings["goal"],
            "dungeons"
        );
    }
    let mut invalid = config.clone();
    invalid["baselines"]["a"]["base_settings"] = json!("bad");
    assert!(OwrEventConfig::parse(&invalid).is_err());
    invalid["source"] = json!("mutual_choices");
    assert!(SeedGenType::from_db(Some("alttpr_dr"), Some(&invalid)).is_none());
    for patch in [
        json!({"base_settings": {}}),
        json!({"start_inventory": []}),
        json!({"baselines": {}}),
        json!({"baselines": null}),
        json!({"default_baseline": "missing"}),
    ] {
        let mut invalid = config.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(OwrEventConfig::parse(&invalid).is_err(), "{invalid}");
    }
    let mut draft = draft_config();
    draft["options"][0]["preset"] = json!("missing");
    assert!(
        configuration::validate_draft(
            Some("ban_pick"),
            Some(&draft),
            Some("owr"),
            Some(&config),
            Some(2)
        )
        .is_err()
    );
    let legacy = json!({"base_settings": {"goal": "crystals"}, "choices": {"flute": {"flute_mode": "active"}}});
    assert_eq!(
        OwrEventConfig::parse(&legacy)
            .unwrap()
            .select(None)
            .unwrap()
            .base_settings,
        legacy["base_settings"]
    );
}

#[tokio::test]
async fn all_six_drafts_select_current_game_and_leave_rr_decider_unused() {
    let mut value = fixture();
    value["default_baseline"] = json!("a");
    let config = OwrEventConfig::parse(&value).unwrap();
    let kind = draft::Kind::from_db(Some("ban_pick"), Some(&draft_config())).unwrap();
    for first in ["a", "b", "c"] {
        for second in ["a", "b", "c"].into_iter().filter(|s| *s != first) {
            let mut state = Draft {
                high_seed: 1_i64.into(),
                went_first: None,
                skipped_bans: 0,
                settings: draft::Picks::new(),
            };
            assert!(
                config
                    .for_draft(Some(&state), Some(&kind), Some(1))
                    .await
                    .is_err()
            );
            for (index, key) in [first, second].into_iter().enumerate() {
                assert!(
                    state
                        .is_active_team(
                            &kind,
                            Some(1),
                            if index == 0 { 1_i64 } else { 2_i64 }.into()
                        )
                        .await
                        .unwrap()
                );
                state
                    .apply(
                        &kind,
                        Some(1),
                        &mut draft::MessageContext::None,
                        draft::Action::Pick {
                            setting: key.into(),
                            value: String::new(),
                        },
                    )
                    .await
                    .unwrap()
                    .unwrap();
                state = serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
                if index == 0 {
                    assert!(
                        config
                            .for_draft(Some(&state), Some(&kind), Some(1))
                            .await
                            .is_err()
                    );
                    assert!(
                        state
                            .apply(
                                &kind,
                                Some(1),
                                &mut draft::MessageContext::None,
                                draft::Action::Pick {
                                    setting: key.into(),
                                    value: String::new()
                                }
                            )
                            .await
                            .unwrap()
                            .is_err()
                    );
                }
            }
            let third = ["a", "b", "c"]
                .into_iter()
                .find(|s| *s != first && *s != second)
                .unwrap();
            for (i, key) in [first, second, third].into_iter().enumerate() {
                let selected = config
                    .for_draft(Some(&state), Some(&kind), Some(i as i16 + 1))
                    .await
                    .unwrap();
                assert_eq!(selected.selected_baseline.unwrap().0, key);
                assert_eq!(
                    selected.base_settings,
                    value["baselines"][key]["base_settings"]
                );
            }
            assert!(!config.draft_summary(&state, 2).unwrap().contains("Game 3"));
            assert!(
                config
                    .draft_summary(&state, 3)
                    .unwrap()
                    .contains("(if needed)")
            );
            assert!(
                config
                    .for_draft(Some(&state), Some(&kind), Some(4))
                    .await
                    .is_err()
            );
            state
                .settings
                .insert("game2_preset".into(), "deleted".into());
            assert!(
                config
                    .for_draft(Some(&state), Some(&kind), Some(2))
                    .await
                    .is_err()
            );
        }
    }
}

#[test]
fn mutual_choices_resolve_once_and_patch_each_selected_baseline() {
    let config = OwrEventConfig::parse(&fixture()).unwrap();
    for key in ["a", "b", "c"] {
        let selected = config.select(Some(key)).unwrap();
        for a in ["never", "random", "always"] {
            for b in ["never", "random", "always"] {
                let expected = if a == "never" || b == "never" {
                    ChoiceValue::Never
                } else if a == "random" || b == "random" {
                    ChoiceValue::Random
                } else {
                    ChoiceValue::Always
                };
                let choices = racetime_bot::resolve_choice_values([
                    &json!({"option": a}),
                    &json!({"option": b}),
                ]);
                assert_eq!(choices.get("option").copied().unwrap_or_default(), expected);
                for coin in [false, true] {
                    let mut calls = 0;
                    let resolved =
                        racetime_bot::resolve_all_choices_with(&choices, &selected, || {
                            calls += 1;
                            coin
                        });
                    assert_eq!(calls, usize::from(expected == ChoiceValue::Random));
                    let enabled =
                        expected == ChoiceValue::Always || expected == ChoiceValue::Random && coin;
                    let (yaml, report) =
                        racetime_bot::build_dr_yaml_with_report(&selected, &resolved, Uuid::nil())
                            .unwrap();
                    let yaml: serde_json::Value = serde_yml::from_str(&yaml).unwrap();
                    assert_eq!(
                        yaml["settings"]["1"]["goal"],
                        if enabled {
                            json!("other")
                        } else {
                            selected.base_settings["goal"].clone()
                        }
                    );
                    if enabled {
                        assert!(yaml["settings"]["1"].get("remove_me").is_none());
                        assert_eq!(yaml["placements"]["1"]["location"], "patched");
                        let mut inventory = selected.start_inventory.clone();
                        inventory.push("Boots".into());
                        assert_eq!(yaml["start_inventory"]["1"], json!(inventory));
                    }
                    let display = presentation(&selected, &resolved, &report).unwrap();
                    assert!(
                        display["settings_summary"]
                            .as_str()
                            .unwrap()
                            .contains(if enabled {
                                "Option: applied"
                            } else {
                                "Option: not applied"
                            })
                    );
                    assert!(
                        display["settings_summary"]
                            .as_str()
                            .unwrap()
                            .contains("Use stream delay")
                    );
                    assert!(
                        !display["async_settings_summary"]
                            .as_str()
                            .unwrap()
                            .contains("delay")
                    );
                }
            }
        }
    }
    assert_eq!(
        racetime_bot::resolve_choice_values([&json!({"x": "yes"}), &json!({"x": true})])["x"],
        ChoiceValue::Always
    );
    assert!(racetime_bot::resolve_choice_values([&json!({"x": "always"}), &json!({})]).is_empty());
    assert!(
        racetime_bot::resolve_choice_values([&json!({"x": "yes"}), &json!({"x": "no"})]).is_empty()
    );
    assert_eq!(
        config.baselines.as_ref().unwrap()["a"].base_settings["goal"],
        "crystals"
    );
}

#[tokio::test]
async fn presentation_survives_delivery_reload_and_config_changes() {
    let mut value = fixture();
    value["choices"]["override"] =
        json!({"label": "Override", "priority": 10, "settings": {"goal": "final"}});
    value["choices"]["suppress"] =
        json!({"label": "Suppressor", "settings": {"mode": "open"}, "supercedes": ["override"]});
    let config = OwrEventConfig::parse(&value)
        .unwrap()
        .select(Some("b"))
        .unwrap();
    let resolved = HashMap::from([("option".into(), true), ("override".into(), true)]);
    let (_, report) =
        racetime_bot::build_dr_yaml_with_report(&config, &resolved, Uuid::nil()).unwrap();
    let display = presentation(&config, &resolved, &report).unwrap();
    assert!(
        display["settings_summary"]
            .as_str()
            .unwrap()
            .contains("goal by Override overridden")
    );
    let mut resolved = resolved;
    resolved.insert("suppress".into(), true);
    let (_, report) =
        racetime_bot::build_dr_yaml_with_report(&config, &resolved, Uuid::nil()).unwrap();
    let display = presentation(&config, &resolved, &report).unwrap();
    assert!(
        display["settings_summary"]
            .as_str()
            .unwrap()
            .contains("Override: not applied (superseded by Suppressor)")
    );
    let uuid = Uuid::new_v4();
    let (tx, rx) = mpsc::channel(1);
    tx.send(SeedRollUpdate::Done {
        seed: seed::Data {
            seed_data: Some(
                seed::Files::AlttprDoorRando { uuid, is_owr: true }.to_seed_data_base(),
            ),
            ..Default::default()
        },
        resolved_randoms: None,
        rsl_preset: None,
        version: None,
        unlock_spoiler_log: UnlockSpoilerLog::Never,
    })
    .await
    .unwrap();
    drop(tx);
    let mut updates = with_presentation(rx, Some(display));
    let SeedRollUpdate::Done { seed, .. } = updates.recv().await.unwrap() else {
        panic!()
    };
    let saved = seed.to_seed_data().unwrap();
    value["baselines"]["b"]["label"] = json!("Changed later");
    let reloaded = seed::Data::from_db(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        None,
        None,
        Some(saved.clone()),
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );
    assert_eq!(reloaded.to_seed_data().unwrap(), saved);
    assert!(
        matches!(reloaded.files(), Some(seed::Files::AlttprDoorRando { uuid: id, is_owr: true }) if id == uuid)
    );
    for is_async in [false, true] {
        let summary = seed_summary(&saved, is_async).unwrap();
        assert!(summary.starts_with("Baseline: Mode B"));
        assert_eq!(
            summary,
            seed_summary(reloaded.seed_data.as_ref().unwrap(), is_async).unwrap()
        );
    }
    assert_eq!(
        seed_summary(&json!({"resolved_randoms": "old result"}), false).as_deref(),
        Some("Final settings - old result")
    );
}

#[test]
fn pooled_modes_keep_explicit_baselines_and_chunks_keep_all_text() {
    for generator in ["owr", "owr_tourney", "alttpr_dr"] {
        let mut value = fixture();
        value["source"] = json!("mutual_choices");
        let kind = SeedGenType::from_db(Some(generator), Some(&value)).unwrap();
        assert!(!event::pooled_qualifiers::generation::supported(&kind));
        let legacy = json!({"source": "mutual_choices", "base_settings": {"goal": "crystals"}});
        let kind = SeedGenType::from_db(Some(generator), Some(&legacy)).unwrap();
        assert!(event::pooled_qualifiers::generation::supported(&kind));
        assert!(matches!(
            kind,
            SeedGenType::Owr { .. }
                | SeedGenType::AlttprDoorRando {
                    source: AlttprDrSource::MutualChoices { .. },
                    ..
                }
        ));
    }
    let text = format!("Baseline: Mode A\n{}\n", "é🦉 options ".repeat(500));
    let chunks = message_chunks(&text);
    assert!(chunks.iter().all(|s| s.len() <= 900));
    assert_eq!(chunks.concat(), text);
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
async fn database_named_draft_propagation_seed_reuse_and_unused_games() {
    let pool = configuration::test_pool().await;
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
    event.draft_kind_str = Some("ban_pick".into());
    event.draft_config = Some(draft_config());
    event.round_modes = None;
    let config = OwrEventConfig::parse(&fixture()).unwrap();
    event.seed_gen_type = Some(SeedGenType::Owr {
        config: config.clone(),
        build: racetime_bot::seed_gen_type::OwrBuild::Tournament,
    });
    assert!(validate_match(&event, 2).is_ok());
    assert!(validate_match(&event, 3).is_ok());
    assert!(validate_match(&event, 4).is_err());
    let kind = event.draft_kind().unwrap();
    let mut higher = Team::dummy();
    higher.id = 1_i64.into();
    higher.qualifier_rank = Some(1);
    let mut lower = Team::dummy();
    lower.id = 2_i64.into();
    lower.qualifier_rank = Some(2);
    let mut state = Draft::for_game1(&mut tx, &client, &kind, &event, None, [&lower, &higher])
        .await
        .unwrap();
    assert_eq!(state.high_seed, higher.id);
    let mut cached_race = base.clone();
    cached_race.game = Some(2);
    cached_race.draft = Some(state.clone());
    for key in ["b", "c"] {
        state
            .apply(
                &kind,
                Some(1),
                &mut draft::MessageContext::None,
                draft::Action::Pick {
                    setting: key.into(),
                    value: String::new(),
                },
            )
            .await
            .unwrap()
            .unwrap();
    }
    // Live rooms retain a cached race while Discord polling and rerolls update
    // the active draft. The generation handoff must carry that completed state.
    assert!(config.for_race(&cached_race, &event).await.is_err());
    cached_race.draft = Some(state.clone());
    assert_eq!(
        config
            .for_race(&cached_race, &event)
            .await
            .unwrap()
            .selected_baseline
            .unwrap()
            .0,
        "c"
    );
    assert!(config.for_draft(Some(&state), None, Some(2)).await.is_err());
    for count in [2, 3] {
        let phase = format!("named-baseline-test-{}", Uuid::new_v4());
        let mut games = Vec::new();
        for game in 1..=count {
            let mut race = base.clone();
            race.id = ids[game as usize - 1].into();
            race.source = cal::Source::Manual;
            race.phase = Some(phase.clone());
            race.round = Some("round".into());
            race.game = Some(game);
            race.ignored = false;
            race.draft = (game == 1).then(|| state.clone());
            race.seed = seed::Data::default();
            race.save(&mut tx).await.unwrap();
            games.push(race);
        }
        games[0]
            .copy_draft_to_remaining_games(&mut tx, &state)
            .await
            .unwrap();
        assert_eq!(games[0].game_count(&mut tx).await.unwrap(), count);
        let mut step = state
            .next_step(&kind, Some(1), &mut draft::MessageContext::None)
            .await
            .unwrap();
        format_draft_step(&event, &games[0], &mut step, &mut tx)
            .await
            .unwrap();
        assert_eq!(step.message.contains("Game 3"), count == 3);

        for (index, original) in games.iter().enumerate() {
            let mut race = Race::from_id(&mut tx, &client, original.id).await.unwrap();
            let selected = config.for_race(&race, &event).await.unwrap();
            assert_eq!(
                selected.selected_baseline.as_ref().unwrap().0,
                ["b", "c", "a"][index]
            );
            let resolved = HashMap::from([("option".into(), index == 0)]);
            let (_, report) =
                racetime_bot::build_dr_yaml_with_report(&selected, &resolved, Uuid::nil()).unwrap();
            let mut data = seed::Files::AlttprDoorRando {
                uuid: Uuid::new_v4(),
                is_owr: true,
            }
            .to_seed_data_base();
            data["seed_presentation"] = presentation(&selected, &resolved, &report).unwrap();
            race.seed.seed_data = Some(data.clone());
            race.save(&mut tx).await.unwrap();
            let reloaded = Race::from_id(&mut tx, &client, race.id).await.unwrap();
            assert_eq!(reloaded.seed.to_seed_data().unwrap(), data);
            assert_eq!(
                seed_summary(&data, true),
                seed_summary(reloaded.seed.seed_data.as_ref().unwrap(), true)
            );
        }
        if count == 2 {
            assert!(
                games[1]
                    .next_game(&mut tx, &client)
                    .await
                    .unwrap()
                    .is_none()
            );
        } else {
            let ignored = games[1].ignore_remaining_games(&mut tx).await.unwrap();
            assert_eq!(ignored, vec![games[2].id]);
            assert!(
                Race::from_id(&mut tx, &client, games[2].id)
                    .await
                    .unwrap()
                    .ignored
            );
        }
    }
    tx.rollback().await.unwrap();
}
