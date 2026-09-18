use super::*;

#[test]
fn final_par_uses_remaining_eligible_finishers() {
    let mut config = tests::config();
    config.submissions_close_at = Some(Utc::now() + chrono::Duration::hours(1));
    let times = || [40, 50, 60, 70].map(|minutes| Duration::from_secs(minutes * 60));
    assert_eq!(seed_par(&config, times()), None);
    config.submissions_close_at = Some(Utc::now() - chrono::Duration::hours(1));
    assert_eq!(seed_par(&config, times()), Some(55.0 * 60.0));
    assert_eq!(seed_par(&config, [Duration::from_secs(3000)]), Some(3000.0));
    assert_eq!(seed_par(&config, []), None);
    assert_eq!(
        seed_par(&config, [10, 20, 30, 40, 50, 90].map(Duration::from_secs)),
        Some(30.0)
    );
    config.submissions_close_at = None;
    assert_eq!(seed_par(&config, times()), None);
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
async fn database_live_before_signup_and_cutoff_recalculation() {
    let pool = event::configuration::test_pool().await;
    let series = Series::from_str("casboots").unwrap();
    let event = "pvltest";
    let race = -9_000_000_000_001_000_i64;
    let teams = [-9_000_000_000_001_001_i64, -9_000_000_000_001_002_i64];
    let users: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id,racetime_id FROM users WHERE racetime_id IS NOT NULL ORDER BY id LIMIT 2",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(users.len(), 2);
    let result = std::panic::AssertUnwindSafe(async {
        sqlx::query("INSERT INTO events(series,event,display_name,team_config,qualifier_mode,automated_asyncs,discord_async_channel) VALUES($1,$2,'Provisional live test','solo','pooled_by_mode',TRUE,1)")
            .bind(series).bind(event).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO pooled_qualifier_configs(series,event,required_mode_count,
            requests_open_at,requests_close_at,starts_close_at,submissions_close_at,retries_close_at,results_release_at,requests_paused)
            VALUES($1,$2,1,NOW()-INTERVAL '1 hour',NOW()+INTERVAL '1 hour',NOW()+INTERVAL '2 hours',
                NOW()+INTERVAL '3 hours',NOW()+INTERVAL '1 hour',NOW()+INTERVAL '4 hours',FALSE)"#)
            .bind(series).bind(event).execute(&pool).await.unwrap();
        let mode: i64 = sqlx::query_scalar("INSERT INTO qualifier_modes(series,event,position,slug,display_name,seed_gen_type,seed_config,generator_profile,settings_fingerprint) VALUES($1,$2,1,'open','Open','owr','{}','default','test') RETURNING id")
            .bind(series).bind(event).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO races(id,series,event,is_qualifier,start) VALUES($1,$2,$3,TRUE,NOW()+INTERVAL '30 minutes')")
            .bind(race).bind(series).bind(event).execute(&pool).await.unwrap();
        let seed: i64 = sqlx::query_scalar(r#"INSERT INTO qualifier_seeds(series,event,mode_id,source,live_race_id,generation_state,seed_data,generator_profile,physical_seed_identity,settings_fingerprint)
            VALUES($1,$2,$3,'live',$4,'ready','{"type":"alttpr_owr","uuid":"00000000-0000-0000-0000-000000000117"}','default','provisional-live-test','test') RETURNING id"#)
            .bind(series).bind(event).bind(mode).bind(race).fetch_one(&pool).await.unwrap();
        let mut entrants: Vec<_> = users.iter().map(|(_, id)| LiveEntrant { racetime_id: id.clone() }).collect();
        entrants.extend((0..3).map(|i| LiveEntrant { racetime_id: format!("provisional-guest-{i}") }));
        assert_eq!(freeze_live_eligibility(&pool, race, &entrants).await.unwrap().eligible, 5);
        assert_eq!(start_live(&pool, race, &entrants.iter().map(|entrant| entrant.racetime_id.clone()).collect()).await.unwrap(), 5);
        let results: Vec<_> = entrants.iter().enumerate().map(|(i, entrant)| LiveResult {
            name: Some(format!("Runner {i}")),
            racetime_id: entrant.racetime_id.clone(), outcome: Outcome::Finished(Duration::from_secs((50 + i as u64 * 10) * 60)),
        }).collect();
        assert_eq!(finish_live(&pool, race, &results).await.unwrap(), 5);
        let mut tx = pool.begin().await.unwrap();
        let config = Config::load(&mut tx, series, event).await.unwrap().unwrap();
        assert!(!signup_closed_in(&mut tx, series, event).await.unwrap());
        let provisional = standings(&mut tx, &config).await.unwrap();
        assert_eq!(provisional.len(), 5);
        assert!(provisional.iter().all(|standing| standing.team_id.is_none()));
        let data = event::Data::new(&mut tx, series, event).await.unwrap().unwrap();
        let mut cache = event::teams::Cache::new(reqwest::Client::new());
        let public = event::teams::signups_sorted(&mut tx, &mut cache, None, &data, false,
            QualifierKind::PooledByMode { required_modes: 1 }, None, false, false).await.unwrap();
        assert_eq!(public.len(), 5);
        assert!(public.iter().all(|row| row.team.is_none()));
        assert!(public.iter().any(|row| row.members.iter().any(|member|
            matches!(&member.user, event::teams::MemberUser::RaceTime { name, .. } if name == "Runner 4"))));

        let old_points = provisional.iter().find(|standing| standing.racetime_id == users[1].1).unwrap().average.unwrap();
        tx.rollback().await.unwrap();

        // Async requests still require an active registration.
        assert!(matches!(request_async(&pool, teams[0], mode, users[0].0).await, Err(Error::NotConfigured)));
        // A live retry can be declared through authenticated racetime ownership before signup.
        let original: i64 = sqlx::query_scalar("SELECT id FROM qualifier_attempts WHERE seed_id=$1 AND racetime_id=$2")
            .bind(seed).bind(&users[0].1).fetch_one(&pool).await.unwrap();
        assert!(matches!(declare_live_retry(&pool, series, event, mode, original, users[1].0).await, Err(Error::RetryForbidden)));
        declare_live_retry(&pool, series, event, mode, original, users[0].0).await.unwrap();
        // Register after the race. No attempt is rekeyed or duplicated.
        sqlx::query("INSERT INTO teams(id,series,event) VALUES($1,$3,$4),($2,$3,$4)")
            .bind(teams[0]).bind(teams[1]).bind(series).bind(event).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO team_members(team,member,status,role) VALUES($1,$3,'created','none'),($2,$4,'created','none')")
            .bind(teams[0]).bind(teams[1]).bind(users[0].0).bind(users[1].0).execute(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(Attempt::for_team(&mut tx, teams[0]).await.unwrap()[0].id, original);
        let registered = standings(&mut tx, &config).await.unwrap();
        assert_eq!(registered.len(), 5);
        assert_eq!(registered.iter().filter(|standing| standing.team_id.is_some()).count(), 2);
        tx.rollback().await.unwrap();
        assert!(matches!(request_async(&pool, teams[1], mode, users[1].0).await, Err(Error::AlreadyAttempted)));

        // Close the qualifier period without sleeping; only registered runners remain.
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(r#"UPDATE pooled_qualifier_configs SET
            requests_open_at=requests_open_at-INTERVAL '10 hours',requests_close_at=requests_close_at-INTERVAL '10 hours',
            starts_close_at=starts_close_at-INTERVAL '10 hours',submissions_close_at=submissions_close_at-INTERVAL '10 hours'
            WHERE series=$1 AND event=$2"#).bind(series).bind(event).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let config = Config::load(&mut tx, series, event).await.unwrap().unwrap();
        assert!(config.signup_closed());
        assert!(signup_closed_in(&mut tx, series, event).await.unwrap());
        let final_scores = standings(&mut tx, &config).await.unwrap();
        assert_eq!(final_scores.len(), 2);
        let data = event::Data::new(&mut tx, series, event).await.unwrap().unwrap();
        let public = event::teams::signups_sorted(&mut tx, &mut cache, None, &data, false,
            QualifierKind::PooledByMode { required_modes: 1 }, None, true, false).await.unwrap();
        assert_eq!(public.len(), 2);
        assert!(public.iter().all(|row| row.team.is_some()));

        let new_points = final_scores.iter().find(|standing| standing.racetime_id == users[1].1).unwrap().average.unwrap();
        assert!((new_points - 100.0 * (2.0 - 60.0 / 55.0)).abs() < 0.000001);
        assert!(new_points < old_points);
        // Excluded attempts are retained, not voided or deleted.
        let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qualifier_attempts WHERE seed_id=$1 AND state='finalized'")
            .bind(seed).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(retained, 5);
        tx.rollback().await.unwrap();
    }).catch_unwind().await;
    for table in [
        "qualifier_live_entries",
        "qualifier_attempts",
        "qualifier_seeds",
        "qualifier_modes",
        "pooled_qualifier_configs",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE series=$1 AND event=$2"))
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM team_members WHERE team=ANY($1)")
        .bind(&teams[..])
        .execute(&pool)
        .await
        .unwrap();
    for table in ["teams", "races", "events"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE series=$1 AND event=$2"))
            .bind(series)
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
    }
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
