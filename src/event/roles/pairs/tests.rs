use super::*;

fn form(action: &str) -> ActionForm {
    ActionForm {
        csrf: String::new(),
        action: action.into(),
        notes: String::new(),
        replace_solo: false,
    }
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
async fn paired_signup_consent_settings_capacity_and_withdrawal() {
    let pool = event::configuration::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    lock(&mut tx).await.unwrap();
    let key = rng().random_range(1_000_000_i32..10_000_000);
    let event = format!("pair{key}");
    let users = [
        i64::from(key) * 100,
        i64::from(key) * 100 + 1,
        i64::from(key) * 100 + 2,
    ];
    for (index, id) in users.iter().enumerate() {
        sqlx::query("INSERT INTO users(id,discord_id,discord_display_name,discord_username,display_source) VALUES($1,$1,$2,$2,'discord')")
            .bind(id).bind(format!("pairuser{key}_{index}")).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("INSERT INTO events(series,event,display_name,team_config,force_custom_role_binding) VALUES('xkeys',$1,'Pair test','solo',true)")
        .bind(&event).execute(&mut *tx).await.unwrap();
    let role: i32 = sqlx::query_scalar("INSERT INTO role_types(name) VALUES($1) RETURNING id")
        .bind(format!("Pair test {key}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let binding:i32=sqlx::query_scalar("INSERT INTO role_bindings(series,event,role_type_id,language,max_count,auto_approve,allow_paired_signups) VALUES('xkeys',$1,$2,'en',2,true,true) RETURNING id")
        .bind(&event).bind(role).fetch_one(&mut *tx).await.unwrap();
    let race = Id::from(users[0] + 10);
    sqlx::query(
        "INSERT INTO races(id,series,event,start) VALUES($1,'xkeys',$2,NOW()+INTERVAL '1 day')",
    )
    .bind(race)
    .bind(&event)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO organizers(series,event,organizer) VALUES('xkeys',$1,$2)")
        .bind(&event)
        .bind(users[2])
        .execute(&mut *tx)
        .await
        .unwrap();
    let data = Data::new(&mut tx, Series::Crosskeys, &event)
        .await
        .unwrap()
        .unwrap();
    let binding = Id::from(i64::from(binding));
    let alice = User::from_id(&mut *tx, Id::from(users[0]))
        .await
        .unwrap()
        .unwrap();
    let bob = User::from_id(&mut *tx, Id::from(users[1]))
        .await
        .unwrap()
        .unwrap();
    let organizer = User::from_id(&mut *tx, Id::from(users[2]))
        .await
        .unwrap()
        .unwrap();

    // Existing solo signup remains available before a race is scheduled.
    sqlx::query("UPDATE races SET start=NULL WHERE id=$1")
        .bind(race)
        .execute(&mut *tx)
        .await
        .unwrap();
    check_race(&mut tx, &data, race, false).await.unwrap();
    assert!(check_race(&mut tx, &data, race, true).await.is_err());
    sqlx::query("UPDATE races SET start=NOW()+INTERVAL '1 day' WHERE id=$1")
        .bind(race)
        .execute(&mut *tx)
        .await
        .unwrap();

    let (pair, created) = create(
        &mut tx,
        &data,
        race,
        binding,
        alice.id,
        bob.id,
        "Alice's note",
    )
    .await
    .unwrap();
    assert!(created);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM signups WHERE race_id=$1")
            .bind(race)
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        0
    );
    assert!(
        !RoleRequest::approved_for_user(&mut tx, binding, bob.id)
            .await
            .unwrap(),
        "inviting another user must not enroll them"
    );
    assert!(
        !create(&mut tx, &data, race, binding, alice.id, bob.id, "")
            .await
            .unwrap()
            .1,
        "double submit returns same invitation"
    );
    assert!(
        act(&mut tx, &data, &pair, &alice, &form("accept"))
            .await
            .is_err(),
        "inviter cannot accept for partner"
    );

    // Explicit false overrides must survive, and disabling future pairs does
    // not revoke an invitation the partner already received.
    EventRoleBindingOverride::upsert(
        &mut tx,
        Series::Crosskeys,
        &event,
        binding,
        None,
        None,
        None,
        Some(false),
        Some(false),
    )
    .await
    .unwrap();
    let effective = super::binding(&mut tx, &data, binding).await.unwrap();
    assert!(!effective.allow_signup_notes);
    assert!(!effective.allow_paired_signups);
    let mut consent = form("accept");
    consent.notes = "New note while disabled".into();
    assert!(act(&mut tx, &data, &pair, &bob, &consent).await.is_err());
    consent.notes.clear();

    // Bob's own solo signup remains untouched until he explicitly replaces it.
    Signup::create(&mut tx, race, binding, bob.id, Some("Bob's note".into()))
        .await
        .unwrap();
    assert!(act(&mut tx, &data, &pair, &bob, &consent).await.is_err());
    consent.replace_solo = true;
    act(&mut tx, &data, &pair, &bob, &consent).await.unwrap();
    let pair = load(&mut tx, pair.id).await.unwrap().unwrap();
    assert_eq!(pair.state, "pending");
    assert!(
        RoleRequest::approved_for_user(&mut tx, binding, bob.id)
            .await
            .unwrap()
    );
    let statuses: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT status::text,notes FROM signups WHERE pair_id=$1 ORDER BY user_id")
            .bind(pair.id)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        statuses,
        vec![
            ("pending".into(), Some("Alice's note".into())),
            ("pending".into(), Some("Bob's note".into()))
        ]
    );
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET CONSTRAINTS ALL DEFERRED")
        .execute(&mut *tx)
        .await
        .unwrap();

    // Capacity is checked again at selection time, even after consent.
    sqlx::query("UPDATE role_bindings SET max_count=1 WHERE id=$1")
        .bind(i64::from(binding))
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        act(&mut tx, &data, &pair, &organizer, &form("confirm"))
            .await
            .is_err()
    );
    sqlx::query("UPDATE role_bindings SET max_count=2 WHERE id=$1")
        .bind(i64::from(binding))
        .execute(&mut *tx)
        .await
        .unwrap();
    act(&mut tx, &data, &pair, &organizer, &form("confirm"))
        .await
        .unwrap();
    let pair = load(&mut tx, pair.id).await.unwrap().unwrap();
    assert_eq!(pair.state, "confirmed");
    let member: i32 =
        sqlx::query_scalar("SELECT id FROM signups WHERE pair_id=$1 ORDER BY id LIMIT 1")
            .bind(pair.id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(
        !Signup::update_status(
            &mut tx,
            Id::from(i64::from(member)),
            VolunteerSignupStatus::Aborted
        )
        .await
        .unwrap(),
        "legacy actions cannot split the pair"
    );
    sqlx::query("INSERT INTO event_disabled_role_bindings(series,event,role_binding_id) VALUES('xkeys',$1,$2)")
        .bind(&event).bind(i64::from(binding)).execute(&mut *tx).await.unwrap();
    // Disabling a role must not trap either volunteer in an accepted pair.
    act(&mut tx, &data, &pair, &alice, &form("withdraw"))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM signups WHERE pair_id=$1 AND status='aborted'"
        )
        .bind(pair.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        2
    );
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
async fn database_rejects_partial_pair_and_preserves_solo_rows() {
    let pool = event::configuration::test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let key = rng().random_range(1_000_000_i32..10_000_000);
    let event = format!("pairdb{key}");
    let user = i64::from(key) * 100;
    for id in [user, user + 1] {
        sqlx::query("INSERT INTO users(id,racetime_id,racetime_display_name) VALUES($1,$2,'Pair database test')").bind(id).bind(format!("p{id}")).execute(&mut *tx).await.unwrap();
    }
    sqlx::query("INSERT INTO events(series,event,display_name,team_config) VALUES('xkeys',$1,'Pair DB test','solo')").bind(&event).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO races(id,series,event,start) VALUES($1,'xkeys',$2,NOW()+INTERVAL '1 day')",
    )
    .bind(user)
    .bind(&event)
    .execute(&mut *tx)
    .await
    .unwrap();
    let role: i32 = sqlx::query_scalar("INSERT INTO role_types(name) VALUES($1) RETURNING id")
        .bind(format!("Pair DB {key}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let binding:i32=sqlx::query_scalar("INSERT INTO role_bindings(series,event,role_type_id,max_count) VALUES('xkeys',$1,$2,2) RETURNING id").bind(&event).bind(role).fetch_one(&mut *tx).await.unwrap();
    let pair:i64=sqlx::query_scalar("INSERT INTO volunteer_signup_pairs(race_id,role_binding_id,inviter_user_id,invitee_user_id,state,accepted_at) VALUES($1,$2,$1,$3,'confirmed',NOW()) RETURNING id").bind(user).bind(binding).bind(user+1).fetch_one(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO signups(race_id,role_binding_id,user_id,pair_id,status) VALUES($1,$2,$1,$3,'confirmed'),($1,$2,$4,$3,'pending')").bind(user).bind(binding).bind(pair).bind(user+1).execute(&mut *tx).await.unwrap();
    assert!(
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *tx)
            .await
            .is_err(),
        "partial confirmation must fail at commit"
    );
    tx.rollback().await.unwrap();
}
