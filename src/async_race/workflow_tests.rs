use super::*;

#[test]
fn seedless_async_ready_skips_delivery_but_randomizers_still_require_a_seed() {
    let empty = seed::Data::default();
    assert!(!uses_seed(&empty, false));
    assert!(uses_seed(&empty, true));
    assert!(bracket_seed_message(&empty, false).unwrap().is_none());
    assert!(matches!(bracket_seed_message(&empty, true), Err(Error::NoSeedAvailable)));

    let prepared = seed::Data {
        seed_data: Some(serde_json::json!({"type":"twwr", "permalink":"test-permalink", "seed_hash":"Bow"})),
        ..seed::Data::default()
    };
    for requires_seed in [false, true] {
        assert!(uses_seed(&prepared, requires_seed));
        let message = bracket_seed_message(&prepared, requires_seed).unwrap().unwrap().build();
        assert!(message.contains("test-permalink"));
        assert!(message.contains("Bow"));
    }

    let invalid = seed::Data { seed_data: Some(serde_json::json!({})), ..seed::Data::default() };
    assert!(matches!(bracket_seed_message(&invalid, false), Err(Error::NoSeedAvailable)));
}

#[test]
fn seedless_async_messages_cover_the_workflow_without_randomizer_instructions() {
    let mut messages = vec![FORCE_START_MESSAGE.to_owned(), RUNNING_MESSAGE.to_owned(), REVERTED_MESSAGE.to_owned()];
    messages.extend(FORCE_START_WARNINGS.map(|(_, text)| text.to_owned()));
    messages.extend([CountdownStep::Announcement, CountdownStep::Number(5), CountdownStep::Go].map(|step| step.content()));
    for delay in [None, Some(0), Some(-1), Some(10)] {
        let mut content = MessageBuilder::default();
        append_async_instructions(&mut content, delay, uses_seed(&seed::Data::default(), false));
        let text = content.build();
        for step in ["**READY!**", "**START COUNTDOWN**", "**GO**", "**FINISH**", "**FORFEIT**"] {
            assert!(text.contains(step));
        }
        assert_eq!(text.contains("10 minutes"), delay == Some(10));
        messages.push(text);

        let mut ready = MessageBuilder::default();
        append_start_countdown_instructions(&mut ready, delay);
        let text = ready.build();
        assert!(text.contains("Start playing at **GO**"));
        assert_eq!(text.contains("starts automatically"), delay == Some(10));
        messages.push(text);
    }
    for first in [true, false] {
        let mut content = MessageBuilder::default();
        append_bracket_recording_instructions(&mut content, first, false, false);
        messages.push(content.build());
    }
    let mut qualifier = MessageBuilder::default();
    append_qualifier_recording_instructions(&mut qualifier, false, false);
    messages.push(qualifier.build());
    let mut organizer = MessageBuilder::default();
    append_organizer_start_instructions(&mut organizer, false);
    messages.push(organizer.build());
    for run in runs() {
        messages.push(completion_message_content(&run, "01:50:20", false, false).0);
        messages.push(forfeit_message(&run, "<@123>").0);
    }
    for text in messages {
        for forbidden in ["seed", "randomizer", "download", "patch", "permalink"] {
            assert!(!text.to_lowercase().contains(forbidden), "unexpected {forbidden}: {text}");
        }
    }
}

#[test]
fn async_seed_instructions_include_required_seeds_before_generation() {
    let empty = seed::Data::default();
    let mut welcome = MessageBuilder::default();
    append_async_instructions(&mut welcome, Some(10), uses_seed(&empty, true));
    let text = welcome.build();
    assert!(text.contains("receive your seed"));
    assert!(text.contains("Download and prepare the seed"));
    assert!(matches!(bracket_seed_message(&empty, true), Err(Error::NoSeedAvailable)));

    let mut organizer = MessageBuilder::default();
    append_organizer_start_instructions(&mut organizer, true);
    assert!(organizer.build().contains("delivered when the runner clicks **READY!**"));
}

#[test]
fn async_completion_evidence_is_consistent_in_welcome_and_finish_messages() {
    for (alttpr, twwr, expected) in [(true, false, "collection rate"), (false, true, "Ganondorf"), (false, false, "proof of run completion")] {
        let mut evidence = MessageBuilder::default();
        append_completion_evidence(&mut evidence, alttpr, twwr);
        let evidence = evidence.build();
        assert!(evidence.contains(expected));
        let mut qualifier = MessageBuilder::default();
        append_qualifier_recording_instructions(&mut qualifier, alttpr, twwr);
        assert!(qualifier.build().contains(&evidence));
        for first in [true, false] {
            let mut welcome = MessageBuilder::default();
            append_bracket_recording_instructions(&mut welcome, first, alttpr, twwr);
            let text = welcome.build();
            assert!(text.contains(&evidence));
            assert_eq!(text.contains("You can stream"), !first);
        }
        for run in runs() {
            assert!(completion_message_content(&run, "01:50:20", alttpr, twwr).0.contains(&evidence));
        }
    }
}

#[test]
fn mmr_async_delivery_uses_remote_seed_and_complete_hash() {
    let data = serde_json::json!({"type":"mmr", "id":"123", "hash":["ITEM_BOW", "0x61", "future-icon"], "encrypted":true, "locked":true});
    let text = seed_message(&data).unwrap().build();
    assert!(text.contains("https://mmrandomizer.com/seed/get?id=123"));
    assert!(text.contains("ITEM_BOW, 0x61, future-icon"));
    assert!(!text.contains("patcher"));
    assert!(seed_message(&serde_json::json!({"type":"mmr", "id":"123", "hash":[]})).is_err());
}

fn runs() -> [AsyncRun; 3] {
    [
        AsyncRun::BracketRace { race_id: 42, async_part: 1 },
        AsyncRun::Qualifier { team_id: 42, async_id: 1 },
        AsyncRun::PooledQualifier { attempt_id: 42, control_version: 3 },
    ]
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a *_test database"]
async fn countdown_claim_excludes_duplicate_and_force_starts_until_released() {
    let pool = event::configuration::test_pool().await;
    let [bracket, qualifier, _] = runs();
    for run in [&bracket, &qualifier] {
        // Manual clicks and automatic starts share this claim. Even while
        // start_time is unset, exactly one concurrent caller can send messages.
        let (first, second) = tokio::join!(claim_countdown(&pool, run), claim_countdown(&pool, run));
        let (first, second) = (first.unwrap(), second.unwrap());
        assert_ne!(first.is_some(), second.is_some());
        let claim = first.or(second).unwrap();
        assert!(claim_countdown(&pool, run).await.unwrap().is_none());

        // Another run with the same numeric ID must not be blocked.
        let other_run = if matches!(run, AsyncRun::BracketRace { .. }) { &qualifier } else { &bracket };
        claim_countdown(&pool, other_run).await.unwrap().unwrap().commit().await.unwrap();

        // A failed countdown rolls back its claim so it can be retried.
        claim.rollback().await.unwrap();
        let retry = claim_countdown(&pool, run).await.unwrap().unwrap();
        retry.commit().await.unwrap();
        claim_countdown(&pool, run).await.unwrap().unwrap().rollback().await.unwrap();
    }
}

#[test]
fn async_workflow_messages_and_existing_buttons_agree() {
    let [bracket, qualifier, pooled] = runs();
    for (alttpr, twwr, screenshot) in [
        (true, false, "collection rate"),
        (false, true, "Ganondorf"),
        (false, false, "proof of run completion"),
    ] {
        let (normal_text, _) = completion_message_content(&qualifier, "01:50:20", alttpr, twwr);
        let (pool_text, controls) = completion_message_content(&pooled, "01:50:20", alttpr, twwr);
        assert_eq!(pool_text, normal_text);
        assert!(pool_text.contains("01:50:20"));
        assert!(pool_text.contains(screenshot));
        assert!(serde_json::to_string(&controls).unwrap().contains(&pooled.button_id("org_result")));
        let (bracket_text, _) = completion_message_content(&bracket, "01:50:20", alttpr, twwr);
        assert!(bracket_text.contains(screenshot));
        assert!(!bracket_text.contains("Qualifier run"));
    }
    assert_eq!(forfeit_message(&qualifier, "<@123>").0, forfeit_message(&pooled, "<@123>").0);
    for run in [&bracket, &qualifier, &pooled] {
        for action in ["ready", "start_countdown", "finish", "forfeit", "revert", "org_result"] {
            let id = run.button_id(action);
            let (parsed_action, parsed_run, _) = AsyncRun::parse_button(&id).unwrap();
            assert_eq!(parsed_action, action);
            assert_eq!(parsed_run.button_id(action), id);
        }
        assert!(serde_json::to_string(&start_button(run)).unwrap().contains(&run.button_id("start_countdown")));
    }
}

#[test]
fn async_force_start_schedule_and_finish_time_use_original_timestamps() {
    let prepared = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
    for (minutes, expected) in [(0, vec![]), (1, vec![30]), (2, vec![90]), (10, vec![480, 570])] {
        let due = prepared + chrono::Duration::minutes(minutes);
        let offsets = force_start_warnings(prepared, due).map(|(at, _)| (at - prepared).num_seconds()).collect::<Vec<_>>();
        assert_eq!(offsets, expected);
    }
    for delay in [None, Some(0), Some(-1), Some(10)] {
        let mut content = MessageBuilder::default();
        append_async_instructions(&mut content, delay, true);
        let text = content.build();
        assert_eq!(text.contains("No automatic force-start"), delay != Some(10));
        assert!(text.contains("30 seconds"));
    }
    let click = prepared + chrono::Duration::seconds(6_620);
    assert_eq!(format_finish_time(prepared, click), "01:50:20");
    assert_ne!(format_finish_time(prepared, click + chrono::Duration::seconds(30)), "01:50:20");
}

#[test]
fn async_seed_delivery_includes_the_hash_for_supported_pool_generators() {
    for data in [
        serde_json::json!({"type":"alttpr_owr", "uuid":"00000000-0000-0000-0000-000000000001", "hash1":"Bow", "hash2":"Hookshot", "hash3":"Boots", "hash4":"Hammer", "hash5":"Mirror"}),
        serde_json::json!({"type":"alttpr_avianart", "hash":"test-seed", "seed_hash":["Bow","Hookshot","Boots","Hammer","Mirror"]}),
        serde_json::json!({"type":"twwr", "permalink":"test-permalink", "seed_hash":"Bow, Hookshot, Boots, Hammer, Mirror"}),
    ] {
        let text = seed_message(&data).unwrap().build();
        assert!(text.contains("Your seed is ready"));
        assert!(text.contains("Bow, Hookshot, Boots, Hammer, Mirror"));
        assert!(!text.contains("/status"));
    }
}

#[tokio::test]
async fn async_thread_membership_requires_entrant_access_and_deduplicates_users() {
    let attempted = std::sync::Mutex::new(Vec::new());
    let entrants = [UserId::new(1), UserId::new(1), UserId::new(2), UserId::new(3)];
    let result = add_thread_members(entrants, |user| {
        attempted.lock().unwrap().push(user.get());
        async move { if user.get() == 2 { Err(Error::NoTeamMembers) } else { Ok(()) } }
    }).await;
    assert!(result.is_err());
    assert_eq!(*attempted.lock().unwrap(), [1, 2]);
    add_thread_members(entrants, |_| async { Ok(()) }).await.unwrap();
}
