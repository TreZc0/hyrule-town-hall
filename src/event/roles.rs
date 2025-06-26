use crate::{
    cal::{self, Entrant, Entrants, Race, RaceSchedule},
    event::{Data, Tab},
    form::{EmptyForm, button_form, form_field, full_form},
    http::{PageError, StatusOrError},
    id::{RoleBindings, RoleRequests, RoleTypes, Signups},
    prelude::*,
    time::format_datetime,
    user::User,
};

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Event(#[from] event::Error),
    #[error(transparent)]
    Data(#[from] event::DataError),
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Wheel(#[from] wheel::Error),
    #[error(transparent)]
    Cal(#[from] cal::Error),
}

impl From<Error> for StatusOrError<Error> {
    fn from(err: Error) -> Self {
        StatusOrError::Err(err)
    }
}

impl From<sqlx::Error> for StatusOrError<Error> {
    fn from(err: sqlx::Error) -> Self {
        StatusOrError::Err(Error::Sqlx(err))
    }
}

impl From<event::DataError> for StatusOrError<Error> {
    fn from(err: event::DataError) -> Self {
        StatusOrError::Err(Error::Data(err))
    }
}

impl From<event::Error> for StatusOrError<Error> {
    fn from(err: event::Error) -> Self {
        StatusOrError::Err(Error::Event(err))
    }
}

impl From<PageError> for StatusOrError<Error> {
    fn from(err: PageError) -> Self {
        StatusOrError::Err(Error::Page(err))
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "role_request_status", rename_all = "lowercase")]
pub(crate) enum RoleRequestStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "signup_status", rename_all = "lowercase")]
pub(crate) enum SignupStatus {
    Pending,
    Confirmed,
    Declined,
}

#[derive(Debug, Clone)]
pub(crate) struct RoleType {
    pub(crate) id: Id<RoleTypes>,
    pub(crate) name: String,
}

#[allow(unused)]
pub(crate) struct RoleBinding {
    pub(crate) id: Id<RoleBindings>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) role_type_id: Id<RoleTypes>,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) struct RoleRequest {
    pub(crate) id: Id<RoleRequests>,
    pub(crate) role_binding_id: Id<RoleBindings>,
    pub(crate) user_id: Id<Users>,
    pub(crate) status: RoleRequestStatus,
    pub(crate) notes: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
}

#[allow(unused)]
pub(crate) struct Signup {
    pub(crate) id: Id<Signups>,
    pub(crate) race_id: Id<Races>,
    pub(crate) role_binding_id: Id<RoleBindings>,
    pub(crate) user_id: Id<Users>,
    pub(crate) status: SignupStatus,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
}

impl RoleType {
    #[allow(unused)]
    pub(crate) async fn from_id(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<RoleTypes>,
    ) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(
            Self,
            r#"SELECT id AS "id: Id<RoleTypes>", name FROM role_types WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut **pool)
        .await
    }

    pub(crate) async fn all(pool: &mut Transaction<'_, Postgres>) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"SELECT id AS "id: Id<RoleTypes>", name FROM role_types ORDER BY name"#
        )
        .fetch_all(&mut **pool)
        .await
    }
}

impl RoleBinding {
    pub(crate) async fn for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT
                    rb.id AS "id: Id<RoleBindings>",
                    rb.series AS "series: Series",
                    rb.event,
                    rb.role_type_id AS "role_type_id: Id<RoleTypes>",
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name"
                FROM role_bindings rb
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.series = $1 AND rb.event = $2
                ORDER BY rt.name
            "#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await
    }

    pub(crate) async fn create(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_type_id: Id<RoleTypes>,
        min_count: i32,
        max_count: i32,
    ) -> sqlx::Result<Id<RoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_bindings (series, event, role_type_id, min_count, max_count) 
               VALUES ($1, $2, $3, $4, $5) RETURNING id"#,
            series as _,
            event,
            role_type_id as _,
            min_count,
            max_count
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(Id::from(id as i64))
    }

    pub(crate) async fn delete(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<RoleBindings>,
    ) -> sqlx::Result<()> {
        sqlx::query!("DELETE FROM role_bindings WHERE id = $1", id as _)
            .execute(&mut **pool)
            .await?;
        Ok(())
    }
}

impl RoleRequest {
    pub(crate) async fn for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT 
                    rr.id AS "id: Id<RoleRequests>",
                    rr.role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    rr.user_id AS "user_id: Id<Users>",
                    rr.status AS "status: RoleRequestStatus",
                    rr.notes,
                    rr.created_at,
                    rr.updated_at,
                    rb.series AS "series: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name"
                FROM role_requests rr
                JOIN role_bindings rb ON rr.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.series = $1 AND rb.event = $2
                ORDER BY rt.name, rr.created_at
            "#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await
    }

    pub(crate) async fn create(
        pool: &mut Transaction<'_, Postgres>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
        notes: String,
    ) -> sqlx::Result<Id<RoleRequests>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_requests (role_binding_id, user_id, notes)
               VALUES ($1, $2, $3) RETURNING id"#,
            role_binding_id as _,
            user_id as _,
            notes
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(Id::from(id as i64))
    }

    pub(crate) async fn update_status(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<RoleRequests>,
        status: RoleRequestStatus,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"UPDATE role_requests SET status = $1, updated_at = NOW() WHERE id = $2"#,
            status as _,
            id as _
        )
        .execute(&mut **pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn exists_for_user(
        pool: &mut Transaction<'_, Postgres>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_requests
                   WHERE role_binding_id = $1 AND user_id = $2)"#,
            role_binding_id as _,
            user_id as _
        )
        .fetch_one(&mut **pool)
        .await?
        .unwrap_or(false))
    }

    pub(crate) async fn approved_for_user(
        pool: &mut Transaction<'_, Postgres>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_requests
                   WHERE role_binding_id = $1 AND user_id = $2 AND status = 'approved')"#,
            role_binding_id as _,
            user_id as _
        )
        .fetch_one(&mut **pool)
        .await?
        .unwrap_or(false))
    }

    pub(crate) async fn pending_for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT 
                    rr.id AS "id: Id<RoleRequests>",
                    rr.role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    rr.user_id AS "user_id: Id<Users>",
                    rr.status AS "status: RoleRequestStatus",
                    rr.notes,
                    rr.created_at,
                    rr.updated_at,
                    rb.series AS "series: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name"
                FROM role_requests rr
                JOIN role_bindings rb ON rr.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.series = $1 AND rb.event = $2 AND rr.status = 'pending'
                ORDER BY rt.name, rr.created_at
            "#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await
    }
}

impl Signup {
    pub(crate) async fn for_race(
        pool: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT 
                    s.id AS "id: Id<Signups>",
                    s.race_id AS "race_id: Id<Races>",
                    s.role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    s.user_id AS "user_id: Id<Users>",
                    s.status AS "status: SignupStatus",
                    s.created_at,
                    s.updated_at,
                    rb.series AS "series: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name"
                FROM signups s
                JOIN role_bindings rb ON s.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE s.race_id = $1
                ORDER BY rt.name, s.created_at
            "#,
            race_id as _
        )
        .fetch_all(&mut **pool)
        .await
    }

    pub(crate) async fn create(
        pool: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
    ) -> sqlx::Result<Id<Signups>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO signups (race_id, role_binding_id, user_id)
               VALUES ($1, $2, $3) RETURNING id"#,
            race_id as _,
            role_binding_id as _,
            user_id as _
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(Id::from(id as i64))
    }

    pub(crate) async fn update_status(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<Signups>,
        status: SignupStatus,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"UPDATE signups SET status = $1, updated_at = NOW() WHERE id = $2"#,
            status as _,
            id as _
        )
        .execute(&mut **pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn exists_for_user(
        pool: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM signups
                   WHERE race_id = $1 AND role_binding_id = $2 AND user_id = $3)"#,
            race_id as _,
            role_binding_id as _,
            user_id as _
        )
        .fetch_one(&mut **pool)
        .await?
        .unwrap_or(false))
    }
}

async fn roles_page(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    uri: &Origin<'_>,
    csrf: Option<&CsrfToken>,
    data: Data<'_>,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Roles, false)
        .await?;

    let content = if let Some(ref me) = me {
        if data.organizers(&mut transaction).await?.contains(me) {
            let role_bindings =
                RoleBinding::for_event(&mut transaction, data.series, &data.event).await?;
            let pending_requests =
                RoleRequest::pending_for_event(&mut transaction, data.series, &data.event).await?;
            let all_role_types = RoleType::all(&mut transaction).await?;

            html! {
                h2 : "Role Management";
                p : "Manage volunteer roles for this event.";

                h3 : "Current Role Bindings";
                @if role_bindings.is_empty() {
                    p : "No role bindings configured yet.";
                } else {
                    table {
                        thead {
                            tr {
                                th : "Role Type";
                                th : "Min Count";
                                th : "Max Count";
                                th;
                            }
                        }
                        tbody {
                            @for binding in role_bindings {
                                tr {
                                    td : binding.role_type_name;
                                    td : binding.min_count;
                                    td : binding.max_count;
                                    td {
                                        @let errors = ctx.errors().collect_vec();
                                        @let (errors, button) = button_form(uri!(delete_role_binding(data.series, &*data.event, binding.id)), csrf, errors, "Delete");
                                        : errors;
                                        div(class = "button-row") : button;
                                    }
                                }
                            }
                        }
                    }
                }

                h3 : "Add Role Binding";
                @let mut errors = ctx.errors().collect_vec();
                : full_form(uri!(add_role_binding(data.series, &*data.event)), csrf, html! {
                    : form_field("role_type_id", &mut errors, html! {
                        label(for = "role_type_id") : "Role Type:";
                        select(name = "role_type_id", id = "role_type_id") {
                            @for role_type in all_role_types {
                                option(value = role_type.id.to_string()) : role_type.name;
                            }
                        }
                    });
                    : form_field("min_count", &mut errors, html! {
                        label(for = "min_count") : "Minimum Count:";
                        input(type = "number", name = "min_count", id = "min_count", value = "1", min = "1");
                    });
                    : form_field("max_count", &mut errors, html! {
                        label(for = "max_count") : "Maximum Count:";
                        input(type = "number", name = "max_count", id = "max_count", value = "1", min = "1");
                    });
                }, errors, "Add Role Binding");

                h3 : "Pending Role Requests";
                @if pending_requests.is_empty() {
                    p : "No pending role requests.";
                } else {
                    table {
                        thead {
                            tr {
                                th : "User";
                                th : "Role Type";
                                th : "Notes";
                                th : "Requested";
                                th : "Actions";
                            }
                        }
                        tbody {
                            @for request in pending_requests {
                                tr {
                                    td {
                                        @let user = User::from_id(&mut *transaction, request.user_id).await?;
                                        : user.map_or_else(|| request.user_id.to_string(), |u| u.to_string());
                                    }
                                    td : request.role_type_name;
                                    td {
                                        @if let Some(ref notes) = request.notes {
                                            : notes;
                                        } else {
                                            : "No notes";
                                        }
                                    }
                                    td : format_datetime(request.created_at, DateTimeFormat { long: true, running_text: false });
                                    td {
                                        @let errors = Vec::new();
                                        @let (errors, approve_button) = button_form(uri!(approve_role_request(data.series, &*data.event, request.id)), csrf, errors, "Approve");
                                        : errors;
                                        : approve_button;
                                        @let errors = Vec::new();
                                        @let (errors, reject_button) = button_form(uri!(reject_role_request(data.series, &*data.event, request.id)), csrf, errors, "Reject");
                                        : errors;
                                        : reject_button;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            html! {
                article {
                    p : "This page is for organizers of this event only.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(get(data.series, &*data.event)))))) : "Sign in";
                    : " to manage roles for this event.";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        uri,
        PageStyle {
            chests: data.chests().await?,
            ..PageStyle::default()
        },
        &format!("Roles — {}", data.display_name),
        html! {
            : header;
            : content;
        },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/roles")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    Ok(roles_page(transaction, me, &uri, csrf.as_ref(), data, ctx).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
    role_type_id: Id<RoleTypes>,
    min_count: i32,
    max_count: i32,
}

#[rocket::post("/event/<series>/<event>/roles/add-binding", data = "<form>")]
pub(crate) async fn add_role_binding(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, AddRoleBindingForm>>
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to manage roles for this event.",
            ));
        }
        if value.min_count > value.max_count {
            form.context.push_error(form::Error::validation(
                "Minimum count cannot be greater than maximum count.",
            ));
        }
        if value.min_count < 1 {
            form.context
                .push_error(form::Error::validation("Minimum count must be at least 1."));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    form.context,
                )
                .await?,
            )
        } else {
            RoleBinding::create(
                &mut transaction,
                data.series,
                &data.event,
                value.role_type_id,
                value.min_count,
                value.max_count,
            )
            .await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                form.context,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/<binding>/delete", data = "<form>")]
pub(crate) async fn delete_role_binding(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    binding: Id<RoleBindings>,
    form: Form<Contextual<'_, EmptyForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to manage roles for this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    form.context,
                )
                .await?,
            )
        } else {
            RoleBinding::delete(&mut transaction, binding).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                form.context,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/<request>/approve", data = "<form>")]
pub(crate) async fn approve_role_request(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    request: Id<RoleRequests>,
    form: Form<Contextual<'_, EmptyForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to manage roles for this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    form.context,
                )
                .await?,
            )
        } else {
            RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Approved)
                .await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                form.context,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/<request>/reject", data = "<form>")]
pub(crate) async fn reject_role_request(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    request: Id<RoleRequests>,
    form: Form<Contextual<'_, EmptyForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to manage roles for this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    form.context,
                )
                .await?,
            )
        } else {
            RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Rejected)
                .await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ApplyForRoleForm {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
    #[field(default = String::new())]
    notes: String,
}

#[rocket::post("/event/<series>/<event>/volunteer-roles/apply", data = "<form>")]
pub(crate) async fn apply_for_role(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, ApplyForRoleForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer accept volunteer applications",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                volunteer_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    form.context,
                )
                .await?,
            )
        } else {
            let notes = if value.notes.is_empty() {
                None
            } else {
                Some(value.notes.clone())
            };

            if RoleRequest::exists_for_user(&mut transaction, value.role_binding_id, me.id).await? {
                form.context.push_error(form::Error::validation(
                    "You have already applied for this role",
                ));
                RedirectOrContent::Content(
                    volunteer_page(
                        transaction,
                        Some(me),
                        &uri,
                        csrf.as_ref(),
                        data,
                        form.context,
                    )
                    .await?,
                )
            } else {
                RoleRequest::create(
                    &mut transaction,
                    value.role_binding_id,
                    me.id,
                    notes.unwrap_or_default(),
                )
                .await?;
                transaction.commit().await?;
                RedirectOrContent::Redirect(Redirect::to(uri!(volunteer_page_get(series, event))))
            }
        }
    } else {
        RedirectOrContent::Content(
            volunteer_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                form.context,
            )
            .await?,
        )
    })
}

async fn volunteer_page(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    uri: &Origin<'_>,
    csrf: Option<&CsrfToken>,
    data: Data<'_>,
    _ctx: Context<'_>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Volunteer, false)
        .await?;

    let content = if let Some(ref me) = me {
        let role_bindings =
            RoleBinding::for_event(&mut transaction, data.series, &data.event).await?;
        let my_requests = RoleRequest::for_event(&mut transaction, data.series, &data.event)
            .await?
            .into_iter()
            .filter(|req| req.user_id == me.id)
            .collect::<Vec<_>>();

        println!("my_requests: {:?}", my_requests);
        let my_approved_roles: Vec<_> = my_requests
            .iter()
            .filter(|req| matches!(req.status, RoleRequestStatus::Approved))
            .collect();
        println!("my_approved_roles: {:?}", my_approved_roles);

        // Get upcoming races for this event
        let all_races = Race::for_event(&mut transaction, &reqwest::Client::new(), &data).await?;
        let upcoming_races: Vec<_> = all_races
            .into_iter()
            .filter(|race| {
                match race.schedule {
                    RaceSchedule::Unscheduled => false, 
                    RaceSchedule::Live { end, .. } => {
                        end.is_none_or(|end_time| end_time > Utc::now())
                    }
                    RaceSchedule::Async { start1, start2, start3, end1, end2, end3, .. } => {
                        let has_started = start1.is_some() || start2.is_some() || start3.is_some();
                        let is_finished = match race.entrants {
                            Entrants::Two(_) => end1.is_some() && end2.is_some(),
                            Entrants::Three(_) => end1.is_some() && end2.is_some() && end3.is_some(),
                            _ => false,
                        };
                        has_started && !is_finished
                    }
                }
            })
            .collect();

        html! {
            h2 : "Volunteer for Roles";
            p : "Apply to volunteer for roles in this event.";

            @if role_bindings.is_empty() {
                p : "No volunteer roles are currently available for this event.";
            } else {
                h3 : "Available Roles";
                @for binding in role_bindings {
                    @let my_request = my_requests.iter().find(|req| req.role_binding_id == binding.id);
                    div(class = "role-binding") {
                        h4 : binding.role_type_name;
                        p {
                            : "Selected per restream: ";
                            : binding.min_count;
                            : " - ";
                            : binding.max_count;
                            : " volunteers";
                        }
                        @if let Some(request) = my_request {
                            p {
                                : "Your application status: ";
                                @match request.status {
                                    RoleRequestStatus::Pending => : "Pending";
                                    RoleRequestStatus::Approved => : "Approved";
                                    RoleRequestStatus::Rejected => : "Rejected";
                                }
                            }
                        } else {
                            @let mut errors = Vec::new();
                            : full_form(uri!(apply_for_role(data.series, &*data.event)), csrf, html! {
                                input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                : form_field("notes", &mut errors, html! {
                                    label(for = "notes") : "Notes (optional):";
                                    textarea(name = "notes", id = "notes", rows = "3") : "";
                                });
                            }, errors, format!("Apply for {}", binding.role_type_name).as_str());
                        }
                    }
                }
            }

            @if !my_approved_roles.is_empty() && !upcoming_races.is_empty() {
                h3 : "Sign Up for Matches";
                p : "You are approved for the following roles. Click on a match to sign up as a volunteer:";
                
                @for role in my_approved_roles {
                    div(class = "approved-role") {
                        h4 : role.role_type_name;
                        p : "You are approved for this role.";
                        
                        @if upcoming_races.is_empty() {
                            p : "No upcoming matches available for signup.";
                        } else {
                            h5 : "Upcoming Matches:";
                            ul {
                                @for race in &upcoming_races {
                                    li {
                                        a(href = uri!(match_signup_page_get(data.series, &*data.event, race.id))) {
                                            : format!("{} {} {}",
                                                race.phase.as_deref().unwrap_or(""),
                                                race.round.as_deref().unwrap_or(""),
                                                match &race.entrants {
                                                    Entrants::Two([team1, team2]) => format!("{} vs {}",
                                                        match team1 {
                                                            Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                                            Entrant::Named { name, .. } => name.clone(),
                                                            Entrant::Discord { .. } => "Discord User".to_string(),
                                                        },
                                                        match team2 {
                                                            Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                                            Entrant::Named { name, .. } => name.clone(),
                                                            Entrant::Discord { .. } => "Discord User".to_string(),
                                                        }
                                                    ),
                                                    Entrants::Three([team1, team2, team3]) => format!("{} vs {} vs {}",
                                                        match team1 {
                                                            Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                                            Entrant::Named { name, .. } => name.clone(),
                                                            Entrant::Discord { .. } => "Discord User".to_string(),
                                                        },
                                                        match team2 {
                                                            Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                                            Entrant::Named { name, .. } => name.clone(),
                                                            Entrant::Discord { .. } => "Discord User".to_string(),
                                                        },
                                                        match team3 {
                                                            Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                                            Entrant::Named { name, .. } => name.clone(),
                                                            Entrant::Discord { .. } => "Discord User".to_string(),
                                                        }
                                                    ),
                                                    _ => "Unknown entrants".to_string(),
                                                }
                                            );
                                            @match race.schedule {
                                                RaceSchedule::Unscheduled => : " (Unscheduled)";
                                                RaceSchedule::Live { end, .. } => {
                                                    : " - ";
                                                    @if let Some(end) = end {
                                                        : format_datetime(end, DateTimeFormat { long: false, running_text: false });
                                                    } else {
                                                        : "(not yet ended)";
                                                    }
                                                }
                                                RaceSchedule::Async { .. } => : " (Async)";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !my_approved_roles.is_empty() && upcoming_races.is_empty() {
                h3 : "No Upcoming Matches";
                p : "You are approved for roles, but there are no upcoming matches available for signup.";
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(volunteer_page_get(data.series, &*data.event)))))) : "Sign in";
                    : " to view volunteer opportunities for this event.";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        uri,
        PageStyle {
            chests: data.chests().await?,
            ..PageStyle::default()
        },
        &data.display_name,
        html! {
            : header;
            : content;
        },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/volunteer-roles")]
pub(crate) async fn volunteer_page_get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    Ok(volunteer_page(transaction, me, &uri, csrf.as_ref(), data, ctx).await?)
}

// Match signup functionality
#[derive(FromForm, CsrfForm)]
pub(crate) struct SignupForMatchForm {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
}

#[rocket::post("/event/<series>/<event>/races/<race_id>/signup", data = "<form>")]
pub(crate) async fn signup_for_match(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, SignupForMatchForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer accept volunteer signups",
            ));
        }

        if !RoleRequest::approved_for_user(&mut transaction, value.role_binding_id, me.id).await? {
            form.context.push_error(form::Error::validation(
                "You must be approved for this role before you can sign up for matches",
            ));
        }

        if Signup::exists_for_user(&mut transaction, race_id, value.role_binding_id, me.id).await? {
            form.context.push_error(form::Error::validation(
                "You have already signed up for this role in this match",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                match_signup_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    race_id,
                    form.context,
                )
                .await?,
            )
        } else {
            Signup::create(&mut transaction, race_id, value.role_binding_id, me.id).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(match_signup_page_get(
                data.series,
                &*data.event,
                race_id
            ))))
        }
    } else {
        RedirectOrContent::Content(
            match_signup_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                race_id,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ManageRosterForm {
    #[field(default = String::new())]
    csrf: String,
    signup_id: Id<Signups>,
    #[field(default = String::new())]
    action: String,
}

#[rocket::post(
    "/event/<series>/<event>/races/<race_id>/manage-roster",
    data = "<form>"
)]
pub(crate) async fn manage_roster(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, ManageRosterForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be managed",
            ));
        }

        let is_organizer = data.organizers(&mut transaction).await?.contains(&me);
        let is_restreamer = data.restreamers(&mut transaction).await?.contains(&me);

        if !is_organizer && !is_restreamer {
            form.context.push_error(form::Error::validation(
                "You must be an organizer or restreamer to manage rosters",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                match_signup_page(
                    transaction,
                    Some(me),
                    &uri,
                    csrf.as_ref(),
                    data,
                    race_id,
                    form.context,
                )
                .await?,
            )
        } else {
            let status = match value.action.as_str() {
                "confirm" => SignupStatus::Confirmed,
                "decline" => SignupStatus::Declined,
                _ => {
                    form.context
                        .push_error(form::Error::validation("Invalid action"));
                    return Ok(RedirectOrContent::Content(
                        match_signup_page(
                            transaction,
                            Some(me),
                            &uri,
                            csrf.as_ref(),
                            data,
                            race_id,
                            form.context,
                        )
                        .await?,
                    ));
                }
            };

            Signup::update_status(&mut transaction, value.signup_id, status).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(match_signup_page_get(
                series, &*event, race_id
            ))))
        }
    } else {
        RedirectOrContent::Content(
            match_signup_page(
                transaction,
                Some(me),
                &uri,
                csrf.as_ref(),
                data,
                race_id,
                form.context,
            )
            .await?,
        )
    })
}

async fn match_signup_page(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    uri: &Origin<'_>,
    csrf: Option<&CsrfToken>,
    data: Data<'_>,
    race_id: Id<Races>,
    _ctx: Context<'_>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Races, true)
        .await?;

    // Get race details
    let race = Race::from_id(&mut transaction, &reqwest::Client::new(), race_id).await?;
    let signups = Signup::for_race(&mut transaction, race_id).await?;
    let role_bindings = RoleBinding::for_event(&mut transaction, data.series, &data.event).await?;

    let content = if let Some(ref me) = me {
        let is_organizer = data.organizers(&mut transaction).await?.contains(me);
        let is_restreamer = data.restreamers(&mut transaction).await?.contains(me);
        let can_manage = is_organizer || is_restreamer;

        html! {
            h2 : "Match Volunteer Signups";
            p {
                : "Race: ";
                : format!("{} {} {}",
                    race.phase.as_deref().unwrap_or(""),
                    race.round.as_deref().unwrap_or(""),
                    match &race.entrants {
                        Entrants::Two([team1, team2]) => format!("{} vs {}",
                            match team1 {
                                Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                Entrant::Named { name, .. } => name.clone(),
                                Entrant::Discord { .. } => "Discord User".to_string(),
                            },
                            match team2 {
                                Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                Entrant::Named { name, .. } => name.clone(),
                                Entrant::Discord { .. } => "Discord User".to_string(),
                            }
                        ),
                        Entrants::Three([team1, team2, team3]) => format!("{} vs {} vs {}",
                            match team1 {
                                Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                Entrant::Named { name, .. } => name.clone(),
                                Entrant::Discord { .. } => "Discord User".to_string(),
                            },
                            match team2 {
                                Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                Entrant::Named { name, .. } => name.clone(),
                                Entrant::Discord { .. } => "Discord User".to_string(),
                            },
                            match team3 {
                                Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                Entrant::Named { name, .. } => name.clone(),
                                Entrant::Discord { .. } => "Discord User".to_string(),
                            }
                        ),
                        _ => "Unknown entrants".to_string(),
                    }
                );
            }

            @if role_bindings.is_empty() {
                p : "No volunteer roles are configured for this event.";
            } else {
                h3 : "Role Signups";
                @for binding in role_bindings {
                    div(class = "role-binding") {
                        h4 : binding.role_type_name;
                        p {
                            : "Required: ";
                            : binding.min_count;
                            : " - ";
                            : binding.max_count;
                            : " volunteers";
                        }

                        @let role_signups = signups.iter().filter(|s| s.role_binding_id == binding.id).collect::<Vec<_>>();
                        @let confirmed_signups = role_signups.iter().filter(|s| matches!(s.status, SignupStatus::Confirmed)).collect::<Vec<_>>();
                        @let pending_signups = role_signups.iter().filter(|s| matches!(s.status, SignupStatus::Pending)).collect::<Vec<_>>();

                        h5 : "Confirmed Volunteers";
                        @if confirmed_signups.is_empty() {
                            p : "No confirmed volunteers yet.";
                        } else {
                            ul {
                                @for signup in confirmed_signups {
                                    li {
                                        @let user = User::from_id(&mut *transaction, signup.user_id).await?;
                                        : user.map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                        : " - ";
                                        : signup.role_type_name;
                                        : " (";
                                        : match signup.status {
                                            SignupStatus::Pending => "Pending",
                                            SignupStatus::Confirmed => "Confirmed",
                                            SignupStatus::Declined => "Declined",
                                        };
                                        : ")";
                                    }
                                }
                            }
                        }

                        @if can_manage && !pending_signups.is_empty() {
                            h5 : "Pending Volunteers";
                            ul {
                                @for signup in pending_signups {
                                    li {
                                        @let user = User::from_id(&mut *transaction, signup.user_id).await?;
                                        : user.map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                        : " - ";
                                        @let errors = Vec::new();
                                        div(class = "button-row") {
                                            @let (errors, confirm_button) = button_form(uri!(manage_roster(data.series, &*data.event, race_id)), csrf, errors, "Confirm");
                                            : errors;
                                            : confirm_button;
                                            @let errors = Vec::new();
                                            @let (errors, decline_button) = button_form(uri!(manage_roster(data.series, &*data.event, race_id)), csrf, errors, "Decline");
                                            : errors;
                                            : decline_button;
                                        }
                                    }
                                }
                            }
                        }

                        @if !can_manage {
                            @let my_approved_roles = RoleRequest::for_event(&mut transaction, data.series, &data.event).await?
                                .into_iter()
                                .filter(|req| req.user_id == me.id && matches!(req.status, RoleRequestStatus::Approved))
                                .collect::<Vec<_>>();

                            @if my_approved_roles.iter().any(|req| req.role_binding_id == binding.id) {
                                @if !role_signups.iter().any(|s| s.user_id == me.id) {
                                    @let errors = Vec::new();
                                    : full_form(uri!(signup_for_match(data.series, &*data.event, race_id)), csrf, html! {
                                        input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                    }, errors, &format!("Sign up for {}", binding.role_type_name).as_str());
                                } else {
                                    p : "You have already signed up for this role.";
                                }
                            } else {
                                p : "You are not approved for this role.";
                            }
                        }

                        @if !role_signups.is_empty() {
                            h5 : "Current Signups:";
                            ul {
                                @for signup in &role_signups {
                                    @let user = User::from_id(&mut *transaction, signup.user_id).await?;
                                    li {
                                        : user.map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                        : " - ";
                                        : signup.role_type_name;
                                        : " (";
                                        : match signup.status {
                                            SignupStatus::Pending => "Pending",
                                            SignupStatus::Confirmed => "Confirmed",
                                            SignupStatus::Declined => "Declined",
                                        };
                                        : ")";
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(match_signup_page_get(data.series, &*data.event, race_id)))))) : "Sign in";
                    : " to view volunteer signups for this match.";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        uri,
        PageStyle {
            chests: data.chests().await?,
            ..PageStyle::default()
        },
        &format!("Volunteer Signups — {}", data.display_name),
        html! {
            : header;
            : content;
        },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/races/<race_id>/volunteers")]
pub(crate) async fn match_signup_page_get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    Ok(match_signup_page(transaction, me, &uri, csrf.as_ref(), data, race_id, ctx).await?)
}
