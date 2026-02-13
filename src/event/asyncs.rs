use crate::{
    event::{Data, Series, Tab, AsyncKind},
    prelude::*,
};

async fn asyncs_form(mut transaction: Transaction<'_, Postgres>, me: User, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, Some(&me), Tab::Asyncs, false).await?;

    struct AsyncRow {
        kind: AsyncKind,
        file_stem: Option<String>,
        web_id: Option<i64>,
        tfb_uuid: Option<Uuid>,
        xkeys_uuid: Option<Uuid>,
        seed_data: Option<serde_json::Value>,
        start: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    }
    let asyncs = sqlx::query_as!(AsyncRow,
        r#"SELECT kind AS "kind: AsyncKind", file_stem, web_id, tfb_uuid, xkeys_uuid, seed_data, start, end_time FROM asyncs WHERE series = $1 AND event = $2 ORDER BY kind"#,
        event.series as _, &event.event
    )
    .fetch_all(&mut *transaction)
    .await?;

    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Asyncs — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Async Qualifiers";
            @if asyncs.is_empty() {
                p : "No asyncs defined.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Kind";
                            @match event.series {
                                Series::TwwrMain => {
                                    th(colspan = "2") : "Seed";
                                }
                                Series::TriforceBlitz => {
                                    th : "TFB UUID";
                                }
                                Series::Crosskeys => {
                                    th : "Crosskeys UUID";
                                }
                                _ => {
                                    th : "File Stem";
                                    th : "Web ID";
                                }
                            }
                            th : "Start";
                            th : "End";
                        }
                    }
                    tbody {
                        @for row in asyncs {
                            tr {
                                td : format!("{:?}", row.kind);
                                @match event.series {
                                    Series::TwwrMain => {
                                        td(colspan = "2") {
                                            @let permalink = row.seed_data.as_ref().and_then(|d| d.get("permalink")).and_then(|v| v.as_str()).unwrap_or("");
                                            @let seed_hash = row.seed_data.as_ref().and_then(|d| d.get("seed_hash")).and_then(|v| v.as_str()).unwrap_or("");
                                            @if !permalink.is_empty() || !seed_hash.is_empty() {
                                                span(class = "settings-link twwr-seed-link") {
                                                    : "Hover for Seed";
                                                    span(class = "tooltip-content") {
                                                        @if !permalink.is_empty() {
                                                            div {
                                                                strong : "Permalink: ";
                                                                code(style = "user-select: all") : permalink;
                                                            }
                                                        }
                                                        @if !seed_hash.is_empty() {
                                                            div {
                                                                strong : "Seed Hash: ";
                                                                : seed_hash;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Series::TriforceBlitz => {
                                        td : row.tfb_uuid.map(|u| u.to_string()).unwrap_or_default();
                                    }
                                    Series::Crosskeys => {
                                        td : row.xkeys_uuid.map(|u| u.to_string()).unwrap_or_default();
                                    }
                                    _ => {
                                        td : row.file_stem.unwrap_or_default();
                                        td : row.web_id.map(|id| id.to_string()).unwrap_or_default();
                                    }
                                }
                                td : row.start.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default();
                                td : row.end_time.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default();
                            }
                        }
                    }
                }
            }

            h3 : "Add/Update Async";
            @let hidden_fields = match event.series {
                Series::TwwrMain => ["file_stem", "web_id", "tfb_uuid", "xkeys_uuid"].as_slice(),
                Series::TriforceBlitz => ["file_stem", "web_id", "permalink", "seed_hash", "xkeys_uuid"].as_slice(),
                Series::Crosskeys => ["file_stem", "web_id", "permalink", "seed_hash", "tfb_uuid"].as_slice(),
                _ => ["permalink", "seed_hash", "tfb_uuid", "xkeys_uuid"].as_slice(),
            };
            @let mut errors = ctx.errors().filter(|e| !hidden_fields.iter().any(|f| e.is_for(f))).collect_vec();
            : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                // Hidden inputs for fields not used by this series
                @for field in hidden_fields {
                    input(type = "hidden", name = *field, value = "");
                }
                : form_field("kind", &mut errors, html! {
                    label(for = "kind") : "Kind";
                    select(name = "kind", id = "kind") {
                        @for kind in all::<AsyncKind>() {
                            option(value = format!("{:?}", kind)) : format!("{:?}", kind);
                        }
                    }
                });
                @match event.series {
                    Series::TwwrMain => {
                        : form_field("permalink", &mut errors, html! {
                            label(for = "permalink") : "Permalink";
                            input(type = "text", name = "permalink", id = "permalink", value = ctx.field_value("permalink").unwrap_or(""), style = "width: 100%; max-width: 600px;");
                        });
                        : form_field("seed_hash", &mut errors, html! {
                            label(for = "seed_hash") : "Seed Hash";
                            input(type = "text", name = "seed_hash", id = "seed_hash", value = ctx.field_value("seed_hash").unwrap_or(""));
                        });
                    }
                    Series::TriforceBlitz => {
                        : form_field("tfb_uuid", &mut errors, html! {
                            label(for = "tfb_uuid") : "TFB UUID";
                            input(type = "text", name = "tfb_uuid", id = "tfb_uuid", value = ctx.field_value("tfb_uuid").unwrap_or(""));
                        });
                    }
                    Series::Crosskeys => {
                        : form_field("xkeys_uuid", &mut errors, html! {
                            label(for = "xkeys_uuid") : "Crosskeys UUID";
                            input(type = "text", name = "xkeys_uuid", id = "xkeys_uuid", value = ctx.field_value("xkeys_uuid").unwrap_or(""));
                        });
                    }
                    _ => {
                        : form_field("file_stem", &mut errors, html! {
                            label(for = "file_stem") : "File Stem";
                            input(type = "text", name = "file_stem", id = "file_stem", value = ctx.field_value("file_stem").unwrap_or(""));
                        });
                        : form_field("web_id", &mut errors, html! {
                            label(for = "web_id") : "Web ID (optional)";
                            input(type = "number", name = "web_id", id = "web_id", value = ctx.field_value("web_id").unwrap_or(""));
                        });
                    }
                }
                : form_field("start", &mut errors, html! {
                    label(for = "start") : "Start Time (UTC)";
                    input(type = "datetime-local", name = "start", id = "start", value = ctx.field_value("start").unwrap_or(""));
                });
                : form_field("end_time", &mut errors, html! {
                    label(for = "end_time") : "End Time (UTC)";
                    input(type = "datetime-local", name = "end_time", id = "end_time", value = ctx.field_value("end_time").unwrap_or(""));
                });
            }, errors, "Save Async");
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/asyncs")]
pub(crate) async fn get(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    if !event_data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    Ok(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AsyncForm {
    #[field(default = String::new())]
    csrf: String,
    kind: AsyncKind,
    #[field(default = None)]
    file_stem: Option<String>,
    #[field(default = None)]
    web_id: Option<i64>,
    #[field(default = None)]
    tfb_uuid: Option<String>,
    #[field(default = None)]
    xkeys_uuid: Option<String>,
    #[field(default = None)]
    permalink: Option<String>,
    #[field(default = None)]
    seed_hash: Option<String>,
    #[field(default = None)]
    start: Option<String>,
    #[field(default = None)]
    end_time: Option<String>,
}

#[rocket::post("/event/<series>/<event>/asyncs", data = "<form>")]
pub(crate) async fn post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, AsyncForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !event_data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        let hidden_fields = match event_data.series {
            Series::TwwrMain => ["file_stem", "web_id", "tfb_uuid", "xkeys_uuid"].as_slice(),
            Series::TriforceBlitz => ["file_stem", "web_id", "permalink", "seed_hash", "xkeys_uuid"].as_slice(),
            Series::Crosskeys => ["file_stem", "web_id", "permalink", "seed_hash", "tfb_uuid"].as_slice(),
            _ => ["permalink", "seed_hash", "tfb_uuid", "xkeys_uuid"].as_slice(),
        };
        let has_relevant_errors = form.context.errors().any(|e| !hidden_fields.iter().any(|f| e.is_for(f)));
        if has_relevant_errors {
             RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?)
        } else {
            // Build seed_data JSON for TWWR
            let seed_data = if matches!(event_data.series, Series::TwwrMain) {
                let permalink = value.permalink.as_deref().unwrap_or("").trim();
                let seed_hash = value.seed_hash.as_deref().unwrap_or("").trim();
                if !permalink.is_empty() || !seed_hash.is_empty() {
                    Some(serde_json::json!({
                        "permalink": permalink,
                        "seed_hash": seed_hash,
                    }))
                } else {
                    None
                }
            } else {
                None
            };

            // Parse UUIDs for TFB/Crosskeys
            let tfb_uuid = if let Some(ref s) = value.tfb_uuid {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Uuid>() {
                        Ok(uuid) => Some(uuid),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid UUID").with_name("tfb_uuid"));
                            return Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let xkeys_uuid = if let Some(ref s) = value.xkeys_uuid {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Uuid>() {
                        Ok(uuid) => Some(uuid),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid UUID").with_name("xkeys_uuid"));
                            return Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let file_stem = value.file_stem.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);

            // Parse start/end times
            let start = if let Some(ref start_str) = value.start {
                if !start_str.is_empty() {
                    match NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid start time format").with_name("start"));
                            return Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let end_time = if let Some(ref end_str) = value.end_time {
                if !end_str.is_empty() {
                    match NaiveDateTime::parse_from_str(end_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid end time format").with_name("end_time"));
                            return Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            sqlx::query!(
                r#"INSERT INTO asyncs (series, event, kind, file_stem, web_id, tfb_uuid, xkeys_uuid, seed_data, start, end_time)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                   ON CONFLICT (series, event, kind) DO UPDATE SET
                       file_stem = EXCLUDED.file_stem,
                       web_id = EXCLUDED.web_id,
                       tfb_uuid = EXCLUDED.tfb_uuid,
                       xkeys_uuid = EXCLUDED.xkeys_uuid,
                       seed_data = EXCLUDED.seed_data,
                       start = EXCLUDED.start,
                       end_time = EXCLUDED.end_time"#,
                event_data.series as _, &event_data.event, value.kind as _, file_stem, value.web_id,
                tfb_uuid, xkeys_uuid, seed_data, start, end_time
            ).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?)
    })
}

