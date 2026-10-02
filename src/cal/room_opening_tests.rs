use super::*;

#[test]
fn room_opening_defaults_remain_thirty_minutes() {
    for kind in [
        EventKind::Normal,
        EventKind::Async1,
        EventKind::Async2,
        EventKind::Async3,
    ] {
        assert_eq!(
            room_open_lead_time(kind, 30, 30, None),
            TimeDelta::minutes(30)
        );
    }
}

#[test]
fn room_opening_live_changes_do_not_affect_any_async_part() {
    assert_eq!(
        room_open_lead_time(EventKind::Normal, 60, 30, None),
        TimeDelta::minutes(60)
    );
    for kind in [EventKind::Async1, EventKind::Async2, EventKind::Async3] {
        assert_eq!(
            room_open_lead_time(kind, 60, 30, None),
            TimeDelta::minutes(30)
        );
        assert_eq!(
            room_open_lead_time(kind, 30, 15, None),
            TimeDelta::minutes(15)
        );
        assert_eq!(
            room_open_lead_time(kind, 15, 60, None),
            TimeDelta::minutes(60)
        );
    }
}

#[test]
fn room_opening_weekly_override_only_affects_live_races() {
    assert_eq!(
        room_open_lead_time(EventKind::Normal, 30, 15, Some(45)),
        TimeDelta::minutes(45)
    );
    assert_eq!(
        room_open_lead_time(EventKind::Normal, 60, 15, None),
        TimeDelta::minutes(60)
    );
    for kind in [EventKind::Async1, EventKind::Async2, EventKind::Async3] {
        assert_eq!(
            room_open_lead_time(kind, 30, 15, Some(45)),
            TimeDelta::minutes(15)
        );
    }
}

#[test]
fn room_opening_uses_exact_boundaries_and_includes_the_final_minute() {
    let start = DateTime::parse_from_rfc3339("2026-10-02T18:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    for minutes in [15, 30, 60] {
        let lead_time = TimeDelta::minutes(minutes);
        let opens_at = start - lead_time;
        assert!(!room_opening_due(
            start,
            opens_at - TimeDelta::milliseconds(1),
            lead_time
        ));
        assert!(room_opening_due(start, opens_at, lead_time));
        assert!(room_opening_due(
            start,
            start - TimeDelta::seconds(30),
            lead_time
        ));
        assert!(!room_opening_due(start, start, lead_time));
        assert!(!room_opening_due(
            start,
            start + TimeDelta::seconds(1),
            lead_time
        ));
    }
}
