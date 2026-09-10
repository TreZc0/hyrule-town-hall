use crate::{event::{Data, Series}, prelude::*};

pub(super) async fn editor(
    transaction: &mut Transaction<'_, Postgres>,
    event: &Data<'_>,
    csrf: Option<&CsrfToken>,
    ctx: &Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    if event.qualifier_mode != "rank" { return Ok(html! {}); }
    let mut teams = Team::for_event(transaction, event.series, &event.event).await?;
    teams.sort_by_key(|team| (team.qualifier_rank.is_none(), team.qualifier_rank, team.id));
    let mut rows = Vec::with_capacity(teams.len());
    for team in teams {
        let name = team.to_html(transaction, false).await?;
        rows.push((team, name));
    }
    Ok(html! {
        h2 : "Stored qualifier ranks";
        p : "Assign the ranking from your external qualifier results here. Rank 1 is highest; equal ranks are allowed for ties. Leave a rank empty to mark that entrant as unranked. Save each entrant separately.";
        p : "These ranks are used immediately by the entrant list's rank-based ordering. This does not calculate scores, import results, or create bracket matches.";
        @if rows.is_empty() {
            p : "No active entrants yet. Register or import entrants before assigning their ranks.";
        } else {
            table {
                thead { tr { th : "Entrant / team"; th : "Qualifier rank"; } }
                tbody {
                    @for (team, name) in rows {
                        @let field_id = format!("qualifier-rank-{}", team.id);
                        @let team_id = team.id.to_string();
                        @let is_submitted = ctx.field_value("team_id") == Some(team_id.as_str());
                        @let value = if is_submitted { ctx.field_value("rank").unwrap_or("").to_owned() } else { team.qualifier_rank.map(|rank| rank.to_string()).unwrap_or_default() };
                        tr {
                            td : name;
                            td {
                                : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                                    input(type = "hidden", name = "team_id", value = &team_id);
                                    input(type = "hidden", name = "previous_rank", value = team.qualifier_rank.map(|rank| rank.to_string()).unwrap_or_default());
                                    : crate::http::setting_label(&field_id, "Rank", html! {
                                        p : "An organizer-assigned placing for this entrant or team. Enter an integer from 1 to 32767. Lower numbers rank ahead of higher numbers; equal numbers retain a tie. This is a rank, not a points total or finish time.";
                                        p : "Clear the field and save to remove the assigned rank. Unranked entrants remain registered and appear after ranked entrants. For team events, the rank belongs to the whole team. Saving takes effect on the event's rank-based entrant list; it does not regenerate an external bracket.";
                                    });
                                    input(type = "number", id = &field_id, name = "rank", min = "1", max = "32767", step = "1", value = value, placeholder = "Unranked");
                                }, if is_submitted { ctx.errors().collect_vec() } else { Vec::new() }, "Save rank");
                            }
                        }
                    }
                }
            }
        }
        script(src = static_url!("setting-help.js")) {}
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RankForm {
    #[field(default = String::new())]
    csrf: String,
    team_id: i64,
    rank: String,
    previous_rank: Option<i16>,
}

fn parse_rank(value: &str) -> Result<Option<i16>, &'static str> {
    if value.trim().is_empty() { return Ok(None); }
    value.trim().parse::<i16>().ok().filter(|rank| *rank > 0).map(Some)
        .ok_or("Enter a whole-number rank from 1 to 32767, or leave it empty to clear the rank.")
}

#[rocket::post("/event/<series>/<event>/qualifiers/rank", data = "<form>")]
pub(crate) async fn post(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, RankForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    // Serialize with changes to the event's qualification method.
    sqlx::query("SELECT 1 FROM events WHERE series = $1 AND event = $2 FOR UPDATE")
        .bind(series).bind(event).execute(&mut *transaction).await?;
    let data = Data::new(&mut transaction, series, event).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    if data.qualifier_mode != "rank" {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    let value = form.value.as_ref().ok_or(StatusOrError::Status(Status::BadRequest))?;
    let current: Option<Option<i16>> = sqlx::query_scalar("SELECT qualifier_rank FROM teams WHERE id = $1 AND series = $2 AND event = $3 AND NOT resigned FOR UPDATE")
        .bind(value.team_id).bind(series).bind(event).fetch_optional(&mut *transaction).await?;
    let current = current.ok_or(StatusOrError::Status(Status::NotFound))?;
    let validation = if current != value.previous_rank {
        Err(format!("Another organizer changed this rank to {}. Your attempted value is still shown; review the change before saving again.", current.map(|rank| rank.to_string()).unwrap_or_else(|| "unranked".into())))
    } else {
        parse_rank(&value.rank).map_err(str::to_owned)
    };
    match validation {
        Ok(rank) => {
            sqlx::query("UPDATE teams SET qualifier_rank = $1 WHERE id = $2 AND series = $3 AND event = $4 AND NOT resigned")
                .bind(rank).bind(value.team_id).bind(series).bind(event).execute(&mut *transaction).await?;
            transaction.commit().await?;
            Ok(RedirectOrContent::Redirect(Redirect::to(uri!(super::get(series, event)))))
        }
        Err(error) => {
            form.context.push_error(form::Error::validation(error).with_name("rank"));
            let is_started = data.is_started(&mut transaction).await?;
            Ok(RedirectOrContent::Content(super::qualifiers_form(transaction, me, uri, csrf.as_ref(), data, is_started, form.context).await?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::{fairing::AdHoc, http::{ContentType, Cookie, Header}, local::asynchronous::Client};

    #[test]
    fn rank_input_accepts_positive_ranks_and_explicit_clear() {
        for text in ["", " "] { assert_eq!(parse_rank(text), Ok(None)); }
        assert_eq!(parse_rank("1"), Ok(Some(1)));
        assert_eq!(parse_rank(" 32767 "), Ok(Some(32767)));
        for text in ["0", "-1", "32768", "1.5", "first"] { assert!(parse_rank(text).is_err(), "{text}"); }
    }

    #[rocket::get("/test-token")]
    fn token(csrf: CsrfToken) -> String { csrf.authenticity_token() }

    async fn submit(client: &Client, staff: i64, event: &str, team: i64, rank: &str, previous: &str, csrf: Option<&str>) -> (Status, String) {
        let mut body = url::form_urlencoded::Serializer::new(String::new());
        body.append_pair("team_id", &team.to_string()).append_pair("rank", rank).append_pair("previous_rank", previous);
        if let Some(csrf) = csrf { body.append_pair("csrf", csrf); }
        let response = client.post(format!("/event/xkeys/{event}/qualifiers/rank"))
            .header(ContentType::Form).header(Header::new("x-test-user", staff.to_string()))
            .private_cookie(Cookie::new("csrf_token", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="))
            .body(body.finish()).dispatch().await;
        (response.status(), response.into_string().await.unwrap_or_default())
    }

    async fn stored(pool: &PgPool, team: i64) -> Option<i16> {
        sqlx::query_scalar("SELECT qualifier_rank FROM teams WHERE id = $1").bind(team).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL"]
    async fn rank_editor_checks_permissions_scope_validation_and_stale_edits() {
        let pool = event::configuration::test_pool().await;
        let event = format!("r{:07x}", rng().random_range(0_u32..0x10000000));
        let other = format!("r{:07x}", rng().random_range(0_u32..0x10000000));
        assert_ne!(event, other);
        let base = rng().random_range(1_000_000_000_000_i64..2_000_000_000_000);
        let teams = [base, base + 1, base + 2, base + 3];
        let user_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM users ORDER BY id LIMIT 20").fetch_all(&pool).await.unwrap();
        let mut users = Vec::new();
        for id in user_ids {
            let user = User::from_id(&pool, Id::from(id as u64)).await.unwrap().unwrap();
            if !user.is_global_admin() { users.push(id); }
        }
        assert!(users.len() >= 2);
        let (staff, outsider) = (users[0], users[1]);
        let result = std::panic::AssertUnwindSafe(async {
            for slug in [&event, &other] {
                sqlx::query("INSERT INTO events (series, event, display_name, team_config, qualifier_mode) VALUES ('xkeys', $1, 'Rank editor test', 'solo', 'rank')")
                    .bind(slug).execute(&pool).await.unwrap();
            }
            for (index, team) in teams.iter().enumerate() {
                sqlx::query("INSERT INTO teams(id, series, event, name, resigned) VALUES ($1, 'xkeys', $2, $3, $4)")
                    .bind(team).bind(if index == 2 { &other } else { &event }).bind(format!("Entrant {index}"))
                    .bind(index == 3).execute(&pool).await.unwrap();
            }
            sqlx::query("INSERT INTO organizers(series, event, organizer) VALUES ('xkeys', $1, $2)").bind(&event).bind(staff).execute(&pool).await.unwrap();
            let auth_pool = pool.clone();
            let rocket = rocket::build().manage(pool.clone()).attach(rocket_csrf::Fairing::default())
                .attach(AdHoc::on_request("rank fixture identity", move |request, _| {
                    let pool = auth_pool.clone();
                    Box::pin(async move {
                        if let Some(id) = request.headers().get_one("x-test-user").and_then(|id| id.parse::<i64>().ok()) {
                            let user = User::from_id(&pool, Id::from(id as u64)).await.unwrap();
                            request.local_cache(|| user);
                        }
                    })
                })).mount("/", rocket::routes![token, post, super::super::get]);
            let client = Client::tracked(rocket).await.unwrap();
            let csrf = client.get("/test-token").private_cookie(Cookie::new("csrf_token", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="))
                .dispatch().await.into_string().await.unwrap();
            for token in [None, Some("incorrect")] {
                assert_eq!(submit(&client, staff, &event, teams[0], "1", "", token).await.0, Status::BadRequest);
                assert_eq!(stored(&pool, teams[0]).await, None);
            }
            assert_eq!(submit(&client, outsider, &event, teams[0], "1", "", Some(&csrf)).await.0, Status::Forbidden);
            assert_eq!(submit(&client, staff, &event, teams[2], "1", "", Some(&csrf)).await.0, Status::NotFound);
            assert_eq!(submit(&client, staff, &event, teams[3], "1", "", Some(&csrf)).await.0, Status::NotFound);
            for value in ["0", "-1", "32768", "1.5"] {
                let (status, body) = submit(&client, staff, &event, teams[0], value, "", Some(&csrf)).await;
                assert_eq!(status, Status::Ok, "{body}");
                assert!(body.contains("Enter a whole-number rank"), "{body}");
                assert_eq!(stored(&pool, teams[0]).await, None);
            }
            for team in &teams[..2] {
                assert_eq!(submit(&client, staff, &event, *team, "1", "", Some(&csrf)).await.0, Status::SeeOther);
                assert_eq!(stored(&pool, *team).await, Some(1));
            }
            // An older form must not overwrite a rank assigned since it was opened.
            let (status, body) = submit(&client, staff, &event, teams[0], "2", "", Some(&csrf)).await;
            assert_eq!(status, Status::Ok);
            assert!(body.contains("Another organizer changed this rank to 1"));
            assert_eq!(stored(&pool, teams[0]).await, Some(1));
            assert_eq!(submit(&client, staff, &event, teams[0], "32767", "1", Some(&csrf)).await.0, Status::SeeOther);
            assert_eq!(stored(&pool, teams[0]).await, Some(32767));
            let response = client.get(format!("/event/xkeys/{event}/qualifiers")).header(Header::new("x-test-user", staff.to_string())).dispatch().await;
            assert_eq!(response.status(), Status::Ok);
            let body = response.into_string().await.unwrap();
            assert!(body.contains("Save rank"));
            assert!(body.find("Entrant 1").unwrap() < body.find("Entrant 0").unwrap());
            assert!(!body.contains("Entrant 2") && !body.contains("Entrant 3"));
            assert_eq!(submit(&client, staff, &event, teams[0], "", "32767", Some(&csrf)).await.0, Status::SeeOther);
            assert_eq!(stored(&pool, teams[0]).await, None);
            sqlx::query("UPDATE events SET qualifier_mode = 'none' WHERE series = 'xkeys' AND event = $1").bind(&event).execute(&pool).await.unwrap();
            assert_eq!(submit(&client, staff, &event, teams[0], "1", "", Some(&csrf)).await.0, Status::Conflict);
            assert_eq!(stored(&pool, teams[0]).await, None);
        }).catch_unwind().await;
        sqlx::query("DELETE FROM organizers WHERE series = 'xkeys' AND event IN ($1, $2)").bind(&event).bind(&other).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM teams WHERE id = ANY($1)").bind(&teams[..]).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM events WHERE series = 'xkeys' AND event IN ($1, $2)").bind(&event).bind(&other).execute(&pool).await.unwrap();
        if let Err(panic) = result { std::panic::resume_unwind(panic); }
    }
}
