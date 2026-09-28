use super::*;
use crate::event::roles::{Signup, VolunteerSignupStatus};
use rocket::{
    fairing::AdHoc,
    http::{ContentType, Cookie, Header},
    local::asynchronous::Client,
};

#[test]
fn manager_permissions_are_narrow_and_signup_decisions_are_optional() {
    for enabled in [false, true] {
        let permissions = Permissions::from_roles(false, false, true, enabled);
        assert!(permissions.review_roles);
        assert!(permissions.view_signups);
        assert_eq!(permissions.decide_signups, enabled);
        assert!(!permissions.manage_managers);
        assert!(!permissions.revert_signups);
    }
}

#[test]
fn enabling_signup_management_does_not_grant_nonmembers_access() {
    let permissions = Permissions::from_roles(false, false, false, true);
    assert!(!permissions.review_roles);
    assert!(!permissions.view_signups);
    assert!(!permissions.decide_signups);
    assert!(!permissions.manage_managers);
    assert!(!permissions.revert_signups);
}

#[test]
fn existing_staff_permissions_are_additive() {
    for manager in [false, true] {
        let coordinator = Permissions::from_roles(false, true, manager, false);
        assert!(coordinator.decide_signups);
        assert!(coordinator.revert_signups);
        assert_eq!(coordinator.review_roles, manager);
        assert!(!coordinator.manage_managers);
        let organizer = Permissions::from_roles(true, false, manager, false);
        assert!(organizer.review_roles);
        assert!(organizer.view_signups);
        assert!(organizer.decide_signups);
        assert!(organizer.manage_managers);
        assert!(organizer.revert_signups);
    }
}

#[rocket::get("/test-token")]
fn token(csrf: CsrfToken) -> String {
    csrf.authenticity_token()
}

async fn post(
    client: &Client,
    user: i64,
    url: &str,
    csrf: &str,
    fields: &[(&str, String)],
) -> (Status, String) {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    body.append_pair("csrf", csrf);
    for (name, value) in fields {
        body.append_pair(name, value);
    }
    let response = tokio::time::timeout(
        Duration::from_secs(15),
        client
            .post(url)
            .header(Header::new("x-test-user", user.to_string()))
            .header(ContentType::Form)
            .private_cookie(Cookie::new(
                "csrf_token",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            ))
            .body(body.finish())
            .dispatch(),
    )
    .await
    .expect("route must not wait for Discord when denying a request");
    let status = response.status();
    (status, response.into_string().await.unwrap_or_default())
}

async fn get(client: &Client, user: i64, url: &str) -> (Status, String) {
    let response = client
        .get(url)
        .header(Header::new("x-test-user", user.to_string()))
        .dispatch()
        .await;
    (
        response.status(),
        response.into_string().await.unwrap_or_default(),
    )
}

async fn assert_race_links(pool: &PgPool, slug: &str, user_id: i64, race_id: i64, enabled: bool) {
    use kuchiki::traits::TendrilSink as _;
    let mut tx = pool.begin().await.unwrap();
    let data = Data::new(&mut tx, Series::Crosskeys, slug)
        .await
        .unwrap()
        .unwrap();
    let user = User::from_id(&mut *tx, Id::from(user_id))
        .await
        .unwrap()
        .unwrap();
    let http_client = reqwest::Client::new();
    let race = Race::from_id(&mut tx, &http_client, race_id.into())
        .await
        .unwrap();
    let uri = Origin(
        rocket::http::uri::Origin::parse_owned(format!("/event/xkeys/{slug}/races")).unwrap(),
    );
    let table = cal::race_table(
        &mut tx,
        None,
        &http_client,
        &uri,
        Some(&data),
        cal::RaceTableOptions {
            game_count: false,
            show_multistreams: false,
            can_edit: false,
            show_restream_consent: false,
            challonge_import_ctx: None,
        },
        &[race],
        Some(&user),
        None,
    )
    .await
    .unwrap()
    .0;
    let document = kuchiki::parse_html().one(table.clone());
    let labels = document
        .select("a")
        .unwrap()
        .map(|anchor| anchor.text_contents())
        .collect::<Vec<_>>();
    assert!(
        labels.iter().any(|label| label
            == if enabled {
                "Manage Volunteers"
            } else {
                "View Volunteer Signups"
            }),
        "{table}"
    );
    assert!(!labels.iter().any(|label| label == "Edit"), "{table}");
    tx.rollback().await.unwrap();
}

async fn request_status(pool: &PgPool, id: i32) -> String {
    sqlx::query_scalar("SELECT status::text FROM role_requests WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
async fn volunteer_manager_routes_enforce_scope_copy_permissions_and_pending_decisions() {
    let pool = event::configuration::test_pool().await;
    let key = rng().random_range(1_000_000_i32..10_000_000);
    let slugs = [format!("v{key}a"), format!("v{key}b"), format!("v{key}c")];
    let [target, source, other] = slugs.each_ref();
    let ids = [
        i64::from(key) * 100_000,
        i64::from(key) * 100_000 + 1,
        i64::from(key) * 100_000 + 2,
        i64::from(key) * 100_000 + 3,
        i64::from(key) * 100_000 + 4,
    ];
    let [organizer, manager, copied_manager, outsider, volunteer] = ids;
    let race_ids = [ids[0] + 10, ids[0] + 11, ids[0] + 12];
    let binding_ids = [key * 10, key * 10 + 1, key * 10 + 2];
    let request_ids = [key * 10 + 10, key * 10 + 11, key * 10 + 12, key * 10 + 13];
    let signup_ids = [key * 10 + 20, key * 10 + 21, key * 10 + 22];
    let result = std::panic::AssertUnwindSafe(async {
        for id in ids {
            sqlx::query("INSERT INTO users (id, racetime_id, racetime_display_name) VALUES ($1, $2, 'Volunteer manager fixture')")
                .bind(id).bind(format!("volunteer-manager-{id}")).execute(&pool).await.unwrap();
        }
        for slug in &slugs {
            sqlx::query("INSERT INTO events (series, event, display_name, team_config) VALUES ('xkeys', $1, 'Volunteer manager fixture', 'solo')")
                .bind(slug).execute(&pool).await.unwrap();
        }
        for slug in [target, source] {
            sqlx::query("INSERT INTO organizers (series, event, organizer) VALUES ('xkeys', $1, $2)")
                .bind(slug).bind(organizer).execute(&pool).await.unwrap();
        }
        for (slug, id) in [(source, copied_manager), (other, outsider)] {
            sqlx::query("INSERT INTO event_volunteer_managers VALUES ('xkeys', $1, $2)").bind(slug).bind(id).execute(&pool).await.unwrap();
        }
        let role_type = key * 10 + 30;
        sqlx::query("INSERT INTO role_types (id, name) VALUES ($1, $2)").bind(role_type).bind(format!("Volunteer manager test {key}")).execute(&pool).await.unwrap();
        for (id, slug) in [(binding_ids[0], Some(target)), (binding_ids[1], Some(other)), (binding_ids[2], None)] {
            sqlx::query("INSERT INTO role_bindings (id, series, event, role_type_id, language, game_id) VALUES ($1, $2, $3, $4, 'en', $5)")
                .bind(id).bind(slug.map(|_| "xkeys")).bind(slug).bind(role_type)
                .bind(if slug.is_none() { Some(sqlx::query_scalar::<_, i32>("SELECT game_id FROM game_series WHERE series = 'xkeys'").fetch_one(&pool).await.unwrap()) } else { None })
                .execute(&pool).await.unwrap();
        }
        for (id, binding) in [(request_ids[0], binding_ids[0]), (request_ids[1], binding_ids[1]), (request_ids[2], binding_ids[2]), (request_ids[3], binding_ids[0])] {
            sqlx::query("INSERT INTO role_requests (id, role_binding_id, user_id, notes) VALUES ($1, $2, $3, 'Private application note')")
                .bind(id).bind(binding).bind(volunteer).execute(&pool).await.unwrap();
        }
        for (id, slug) in [(race_ids[0], target), (race_ids[1], target), (race_ids[2], other)] {
            sqlx::query("INSERT INTO races (id, series, event, start) VALUES ($1, 'xkeys', $2, NOW() + INTERVAL '1 day')")
                .bind(id).bind(slug).execute(&pool).await.unwrap();
        }
        for (id, race, binding) in [(signup_ids[0], race_ids[0], binding_ids[0]), (signup_ids[1], race_ids[0], binding_ids[2]), (signup_ids[2], race_ids[2], binding_ids[1])] {
            sqlx::query("INSERT INTO signups (id, race_id, role_binding_id, user_id, notes) VALUES ($1, $2, $3, $4, 'Private signup note')")
                .bind(id).bind(race).bind(binding).bind(volunteer).execute(&pool).await.unwrap();
        }
        let auth_pool = pool.clone();
        let client = Client::tracked(rocket::build().manage(pool.clone()).manage(reqwest::Client::new())
            .manage(RwFuture::new(std::future::pending::<DiscordCtx>()))
            .attach(rocket_csrf::Fairing::default())
            .attach(AdHoc::on_request("volunteer manager fixture identity", move |request, _| {
                let pool = auth_pool.clone();
                Box::pin(async move {
                    if let Some(id) = request.headers().get_one("x-test-user").and_then(|id| id.parse::<i64>().ok()) {
                        let user = User::from_id(&pool, Id::from(id)).await.unwrap();
                        request.local_cache(|| user);
                    }
                })
            }))
            .mount("/", rocket::routes![token, settings, add_manager, remove_manager, save_settings, copy_managers, review,
                roles::get, roles::approve_role_request, roles::reject_role_request, roles::manage_roster, roles::revoke_signup,
                roles::revoke_role_request, roles::edit_role_binding, roles::match_signup_page_get,
                crate::games::approve_game_role_request])).await.unwrap();
        let csrf = client.get("/test-token").private_cookie(Cookie::new("csrf_token", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="))
            .dispatch().await.into_string().await.unwrap();
        let base = format!("/event/xkeys/{target}");
        let config = format!("{base}/configure/volunteer-managers");

        // Only organizers can appoint managers; invalid CSRF performs no mutation.
        assert_eq!(post(&client, outsider, &format!("{config}/add"), &csrf, &[("user_id", manager.to_string())]).await.0, Status::Forbidden);
        let invalid = post(&client, organizer, &format!("{config}/add"), "invalid", &[("user_id", manager.to_string())]).await;
        assert!(invalid.1.contains("confirm your identity"), "{}", invalid.1);
        assert_eq!(get(&client, manager, &format!("{base}/volunteer-management")).await.0, Status::Forbidden);
        assert_eq!(post(&client, organizer, &format!("{config}/add"), &csrf, &[("user_id", manager.to_string())]).await.0, Status::SeeOther);
        assert_eq!(get(&client, manager, &config).await.0, Status::Forbidden);
        assert_eq!(post(&client, manager, &format!("{config}/settings"), &csrf, &[("allow_signups", "on".into())]).await.0, Status::Forbidden);

        // Copy adds members, deduplicates, checks the source, and preserves the target toggle.
        sqlx::query("UPDATE events SET volunteer_managers_can_manage_signups = true WHERE series = 'xkeys' AND event = $1").bind(source).execute(&pool).await.unwrap();
        for _ in 0..2 {
            assert_eq!(post(&client, organizer, &format!("{config}/copy-from"), &csrf, &[("source_event", format!("xkeys/{source}"))]).await.0, Status::SeeOther);
        }
        assert_eq!(post(&client, organizer, &format!("{config}/copy-from"), &csrf, &[("source_event", format!("xkeys/{other}"))]).await.0, Status::Forbidden);
        let members: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM event_volunteer_managers WHERE series = 'xkeys' AND event = $1 ORDER BY user_id").bind(target).fetch_all(&pool).await.unwrap();
        assert_eq!(members, vec![manager, copied_manager]);
        assert!(!sqlx::query_scalar::<_, bool>("SELECT volunteer_managers_can_manage_signups FROM events WHERE series = 'xkeys' AND event = $1").bind(target).fetch_one(&pool).await.unwrap());

        let (status, page) = get(&client, manager, &format!("{base}/roles")).await;
        assert_eq!(status, Status::Ok, "{page}");
        assert!(page.contains("Private application note"));
        assert!(!page.contains("add-binding"));
        assert!(!page.contains("revoke-role-request"));
        assert_eq!(post(&client, manager, &format!("{base}/roles/binding/{}/edit", binding_ids[0]), &csrf,
            &[("min_count", "1".into()), ("max_count", "2".into()), ("discord_role_id", String::new())]).await.0, Status::Forbidden);
        let signup_url = format!("{base}/races/{}/signups", race_ids[0]);
        let (status, page) = get(&client, manager, &signup_url).await;
        assert_eq!(status, Status::Ok, "{page}");
        assert!(page.contains("Private signup note"));
        assert!(!page.contains("manage-roster"));
        assert!(!page.contains("revoke-signup"));

        assert_race_links(&pool, target, manager, race_ids[0], false).await;

        // Event-only applications and pending-only decisions, including direct endpoint calls.
        for id in [request_ids[1], request_ids[2]] {
            assert_eq!(post(&client, manager, &format!("{base}/roles/{id}/approve"), &csrf, &[]).await.0, Status::Conflict);
            assert_eq!(post(&client, manager, &format!("{base}/roles/{id}/reject"), &csrf, &[]).await.0, Status::Conflict);
            assert_eq!(request_status(&pool, id).await, "pending");
        }
        // Preserve organizer approvals of shared bindings used by this event, while
        // the new manager role remains event-specific even on a shared-binding event.
        sqlx::query("UPDATE events SET force_custom_role_binding = false WHERE series = 'xkeys' AND event = $1").bind(target).execute(&pool).await.unwrap();
        assert_eq!(post(&client, manager, &format!("{base}/roles/{}/approve", request_ids[2]), &csrf, &[]).await.0, Status::Conflict);
        assert_eq!(post(&client, organizer, &format!("{base}/roles/{}/approve", request_ids[2]), &csrf, &[]).await.0, Status::SeeOther);
        assert_eq!(request_status(&pool, request_ids[2]).await, "approved");
        let game_name: String = sqlx::query_scalar("SELECT g.name FROM games g JOIN game_series gs ON gs.game_id = g.id WHERE gs.series = 'xkeys'").fetch_one(&pool).await.unwrap();
        assert_eq!(post(&client, manager, &format!("/games/{game_name}/roles/{}/approve", request_ids[2]), &csrf, &[]).await.0, Status::Forbidden);
        assert_eq!(post(&client, manager, &format!("{base}/roles/{}/approve", request_ids[0]), &csrf, &[]).await.0, Status::SeeOther);
        assert_eq!(request_status(&pool, request_ids[0]).await, "approved");
        assert_eq!(post(&client, manager, &format!("{base}/roles/{}/reject", request_ids[0]), &csrf, &[]).await.0, Status::Conflict);
        assert_eq!(post(&client, copied_manager, &format!("{base}/roles/{}/reject", request_ids[3]), &csrf, &[]).await.0, Status::SeeOther);
        assert_eq!(request_status(&pool, request_ids[3]).await, "rejected");
        post(&client, manager, &format!("{base}/revoke-role-request/{}", request_ids[0]), &csrf, &[]).await;
        assert_eq!(request_status(&pool, request_ids[0]).await, "approved");

        let roster_url = format!("{base}/races/{}/manage-roster", race_ids[0]);
        let fields = [("signup_id", signup_ids[0].to_string()), ("action", "confirm".into())];
        let denied = post(&client, manager, &roster_url, &csrf, &fields).await;
        assert!(denied.1.contains("do not have permission"));
        assert_eq!(post(&client, organizer, &format!("{config}/settings"), &csrf, &[("allow_signups", "on".into())]).await.0, Status::SeeOther);
        assert_race_links(&pool, target, manager, race_ids[0], true).await;
        let page = get(&client, manager, &signup_url).await.1;
        assert!(page.contains("manage-roster"));
        assert!(!page.contains("revoke-signup"));
        assert_eq!(post(&client, manager, &roster_url, &csrf, &[("signup_id", signup_ids[2].to_string()), ("action", "confirm".into())]).await.0, Status::Conflict);
        assert_eq!(post(&client, manager, &format!("{base}/races/{}/manage-roster", race_ids[2]), &csrf, &fields).await.0, Status::NotFound);
        assert_eq!(get(&client, manager, &format!("{base}/races/{}/signups", race_ids[2])).await.0, Status::NotFound);

        assert_eq!(post(&client, manager, &roster_url, &csrf, &[("signup_id", (i64::from(signup_ids[0]) + (1_i64 << 32)).to_string()), ("action", "confirm".into())]).await.0, Status::Conflict);
        // Exercise the same atomic transitions used by the route without Discord side effects.
        let mut tx = pool.begin().await.unwrap();
        let data = Data::new(&mut tx, Series::Crosskeys, target.as_str()).await.unwrap().unwrap();
        let user = User::from_id(&mut *tx, Id::from(manager)).await.unwrap().unwrap();
        let permissions = Permissions::load(&mut tx, &data, &user).await.unwrap();
        assert!(permissions.decide_signups);
        assert!(!permissions.revert_signups);
        assert!(!Signup::decide_pending(&mut tx, Id::from(i64::from(signup_ids[0])), race_ids[1].into(), data.series, target, VolunteerSignupStatus::Confirmed).await.unwrap());
        for (id, status) in [(signup_ids[0], VolunteerSignupStatus::Confirmed), (signup_ids[1], VolunteerSignupStatus::Declined)] {
            assert!(Signup::decide_pending(&mut tx, Id::from(i64::from(id)), race_ids[0].into(), data.series, target, status).await.unwrap());
            assert!(!Signup::decide_pending(&mut tx, Id::from(i64::from(id)), race_ids[0].into(), data.series, target, VolunteerSignupStatus::Confirmed).await.unwrap());
        }
        tx.commit().await.unwrap();
        let page = get(&client, manager, &signup_url).await.1;
        assert!(!page.contains("revoke-signup"));
        post(&client, manager, &format!("{base}/races/{}/revoke-signup", race_ids[0]), &csrf, &[("signup_id", signup_ids[0].to_string())]).await;
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT status::text FROM signups WHERE id = $1").bind(signup_ids[0]).fetch_one(&pool).await.unwrap(), "confirmed");
        // Changes apply to stale browser forms on their next submission.
        post(&client, organizer, &format!("{config}/settings"), &csrf, &[]).await;
        assert!(post(&client, manager, &roster_url, &csrf, &fields).await.1.contains("do not have permission"));
        post(&client, organizer, &format!("{config}/remove"), &csrf, &[("user_id", manager.to_string())]).await;
        assert_eq!(get(&client, manager, &format!("{base}/volunteer-management")).await.0, Status::Forbidden);
        sqlx::query("UPDATE events SET end_time = NOW() - INTERVAL '1 day' WHERE series = 'xkeys' AND event = $1").bind(target).execute(&pool).await.unwrap();
        let ended = post(&client, copied_manager, &format!("{base}/roles/{}/approve", request_ids[3]), &csrf, &[]).await;
        assert!(ended.1.contains("has ended"));
        assert_eq!(request_status(&pool, request_ids[3]).await, "rejected");
    }).catch_unwind().await;
    // Cleanup even when an assertion fails; the suite operates only on a *_test database.
    sqlx::query("DELETE FROM signups WHERE id = ANY($1)")
        .bind(&signup_ids)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_requests WHERE id = ANY($1)")
        .bind(&request_ids)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_bindings WHERE id = ANY($1)")
        .bind(&binding_ids)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_types WHERE id = $1")
        .bind(key * 10 + 30)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM races WHERE id = ANY($1)")
        .bind(&race_ids)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizers WHERE organizer = ANY($1)")
        .bind(&ids)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM events WHERE series = 'xkeys' AND event = ANY($1)")
        .bind(&slugs)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(&ids)
        .execute(&pool)
        .await
        .unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
