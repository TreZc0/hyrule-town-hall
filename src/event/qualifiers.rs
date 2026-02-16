use crate::{
    cal::{Entrants, Race, RaceSchedule, Source},
    event::{Data, Series, Tab},
    prelude::*,
    seed,
    volunteer_requests,
};

async fn qualifiers_form(mut transaction: Transaction<'_, Postgres>, me: User, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, is_started: bool, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, Some(&me), Tab::Qualifiers, false).await?;

    struct RaceRow {
        id: Id<Races>,
        round: Option<String>,
        start: Option<DateTime<Utc>>,
        room: Option<String>,
    }
    let races = sqlx::query_as!(RaceRow,
        "SELECT id AS \"id: Id<Races>\", round, start, room FROM races WHERE series = $1 AND event = $2 AND phase = 'Qualifier' ORDER BY start NULLS LAST, round",
        event.series as _, &event.event
    )
    .fetch_all(&mut *transaction)
    .await?;

    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Qualifiers — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Live Qualifier Races";
            @if races.is_empty() {
                p : "No live qualifier races defined.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Round";
                            th : "Start Time";
                            th : "Room";
                            @if !is_started {
                                th : "Actions";
                            }
                        }
                    }
                    tbody {
                        @for row in races {
                            tr {
                                td : row.round.unwrap_or_default();
                                td : row.start.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or("Unscheduled".to_owned());
                                td {
                                    @if let Some(ref room) = row.room {
                                        a(href = room) : room;
                                    }
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
                }, ctx.errors().collect_vec(), "Create Race");
            }
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/qualifiers")]
pub(crate) async fn get(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    Ok(qualifiers_form(transaction, me, uri, csrf.as_ref(), event_data, is_started, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RaceForm {
    #[field(default = String::new())]
    csrf: String,
    race_round: String,
    race_start: String,
    #[field(default = None)]
    race_room: Option<String>,
}

#[rocket::post("/event/<series>/<event>/qualifiers/create-race", data = "<form>")]
pub(crate) async fn post_race(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, RaceForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
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
            RedirectOrContent::Content(qualifiers_form(transaction, me, uri, csrf.as_ref(), event_data, false, form.context).await?)
        } else {
            let start = match NaiveDateTime::parse_from_str(&value.race_start, "%Y-%m-%dT%H:%M") {
                Ok(naive_dt) => DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(_) => {
                    form.context.push_error(form::Error::validation("Invalid start time format").with_name("race_start"));
                    return Ok(RedirectOrContent::Content(qualifiers_form(transaction, me, uri, csrf.as_ref(), event_data, false, form.context).await?));
                }
            };

            let room = if let Some(ref room_str) = value.race_room {
                let trimmed = room_str.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Url>() {
                        Ok(url) => Some(url),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid room URL").with_name("race_room"));
                            return Ok(RedirectOrContent::Content(qualifiers_form(transaction, me, uri, csrf.as_ref(), event_data, false, form.context).await?));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let race = Race {
                id: Id::<Races>::new(&mut transaction).await?,
                series: event_data.series,
                event: event_data.event.to_string(),
                source: Source::Manual,
                entrants: Entrants::Open,
                phase: Some("Qualifier".to_string()),
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
            };
            race.save(&mut transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(qualifiers_form(transaction, me, uri, csrf.as_ref(), event_data, false, form.context).await?)
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DeleteForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/qualifiers/<race_id>/delete", data = "<form>")]
pub(crate) async fn delete_race(pool: &State<PgPool>, me: User, csrf: Option<CsrfToken>, series: Series, event: &str, race_id: Id<Races>, form: Form<Contextual<'_, DeleteForm>>) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    if is_started {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.value.is_some() {
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

        // Delete the race (signups will cascade delete automatically)
        sqlx::query!(
            "DELETE FROM races WHERE id = $1 AND series = $2 AND event = $3 AND phase = 'Qualifier'",
            race_id as _, series as _, event
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

async fn edit_race_form(mut transaction: Transaction<'_, Postgres>, me: User, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, race_id: Id<Races>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, Some(&me), Tab::Qualifiers, false).await?;

    struct RaceData {
        round: Option<String>,
        start: Option<DateTime<Utc>>,
        room: Option<String>,
    }
    let race = sqlx::query_as!(RaceData,
        "SELECT round, start, room FROM races WHERE id = $1 AND series = $2 AND event = $3 AND phase = 'Qualifier'",
        race_id as _, event.series as _, &event.event
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let race = match race {
        Some(r) => r,
        None => return Err(event::Error::Sql(sqlx::Error::RowNotFound)),
    };

    let start_formatted = race.start.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default();

    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Edit Qualifier Race — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Edit Live Qualifier Race";
            : full_form(uri!(post_edit_race(event.series, &*event.event, race_id)), csrf, html! {
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
            }, ctx.errors().collect_vec(), "Update Race");
            p {
                a(href = uri!(get(event.series, &*event.event)).to_string()) : "Cancel";
            }
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/qualifiers/<race_id>/edit")]
pub(crate) async fn get_edit(pool: &State<PgPool>, _discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String, race_id: Id<Races>) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let is_started = event_data.is_started(&mut transaction).await?;
    if is_started {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    Ok(edit_race_form(transaction, me, uri, csrf.as_ref(), event_data, race_id, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditRaceForm {
    #[field(default = String::new())]
    csrf: String,
    race_round: String,
    race_start: String,
    #[field(default = None)]
    race_room: Option<String>,
}

#[rocket::post("/event/<series>/<event>/qualifiers/<race_id>/edit", data = "<form>")]
pub(crate) async fn post_edit_race(pool: &State<PgPool>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, race_id: Id<Races>, form: Form<Contextual<'_, EditRaceForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
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
            RedirectOrContent::Content(edit_race_form(transaction, me, uri, csrf.as_ref(), event_data, race_id, form.context).await?)
        } else {
            // Fetch original start time to detect if it changed
            let original_start = sqlx::query_scalar!(
                "SELECT start FROM races WHERE id = $1 AND series = $2 AND event = $3 AND phase = 'Qualifier'",
                race_id as _, series as _, event
            )
            .fetch_optional(&mut *transaction)
            .await?
            .flatten();

            let start = match NaiveDateTime::parse_from_str(&value.race_start, "%Y-%m-%dT%H:%M") {
                Ok(naive_dt) => DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(_) => {
                    form.context.push_error(form::Error::validation("Invalid start time format").with_name("race_start"));
                    return Ok(RedirectOrContent::Content(edit_race_form(transaction, me, uri, csrf.as_ref(), event_data, race_id, form.context).await?));
                }
            };

            let room = if let Some(ref room_str) = value.race_room {
                let trimmed = room_str.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Url>() {
                        Ok(url) => Some(url.to_string()),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid room URL").with_name("race_room"));
                            return Ok(RedirectOrContent::Content(edit_race_form(transaction, me, uri, csrf.as_ref(), event_data, race_id, form.context).await?));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let time_changed = original_start.is_some() && original_start != Some(start);

            sqlx::query!(
                "UPDATE races SET round = $1, start = $2, room = $3, last_edited_by = $4, last_edited_at = $5 WHERE id = $6 AND series = $7 AND event = $8 AND phase = 'Qualifier'",
                value.race_round, start, room, me.id as _, Utc::now(), race_id as _, series as _, event
            )
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;

            // Update volunteer post if the time changed
            if time_changed {
                use serenity::all::{UserId, ButtonStyle, CreateActionRow, CreateButton, CreateMessage};

                let _ = volunteer_requests::update_volunteer_post_for_race(
                    pool,
                    &*discord_ctx.read().await,
                    race_id,
                ).await;

                // Send reschedule notification DMs to volunteers
                let mut transaction = pool.begin().await?;
                if let Ok(signups) = event::roles::Signup::for_race(&mut transaction, race_id).await {
                    let affected_signups: Vec<_> = signups.iter()
                        .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Pending | event::roles::VolunteerSignupStatus::Confirmed))
                        .collect();

                    // Build race description for qualifier
                    let race_description = value.race_round.clone();

                    // Send DM to each affected volunteer
                    for signup in affected_signups {
                        if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                            if let Some(discord) = user.discord {
                                let discord_user_id = UserId::new(discord.id.get());

                                let mut msg = MessageBuilder::default();
                                msg.push("**Race Rescheduled**\n\n");
                                msg.push("The race ");
                                msg.push_mono(&race_description);
                                msg.push(" in ");
                                msg.push(&event_data.display_name);
                                msg.push(" has been rescheduled.\n\n");
                                msg.push("**New time:** ");
                                msg.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                msg.push("\n\n");
                                msg.push("If you're no longer available, you can withdraw your signup using the button below or on the website: <");
                                msg.push(&format!("{}/event/{}/{}/races/{}/signups",
                                    base_uri(), series.slug(), event, u64::from(race_id)));
                                msg.push(">");

                                // Create withdraw button
                                let button = CreateButton::new(format!("volunteer_withdraw_{}", u64::from(signup.id)))
                                    .label("Withdraw Signup")
                                    .style(ButtonStyle::Danger);
                                let row = CreateActionRow::Buttons(vec![button]);

                                // Send DM
                                let discord_ctx_guard = discord_ctx.read().await;
                                if let Ok(dm_channel) = discord_user_id.create_dm_channel(&*discord_ctx_guard).await {
                                    if let Err(e) = dm_channel.send_message(&*discord_ctx_guard,
                                        CreateMessage::new()
                                            .content(msg.build())
                                            .components(vec![row])
                                    ).await {
                                        eprintln!("Failed to send reschedule notification DM to user {}: {}", signup.user_id, e);
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
        RedirectOrContent::Content(edit_race_form(transaction, me, uri, csrf.as_ref(), event_data, race_id, form.context).await?)
    })
}
