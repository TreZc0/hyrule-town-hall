use crate::{
    event::{Data, Tab, AsyncKind},
    prelude::*,
};

async fn asyncs_form(mut transaction: Transaction<'_, Postgres>, me: User, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, Some(&me), Tab::Asyncs, false).await?;
    
    struct AsyncRow {
        kind: AsyncKind,
        file_stem: Option<String>,
        web_id: Option<i64>,
    }
    let asyncs = sqlx::query_as!(AsyncRow,
        r#"SELECT kind AS "kind: AsyncKind", file_stem, web_id FROM asyncs WHERE series = $1 AND event = $2 ORDER BY kind"#,
        event.series as _, &event.event
    )
    .fetch_all(&mut *transaction)
    .await?;

    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Asyncs — {}", event.display_name), html! {
        : header;
        article {
            h2 : "Async Management";
            
            h3 : "Existing Asyncs";
            @if asyncs.is_empty() {
                p : "No asyncs defined.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Kind";
                            th : "File Stem";
                            th : "Web ID";
                        }
                    }
                    tbody {
                        @for row in asyncs {
                            tr {
                                td : format!("{:?}", row.kind);
                                td : row.file_stem.unwrap_or_default();
                                td : row.web_id.map(|id| id.to_string()).unwrap_or_default();
                            }
                        }
                    }
                }
            }

            h3 : "Add/Update Async";
            : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                : form_field("kind", &mut ctx.errors().collect_vec(), html! {
                    label(for = "kind") : "Kind";
                    select(name = "kind", id = "kind") {
                        @for kind in all::<AsyncKind>() {
                            option(value = format!("{:?}", kind)) : format!("{:?}", kind);
                        }
                    }
                });
                : form_field("file_stem", &mut ctx.errors().collect_vec(), html! {
                    label(for = "file_stem") : "File Stem";
                    input(type = "text", name = "file_stem", id = "file_stem", value = ctx.field_value("file_stem").unwrap_or(""));
                });
                : form_field("web_id", &mut ctx.errors().collect_vec(), html! {
                    label(for = "web_id") : "Web ID (optional)";
                    input(type = "number", name = "web_id", id = "web_id", value = ctx.field_value("web_id").unwrap_or(""));
                });
            }, ctx.errors().collect_vec(), "Save Async");
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/asyncs")]
pub(crate) async fn get(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    if !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    Ok(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AsyncForm {
    #[field(default = String::new())]
    csrf: String,
    kind: AsyncKind,
    file_stem: String,
    web_id: Option<i64>,
}

#[rocket::post("/event/<series>/<event>/asyncs", data = "<form>")]
pub(crate) async fn post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, AsyncForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        if form.context.errors().next().is_some() {
             RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?)
        } else {
            sqlx::query!(
                r#"INSERT INTO asyncs (series, event, kind, file_stem, web_id) VALUES ($1, $2, $3, $4, $5)
                   ON CONFLICT (series, event, kind) DO UPDATE SET file_stem = EXCLUDED.file_stem, web_id = EXCLUDED.web_id"#,
                event_data.series as _, &event_data.event, value.kind as _, value.file_stem, value.web_id
            ).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, form.context).await?)
    })
}
