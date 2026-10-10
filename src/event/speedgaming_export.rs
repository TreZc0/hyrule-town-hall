//! SpeedGaming export configuration tab for events.

use {
    crate::{
        event::{self, Data, Tab},
        form::{form_field, full_form},
        http::{PageError, PageKind, PageStyle, StatusOrError, page},
        prelude::*,
        series::Series,
        speedgaming_export::{self, ExportConfig, ExportTrigger},
        user::User,
    },
    rocket::{State, form::Form, http::Status, response::Redirect},
    rocket_csrf::CsrfToken,
    rocket_util::Origin,
};

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Event(#[from] event::Error),
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    SpeedGaming(#[from] speedgaming_export::Error),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
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

#[derive(sqlx::FromRow)]
struct Delivery {
    export_id: i32,
    kind: String,
    id: i64,
    race_id: Option<i64>,
    description: String,
    episode_id: Option<i64>,
    status: String,
    last_error: Option<String>,
    succeeded: bool,
    needs_attention: bool,
}

async fn deliveries(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
) -> sqlx::Result<Vec<Delivery>> {
    sqlx::query_as(
        r#"
        SELECT re.export_id, 'race' AS kind, re.race_id AS id, re.race_id,
               COALESCE(r.custom_title, NULLIF(concat_ws(' ', r.phase, r.round), ''), 'Race ' || re.race_id) AS description,
               re.episode_id, re.operation || ' — ' || re.state::text AS status, re.last_error,
               re.episode_id IS NOT NULL AS succeeded,
               re.state IN ('failed', 'ambiguous') AS needs_attention
        FROM speedgaming_race_exports re
        JOIN speedgaming_exports e ON e.id = re.export_id
        LEFT JOIN races r ON r.id = re.race_id
        WHERE e.series = $1 AND e.event = $2 AND e.archived_at IS NULL
        UNION ALL
        SELECT v.export_id, 'volunteer', v.signup_id::BIGINT, s.race_id,
               concat_ws(' — ', COALESCE(u.discord_display_name, u.racetime_display_name, 'Signup ' || v.signup_id),
                   COALESCE(v.role, rt.name), rb.language::text),
               v.episode_id, v.state::text, v.last_error, v.state = 'succeeded',
               v.state IN ('failed', 'ambiguous')
        FROM speedgaming_volunteer_exports v
        JOIN speedgaming_exports e ON e.id = v.export_id
        LEFT JOIN signups s ON s.id = v.signup_id
        LEFT JOIN users u ON u.id = s.user_id
        LEFT JOIN role_bindings rb ON rb.id = s.role_binding_id
        LEFT JOIN role_types rt ON rt.id = rb.role_type_id
        WHERE e.series = $1 AND e.event = $2 AND e.archived_at IS NULL
        ORDER BY export_id, kind, id
        "#,
    )
    .bind(series)
    .bind(event)
    .fetch_all(&mut **transaction)
    .await
}

fn delivery_issues(
    deliveries: &[Delivery],
    export_id: i32,
    series: Series,
    event: &str,
    csrf: Option<&CsrfToken>,
) -> RawHtml<String> {
    let issues = deliveries
        .iter()
        .filter(|delivery| delivery.export_id == export_id && delivery.needs_attention)
        .collect_vec();
    html! {
        details {
            summary : format!("Exports needing attention ({})", issues.len());
            @if issues.is_empty() {
                p : "No export issues.";
            } else {
                p : "Retry an export, or ignore it to stop synchronization for that export. Ignoring keeps existing SpeedGaming entries.";
                p : "Before retrying an uncertain submission, check SpeedGaming to avoid submitting it twice.";
                form(method = "post", action = uri!(resolve_issues(series, event, export_id))) {
                    input(type = "hidden", name = "csrf", value = csrf.map(|token| token.authenticity_token().to_string()).unwrap_or_default());
                    button(type = "submit", name = "action", value = "ignore") : "Ignore all issues";
                }
                table {
                    thead { tr { th : "Export"; th : "Race"; th : "SpeedGaming episode"; th : "Sync status"; th : "Details"; th : "Actions"; } }
                    tbody {
                        @for delivery in issues {
                            tr {
                                td : &delivery.description;
                                td : delivery.race_id.map(|id| id.to_string()).unwrap_or_default();
                                td : delivery.episode_id.map(|id| id.to_string()).unwrap_or_default();
                                td : &delivery.status;
                                td : delivery.last_error.as_deref().unwrap_or_default();
                                td {
                                    form(method = "post", action = uri!(resolve_issues(series, event, export_id))) {
                                        input(type = "hidden", name = "csrf", value = csrf.map(|token| token.authenticity_token().to_string()).unwrap_or_default());
                                        input(type = "hidden", name = "kind", value = &delivery.kind);
                                        input(type = "hidden", name = "id", value = delivery.id.to_string());
                                        button(type = "submit", name = "action", value = "retry", onclick? = delivery.status.contains("ambiguous").then_some("return confirm('Retry this uncertain submission? Confirm that SpeedGaming did not already receive it.')")) : "Retry";
                                        button(type = "submit", name = "action", value = "ignore") : "Ignore";
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
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
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let header = event_data
        .header(&mut transaction, Some(&me), Tab::SpeedGamingExport, false)
        .await?;
    let exports = ExportConfig::for_event(&mut transaction, series, &event).await?;
    let deliveries = deliveries(&mut transaction, series, &event).await?;

    let content = html! {
        : header;
        article {
            h2 : "SpeedGaming Export";
            p : "Exports upcoming 1v1 and open races, including qualifiers, to one SpeedGaming event. Open races are submitted without player details. Volunteer signup languages are selected separately.";
            @if !speedgaming_export::lifecycle::configured() {
                p(class = "error") : "SpeedGaming username and password must be configured on the server before synchronization can run.";
            }

            @if exports.is_empty() {
                p : "No SpeedGaming exports are configured for this event.";
            }
            @for export in &exports {
                @let race_count = deliveries.iter().filter(|delivery| delivery.export_id == export.id && delivery.kind == "race" && delivery.succeeded).count();
                @let volunteer_count = deliveries.iter().filter(|delivery| delivery.export_id == export.id && delivery.kind == "volunteer" && delivery.succeeded).count();
                section {
                    h3 : &export.slug;
                    p : format!("Exported races: {race_count}; exported volunteer signups: {volunteer_count}");
                    : delivery_issues(&deliveries, export.id, series, &event, csrf.as_ref());
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
        PageStyle {
            kind: PageKind::Other,
            ..PageStyle::default()
        },
        &format!("SpeedGaming Export — {}", event_data.display_name),
        content,
    )
    .await?)
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
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(value) = &form.value {
        if value.delay_minutes < 0 || !valid_slug(&value.slug) {
            return Err(StatusOrError::Status(Status::BadRequest));
        }
        let trigger =
            trigger(&value.trigger_condition).ok_or(StatusOrError::Status(Status::BadRequest))?;
        let mut transaction = pool.begin().await?;
        Data::new(&mut transaction, series, event)
            .await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        ExportConfig::create(
            &mut transaction,
            series,
            event,
            &value.slug,
            trigger,
            value.delay_minutes,
            value.export_volunteers,
            &value.volunteer_languages,
        )
        .await?;
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
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(value) = &form.value {
        if value.delay_minutes < 0 || !valid_slug(&value.slug) {
            return Err(StatusOrError::Status(Status::BadRequest));
        }
        let trigger =
            trigger(&value.trigger_condition).ok_or(StatusOrError::Status(Status::BadRequest))?;
        let mut transaction = pool.begin().await?;
        let export = ExportConfig::from_id(&mut transaction, export_id)
            .await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        if export.series != series || export.event != event {
            return Err(StatusOrError::Status(Status::NotFound));
        }
        let has_attempts = sqlx::query_scalar!("SELECT EXISTS (SELECT 1 FROM speedgaming_race_exports WHERE export_id = $1) AS \"exists!\"", export_id)
            .fetch_one(&mut *transaction).await?;
        if has_attempts && export.slug != value.slug {
            return Err(StatusOrError::Status(Status::BadRequest));
        }
        ExportConfig::update(
            &mut transaction,
            export_id,
            &value.slug,
            trigger,
            value.delay_minutes,
            value.export_volunteers,
            value.enabled,
            &value.volunteer_languages,
        )
        .await?;
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
    race_id: Option<i64>,
}

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct IssueForm {
    #[field(default = String::new())]
    csrf: String,
    kind: Option<String>,
    id: Option<i64>,
    action: String,
}

async fn resolve_delivery_issues(
    transaction: &mut Transaction<'_, Postgres>,
    export_id: i32,
    kind: Option<&str>,
    id: Option<i64>,
    retry: bool,
) -> sqlx::Result<()> {
    if kind.is_none_or(|kind| kind == "race") {
        sqlx::query(
            "UPDATE speedgaming_race_exports
             SET state = $3::speedgaming_delivery_state, last_attempt_at = NULL
             WHERE export_id = $1 AND ($2::BIGINT IS NULL OR race_id = $2)
               AND state IN ('failed', 'ambiguous')",
        )
        .bind(export_id)
        .bind(id)
        .bind(if retry { "pending" } else { "ignored" })
        .execute(&mut **transaction)
        .await?;
    }
    if kind.is_none_or(|kind| kind == "volunteer") {
        sqlx::query(
            "UPDATE speedgaming_volunteer_exports
             SET state = $3::speedgaming_delivery_state, last_attempt_at = NULL
             WHERE export_id = $1 AND ($2::BIGINT IS NULL OR signup_id = $2)
               AND state IN ('failed', 'ambiguous')",
        )
        .bind(export_id)
        .bind(id)
        .bind(if retry { "pending" } else { "ignored" })
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

#[rocket::post(
    "/event/<series>/<event>/sg-export/<export_id>/issues",
    data = "<form>"
)]
pub(crate) async fn resolve_issues(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, IssueForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(value) = form.value {
        let retry = match value.action.as_str() {
            "retry" => true,
            "ignore" => false,
            _ => return Err(StatusOrError::Status(Status::BadRequest)),
        };
        match (value.kind.as_deref(), value.id, retry) {
            (None, None, false) | (Some("race" | "volunteer"), Some(_), _) => {}
            _ => return Err(StatusOrError::Status(Status::BadRequest)),
        }
        let _guard = speedgaming_export::SYNC_LOCK.lock().await;
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(728914, 1)")
            .execute(&mut *transaction)
            .await?;
        let export = ExportConfig::from_id_for_update(&mut transaction, export_id)
            .await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        if export.series != series || export.event != event {
            return Err(StatusOrError::Status(Status::NotFound));
        }
        resolve_delivery_issues(
            &mut transaction,
            export_id,
            value.kind.as_deref(),
            value.id,
            retry,
        )
        .await?;
        transaction.commit().await?;
        if retry {
            speedgaming_export::schedule_sync(pool.inner().clone(), http_client.inner().clone());
        }
    }
    Ok(Redirect::to(uri!(get(series, event))))
}

#[rocket::post(
    "/event/<series>/<event>/sg-export/<export_id>/delete",
    data = "<form>"
)]
pub(crate) async fn delete_export(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    export_id: i32,
    form: Form<Contextual<'_, ActionForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_some() {
        let mut transaction = pool.begin().await?;
        let export = ExportConfig::from_id_for_update(&mut transaction, export_id)
            .await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        if export.series != series || export.event != event {
            return Err(StatusOrError::Status(Status::NotFound));
        }
        sqlx::query!(
            "UPDATE speedgaming_exports SET enabled = false, updated_at = NOW() WHERE id = $1",
            export_id
        )
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
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(value) = form.value {
        sqlx::query(
            "UPDATE speedgaming_race_exports re SET last_attempt_at=NULL
            FROM speedgaming_exports e WHERE e.id=re.export_id AND e.series=$1 AND e.event=$2
              AND re.state IN ('failed','ambiguous')
              AND ($3::BIGINT IS NULL OR re.race_id=$3)",
        )
        .bind(series)
        .bind(event)
        .bind(value.race_id)
        .execute(pool.inner())
        .await?;
        speedgaming_export::schedule_sync(pool.inner().clone(), http_client.inner().clone());
    }
    Ok(Redirect::to(uri!(get(series, event))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_list_excludes_successes_ignored_errors_and_other_exports() {
        let deliveries = [
            (1, "race", "Failed qualifier", true),
            (1, "volunteer", "Failed commentator", true),
            (1, "race", "Successful qualifier", false),
            (1, "race", "Ignored qualifier", false),
            (2, "race", "Other event qualifier", true),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(id, (export_id, kind, description, needs_attention))| Delivery {
                export_id,
                kind: kind.into(),
                id: id as i64,
                race_id: Some(123),
                description: description.into(),
                episode_id: Some(124),
                status: "failed".into(),
                last_error: Some("Export error".into()),
                succeeded: false,
                needs_attention,
            },
        )
        .collect_vec();
        let html = delivery_issues(&deliveries, 1, Series::Crosskeys, "test", None).0;
        assert!(html.contains("Exports needing attention (2)"));
        assert!(html.contains("Failed qualifier"));
        assert!(html.contains("Failed commentator"));
        assert!(!html.contains("Successful qualifier"));
        assert!(!html.contains("Ignored qualifier"));
        assert!(!html.contains("Other event qualifier"));
        assert!(!html.contains("<details open"));
        assert!(html.contains("Ignore all issues"));
        assert_eq!(html.matches(">Ignore</button>").count(), 2);
        assert_eq!(html.matches(">Retry</button>").count(), 2);
        let html = delivery_issues(&[], 1, Series::Crosskeys, "test", None).0;
        assert!(html.contains("No export issues."));
        assert!(!html.contains("<table"));
        assert!(!html.contains("Ignore all issues"));
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated *_test database"]
    async fn retry_and_ignore_preserve_remote_ids_and_event_scope() {
        let pool = event::configuration::test_pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let key = rng().random_range(1_000_000_i64..10_000_000);
        let event = format!("sg{key}");
        let other_event = format!("so{key}");
        let race_id = key * 100;
        let signup_id = key as i32;
        for slug in [&event, &other_event] {
            sqlx::query("INSERT INTO events (series, event, display_name, team_config) VALUES ('xkeys', $1, 'SpeedGaming issue test', 'solo')")
                .bind(slug).execute(&mut *transaction).await.unwrap();
        }
        let mut export_ids = Vec::new();
        for slug in [&event, &other_event] {
            export_ids.push(sqlx::query_scalar::<_, i32>("INSERT INTO speedgaming_exports (series, event, slug, trigger_condition) VALUES ('xkeys', $1, 'test', 'when_scheduled') RETURNING id")
                .bind(slug).fetch_one(&mut *transaction).await.unwrap());
        }
        let export_id = export_ids[0];
        // Delivery records intentionally outlive deleted races and signups.
        for id in &export_ids {
            sqlx::query("INSERT INTO speedgaming_race_exports (race_id, export_id, episode_id, state, operation, last_error, attempt_count) VALUES ($1, $2, 124, 'failed', 'delete', 'Deletion rejected', 3), ($1 + 1, $2, 125, 'succeeded', 'create', NULL, 1)")
                .bind(race_id).bind(id).execute(&mut *transaction).await.unwrap();
            sqlx::query("INSERT INTO speedgaming_volunteer_exports (signup_id, export_id, episode_id, remote_id, role, state, last_error, attempt_count) VALUES ($1, $2, 124, 126, 'Commentary', 'ambiguous', 'Uncertain submission', 2)")
                .bind(signup_id).bind(id).execute(&mut *transaction).await.unwrap();
        }
        let before = deliveries(&mut transaction, Series::Crosskeys, &event)
            .await
            .unwrap();
        assert_eq!(before.iter().filter(|d| d.needs_attention).count(), 2);
        let html = delivery_issues(&before, export_id, Series::Crosskeys, &event, None).0;
        assert!(html.contains("Deletion rejected"));
        assert!(html.contains("Uncertain submission"));
        assert!(
            !html.contains(&format!("Race {}", race_id + 1)),
            "successful exports must not be listed"
        );
        assert!(!html.contains("<details open"), "issues start collapsed");

        resolve_delivery_issues(
            &mut transaction,
            export_id,
            Some("race"),
            Some(race_id),
            false,
        )
        .await
        .unwrap();
        let after = deliveries(&mut transaction, Series::Crosskeys, &event)
            .await
            .unwrap();
        assert_eq!(after.iter().filter(|d| d.needs_attention).count(), 1);
        let tracking: (Option<i64>, String, String, i32) = sqlx::query_as("SELECT episode_id, state::text, operation, attempt_count FROM speedgaming_race_exports WHERE export_id = $1 AND race_id = $2")
            .bind(export_id).bind(race_id).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!(tracking, (Some(124), "ignored".into(), "delete".into(), 3));

        // A later race edit cannot revive an ignored export.
        sqlx::query("UPDATE speedgaming_race_exports SET operation = 'update', state = 'pending' WHERE export_id = $1 AND race_id = $2")
            .bind(export_id).bind(race_id).execute(&mut *transaction).await.unwrap();
        let state: String = sqlx::query_scalar("SELECT state::text FROM speedgaming_race_exports WHERE export_id = $1 AND race_id = $2")
            .bind(export_id).bind(race_id).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!(state, "ignored");

        // An explicit retry releases uncertain volunteer submissions for another attempt.
        resolve_delivery_issues(
            &mut transaction,
            export_id,
            Some("volunteer"),
            Some(i64::from(signup_id)),
            true,
        )
        .await
        .unwrap();
        let tracking: (Option<i64>, Option<i64>, String, bool) = sqlx::query_as("SELECT episode_id, remote_id, state::text, last_attempt_at IS NULL FROM speedgaming_volunteer_exports WHERE export_id = $1 AND signup_id = $2")
            .bind(export_id).bind(signup_id).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!(tracking, (Some(124), Some(126), "pending".into(), true));
        sqlx::query("UPDATE speedgaming_volunteer_exports SET state = 'failed' WHERE export_id = $1 AND signup_id = $2")
            .bind(export_id).bind(signup_id).execute(&mut *transaction).await.unwrap();

        resolve_delivery_issues(&mut transaction, export_id, None, None, false)
            .await
            .unwrap();
        let after = deliveries(&mut transaction, Series::Crosskeys, &event)
            .await
            .unwrap();
        assert_eq!(after.len(), before.len());
        assert!(after.iter().all(|d| !d.needs_attention));
        let tracking: (Option<i64>, Option<i64>, String, i32) = sqlx::query_as("SELECT episode_id, remote_id, state::text, attempt_count FROM speedgaming_volunteer_exports WHERE export_id = $1 AND signup_id = $2")
            .bind(export_id).bind(signup_id).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!(tracking, (Some(124), Some(126), "ignored".into(), 2));
        assert_eq!(
            deliveries(&mut transaction, Series::Crosskeys, &other_event)
                .await
                .unwrap()
                .iter()
                .filter(|d| d.needs_attention)
                .count(),
            2
        );
        transaction.rollback().await.unwrap();
    }
}
