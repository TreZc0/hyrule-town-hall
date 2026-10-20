use super::*;

pub(super) fn placeholder(url: rocket::http::uri::Origin<'_>) -> RawHtml<String> {
    html! {
        div(class = "qualifier-lazy-content") {
            p(role = "status") { a(href = url.to_string()) : "Load results"; }
        }
    }
}

fn results(
    event: &Data<'_>,
    csrf: Option<&CsrfToken>,
    config: &pooled_qualifiers::Config,
    attempts: &[pools::AttemptRow],
) -> RawHtml<String> {
    let par = pools::population(config, &attempts.iter().collect_vec()).par;
    html! {
        table {
            thead { tr { th : "Entrant"; th : "Mode"; th : "Source"; th : "State"; th : "Outcome"; th : "Par time"; th : "Points"; th : "Counted"; th : "Retry of"; th : "Re-attempt declaration"; th : "Review / history"; th : "VOD"; } }
            tbody {
                @for attempt in attempts {
                    tr(id = format!("attempt-{}", attempt.id)) {
                        td { : &attempt.entrant_name; : format!(" ({})", attempt.team_id.map_or_else(|| "awaiting signup".into(), |team| team.to_string())); }
                        td : &attempt.mode_name;
                        td : &attempt.source;
                        td : &attempt.state;
                        td {
                            : attempt.official_outcome.as_deref().unwrap_or("");
                            @if let Some(ref time) = attempt.official_time {
                                : format!(" — {}", English.format_duration(Duration::from_micros(u64::try_from(time.microseconds.max(0)).unwrap_or_default()), false));
                            }
                        }
                        td : par.map(|seconds| English.format_duration(Duration::from_secs_f64(seconds), false)).unwrap_or_else(|| "Pending".into());
                        td {
                            @if attempt.state == "void" || !attempt.signup_eligible {
                                : "Excluded";
                            } else if let Some(outcome) = attempt.outcome() {
                                @match pooled_qualifiers::performance_score(config, outcome, par) {
                                    pooled_qualifiers::ModeScore::Pending => { : "Pending"; }
                                    pooled_qualifiers::ModeScore::Score(points) => { : format!("{points:.3}"); }
                                }
                            } else {
                                : "Pending";
                            }
                        }
                        td : if attempt.counts_for_entrant { "yes" } else { "no" };
                        td {
                            @if let Some(id) = attempt.retry_of {
                                a(href = format!("#attempt-{id}")) : "Previous result";
                            }
                        }
                        td {
                            @if let Some(declared_at) = attempt.retry_declared_at {
                                : if attempt.superseded_by.is_some() { "Used — " } else if attempt.retry_banned_at.is_some() { "Blocked — " } else if attempt.counts_for_entrant && attempt.state != "void" { "Pending — " } else { "Inactive — " };
                                : format_datetime(declared_at, DateTimeFormat { long: true, running_text: false });
                            }
                        }
                        td {
                            @if let Some(error) = &attempt.delivery_error { p : error; }
                            @if attempt.delivery_error.is_some() {
                                details {
                                    summary : "Recover Discord delivery";
                                    p : "Review the private thread or organizers channel first. Retrying READY or seed delivery keeps the original seed and preparation deadline. GO recovery only accepts an existing bot GO message.";
                                    : full_form(uri!(post_pooled_recover(event.series, &*event.event)), csrf, html! {
                                        input(type = "hidden", name = "attempt_id", value = attempt.id);
                                        input(type = "hidden", name = "control_version", value = attempt.control_version);
                                        : help::label(&format!("recovery-{}-action", attempt.id), "recovery_action", "Recovery action");
                                        select(id = format!("recovery-{}-action", attempt.id), name = "action") {
                                            option(value = "ready") : "Retry READY delivery";
                                            option(value = "seed") : "Retry same seed delivery";
                                            option(value = "thread") : "Connect existing private thread";
                                            option(value = "go") : "Connect existing GO message";
                                            option(value = "retry-declared") : "Retry re-attempt announcement (or connect existing message)";
                                        }
                                        : help::label(&format!("recovery-{}-discord_id", attempt.id), "discord_id", "Discord thread/message ID (for connection actions)");
                                        input(id = format!("recovery-{}-discord_id", attempt.id), name = "discord_id", type = "number");
                                        : help::label(&format!("recovery-{}-reason", attempt.id), "reason", "Reason (optional)");
                                        textarea(id = format!("recovery-{}-reason", attempt.id), name = "reason");
                                    }, Vec::new(), "Save recovery");
                                }
                            }

                            @if attempt.source == "async" || attempt.state == "void" || attempt.correction_history.as_array().is_some_and(|history| !history.is_empty()) {
                                details {
                                    summary : if attempt.source == "async" && attempt.state != "void" { "Review / correct result" } else { "Review history" };
                                    @if attempt.state == "void" {
                                        p : "This attempt was invalidated and does not count.";
                                    } else if attempt.source == "async" {
                                        p(class = "qualifier-hint") : "Verify this run’s evidence or correct its recorded result. For a finish, enter the official time and recording URL.";
                                        : full_form(uri!(post_pooled_result(event.series, &*event.event)), csrf, html! {
                                            input(type = "hidden", name = "attempt_id", value = attempt.id);
                                            input(type = "hidden", name = "control_version", value = attempt.control_version);
                                            input(type = "hidden", name = "action", value = "result");
                                            : help::label(&format!("review-{}-outcome", attempt.id), "outcome", "Official outcome");
                                            select(id = format!("review-{}-outcome", attempt.id), name = "outcome") {
                                                option(value = "finished") : "Finished";
                                                option(value = "forfeit") : "Forfeit / missing evidence";
                                            }
                                            : help::label(&format!("review-{}-finish_time", attempt.id), "finish_time", "Time (HH:MM:SS, required for a finish)");
                                            input(id = format!("review-{}-finish_time", attempt.id), name = "finish_time", placeholder = "01:23:45");
                                            : help::label(&format!("review-{}-vod", attempt.id), "vod", "VOD URL (required for a finish)");
                                            input(id = format!("review-{}-vod", attempt.id), name = "vod", type = "url", value = attempt.vod.as_deref().unwrap_or(""));
                                            : help::label(&format!("review-{}-reason", attempt.id), "reason", "Reason (optional)");
                                            textarea(id = format!("review-{}-reason", attempt.id), name = "reason");
                                        }, Vec::new(), "Save reviewed change");
                                    }
                                    @if attempt.correction_history.as_array().is_some_and(|history| !history.is_empty()) {
                                        pre : serde_json::to_string_pretty(&attempt.correction_history).unwrap_or_default();
                                    }
                                }
                            }
                            @if attempt.state != "void" {
                                details {
                                    summary : "DQ / invalidate attempt";
                                    form(action = uri!(post_pooled_result(event.series, &*event.event)).to_string(), method = "post") {
                                        : csrf;
                                        input(type = "hidden", name = "attempt_id", value = attempt.id);
                                        input(type = "hidden", name = "control_version", value = attempt.control_version);
                                        p(class = "qualifier-hint") : "DQ keeps this attempt with 0 points. Invalidate cancels it, removing it from scoring and freeing the attempt or retry it used.";
                                        : help::label(&format!("attempt-{}-reason", attempt.id), "reason", "Reason (optional)");
                                        textarea(id = format!("attempt-{}-reason", attempt.id), name = "reason");
                                        fieldset {
                                            button(type = "submit", name = "action", value = "dq", onclick = "return confirm('Disqualify this attempt? It will remain used and count for 0 points.')") : "DQ";
                                            button(type = "submit", name = "action", value = "invalidate", onclick = "return confirm('Invalidate this attempt? It will be removed from scoring and its attempt or retry usage will be released. If this was a retry, the previous valid result will be restored.')") : "Invalidate";
                                        }
                                    }
                                }
                            }
                        }

                        td {
                            @if let Some(ref vod) = attempt.vod {
                                a(href = vod) : "VOD";
                            }
                        }
                    }
                }
            }
        }
    }
}

#[rocket::get("/event/<series>/<event>/qualifiers/details/results/<seed_id>")]
pub(crate) async fn get_results(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
    seed_id: i64,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    require_organizer(&mut transaction, &me, series, &event).await?;
    require_seed(&mut transaction, series, &event, seed_id).await?;
    let config = pooled_qualifiers::Config::load(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let attempts = pools::load_attempts(&mut transaction, series, &event, Some(seed_id)).await?;
    Ok(results(&data, csrf.as_ref(), &config, &attempts))
}

async fn require_seed(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    seed_id: i64,
) -> Result<(), StatusOrError<event::Error>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM qualifier_seeds WHERE series = $1 AND event = $2 AND id = $3)",
    )
    .bind(series)
    .bind(event)
    .bind(seed_id)
    .fetch_one(&mut **transaction)
    .await?;
    if !exists {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    Ok(())
}

#[rocket::get("/event/<series>/<event>/qualifiers/details/pool/<seed_id>")]
pub(crate) async fn get_pool(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: String,
    seed_id: i64,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    require_organizer(&mut transaction, &me, series, &event).await?;
    let config = pooled_qualifiers::Config::load(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let seeds = pools::load_seeds(&mut transaction, series, &event).await?;
    let seed = seeds
        .iter()
        .find(|seed| seed.id == seed_id)
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let attempts = pools::load_attempts(&mut transaction, series, &event, Some(seed_id)).await?;
    let data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(pools::history(
        &config,
        seed,
        &attempts.iter().collect_vec(),
        data.discord_guild.map(|guild| guild.get()),
    ))
}

#[derive(sqlx::FromRow)]
struct LiveEntryRow {
    racetime_entrant_id: String,
    entrant_name: Option<String>,
    eligible: Option<bool>,
    exclusion_reason: Option<String>,
    present_at_go: Option<bool>,
    retry_reserved_at: Option<DateTime<Utc>>,
    retry_committed_at: Option<DateTime<Utc>>,
    retry_released_at: Option<DateTime<Utc>>,
}

#[rocket::get("/event/<series>/<event>/qualifiers/details/live/<race_id>")]
pub(crate) async fn get_live(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: String,
    race_id: i64,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    require_organizer(&mut transaction, &me, series, &event).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qualifier_seeds WHERE series = $1 AND event = $2 AND live_race_id = $3)")
        .bind(series).bind(&event).bind(race_id).fetch_one(&mut *transaction).await?;
    if !exists {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    let entries = sqlx::query_as::<_, LiveEntryRow>(
            r#"SELECT entry.racetime_entrant_id,
                COALESCE(user_account.racetime_display_name, user_account.discord_display_name,
                    linked_account.racetime_display_name, linked_account.discord_display_name,
                    result.racetime_name) AS entrant_name,
                entry.eligible, entry.exclusion_reason, entry.present_at_go,
                entry.retry_reserved_at, entry.retry_committed_at, entry.retry_released_at
            FROM qualifier_live_entries entry
            JOIN qualifier_seeds seed ON seed.id = entry.seed_id
            LEFT JOIN users user_account ON user_account.racetime_id = entry.racetime_entrant_id
            LEFT JOIN users linked_account ON linked_account.id = entry.user_id
            LEFT JOIN LATERAL (
                SELECT attempt.allocation_metadata->>'racetime_name' AS racetime_name
                FROM qualifier_attempts attempt
                WHERE attempt.racetime_id = entry.racetime_entrant_id
                    AND NULLIF(attempt.allocation_metadata->>'racetime_name', '') IS NOT NULL
                ORDER BY attempt.requested_at DESC, attempt.id DESC LIMIT 1
            ) result ON TRUE
            WHERE entry.series = $1 AND entry.event = $2 AND seed.live_race_id = $3
                AND EXISTS (SELECT 1 FROM qualifier_live_entries started
                    WHERE started.seed_id = entry.seed_id AND started.present_at_go = TRUE)
            ORDER BY LOWER(COALESCE(user_account.racetime_display_name, user_account.discord_display_name,
                linked_account.racetime_display_name, linked_account.discord_display_name,
                result.racetime_name, entry.racetime_entrant_id)), entry.racetime_entrant_id"#,
        )
        .bind(series)
        .bind(&event)
        .bind(race_id)
        .fetch_all(&mut *transaction)
        .await?;

    Ok(html! {
        table {
            thead { tr { th : "Entrant"; th : "Eligible"; th : "Present at start"; th : "Re-attempt"; th : "Reason"; } }
            tbody {
                @for entry in entries {
                    tr {
                        td {
                            a(href = format!("https://{}/user/{}", racetime_host(), entry.racetime_entrant_id)) {
                                : entry.entrant_name.as_deref().unwrap_or("Unknown racer");
                            }
                        }
                        td : entry.eligible.map(|value| if value { "yes" } else { "no" }).unwrap_or("pending");
                        td : entry.present_at_go.map(|value| if value { "yes" } else { "no" }).unwrap_or("pending");
                        td : if entry.retry_committed_at.is_some() { "Used" } else if entry.retry_released_at.is_some() { "Not used" } else if entry.retry_reserved_at.is_some() { "Pending" } else { "" };
                        td : entry.exclusion_reason.as_deref().unwrap_or("");
                    }
                }
            }
        }
    })
}
