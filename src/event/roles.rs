use crate::{
    cal::{self, Entrant, Entrants, Race, RaceSchedule},
    event::{Data, Tab},
    form::{EmptyForm, button_form_ext_disabled, form_field, full_form},
    http::{PageError, StatusOrError},
    id::{RoleBindings, RoleRequests, RoleTypes, Signups},
    prelude::*,
    time::format_datetime,
    user::User,
    series::Series,
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

impl From<cal::Error> for StatusOrError<Error> {
    fn from(err: cal::Error) -> Self {
        StatusOrError::Err(Error::Cal(err))
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "role_request_status", rename_all = "lowercase")]
pub(crate) enum RoleRequestStatus {
    Pending,
    Approved,
    Rejected,
    Aborted,
}

#[derive(Debug, Clone, Copy, sqlx::Type, PartialEq)]
#[sqlx(type_name = "volunteer_signup_status", rename_all = "lowercase")]
pub(crate) enum VolunteerSignupStatus {
    Pending,
    Confirmed,
    Declined,
    Aborted,
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
    pub(crate) discord_role_id: Option<i64>,
}

#[allow(unused)]
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
    pub(crate) status: VolunteerSignupStatus,
    pub(crate) notes: Option<String>,
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
                    rt.name AS "role_type_name",
                    rb.discord_role_id
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
        discord_role_id: Option<i64>,
    ) -> sqlx::Result<Id<RoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_bindings (series, event, role_type_id, min_count, max_count, discord_role_id) 
               VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"#,
            series as _,
            event,
            role_type_id as _,
            min_count,
            max_count,
            discord_role_id
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

    pub(crate) async fn exists_for_role_type(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_type_id: Id<RoleTypes>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_bindings
                   WHERE series = $1 AND event = $2 AND role_type_id = $3)"#,
            series as _,
            event,
            role_type_id as _
        )
        .fetch_one(&mut **pool)
        .await?
        .unwrap_or(false))
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

    pub(crate) async fn active_for_user(
        pool: &mut Transaction<'_, Postgres>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_requests
                   WHERE role_binding_id = $1 AND user_id = $2 AND status IN ('pending', 'approved'))"#,
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
        Ok(sqlx::query_as!(
            Self,
            r#"SELECT rr.id as "id!: Id<RoleRequests>", rr.role_binding_id as "role_binding_id!: Id<RoleBindings>", rr.user_id as "user_id!: Id<Users>", 
                      rr.status as "status!: RoleRequestStatus", rr.notes, rr.created_at as "created_at!", rr.updated_at as "updated_at!",
                      rb.series as "series!: Series", rb.event as "event!", rb.min_count as "min_count!", rb.max_count as "max_count!", 
                      rt.name as "role_type_name!"
               FROM role_requests rr
               JOIN role_bindings rb ON rr.role_binding_id = rb.id
               JOIN role_types rt ON rb.role_type_id = rt.id
               WHERE rb.series = $1 AND rb.event = $2 AND rr.status = 'pending'
               ORDER BY rr.created_at ASC"#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await?)
    }

    pub(crate) async fn approved_for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        Ok(sqlx::query_as!(
            Self,
            r#"SELECT rr.id as "id!: Id<RoleRequests>", rr.role_binding_id as "role_binding_id!: Id<RoleBindings>", rr.user_id as "user_id!: Id<Users>", 
                      rr.status as "status!: RoleRequestStatus", rr.notes, rr.created_at as "created_at!", rr.updated_at as "updated_at!",
                      rb.series as "series!: Series", rb.event as "event!", rb.min_count as "min_count!", rb.max_count as "max_count!", 
                      rt.name as "role_type_name!"
               FROM role_requests rr
               JOIN role_bindings rb ON rr.role_binding_id = rb.id
               JOIN role_types rt ON rb.role_type_id = rt.id
               WHERE rb.series = $1 AND rb.event = $2 AND rr.status = 'approved'
               ORDER BY rt.name ASC, rr.created_at ASC"#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await?)
    }

    pub(crate) async fn from_id(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<RoleRequests>,
    ) -> sqlx::Result<Option<Self>> {
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
                WHERE rr.id = $1
            "#,
            id as _
        )
        .fetch_optional(&mut **pool)
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
                    s.status AS "status: VolunteerSignupStatus",
                    s.notes,
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
        notes: Option<String>,
    ) -> sqlx::Result<Id<Signups>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO signups (race_id, role_binding_id, user_id, notes)
               VALUES ($1, $2, $3, $4) RETURNING id"#,
            race_id as _,
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
        id: Id<Signups>,
        status: VolunteerSignupStatus,
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

    pub(crate) async fn active_for_user(
        pool: &mut Transaction<'_, Postgres>,
        race_id: Id<Races>,
        role_binding_id: Id<RoleBindings>,
        user_id: Id<Users>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM signups
                   WHERE race_id = $1 AND role_binding_id = $2 AND user_id = $3 AND status IN ('pending', 'confirmed'))"#,
            race_id as _,
            role_binding_id as _,
            user_id as _
        )
        .fetch_one(&mut **pool)
        .await?
        .unwrap_or(false))
    }

    pub(crate) async fn from_id(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<Signups>,
    ) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT 
                    s.id AS "id: Id<Signups>",
                    s.race_id AS "race_id: Id<Races>",
                    s.role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    s.user_id AS "user_id: Id<Users>",
                    s.status AS "status: VolunteerSignupStatus",
                    s.notes,
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
                WHERE s.id = $1
            "#,
            id as _
        )
        .fetch_optional(&mut **pool)
        .await
    }

    /// Auto-reject overlapping signups for a user when they are confirmed for a race
    async fn auto_reject_overlapping_signups(
        pool: &mut Transaction<'_, Postgres>,
        confirmed_signup_id: Id<Signups>,
        user_id: Id<Users>,
    ) -> sqlx::Result<()> {
        let confirmed_signup = sqlx::query!(
            r#"SELECT s.race_id, s.role_binding_id, r.series as "series: Series", r.start
               FROM signups s
               JOIN races r ON s.race_id = r.id
               WHERE s.id = $1"#,
            confirmed_signup_id as _
        )
        .fetch_one(&mut **pool)
        .await?;

        if let Some(start_time) = confirmed_signup.start {
            let duration = confirmed_signup.series.default_race_duration();
            let end_time = start_time + duration;

         
            let all_user_signups = sqlx::query!(
                r#"SELECT s.id, s.race_id, r.series as "series: Series", r.start
                   FROM signups s
                   JOIN races r ON s.race_id = r.id
                   WHERE s.user_id = $1 
                   AND s.id != $2
                   AND s.status = 'pending'
                   AND r.start IS NOT NULL"#,
                user_id as _,
                confirmed_signup_id as _
            )
            .fetch_all(&mut **pool)
            .await?;

            for signup in all_user_signups {
                if let Some(signup_start_time) = signup.start {
                    let signup_duration = signup.series.default_race_duration();
                    let signup_end_time = signup_start_time + signup_duration;

                    if start_time < signup_end_time && signup_start_time < end_time {
                        sqlx::query!(
                            r#"UPDATE signups SET status = 'declined', updated_at = NOW() WHERE id = $1"#,
                            signup.id
                        )
                        .execute(&mut **pool)
                        .await?;
                    }
                }
            }
        }

        Ok(())
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
            let approved_requests =
                RoleRequest::approved_for_event(&mut transaction, data.series, &data.event).await?;
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
                                th : "Discord Role";
                                th;
                            }
                        }
                        tbody {
                            @for binding in &role_bindings {
                                tr {
                                    td : binding.role_type_name;
                                    td : binding.min_count;
                                    td : binding.max_count;
                                    td {
                                        @if let Some(discord_role_id) = binding.discord_role_id {
                                            : format!("{}", discord_role_id);
                                        } else {
                                            : "None";
                                        }
                                    }
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
                    : form_field("discord_role_id", &mut errors, html! {
                        label(for = "discord_role_id") : "Discord Role ID (optional):";
                        input(type = "text", name = "discord_role_id", id = "discord_role_id", placeholder = "e.g. 123456789012345678");
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

                h3 : "Confirmed Role Requests";
                @if approved_requests.is_empty() {
                    p : "No confirmed role requests.";
                } else {
                    @for binding in &role_bindings {
                        @let binding_requests = approved_requests.iter().filter(|req| req.role_binding_id == binding.id).collect::<Vec<_>>();
                        @if !binding_requests.is_empty() {
                            details {
                                summary : format!("{} ({})", binding.role_type_name, binding_requests.len());
                                table {
                                    thead {
                                        tr {
                                            th : "User";
                                            th : "Notes";
                                            th : "Approved";
                                            th : "Actions";
                                        }
                                    }
                                    tbody {
                                        @for request in binding_requests {
                                            tr {
                                                td {
                                                    @let user = User::from_id(&mut *transaction, request.user_id).await?;
                                                    : user.map_or_else(|| request.user_id.to_string(), |u| u.to_string());
                                                }
                                                td {
                                                    @if let Some(ref notes) = request.notes {
                                                        : notes;
                                                    } else {
                                                        : "No notes";
                                                    }
                                                }
                                                td : format_datetime(request.updated_at, DateTimeFormat { long: true, running_text: false });
                                                td {
                                                    @let errors = Vec::new();
                                                    @let (errors, revoke_button) = button_form_ext(uri!(revoke_role_request(data.series, &*data.event)), csrf, errors, html! {
                                                        input(type = "hidden", name = "request_id", value = request.id.to_string());
                                                    }, "Revoke");
                                                    : errors;
                                                    : revoke_button;
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
    #[field(default = String::new())]
    discord_role_id: String,
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

        if RoleBinding::exists_for_role_type(&mut transaction, data.series, &data.event, value.role_type_id).await? {
            form.context.push_error(form::Error::validation(
                "A role binding for this role type already exists.",
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
            let discord_role_id = if value.discord_role_id.is_empty() {
                None
            } else {
                match value.discord_role_id.parse::<i64>() {
                    Ok(id) => Some(id),
                    Err(_) => {
                        form.context.push_error(form::Error::validation(
                            "Discord role ID must be a valid number.",
                        ));
                        return Ok(RedirectOrContent::Content(
                            roles_page(
                                transaction,
                                Some(me),
                                &uri,
                                csrf.as_ref(),
                                data,
                                form.context,
                            )
                            .await?,
                        ));
                    }
                }
            };

            RoleBinding::create(
                &mut transaction,
                data.series,
                &data.event,
                value.role_type_id,
                value.min_count,
                value.max_count,
                discord_role_id,
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
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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
            // Get the role request details
            let role_request = RoleRequest::from_id(&mut transaction, request).await?
                .ok_or(StatusOrError::Status(Status::NotFound))?;
            
            // Get the role binding to check for Discord role ID
            let role_binding = sqlx::query_as!(
                RoleBinding,
                r#"SELECT rb.id as "id: Id<RoleBindings>", rb.series as "series: Series", rb.event as "event!", 
                          rb.role_type_id as "role_type_id: Id<RoleTypes>", rb.min_count as "min_count!", 
                          rb.max_count as "max_count!", rt.name as "role_type_name!", rb.discord_role_id
                   FROM role_bindings rb
                   JOIN role_types rt ON rb.role_type_id = rt.id
                   WHERE rb.id = $1"#,
                role_request.role_binding_id as _
            )
            .fetch_one(&mut *transaction)
            .await?;

            // Update the role request status
            RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Approved)
                .await?;

            // Handle Discord role assignment if applicable
            if let Some(discord_role_id) = role_binding.discord_role_id {
                if let Some(discord_guild) = data.discord_guild {
                    // Get the user's Discord ID
                    let user = User::from_id(&mut *transaction, role_request.user_id).await?
                        .ok_or(StatusOrError::Status(Status::NotFound))?;
                    
                    if let Some(discord_user) = user.discord {
                        let discord_ctx = discord_ctx.read().await;
                        
                        // Check if user is in the Discord server
                        match discord_guild.member(&*discord_ctx, discord_user.id).await {
                            Ok(member) => {
                                // User is in the server, assign the role directly
                                if let Err(e) = member.add_role(&*discord_ctx, RoleId::new(discord_role_id.try_into().unwrap())).await {
                                    eprintln!("Failed to assign Discord role {} to user {}: {:?}", discord_role_id, discord_user.id, e);
                                    // Don't fail the entire request, just log the error
                                }
                            }
                            Err(_) => {
                                // User is not in the server, create a pending invite
                                let invite_url = if let Some(invite_url) = data.discord_invite_url {
                                    invite_url.to_string()
                                } else {
                                    // For now, we'll just log that we need an invite URL
                                    eprintln!("No Discord invite URL configured for event {}", event);
                                    return Err(StatusOrError::Status(Status::InternalServerError));
                                };

                                // Store the pending invite
                                sqlx::query!(
                                    r#"INSERT INTO pending_discord_invites 
                                       (user_id, role_request_id, discord_guild_id, discord_role_id, invite_url)
                                       VALUES ($1, $2, $3, $4, $5)
                                       ON CONFLICT (user_id, role_request_id) DO UPDATE SET
                                       discord_guild_id = $3, discord_role_id = $4, invite_url = $5, created_at = NOW()"#,
                                    role_request.user_id as _,
                                    request as _,
                                    discord_guild.get() as i64,
                                    discord_role_id as i64,
                                    invite_url
                                )
                                .execute(&mut *transaction)
                                .await?;

                                // Send DM to user with invite link
                                if let Ok(dm_channel) = discord_user.id.create_dm_channel(&*discord_ctx).await {
                                    let message = format!(
                                        "Your role request for **{}** in **{}** has been approved! 🎉\n\nPlease join the Discord server to receive your role (invite valid for 7 days):\n{}",
                                        role_binding.role_type_name, data.display_name, invite_url
                                    );
                                    if let Err(e) = dm_channel.say(&*discord_ctx, message).await {
                                        eprintln!("Failed to send DM to user {}: {}", discord_user.id, e);
                                    }
                                } else {
                                    eprintln!("Failed to create DM channel for user {}", discord_user.id);
                                }
                                
                                eprintln!("User {} needs Discord invite to join server {} for role {}", 
                                         discord_user.id, discord_guild.get(), discord_role_id);
                            }
                        }
                    } else {
                        // User doesn't have Discord connected
                        eprintln!("User {} doesn't have Discord account connected for role assignment", role_request.user_id);
                    }
                } else {
                    // Event doesn't have a Discord guild configured
                    eprintln!("Event {} doesn't have Discord guild configured for role assignment", event);
                }
            }

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
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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

            if RoleRequest::active_for_user(&mut transaction, value.role_binding_id, me.id).await? {
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
                // Get the role binding details for the notification
                let role_binding = sqlx::query_as!(
                    RoleBinding,
                    r#"SELECT rb.id as "id: Id<RoleBindings>", rb.series as "series: Series", rb.event as "event!", 
                              rb.role_type_id as "role_type_id: Id<RoleTypes>", rb.min_count as "min_count!", 
                              rb.max_count as "max_count!", rt.name as "role_type_name!", rb.discord_role_id
                       FROM role_bindings rb
                       JOIN role_types rt ON rb.role_type_id = rt.id
                       WHERE rb.id = $1"#,
                    value.role_binding_id as _
                )
                .fetch_one(&mut *transaction)
                .await?;

                // Create the role request
                RoleRequest::create(
                    &mut transaction,
                    value.role_binding_id,
                    me.id,
                    notes.clone().unwrap_or_default(),
                )
                .await?;

                // Send Discord notification to organizer channel
                if let Some(organizer_channel) = data.discord_organizer_channel {
                    let discord_ctx = discord_ctx.read().await;
                    let mut msg = MessageBuilder::default();
                    msg.push("New volunteer application: ");
                    msg.mention_user(&me);
                    msg.push(" has applied for the **");
                    msg.push_safe(&role_binding.role_type_name);
                    msg.push("** role in **");
                    msg.push_safe(&data.display_name);
                    msg.push("**.");
                    
                    if let Some(notes) = notes {
                        msg.push("\nNotes: ");
                        msg.push_safe(&notes);
                    }
                    
                    msg.push("\n\nClick here to review and manage role requests for the event: ");
                    msg.push_named_link_no_preview("Manage Roles", format!("{}/event/{}/{}/roles", 
                        base_uri(),
                        series.slug(),
                        event
                    ));

                    if let Err(e) = organizer_channel.say(&*discord_ctx, msg.build()).await {
                        eprintln!("Failed to send Discord notification for role request: {}", e);
                    }
                }

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

        // Get my approved roles
        let my_approved_roles: Vec<_> = my_requests
            .iter()
            .filter(|req| matches!(req.status, RoleRequestStatus::Approved))
            .collect();

        // Get upcoming races for this event
        let all_races = Race::for_event(&mut transaction, &reqwest::Client::new(), &data).await?;
        let upcoming_races: Vec<_> = all_races
            .into_iter()
            .filter(|race| {
                let scheduled = match race.schedule {
                    RaceSchedule::Unscheduled => false, 
                    RaceSchedule::Live { end, .. } => {
                        end.is_none_or(|end_time| end_time > Utc::now())
                    }
                    RaceSchedule::Async { .. } => false, // Exclude async races entirely
                };
                let all_teams_consented = race.teams_opt().map_or(false, |mut teams| teams.all(|team| team.restream_consent));
                scheduled && all_teams_consented
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
                    @let my_request = my_requests.iter()
                        .filter(|req| req.role_binding_id == binding.id && !matches!(req.status, RoleRequestStatus::Aborted))
                        .max_by_key(|req| req.created_at);
                    @let has_active_request = my_request.map_or(false, |req| matches!(req.status, RoleRequestStatus::Pending | RoleRequestStatus::Approved));
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
                                    RoleRequestStatus::Aborted => : "Aborted";
                                }
                            }
                            @if matches!(request.status, RoleRequestStatus::Pending) {
                                @let errors = Vec::new();
                                div(class = "button-row") {
                                    @let (errors, withdraw_button) = button_form_ext(
                                        uri!(withdraw_role_request(data.series, &*data.event)),
                                        csrf,
                                        errors,
                                        html! {
                                            input(type = "hidden", name = "request_id", value = request.id.to_string());
                                        },
                                        "Withdraw Application"
                                    );
                                    : errors;
                                    : withdraw_button;
                                }
                            }
                        }
                        @if !has_active_request {
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
                                                RaceSchedule::Live { start, .. } => {
                                                    : " - ";
                                                    : format_datetime(start, DateTimeFormat { long: false, running_text: false });
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
    #[field(default = String::new())]
    notes: String,
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

        if Signup::active_for_user(&mut transaction, race_id, value.role_binding_id, me.id).await? {
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
            let notes = if value.notes.trim().is_empty() {
                None
            } else {
                Some(value.notes.trim().to_string())
            };
            Signup::create(&mut transaction, race_id, value.role_binding_id, me.id, notes).await?;
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
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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
                "confirm" => VolunteerSignupStatus::Confirmed,
                "decline" => VolunteerSignupStatus::Declined,
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
            
            // If the signup is being confirmed, auto-reject overlapping signups for the same user
            if status == VolunteerSignupStatus::Confirmed {
                // Get the user ID for the confirmed signup
                let signup = Signup::from_id(&mut transaction, value.signup_id).await?
                    .ok_or(StatusOrError::Status(Status::NotFound))?;
                
                Signup::auto_reject_overlapping_signups(&mut transaction, value.signup_id, signup.user_id).await?;
                
                // Send Discord notification to volunteer info channel
                if let Some(discord_volunteer_info_channel) = data.discord_volunteer_info_channel {
                    // Get race details for the notification
                    let race = Race::from_id(&mut transaction, &reqwest::Client::new(), race_id).await?;
                    let user = User::from_id(&mut *transaction, signup.user_id).await?;
                    
                    // Get all confirmed volunteers for this race
                    let all_signups = Signup::for_race(&mut transaction, race_id).await?;
                    let confirmed_signups = all_signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Confirmed)).collect::<Vec<_>>();
                    
                    // Format race description
                    let race_description = match &race.entrants {
                        cal::Entrants::Two([team1, team2]) => format!("{} vs {}",
                            match team1 {
                                cal::Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                cal::Entrant::Named { name, .. } => name.clone(),
                                cal::Entrant::Discord { .. } => "Discord User".to_string(),
                            },
                            match team2 {
                                cal::Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                cal::Entrant::Named { name, .. } => name.clone(),
                                cal::Entrant::Discord { .. } => "Discord User".to_string(),
                            }
                        ),
                        cal::Entrants::Three([team1, team2, team3]) => format!("{} vs {} vs {}",
                            match team1 {
                                cal::Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                cal::Entrant::Named { name, .. } => name.clone(),
                                cal::Entrant::Discord { .. } => "Discord User".to_string(),
                            },
                            match team2 {
                                cal::Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                cal::Entrant::Named { name, .. } => name.clone(),
                                cal::Entrant::Discord { .. } => "Discord User".to_string(),
                            },
                            match team3 {
                                cal::Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                cal::Entrant::Named { name, .. } => name.clone(),
                                cal::Entrant::Discord { .. } => "Discord User".to_string(),
                            }
                        ),
                        _ => "Unknown entrants".to_string(),
                    };
                    
                    // Get race start time for timestamp
                    let race_start_time = match race.schedule {
                        cal::RaceSchedule::Live { start, .. } => start,
                        _ => return Err(StatusOrError::Status(Status::BadRequest)), // Volunteers can't sign up for unscheduled races
                    };
                    
                    // Build Discord message
                    let mut msg = MessageBuilder::default();
                    msg.push("Volunteers selected for race ");
                    msg.push_mono(&race_description);
                    msg.push(" at ");
                    msg.push_timestamp(race_start_time, serenity_utils::message::TimestampStyle::Relative);
                    msg.push("\n\n");
                    
                    // Add role and user information for the newly selected volunteer
                    msg.push("**Role:** ");
                    msg.push_mono(&signup.role_type_name);
                    msg.push("\n**Selected:** ");
                    
                    if let Some(user) = user {
                        // Check if user has Discord connected
                        if let Some(discord) = user.discord {
                            // Ping the user using their Discord ID
                            msg.mention(&UserId::new(discord.id.get()));
                        } else {
                            // Just mention by display name
                            msg.push(&user.to_string());
                        }
                    } else {
                        // Fallback to user ID if user not found
                        msg.push(&signup.user_id.to_string());
                    }
                    
                    // Add all confirmed volunteers section
                    if confirmed_signups.len() > 1 {
                        msg.push("\n\n**Confirmed volunteers for this race:**\n");
                        
                        // Group by role type
                        let role_bindings = RoleBinding::for_event(&mut transaction, data.series, &data.event).await?;
                        for binding in role_bindings {
                            let binding_signups = confirmed_signups.iter().filter(|s| s.role_binding_id == binding.id).collect::<Vec<_>>();
                            if !binding_signups.is_empty() {
                                msg.push("**");
                                msg.push(&binding.role_type_name);
                                msg.push(":** ");
                                
                                for (i, signup) in binding_signups.iter().enumerate() {
                                    if i > 0 { msg.push(", "); }
                                    let volunteer_user = User::from_id(&mut *transaction, signup.user_id).await?;
                                    if let Some(volunteer_user) = volunteer_user {
                                        msg.push(&volunteer_user.to_string());
                                    } else {
                                        msg.push(&signup.user_id.to_string());
                                    }
                                }
                                msg.push("\n");
                            }
                        }
                    }
                    
                    // Send the message to Discord
                    let discord_ctx = discord_ctx.read().await;
                    if let Err(e) = discord_volunteer_info_channel.say(&*discord_ctx, msg.build()).await {
                        eprintln!("Failed to send volunteer notification to Discord: {}", e);
                    }
                }
            }
            
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
            h3 {
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
            p {
                @match race.schedule {
                    RaceSchedule::Unscheduled => : "Unscheduled";
                    RaceSchedule::Live { start, .. } => {
                        : "Scheduled for ";
                        : format_datetime(start, DateTimeFormat { long: true, running_text: false });
                    }
                    RaceSchedule::Async { .. } => : "Async Race";
                }
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
                        @let confirmed_signups = role_signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Confirmed)).collect::<Vec<_>>();
                        @let pending_signups = role_signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Pending)).collect::<Vec<_>>();

                        h5 : "Confirmed Volunteers";
                        @if confirmed_signups.is_empty() {
                            p : "No confirmed volunteers yet.";
                        } else {
                            ul {
                                @for signup in &confirmed_signups {
                                    li {
                                        @let user = User::from_id(&mut *transaction, signup.user_id).await?;
                                        : user.map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                        @if can_manage && signup.notes.is_some() {
                                            : " [";
                                            : signup.notes.as_ref().unwrap();
                                            : "]";
                                        }
                                        @if can_manage {
                                            @let errors = Vec::new();
                                            div(class = "button-row") {
                                                @let (errors, revoke_button) = button_form_ext(
                                                    uri!(revoke_signup(data.series, &*data.event, race_id)),
                                                    csrf,
                                                    errors,
                                                    html! {
                                                        input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                                    },
                                                    "Revoke"
                                                );
                                                : errors;
                                                : revoke_button;
                                            }
                                        }
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
                                        @if signup.notes.is_some() {
                                            : " [";
                                            : signup.notes.as_ref().unwrap();
                                            : "]";
                                        }
                                        @let errors = Vec::new();
                                        div(class = "button-row") {
                                            @let (errors, confirm_button) = button_form_ext_disabled(
                                                uri!(manage_roster(data.series, &*data.event, race_id)), 
                                                csrf, 
                                                errors, 
                                                html! {
                                                    input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                                    input(type = "hidden", name = "action", value = "confirm");
                                                },
                                                "Confirm",
                                                false
                                            );
                                            : errors;
                                            : confirm_button;
                                            @let errors = Vec::new();
                                            @let (errors, decline_button) = button_form_ext_disabled(
                                                uri!(manage_roster(data.series, &*data.event, race_id)), 
                                                csrf, 
                                                errors, 
                                                html! {
                                                    input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                                    input(type = "hidden", name = "action", value = "decline");
                                                },
                                                "Decline",
                                                false
                                            );
                                            : errors;
                                            : decline_button;
                                        }
                                    }
                                }
                            }
                        }

                        @let my_approved_roles = RoleRequest::for_event(&mut transaction, data.series, &data.event).await?
                            .into_iter()
                            .filter(|req| req.user_id == me.id && matches!(req.status, RoleRequestStatus::Approved))
                            .collect::<Vec<_>>();

                        @if my_approved_roles.iter().any(|req| req.role_binding_id == binding.id) {
                            @let has_active_signup = role_signups.iter().any(|s| s.user_id == me.id && matches!(s.status, VolunteerSignupStatus::Pending | VolunteerSignupStatus::Confirmed));
                            @if !has_active_signup {
                                @let errors = Vec::new();
                                @let max_reached = confirmed_signups.len() as i32 >= binding.max_count;
                                @let is_async = matches!(race.schedule, RaceSchedule::Async { .. });
                                @let is_ended = race.is_ended();
                                @let disabled = max_reached || is_async || is_ended;
                                @let reason = if max_reached {
                                    Some("Maximum number of volunteers reached for this role.")
                                } else if is_async {
                                    Some("Signups are not available for async races.")
                                } else if is_ended {
                                    Some("This race has ended and can no longer accept signups.")
                                } else {
                                    None
                                };
                                @if disabled {
                                    @let (errors, signup_button) = button_form_ext_disabled(
                                        uri!(signup_for_match(data.series, &*data.event, race_id)),
                                        csrf,
                                        errors,
                                        html! {
                                            input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                        },
                                        &format!("Sign up for {}", binding.role_type_name),
                                        true
                                    );
                                    : errors;
                                    div(class = "button-row") {
                                        : signup_button;
                                    }
                                } else {
                                    @let mut errors = Vec::new();
                                    : full_form(uri!(signup_for_match(data.series, &*data.event, race_id)), csrf, html! {
                                        input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                        : form_field("notes", &mut errors, html! {
                                            label(for = "notes") : "Notes:";
                                            input(type = "text", name = "notes", id = "notes", maxlength = "60", size = "30", placeholder = "Optional notes for organizers");
                                        });
                                    }, errors, &format!("Sign up for {}", binding.role_type_name));
                                }
                                @if let Some(reason) = reason {
                                    p(class = "disabled-reason") : reason;
                                }
                            } else {
                                p : "You have already signed up for this role.";
                            }
                        } else {
                            p : "You are not approved for this role.";
                        }

                        @if !role_signups.is_empty() {
                            @let has_non_confirmed_non_aborted = role_signups.iter().any(|s| !matches!(s.status, VolunteerSignupStatus::Confirmed | VolunteerSignupStatus::Aborted));
                            @if has_non_confirmed_non_aborted {
                                h5 : "Current Signups:";
                                ul {
                                    @for signup in &role_signups {
                                        @if !matches!(signup.status, VolunteerSignupStatus::Confirmed | VolunteerSignupStatus::Aborted) {
                                            @let user = User::from_id(&mut *transaction, signup.user_id).await?;
                                            li {
                                                : user.map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                                @if can_manage && signup.notes.is_some() {
                                                    : " [";
                                                    : signup.notes.as_ref().unwrap();
                                                    : "]";
                                                }
                                                : " (";
                                                : match signup.status {
                                                    VolunteerSignupStatus::Pending => "Pending",
                                                    VolunteerSignupStatus::Confirmed => "Confirmed",
                                                    VolunteerSignupStatus::Declined => "Declined",
                                                    VolunteerSignupStatus::Aborted => "Aborted",
                                                };
                                                : ")";
                                                @if signup.user_id == me.id && matches!(signup.status, VolunteerSignupStatus::Pending) {
                                                    @let errors = Vec::new();
                                                    div(class = "button-row") {
                                                        @let (errors, withdraw_button) = button_form_ext(
                                                            uri!(withdraw_signup(data.series, &*data.event, race_id)),
                                                            csrf,
                                                            errors,
                                                            html! {
                                                                input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                                            },
                                                            "Withdraw"
                                                        );
                                                        : errors;
                                                        : withdraw_button;
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

#[derive(FromForm, CsrfForm)]
pub(crate) struct WithdrawSignupForm {
    #[field(default = String::new())]
    csrf: String,
    signup_id: Id<Signups>,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RevokeSignupForm {
    #[field(default = String::new())]
    csrf: String,
    signup_id: Id<Signups>,
}

#[rocket::post("/event/<series>/<event>/races/<race_id>/withdraw-signup", data = "<form>")]
pub(crate) async fn withdraw_signup(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, WithdrawSignupForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        // Verify the signup exists and belongs to the current user
        let signup = Signup::from_id(&mut transaction, value.signup_id).await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

        if signup.user_id != me.id {
            form.context.push_error(form::Error::validation(
                "You can only withdraw your own signups",
            ));
        }

        if signup.race_id != race_id {
            form.context.push_error(form::Error::validation(
                "Invalid signup for this race",
            ));
        }

        // Only allow withdrawing pending signups
        if !matches!(signup.status, VolunteerSignupStatus::Pending) {
            form.context.push_error(form::Error::validation(
                "You can only withdraw pending signups",
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
            // Update the signup status to Aborted
            Signup::update_status(&mut transaction, value.signup_id, VolunteerSignupStatus::Aborted).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(match_signup_page_get(series, event, race_id))))
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

#[rocket::post("/event/<series>/<event>/races/<race_id>/revoke-signup", data = "<form>")]
pub(crate) async fn revoke_signup(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    form: Form<Contextual<'_, RevokeSignupForm>>,
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
                "You must be an organizer or restreamer to revoke signups",
            ));
        }

        // Verify the signup exists
        let signup = Signup::from_id(&mut transaction, value.signup_id).await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

        if signup.race_id != race_id {
            form.context.push_error(form::Error::validation(
                "Invalid signup for this race",
            ));
        }

        // Only allow revoking confirmed signups
        if !matches!(signup.status, VolunteerSignupStatus::Confirmed) {
            form.context.push_error(form::Error::validation(
                "You can only revoke confirmed signups",
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
            // Update the signup status back to Pending
            Signup::update_status(&mut transaction, value.signup_id, VolunteerSignupStatus::Pending).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(match_signup_page_get(series, event, race_id))))
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
pub(crate) struct WithdrawRoleRequestForm {
    #[field(default = String::new())]
    csrf: String,
    request_id: Id<RoleRequests>,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RevokeRoleRequestForm {
    #[field(default = String::new())]
    csrf: String,
    request_id: Id<RoleRequests>,
}

#[rocket::post("/event/<series>/<event>/withdraw-role-request", data = "<form>")]
pub(crate) async fn withdraw_role_request(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, WithdrawRoleRequestForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        // Verify the role request exists and belongs to the current user
        let request = RoleRequest::from_id(&mut transaction, value.request_id).await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

        if request.user_id != me.id {
            form.context.push_error(form::Error::validation(
                "You can only withdraw your own role requests",
            ));
        }

        // Only allow withdrawing pending role requests
        if !matches!(request.status, RoleRequestStatus::Pending) {
            form.context.push_error(form::Error::validation(
                "You can only withdraw pending role requests",
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
            // Update the role request status to Aborted
            RoleRequest::update_status(&mut transaction, value.request_id, RoleRequestStatus::Aborted).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(volunteer_page_get(series, event))))
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

#[rocket::post("/event/<series>/<event>/revoke-role-request", data = "<form>")]
pub(crate) async fn revoke_role_request(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, RevokeRoleRequestForm>>,
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

        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to revoke role requests",
            ));
        }

        // Verify the role request exists
        let request = RoleRequest::from_id(&mut transaction, value.request_id).await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

        if request.series != series || request.event != event {
            form.context.push_error(form::Error::validation(
                "Invalid role request for this event",
            ));
        }

        // Only allow revoking approved role requests
        if !matches!(request.status, RoleRequestStatus::Approved) {
            form.context.push_error(form::Error::validation(
                "You can only revoke approved role requests",
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
            // Update the role request status back to Pending
            RoleRequest::update_status(&mut transaction, value.request_id, RoleRequestStatus::Pending).await?;
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
