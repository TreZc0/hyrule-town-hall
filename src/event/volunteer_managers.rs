use crate::{
    event::{
        Data, Tab,
        roles::{self, RoleRequest},
    },
    id::RoleRequests,
    prelude::*,
};

#[cfg(test)]
mod tests;

/// Separate volunteer capabilities from race editing and event configuration.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Permissions {
    pub(crate) manage_managers: bool,
    pub(crate) review_roles: bool,
    pub(crate) view_signups: bool,
    pub(crate) decide_signups: bool,
    pub(crate) revert_signups: bool,
}

impl Permissions {
    fn from_roles(organizer: bool, coordinator: bool, manager: bool, manage_signups: bool) -> Self {
        let staff = organizer || coordinator;
        Self {
            manage_managers: organizer,
            review_roles: organizer || manager,
            view_signups: staff || manager,
            decide_signups: staff || (manager && manage_signups),
            revert_signups: staff,
        }
    }

    pub(crate) async fn load(
        transaction: &mut Transaction<'_, Postgres>,
        data: &Data<'_>,
        user: &User,
    ) -> Result<Self, event::Error> {
        let organizer =
            user.is_global_admin() || data.organizers(transaction).await?.contains(user);
        let mut coordinator = data.restreamers(transaction).await?.contains(user);
        if !organizer && !coordinator {
            if let Some(game) = data.game(transaction).await? {
                coordinator = game.is_restreamer_any_language(transaction, user).await?;
            }
        }
        let manager = is_manager(transaction, data, user).await?;
        Ok(Self::from_roles(
            organizer,
            coordinator,
            manager,
            data.volunteer_managers_can_manage_signups,
        ))
    }
}

pub(crate) async fn is_manager(
    transaction: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    user: &User,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM event_volunteer_managers WHERE series = $1 AND event = $2 AND user_id = $3)")
        .bind(data.series.to_string()).bind(&*data.event).bind(i64::from(user.id))
        .fetch_one(&mut **transaction).await
}

async fn can_configure(
    transaction: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    me: &User,
) -> Result<bool, event::Error> {
    Ok(me.is_global_admin() || data.organizers(transaction).await?.contains(me))
}

async fn settings_page(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    data: Data<'_>,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    if !can_configure(&mut transaction, &data, &me).await? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let ids: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM event_volunteer_managers WHERE series = $1 AND event = $2 ORDER BY user_id")
        .bind(data.series.to_string()).bind(&*data.event).fetch_all(&mut *transaction).await?;
    let mut managers = Vec::new();
    for id in ids {
        if let Some(user) = User::from_id(&mut *transaction, Id::from(id)).await? {
            managers.push(user);
        }
    }
    managers.sort_by_key(|user| user.display_name().to_string());
    let sources: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT e.series, e.event, e.display_name FROM events e WHERE (e.series, e.event) != ($1, $2) AND ($3 OR EXISTS (SELECT 1 FROM organizers o WHERE o.series = e.series AND o.event = e.event AND o.organizer = $4)) ORDER BY e.display_name, e.series, e.event")
        .bind(data.series.to_string()).bind(&*data.event).bind(me.is_global_admin()).bind(i64::from(me.id))
        .fetch_all(&mut *transaction).await?;
    let header = data
        .header(&mut transaction, Some(&me), Tab::Configure, true)
        .await?;
    let content = html! {
        : header;
        article {
            h2 : "Manage volunteer managers";
            p : "Volunteer managers review event-specific role requests in all languages. You can also allow them to confirm or decline race signups.";
            @for error in ctx.errors() { p(class = "error") : error; }
            @if data.is_ended() {
                p : "This event has ended and can no longer be configured.";
            } else {
                h3 : "Race signup permissions";
                : full_form(uri!(save_settings(data.series, &*data.event)), csrf, html! {
                    label {
                        input(type = "checkbox", name = "allow_signups", checked? = data.volunteer_managers_can_manage_signups);
                        : " Allow volunteer managers to confirm or decline race signups";
                    }
                }, Vec::new(), "Save");
                h3 : "Current volunteer managers";
                @if managers.is_empty() { p : "No volunteer managers assigned."; }
                @for manager in &managers {
                    div(class = "signup-item") {
                        : manager.to_html();
                        @let (errors, button) = button_form_ext(
                            uri!(remove_manager(data.series, &*data.event)), csrf, Vec::new(),
                            html! { input(type = "hidden", name = "user_id", value = manager.id.to_string()); }, "Remove");
                        : errors;
                        : button;
                    }
                }
                h3 : "Add volunteer manager";
                : full_form(uri!(add_manager(data.series, &*data.event)), csrf, html! {
                    label(for = "volunteer-manager") : "Volunteer manager:";
                    div(class = "autocomplete-container") {
                        input(type = "text", id = "volunteer-manager", name = "user_id", autocomplete = "off", required);
                        div(id = "user-suggestions", class = "suggestions", style = "display: none;") {}
                    }
                    p(class = "help") : "Start typing a username and select an account, or enter its HTH user ID.";
                }, Vec::new(), "Add");
                h3 : "Copy from another event";
                : full_form(uri!(copy_managers(data.series, &*data.event)), csrf, html! {
                    label(for = "source_event") : "Copy volunteer managers from:";
                    select(id = "source_event", name = "source_event", required) {
                        option(value = "") : "Select event";
                        @for (series, event, name) in &sources {
                            option(value = format!("{series}/{event}")) : format!("{name} ({series}/{event})");
                        }
                    }
                    p(class = "help") : "Adds the selected event's volunteer managers, keeping existing managers and skipping duplicates. This event's race signup permission stays unchanged.";
                }, Vec::new(), "Copy");
                script(src = static_url!("user-search.js")) {}
            }
        }
    };
    Ok(page(
        transaction,
        &Some(me),
        &uri,
        PageStyle::default(),
        &format!("Volunteer managers — {}", data.display_name),
        content,
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/configure/volunteer-managers")]
pub(crate) async fn settings(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    settings_page(
        transaction,
        me,
        data,
        uri,
        csrf.as_ref(),
        Context::default(),
    )
    .await
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ManagerForm {
    #[field(default = String::new())]
    csrf: String,
    user_id: Id<Users>,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct SettingsForm {
    #[field(default = String::new())]
    csrf: String,
    allow_signups: bool,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct CopyForm {
    #[field(default = String::new())]
    csrf: String,
    source_event: String,
}

enum Change {
    Add(Id<Users>),
    Remove(Id<Users>),
    Settings(bool),
    Copy(String),
}

async fn change(
    pool: &PgPool,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    mut ctx: Context<'_>,
    change: Option<Change>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !can_configure(&mut transaction, &data, &me).await? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    if data.is_ended() {
        ctx.push_error(form::Error::validation(
            "This event has ended and can no longer be configured.",
        ));
    }
    if ctx.errors().next().is_none() {
        if let Some(change) = change {
            match change {
                Change::Add(id) => {
                    let exists: bool =
                        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)")
                            .bind(i64::from(id))
                            .fetch_one(&mut *transaction)
                            .await?;
                    if exists {
                        sqlx::query("INSERT INTO event_volunteer_managers (series, event, user_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                            .bind(series.to_string()).bind(event).bind(i64::from(id)).execute(&mut *transaction).await?;
                    } else {
                        ctx.push_error(form::Error::validation("Select an existing HTH user."));
                    }
                }
                Change::Remove(id) => {
                    sqlx::query("DELETE FROM event_volunteer_managers WHERE series = $1 AND event = $2 AND user_id = $3")
                        .bind(series.to_string()).bind(event).bind(i64::from(id)).execute(&mut *transaction).await?;
                }
                Change::Settings(allow) => {
                    sqlx::query("UPDATE events SET volunteer_managers_can_manage_signups = $1 WHERE series = $2 AND event = $3")
                        .bind(allow).bind(series.to_string()).bind(event).execute(&mut *transaction).await?;
                }
                Change::Copy(source) => {
                    if let Some((source_series, source_event)) = source
                        .split_once('/')
                        .and_then(|(s, e)| s.parse::<Series>().ok().map(|s| (s, e)))
                    {
                        if source_series == series && source_event == event {
                            ctx.push_error(form::Error::validation(
                                "Select a different source event.",
                            ));
                        } else if let Some(source_data) =
                            Data::new(&mut transaction, source_series, source_event).await?
                        {
                            if !can_configure(&mut transaction, &source_data, &me).await? {
                                return Err(StatusOrError::Status(Status::Forbidden));
                            }
                            copy_members(
                                &mut transaction,
                                series,
                                event,
                                source_series,
                                source_event,
                            )
                            .await?;
                        } else {
                            ctx.push_error(form::Error::validation(
                                "The source event does not exist.",
                            ));
                        }
                    } else {
                        ctx.push_error(form::Error::validation("Select a source event."));
                    }
                }
            }
            if ctx.errors().next().is_none() {
                transaction.commit().await?;
                return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(settings(
                    series, event
                )))));
            }
        }
    }
    Ok(RedirectOrContent::Content(
        settings_page(transaction, me, data, uri, csrf.as_ref(), ctx).await?,
    ))
}

async fn copy_members(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    source_series: Series,
    source_event: &str,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO event_volunteer_managers (series, event, user_id) SELECT $1, $2, user_id FROM event_volunteer_managers WHERE series = $3 AND event = $4 ON CONFLICT DO NOTHING")
        .bind(series.to_string()).bind(event).bind(source_series.to_string()).bind(source_event).execute(&mut **transaction).await?;
    Ok(())
}

#[rocket::post(
    "/event/<series>/<event>/configure/volunteer-managers/add",
    data = "<form>"
)]
pub(crate) async fn add_manager(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, ManagerForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    let action = form.value.map(|value| Change::Add(value.user_id));
    change(pool, me, uri, csrf, series, event, form.context, action).await
}

#[rocket::post(
    "/event/<series>/<event>/configure/volunteer-managers/remove",
    data = "<form>"
)]
pub(crate) async fn remove_manager(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, ManagerForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    let action = form.value.map(|value| Change::Remove(value.user_id));
    change(pool, me, uri, csrf, series, event, form.context, action).await
}

#[rocket::post(
    "/event/<series>/<event>/configure/volunteer-managers/settings",
    data = "<form>"
)]
pub(crate) async fn save_settings(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, SettingsForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    let action = form
        .value
        .map(|value| Change::Settings(value.allow_signups));
    change(pool, me, uri, csrf, series, event, form.context, action).await
}

#[rocket::post(
    "/event/<series>/<event>/configure/volunteer-managers/copy-from",
    data = "<form>"
)]
pub(crate) async fn copy_managers(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, CopyForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    let action = form.value.map(|value| Change::Copy(value.source_event));
    change(pool, me, uri, csrf, series, event, form.context, action).await
}

pub(crate) async fn review_page(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    data: Data<'_>,
    uri: &Origin<'_>,
    csrf: Option<&CsrfToken>,
    ctx: &Context<'_>,
    msg: Option<&str>,
) -> Result<RawHtml<String>, roles::Error> {
    let permissions = Permissions::load(&mut transaction, &data, &me).await?;
    let header = data
        .header(&mut transaction, Some(&me), Tab::VolunteerManagement, false)
        .await?;
    let content = if permissions.review_roles {
        let requests =
            RoleRequest::pending_for_event(&mut transaction, data.series, &data.event).await?;
        html! {
            h2 : "Volunteer management";
            @for error in ctx.errors() { p(class = "error") : error; }
            @if let Some(msg) = msg { p(class = "success") : msg; }
            @if data.is_ended() { p : "This event has ended. Volunteer decisions are closed."; }
            @if !data.force_custom_role_binding {
                p : "This event uses shared game roles. Role requests are reviewed by game staff.";
            }
            @if permissions.decide_signups {
                p : "Open a race's volunteer signups to confirm or decline pending volunteers.";
            } else {
                p : "You can view race signups. Organizers have not enabled race signup decisions for volunteer managers.";
            }
            p { a(href = uri!(event::races(data.series, &*data.event))) : "View races"; }
            h3 : "Pending event role requests";
            @if requests.is_empty() { p : "No pending event-specific role requests."; }
            @for request in requests {
                @if let Some(user) = User::from_id(&mut *transaction, request.user_id).await? {
                    div(class = "signup-item") {
                        p { : user.to_html(); : format!(" — {} ({})", request.role_type_name, request.language); }
                        @if let Some(notes) = request.notes { p : notes; }
                        @if !data.is_ended() {
                            @let (errors, approve) = button_form(uri!(roles::approve_role_request(data.series, &*data.event, request.id)), csrf, Vec::new(), "Approve");
                            : errors; : approve;
                            @let (errors, reject) = button_form(uri!(roles::reject_role_request(data.series, &*data.event, request.id)), csrf, Vec::new(), "Reject");
                            : errors; : reject;
                        }
                    }
                }
            }
        }
    } else {
        html! { p : "You must be an organizer or volunteer manager to review this event's role requests."; }
    };
    Ok(page(
        transaction,
        &Some(me),
        uri,
        PageStyle::default(),
        &format!("Volunteer management — {}", data.display_name),
        html! { : header; article { : content; } },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/volunteer-management?<msg>")]
pub(crate) async fn review(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    msg: Option<&str>,
) -> Result<RawHtml<String>, StatusOrError<roles::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !Permissions::load(&mut transaction, &data, &me)
        .await?
        .review_roles
    {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    Ok(review_page(
        transaction,
        me,
        data,
        &uri,
        csrf.as_ref(),
        &Context::default(),
        msg,
    )
    .await?)
}
