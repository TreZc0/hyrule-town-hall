use crate::{
    cal::{Entrants, Race, RaceSchedule, Source},
    event::{Data, Series, Tab},
    prelude::*,
    seed,
};

async fn qualifiers_form(mut transaction: Transaction<'_, Postgres>, me: User, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, is_started: bool, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, Some(&me), Tab::Qualifiers, false).await?;

    struct RaceRow {
        round: Option<String>,
        start: Option<DateTime<Utc>>,
        room: Option<String>,
    }
    let races = sqlx::query_as!(RaceRow,
        "SELECT round, start, room FROM races WHERE series = $1 AND event = $2 AND phase = 'Qualifier' ORDER BY start NULLS LAST, round",
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
            };
            race.save(&mut transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(qualifiers_form(transaction, me, uri, csrf.as_ref(), event_data, false, form.context).await?)
    })
}
