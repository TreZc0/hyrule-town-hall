use super::*;

fn schedule() -> RaceSchedule {
    let past = Utc::now() - TimeDelta::hours(2);
    RaceSchedule::Async {
        start1: Some(past),
        start2: Some(past),
        start3: Some(past),
        end1: None,
        end2: None,
        end3: None,
        room1: Some("https://racetime.gg/test/first".parse().unwrap()),
        room2: Some("https://racetime.gg/test/second".parse().unwrap()),
        room3: Some("https://racetime.gg/test/third".parse().unwrap()),
    }
}

#[test]
fn past_schedules_and_created_rooms_do_not_release_async_seeds() {
    let schedule = schedule();
    assert!(!schedule.async_seed_is_public(2, [false; 3]));
    assert!(!schedule.async_seed_is_public(2, [true, false, true]));
    assert!(!schedule.async_seed_is_public(2, [false, true, true]));
}

#[test]
fn every_required_entrant_must_have_received_the_seed() {
    let schedule = schedule();
    assert!(schedule.async_seed_is_public(2, [true, true, false]));
    assert!(!schedule.async_seed_is_public(3, [true, true, false]));
    assert!(!schedule.async_seed_is_public(3, [true, false, true]));
    assert!(!schedule.async_seed_is_public(3, [false, true, true]));
    assert!(schedule.async_seed_is_public(3, [true; 3]));
}

#[test]
fn finished_parts_count_but_do_not_exempt_other_entrants() {
    let mut schedule = schedule();
    if let RaceSchedule::Async { end1, .. } = &mut schedule {
        *end1 = Some(Utc::now());
    }
    assert!(!schedule.async_seed_is_public(2, [false; 3]));
    assert!(schedule.async_seed_is_public(2, [false, true, false]));
    assert!(!schedule.async_seed_is_public(3, [false, true, false]));
    assert!(schedule.async_seed_is_public(3, [false, true, true]));
    if let RaceSchedule::Async { end2, end3, .. } = &mut schedule {
        *end2 = Some(Utc::now());
        *end3 = Some(Utc::now());
    }
    assert!(schedule.async_seed_is_public(3, [false; 3]));
}

#[test]
fn unsupported_async_entrant_counts_fail_closed() {
    for count in [0, 1, 4] {
        assert!(!schedule().async_seed_is_public(count, [true; 3]));
    }
}
