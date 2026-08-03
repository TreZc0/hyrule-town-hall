//! SpeedGaming export configuration tab for events.

use {
    rocket::{
        form::Form,
        http::Status,
        response::Redirect,
        State,
    },
    rocket_csrf::CsrfToken,
    rocket_util::Origin,
    crate::{
        event::{self, Data, Tab},
        form::{full_form, form_field},
        http::{page, PageError, PageKind, PageStyle, StatusOrError},
        prelude::*,
        series::Series,
        speedgaming_export::{self, ExportConfig, ExportTrigger},
        user::User,
    },
};

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] SpeedGaming(#[from] speedgaming_export::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

impl From<Error> for StatusOrError<Error> {
    fn from(error: Error) -> Self {
        Self::Err(error)
    }
}

impl From<sqlx::Error> for StatusOrError<Error> {
    fn from(error: sqlx::Error) -> Self {
        Self::Err(Error::Sql(error))
    }
}

impl From<event::DataError> for StatusOrError<Error> {
    fn from(error: event::DataError) -> Self {
        Self::Err(Error::Event(error.into()))
    }
}

impl From<event::Error> for StatusOrError<Error> {
    fn from(error: event::Error) -> Self {
        Self::Err(Error::Event(error))
    }
}

impl From<PageError> for StatusOrError<Error> {
    fn from(error: PageError) -> Self {
        Self::Err(Error::Page(error))
    }
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Event(error) => error.is_network_error(),
            Self::SpeedGaming(error) => error.is_network_error(),
            _ => false,
        }
    }
}

fn trigger(value: &str) -> Option<ExportTrigger> {
    match value {
        "when_scheduled" => Some(ExportTrigger::WhenScheduled),
        "when_restream_channel_set" => Some(ExportTrigger::WhenRestreamChannelSet),
        "when_volunteer_signed_up" => Some(ExportTrigger::WhenVolunteerSignedUp),
        _ => None,
    }
}

fn valid_slug(slug: &str) -> bool {
    regex_is_match!("^[0-9A-Za-z_-]+$", slug)
}

#[rocket::get("/event/<series>/<event>/sg-export")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden))
    }
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let header = event_data.header(&mut transaction, Some(&me), Tab::SpeedGamingExport, false).await?;
    let exports = ExportConfig::for_event(&mut transaction, series, &event).await?;
    let mut stats: HashMap<i32, (i64, i64)> = HashMap::default();
    for export in &exports {
        let row = sqlx::query!(r#"
            SELECT COUNT(*) FILTER (WHERE state = 'succeeded') AS "succeeded!",
                   COUNT(*) FILTER (WHERE state IN ('failed', 'ambiguous')) AS "attention!"
            FROM speedgaming_race_exports WHERE export_id = $1
        "#, export.id).fetch_one(&mut *transaction).await?;
        stats.insert(export.id, (row.succeeded, row.attention));
    }

    let content = html! {
        : header;
        article {
            h2 : "SpeedGaming Export";
            p : "Exports upcoming 1v1 races to one SpeedGaming event. Volunteer signup languages are selected separately.";

            @if exports.is_empty() {
                p : "No SpeedGaming exports are configured for this event.";
            }
            @for export in &exports {
                @let (succeeded, attention) = stats.get(&export.id).copied().unwrap_or_default();
                section {
                    h3 : &export.slug;
                    p : format!("Exported races: {succeeded}; needs attention: {attention}");
                    : full_form(uri!(update_export(series, &*event, export.id)), csrf.as_ref(), html! {
                        : form_field("volunteer_languages", &mut Vec::new(), html! {
                            label : "Volunteer signup languages";
                            div {
                                @for language in all::<Language>() {
                                    label {
                                        input(type = "checkbox", name = "volunteer_languages", value = language.short_code(), checked? = export.volunteer_languages.contains(&language));
                                        : format!(" {language}");
                                    }
                                }
                            }
                            small : " Only applications in these languages are sent to SpeedGaming and synchronized back to HTH.";
                        });
                        : form_field("slug", &mut Vec::new(), html! {
                            label(for = "slug") : "SpeedGaming Slug";
                            input(type = "text", name = "slug", value = &export.slug, required, pattern = "[0-9A-Za-z_-]+");
                        });
                        : form_field("trigger_condition", &mut Vec::new(), html! {
                            label(for = "trigger_condition") : "Trigger Condition";
                            select(name = "trigger_condition", required) {
                                option(value = "when_scheduled", selected? = matches!(export.trigger_condition, ExportTrigger::WhenScheduled)) : "When Scheduled";
                                option(value = "when_restream_channel_set", selected? = matches!(export.trigger_condition, ExportTrigger::WhenRestreamChannelSet)) : "When Restream Channel Set";
                                option(value = "when_volunteer_signed_up", selected? = matches!(export.trigger_condition, ExportTrigger::WhenVolunteerSignedUp)) : "When Volunteer Signed Up";
                            }
                        });
                        : form_field("delay_minutes", &mut Vec::new(), html! {
                            label(for = "delay_minutes") : "Delay (minutes)";
                            input(type = "number", name = "delay_minutes", value = export.delay_minutes.to_string(), min = "0", required);
                        });
                        : form_field("export_volunteers", &mut Vec::new(), html! {
                            input(type = "checkbox", name = "export_volunteers", checked? = export.export_volunteers);
                            label : " Export commentary and tracking signups";
                        });
                        : form_field("enabled", &mut Vec::new(), html! {
                            input(type = "checkbox", name = "enabled", checked? = export.enabled);
                            label : " Enabled";
                        });
                    }, Vec::new(), "Save");
                    form(method = "post", action = uri!(delete_export(series, &*event, export.id))) {
                        input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|token| token.authenticity_token().to_string()).unwrap_or_default());
                        button(type = "submit", onclick = "return confirm('Delete this SpeedGaming export?')") : "Delete";
                    }
                }
            }

            @if exports.is_empty() {
                h3 : "Add Export";
                : full_form(uri!(add_export(series, &*event)), csrf.as_ref(), html! {
                    : form_field("volunteer_languages", &mut Vec::new(), html! {
                        label : "Volunteer signup languages";
                        div {
                            @for language in all::<Language>() {
                                label {
                                    input(type = "checkbox", name = "volunteer_languages", value = language.short_code());
                                    : format!(" {language}");
                                }
                            }
                        }
                        small : " Only applications in these languages are sent to SpeedGaming and synchronized back to HTH.";
                    });
                    : form_field("slug", &mut Vec::new(), html! {
                        label(for = "slug") : "SpeedGaming Slug";
                        input(type = "text", name = "slug", required, pattern = "[0-9A-Za-z_-]+");
                    });
                    : form_field("trigger_condition", &mut Vec::new(), html! {
                        label(for = "trigger_condition") : "Trigger Condition";
                        select(name = "trigger_condition", required) {
                            option(value = "when_scheduled") : "When Scheduled";
                            option(value = "when_restream_channel_set") : "When Restream Channel Set";
                            option(value = "when_volunteer_signed_up") : "When Volunteer Signed Up";
                        }
                    });
                    : form_field("delay_minutes", &mut Vec::new(), html! {
                        label(for = "delay_minutes") : "Delay (minutes)";
                        input(type = "number", name = "delay_minutes", value = "0", min = "0", required);
                    });
                    : form_field("export_volunteers", &mut Vec::new(), html! {
                        input(type = "checkbox", name = "export_volunteers");
                        label : " Export commentary and tracking signups";
                    });
                }, Vec::new(), "Add Export");
            }

            form(method = "post", action = uri!(sync_all(series, &*event))) {
                input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|token| token.authenticity_token().to_string()).unwrap_or_default());
                button(type = "submit") : "Sync Now";
            }
        }
    };
    transaction.commit().await?;
    Ok(page(
        pool.begin().await?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("SpeedGaming Export — {}", event_data.display_name),
        content,
    ).await?)
}

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct ExportForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = Vec::new())]
    volunteer_languages: Vec<Language>,
    slug: String,
    trigger_condition: String,
    delay_minutes: i32,
    export_volunteers: bool,
    enabled: bool,
}

#[rocket::post("/event/<series>/<event>/sg-export", data = "<form>")]
pub(crate) async fn add_export(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, ExportForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() { return Err(StatusOrError::Status(Status::Forbidden)) }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(value) = &form.value {
        if value.delay_minutes < 0 || !valid_slug(&value.slug) {
            return Err(StatusOrError::Status(Status::BadRequest))
        }
        let trigger = trigger(&value.trigger_condition).ok_or(StatusOrError::Status(Status::BadRequest))?;
        let mut transaction = pool.begin().await?;
        Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
        ExportConfig::create(&mut transaction, series, event, &value.slug, trigger, value.delay_minutes, value.export_volunteers, &value.volunteer_languages).await?;
        transaction.commit().await?;
        speedgaming_export::schedule_sync(pool.inner().clone(), http_client.inner().clone());
    }
    Ok(Redirect::to(uri!(get(series, event))))
}

#[rocket::post("/event/<series>/<event>/sg-export/<export_id>/edit", data = "<form>")]
pub(crate) async fn update_export(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, ExportForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() { return Err(StatusOrError::Status(Status::Forbidden)) }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(value) = &form.value {
        if value.delay_minutes < 0 || !valid_slug(&value.slug) {
            return Err(StatusOrError::Status(Status::BadRequest))
        }
        let trigger = trigger(&value.trigger_condition).ok_or(StatusOrError::Status(Status::BadRequest))?;
        let mut transaction = pool.begin().await?;
        let export = ExportConfig::from_id(&mut transaction, export_id).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
        if export.series != series || export.event != event {
            return Err(StatusOrError::Status(Status::NotFound))
        }
        let has_attempts = sqlx::query_scalar!("SELECT EXISTS (SELECT 1 FROM speedgaming_race_exports WHERE export_id = $1) AS \"exists!\"", export_id)
            .fetch_one(&mut *transaction).await?;
        if has_attempts && export.slug != value.slug {
            return Err(StatusOrError::Status(Status::BadRequest))
        }
        ExportConfig::update(&mut transaction, export_id, &value.slug, trigger, value.delay_minutes, value.export_volunteers, value.enabled, &value.volunteer_languages).await?;
        transaction.commit().await?;
        if value.enabled {
            speedgaming_export::schedule_sync(pool.inner().clone(), http_client.inner().clone());
        }
    }
    Ok(Redirect::to(uri!(get(series, event))))
}

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct ActionForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/sg-export/<export_id>/delete", data = "<form>")]
pub(crate) async fn delete_export(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, ActionForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() { return Err(StatusOrError::Status(Status::Forbidden)) }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_some() {
        let mut transaction = pool.begin().await?;
        let export = ExportConfig::from_id_for_update(&mut transaction, export_id).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
        if export.series != series || export.event != event {
            return Err(StatusOrError::Status(Status::NotFound))
        }
        sqlx::query!("UPDATE speedgaming_exports SET enabled = false, updated_at = NOW() WHERE id = $1", export_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;

        // Let an already-running request finish, while the disabled flag prevents it from
        // claiming any more races. This keeps a stale sync snapshot from racing the delete.
        let _guard = speedgaming_export::SYNC_LOCK.lock().await;
        let mut transaction = pool.begin().await?;
        ExportConfig::archive(&mut transaction, export_id).await?;
        transaction.commit().await?;
    }
    Ok(Redirect::to(uri!(get(series, event))))
}

#[rocket::post("/event/<series>/<event>/sg-export/sync", data = "<form>")]
pub(crate) async fn sync_all(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, ActionForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() { return Err(StatusOrError::Status(Status::Forbidden)) }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_some() {
        speedgaming_export::schedule_sync(pool.inner().clone(), http_client.inner().clone());
    }
    Ok(Redirect::to(uri!(get(series, event))))
}
