use {
    rocket::{
        form::{Contextual, Form},
        http::{Status, uri::Origin as HttpOrigin},
        response::Redirect,
        State,
    },
    sqlx::{
        PgPool,
        Transaction,
        Postgres,
    },
    chrono::{DateTime, Utc},
    crate::{
        event::{Data, Tab},
        form::{EmptyForm, button_form_ext_disabled, form_field, full_form},
        http::{PageError, StatusOrError},
        id::{RoleBindings, RoleRequests, RoleTypes, Signups, EventDiscordRoleOverrides, EventDisabledRoleBindings},
        prelude::*,
        time::format_datetime,
        user::User,
        series::Series,
        game,
        cal::{Race, RaceSchedule, Entrants, Entrant},
    },
    rocket_util::Origin,
    std::collections::{HashMap, HashSet},
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
    #[error(transparent)]
    Game(#[from] game::GameError),
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

impl From<game::GameError> for StatusOrError<Error> {
    fn from(err: game::GameError) -> Self {
        StatusOrError::Err(Error::Game(err))
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
    pub(crate) series: Option<Series>,
    pub(crate) event: Option<String>,
    pub(crate) game_id: Option<i32>,
    pub(crate) role_type_id: Id<RoleTypes>,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
    pub(crate) discord_role_id: Option<i64>,
    pub(crate) auto_approve: bool,
}

#[allow(unused)]
pub(crate) struct GameRoleBinding {
    pub(crate) id: Id<RoleBindings>,
    pub(crate) role_type_id: Id<RoleTypes>,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
    pub(crate) discord_role_id: Option<i64>,
    pub(crate) auto_approve: bool,
}

#[allow(unused)]
#[derive(Clone)]
pub(crate) struct RoleRequest {
    pub(crate) id: Id<RoleRequests>,
    pub(crate) role_binding_id: Id<RoleBindings>,
    pub(crate) user_id: Id<Users>,
    pub(crate) status: RoleRequestStatus,
    pub(crate) notes: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) series: Option<Series>,
    pub(crate) event: Option<String>,
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
    pub(crate) series: Option<Series>,
    pub(crate) event: Option<String>,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
}

#[allow(unused)]
pub(crate) struct EffectiveRoleBinding {
    pub(crate) id: Id<RoleBindings>,
    pub(crate) role_type_id: Id<RoleTypes>,
    pub(crate) min_count: i32,
    pub(crate) max_count: i32,
    pub(crate) role_type_name: String,
    pub(crate) discord_role_id: Option<i64>,
    pub(crate) auto_approve: bool,
    pub(crate) is_game_binding: bool,
    pub(crate) has_event_override: bool,
    pub(crate) is_disabled: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct EventDiscordRoleOverride {
    pub(crate) id: Id<EventDiscordRoleOverrides>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) role_type_id: Id<RoleTypes>,
    pub(crate) discord_role_id: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct EventDisabledRoleBinding {
    pub(crate) id: Id<EventDisabledRoleBindings>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) role_type_id: Id<RoleTypes>,
    pub(crate) created_at: DateTime<Utc>,
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

    pub(crate) async fn from_name(pool: &mut Transaction<'_, Postgres>, name: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(
            Self,
            r#"SELECT id AS "id: Id<RoleTypes>", name FROM role_types WHERE name = $1"#,
            name
        )
        .fetch_optional(&mut **pool)
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
                    rb.game_id,
                    rb.role_type_id AS "role_type_id: Id<RoleTypes>",
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.discord_role_id,
                    rb.auto_approve
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
        auto_approve: bool,
    ) -> sqlx::Result<Id<RoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_bindings (series, event, role_type_id, min_count, max_count, discord_role_id, game_id, auto_approve) 
               VALUES ($1, $2, $3, $4, $5, $6, NULL, $7) RETURNING id"#,
            series as _,
            event,
            role_type_id as _,
            min_count,
            max_count,
            discord_role_id,
            auto_approve
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
                   WHERE series = $1 AND event = $2 AND role_type_id = $3 AND game_id IS NULL)"#,
            series as _,
            event,
            role_type_id as _
        )
        .fetch_one(&mut **pool)
        .await?
        .unwrap_or(false))
    }
}

impl GameRoleBinding {
    pub(crate) async fn for_game(
        pool: &mut Transaction<'_, Postgres>,
        game_id: i32,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT
                    rb.id AS "id: Id<RoleBindings>",
                    rb.role_type_id AS "role_type_id: Id<RoleTypes>",
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.discord_role_id,
                    rb.auto_approve
                FROM role_bindings rb
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.game_id = $1 AND rb.series IS NULL AND rb.event IS NULL
                ORDER BY rt.name
            "#,
            game_id
        )
        .fetch_all(&mut **pool)
        .await
    }

    pub(crate) async fn create(
        pool: &mut Transaction<'_, Postgres>,
        game_id: i32,
        role_type_id: Id<RoleTypes>,
        min_count: i32,
        max_count: i32,
        discord_role_id: Option<i64>,
        auto_approve: bool,
    ) -> sqlx::Result<Id<RoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_bindings (game_id, role_type_id, min_count, max_count, discord_role_id, series, event, auto_approve) 
               VALUES ($1, $2, $3, $4, $5, NULL, NULL, $6) RETURNING id"#,
            game_id,
            role_type_id as _,
            min_count,
            max_count,
            discord_role_id,
            auto_approve
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
        game_id: i32,
        role_type_id: Id<RoleTypes>,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_bindings
                   WHERE game_id = $1 AND role_type_id = $2 AND series IS NULL AND event IS NULL)"#,
            game_id,
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

    pub(crate) async fn for_game(
        pool: &mut Transaction<'_, Postgres>,
        game_id: i32,
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
                WHERE rb.game_id = $1 AND rb.series IS NULL AND rb.event IS NULL
                ORDER BY rt.name, rr.created_at
            "#,
            game_id
        )
        .fetch_all(&mut **pool)
        .await
    }

    pub(crate) async fn for_user(
        pool: &mut Transaction<'_, Postgres>,
        user_id: Id<Users>,
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
                WHERE rr.user_id = $1
                ORDER BY rr.created_at DESC
            "#,
            user_id as _
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
        // Check if the role binding has auto-approve enabled
        let auto_approve = sqlx::query_scalar!(
            r#"SELECT auto_approve FROM role_bindings WHERE id = $1"#,
            role_binding_id as _
        )
        .fetch_one(&mut **pool)
        .await?;

        let status = if auto_approve {
            RoleRequestStatus::Approved
        } else {
            RoleRequestStatus::Pending
        };

        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_requests (role_binding_id, user_id, notes, status)
               VALUES ($1, $2, $3, $4) RETURNING id"#,
            role_binding_id as _,
            user_id as _,
            notes,
            status as _
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
    _uri: &Origin<'_>,
    data: Data<'_>,
    ctx: Context<'_>,
    csrf: Option<CsrfToken>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Roles, false)
        .await?;

    let content = if let Some(ref me) = me {
        if data.organizers(&mut transaction).await?.contains(me) {
            // Check if event uses custom role bindings
            let uses_custom_bindings = sqlx::query_scalar!(
                r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
                data.series as _,
                &data.event
            )
            .fetch_optional(&mut *transaction)
            .await?
            .unwrap_or(Some(true)).unwrap_or(true);

            let (pending_requests, approved_requests) = if uses_custom_bindings {
                // Use event-specific role requests
                let all_role_requests = RoleRequest::for_event(&mut transaction, data.series, &data.event).await?;
                let pending = all_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Pending)).cloned().collect::<Vec<_>>();
                let approved = all_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Approved)).cloned().collect::<Vec<_>>();
                (pending, approved)
            } else {
                // Use game role bindings - get all approved volunteers for the game
                let game = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)?;
                if let Some(game) = game {
                    let all_game_role_requests = RoleRequest::for_game(&mut transaction, game.id).await?;
                    let pending = all_game_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Pending)).cloned().collect::<Vec<_>>();
                    let approved = all_game_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Approved)).cloned().collect::<Vec<_>>();
                    (pending, approved)
                } else {
                    (Vec::new(), Vec::new())
                }
            };
            let all_role_types = RoleType::all(&mut transaction).await?;

            // Get game info if using game bindings
            let game_info = if !uses_custom_bindings {
                game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)?
            } else {
                None
            };

            // Get event overrides if using game bindings
            let event_overrides = if !uses_custom_bindings {
                EventDiscordRoleOverride::for_event(&mut transaction, data.series, &data.event).await?
            } else {
                Vec::new()
            };

            let effective_role_bindings = EffectiveRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;

            html! {
                h2 : "Role Management";
                p : "Manage volunteer roles for this event.";

                @if !uses_custom_bindings {
                    div(class = "info-box") {
                        h3 : "Using Game Volunteer roles";
                        p {
                            : "This event is using global volunteer rolesfrom ";
                            @if let Some(ref game) = game_info {
                                strong : &game.display_name;
                            } else {
                                strong : "the game";
                            }
                            : " and only allows event-specific Discord roles to be attached.";
                        }
                    }
                }

                h3 : "Current Volunteer Roles";
                @if effective_role_bindings.is_empty() {
                    p : "No volunteer roles configured yet.";
                } else {
                    table {
                        thead {
                            tr {
                                th : "Role Type";
                                th : "Min Count";
                                th : "Max Count";
                                th : "Discord Role";
                                th : "Actions";
                            }
                        }
                        tbody {
                            @for binding in &effective_role_bindings {
                                tr {
                                    td : binding.role_type_name;
                                    td : binding.min_count;
                                    td : binding.max_count;
                                    td {
                                        @if let Some(discord_role_id) = binding.discord_role_id {
                                            : format!("{}", discord_role_id);
                                            @if binding.has_event_override {
                                                span(class = "override-indicator") : " (event specific)";
                                            } else if binding.is_game_binding {
                                                span(class = "game-indicator") : " (defined by game)";
                                            }
                                        } else {
                                            : "None";
                                        }
                                    }
                                    td {
                                        @if binding.is_game_binding {
                                            p(class = "game-binding-info") {
                                                : "This role is managed by the game's role binding system";
                                                @if binding.has_event_override {
                                                    : " with event-specific Discord role override";
                                                }
                                            }
                                            @if !uses_custom_bindings {
                                                @let is_disabled = EventDisabledRoleBinding::exists_for_role_type(&mut transaction, data.series, &data.event, binding.role_type_id).await?;
                                                @if is_disabled {
                                                    @let (errors, enable_button) = button_form(
                                                        uri!(enable_role_binding(data.series, &*data.event, binding.role_type_id)),
                                                        csrf.as_ref(),
                                                        Vec::new(),
                                                        "Enable"
                                                    );
                                                    : errors;
                                                    div(class = "button-row") : enable_button;
                                                } else {
                                                    @let (errors, disable_button) = button_form(
                                                        uri!(disable_role_binding(data.series, &*data.event, binding.role_type_id)),
                                                        csrf.as_ref(),
                                                        Vec::new(),
                                                        "Disable"
                                                    );
                                                    : errors;
                                                    div(class = "button-row") : disable_button;
                                                }
                                                
                                                @if binding.has_event_override {
                                                    @let (errors, remove_override_button) = button_form(
                                                        uri!(delete_discord_override(data.series, &*data.event, binding.role_type_id)),
                                                        csrf.as_ref(),
                                                        Vec::new(),
                                                        "Remove Discord Override"
                                                    );
                                                    : errors;
                                                    div(class = "button-row") : remove_override_button;
                                                }
                                            }
                                        } else {
                                            @let (errors, delete_button) = button_form(
                                                uri!(delete_role_binding(data.series, &*data.event, binding.id)),
                                                csrf.as_ref(),
                                                Vec::new(),
                                                "Delete"
                                            );
                                            : errors;
                                            div(class = "button-row") : delete_button;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                
                @if uses_custom_bindings {
                    h3 : "Add Volunteer Role";
                    @let mut errors = ctx.errors().collect_vec();
                    : full_form(uri!(add_role_binding(data.series, &*data.event)), csrf.as_ref(), html! {
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
                        : form_field("auto_approve", &mut errors, html! {
                            label(for = "auto_approve") : "Auto-approve requests:";
                            input(type = "checkbox", name = "auto_approve", id = "auto_approve");
                            span(class = "help-text") : "When enabled, role requests for this binding will be automatically approved without manual review.";
                        });
                    }, errors, "Add Role Binding");
                } else {
                    h3 : "Discord Role Overrides";
                    p : "You can override Discord role IDs for specific role types while using the game's role binding structure.";
                    
                    @if event_overrides.is_empty() {
                        p : "No Discord role overrides configured.";
                    } else {
                        table {
                            thead {
                                tr {
                                    th : "Role Type";
                                    th : "Discord Role ID";
                                    th : "Actions";
                                }
                            }
                            tbody {
                                @for override_item in &event_overrides {
                                    tr {
                                        td {
                                            @if let Some(role_type) = all_role_types.iter().find(|rt| rt.id == override_item.role_type_id) {
                                                : role_type.name;
                                            } else {
                                                : "Unknown";
                                            }
                                        }
                                        td : override_item.discord_role_id;
                                        td {
                                            @let (errors, button) = button_form(
                                                uri!(delete_discord_override(data.series, &*data.event, override_item.role_type_id)),
                                                csrf.as_ref(),
                                                Vec::new(),
                                                "Remove"
                                            );
                                            : errors;
                                            div(class = "button-row") : button;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    h4 : "Add Discord Role Override";
                    @let mut errors = ctx.errors().collect_vec();
                    @let available_role_types = effective_role_bindings.iter()
                        .filter(|binding| binding.is_game_binding && !binding.has_event_override)
                        .map(|binding| RoleType { id: binding.role_type_id, name: binding.role_type_name.clone() })
                        .collect::<Vec<_>>();
                    @if !available_role_types.is_empty() {
                        : full_form(uri!(add_discord_override_form(data.series, &*data.event, available_role_types[0].id)), csrf.as_ref(), html! {
                            : form_field("role_type_id", &mut errors, html! {
                                label(for = "role_type_id") : "Role Type:";
                                select(name = "role_type_id", id = "role_type_id") {
                                    @for role_type in available_role_types {
                                        option(value = role_type.id.to_string()) : role_type.name;
                                    }
                                }
                            });
                            : form_field("discord_role_id", &mut errors, html! {
                                label(for = "discord_role_id") : "Discord Role ID:";
                                input(type = "text", name = "discord_role_id", id = "discord_role_id", placeholder = "e.g. 123456789012345678", required);
                            });
                        }, errors, "Add Override");
                    } else {
                        p : "No game role bindings available for Discord role override.";
                    }

                    h3 : "Disabled Role Bindings";
                    @let disabled_bindings = EventDisabledRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;
                    @if disabled_bindings.is_empty() {
                        p : "No role bindings are currently disabled for this event.";
                    } else {
                        p : "The following game role bindings are disabled for this event:";
                        table {
                            thead {
                                tr {
                                    th : "Role Type";
                                    th : "Actions";
                                }
                            }
                            tbody {
                                @for disabled_binding in &disabled_bindings {
                                    @let role_type = RoleType::from_id(&mut transaction, disabled_binding.role_type_id).await?;
                                    @if let Some(role_type) = role_type {
                                        tr {
                                            td : role_type.name;
                                            td {
                                                @let (errors, enable_button) = button_form(
                                                    uri!(enable_role_binding(data.series, &*data.event, disabled_binding.role_type_id)),
                                                    csrf.as_ref(),
                                                    Vec::new(),
                                                    "Enable"
                                                );
                                                : errors;
                                                div(class = "button-row") : enable_button;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                @if !pending_requests.is_empty() {
                    h3 : "Pending Role Requests";
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
                            @for request in &pending_requests {
                                @let user = User::from_id(&mut *transaction, request.user_id).await?;
                                @if let Some(user) = user {
                                    tr {
                                        td : user.display_name();
                                        td : request.role_type_name;
                                        td {
                                            @if let Some(ref notes) = request.notes {
                                                : notes;
                                            } else {
                                                : "No notes";
                                            }
                                        }
                                        td : format_datetime(request.created_at, DateTimeFormat { long: false, running_text: false });
                                        td {
                                            @let (errors, approve_button) = button_form(
                                                uri!(approve_role_request(data.series, &*data.event, request.id)),
                                                csrf.as_ref(),
                                                Vec::new(),
                                                "Approve"
                                            );
                                            : errors;
                                            div(class = "button-row") : approve_button;
                                            @let (errors, reject_button) = button_form(
                                                uri!(reject_role_request(data.series, &*data.event, request.id)),
                                                csrf.as_ref(),
                                                Vec::new(),
                                                "Reject"
                                            );
                                            : errors;
                                            div(class = "button-row") : reject_button;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                @if !approved_requests.is_empty() {
                    h3 : "Volunteer Pool";
                    @let approved_by_role_type = approved_requests.iter()
                        .filter(|request| {
                            // For game bindings, only show if the role type is not disabled
                            if request.series.is_none() && request.event.is_none() {
                                // We'll check this separately in the template
                                true
                            } else {
                                true // Always show event-specific requests
                            }
                        })
                        .fold(HashMap::<String, Vec<_>>::new(), |mut acc, request| {
                            acc.entry(request.role_type_name.clone()).or_insert_with(Vec::new).push(request);
                            acc
                        });
                    @for (role_type_name, requests) in approved_by_role_type {
                        @let role_type = RoleType::from_name(&mut transaction, &requests[0].role_type_name).await?;
                        @let is_disabled = if let Some(role_type) = role_type {
                            EventDisabledRoleBinding::exists_for_role_type(&mut transaction, data.series, &data.event, role_type.id).await?
                        } else {
                            false
                        };
                        @if !is_disabled {
                            details {
                                summary : format!("{} ({})", role_type_name, requests.len());
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
                                        @for request in requests {
                                            @let user = User::from_id(&mut *transaction, request.user_id).await?;
                                            @if let Some(user) = user {
                                                tr {
                                                    td : user.display_name();
                                                    td {
                                                        @if let Some(ref notes) = request.notes {
                                                            : notes;
                                                        } else {
                                                            : "No notes";
                                                        }
                                                    }
                                                    td : format_datetime(request.updated_at, DateTimeFormat { long: false, running_text: false });
                                                    td {
                                                        @if request.series.is_none() && request.event.is_none() {
                                                            p(class = "game-binding-info") : "Globally managed role assignment - no editing here";
                                                        } else {
                                                                                                        @let (errors, revoke_button) = button_form(
                                                    uri!(revoke_role_request(data.series, &*data.event)),
                                                    csrf.as_ref(),
                                                    Vec::new(),
                                                    "Revoke"
                                                );
                                                            : errors;
                                                            div(class = "button-row") : revoke_button;
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
                    p : "This page is for organizers of this event only.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = "/login") : "Sign in";
                    : " to manage roles for this event.";
                }
            }
        }
    };
    Ok(page(
        transaction,
        &me,
        &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", data.series.slug(), data.event)).unwrap()),
        PageStyle::default(),
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
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    let uri = HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap();
    Ok(roles_page(transaction, me, &Origin(uri.clone()), data, ctx, None).await?)
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
    auto_approve: bool,
}

#[rocket::post("/event/<series>/<event>/roles/add-binding", data = "<form>")]
pub(crate) async fn add_role_binding(
    pool: &State<PgPool>,
    me: User,
    _uri: &HttpOrigin<'_>,
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
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
                                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                                data,
                                form.context,
                                csrf,
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
                value.auto_approve,
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/<binding>/delete", data = "<form>")]
pub(crate) async fn delete_role_binding(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    binding: Id<RoleBindings>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
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
    series: Series,
    event: &str,
    request: Id<RoleRequests>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
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
                r#"SELECT rb.id as "id: Id<RoleBindings>", rb.series as "series: Series", rb.event, rb.game_id, rb.role_type_id as "role_type_id: Id<RoleTypes>", rb.min_count, rb.max_count, rt.name as role_type_name, rb.discord_role_id, rb.auto_approve FROM role_bindings rb JOIN role_types rt ON rb.role_type_id = rt.id WHERE rb.id = $1"#,
                i64::from(role_request.role_binding_id) as i32
            )
            .fetch_optional(&mut *transaction)
            .await?;

            // Update the role request status
            RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Approved).await?;

            // If there's a Discord role ID, assign the role
            if let Some(binding) = role_binding {
                if let Some(discord_role_id) = binding.discord_role_id {
                    let user = User::from_id(&mut *transaction, role_request.user_id).await?
                        .ok_or(StatusOrError::Status(Status::NotFound))?;
                    
                    if let Some(discord_user) = user.discord {
                        // Get the Discord context and guild
                        let discord_ctx = discord_ctx.read().await;
                        if let Some(discord_guild) = data.discord_guild {
                            if let Ok(member) = discord_guild.member(&*discord_ctx, discord_user.id).await {
                                if let Err(e) = member.add_role(&*discord_ctx, RoleId::new(discord_role_id.try_into().unwrap())).await {
                                    eprintln!("Failed to assign Discord role {} to user {}: {}", discord_role_id, discord_user.id, e);
                                }
                            }
                        }
                    }
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/<request>/reject", data = "<form>")]
pub(crate) async fn reject_role_request(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    request: Id<RoleRequests>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
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
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
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
                return Ok(RedirectOrContent::Content(
                    volunteer_page(
                        transaction,
                        Some(me),
                        &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                        data,
                        form.context,
                        csrf,
                    )
                    .await?,
                ));
            }

            // Get the role binding details for the notification
            let role_binding = sqlx::query_as!(
                RoleBinding,
                r#"SELECT rb.id as "id: Id<RoleBindings>", rb.series as "series: Series", rb.event as "event!", rb.game_id,
                          rb.role_type_id as "role_type_id: Id<RoleTypes>", rb.min_count as "min_count!", 
                          rb.max_count as "max_count!", rt.name as "role_type_name!", rb.discord_role_id, rb.auto_approve
                       FROM role_bindings rb
                       JOIN role_types rt ON rb.role_type_id = rt.id
                       WHERE rb.id = $1"#,
                i64::from(value.role_binding_id) as i32
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
    } else {
        RedirectOrContent::Content(
            volunteer_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

async fn volunteer_page(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    _uri: &Origin<'_>,
    data: Data<'_>,
    _ctx: Context<'_>,
    csrf: Option<CsrfToken>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Volunteer, false)
        .await?;

    let content = if let Some(ref me) = me {
        // Check if user has access to view volunteer signups
        let is_organizer = data.organizers(&mut transaction).await?.contains(me);
        let is_restreamer = data.restreamers(&mut transaction).await?.contains(me);
        // Check if event uses custom role bindings
        let uses_custom_bindings = sqlx::query_scalar!(
            r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
            data.series as _,
            &data.event
        )
        .fetch_optional(&mut *transaction)
        .await?
        .unwrap_or(Some(true)).unwrap_or(true);

        let has_confirmed_roles = if uses_custom_bindings {
            // Check for event-specific role requests
            let my_requests = RoleRequest::for_user(&mut transaction, me.id).await?;
            my_requests.iter().any(|req| {
                matches!(req.status, RoleRequestStatus::Approved)
                    && req.series == Some(data.series)
                    && req.event == Some(data.event.to_string())
            })
        } else {
            // Check for game role requests
            let game = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)?;
            if let Some(_game) = game {
                let my_requests = RoleRequest::for_user(&mut transaction, me.id).await?;
                my_requests.iter().any(|req| {
                    matches!(req.status, RoleRequestStatus::Approved)
                        && u64::from(req.role_binding_id) > 0 // Valid role binding ID
                })
            } else {
                false
            }
        };

        if !is_organizer && !is_restreamer && !has_confirmed_roles {
            // User doesn't have access - show appropriate message
            html! {
                article {
                    h2 : "Volunteer Signups";
                    p : "You need to be an organizer, restreamer, or have confirmed roles for this event to view volunteer signups.";
                    p {
                        a(href = uri!(volunteer_page_get(data.series, &*data.event))) : "Apply for volunteer roles";
                    }
                }
            }
        } else {
            // User has access - show the full volunteer interface
            let effective_role_bindings = EffectiveRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;
            let my_requests = RoleRequest::for_user(&mut transaction, me.id).await?;
            let my_approved_roles = if uses_custom_bindings {
                // For custom bindings, show event-specific approved roles
                my_requests
                    .iter()
                    .filter(|req| {
                        matches!(req.status, RoleRequestStatus::Approved)
                            && req.series == Some(data.series)
                                                         && req.event == Some(data.event.to_string())
                    })
                    .collect::<Vec<_>>()
            } else {
                // For game bindings, show all approved roles for the game
                my_requests
                    .iter()
                    .filter(|req| {
                        matches!(req.status, RoleRequestStatus::Approved)
                    })
                    .collect::<Vec<_>>()
            };

            let upcoming_races = Race::for_event(&mut transaction, &reqwest::Client::new(), &data).await?;

            html! {
                h2 : "Volunteer for Roles";
                
                @if !uses_custom_bindings {
                    div(class = "game-binding-notice") {
                        h3 : "Game Role Bindings";
                        p : "This event uses game-level role bindings. To volunteer for roles, you need to apply for game roles instead of event-specific roles.";
                        p {
                            : "Please visit the ";
                            a(href = uri!(crate::games::manage_roles(data.series.slug()))) : "game role management page";
                            : " to apply for roles that will be available across all events for this game.";
                        }
                    }
                } else {
                    p : "Apply to volunteer for roles in this event.";
                }

                @if effective_role_bindings.is_empty() {
                    p : "No volunteer roles are currently available for this event.";
                } else {
                    h3 : "Available Roles";
                    @for binding in &effective_role_bindings {
                        @let my_request = my_requests.iter()
                            .filter(|req| req.role_binding_id == binding.id && !matches!(req.status, RoleRequestStatus::Aborted))
                            .max_by_key(|req| req.created_at);
                        @let has_active_request = my_request.map_or(false, |req| matches!(req.status, RoleRequestStatus::Pending | RoleRequestStatus::Approved));
                        div(class = "role-binding") {
                            h4 : binding.role_type_name;
                            p {
                                : "Required: ";
                                : binding.min_count;
                                : " - ";
                                : binding.max_count;
                                : " volunteers";
                            }
                            @if let Some(discord_role_id) = binding.discord_role_id {
                                p {
                                    : "Discord Role: ";
                                    : format!("{}", discord_role_id);
                                    @if binding.has_event_override {
                                        span(class = "override-indicator") : " (event specific)";
                                    } else if binding.is_game_binding {
                                        span(class = "game-indicator") : " (defined by game)";
                                    }
                                }
                            }
                            @if binding.auto_approve {
                                p(class = "auto-approve-indicator") {
                                    : "Auto-approve enabled";
                                }
                            }
                            @if binding.is_game_binding {
                                p(class = "game-binding-info") {
                                    : "This role is managed by the game's role binding system";
                                    @if binding.has_event_override {
                                        : " with event-specific Discord role override";
                                    }
                                }
                            }

                            @if has_active_request {
                                @let request = my_request.unwrap();
                                p(class = "request-status") {
                                    : "Your request status: ";
                                    @match request.status {
                                        RoleRequestStatus::Pending => {
                                            span(class = "status-pending") : "Pending";
                                        }
                                        RoleRequestStatus::Approved => {
                                            span(class = "status-approved") : "Approved";
                                        }
                                        RoleRequestStatus::Rejected => {
                                            span(class = "status-rejected") : "Rejected";
                                        }
                                        RoleRequestStatus::Aborted => {
                                            span(class = "status-aborted") : "Aborted";
                                        }
                                    }
                                }
                                @if let Some(ref notes) = request.notes {
                                    p(class = "request-notes") {
                                        : "Your notes: ";
                                        : notes;
                                    }
                                }
                            } else {
                                @let mut errors = Vec::new();
                                : full_form(uri!(apply_for_role(data.series, &*data.event)), csrf.as_ref(), html! {
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
                    p : "You have been approved for the following roles. You can now sign up for specific matches:";
                    
                    @for role_request in my_approved_roles {
                        @let binding = effective_role_bindings.iter().find(|b| b.id == role_request.role_binding_id);
                        @if let Some(binding) = binding {
                            h4 : format!("{} - {}", binding.role_type_name, role_request.role_type_name);
                            @let available_races = upcoming_races.iter().filter(|race| {
                                // Filter races that need this role type
                                // This is a simplified check - you might want more sophisticated logic
                                true
                            }).collect::<Vec<_>>();
                            
                            @if available_races.is_empty() {
                                p : "No upcoming races available for signup.";
                            } else {
                                ul {
                                    @for race in available_races {
                                        li {
                                            a(href = uri!(match_signup_page_get(data.series, &*data.event, race.id))) : match &race.entrants {
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
                                                _ => "TBD vs TBD".to_string(),
                                            };
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
                    a(href = "/login") : "Sign in";
                    : " to volunteer for roles in this event.";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", data.series.slug(), data.event)).unwrap()),
        PageStyle::default(),
        &format!("Volunteer — {}", data.display_name),
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
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    let uri = HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap();
    Ok(volunteer_page(transaction, me, &Origin(uri.clone()), data, ctx, None).await?)
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
    series: Series,
    event: &str,
    race_id: Id<Races>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                    data,
                    race_id,
                    form.context,
                    csrf,
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
                series, &*event, race_id
            ))))
        }
    } else {
        RedirectOrContent::Content(
            match_signup_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                data,
                race_id,
                form.context,
                csrf,
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
    series: Series,
    event: &str,
    race_id: Id<Races>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                    data,
                    race_id,
                    form.context,
                    csrf,
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
                            &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                            data,
                            race_id,
                            form.context,
                            csrf,
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
                    
                    // Format race description with round info
                    let race_description = match &race.entrants {
                        cal::Entrants::Two([team1, team2]) => {
                            let matchup = format!("{} vs {}",
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
                            );
                            if let Some(round) = &race.round {
                                format!("{} ({})", matchup, round)
                            } else {
                                matchup
                            }
                        },
                        cal::Entrants::Three([team1, team2, team3]) => {
                            let matchup = format!("{} vs {} vs {}",
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
                            );
                            if let Some(round) = &race.round {
                                format!("{} ({})", matchup, round)
                            } else {
                                matchup
                            }
                        },
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
                    msg.push_timestamp(race_start_time, serenity_utils::message::TimestampStyle::LongDateTime);
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
                    
                    // Add restream information if the race has restream URLs
                    if !race.video_urls.is_empty() {
                        msg.push("\n**The race will be restreamed on:**\n");
                        for (language, video_url) in &race.video_urls {
                            msg.push("**");
                            msg.push(&language.to_string());
                            msg.push(":** ");
                            msg.push("<");
                            msg.push(&video_url.to_string());
                            msg.push(">");
                            msg.push("\n");
                        }
                    } else {
                        msg.push("\nNo restream has been scheduled for this race so far.");
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                data,
                race_id,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

async fn match_signup_page(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    _uri: &Origin<'_>,
    data: Data<'_>,
    race_id: Id<Races>,
    _ctx: Context<'_>,
    csrf: Option<CsrfToken>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Races, true)
        .await?;

    // Get race details
    let race = Race::from_id(&mut transaction, &reqwest::Client::new(), race_id).await?;
    let signups = Signup::for_race(&mut transaction, race_id).await?;
    let effective_role_bindings = EffectiveRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;

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
            p {
                : timezone_info_html();
            }

            @if can_manage {
                h3 : "Manage Signups";
                @for signup in &signups {
                    @if let Some(user) = User::from_id(&mut *transaction, signup.user_id).await? {
                        div(class = "signup-item") {
                            p {
                                strong : user.display_name();
                                : " - ";
                                : signup.role_type_name;
                                : " (";
                                @match signup.status {
                                    VolunteerSignupStatus::Pending => : "Pending";
                                    VolunteerSignupStatus::Confirmed => : "Confirmed";
                                    VolunteerSignupStatus::Declined => : "Declined";
                                    VolunteerSignupStatus::Aborted => : "Aborted";
                                }
                                : ")";
                            }
                            @if let Some(ref notes) = signup.notes {
                                p(class = "signup-notes") : notes;
                            }
                            @if matches!(signup.status, VolunteerSignupStatus::Pending) {
                                div(class = "signup-actions") {
                                    @let (errors, confirm_button) = button_form_ext(
                                        uri!(manage_roster(data.series, &*data.event, race_id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        html! {
                                            input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                            input(type = "hidden", name = "action", value = "confirm");
                                        },
                                        "Confirm"
                                    );
                                    : errors;
                                    : confirm_button;
                                    @let (errors, decline_button) = button_form_ext(
                                        uri!(manage_roster(data.series, &*data.event, race_id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        html! {
                                            input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                            input(type = "hidden", name = "action", value = "decline");
                                        },
                                        "Decline"
                                    );
                                    : errors;
                                    : decline_button;
                                }
                            }
                        }
                    }
                }
            }

            h3 : "Role Signups";
            @for binding in &effective_role_bindings {
                div(class = "role-binding") {
                    h4 : binding.role_type_name;
                    p {
                        : "Required: ";
                        : binding.min_count;
                        : " - ";
                        : binding.max_count;
                        : " volunteers";
                    }
                    @if let Some(discord_role_id) = binding.discord_role_id {
                        p {
                            : "Discord Role: ";
                            : format!("{}", discord_role_id);
                            @if binding.has_event_override {
                                span(class = "override-indicator") : " (event specific)";
                            } else if binding.is_game_binding {
                                span(class = "game-indicator") : " (defined by game)";
                            }
                        }
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
                                @if let Some(user) = User::from_id(&mut *transaction, signup.user_id).await? {
                                    li {
                                        : user.display_name();
                                        @if let Some(ref notes) = signup.notes {
                                            : " - ";
                                            : notes;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    @if !pending_signups.is_empty() {
                        h5 : "Pending Volunteers";
                        ul {
                            @for signup in pending_signups {
                                @if let Some(user) = User::from_id(&mut *transaction, signup.user_id).await? {
                                    li {
                                        : user.display_name();
                                        @if let Some(ref notes) = signup.notes {
                                            : " - ";
                                            : notes;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    @if let Some(ref me) = Some(me) {
                        @let my_signup = role_signups.iter().find(|s| s.user_id == me.id);
                        @if my_signup.is_none() {
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
                                    csrf.as_ref(),
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
                                : full_form(uri!(signup_for_match(data.series, &*data.event, race_id)), csrf.as_ref(), html! {
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
                        p(class = "signup-count") {
                            : format!("{}/{} confirmed volunteers", confirmed_signups.len(), binding.max_count);
                        }
                    }
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = "/login") : "Sign in";
                    : " to view volunteer signups for this race.";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", data.series.slug(), data.event)).unwrap()),
        PageStyle::default(),
        &format!("Race Signups — {}", data.display_name),
        html! {
            : header;
            : content;
        },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/races/<race_id>/signups")]
pub(crate) async fn match_signup_page_get(
    pool: &State<PgPool>,
    me: Option<User>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    let uri = HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap();
    Ok(match_signup_page(transaction, me, &Origin(uri.clone()), data, race_id, ctx, None).await?)
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
    series: Series,
    event: &str,
    race_id: Id<Races>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                    data,
                    race_id,
                    form.context,
                    csrf,
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                data,
                race_id,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/races/<race_id>/revoke-signup", data = "<form>")]
pub(crate) async fn revoke_signup(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    csrf: Option<CsrfToken>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                    data,
                    race_id,
                    form.context,
                    csrf,
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap()),
                data,
                race_id,
                form.context,
                csrf,
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
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
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
                roles_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                )
                .await?,
            )
        } else {
            // Update the role request status to Aborted
            RoleRequest::update_status(&mut transaction, value.request_id, RoleRequestStatus::Aborted).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/revoke-role-request", data = "<form>")]
pub(crate) async fn revoke_role_request(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
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

        if request.series != Some(series) || request.event != Some(event.to_string()) {
            form.context.push_error(form::Error::validation(
                "Invalid role request for this event",
            ));
        }

        // Prevent revoking game role bindings
        if request.series.is_none() && request.event.is_none() {
            form.context.push_error(form::Error::validation(
                "Cannot revoke globally managed role assignments",
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
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
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
            )
            .await?,
        )
    })
}

#[allow(dead_code)]
impl EventDiscordRoleOverride {
    pub(crate) async fn for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT
                    id AS "id: Id<EventDiscordRoleOverrides>",
                    series AS "series: Series",
                    event,
                    role_type_id AS "role_type_id: Id<RoleTypes>",
                    discord_role_id,
                    created_at AS "created_at!",
                    updated_at AS "updated_at!"
                FROM event_discord_role_overrides
                WHERE series = $1 AND event = $2
                ORDER BY role_type_id
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
        discord_role_id: i64,
    ) -> sqlx::Result<Id<EventDiscordRoleOverrides>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO event_discord_role_overrides (series, event, role_type_id, discord_role_id)
               VALUES ($1, $2, $3, $4) RETURNING id"#,
            series as _,
            event,
            role_type_id as _,
            discord_role_id
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(Id::from(id as i64))
    }

    pub(crate) async fn delete(
        pool: &mut Transaction<'_, Postgres>,
        id: Id<EventDiscordRoleOverrides>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"DELETE FROM event_discord_role_overrides WHERE id = $1"#,
            id as _
        )
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
        let result = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM event_discord_role_overrides WHERE series = $1 AND event = $2 AND role_type_id = $3)"#,
            series as _,
            event,
            role_type_id as _
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(result.unwrap_or(false))
    }

    pub(crate) async fn delete_for_role_type(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_type_id: Id<RoleTypes>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"DELETE FROM event_discord_role_overrides WHERE series = $1 AND event = $2 AND role_type_id = $3"#,
            series as _,
            event,
            role_type_id as _
        )
        .execute(&mut **pool)
        .await?;
        Ok(())
    }
}

impl EventDisabledRoleBinding {
    pub(crate) async fn for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            Self,
            r#"
                SELECT
                    id AS "id: Id<EventDisabledRoleBindings>",
                    series AS "series: Series",
                    event,
                    role_type_id AS "role_type_id: Id<RoleTypes>",
                    created_at AS "created_at!"
                FROM event_disabled_role_bindings
                WHERE series = $1 AND event = $2
                ORDER BY role_type_id
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
    ) -> sqlx::Result<Id<EventDisabledRoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO event_disabled_role_bindings (series, event, role_type_id)
               VALUES ($1, $2, $3) RETURNING id"#,
            series as _,
            event,
            role_type_id as _
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(Id::from(id as i64))
    }

    pub(crate) async fn delete(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_type_id: Id<RoleTypes>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"DELETE FROM event_disabled_role_bindings WHERE series = $1 AND event = $2 AND role_type_id = $3"#,
            series as _,
            event,
            role_type_id as _
        )
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
        let result = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM event_disabled_role_bindings WHERE series = $1 AND event = $2 AND role_type_id = $3)"#,
            series as _,
            event,
            role_type_id as _
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(result.unwrap_or(false))
    }
}

impl EffectiveRoleBinding {
    pub(crate) async fn for_event(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> sqlx::Result<Vec<Self>> {
        // Get event-specific role bindings
        let event_bindings = sqlx::query_as!(
            Self,
            r#"
                SELECT
                    rb.id AS "id: Id<RoleBindings>",
                    rb.role_type_id AS "role_type_id: Id<RoleTypes>",
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.discord_role_id,
                    rb.auto_approve,
                    false AS "is_game_binding!: bool",
                    false AS "has_event_override!: bool",
                    false AS "is_disabled!: bool"
                FROM role_bindings rb
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.series = $1 AND rb.event = $2
                ORDER BY rt.name
            "#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await?;

        // Get game role bindings
        let game = match game::Game::from_series(&mut *pool, series).await {
            Ok(game) => game,
            Err(_) => None, // If we can't get the game, just continue without game bindings
        };
        let game_bindings = if let Some(game) = game {
            sqlx::query_as!(
                Self,
                r#"
                    SELECT
                        rb.id AS "id: Id<RoleBindings>",
                        rb.role_type_id AS "role_type_id: Id<RoleTypes>",
                        rb.min_count,
                        rb.max_count,
                        rt.name AS "role_type_name",
                        rb.discord_role_id,
                        rb.auto_approve,
                        true AS "is_game_binding!: bool",
                        false AS "has_event_override!: bool",
                        false AS "is_disabled!: bool"
                    FROM role_bindings rb
                    JOIN role_types rt ON rb.role_type_id = rt.id
                    WHERE rb.game_id = $1
                    ORDER BY rt.name
                "#,
                game.id
            )
            .fetch_all(&mut **pool)
            .await?
        } else {
            Vec::new()
        };

        // Get event Discord role overrides
        let discord_overrides = EventDiscordRoleOverride::for_event(&mut *pool, series, event).await?;
        let discord_override_map: HashMap<Id<RoleTypes>, i64> = discord_overrides
            .into_iter()
            .map(|override_| (override_.role_type_id, override_.discord_role_id))
            .collect();

        // Get disabled role bindings for this event
        let disabled_bindings = EventDisabledRoleBinding::for_event(&mut *pool, series, event).await?;
        let disabled_role_types: HashSet<Id<RoleTypes>> = disabled_bindings
            .into_iter()
            .map(|binding| binding.role_type_id)
            .collect();

        // Combine and process all bindings
        let mut all_bindings = Vec::new();
        
        // Add event-specific bindings
        for mut binding in event_bindings {
            binding.has_event_override = discord_override_map.contains_key(&binding.role_type_id);
            if binding.has_event_override {
                binding.discord_role_id = discord_override_map.get(&binding.role_type_id).copied();
            }
            all_bindings.push(binding);
        }

        // Add game bindings (excluding disabled ones)
        for mut binding in game_bindings {
            if !disabled_role_types.contains(&binding.role_type_id) {
                binding.has_event_override = discord_override_map.contains_key(&binding.role_type_id);
                if binding.has_event_override {
                    binding.discord_role_id = discord_override_map.get(&binding.role_type_id).copied();
                }
                all_bindings.push(binding);
            }
        }

        Ok(all_bindings)
    }
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DisableRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EnableRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/role-types/<role_type_id>/disable-binding")]
pub(crate) async fn disable_role_binding(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    role_type_id: Id<RoleTypes>,
    _csrf: Option<CsrfToken>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    if !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    // Check if this is a game role binding that can be disabled
    let game = game::Game::from_series(&mut transaction, series).await?;
    if let Some(game) = game {
        // Check if this role type exists in the game's role bindings
        let game_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await?;
        let role_type_exists = game_bindings.iter().any(|binding| binding.role_type_id == role_type_id);
        
        if !role_type_exists {
            return Err(StatusOrError::Status(Status::BadRequest));
        }

        // Check if already disabled
        if EventDisabledRoleBinding::exists_for_role_type(&mut transaction, series, event, role_type_id).await? {
            return Err(StatusOrError::Status(Status::BadRequest));
        }

        // Disable the role binding
        EventDisabledRoleBinding::create(&mut transaction, series, event, role_type_id).await?;
    }

    transaction.commit().await?;
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))))
}

#[rocket::post("/event/<series>/<event>/role-types/<role_type_id>/enable-binding")]
pub(crate) async fn enable_role_binding(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    role_type_id: Id<RoleTypes>,
    _csrf: Option<CsrfToken>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    if !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    // Check if this role binding is currently disabled
    if !EventDisabledRoleBinding::exists_for_role_type(&mut transaction, series, event, role_type_id).await? {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    // Enable the role binding
    EventDisabledRoleBinding::delete(&mut transaction, series, event, role_type_id).await?;

    transaction.commit().await?;
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))))
}

#[rocket::get("/event/<series>/<event>/role-types/<role_type_id>/discord-override")]
pub(crate) async fn add_discord_override_form(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    role_type_id: Id<RoleTypes>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    if !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    // Check if this is a game role binding that can have Discord overrides
    let game = game::Game::from_series(&mut transaction, series).await?;
    if let Some(game) = game {
        // Check if this role type exists in the game's role bindings
        let game_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await?;
        let role_type_exists = game_bindings.iter().any(|binding| binding.role_type_id == role_type_id);
        
        if !role_type_exists {
            return Err(StatusOrError::Status(Status::BadRequest));
        }
    }

    Ok(html! {
        h1 : "Add Discord Role Override";
        p : "Set a custom Discord role for this game role binding.";
        form(method = "post", action = uri!(add_discord_override(series, event, role_type_id))) {
            label {
                : "Discord Role ID: ";
                input(type = "text", name = "discord_role_id", required);
            }
            button(type = "submit") : "Add Override";
        }
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddDiscordOverrideForm {
    #[field(default = String::new())]
    csrf: String,
    discord_role_id: String,
}

#[rocket::post("/event/<series>/<event>/role-types/<role_type_id>/discord-override", data = "<form>")]
pub(crate) async fn add_discord_override(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    role_type_id: Id<RoleTypes>,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, AddDiscordOverrideForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to manage roles for this event.",
            ));
        }

        // Check if this is a game role binding that can have Discord overrides
        let game = game::Game::from_series(&mut transaction, series).await?;
        if let Some(game) = game {
            // Check if this role type exists in the game's role bindings
            let game_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await?;
            let role_type_exists = game_bindings.iter().any(|binding| binding.role_type_id == role_type_id);
            
            if !role_type_exists {
                form.context.push_error(form::Error::validation(
                    "This role type does not exist in the game's role bindings.",
                ));
            }
        }

        // Check if an override already exists for this role type
        if EventDiscordRoleOverride::exists_for_role_type(&mut transaction, series, event, role_type_id).await? {
            form.context.push_error(form::Error::validation(
                "A Discord role override already exists for this role type.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                )
                .await?,
            )
        } else {
            // Parse the Discord role ID
            let discord_role_id = match value.discord_role_id.parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    form.context.push_error(form::Error::validation(
                        "Discord role ID must be a valid number.",
                    ));
                    return Ok(RedirectOrContent::Content(
                        roles_page(
                            transaction,
                            Some(me),
                            &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}", series.slug(), event)).unwrap()),
                            data,
                            form.context,
                            csrf,
                        )
                        .await?,
                    ));
                }
            };

            // Add the override
            EventDiscordRoleOverride::create(&mut transaction, series, event, role_type_id, discord_role_id).await?;

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
    })
}

#[rocket::post("/event/<series>/<event>/role-types/<role_type_id>/delete-discord-override")]
pub(crate) async fn delete_discord_override(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    role_type_id: Id<RoleTypes>,
    _csrf: Option<CsrfToken>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    if !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    // Delete the override
    EventDiscordRoleOverride::delete_for_role_type(&mut transaction, series, event, role_type_id).await?;

    transaction.commit().await?;
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))))
}
