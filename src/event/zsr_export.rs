//! ZSR Export configuration tab for events (global admin only)

use {
    rocket::{
        form::Form,
        http::Status,
        response::Redirect,
        State,
    },
    rocket_util::Origin,
    rocket_csrf::CsrfToken,
    crate::{
        event::{self, Data, Tab},
        form::{full_form, form_field},
        http::{page, PageError, PageStyle, StatusOrError},
        prelude::*,
        series::Series,
        user::User,
        zsr_export::{self, ExportConfig, ExportTrigger, RestreamingBackend},
    },
};

// ============================================================================
// Error Type
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] ZsrExport(#[from] zsr_export::Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error("unauthorized")]
    Unauthorized,
}

impl From<Error> for StatusOrError<Error> {
    fn from(e: Error) -> Self {
        StatusOrError::Err(e)
    }
}

impl From<sqlx::Error> for StatusOrError<Error> {
    fn from(e: sqlx::Error) -> Self {
        StatusOrError::Err(Error::Sql(e))
    }
}

impl From<event::DataError> for StatusOrError<Error> {
    fn from(e: event::DataError) -> Self {
        StatusOrError::Err(Error::Event(e.into()))
    }
}

impl From<PageError> for StatusOrError<Error> {
    fn from(e: PageError) -> Self {
        StatusOrError::Err(Error::Page(e))
    }
}

impl From<zsr_export::Error> for StatusOrError<Error> {
    fn from(e: zsr_export::Error) -> Self {
        StatusOrError::Err(Error::ZsrExport(e))
    }
}

impl From<event::Error> for StatusOrError<Error> {
    fn from(e: event::Error) -> Self {
        StatusOrError::Err(Error::Event(e))
    }
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Event(e) => e.is_network_error(),
            Self::ZsrExport(e) => e.is_network_error(),
            _ => false,
        }
    }
}

// ============================================================================
// Main Tab View
// ============================================================================

#[rocket::get("/event/<series>/<event>/zsr-export")]
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
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await
        .map_err(event::Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let header = event_data.header(&mut transaction, Some(&me), Tab::ZsrExport, false).await
        .map_err(event::Error::from)?;

    // Get existing exports for this event
    let exports = ExportConfig::for_event(&mut transaction, series, &event).await?;

    // Get all available backends
    let backends = RestreamingBackend::all(&mut transaction).await?;

    // Get backend names for display
    let mut export_displays = Vec::new();
    for export in &exports {
        let backend_name = backends.iter()
            .find(|b| b.id == export.backend_id)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| format!("Backend {}", export.backend_id));
        export_displays.push((export, backend_name));
    }

    // Find backends not yet configured for this event
    let used_backend_ids: Vec<i32> = exports.iter().map(|e| e.backend_id).collect();
    let available_backends: Vec<_> = backends.iter()
        .filter(|b| !used_backend_ids.contains(&b.id))
        .collect();

    let content = html! {
        : header;

        article {
            h2 : "ZSR Restreaming Exports";

            @if exports.is_empty() {
                p : "No exports configured for this event.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Backend";
                            th : "Title Override";
                            th : "Delay (min)";
                            th : "Trigger";
                            th : "Enabled";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for (export, backend_name) in &export_displays {
                            tr {
                                td : backend_name;
                                td : export.title.as_deref().unwrap_or("(event name)");
                                td : export.delay_minutes.to_string();
                                td : export.trigger_condition.to_string();
                                td : if export.enabled { "Yes" } else { "No" };
                                td {
                                    a(href = uri!(edit_export(series, &*event, export.id))) : "Edit";
                                    : " | ";
                                    form(method = "post", action = uri!(sync_export(series, &*event, export.id)), style = "display: inline;") {
                                        input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|t| t.authenticity_token().to_string()).unwrap_or_default());
                                        button(type = "submit") : "Sync Now";
                                    }
                                    : " | ";
                                    form(method = "post", action = uri!(delete_export(series, &*event, export.id)), style = "display: inline;") {
                                        input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|t| t.authenticity_token().to_string()).unwrap_or_default());
                                        button(type = "submit", onclick = "return confirm('Delete this export configuration?')") : "Delete";
                                    }
                                }
                            }
                        }
                    }
                }
            }

            h3 : "Add New Export";

            @if backends.is_empty() {
                p {
                    : "No restreaming backends configured. ";
                    a(href = uri!(crate::admin::zsr_backends())) : "Configure backends in the admin panel";
                    : " first.";
                }
            } else if available_backends.is_empty() {
                p : "All backends are already configured for this event.";
            } else {
                : full_form(uri!(add_export(series, &*event)), csrf.as_ref(), html! {
                    : form_field("backend_id", &mut Vec::new(), html! {
                        label(for = "backend_id") : "Backend";
                        select(id = "backend_id", name = "backend_id", required) {
                            @for backend in &available_backends {
                                option(value = backend.id.to_string()) : format!("{} ({})", &backend.name, backend.language);
                            }
                        }
                    });

                    : form_field("title", &mut Vec::new(), html! {
                        label(for = "title") : "Title Override (optional)";
                        input(type = "text", id = "title", name = "title", placeholder = "Leave empty to use event name");
                    });

                    : form_field("description", &mut Vec::new(), html! {
                        label(for = "description") : "Description/Estimate";
                        input(type = "text", id = "description", name = "description", placeholder = "e.g., 3:00:00");
                    });

                    : form_field("delay_minutes", &mut Vec::new(), html! {
                        label(for = "delay_minutes") : "Delay (minutes)";
                        input(type = "number", id = "delay_minutes", name = "delay_minutes", value = "0", min = "0");
                    });

                    : form_field("nodecg_pk", &mut Vec::new(), html! {
                        label(for = "nodecg_pk") : "NodeCG PK (optional)";
                        input(type = "number", id = "nodecg_pk", name = "nodecg_pk", placeholder = "Integer ID");
                    });

                    : form_field("trigger_condition", &mut Vec::new(), html! {
                        label(for = "trigger_condition") : "Trigger Condition";
                        select(id = "trigger_condition", name = "trigger_condition", required) {
                            option(value = "when_scheduled") : "When Scheduled";
                            option(value = "when_restream_channel_set") : "When Restream Channel Set";
                            option(value = "when_volunteer_signed_up") : "When Volunteer Signed Up";
                        }
                    });
                }, Vec::new(), "Add Export");
            }

            h3 : "Manual Actions";
            form(method = "post", action = uri!(sync_all(series, &*event))) {
                input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|t| t.authenticity_token().to_string()).unwrap_or_default());
                button(type = "submit") : "Sync All Exports Now";
            }
        }
    };

    transaction.commit().await?;

    Ok(page(
        pool.begin().await?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("ZSR Export — {}", event_data.display_name),
        content,
    ).await?)
}

// ============================================================================
// Add Export
// ============================================================================

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct AddExportForm {
    #[field(default = String::new())]
    csrf: String,
    backend_id: i32,
    title: Option<String>,
    description: Option<String>,
    delay_minutes: i32,
    nodecg_pk: Option<i32>,
    trigger_condition: String,
}

#[rocket::post("/event/<series>/<event>/zsr-export", data = "<form>")]
pub(crate) async fn add_export(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, AddExportForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let trigger = match value.trigger_condition.as_str() {
            "when_scheduled" => ExportTrigger::WhenScheduled,
            "when_restream_channel_set" => ExportTrigger::WhenRestreamChannelSet,
            "when_volunteer_signed_up" => ExportTrigger::WhenVolunteerSignedUp,
            _ => return Err(StatusOrError::Status(Status::BadRequest)),
        };

        let title = value.title.as_ref().filter(|s| !s.is_empty()).map(|s| s.as_str());
        let description = value.description.as_ref().filter(|s| !s.is_empty()).map(|s| s.as_str());

        let mut transaction = pool.begin().await?;

        let export = ExportConfig::create(
            &mut transaction,
            series,
            event,
            value.backend_id,
            title,
            description,
            value.delay_minutes,
            value.nodecg_pk,
            trigger,
        ).await?;

        // Get backend and event data to ensure description entry exists
        if let Some(backend) = RestreamingBackend::from_id(&mut transaction, value.backend_id).await? {
            if let Some(event_data) = Data::new(&mut transaction, series, event).await
                .map_err(event::Error::from)?
            {
                // Ensure the description entry exists in the Descriptions sheet
                zsr_export::ensure_description_entry(
                    http_client,
                    &export,
                    &backend,
                    &event_data.display_name,
                    2, // default runner count
                ).await?;
            }
        }

        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

// ============================================================================
// Edit Export
// ============================================================================

#[rocket::get("/event/<series>/<event>/zsr-export/<export_id>/edit")]
pub(crate) async fn edit_export(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
    export_id: i32,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;

    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await
        .map_err(event::Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let header = event_data.header(&mut transaction, Some(&me), Tab::ZsrExport, true).await
        .map_err(event::Error::from)?;

    let export = ExportConfig::from_id(&mut transaction, export_id).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let backend = RestreamingBackend::from_id(&mut transaction, export.backend_id).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let content = html! {
        : header;

        article {
            h2 : format!("Edit Export to {}", backend.name);

            : full_form(uri!(update_export(series, &*event, export_id)), csrf.as_ref(), html! {
                : form_field("title", &mut Vec::new(), html! {
                    label(for = "title") : "Title Override (optional)";
                    input(type = "text", id = "title", name = "title",
                        value = export.title.as_deref().unwrap_or(""),
                        placeholder = "Leave empty to use event name");
                });

                : form_field("description", &mut Vec::new(), html! {
                    label(for = "description") : "Description/Estimate";
                    input(type = "text", id = "description", name = "description",
                        value = export.description.as_deref().unwrap_or(""));
                });

                : form_field("delay_minutes", &mut Vec::new(), html! {
                    label(for = "delay_minutes") : "Delay (minutes)";
                    input(type = "number", id = "delay_minutes", name = "delay_minutes",
                        value = export.delay_minutes.to_string(), min = "0");
                });

                : form_field("nodecg_pk", &mut Vec::new(), html! {
                    label(for = "nodecg_pk") : "NodeCG PK (optional)";
                    input(type = "number", id = "nodecg_pk", name = "nodecg_pk",
                        value? = export.nodecg_pk.map(|v| v.to_string()));
                });

                : form_field("trigger_condition", &mut Vec::new(), html! {
                    label(for = "trigger_condition") : "Trigger Condition";
                    select(id = "trigger_condition", name = "trigger_condition", required) {
                        option(value = "when_scheduled", selected? = matches!(export.trigger_condition, ExportTrigger::WhenScheduled)) : "When Scheduled";
                        option(value = "when_restream_channel_set", selected? = matches!(export.trigger_condition, ExportTrigger::WhenRestreamChannelSet)) : "When Restream Channel Set";
                        option(value = "when_volunteer_signed_up", selected? = matches!(export.trigger_condition, ExportTrigger::WhenVolunteerSignedUp)) : "When Volunteer Signed Up";
                    }
                });

                : form_field("enabled", &mut Vec::new(), html! {
                    input(type = "checkbox", id = "enabled", name = "enabled", checked? = export.enabled);
                    label(for = "enabled") : " Enabled";
                });
            }, Vec::new(), "Save Changes");

            p {
                a(href = uri!(get(series, &*event))) : "Back to ZSR Export";
            }
        }
    };

    transaction.commit().await?;

    Ok(page(
        pool.begin().await?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Edit Export — {}", event_data.display_name),
        content,
    ).await?)
}

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct UpdateExportForm {
    #[field(default = String::new())]
    csrf: String,
    title: Option<String>,
    description: Option<String>,
    delay_minutes: i32,
    nodecg_pk: Option<i32>,
    trigger_condition: String,
    enabled: bool,
}

#[rocket::post("/event/<series>/<event>/zsr-export/<export_id>", data = "<form>")]
pub(crate) async fn update_export(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, UpdateExportForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let trigger = match value.trigger_condition.as_str() {
            "when_scheduled" => ExportTrigger::WhenScheduled,
            "when_restream_channel_set" => ExportTrigger::WhenRestreamChannelSet,
            "when_volunteer_signed_up" => ExportTrigger::WhenVolunteerSignedUp,
            _ => return Err(StatusOrError::Status(Status::BadRequest)),
        };

        let title = value.title.as_ref().filter(|s| !s.is_empty()).map(|s| s.as_str());
        let description = value.description.as_ref().filter(|s| !s.is_empty()).map(|s| s.as_str());

        let mut transaction = pool.begin().await?;

        ExportConfig::update(
            &mut transaction,
            export_id,
            title,
            description,
            value.delay_minutes,
            value.nodecg_pk,
            trigger,
            value.enabled,
        ).await?;

        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

// ============================================================================
// Delete Export
// ============================================================================

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct DeleteExportForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/zsr-export/<export_id>/delete", data = "<form>")]
pub(crate) async fn delete_export(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, DeleteExportForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        let mut transaction = pool.begin().await?;
        ExportConfig::delete(&mut transaction, export_id).await?;
        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

// ============================================================================
// Sync Actions
// ============================================================================

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct SyncForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/zsr-export/<export_id>/sync", data = "<form>")]
pub(crate) async fn sync_export(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, SyncForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        let mut transaction = pool.begin().await?;

        let export = ExportConfig::from_id(&mut transaction, export_id).await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

        let (exported, removed, errors) = zsr_export::sync_all_races(&mut transaction, http_client.inner(), &export).await?;

        transaction.commit().await?;

        eprintln!("Manual sync: exported {}, removed {}, {} errors", exported, removed, errors.len());
        for err in &errors {
            eprintln!("  - {}", err);
        }
    }

    Ok(Redirect::to(uri!(get(series, event))))
}

#[rocket::post("/event/<series>/<event>/zsr-export/sync-all", data = "<form>")]
pub(crate) async fn sync_all(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, SyncForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        let mut transaction = pool.begin().await?;

        let exports = ExportConfig::for_event(&mut transaction, series, event).await?;

        for export in exports {
            if export.enabled {
                let (exported, removed, errors) = zsr_export::sync_all_races(&mut transaction, http_client.inner(), &export).await?;
                eprintln!("Manual sync to backend {}: exported {}, removed {}, {} errors",
                    export.backend_id, exported, removed, errors.len());
            }
        }

        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event))))
}
