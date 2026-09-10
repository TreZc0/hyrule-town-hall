use crate::{
    cal::{Entrants, Race, RaceSchedule, Source},
    discord_scheduled_events::DiscordCtx,
    event::{Data, QualifierScoreHiding, Series, Tab, pooled_qualifiers},
    prelude::*,
    seed, volunteer_requests,
};

async fn qualifiers_form(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    event: Data<'_>,
    is_started: bool,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    let header = event
        .header(&mut transaction, Some(&me), Tab::Qualifiers, false)
        .await?;

    struct RaceRow {
        id: Id<Races>,
        phase: Option<String>,
        qualifier_number: Option<i64>,
        round: Option<String>,
        start: Option<DateTime<Utc>>,
        room: Option<String>,
        mode_name: Option<String>,
    }
    let races = sqlx::query_as!(
        RaceRow,
        r#"SELECT race.id AS "id: Id<Races>", race.phase, race.qualifier_number,
            race.round, race.start, race.room, mode.display_name AS mode_name
        FROM races race
        LEFT JOIN qualifier_seeds seed ON seed.live_race_id = race.id AND seed.source = 'live'
        LEFT JOIN qualifier_modes mode ON mode.id = seed.mode_id
        WHERE race.series = $1 AND race.event = $2 AND race.is_qualifier
        ORDER BY race.start NULLS LAST, race.round"#,
        event.series as _,
        &event.event
    )
    .fetch_all(&mut *transaction)
    .await?;
    let seeding_race = sqlx::query_as!(RaceRow,
        r#"SELECT race.id AS "id: Id<Races>", race.phase, race.qualifier_number,
            race.round, race.start, race.room, NULL::TEXT AS mode_name
        FROM races race WHERE race.series = $1 AND race.event = $2 AND race.phase = 'Seeding' LIMIT 1"#,
        event.series as _, &event.event
    )
    .fetch_optional(&mut *transaction)
    .await?;

    #[derive(sqlx::FromRow)]
    struct SeedRow {
        mode_id: i64,
        source: String,
        pool_position: Option<i16>,
        generation_state: String,
        physical_seed_identity: Option<String>,
        released_at: Option<DateTime<Utc>>,
        generation_error: Option<String>,
    }
    #[derive(sqlx::FromRow)]
    struct AttemptRow {
        id: i64,
        team_id: i64,
        entrant_name: String,
        mode_name: String,
        source: String,
        state: String,
        counts_for_entrant: bool,
        official_outcome: Option<String>,
        official_time: Option<sqlx::postgres::types::PgInterval>,
        vod: Option<String>,
        retry_of: Option<i64>,
        control_version: i64,
        delivery_error: Option<String>,
        correction_history: serde_json::Value,
    }
    #[derive(sqlx::FromRow)]
    struct LiveEntryRow {
        live_race_id: i64,
        mode_name: String,
        racetime_entrant_id: String,
        eligible: Option<bool>,
        exclusion_reason: Option<String>,
        present_at_go: Option<bool>,
        retry_reserved_at: Option<DateTime<Utc>>,
        retry_committed_at: Option<DateTime<Utc>>,
        retry_released_at: Option<DateTime<Utc>>,
        attempt_id: Option<i64>,
    }
    let pooled_config =
        pooled_qualifiers::Config::load(&mut transaction, event.series, &event.event).await?;
    let pooled_modes = if pooled_config.is_some() {
        pooled_qualifiers::Mode::for_event(&mut transaction, event.series, &event.event).await?
    } else {
        Vec::new()
    };
    let pooled_seeds = if pooled_config.is_some() {
        sqlx::query_as::<_, SeedRow>(
            r#"SELECT mode_id, source, pool_position,
            generation_state, physical_seed_identity, released_at, generation_error
            FROM qualifier_seeds WHERE series = $1 AND event = $2
            ORDER BY mode_id, source, pool_position, live_race_id"#,
        )
        .bind(event.series)
        .bind(&event.event)
        .fetch_all(&mut *transaction)
        .await?
    } else {
        Vec::new()
    };
    let pooled_readiness = if pooled_config.is_some() {
        pooled_qualifiers::readiness(&mut transaction, event.series, &event.event).await?
    } else {
        Vec::new()
    };
    let pooled_attempts = if pooled_config.is_some() {
        sqlx::query_as::<_, AttemptRow>(
            r#"SELECT attempt.id, attempt.team_id,
                COALESCE(user_account.discord_display_name, user_account.racetime_display_name,
                    'Team ' || attempt.team_id::TEXT) AS entrant_name,
                mode.display_name AS mode_name, attempt.source, attempt.state,
                attempt.counts_for_entrant, attempt.official_outcome,
                attempt.official_time, attempt.vod, attempt.retry_of, attempt.control_version, attempt.delivery_error, attempt.correction_history
            FROM qualifier_attempts attempt
            JOIN qualifier_modes mode ON mode.id = attempt.mode_id
            LEFT JOIN team_members member ON member.team = attempt.team_id
            LEFT JOIN users user_account ON user_account.id = member.member
            WHERE attempt.series = $1 AND attempt.event = $2
            ORDER BY attempt.requested_at, attempt.id"#,
        )
        .bind(event.series)
        .bind(&event.event)
        .fetch_all(&mut *transaction)
        .await?
    } else {
        Vec::new()
    };
    let pooled_live_entries = if pooled_config.is_some() {
        sqlx::query_as::<_, LiveEntryRow>(
            r#"SELECT seed.live_race_id, mode.display_name AS mode_name,
                entry.racetime_entrant_id, entry.eligible, entry.exclusion_reason,
                entry.present_at_go, entry.retry_reserved_at, entry.retry_committed_at,
                entry.retry_released_at, entry.attempt_id
            FROM qualifier_live_entries entry
            JOIN qualifier_seeds seed ON seed.id = entry.seed_id
            JOIN qualifier_modes mode ON mode.id = entry.mode_id
            WHERE entry.series = $1 AND entry.event = $2
            ORDER BY seed.live_race_id, entry.racetime_entrant_id"#,
        )
        .bind(event.series)
        .bind(&event.event)
        .fetch_all(&mut *transaction)
        .await?
    } else {
        Vec::new()
    };
    let new_pooled_mode = pooled_qualifiers::Mode {
        id: 0,
        position: i16::try_from(pooled_modes.len() + 1).unwrap_or(1),
        slug: String::new(),
        display_name: String::new(),
        seed_gen_type: String::new(),
        seed_config: serde_json::json!({}),
        generator_profile: "default".into(),
        settings_fingerprint: String::new(),
        enabled: true,
    };

    let current_role_str = event
        .qualifier_notification_role_id
        .map(|id| id.get().to_string())
        .unwrap_or_default();
    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Qualifiers — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Qualifier Announcement Ping";
            : full_form(uri!(post_notification_role(event.series, &*event.event)), csrf, html! {
                : form_field("notification_role_id", &mut ctx.errors().collect_vec(), html! {
                    label(for = "notification_role_id") : "Role ID to ping when a qualifier room opens:";
                    input(type = "text", id = "notification_role_id", name = "notification_role_id", value = ctx.field_value("notification_role_id").unwrap_or(&current_role_str), placeholder = "Discord role ID (optional)", style = "width: 100%; max-width: 400px;");
                });
            }, ctx.errors().collect_vec(), "Save");
            @if event.qualifier_notification_role_id.is_some() {
                form(action = uri!(delete_notification_role(event.series, &*event.event)).to_string(), method = "post", style = "display: inline;") {
                    input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                    button(type = "submit") : "Disable Ping";
                }
            }

            h2 : "Qualifier Settings";
            : full_form(uri!(post_settings(event.series, &*event.event)), csrf, html! {
                : form_field("qualifier_score_hiding", &mut ctx.errors().collect_vec(), html! {
                    label(for = "qualifier_score_hiding") : "Qualifier Score Hiding";
                    select(id = "qualifier_score_hiding", name = "qualifier_score_hiding") {
                        option(value = "none", selected? = ctx.field_value("qualifier_score_hiding").map_or(event.qualifier_score_hiding == QualifierScoreHiding::None, |v| v == "none")) : "None (show all scores)";
                        option(value = "async_only", selected? = ctx.field_value("qualifier_score_hiding").map_or(event.qualifier_score_hiding == QualifierScoreHiding::AsyncOnly, |v| v == "async_only")) : "Async only (hide async scores)";
                        option(value = "full_points", selected? = ctx.field_value("qualifier_score_hiding").map_or(event.qualifier_score_hiding == QualifierScoreHiding::FullPoints, |v| v == "full_points")) : "Full points (hide all points)";
                        option(value = "full_points_counts", selected? = ctx.field_value("qualifier_score_hiding").map_or(event.qualifier_score_hiding == QualifierScoreHiding::FullPointsCounts, |v| v == "full_points_counts")) : "Full points + counts";
                        option(value = "full_complete", selected? = ctx.field_value("qualifier_score_hiding").map_or(event.qualifier_score_hiding == QualifierScoreHiding::FullComplete, |v| v == "full_complete")) : "Full complete (hide everything)";
                    }
                });
                : form_field("automated_asyncs", &mut ctx.errors().collect_vec(), html! {
                    input(type = "checkbox", id = "automated_asyncs", name = "automated_asyncs", checked? = ctx.field_value("automated_asyncs").map_or(event.automated_asyncs, |v| v == "on"));
                    label(for = "automated_asyncs") : "Use automated Discord threads for qualifier asyncs";
                    label(class = "help") : " (When enabled, qualifier requests create private Discord threads with READY/countdown/FINISH buttons)";
                });
            }, ctx.errors().collect_vec(), "Save Settings");

            @if let Some(ref config) = pooled_config {
                h2 : "Pooled Qualifier Configuration";
                @if pooled_readiness.is_empty() {
                    p : "Ready for activation.";
                } else {
                    div(class = "bg-surface") {
                        strong : "Configuration still needs attention:";
                        ul {
                            @for error in &pooled_readiness { li : error; }
                        }
                    }
                }
                : full_form(uri!(post_pooled_config(event.series, &*event.event)), csrf, html! {
                    label(for = "required_mode_count") : "Required modes";
                    input(type = "number", min = "1", name = "required_mode_count", value = config.required_mode_count);
                    label(for = "pool_seed_count") : "Private seeds per mode";
                    input(type = "number", min = "1", name = "pool_seed_count", value = config.pool_seed_count);
                    label(for = "live_races_per_mode") : "Live races per mode";
                    input(type = "number", min = "0", name = "live_races_per_mode", value = config.live_races_per_mode);
                    label(for = "requests_open_at") : "Requests open (UTC)";
                    input(type = "datetime-local", name = "requests_open_at", value = config.requests_open_at.map(|value| value.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default());
                    label(for = "requests_close_at") : "Last request (UTC)";
                    input(type = "datetime-local", name = "requests_close_at", value = config.requests_close_at.map(|value| value.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default());
                    label(for = "starts_close_at") : "Last GO (UTC)";
                    input(type = "datetime-local", name = "starts_close_at", value = config.starts_close_at.map(|value| value.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default());
                    label(for = "submissions_close_at") : "Submission deadline (UTC)";
                    input(type = "datetime-local", name = "submissions_close_at", value = config.submissions_close_at.map(|value| value.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default());
                    label(for = "retries_close_at") : "Retry deadline (UTC)";
                    input(type = "datetime-local", name = "retries_close_at", value = config.retries_close_at.map(|value| value.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default());
                    label(for = "results_release_at") : "Publish standings (UTC)";
                    input(type = "datetime-local", name = "results_release_at", value = config.results_release_at.map(|value| value.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default());
                    label(for = "async_run_limit_hours") : "Async time limit (hours)";
                    input(type = "number", min = "1", name = "async_run_limit_hours", value = config.run_limit().num_hours());
                    label(for = "live_entry_close_minutes") : "Live entry cutoff lead (minutes)";
                    input(type = "number", min = "0", name = "live_entry_close_minutes", value = i64::from(config.live_entry_close_lead.days) * 1440 + config.live_entry_close_lead.microseconds / 60_000_000);
                    label(for = "retry_limit") : "Event-wide retry limit";
                    input(type = "number", min = "0", max = "1", name = "retry_limit", value = config.retry_limit);
                    label(for = "allocation_spread") : "Maximum pool allocation spread";
                    input(type = "number", min = "1", name = "allocation_spread", value = config.allocation_spread);
                    label(for = "par_finishers") : "Finishers used for cohort par";
                    input(type = "number", min = "1", name = "par_finishers", value = config.par_finishers);
                    label(for = "score_scale") : "Score scale";
                    input(type = "number", step = "any", name = "score_scale", value = config.score_scale);
                    label(for = "score_offset") : "Score offset";
                    input(type = "number", step = "any", name = "score_offset", value = config.score_offset);
                    label(for = "score_minimum") : "Minimum score";
                    input(type = "number", step = "any", name = "score_minimum", value = config.score_minimum);
                    label(for = "score_maximum") : "Maximum score";
                    input(type = "number", step = "any", name = "score_maximum", value = config.score_maximum);
                    input(type = "checkbox", name = "requests_paused", id = "requests_paused", checked? = config.requests_paused);
                    label(for = "requests_paused") : "Pause new requests";
                }, ctx.errors().collect_vec(), "Save pooled configuration");

                h3 : "Modes";
                @for mode in pooled_modes.iter().chain(iter::once(&new_pooled_mode)) {
                    @let is_new = mode.id == 0;
                    : full_form(uri!(post_pooled_mode(event.series, &*event.event)), csrf, html! {
                        input(type = "hidden", name = "mode_id", value = mode.id);
                        label : if is_new { "New mode" } else { "Mode" };
                        input(type = "number", min = "1", name = "position", value = mode.position);
                        input(type = "text", name = "slug", value = &mode.slug, placeholder = "stable-slug");
                        input(type = "text", name = "display_name", value = &mode.display_name, placeholder = "Display name");
                        input(type = "text", name = "seed_gen_type", value = &mode.seed_gen_type, placeholder = "owr");
                        textarea(name = "seed_config", rows = "6", cols = "80") : serde_json::to_string_pretty(&mode.seed_config)?;
                        select(name = "generator_profile") {
                            option(value = "default", selected? = true) : "Default qualifier profile (OWR tournament build)";
                        }
                        input(type = "checkbox", name = "enabled", checked? = mode.enabled);
                        label : "Enabled";
                    }, Vec::new(), if is_new { "Add mode" } else { "Save mode" });
                }

                h3 : "Private Seed Pool";
                p : "Seed identities and pool slots are organizer-only. Import a generated seed payload only after its build and settings have been verified.";
                @for mode in pooled_modes.iter().filter(|mode| mode.enabled) {
                    h4 : &mode.display_name;
                    : full_form(uri!(post_pooled_generate(event.series, &*event.event)), csrf, html! {
                        input(type = "hidden", name = "mode_id", value = mode.id);
                        input(type = "hidden", name = "retry_failed", value = "false");
                    }, Vec::new(), "Generate missing slots");
                    : full_form(uri!(post_pooled_generate(event.series, &*event.event)), csrf, html! {
                        input(type = "hidden", name = "mode_id", value = mode.id);
                        label : "Slot";
                        input(type = "number", name = "pool_position", min = "1", max = config.pool_seed_count, required? = true);
                        input(type = "hidden", name = "retry_failed", value = "true");
                    }, Vec::new(), "Generate slot / retry failed slot");
                    table {
                        thead { tr { th : "Slot"; th : "State"; th : "Identity"; th : "Released"; } }
                        tbody {
                            @for seed in pooled_seeds.iter().filter(|seed| seed.mode_id == mode.id && seed.source == "async_pool") {
                                tr {
                                    td : seed.pool_position;
                                    td { : &seed.generation_state; @if let Some(error) = &seed.generation_error { p : error; } }
                                    td : seed.physical_seed_identity.as_deref().unwrap_or("");
                                    td : seed.released_at.map(|value| value.to_rfc3339()).unwrap_or_default();
                                }
                            }
                        }
                    }
                    : full_form(uri!(post_pooled_seed(event.series, &*event.event)), csrf, html! {
                        input(type = "hidden", name = "mode_id", value = mode.id);
                        label : "Pool slot";
                        input(type = "number", min = "1", name = "pool_position");
                        label : "Canonical seed data JSON";
                        textarea(name = "seed_data", rows = "6", cols = "80");
                        label {
                            input(type = "checkbox", name = "attest_settings", required? = true);
                            : "I verified that this seed uses this mode’s baseline settings and the configured deployed generator build.";
                        }
                    }, Vec::new(), "Import or replace unused seed");
                }

                h3 : "Attempt Ledger";
                @if pooled_attempts.is_empty() {
                    p : "No attempts have been assigned.";
                } else {
                    table {
                        thead { tr { th : "ID"; th : "Entrant"; th : "Mode"; th : "Source"; th : "State"; th : "Outcome"; th : "Counted"; th : "Retry of"; th : "Review / history"; th : "VOD"; } }
                        tbody {
                            @for attempt in &pooled_attempts {
                                tr {
                                    td : attempt.id;
                                    td { : &attempt.entrant_name; : format!(" ({})", attempt.team_id); }
                                    td : &attempt.mode_name;
                                    td : &attempt.source;
                                    td : &attempt.state;
                                    td {
                                        : attempt.official_outcome.as_deref().unwrap_or("");
                                        @if let Some(ref time) = attempt.official_time {
                                            : format!(" — {}", English.format_duration(Duration::from_micros(u64::try_from(time.microseconds.max(0)).unwrap_or_default()), false));
                                        }
                                    }
                                    td : if attempt.counts_for_entrant { "yes" } else { "no" };
                                    td : attempt.retry_of.map(|id| id.to_string()).unwrap_or_default();
                                    td {
                                        @if let Some(error) = &attempt.delivery_error { p : error; }
                                        @if attempt.delivery_error.is_some() {
                                            details {
                                                summary : "Recover Discord delivery";
                                                p : "Review the private thread first. Retrying READY or seed delivery keeps the original seed and preparation deadline. GO recovery only accepts an existing bot GO message.";
                                                : full_form(uri!(post_pooled_recover(event.series, &*event.event)), csrf, html! {
                                                    input(type = "hidden", name = "attempt_id", value = attempt.id);
                                                    input(type = "hidden", name = "control_version", value = attempt.control_version);
                                                    select(name = "action") {
                                                        option(value = "ready") : "Retry READY delivery";
                                                        option(value = "seed") : "Retry same seed delivery";
                                                        option(value = "thread") : "Connect existing private thread";
                                                        option(value = "go") : "Connect existing GO message";
                                                    }
                                                    label : "Discord thread/message ID (for connection actions)";
                                                    input(name = "discord_id", type = "number");
                                                    label : "Reason and reviewed evidence";
                                                    textarea(name = "reason", required? = true);
                                                }, Vec::new(), "Save recovery");
                                            }
                                        }

                                        details {
                                            summary : "Review / correct result";
                                            : full_form(uri!(post_pooled_result(event.series, &*event.event)), csrf, html! {
                                                input(type = "hidden", name = "attempt_id", value = attempt.id);
                                                input(type = "hidden", name = "control_version", value = attempt.control_version);
                                                label : "Action";
                                                select(name = "action") {
                                                    option(value = "result") : "Verify or correct result";
                                                    option(value = "disclosure") : "Apply mode disclosure sanction";
                                                    option(value = "reverse_disclosure") : "Reverse mode disclosure sanction";
                                                }
                                                label : "Official outcome";
                                                select(name = "outcome") {
                                                    option(value = "finished") : "Finished";
                                                    option(value = "forfeit") : "Forfeit / missing evidence";
                                                    option(value = "dq") : "Disqualified";
                                                    option(value = "invalid") : "Invalid";
                                                }
                                                label : "Time (HH:MM:SS, required for a finish)";
                                                input(name = "finish_time", placeholder = "01:23:45");
                                                label : "VOD URL (required for a finish)";
                                                input(name = "vod", type = "url", value = attempt.vod.as_deref().unwrap_or(""));
                                                label : "Reason";
                                                textarea(name = "reason", required? = true);
                                            }, Vec::new(), "Save reviewed change");
                                            pre : serde_json::to_string_pretty(&attempt.correction_history).unwrap_or_default();
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

                h3 : "Live Eligibility Ledger";
                @if pooled_live_entries.is_empty() {
                    p : "No live entry cutoff has been recorded.";
                } else {
                    table {
                        thead { tr { th : "Race"; th : "Mode"; th : "Racetime entrant"; th : "Eligible"; th : "At GO"; th : "Retry"; th : "Attempt"; th : "Reason"; } }
                        tbody {
                            @for entry in &pooled_live_entries {
                                tr {
                                    td : entry.live_race_id;
                                    td : &entry.mode_name;
                                    td : &entry.racetime_entrant_id;
                                    td : entry.eligible.map(|value| if value { "yes" } else { "no" }).unwrap_or("pending");
                                    td : entry.present_at_go.map(|value| if value { "yes" } else { "no" }).unwrap_or("pending");
                                    td : if entry.retry_committed_at.is_some() { "committed" } else if entry.retry_released_at.is_some() { "released" } else if entry.retry_reserved_at.is_some() { "reserved" } else { "" };
                                    td : entry.attempt_id.map(|id| id.to_string()).unwrap_or_default();
                                    td : entry.exclusion_reason.as_deref().unwrap_or("");
                                }
                            }
                        }
                    }
                }
            }

            h2 : "Seeding Race";
            @if let Some(ref sr) = seeding_race {
                table {
                    thead {
                        tr {
                            th : "Start Time";
                            th : "Room";
                            @if pooled_config.is_some() { th : "Mode"; }
                            @if !is_started {
                                th : "Actions";
                            }
                        }
                    }
                    tbody {
                        tr {
                            td : sr.start.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or("Unscheduled".to_owned());
                            td {
                                @if let Some(ref room) = sr.room {
                                    a(href = room) : room;
                                }
                            }
                            @if !is_started {
                                td {
                                    a(class = "button", href = uri!(get_edit_seeding_race(event.series, &*event.event, sr.id)).to_string()) : "Edit";
                                    : " | ";
                                    form(action = uri!(delete_seeding_race(event.series, &*event.event, sr.id)).to_string(), method = "post", style = "display: inline;") {
                                        input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                                        button(type = "submit", onclick = "return confirm('Are you sure you want to delete the seeding race?')") : "Delete";
                                    }
                                }
                            }
                        }
                    }
                }
            } else if is_started {
                p : "The event has started. A seeding race cannot be created.";
            } else {
                h3 : "Create Seeding Race";
                : full_form(uri!(post_seeding_race(event.series, &*event.event)), csrf, html! {
                    : form_field("race_start", &mut ctx.errors().collect_vec(), html! {
                        label(for = "seeding_race_start") : "Start Time (UTC)";
                        input(type = "datetime-local", name = "race_start", id = "seeding_race_start", value = ctx.field_value("race_start").unwrap_or(""));
                    });
                    : form_field("race_room", &mut ctx.errors().collect_vec(), html! {
                        label(for = "seeding_race_room") : "Racetime.gg Room URL (optional)";
                        input(type = "text", name = "race_room", id = "seeding_race_room", value = ctx.field_value("race_room").unwrap_or(""), placeholder = "https://racetime.gg/...", style = "width: 100%; max-width: 600px;");
                    });
                }, ctx.errors().collect_vec(), "Create Seeding Race");
            }

            h2 : "Live Qualifier Races";
            @if races.is_empty() {
                p : "No live qualifier races defined.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Number";
                            th : "Phase";
                            th : "Round";
                            th : "Start Time";
                            th : "Room";
                            @if pooled_config.is_some() { th : "Mode"; }
                            @if !is_started {
                                th : "Actions";
                            }
                        }
                    }
                    tbody {
                        @for row in races {
                            tr {
                                td : row.qualifier_number.unwrap_or(1);
                                td : row.phase.as_deref().unwrap_or("");
                                td : row.round.as_deref().unwrap_or("");
                                td : row.start.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or("Unscheduled".to_owned());
                                td {
                                    @if let Some(ref room) = row.room {
                                        a(href = room) : room;
                                    }
                                }
                                @if pooled_config.is_some() { td : row.mode_name.as_deref().unwrap_or("Unassigned");
                                }
                                @if !is_started {
                                    td {
                                        a(class = "button", href = uri!(get_edit(event.series, &*event.event, row.id)).to_string()) : "Edit";
                                        : " | ";
                                        form(action = uri!(delete_race(event.series, &*event.event, row.id)).to_string(), method = "post", style = "display: inline;") {
                                            input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                                            button(type = "submit", onclick = "return confirm('Are you sure you want to delete this qualifier race? This will also delete all volunteer signups for this race.')") : "Delete";
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            @if is_started {
                p : "The event has started. New qualifier races cannot be created.";
            } else {
                h3 : "Create Live Qualifier Race";
                : full_form(uri!(post_race(event.series, &*event.event)), csrf, html! {
                    : form_field("race_phase", &mut ctx.errors().collect_vec(), html! {
                        label(for = "race_phase") : "Phase display name";
                        input(type = "text", name = "race_phase", id = "race_phase", value = ctx.field_value("race_phase").unwrap_or("Qualifier"));
                        label(class = "help") : "This race counts as a qualifier regardless of its display name.";
                    });
                    : form_field("qualifier_number", &mut ctx.errors().collect_vec(), html! {
                        label(for = "qualifier_number") : "Qualifier number";
                        input(type = "number", min = "1", name = "qualifier_number", id = "qualifier_number", value = ctx.field_value("qualifier_number").unwrap_or("1"));
                    });
                    : form_field("race_round", &mut ctx.errors().collect_vec(), html! {
                        label(for = "race_round") : "Round";
                        input(type = "text", name = "race_round", id = "race_round", value = ctx.field_value("race_round").unwrap_or(""), placeholder = "e.g. Live 1");
                    });
                    : form_field("race_start", &mut ctx.errors().collect_vec(), html! {
                        label(for = "race_start") : "Start Time (UTC)";
                        input(type = "datetime-local", name = "race_start", id = "race_start", value = ctx.field_value("race_start").unwrap_or(""));
                    });
                    : form_field("race_room", &mut ctx.errors().collect_vec(), html! {
                        label(for = "race_room") : "Racetime.gg Room URL (optional)";
                        input(type = "text", name = "race_room", id = "race_room", value = ctx.field_value("race_room").unwrap_or(""), placeholder = "https://racetime.gg/...", style = "width: 100%; max-width: 600px;");
                    });
                    @if pooled_config.is_some() {
                        label(for = "qualifier_mode_id") : "Qualifier mode";
                        select(name = "qualifier_mode_id", id = "qualifier_mode_id", required) {
                            option(value = "") : "Select a mode";
                            @for mode in pooled_modes.iter().filter(|mode| mode.enabled) {
                                option(value = mode.id) : &mode.display_name;
                            }
                        }
                    }
                }, ctx.errors().collect_vec(), "Create Race");
            }
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/qualifiers")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    Ok(qualifiers_form(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        event_data,
        is_started,
        Context::default(),
    )
    .await?)
}

fn optional_utc(value: &str) -> Result<Option<DateTime<Utc>>, ()> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%dT%H:%M")
            .map(|value| Some(DateTime::<Utc>::from_naive_utc_and_offset(value, Utc)))
            .map_err(drop)
    }
}

async fn require_organizer(
    transaction: &mut Transaction<'_, Postgres>,
    me: &User,
    series: Series,
    event: &str,
) -> Result<(), StatusOrError<event::Error>> {
    let data = Data::new(transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !data.organizers(transaction).await?.contains(me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    if data.qualifier_mode != "pooled_by_mode" {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    Ok(())
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct PooledConfigForm {
    #[field(default = String::new())]
    csrf: String,
    required_mode_count: i16,
    pool_seed_count: i16,
    live_races_per_mode: i16,
    #[field(default = String::new())]
    requests_open_at: String,
    #[field(default = String::new())]
    requests_close_at: String,
    #[field(default = String::new())]
    starts_close_at: String,
    #[field(default = String::new())]
    submissions_close_at: String,
    #[field(default = String::new())]
    retries_close_at: String,
    #[field(default = String::new())]
    results_release_at: String,
    async_run_limit_hours: i64,
    live_entry_close_minutes: i64,
    retry_limit: i16,
    allocation_spread: i16,
    par_finishers: i16,
    score_scale: f64,
    score_offset: f64,
    score_minimum: f64,
    score_maximum: f64,
    requests_paused: bool,
}

#[rocket::post("/event/<series>/<event>/qualifiers/pooled-config", data = "<form>")]
pub(crate) async fn post_pooled_config(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    uri: Origin<'_>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, PooledConfigForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let _ = require_organizer(&mut transaction, &me, series, event).await?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
    let value = form
        .value
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    if value.required_mode_count <= 0
        || value.pool_seed_count <= 0
        || value.live_races_per_mode < 0
        || value.async_run_limit_hours <= 0
        || value.live_entry_close_minutes < 0
        || !(0..=1).contains(&value.retry_limit)
        || value.allocation_spread < 1
        || value.par_finishers < 1
        || [
            value.score_scale,
            value.score_offset,
            value.score_minimum,
            value.score_maximum,
        ]
        .iter()
        .any(|value| !value.is_finite())
        || value.score_scale <= 0.0
        || value.score_maximum < value.score_minimum
    {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let dates = [
        optional_utc(&value.requests_open_at),
        optional_utc(&value.requests_close_at),
        optional_utc(&value.starts_close_at),
        optional_utc(&value.submissions_close_at),
        optional_utc(&value.retries_close_at),
        optional_utc(&value.results_release_at),
    ];
    let [
        Ok(requests_open),
        Ok(requests_close),
        Ok(starts_close),
        Ok(submissions_close),
        Ok(retries_close),
        Ok(results_release),
    ] = dates
    else {
        return Err(StatusOrError::Status(Status::BadRequest));
    };
    let old = pooled_qualifiers::Config::load(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::Conflict))?;
    let structural_changed = old.required_mode_count != value.required_mode_count
        || old.pool_seed_count != value.pool_seed_count
        || old.live_races_per_mode != value.live_races_per_mode
        || old.retry_limit != value.retry_limit
        || old.allocation_spread != value.allocation_spread
        || old.par_finishers != value.par_finishers
        || old.score_scale != value.score_scale
        || old.score_offset != value.score_offset
        || old.score_minimum != value.score_minimum
        || old.score_maximum != value.score_maximum
        || old.run_limit().num_hours() != value.async_run_limit_hours
        || i64::from(old.live_entry_close_lead.days) * 1440
            + old.live_entry_close_lead.microseconds / 60_000_000
            != value.live_entry_close_minutes;
    if old.settings_locked_at.is_some() && structural_changed {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    sqlx::query(
        r#"UPDATE pooled_qualifier_configs SET
        required_mode_count = $3, pool_seed_count = $4, live_races_per_mode = $5,
        requests_open_at = $6, requests_close_at = $7, starts_close_at = $8,
        submissions_close_at = $9, retries_close_at = $10, results_release_at = $11,
        async_run_limit = make_interval(hours => $12::INT),
        live_entry_close_lead = make_interval(mins => $13::INT), retry_limit = $14,
        allocation_spread = $15, par_finishers = $16, score_scale = $17,
        score_offset = $18, score_minimum = $19, score_maximum = $20,
        requests_paused = $21, updated_at = NOW()
        WHERE series = $1 AND event = $2"#,
    )
    .bind(series)
    .bind(event)
    .bind(value.required_mode_count)
    .bind(value.pool_seed_count)
    .bind(value.live_races_per_mode)
    .bind(requests_open)
    .bind(requests_close)
    .bind(starts_close)
    .bind(submissions_close)
    .bind(retries_close)
    .bind(results_release)
    .bind(
        i32::try_from(value.async_run_limit_hours)
            .map_err(|_| StatusOrError::Status(Status::BadRequest))?,
    )
    .bind(
        i32::try_from(value.live_entry_close_minutes)
            .map_err(|_| StatusOrError::Status(Status::BadRequest))?,
    )
    .bind(value.retry_limit)
    .bind(value.allocation_spread)
    .bind(value.par_finishers)
    .bind(value.score_scale)
    .bind(value.score_offset)
    .bind(value.score_minimum)
    .bind(value.score_maximum)
    .bind(value.requests_paused)
    .execute(&mut *transaction)
    .await?;
    if !value.requests_paused {
        let errors = pooled_qualifiers::readiness(&mut transaction, series, event).await?;
        if !errors.is_empty() {
            transaction.rollback().await?;
            let mut transaction = pool.begin().await?;
            let data = Data::new(&mut transaction, series, event)
                .await?
                .ok_or(StatusOrError::Status(Status::NotFound))?;
            for error in errors {
                form.context.push_error(form::Error::validation(error));
            }
            return Ok(RedirectOrContent::Content(
                qualifiers_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    data,
                    false,
                    form.context,
                )
                .await?,
            ));
        }
    }
    transaction.commit().await?;
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(
        series, event
    )))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct PooledModeForm {
    #[field(default = String::new())]
    csrf: String,
    mode_id: i64,
    position: i16,
    slug: String,
    display_name: String,
    seed_gen_type: String,
    seed_config: String,
    generator_profile: String,
    enabled: bool,
}

#[rocket::post("/event/<series>/<event>/qualifiers/pooled-mode", data = "<form>")]
pub(crate) async fn post_pooled_mode(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, PooledModeForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let _ = require_organizer(&mut transaction, &me, series, event).await?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
    let value = form
        .value
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    let seed_config: serde_json::Value = serde_json::from_str(&value.seed_config)?;
    if value.position <= 0
        || value.slug.is_empty()
        || value.display_name.trim().is_empty()
        || value.generator_profile.trim().is_empty()
        || !seed_config.is_object()
        || racetime_bot::seed_gen_type::SeedGenType::from_db(
            Some(&value.seed_gen_type),
            Some(&seed_config),
        )
        .is_none()
    {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let config = pooled_qualifiers::Config::load(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::Conflict))?;
    let existing: Option<(String, serde_json::Value, String, String, bool)> = if value.mode_id == 0
    {
        None
    } else {
        sqlx::query_as("SELECT seed_gen_type, seed_config, generator_profile, slug, enabled FROM qualifier_modes WHERE id = $1 AND series = $2 AND event = $3 FOR UPDATE")
            .bind(value.mode_id).bind(series).bind(event).fetch_optional(&mut *transaction).await?
    };
    let material_changed = existing.as_ref().is_none_or(|old| {
        old.0 != value.seed_gen_type
            || old.1 != seed_config
            || old.2 != value.generator_profile
            || old.3 != value.slug
            || old.4 != value.enabled
    });
    if (config.settings_locked_at.is_some() || !config.requests_paused) && material_changed {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    if value.mode_id != 0 && material_changed {
        let has_seed_material: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM qualifier_seeds seed
            WHERE seed.mode_id = $1 AND (
                seed.generation_state <> 'pending' OR seed.seed_data IS NOT NULL
                OR seed.released_at IS NOT NULL OR seed.entry_closed_at IS NOT NULL
                OR EXISTS(SELECT 1 FROM qualifier_attempts attempt WHERE attempt.seed_id = seed.id)
            ))"#,
        )
        .bind(value.mode_id)
        .fetch_one(&mut *transaction)
        .await?;
        if has_seed_material {
            return Err(StatusOrError::Status(Status::Conflict));
        }
    }
    let fingerprint = format!(
        "{}:{}:{}",
        value.seed_gen_type, value.generator_profile, seed_config
    );
    if value.mode_id == 0 {
        sqlx::query(r#"INSERT INTO qualifier_modes(series, event, position, slug,
            display_name, seed_gen_type, seed_config, generator_profile, settings_fingerprint, enabled)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#)
            .bind(series).bind(event).bind(value.position).bind(&value.slug)
            .bind(value.display_name.trim()).bind(&value.seed_gen_type).bind(seed_config)
            .bind(value.generator_profile.trim()).bind(fingerprint).bind(value.enabled)
            .execute(&mut *transaction).await?;
    } else {
        let updated = sqlx::query(r#"UPDATE qualifier_modes SET position=$2, slug=$3,
            display_name=$4, seed_gen_type=$5, seed_config=$6, generator_profile=$7,
            settings_fingerprint=$8, enabled=$9, updated_at=NOW() WHERE id=$1 AND series=$10 AND event=$11"#)
            .bind(value.mode_id).bind(value.position).bind(&value.slug).bind(value.display_name.trim())
            .bind(&value.seed_gen_type).bind(seed_config).bind(value.generator_profile.trim())
            .bind(&fingerprint).bind(value.enabled).bind(series).bind(event)
            .execute(&mut *transaction).await?;
        if updated.rows_affected() != 1 {
            return Err(StatusOrError::Status(Status::NotFound));
        }
        sqlx::query(
            r#"UPDATE qualifier_seeds SET generator_profile = $2,
                settings_fingerprint = $3
            WHERE mode_id = $1 AND generation_state = 'pending'
              AND seed_data IS NULL AND released_at IS NULL AND entry_closed_at IS NULL
              AND NOT EXISTS(SELECT 1 FROM qualifier_attempts attempt
                  WHERE attempt.seed_id = qualifier_seeds.id)"#,
        )
        .bind(value.mode_id)
        .bind(value.generator_profile.trim())
        .bind(&fingerprint)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct PooledSeedForm {
    #[field(default = String::new())]
    csrf: String,
    mode_id: i64,
    pool_position: i16,
    seed_data: String,
    attest_settings: bool,
}

#[rocket::post("/event/<series>/<event>/qualifiers/pooled-seed", data = "<form>")]
pub(crate) async fn post_pooled_seed(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, PooledSeedForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let _ = require_organizer(&mut transaction, &me, series, event).await?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
    let value = form
        .value
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    let seed_data: serde_json::Value = serde_json::from_str(&value.seed_data)?;
    let physical_identity = pooled_qualifiers::physical_seed_identity(&seed_data)
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    if value.pool_position <= 0 || !value.attest_settings {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let config = pooled_qualifiers::Config::load(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::Conflict))?;
    if !config.requests_paused || value.pool_position > config.pool_seed_count {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    let configured_mode = pooled_qualifiers::Mode::for_event(&mut transaction, series, event)
        .await?
        .into_iter()
        .find(|mode| mode.id == value.mode_id && mode.enabled)
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let kind = racetime_bot::seed_gen_type::SeedGenType::from_db(
        Some(&configured_mode.seed_gen_type),
        Some(&configured_mode.seed_config),
    )
    .ok_or(StatusOrError::Status(Status::BadRequest))?;
    pooled_qualifiers::generation::validate_payload(&kind, &seed_data)
        .await
        .map_err(|error| event::Error::Sql(sqlx::Error::Protocol(error.to_string())))?;
    let mode: Option<(String, String)> = sqlx::query_as(
        "SELECT generator_profile, settings_fingerprint FROM qualifier_modes WHERE id=$1 AND series=$2 AND event=$3 AND enabled"
    ).bind(value.mode_id).bind(series).bind(event).fetch_optional(&mut *transaction).await?;
    let (profile, fingerprint) = mode.ok_or(StatusOrError::Status(Status::NotFound))?;
    let result = sqlx::query(
        r#"INSERT INTO qualifier_seeds(series,event,mode_id,source,
        pool_position,generation_state,seed_data,generator_profile,physical_seed_identity,
        settings_fingerprint,generated_at,settings_attested_by,settings_attested_at)
        VALUES($1,$2,$3,'async_pool',$4,'ready',$5,$6,$7,$8,NOW(),$9,NOW())
        ON CONFLICT(mode_id,pool_position) DO UPDATE SET generation_state='ready',
          seed_data=EXCLUDED.seed_data,generator_profile=EXCLUDED.generator_profile,
          physical_seed_identity=EXCLUDED.physical_seed_identity,
          settings_fingerprint=EXCLUDED.settings_fingerprint,generated_at=NOW(),
          settings_attested_by=EXCLUDED.settings_attested_by,settings_attested_at=NOW()
        WHERE qualifier_seeds.released_at IS NULL AND qualifier_seeds.generation_state <> 'generating' AND NOT EXISTS(
          SELECT 1 FROM qualifier_attempts WHERE seed_id=qualifier_seeds.id)"#,
    )
    .bind(series)
    .bind(event)
    .bind(value.mode_id)
    .bind(value.pool_position)
    .bind(seed_data)
    .bind(profile)
    .bind(physical_identity)
    .bind(fingerprint)
    .bind(i64::from(me.id))
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    transaction.commit().await?;
    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct PooledGenerateForm {
    #[field(default = String::new())]
    csrf: String,
    mode_id: i64,
    pool_position: Option<i16>,
    retry_failed: bool,
}

#[rocket::post("/event/<series>/<event>/qualifiers/pooled-generate", data = "<form>")]
pub(crate) async fn post_pooled_generate(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, PooledGenerateForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let value = form
        .value
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    let mut tx = pool.begin().await?;
    require_organizer(&mut tx, &me, series, event).await?;
    pooled_qualifiers::generation::enqueue(
        &mut tx,
        series,
        event,
        value.mode_id,
        value.pool_position,
        value.retry_failed,
    )
    .await
    .map_err(|error| event::Error::Sql(sqlx::Error::Protocol(error.to_string())))?;
    tx.commit().await?;
    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct PooledResultForm {
    #[field(default = String::new())]
    csrf: String,
    attempt_id: i64,
    control_version: i64,
    action: String,
    reason: String,
    #[field(default = String::new())]
    outcome: String,
    #[field(default = String::new())]
    finish_time: String,
    #[field(default = String::new())]
    vod: String,
}

#[rocket::post("/event/<series>/<event>/qualifiers/pooled-result", data = "<form>")]
pub(crate) async fn post_pooled_result(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, PooledResultForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let value = form
        .value
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    let mut tx = pool.begin().await?;
    require_organizer(&mut tx, &me, series, event).await?;
    pooled_qualifiers::lock_event(&mut tx, series, event).await?;
    let belongs: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qualifier_attempts WHERE id = $1 AND series = $2 AND event = $3)")
        .bind(value.attempt_id).bind(series).bind(event).fetch_one(&mut *tx).await?;
    if !belongs {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    let result = match value.action.as_str() {
        "result" => {
            let outcome = match value.outcome.as_str() {
                "finished" => {
                    let parts: Option<Vec<u64>> = value
                        .finish_time
                        .split(':')
                        .map(|part| part.parse().ok())
                        .collect();
                    let parts = parts.ok_or(StatusOrError::Status(Status::BadRequest))?;
                    let [hours, minutes, seconds] = parts.as_slice() else {
                        return Err(StatusOrError::Status(Status::BadRequest));
                    };
                    if *minutes >= 60 || *seconds >= 60 || *hours > 10_000 {
                        return Err(StatusOrError::Status(Status::BadRequest));
                    }
                    pooled_qualifiers::Outcome::Finished(Duration::from_secs(
                        hours * 3600 + minutes * 60 + seconds,
                    ))
                }
                "forfeit" => pooled_qualifiers::Outcome::Forfeit,
                "dq" => pooled_qualifiers::Outcome::Dq,
                "invalid" => pooled_qualifiers::Outcome::Invalid,
                _ => return Err(StatusOrError::Status(Status::BadRequest)),
            };
            pooled_qualifiers::correct_result(
                &mut tx,
                value.attempt_id,
                value.control_version,
                me.id.into(),
                &value.reason,
                outcome,
                (!value.vod.trim().is_empty()).then_some(value.vod.trim()),
            )
            .await
        }
        "disclosure" | "reverse_disclosure" => {
            pooled_qualifiers::disclosure(
                &mut tx,
                value.attempt_id,
                value.control_version,
                me.id.into(),
                &value.reason,
                value.action == "reverse_disclosure",
            )
            .await
        }
        _ => return Err(StatusOrError::Status(Status::BadRequest)),
    };
    result.map_err(|error| {
        StatusOrError::Err(event::Error::Sql(sqlx::Error::Protocol(error.to_string())))
    })?;
    tx.commit().await?;
    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct PooledRecoveryForm {
    #[field(default = String::new())]
    csrf: String,
    attempt_id: i64,
    control_version: i64,
    action: String,
    reason: String,
    discord_id: Option<u64>,
}

#[rocket::post("/event/<series>/<event>/qualifiers/pooled-recover", data = "<form>")]
pub(crate) async fn post_pooled_recover(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, PooledRecoveryForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let value = form
        .value
        .ok_or(StatusOrError::Status(Status::BadRequest))?;
    if value.reason.trim().is_empty() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let mut tx = pool.begin().await?;
    require_organizer(&mut tx, &me, series, event).await?;
    let row: Option<(String, Option<i64>, i64)> = sqlx::query_as(r#"SELECT attempt.state, attempt.discord_thread, event.discord_async_channel
        FROM qualifier_attempts attempt JOIN events event USING (series, event)
        WHERE attempt.id=$1 AND attempt.control_version=$2 AND attempt.series=$3 AND attempt.event=$4 AND attempt.source='async'"#)
        .bind(value.attempt_id).bind(value.control_version).bind(series).bind(event).fetch_optional(&mut *tx).await?;
    let (state, thread, parent) = row.ok_or(StatusOrError::Status(Status::Conflict))?;
    tx.commit().await?;
    // Validate external references without holding the event lock.
    let (key, message_id, adopted_thread) = match value.action.as_str() {
        "ready" if state == "assigned" && thread.is_some() => ("ready".to_owned(), None, None),
        "seed" if state == "revealed" && thread.is_some() => ("seed".to_owned(), None, None),
        "thread" if state == "assigned" && thread.is_none() => {
            let id = value
                .discord_id
                .filter(|id| *id != 0)
                .ok_or(StatusOrError::Status(Status::BadRequest))?;
            let ctx = discord_ctx.read().await;
            let channel = ChannelId::new(id)
                .to_channel(&*ctx)
                .await?
                .guild()
                .ok_or(StatusOrError::Status(Status::BadRequest))?;
            let bot = ctx.http.get_current_user().await?;
            if channel.parent_id != Some(ChannelId::new(parent as u64))
                || channel.kind != ChannelType::PrivateThread
                || channel.owner_id != Some(bot.id)
                || channel.name != format!("qualifier-{}", value.attempt_id)
            {
                return Err(StatusOrError::Status(Status::BadRequest));
            }
            ("thread".to_owned(), Some(id), Some(id as i64))
        }
        "go" if state == "starting" => {
            let thread = thread.ok_or(StatusOrError::Status(Status::BadRequest))?;
            let id = value
                .discord_id
                .filter(|id| *id != 0)
                .ok_or(StatusOrError::Status(Status::BadRequest))?;
            let ctx = discord_ctx.read().await;
            let message = ChannelId::new(thread as u64)
                .message(&*ctx, MessageId::new(id))
                .await?;
            let bot = ctx.http.get_current_user().await?;
            let key = format!("go-{}", value.control_version);
            if message.author.id != bot.id
                || !message
                    .content
                    .ends_with(&format!("[qualifier:{}:{key}]", value.attempt_id))
            {
                return Err(StatusOrError::Status(Status::BadRequest));
            }
            (key, Some(id), None)
        }
        _ => return Err(StatusOrError::Status(Status::BadRequest)),
    };
    let mut tx = pool.begin().await?;
    pooled_qualifiers::lock_event(&mut tx, series, event).await?;
    let updated = sqlx::query(r#"UPDATE qualifier_attempts SET
        discord_thread = COALESCE($6, discord_thread),
        delivery_messages = CASE WHEN $5::JSONB IS NULL THEN delivery_messages - $4 ELSE jsonb_set(delivery_messages, ARRAY[$4], $5) END,
        delivery_error=NULL, delivery_claim=NULL, delivery_claim_until=NULL,
        correction_history = correction_history || jsonb_build_array(jsonb_build_object('action','delivery_recovery','operation',$4::TEXT,'actor',$7::BIGINT,'at',NOW(),'reason',$8::TEXT,'message',$5::JSONB))
        WHERE id=$1 AND control_version=$2 AND state=$3 AND (delivery_claim IS NULL OR delivery_claim_until < NOW())"#)
        .bind(value.attempt_id).bind(value.control_version).bind(state).bind(key).bind(message_id.map(|id| serde_json::json!(id)))
        .bind(adopted_thread).bind(i64::from(me.id)).bind(value.reason.trim()).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(StatusOrError::Status(Status::Conflict));
    }
    tx.commit().await?;
    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RaceForm {
    #[field(default = String::new())]
    csrf: String,
    race_round: String,
    race_phase: String,
    #[field(validate = range(1..))]
    qualifier_number: i64,
    race_start: String,
    #[field(default = None)]
    race_room: Option<String>,
    qualifier_mode_id: Option<i64>,
}

#[rocket::post("/event/<series>/<event>/qualifiers/create-race", data = "<form>")]
pub(crate) async fn post_race(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    http_client: &State<reqwest::Client>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, RaceForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    if is_started {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                qualifiers_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    event_data,
                    false,
                    form.context,
                )
                .await?,
            )
        } else {
            let start = match NaiveDateTime::parse_from_str(&value.race_start, "%Y-%m-%dT%H:%M") {
                Ok(naive_dt) => DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(_) => {
                    form.context.push_error(
                        form::Error::validation("Invalid start time format")
                            .with_name("race_start"),
                    );
                    return Ok(RedirectOrContent::Content(
                        qualifiers_form(
                            transaction,
                            me,
                            uri,
                            csrf.as_ref(),
                            event_data,
                            false,
                            form.context,
                        )
                        .await?,
                    ));
                }
            };

            let room = if let Some(ref room_str) = value.race_room {
                let trimmed = room_str.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Url>() {
                        Ok(url) => Some(url),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid room URL").with_name("race_room"),
                            );
                            return Ok(RedirectOrContent::Content(
                                qualifiers_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    false,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if event_data.qualifier_mode == "pooled_by_mode" {
                let valid_mode = if let Some(mode_id) = value.qualifier_mode_id {
                    sqlx::query_scalar::<_, bool>(
                        r#"SELECT EXISTS(SELECT 1 FROM qualifier_modes
                        WHERE id = $1 AND series = $2 AND event = $3 AND enabled)"#,
                    )
                    .bind(mode_id)
                    .bind(series)
                    .bind(event)
                    .fetch_one(&mut *transaction)
                    .await?
                } else {
                    false
                };
                if !valid_mode {
                    form.context.push_error(
                        form::Error::validation("Select an enabled qualifier mode.")
                            .with_name("qualifier_mode_id"),
                    );
                    return Ok(RedirectOrContent::Content(
                        qualifiers_form(
                            transaction,
                            me,
                            uri,
                            csrf.as_ref(),
                            event_data,
                            false,
                            form.context,
                        )
                        .await?,
                    ));
                }
            }

            let mut race = Race {
                is_qualifier: true,
                qualifier_number: Some(value.qualifier_number),
                id: Id::<Races>::new(&mut transaction).await?,
                series: event_data.series,
                event: event_data.event.to_string(),
                source: Source::Manual,
                entrants: Entrants::Open,
                phase: Some(value.race_phase.clone()),
                round: Some(value.race_round.clone()),
                game: None,
                scheduling_thread: None,
                schedule: RaceSchedule::Live {
                    start,
                    end: None,
                    room,
                },
                schedule_updated_at: Some(Utc::now()),
                fpa_invoked: false,
                breaks_used: false,
                draft: None,
                seed: seed::Data::default(),
                video_urls: HashMap::default(),
                restreamers: HashMap::default(),
                last_edited_by: Some(me.id),
                last_edited_at: Some(Utc::now()),
                ignored: false,
                schedule_locked: false,
                notified: false,
                async_notified_1: false,
                async_notified_2: false,
                async_notified_3: false,
                discord_scheduled_event_id: None,
                volunteer_request_sent: false,
                volunteer_request_message_id: None,
                racetime_goal_slug: event_data.racetime_goal_slug.clone(),
                scheduling_deadline: None,
                restream_consent_required: false,
                custom_title: None,
                custom_create_room: true,
                companion_race_id: None,
            };
            if event_data.qualifier_mode == "pooled_by_mode" {
                pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
                let config = pooled_qualifiers::Config::load(&mut transaction, series, event)
                    .await?
                    .ok_or(StatusOrError::Status(Status::Conflict))?;
                if !config.requests_paused {
                    return Err(StatusOrError::Status(Status::Conflict));
                }
            }
            race.save(&mut transaction).await?;
            if let Some(mode_id) = value
                .qualifier_mode_id
                .filter(|_| event_data.qualifier_mode == "pooled_by_mode")
            {
                sqlx::query(r#"INSERT INTO qualifier_seeds
                    (series, event, mode_id, source, live_race_id, generator_profile, settings_fingerprint)
                    SELECT series, event, id, 'live', $2, generator_profile, settings_fingerprint
                    FROM qualifier_modes WHERE id = $1"#)
                    .bind(mode_id).bind(i64::from(race.id))
                    .execute(&mut *transaction).await?;
            }
            if event_data.qualifier_mode == "pooled_by_mode" {
                transaction.commit().await?;
                transaction = pool.begin().await?;
            }
            match crate::discord_scheduled_events::create_discord_scheduled_event(
                &*discord_ctx.read().await,
                &mut transaction,
                &mut race,
                &event_data,
                http_client.inner(),
            )
            .await
            {
                Ok(()) => {
                    race.save(&mut transaction).await?;
                }
                Err(e) => {
                    eprintln!(
                        "Failed to create Discord scheduled event for qualifier race {}: {}",
                        race.id, e
                    );
                }
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            qualifiers_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                false,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DeleteForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/qualifiers/<race_id>/delete", data = "<form>")]
pub(crate) async fn delete_race(
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    http_client: &State<reqwest::Client>,
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, DeleteForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    if is_started {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    if form.value.is_some() {
        let mut notifications = Vec::new();
        // Check if race has teams assigned
        let has_teams = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM races WHERE id = $1 AND (team1 IS NOT NULL OR team2 IS NOT NULL))",
            race_id as _
        )
        .fetch_one(&mut *transaction)
        .await?
        .unwrap_or(false);

        if has_teams {
            return Err(StatusOrError::Status(Status::Conflict));
        }

        // Send cancel DMs to confirmed volunteers before cascade delete
        if let Ok(race) = Race::from_id(&mut transaction, http_client, race_id).await {
            if let Ok(description) = race.notification_description(&mut transaction).await {
                let signups = event::roles::Signup::for_race(&mut transaction, race_id)
                    .await
                    .unwrap_or_default();
                for signup in signups
                    .iter()
                    .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Confirmed))
                {
                    if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                        if let Some(discord) = user.discord {
                            let discord_user_id = UserId::new(discord.id.get());
                            let mut msg = MessageBuilder::default();
                            msg.push("**Race Canceled**\n\nThe race ");
                            msg.push_mono(&description);
                            msg.push(" in ");
                            msg.push(&event_data.display_name);
                            msg.push(
                                " has been canceled and your volunteer signup has been removed.",
                            );
                            notifications.push((discord_user_id, msg.build()));
                        }
                    }
                }
            }
        }

        if event_data.qualifier_mode == "pooled_by_mode" {
            pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
            let config = pooled_qualifiers::Config::load(&mut transaction, series, event)
                .await?
                .ok_or(StatusOrError::Status(Status::Conflict))?;
            if !config.requests_paused {
                return Err(StatusOrError::Status(Status::Conflict));
            }
        }
        // A never-used pooled live cohort can be removed with its race. Frozen
        // eligibility or an attempt is historical data and makes deletion unsafe.
        sqlx::query(
            r#"DELETE FROM qualifier_seeds seed WHERE live_race_id = $1
            AND released_at IS NULL
            AND NOT EXISTS(SELECT 1 FROM qualifier_attempts WHERE seed_id = seed.id)
            AND NOT EXISTS(SELECT 1 FROM qualifier_live_entries WHERE seed_id = seed.id)"#,
        )
        .bind(i64::from(race_id))
        .execute(&mut *transaction)
        .await?;
        if sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM qualifier_seeds WHERE live_race_id = $1)",
        )
        .bind(i64::from(race_id))
        .fetch_one(&mut *transaction)
        .await?
        {
            return Err(StatusOrError::Status(Status::Conflict));
        }

        // Delete the race (signups will cascade delete automatically)
        sqlx::query!(
            "DELETE FROM races WHERE id = $1 AND series = $2 AND event = $3 AND is_qualifier",
            race_id as _,
            series as _,
            event
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        let discord_ctx = discord_ctx.read().await;
        for (user, message) in notifications {
            if let Ok(dm) = user.create_dm_channel(&*discord_ctx).await {
                let _ = dm.say(&*discord_ctx, message).await;
            }
        }
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct QualifierSettingsForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    qualifier_score_hiding: String,
    automated_asyncs: bool,
}

#[rocket::post("/event/<series>/<event>/qualifiers/settings", data = "<form>")]
pub(crate) async fn post_settings(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, QualifierSettingsForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    if event_data.qualifier_mode == "pooled_by_mode" {
        pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
        let config = pooled_qualifiers::Config::load(&mut transaction, series, event)
            .await?
            .ok_or(StatusOrError::Status(Status::Conflict))?;
        if !config.requests_paused
            && form
                .value
                .as_ref()
                .is_some_and(|value| !value.automated_asyncs)
        {
            return Err(StatusOrError::Status(Status::Conflict));
        }
    }
    Ok(if let Some(ref value) = form.value {
        let qualifier_score_hiding = match value.qualifier_score_hiding.as_str() {
            "none" | "" => QualifierScoreHiding::None,
            "async_only" => QualifierScoreHiding::AsyncOnly,
            "full_points" => QualifierScoreHiding::FullPoints,
            "full_points_counts" => QualifierScoreHiding::FullPointsCounts,
            "full_complete" => QualifierScoreHiding::FullComplete,
            _ => {
                form.context.push_error(
                    form::Error::validation("Invalid qualifier score hiding value")
                        .with_name("qualifier_score_hiding"),
                );
                let is_started = event_data.is_started(&mut transaction).await?;
                return Ok(RedirectOrContent::Content(
                    qualifiers_form(
                        transaction,
                        me,
                        uri,
                        csrf.as_ref(),
                        event_data,
                        is_started,
                        form.context,
                    )
                    .await?,
                ));
            }
        };

        sqlx::query!(
            "UPDATE events SET qualifier_score_hiding = $1, automated_asyncs = $2 WHERE series = $3 AND event = $4",
            qualifier_score_hiding as _, value.automated_asyncs, series as _, event
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
    } else {
        let is_started = event_data.is_started(&mut transaction).await?;
        RedirectOrContent::Content(
            qualifiers_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                is_started,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct NotificationRoleForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = None)]
    notification_role_id: Option<String>,
}

#[rocket::post(
    "/event/<series>/<event>/qualifiers/notification-role",
    data = "<form>"
)]
pub(crate) async fn post_notification_role(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, NotificationRoleForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        let role_id = value.notification_role_id.as_ref().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                match trimmed.parse::<u64>() {
                    Ok(id) => Some(id as i64),
                    Err(_) => {
                        form.context.push_error(
                            form::Error::validation("Invalid Discord role ID. Must be a number.")
                                .with_name("notification_role_id"),
                        );
                        None
                    }
                }
            }
        });

        if form.context.errors().next().is_some() {
            let is_started = event_data.is_started(&mut transaction).await?;
            return Ok(RedirectOrContent::Content(
                qualifiers_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    event_data,
                    is_started,
                    form.context,
                )
                .await?,
            ));
        }

        sqlx::query!(
            "UPDATE events SET qualifier_notification_role_id = $1 WHERE series = $2 AND event = $3",
            role_id, series as _, event
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
    } else {
        let is_started = event_data.is_started(&mut transaction).await?;
        RedirectOrContent::Content(
            qualifiers_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                is_started,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DisableNotificationRoleForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post(
    "/event/<series>/<event>/qualifiers/notification-role/disable",
    data = "<form>"
)]
pub(crate) async fn delete_notification_role(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, DisableNotificationRoleForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.value.is_some() {
        sqlx::query!(
            "UPDATE events SET qualifier_notification_role_id = NULL WHERE series = $1 AND event = $2",
            series as _, event
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

async fn edit_race_form(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    event: Data<'_>,
    race_id: Id<Races>,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    let header = event
        .header(&mut transaction, Some(&me), Tab::Qualifiers, false)
        .await?;

    struct RaceData {
        phase: Option<String>,
        qualifier_number: Option<i64>,
        round: Option<String>,
        start: Option<DateTime<Utc>>,
        room: Option<String>,
        mode_id: Option<i64>,
    }
    let race = sqlx::query_as!(RaceData,
        r#"SELECT race.phase, race.qualifier_number, race.round, race.start, race.room,
            seed.mode_id FROM races race LEFT JOIN qualifier_seeds seed ON seed.live_race_id = race.id
            WHERE race.id = $1 AND race.series = $2 AND race.event = $3 AND race.is_qualifier"#,
        race_id as _, event.series as _, &event.event
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let race = match race {
        Some(r) => r,
        None => return Err(event::Error::Sql(sqlx::Error::RowNotFound)),
    };

    let start_formatted = race
        .start
        .map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default();
    let modes = if event.qualifier_mode == "pooled_by_mode" {
        pooled_qualifiers::Mode::for_event(&mut transaction, event.series, &event.event).await?
    } else {
        Vec::new()
    };

    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Edit Qualifier Race — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Edit Live Qualifier Race";
            : full_form(uri!(post_edit_race(event.series, &*event.event, race_id)), csrf, html! {
                : form_field("race_phase", &mut ctx.errors().collect_vec(), html! {
                    label(for = "race_phase") : "Phase display name";
                    input(type = "text", name = "race_phase", id = "race_phase", value = ctx.field_value("race_phase").unwrap_or(race.phase.as_deref().unwrap_or("")));
                    label(class = "help") : "This race counts as a qualifier regardless of its display name.";
                });
                : form_field("qualifier_number", &mut ctx.errors().collect_vec(), html! {
                    label(for = "qualifier_number") : "Qualifier number";
                    input(type = "number", min = "1", name = "qualifier_number", id = "qualifier_number", value = ctx.field_value("qualifier_number").unwrap_or(&race.qualifier_number.unwrap_or(1).to_string()));
                });
                : form_field("race_round", &mut ctx.errors().collect_vec(), html! {
                    label(for = "race_round") : "Round";
                    input(type = "text", name = "race_round", id = "race_round", value = ctx.field_value("race_round").unwrap_or(&race.round.unwrap_or_default()), placeholder = "e.g. Live 1");
                });
                : form_field("race_start", &mut ctx.errors().collect_vec(), html! {
                    label(for = "race_start") : "Start Time (UTC)";
                    input(type = "datetime-local", name = "race_start", id = "race_start", value = ctx.field_value("race_start").unwrap_or(&start_formatted));
                });
                : form_field("race_room", &mut ctx.errors().collect_vec(), html! {
                    label(for = "race_room") : "Racetime.gg Room URL (optional)";
                    input(type = "text", name = "race_room", id = "race_room", value = ctx.field_value("race_room").unwrap_or(&race.room.unwrap_or_default()), placeholder = "https://racetime.gg/...", style = "width: 100%; max-width: 600px;");
                });
                @if event.qualifier_mode == "pooled_by_mode" {
                    label(for = "qualifier_mode_id") : "Qualifier mode";
                    select(name = "qualifier_mode_id", id = "qualifier_mode_id", required) {
                        option(value = "") : "Select a mode";
                        @for mode in modes.iter().filter(|mode| mode.enabled || Some(mode.id) == race.mode_id) {
                            option(value = mode.id, selected? = Some(mode.id) == race.mode_id) : &mode.display_name;
                        }
                    }
                }
            }, ctx.errors().collect_vec(), "Update Race");
            p {
                a(href = uri!(get(event.series, &*event.event)).to_string()) : "Cancel";
            }
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/qualifiers/<race_id>/edit")]
pub(crate) async fn get_edit(
    pool: &State<PgPool>,
    _discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
    race_id: Id<Races>,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    if is_started {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    Ok(edit_race_form(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        event_data,
        race_id,
        Context::default(),
    )
    .await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditRaceForm {
    #[field(default = String::new())]
    csrf: String,
    race_round: String,
    race_phase: String,
    #[field(validate = range(1..))]
    qualifier_number: i64,
    race_start: String,
    #[field(default = None)]
    race_room: Option<String>,
    qualifier_mode_id: Option<i64>,
}

#[rocket::post("/event/<series>/<event>/qualifiers/<race_id>/edit", data = "<form>")]
pub(crate) async fn post_edit_race(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    http_client: &State<reqwest::Client>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, EditRaceForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    if is_started {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                edit_race_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    event_data,
                    race_id,
                    form.context,
                )
                .await?,
            )
        } else {
            // Fetch original start time to detect if it changed
            let original_start = sqlx::query_scalar!(
                "SELECT start FROM races WHERE id = $1 AND series = $2 AND event = $3 AND is_qualifier",
                race_id as _, series as _, event
            )
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

            let start = match NaiveDateTime::parse_from_str(&value.race_start, "%Y-%m-%dT%H:%M") {
                Ok(naive_dt) => DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(_) => {
                    form.context.push_error(
                        form::Error::validation("Invalid start time format")
                            .with_name("race_start"),
                    );
                    return Ok(RedirectOrContent::Content(
                        edit_race_form(
                            transaction,
                            me,
                            uri,
                            csrf.as_ref(),
                            event_data,
                            race_id,
                            form.context,
                        )
                        .await?,
                    ));
                }
            };

            let room = if let Some(ref room_str) = value.race_room {
                let trimmed = room_str.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Url>() {
                        Ok(url) => Some(url.to_string()),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid room URL").with_name("race_room"),
                            );
                            return Ok(RedirectOrContent::Content(
                                edit_race_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    race_id,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let time_changed = original_start.is_some() && original_start != Some(start);

            if event_data.qualifier_mode == "pooled_by_mode" {
                pooled_qualifiers::lock_event(&mut transaction, series, event).await?;
                let config = pooled_qualifiers::Config::load(&mut transaction, series, event)
                    .await?
                    .ok_or(StatusOrError::Status(Status::Conflict))?;
                if !config.requests_paused {
                    return Err(StatusOrError::Status(Status::Conflict));
                }

                let Some(mode_id) = value.qualifier_mode_id else {
                    return Err(StatusOrError::Status(Status::BadRequest));
                };
                let mode: Option<(String, String)> = sqlx::query_as(
                    "SELECT generator_profile, settings_fingerprint FROM qualifier_modes WHERE id=$1 AND series=$2 AND event=$3 AND enabled"
                ).bind(mode_id).bind(series).bind(event).fetch_optional(&mut *transaction).await?;
                let (profile, fingerprint) =
                    mode.ok_or(StatusOrError::Status(Status::BadRequest))?;
                let existing: Option<(i64, bool)> = sqlx::query_as(
                    r#"SELECT seed.mode_id,
                    seed.generation_state <> 'pending' OR seed.seed_data IS NOT NULL
                    OR seed.released_at IS NOT NULL OR seed.entry_closed_at IS NOT NULL
                    OR EXISTS(SELECT 1 FROM qualifier_attempts WHERE seed_id=seed.id)
                    FROM qualifier_seeds seed WHERE seed.live_race_id=$1 FOR UPDATE"#,
                )
                .bind(i64::from(race_id))
                .fetch_optional(&mut *transaction)
                .await?;
                match existing {
                    Some((old_mode, true)) if old_mode != mode_id => {
                        return Err(StatusOrError::Status(Status::Conflict));
                    }
                    Some(_) => {
                        sqlx::query("UPDATE qualifier_seeds SET mode_id=$2,generator_profile=$3,settings_fingerprint=$4 WHERE live_race_id=$1")
                            .bind(i64::from(race_id)).bind(mode_id).bind(profile).bind(fingerprint)
                            .execute(&mut *transaction).await?;
                    }
                    None => {
                        sqlx::query(r#"INSERT INTO qualifier_seeds(series,event,mode_id,source,live_race_id,generator_profile,settings_fingerprint)
                            VALUES($1,$2,$3,'live',$4,$5,$6)"#)
                            .bind(series).bind(event).bind(mode_id).bind(i64::from(race_id)).bind(profile).bind(fingerprint)
                            .execute(&mut *transaction).await?;
                    }
                }
            }

            sqlx::query!(
                "UPDATE races SET round = $1, start = $2, room = $3, last_edited_by = $4, last_edited_at = $5, phase = $9, qualifier_number = $10 WHERE id = $6 AND series = $7 AND event = $8 AND is_qualifier",
                value.race_round, start, room, me.id as _, Utc::now(), race_id as _, series as _, event, value.race_phase, value.qualifier_number
            )
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;

            // Update Discord scheduled event
            {
                let mut transaction = pool.begin().await?;
                match Race::from_id(&mut transaction, http_client.inner(), race_id).await {
                    Ok(race) => {
                        if let Err(e) =
                            crate::discord_scheduled_events::update_discord_scheduled_event(
                                &*discord_ctx.read().await,
                                &mut transaction,
                                &race,
                                &event_data,
                                http_client.inner(),
                            )
                            .await
                        {
                            eprintln!(
                                "Failed to update Discord scheduled event for qualifier race {}: {}",
                                race_id, e
                            );
                        }
                        let _ = transaction.commit().await;
                    }
                    Err(e) => eprintln!(
                        "Failed to load race {} for Discord event update: {}",
                        race_id, e
                    ),
                }
            }

            // Update volunteer post if the time changed
            if time_changed {
                use serenity::all::{
                    ButtonStyle, CreateActionRow, CreateButton, CreateMessage, UserId,
                };

                let _ = volunteer_requests::update_volunteer_post_for_race(
                    pool,
                    &*discord_ctx.read().await,
                    race_id,
                )
                .await;

                // Send reschedule notification DMs to volunteers
                let mut transaction = pool.begin().await?;
                if let Ok(signups) = event::roles::Signup::for_race(&mut transaction, race_id).await
                {
                    let affected_signups: Vec<_> = signups
                        .iter()
                        .filter(|s| {
                            matches!(
                                s.status,
                                event::roles::VolunteerSignupStatus::Pending
                                    | event::roles::VolunteerSignupStatus::Confirmed
                            )
                        })
                        .collect();

                    // Build race description for qualifier
                    let race_description = value.race_round.clone();

                    // Send DM to each affected volunteer
                    for signup in affected_signups {
                        if let Ok(Some(user)) =
                            User::from_id(&mut *transaction, signup.user_id).await
                        {
                            if let Some(discord) = user.discord {
                                let discord_user_id = UserId::new(discord.id.get());

                                let mut msg = MessageBuilder::default();
                                msg.push("**Race Rescheduled**\n\n");
                                msg.push("The race ");
                                msg.push_mono(&race_description);
                                msg.push(" in ");
                                msg.push(&event_data.display_name);
                                msg.push(" has been rescheduled.\n\n");
                                msg.push("**New time (in your timezone):** ");
                                msg.push_timestamp(
                                    start,
                                    serenity_utils::message::TimestampStyle::LongDateTime,
                                );
                                msg.push(" (");
                                msg.push_timestamp(
                                    start,
                                    serenity_utils::message::TimestampStyle::Relative,
                                );
                                msg.push(")\n\n");
                                msg.push("If you're no longer available, you can withdraw your signup using the button below or on the website: <");
                                msg.push(&format!(
                                    "{}/event/{}/{}/races/{}/signups",
                                    base_uri(),
                                    series.slug(),
                                    event,
                                    u64::from(race_id)
                                ));
                                msg.push(">");

                                // Create withdraw button
                                let button = CreateButton::new(format!(
                                    "volunteer_withdraw_{}",
                                    u64::from(signup.id)
                                ))
                                .label("Withdraw Signup")
                                .style(ButtonStyle::Danger);
                                let row = CreateActionRow::Buttons(vec![button]);

                                // Send DM
                                let discord_ctx_guard = discord_ctx.read().await;
                                if let Ok(dm_channel) =
                                    discord_user_id.create_dm_channel(&*discord_ctx_guard).await
                                {
                                    if let Err(e) = dm_channel
                                        .send_message(
                                            &*discord_ctx_guard,
                                            CreateMessage::new()
                                                .content(msg.build())
                                                .components(vec![row]),
                                        )
                                        .await
                                    {
                                        eprintln!(
                                            "Failed to send reschedule notification DM to user {}: {}",
                                            signup.user_id, e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            edit_race_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                race_id,
                form.context,
            )
            .await?,
        )
    })
}

// — Seeding race routes —

#[derive(FromForm, CsrfForm)]
pub(crate) struct SeedingRaceForm {
    #[field(default = String::new())]
    csrf: String,
    race_start: String,
    #[field(default = None)]
    race_room: Option<String>,
}

#[rocket::post(
    "/event/<series>/<event>/qualifiers/create-seeding-race",
    data = "<form>"
)]
pub(crate) async fn post_seeding_race(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    http_client: &State<reqwest::Client>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, SeedingRaceForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    if event_data.is_started(&mut transaction).await? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let already_exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM races WHERE series = $1 AND event = $2 AND phase = 'Seeding') AS \"exists!\"",
        series as _, event
    )
    .fetch_one(&mut *transaction)
    .await?;
    if already_exists {
        return Err(StatusOrError::Status(Status::Conflict));
    }

    Ok(if let Some(ref value) = form.value {
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                qualifiers_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    event_data,
                    false,
                    form.context,
                )
                .await?,
            )
        } else {
            let start = match NaiveDateTime::parse_from_str(&value.race_start, "%Y-%m-%dT%H:%M") {
                Ok(naive_dt) => DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(_) => {
                    form.context.push_error(
                        form::Error::validation("Invalid start time format")
                            .with_name("race_start"),
                    );
                    return Ok(RedirectOrContent::Content(
                        qualifiers_form(
                            transaction,
                            me,
                            uri,
                            csrf.as_ref(),
                            event_data,
                            false,
                            form.context,
                        )
                        .await?,
                    ));
                }
            };

            let room = if let Some(ref room_str) = value.race_room {
                let trimmed = room_str.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Url>() {
                        Ok(url) => Some(url),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid room URL").with_name("race_room"),
                            );
                            return Ok(RedirectOrContent::Content(
                                qualifiers_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    false,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let mut race = Race {
                is_qualifier: false,
                qualifier_number: None,
                id: Id::<Races>::new(&mut transaction).await?,
                series: event_data.series,
                event: event_data.event.to_string(),
                source: Source::Manual,
                entrants: Entrants::Open,
                phase: Some("Seeding".to_string()),
                round: None,
                game: None,
                scheduling_thread: None,
                schedule: RaceSchedule::Live {
                    start,
                    end: None,
                    room,
                },
                schedule_updated_at: Some(Utc::now()),
                fpa_invoked: false,
                breaks_used: false,
                draft: None,
                seed: seed::Data::default(),
                video_urls: HashMap::default(),
                restreamers: HashMap::default(),
                last_edited_by: Some(me.id),
                last_edited_at: Some(Utc::now()),
                ignored: false,
                schedule_locked: false,
                notified: false,
                async_notified_1: false,
                async_notified_2: false,
                async_notified_3: false,
                discord_scheduled_event_id: None,
                volunteer_request_sent: false,
                volunteer_request_message_id: None,
                racetime_goal_slug: None,
                scheduling_deadline: None,
                restream_consent_required: false,
                custom_title: None,
                custom_create_room: true,
                companion_race_id: None,
            };
            race.save(&mut transaction).await?;
            match crate::discord_scheduled_events::create_discord_scheduled_event(
                &*discord_ctx.read().await,
                &mut transaction,
                &mut race,
                &event_data,
                http_client.inner(),
            )
            .await
            {
                Ok(()) => {
                    race.save(&mut transaction).await?;
                }
                Err(e) => {
                    eprintln!(
                        "Failed to create Discord scheduled event for seeding race {}: {}",
                        race.id, e
                    );
                }
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            qualifiers_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                false,
                form.context,
            )
            .await?,
        )
    })
}

#[rocket::post(
    "/event/<series>/<event>/qualifiers/seeding-race/<race_id>/delete",
    data = "<form>"
)]
pub(crate) async fn delete_seeding_race(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, DeleteForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    if event_data.is_started(&mut transaction).await? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.value.is_some() {
        sqlx::query!(
            "DELETE FROM races WHERE id = $1 AND series = $2 AND event = $3 AND phase = 'Seeding'",
            race_id as _,
            series as _,
            event
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

async fn edit_seeding_race_form(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    event: Data<'_>,
    race_id: Id<Races>,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    let header = event
        .header(&mut transaction, Some(&me), Tab::Qualifiers, false)
        .await?;

    struct SeedingRaceData {
        start: Option<DateTime<Utc>>,
        room: Option<String>,
    }
    let race = sqlx::query_as!(SeedingRaceData,
        "SELECT start, room FROM races WHERE id = $1 AND series = $2 AND event = $3 AND phase = 'Seeding'",
        race_id as _, event.series as _, &event.event
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let race = match race {
        Some(r) => r,
        None => return Err(event::Error::Sql(sqlx::Error::RowNotFound)),
    };

    let start_formatted = race
        .start
        .map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default();

    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Edit Seeding Race — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Edit Seeding Race";
            : full_form(uri!(post_edit_seeding_race(event.series, &*event.event, race_id)), csrf, html! {
                : form_field("race_start", &mut ctx.errors().collect_vec(), html! {
                    label(for = "race_start") : "Start Time (UTC)";
                    input(type = "datetime-local", name = "race_start", id = "race_start", value = ctx.field_value("race_start").unwrap_or(&start_formatted));
                });
                : form_field("race_room", &mut ctx.errors().collect_vec(), html! {
                    label(for = "race_room") : "Racetime.gg Room URL (optional)";
                    input(type = "text", name = "race_room", id = "race_room", value = ctx.field_value("race_room").unwrap_or(&race.room.unwrap_or_default()), placeholder = "https://racetime.gg/...", style = "width: 100%; max-width: 600px;");
                });
            }, ctx.errors().collect_vec(), "Update Race");
            p {
                a(href = uri!(get(event.series, &*event.event)).to_string()) : "Cancel";
            }
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/qualifiers/seeding-race/<race_id>/edit")]
pub(crate) async fn get_edit_seeding_race(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
    race_id: Id<Races>,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    if event_data.is_started(&mut transaction).await? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    Ok(edit_seeding_race_form(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        event_data,
        race_id,
        Context::default(),
    )
    .await?)
}

#[rocket::post(
    "/event/<series>/<event>/qualifiers/seeding-race/<race_id>/edit",
    data = "<form>"
)]
pub(crate) async fn post_edit_seeding_race(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    http_client: &State<reqwest::Client>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, SeedingRaceForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    if event_data.is_started(&mut transaction).await? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                edit_seeding_race_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    event_data,
                    race_id,
                    form.context,
                )
                .await?,
            )
        } else {
            let start = match NaiveDateTime::parse_from_str(&value.race_start, "%Y-%m-%dT%H:%M") {
                Ok(naive_dt) => DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(_) => {
                    form.context.push_error(
                        form::Error::validation("Invalid start time format")
                            .with_name("race_start"),
                    );
                    return Ok(RedirectOrContent::Content(
                        edit_seeding_race_form(
                            transaction,
                            me,
                            uri,
                            csrf.as_ref(),
                            event_data,
                            race_id,
                            form.context,
                        )
                        .await?,
                    ));
                }
            };

            let room = if let Some(ref room_str) = value.race_room {
                let trimmed = room_str.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Url>() {
                        Ok(url) => Some(url.to_string()),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid room URL").with_name("race_room"),
                            );
                            return Ok(RedirectOrContent::Content(
                                edit_seeding_race_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    race_id,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            sqlx::query!(
                "UPDATE races SET start = $1, room = $2, last_edited_by = $3, last_edited_at = $4 WHERE id = $5 AND series = $6 AND event = $7 AND phase = 'Seeding'",
                start, room, me.id as _, Utc::now(), race_id as _, series as _, event
            )
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;

            {
                let mut transaction = pool.begin().await?;
                match Race::from_id(&mut transaction, http_client.inner(), race_id).await {
                    Ok(race) => {
                        if let Err(e) =
                            crate::discord_scheduled_events::update_discord_scheduled_event(
                                &*discord_ctx.read().await,
                                &mut transaction,
                                &race,
                                &event_data,
                                http_client.inner(),
                            )
                            .await
                        {
                            eprintln!(
                                "Failed to update Discord scheduled event for seeding race {}: {}",
                                race_id, e
                            );
                        }
                        let _ = transaction.commit().await;
                    }
                    Err(e) => eprintln!(
                        "Failed to load seeding race {} for Discord event update: {}",
                        race_id, e
                    ),
                }
            }

            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            edit_seeding_race_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                race_id,
                form.context,
            )
            .await?,
        )
    })
}

#[cfg(test)]
pub(crate) mod route_tests {
    use super::*;

    #[rocket::get("/test-token")]
    fn token(csrf: CsrfToken) -> String {
        csrf.authenticity_token()
    }

    pub(crate) async fn verify_pooled_routes(
        pool: &PgPool,
        staff: i64,
        outsider: i64,
        series: &str,
        event: &str,
        mode: i64,
    ) {
        use rocket::{fairing::AdHoc, http::ContentType, local::asynchronous::Client};
        let auth_pool = pool.clone();
        let rocket = rocket::build()
            .manage(pool.clone())
            .attach(rocket_csrf::Fairing::default())
            .attach(AdHoc::on_request("fixture identity", move |request, _| {
                let pool = auth_pool.clone();
                Box::pin(async move {
                    if let Some(id) = request
                        .headers()
                        .get_one("x-test-user")
                        .and_then(|id| id.parse::<i64>().ok())
                    {
                        let user = User::from_id(&pool, Id::from(id as u64)).await.unwrap();
                        request.local_cache(|| user);
                    }
                })
            }))
            .mount(
                "/",
                rocket::routes![
                    token,
                    post_pooled_config,
                    post_pooled_mode,
                    post_pooled_seed,
                    post_pooled_result,
                    post_pooled_generate
                ],
            );
        let client = Client::tracked(rocket).await.unwrap();
        let csrf = client
            .get("/test-token")
            .private_cookie(rocket::http::Cookie::new(
                "csrf_token",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            ))
            .dispatch()
            .await
            .into_string()
            .await
            .unwrap();
        let base = format!("/event/{series}/{event}/qualifiers");
        let mode_body = format!(
            "mode_id={mode}&position=2&slug=other&display_name=Renamed&seed_gen_type=owr&seed_config=%7B%22base_settings%22%3A%7B%7D%7D&generator_profile=default&enabled=true"
        );
        let seed_body =
            format!("mode_id={mode}&pool_position=1&seed_data=%7B%7D&attest_settings=true");
        let generate_body = format!("mode_id={mode}&retry_failed=false");
        let config_body = "required_mode_count=1&pool_seed_count=2&live_races_per_mode=1&async_run_limit_hours=12&live_entry_close_minutes=10&retry_limit=1&allocation_spread=2&par_finishers=5&score_scale=100&score_offset=2&score_minimum=0&score_maximum=105&requests_paused=true";
        let result_body =
            "attempt_id=0&control_version=1&action=result&reason=test&outcome=forfeit";
        for (path, body) in [
            ("pooled-mode", mode_body.as_str()),
            ("pooled-seed", seed_body.as_str()),
            ("pooled-generate", generate_body.as_str()),
            ("pooled-config", config_body),
            ("pooled-result", result_body),
        ] {
            for token in [None, Some("incorrect")] {
                let body =
                    token.map_or_else(|| body.to_owned(), |token| format!("{body}&csrf={token}"));
                let response = client
                    .post(format!("{base}/{path}"))
                    .header(ContentType::Form)
                    .private_cookie(rocket::http::Cookie::new(
                        "csrf_token",
                        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                    ))
                    .header(rocket::http::Header::new("x-test-user", staff.to_string()))
                    .body(body)
                    .dispatch()
                    .await;
                assert_eq!(
                    response.status(),
                    Status::BadRequest,
                    "invalid CSRF at {path}"
                );
            }
        }
        let name: String =
            sqlx::query_scalar("SELECT display_name FROM qualifier_modes WHERE id=$1")
                .bind(mode)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(name, "Other");
        let encode = |body: &str| {
            format!(
                "{body}&{}",
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("csrf", &csrf)
                    .finish()
            )
        };
        let response = client
            .post(format!("{base}/pooled-mode"))
            .header(ContentType::Form)
            .private_cookie(rocket::http::Cookie::new(
                "csrf_token",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            ))
            .header(rocket::http::Header::new(
                "x-test-user",
                outsider.to_string(),
            ))
            .body(encode(&mode_body))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Forbidden);
        let response = client
            .post(format!("{base}/pooled-mode"))
            .header(ContentType::Form)
            .private_cookie(rocket::http::Cookie::new(
                "csrf_token",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            ))
            .header(rocket::http::Header::new("x-test-user", staff.to_string()))
            .body(encode(&mode_body))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::SeeOther);
        let name: String =
            sqlx::query_scalar("SELECT display_name FROM qualifier_modes WHERE id=$1")
                .bind(mode)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(name, "Renamed");
        let response = client
            .post(format!("{base}/pooled-generate"))
            .header(ContentType::Form)
            .private_cookie(rocket::http::Cookie::new(
                "csrf_token",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            ))
            .header(rocket::http::Header::new("x-test-user", staff.to_string()))
            .body(encode(&generate_body))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::SeeOther);
    }
}
