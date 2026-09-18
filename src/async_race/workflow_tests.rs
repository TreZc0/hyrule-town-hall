use super::*;

fn runs() -> [AsyncRun; 3] {
    [
        AsyncRun::BracketRace { race_id: 42, async_part: 1 },
        AsyncRun::Qualifier { team_id: 42, async_kind: AsyncKind::Qualifier1 },
        AsyncRun::PooledQualifier { attempt_id: 42, control_version: 3 },
    ]
}

#[test]
fn async_workflow_messages_and_existing_buttons_agree() {
    let [bracket, qualifier, pooled] = runs();
    for (alttpr, twwr, screenshot) in [
        (true, false, "collection rate"),
        (false, true, "Ganondorf"),
        (false, false, "indication of seed completion"),
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
        append_async_instructions(&mut content, delay);
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
