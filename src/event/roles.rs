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
        form::{EmptyForm, button_form, button_form_confirm, button_form_ext_disabled, form_field, full_form, full_form_confirm},
        http::{PageError, StatusOrError},
        id::{RoleBindings, RoleRequests, RoleTypes, Signups, EventDiscordRoleOverrides, EventDisabledRoleBindings},
        prelude::*,
        time::format_datetime,
        user::User,
        series::Series,
        game,
        cal::{Race, RaceSchedule, Entrants, Entrant},
        prelude::DiscordCtx,
        volunteer_requests,
    },
    rocket_util::Origin,
    std::collections::{HashMap, HashSet},
    serenity::model::id::RoleId,
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
    #[error(transparent)]
    VolunteerRequests(#[from] volunteer_requests::Error),
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

#[derive(Debug, Clone, Copy, sqlx::Type, PartialEq)]
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
    pub(crate) language: Language,
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
    pub(crate) language: Language,
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
    pub(crate) language: Language,
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
    pub(crate) language: Language,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct EventDiscordRoleOverride {
    pub(crate) id: Id<EventDiscordRoleOverrides>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) role_binding_id: Id<RoleBindings>,
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
    pub(crate) role_binding_id: Id<RoleBindings>,
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
                    rb.auto_approve,
                    rb.language AS "language: Language"
                FROM role_bindings rb
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.series = $1 AND rb.event = $2
                ORDER BY rt.name, rb.language
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
        language: Language,
    ) -> sqlx::Result<Id<RoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_bindings (series, event, role_type_id, min_count, max_count, discord_role_id, game_id, auto_approve, language)
               VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $8) RETURNING id"#,
            series as _,
            event,
            role_type_id as _,
            min_count,
            max_count,
            discord_role_id,
            auto_approve,
            language as _
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
        language: Language,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_bindings
                   WHERE series = $1 AND event = $2 AND role_type_id = $3 AND language = $4 AND game_id IS NULL)"#,
            series as _,
            event,
            role_type_id as _,
            language as _
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
                    rb.auto_approve,
                    rb.language AS "language: Language"
                FROM role_bindings rb
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.game_id = $1 AND rb.series IS NULL AND rb.event IS NULL
                ORDER BY rt.name, rb.language
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
        language: Language,
    ) -> sqlx::Result<Id<RoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO role_bindings (game_id, role_type_id, min_count, max_count, discord_role_id, series, event, auto_approve, language)
               VALUES ($1, $2, $3, $4, $5, NULL, NULL, $6, $7) RETURNING id"#,
            game_id,
            role_type_id as _,
            min_count,
            max_count,
            discord_role_id,
            auto_approve,
            language as _
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
        language: Language,
    ) -> sqlx::Result<bool> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM role_bindings
                   WHERE game_id = $1 AND role_type_id = $2 AND language = $3 AND series IS NULL AND event IS NULL)"#,
            game_id,
            role_type_id as _,
            language as _
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
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
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
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
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
                    rb.series AS "series?: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
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
                      rt.name as "role_type_name!",
                      rb.language as "language!: Language"
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
                      rt.name as "role_type_name!",
                      rb.language as "language!: Language"
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
                    rb.series AS "series?: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
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

    /// Get a user's approved role requests for a specific event.
    /// This is used for Discord signup buttons to check if a user can sign up for races.
    pub(crate) async fn for_user_and_event(
        pool: &mut Transaction<'_, Postgres>,
        user_id: Id<Users>,
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
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
                FROM role_requests rr
                JOIN role_bindings rb ON rr.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rr.user_id = $1
                  AND rb.series = $2
                  AND rb.event = $3
                  AND rr.status = 'approved'
                ORDER BY rt.name
            "#,
            user_id as _,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await
    }

    /// Get a user's approved role requests for a game (game-level role bindings).
    /// This is used for Discord signup buttons when events use game-level bindings.
    pub(crate) async fn for_user_and_game(
        pool: &mut Transaction<'_, Postgres>,
        user_id: Id<Users>,
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
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
                FROM role_requests rr
                JOIN role_bindings rb ON rr.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rr.user_id = $1
                  AND rb.game_id = $2
                  AND rb.series IS NULL
                  AND rb.event IS NULL
                  AND rr.status = 'approved'
                ORDER BY rt.name
            "#,
            user_id as _,
            game_id
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
    selected_lang: Option<Language>,
    msg: Option<String>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Roles, false)
        .await?;

    let content = if let Some(ref me) = me {
        if data.organizers(&mut transaction).await?.contains(me) || me.is_global_admin() {
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



            let effective_role_bindings = EffectiveRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;

            // Get active languages and determine selected language
            let active_languages = EffectiveRoleBinding::active_languages(&effective_role_bindings, data.default_volunteer_language);
            let current_language = selected_lang
                .filter(|l| active_languages.contains(l))
                .or_else(|| active_languages.iter().find(|&&l| l == data.default_volunteer_language).copied())
                .or_else(|| active_languages.first().copied())
                .unwrap_or(English);

            // Filter bindings by selected language
            let filtered_bindings: Vec<&EffectiveRoleBinding> = EffectiveRoleBinding::filter_by_language(&effective_role_bindings, current_language);
            let base_url = format!("/event/{}/{}/roles", data.series.slug(), &data.event);

            html! {
                @if let Some(ref msg) = msg {
                    div(class = "info-box") { p : msg; }
                }

                h2 : "Volunteer Request Settings";
                p : "Configure automatic volunteer request announcements for upcoming races.";

                @let mut errors = ctx.errors().collect_vec();
                : full_form(uri!(update_volunteer_request_settings(data.series, &*data.event)), csrf.as_ref(), html! {
                    : form_field("volunteer_requests_enabled", &mut errors, html! {
                        input(type = "checkbox", name = "volunteer_requests_enabled", id = "volunteer_requests_enabled", checked? = data.volunteer_requests_enabled);
                        label(for = "volunteer_requests_enabled") : " Enable automatic volunteer request posts";
                    });
                    : form_field("volunteer_request_lead_time_hours", &mut errors, html! {
                        label(for = "volunteer_request_lead_time_hours") : "Lead time (hours before race):";
                        input(type = "number", name = "volunteer_request_lead_time_hours", id = "volunteer_request_lead_time_hours", value = data.volunteer_request_lead_time_hours.to_string(), min = "1", max = "168");
                    });
                    : form_field("volunteer_request_ping_enabled", &mut errors, html! {
                        input(type = "checkbox", name = "volunteer_request_ping_enabled", id = "volunteer_request_ping_enabled", checked? = data.volunteer_request_ping_enabled);
                        label(for = "volunteer_request_ping_enabled") : " Ping roles when below minimum volunteers";
                    });
                }, errors, "Save Volunteer Request Settings");

                // Manual trigger button
                : full_form(uri!(trigger_volunteer_requests(data.series, &*data.event)), csrf.as_ref(), html! {
                    p : "Manually check for races needing volunteers and post announcements now.";
                }, Vec::new(), "Check Now");

                hr;

                h2 : "Role Management";
                p : "Manage volunteer roles for this event.";

                @if !uses_custom_bindings {
                    div(class = "info-box") {
                        h3 : "Using Game Volunteer roles";
                        p {
                            : "This event is using global volunteer roles from ";
                            @if let Some(ref game) = game_info {
                                strong : &game.display_name;
                            } else {
                                strong : "the game";
                            }
                            : " and only allows event-specific Discord roles to be attached.";
                        }
                    }
                }

                // Language tabs (only shown if multiple languages) - placed after info box
                : render_language_tabs(&active_languages, current_language, &base_url);

                // Start content box if we have tabs
                @if active_languages.len() > 1 {
                    : render_language_content_box_start();
                }

                h3 : format!("Current Volunteer Roles ({})", current_language);
                @if filtered_bindings.is_empty() {
                    p : "No volunteer roles configured for this language.";
                } else {
                    table {
                        thead {
                            tr {
                                th : "Role Type";
                                th : "Min Count";
                                th : "Max Count";
                                th : "Auto-approve";
                                th : "Discord Role";
                                th : "Actions";
                            }
                        }
                        tbody {
                            @for binding in &filtered_bindings {
                                tr(data_binding_id = binding.id.to_string()) {
                                    td(class = "role-type") : binding.role_type_name;
                                    td(class = "min-count", data_value = binding.min_count.to_string()) : binding.min_count;
                                    td(class = "max-count", data_value = binding.max_count.to_string()) : binding.max_count;
                                    td(class = "auto-approve", data_value = binding.auto_approve.to_string()) {
                                        @if binding.auto_approve {
                                            span(style = "color: green;") : "✓ Yes";
                                        } else {
                                            span(style = "color: red;") : "✗ No";
                                        }
                                    }
                                    td(class = "discord-role", data_value = binding.discord_role_id.map(|id| id.to_string()).unwrap_or_default()) {
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
                                    td(style = "text-align: center;") {
                                        @if binding.is_game_binding {
                                            p(class = "game-binding-info") {
                                                : "This role is managed by the game's role binding system";
                                                @if binding.has_event_override {
                                                    : " with event-specific Discord role override";
                                                }
                                            }
                                            @if !uses_custom_bindings {
                                                @let is_disabled = EventDisabledRoleBinding::exists_for_binding(&mut transaction, data.series, &data.event, binding.id).await?;
                                                div(style = "display: flex; justify-content: center; gap: 8px; flex-wrap: wrap;") {
                                                    @if is_disabled {
                                                        @let (errors, enable_button) = button_form(
                                                            uri!(enable_role_binding(data.series, &*data.event, binding.id)),
                                                            csrf.as_ref(),
                                                            Vec::new(),
                                                            "Enable"
                                                        );
                                                        : errors;
                                                        : enable_button;
                                                    } else {
                                                        @let (errors, disable_button) = button_form(
                                                            uri!(disable_role_binding(data.series, &*data.event, binding.id)),
                                                            csrf.as_ref(),
                                                            Vec::new(),
                                                            "Disable"
                                                        );
                                                        : errors;
                                                        : disable_button;
                                                    }
                                                    
                                                    @if binding.has_event_override {
                                                        @let (errors, remove_override_button) = button_form(
                                                            uri!(delete_discord_override(data.series, &*data.event, binding.id)),
                                                            csrf.as_ref(),
                                                            Vec::new(),
                                                            "Remove Discord Override"
                                                        );
                                                        : errors;
                                                        : remove_override_button;
                                                    }
                                                }
                                            }
                                        } else {
                                            div(class = "actions", style = "display: flex; justify-content: center; gap: 8px; flex-wrap: wrap;") {
                                                button(class = "button edit-btn config-edit-btn", onclick = format!("startEdit({})", binding.id)) : "Edit";
                                                
                                                @let (errors, delete_button) = button_form(
                                                    uri!(delete_role_binding(data.series, &*data.event, binding.id)),
                                                    csrf.as_ref(),
                                                    Vec::new(),
                                                    "Delete"
                                                );
                                                : errors;
                                                : delete_button;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Close content box if we have tabs
                @if active_languages.len() > 1 {
                    : render_language_content_box_end();
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
                        : form_field("language", &mut errors, html! {
                            label(for = "language") : "Language:";
                            select(name = "language", id = "language") {
                                option(value = "en", selected? = current_language == English) : "English";
                                option(value = "fr", selected? = current_language == French) : "French";
                                option(value = "de", selected? = current_language == German) : "German";
                                option(value = "pt", selected? = current_language == Portuguese) : "Portuguese";
                            }
                        });
                    }, errors, "Add Role Binding");
                } else {
                    h3 : "Discord Role Overrides";
                    p : "You can override Discord role IDs for specific role types while using the game's role binding structure.";
                    
                    h4 : "Add Discord Role Override";
                    @let mut errors = ctx.errors().collect_vec();
                    @let available_bindings = effective_role_bindings.iter()
                        .filter(|binding| binding.is_game_binding && !binding.has_event_override)
                        .collect::<Vec<_>>();
                    @if !available_bindings.is_empty() {
                        : full_form(uri!(add_discord_override_from_form_data(data.series, &*data.event)), csrf.as_ref(), html! {
                            : form_field("role_binding_id", &mut errors, html! {
                                label(for = "role_binding_id") : "Role Binding:";
                                select(name = "role_binding_id", id = "role_binding_id") {
                                    @for binding in available_bindings {
                                        option(value = binding.id.to_string()) : format!("{} [{}]", binding.role_type_name, binding.language.short_code().to_uppercase());
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

                    h3 : "Disabled Volunteer Roles";
                    @let disabled_bindings = EventDisabledRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;
                    @if disabled_bindings.is_empty() {
                        p : "No game volunteer roles are currently disabled for this event.";
                    } else {
                        p : "The following game volunteer roles are disabled for this event:";
                        table {
                            thead {
                                tr {
                                    th : "Role Type";
                                    th : "Language";
                                    th : "Actions";
                                }
                            }
                            tbody {
                                @for disabled_binding in &disabled_bindings {
                                    @let binding_info = sqlx::query!(
                                        r#"
                                            SELECT rt.name AS role_type_name, rb.language AS "language: Language"
                                            FROM role_bindings rb
                                            JOIN role_types rt ON rb.role_type_id = rt.id
                                            WHERE rb.id = $1
                                        "#,
                                        disabled_binding.role_binding_id as _
                                    ).fetch_optional(&mut *transaction).await?;
                                    @if let Some(binding_info) = binding_info {
                                        tr {
                                            td : binding_info.role_type_name;
                                            td : binding_info.language;
                                            td {
                                                @let (errors, enable_button) = button_form(
                                                    uri!(enable_role_binding(data.series, &*data.event, disabled_binding.role_binding_id)),
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

                    @if uses_custom_bindings {
                        // Show copy volunteers form
                        details {
                            summary : "Copy Volunteers from Another Event";
                            p : "Import all approved volunteers from another event in this series. Only volunteers for matching role types will be copied, and users who already have approved roles in this event will be skipped.";

                            @let all_events_in_series = sqlx::query!(
                                r#"
                                SELECT event FROM events
                                WHERE series = $1 AND event != $2
                                ORDER BY start DESC
                                "#,
                                data.series as _,
                                &data.event
                            )
                            .fetch_all(&mut *transaction)
                            .await?;

                            @if !all_events_in_series.is_empty() {
                                @let mut errors = ctx.errors().collect_vec();
                                : full_form(uri!(copy_volunteers_from_event(data.series, &*data.event)), csrf.as_ref(), html! {
                                    : form_field("source_event", &mut errors, html! {
                                        label(for = "source_event") : "Source Event:";
                                        select(name = "source_event", id = "source_event", required) {
                                            option(value = "", disabled, selected) : "-- Select an event --";
                                            @for evt in all_events_in_series {
                                                option(value = evt.event) : evt.event;
                                            }
                                        }
                                    });
                                }, errors, "Copy All Approved Volunteers");
                            } else {
                                p : "No other events found in this series to copy from.";
                            }
                        }
                    }

                    @let approved_by_role_type = approved_requests.iter()
                        .filter(|request| {
                            // Filter by current language and check if disabled
                            request.language == current_language
                        })
                        .fold(HashMap::<(String, Language), Vec<_>>::new(), |mut acc, request| {
                            acc.entry((request.role_type_name.clone(), request.language)).or_insert_with(Vec::new).push(request);
                            acc
                        });
                    @for ((role_type_name, language), requests) in approved_by_role_type {
                        @let is_disabled = EventDisabledRoleBinding::exists_for_binding(&mut transaction, data.series, &data.event, requests[0].role_binding_id).await?;
                        @if !is_disabled {
                            details {
                                summary : format!("{} [{}] ({})", role_type_name, language.short_code().to_uppercase(), requests.len());
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
                                                                                                        @let (errors, revoke_button) = button_form_confirm(
                                                    uri!(revoke_role_request(data.series, &*data.event, request.id)),
                                                    csrf.as_ref(),
                                                    Vec::new(),
                                                    "Revoke",
                                                    "Are you sure you want to revoke this approved role?"
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
            link(rel = "stylesheet", href = static_url!("roles-page.css"));
            script(src = static_url!("role-binding-edit.js")) {}
        },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/roles?<lang>&<msg>")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    lang: Option<Language>,
    msg: Option<String>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    Ok(roles_page(transaction, me, &uri, data, ctx, csrf, lang, msg).await?)
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
    language: Language,
}

/// Form for editing an existing role binding.
/// Note: language is intentionally not editable after creation - to change the language,
/// delete the binding and create a new one. This prevents volunteers from being silently
/// moved between language tabs.
#[derive(FromForm, CsrfForm)]
pub(crate) struct EditRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
    min_count: i32,
    max_count: i32,
    #[field(default = String::new())]
    discord_role_id: String,
    auto_approve: bool,
}

#[rocket::post("/event/<series>/<event>/roles/add-binding", data = "<form>")]
pub(crate) async fn add_role_binding(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
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

        if RoleBinding::exists_for_role_type(&mut transaction, data.series, &data.event, value.role_type_id, value.language).await? {
            form.context.push_error(form::Error::validation(
                "A role binding for this role type and language already exists.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
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
                                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                                data,
                                form.context,
                                csrf,
                                None,
                                None,
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
                value.language,
            )
            .await?;
            transaction.commit().await?;

            // Update volunteer info messages to show the new role
            let _ = volunteer_requests::update_volunteer_posts_for_event(
                pool,
                &*discord_ctx.read().await,
                series,
                event,
            ).await;

            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/<binding>/delete", data = "<form>")]
pub(crate) async fn delete_role_binding(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
                )
                .await?,
            )
        } else {
            RoleBinding::delete(&mut transaction, binding).await?;
            transaction.commit().await?;

            // Update volunteer info messages to remove the deleted role
            let _ = volunteer_requests::update_volunteer_posts_for_event(
                pool,
                &*discord_ctx.read().await,
                series,
                event,
            ).await;

            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
            )
            .await?,
        )
    })
}



#[rocket::post("/event/<series>/<event>/roles/binding/<binding>/edit", data = "<form>")]
pub(crate) async fn edit_role_binding(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    binding: Id<RoleBindings>,
    _csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, EditRoleBindingForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    if !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let value = form.value.as_ref().ok_or(StatusOrError::Status(Status::BadRequest))?;

    // Validate form data
    if value.min_count < 1 {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    if value.max_count < value.min_count {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    // Parse Discord role ID
    let discord_role_id = if value.discord_role_id.trim().is_empty() {
        None
    } else {
        Some(value.discord_role_id.parse::<i64>().map_err(|_| StatusOrError::Status(Status::BadRequest))?)
    };

    // Update the role binding
    sqlx::query!(
        r#"UPDATE role_bindings 
           SET min_count = $1, max_count = $2, discord_role_id = $3, auto_approve = $4
           WHERE id = $5 AND series = $6 AND event = $7"#,
        value.min_count,
        value.max_count,
        discord_role_id,
        value.auto_approve,
        binding as _,
        series as _,
        event
    )
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _)))))
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
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
                r#"SELECT rb.id as "id: Id<RoleBindings>", rb.series as "series: Series", rb.event, rb.game_id, rb.role_type_id as "role_type_id: Id<RoleTypes>", rb.min_count, rb.max_count, rt.name as role_type_name, rb.discord_role_id, rb.auto_approve, rb.language AS "language: Language" FROM role_bindings rb JOIN role_types rt ON rb.role_type_id = rt.id WHERE rb.id = $1"#,
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
            let redirect_url = format!("/event/{}/{}/roles?msg={}", series.slug(), event, urlencoding::encode("Role request approved successfully."));
            RedirectOrContent::Redirect(Redirect::to(redirect_url))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
                )
                .await?,
            )
        } else {
            RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Rejected)
                .await?;
            transaction.commit().await?;
            let redirect_url = format!("/event/{}/{}/roles?msg={}", series.slug(), event, urlencoding::encode("Role request rejected."));
            RedirectOrContent::Redirect(Redirect::to(redirect_url))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
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

#[derive(FromForm, CsrfForm)]
pub(crate) struct ForfeitRoleForm {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/volunteer-roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                )
                .await?,
            )
        } else {
            let notes = if value.notes.is_empty() {
                None
            } else {
                Some(value.notes.clone())
            };

            // Get the role binding details to check auto-approve status
            let role_binding = sqlx::query_as!(
                RoleBinding,
                r#"SELECT rb.id as "id: Id<RoleBindings>", rb.series as "series: Series", rb.event as "event!", rb.game_id,
                          rb.role_type_id as "role_type_id: Id<RoleTypes>", rb.min_count as "min_count!",
                          rb.max_count as "max_count!", rt.name as "role_type_name!", rb.discord_role_id, rb.auto_approve,
                          rb.language AS "language: Language"
                       FROM role_bindings rb
                       JOIN role_types rt ON rb.role_type_id = rt.id
                       WHERE rb.id = $1"#,
                i64::from(value.role_binding_id) as i32
            )
            .fetch_one(&mut *transaction)
            .await?;

            // Check if user already has an active request for this role binding
            if RoleRequest::active_for_user(&mut transaction, value.role_binding_id, me.id).await? {
                form.context.push_error(form::Error::validation(
                    "You have already applied for this role",
                ));
                return Ok(RedirectOrContent::Content(
                    volunteer_page(
                        transaction,
                        Some(me),
                        &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/volunteer-roles", series.slug(), event)).unwrap()),
                        data,
                        form.context,
                        csrf,
                        None,
                    )
                    .await?,
                ));
            }

            // Create the role request
            RoleRequest::create(
                &mut transaction,
                value.role_binding_id,
                me.id,
                notes.clone().unwrap_or_default(),
            )
            .await?;

            // Only send Discord notification for non-auto-approve roles
            if !role_binding.auto_approve {
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
            }

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(volunteer_page_get(series, event, _))))
        }
    } else {
        RedirectOrContent::Content(
            volunteer_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/volunteer-roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/volunteer-roles/forfeit", data = "<form>")]
pub(crate) async fn forfeit_role(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ForfeitRoleForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        // Find the role request for this user and role binding
        let role_request = sqlx::query_as!(
            RoleRequest,
            r#"
                SELECT
                    rr.id AS "id: Id<RoleRequests>",
                    rr.role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    rr.user_id AS "user_id: Id<Users>",
                    rr.status AS "status: RoleRequestStatus",
                    rr.notes,
                    rr.created_at,
                    rr.updated_at,
                    rb.series AS "series?: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
                FROM role_requests rr
                JOIN role_bindings rb ON rr.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rr.role_binding_id = $1 AND rr.user_id = $2 AND rr.status IN ('pending', 'approved')
                ORDER BY rr.created_at DESC
                LIMIT 1
            "#,
            value.role_binding_id as _,
            me.id as _
        )
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(request) = role_request {
            // Update the status to aborted
            RoleRequest::update_status(&mut transaction, request.id, RoleRequestStatus::Aborted).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(volunteer_page_get(series, event, _))))
        } else {
            form.context.push_error(form::Error::validation(
                "No active role request found to forfeit",
            ));
            RedirectOrContent::Content(
                volunteer_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/volunteer-roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                )
                .await?,
            )
        }
    } else {
        RedirectOrContent::Content(
            volunteer_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/volunteer-roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
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
    ctx: Context<'_>,
    csrf: Option<CsrfToken>,
    selected_lang: Option<Language>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Volunteer, false)
        .await?;

    let content = if let Some(ref me) = me {
        // Check if event uses custom role bindings
        let uses_custom_bindings = sqlx::query_scalar!(
            r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
            data.series as _,
            &data.event
        )
        .fetch_optional(&mut *transaction)
        .await?
        .unwrap_or(Some(true)).unwrap_or(true);

        // Get the game for this series (needed for game role binding links)
        let game = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)?;

        {
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
                // For game bindings, show approved game-level roles (not event-specific)
                my_requests
                    .iter()
                    .filter(|req| {
                        matches!(req.status, RoleRequestStatus::Approved)
                            && req.series.is_none()
                            && req.event.is_none()
                    })
                    .collect::<Vec<_>>()
            };

            let upcoming_races = Race::for_event(&mut transaction, &reqwest::Client::new(), &data).await?;

            // Get active languages and determine selected language
            let active_languages = EffectiveRoleBinding::active_languages(&effective_role_bindings, data.default_volunteer_language);
            let current_language = selected_lang
                .filter(|l| active_languages.contains(l))
                .or_else(|| active_languages.iter().find(|&&l| l == data.default_volunteer_language).copied())
                .or_else(|| active_languages.first().copied())
                .unwrap_or(English);

            // Filter bindings by selected language
            let filtered_bindings: Vec<&EffectiveRoleBinding> = EffectiveRoleBinding::filter_by_language(&effective_role_bindings, current_language);
            let base_url = format!("/event/{}/{}/volunteer-roles", data.series.slug(), &data.event);

            html! {
                h2 : "Volunteer for Roles";

                @if !uses_custom_bindings {
                    div(class = "game-binding-notice") {
                        h3 : "Game Role Bindings";
                        p : "This event uses game-level role bindings. To volunteer for roles, you need to apply for game roles instead of event-specific roles.";
                        @if let Some(ref game) = game {
                            p {
                                : "Please visit the ";
                                a(href = uri!(crate::games::get(&game.name, _))) : "game volunteer page";
                                : " to apply for roles that will be available across all events for this game.";
                            }
                        }
                    }
                } else {
                    p : "Apply to volunteer for roles in this event.";
                }

                // Language tabs (only shown if multiple languages)
                : render_language_tabs(&active_languages, current_language, &base_url);

                // Start content box if we have tabs
                @if active_languages.len() > 1 {
                    : render_language_content_box_start();
                }

                @if filtered_bindings.is_empty() {
                    p : "No volunteer roles are currently available for this language.";
                } else {
                    h3 : "Available Roles";
                    @for binding in &filtered_bindings {
                        @let my_request = my_requests.iter()
                            .filter(|req| req.role_binding_id == binding.id && !matches!(req.status, RoleRequestStatus::Aborted))
                            .max_by_key(|req| req.created_at);
                        @let has_active_request = my_request.map_or(false, |req| matches!(req.status, RoleRequestStatus::Pending | RoleRequestStatus::Approved));
                        div(class = "role-binding") {
                            h4 {
                                : binding.role_type_name;
                                : " (";
                                : binding.language;
                                : ")";
                            }
                            p {
                                @if binding.min_count == binding.max_count {
                                    : "Required: ";
                                    : binding.min_count;
                                    @if binding.min_count == 1 {
                                        : " volunteer";
                                    } else {
                                        : " volunteers";
                                    }
                                } else {
                                    : "Required: ";
                                    : binding.min_count;
                                    : " - ";
                                    : binding.max_count;
                                    : " volunteers";
                                }
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
                                    : "Managed globally by the game";
                                    @if binding.has_event_override {
                                        : " with an event-specific Discord role override";
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
                                @if request.status == RoleRequestStatus::Pending || request.status == RoleRequestStatus::Rejected {
                                    @if let Some(ref notes) = request.notes {
                                        @if !notes.is_empty() {
                                            p(class = "request-notes") {
                                                : "Your application note: ";
                                                : notes;
                                            }
                                        }
                                    }
                                }
                                @if binding.is_game_binding {
                                    @if let Some(ref game) = game {
                                        p(class = "game-role-link") {
                                            : "To forfeit this game-level role, visit the ";
                                            a(href = uri!(crate::games::get(&game.name, _))) : "game volunteer page";
                                            : ".";
                                        }
                                    }
                                } else {
                                    @let errors = ctx.errors().collect::<Vec<_>>();
                                    : full_form_confirm(uri!(forfeit_role(data.series, &*data.event)), csrf.as_ref(), html! {
                                        input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                    }, errors, "Forfeit Role", "Are you sure you want to forfeit this role?");
                                }
                            } else {
                                @if binding.is_game_binding {
                                    @if let Some(ref game) = game {
                                        p(class = "game-role-link") {
                                            : "To apply for this game-level role, visit the ";
                                            a(href = uri!(crate::games::get(&game.name, _))) : "game volunteer page";
                                            : ".";
                                        }
                                    }
                                } else {
                                    @let mut errors = ctx.errors().collect::<Vec<_>>();
                                    @let button_text = if binding.auto_approve {
                                        format!("Volunteer for {} role", binding.role_type_name)
                                    } else {
                                        format!("Apply for {} role", binding.role_type_name)
                                    };
                                    : full_form(uri!(apply_for_role(data.series, &*data.event)), csrf.as_ref(), html! {
                                        input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                        @if !binding.auto_approve {
                                            : form_field("notes", &mut errors, html! {
                                                label(for = "notes") : "Notes (optional):";
                                                textarea(name = "notes", id = "notes", rows = "3") : "";
                                            });
                                        }
                                    }, errors, button_text.as_str());
                                }
                            }
                        }
                    }
                }

                // Close content box if we have tabs
                @if active_languages.len() > 1 {
                    : render_language_content_box_end();
                }

                @if !my_approved_roles.is_empty() && !upcoming_races.is_empty() {
                    h3 : "Sign Up for Matches";
                    p : "You have been approved for the following roles. You can now sign up for specific matches:";

                    @for role_request in my_approved_roles {
                        @let binding = effective_role_bindings.iter().find(|b| b.id == role_request.role_binding_id);
                        @if let Some(binding) = binding {
                            h4 {
                                : binding.role_type_name;
                                : " (";
                                : binding.language;
                                : ")";
                            }
                            @let now = chrono::Utc::now();
                            @let available_races = upcoming_races.iter().filter(|race| {
                                // Filter out races that have already started
                                match race.schedule {
                                    RaceSchedule::Live { start, .. } => start > now,
                                    _ => false,
                                }
                            }).collect::<Vec<_>>();

                            @if available_races.is_empty() {
                                p : "No upcoming races available for signup.";
                            } else {
                                ul {
                                    @for race in available_races {
                                        li {
                                            a(href = uri!(match_signup_page_get(data.series, &*data.event, race.id, _))) : {
                                                // For qualifier races, show the round name (e.g., "Live 1") with "(Qualifier)" indicator
                                                if race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                                                    format!("{} (Qualifier)", race.round.clone().unwrap_or_else(|| "Qualifier".to_string()))
                                                } else {
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
                                                        _ => "TBD vs TBD".to_string(),
                                                    }
                                                }
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


#[rocket::get("/event/<series>/<event>/volunteer-roles?<lang>")]
pub(crate) async fn volunteer_page_get(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    lang: Option<Language>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    let uri = HttpOrigin::parse_owned(format!("/event/{}/{}/volunteer-roles", series.slug(), event)).unwrap();
    Ok(volunteer_page(transaction, me, &Origin(uri.clone()), data, ctx, csrf, lang).await?)
}

// Match signup functionality
#[derive(FromForm, CsrfForm)]
pub(crate) struct SignupForMatchForm {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
    #[field(default = String::new())]
    notes: String,
    lang: Option<Language>,
}

#[rocket::post("/event/<series>/<event>/races/<race_id>/signup", data = "<form>")]
pub(crate) async fn signup_for_match(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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
                    value.lang,
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

            // Update the volunteer info post to reflect the new signup
            let _ = volunteer_requests::update_volunteer_post_for_race(
                pool,
                &*discord_ctx.read().await,
                race_id,
            ).await;

            RedirectOrContent::Redirect(match value.lang {
                Some(lang) => Redirect::to(format!("/event/{}/{}/races/{}/signups?lang={}", series.slug(), event, race_id, lang.short_code())),
                None => Redirect::to(format!("/event/{}/{}/races/{}/signups", series.slug(), event, race_id)),
            })
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
                None,
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
    lang: Option<Language>,
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
        let mut is_restreamer = data.restreamers(&mut transaction).await?.contains(&me);
        if !is_restreamer {
            if let Some(game) = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)? {
                is_restreamer = game.is_restreamer_any_language(&mut transaction, &me).await.map_err(Error::from)?;
            }
        }

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
                    value.lang,
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
                            value.lang,
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
                
                // Send DM notification to the selected volunteer
                {
                    // Get race details for the notification
                    let race = Race::from_id(&mut transaction, &reqwest::Client::new(), race_id).await?;
                    let user = User::from_id(&mut *transaction, signup.user_id).await?;

                    // Format race description with round info
                    // For qualifier races, use the round name directly
                    let race_description = if race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                        race.round.clone().unwrap_or_else(|| "Qualifier".to_string())
                    } else {
                        match &race.entrants {
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
                            _ => race.round.clone().unwrap_or_else(|| "Race".to_string()),
                        }
                    };

                    // Get race start time for timestamp
                    let race_start_time = match race.schedule {
                        cal::RaceSchedule::Live { start, .. } => start,
                        _ => return Err(StatusOrError::Status(Status::BadRequest)), // Volunteers can't sign up for unscheduled races
                    };

                    if let Some(ref user) = user {
                        if let Some(ref discord) = user.discord {
                            let discord_ctx = discord_ctx.read().await;
                            let discord_user_id = UserId::new(discord.id.get());

                            // Build DM message
                            let mut msg = MessageBuilder::default();
                            msg.push("You have been selected to volunteer for ");
                            msg.push_mono(&race_description);
                            msg.push(" in ");
                            msg.push(&data.display_name);
                            msg.push("!\n\n");
                            msg.push("**Role:** ");
                            msg.push_mono(&signup.role_type_name);
                            msg.push("\n**When:** ");
                            msg.push_timestamp(race_start_time, serenity_utils::message::TimestampStyle::LongDateTime);

                            // Add restream information if the race has restream URLs for the volunteer's language
                            if !race.video_urls.is_empty() {
                                // Check if there's a restream for the volunteer's language
                                let binding = EffectiveRoleBinding::for_event(&mut transaction, data.series, &data.event).await?
                                    .into_iter()
                                    .find(|b| b.id == signup.role_binding_id);

                                if let Some(binding) = binding {
                                    if let Some(video_url) = race.video_urls.get(&binding.language) {
                                        msg.push("\n**Restream (");
                                        msg.push(&binding.language.to_string());
                                        msg.push("):** <");
                                        msg.push(&video_url.to_string());
                                        msg.push(">");
                                    }
                                }
                            } else {
                                msg.push("\n\nNo restream channel has been assigned yet. A restreamer will reach out before the race to inform you of the next steps.");
                            }

                            // Send DM
                            if let Ok(dm_channel) = discord_user_id.create_dm_channel(&*discord_ctx).await {
                                if let Err(e) = dm_channel.say(&*discord_ctx, msg.build()).await {
                                    eprintln!("Failed to send volunteer selection DM: {}", e);
                                }
                            }
                        }
                    }
                }
            }

            transaction.commit().await?;

            // Update the volunteer info post to reflect the status change
            let _ = volunteer_requests::update_volunteer_post_for_race(
                pool,
                &*discord_ctx.read().await,
                race_id,
            ).await;

            RedirectOrContent::Redirect(match value.lang {
                Some(lang) => Redirect::to(format!("/event/{}/{}/races/{}/signups?lang={}", series.slug(), event, race_id, lang.short_code())),
                None => Redirect::to(format!("/event/{}/{}/races/{}/signups", series.slug(), event, race_id)),
            })
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
                None,
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
    selected_lang: Option<Language>,
) -> Result<RawHtml<String>, Error> {
    let header = data
        .header(&mut transaction, me.as_ref(), Tab::Races, true)
        .await?;

    // Get race details
    let race = Race::from_id(&mut transaction, &reqwest::Client::new(), race_id).await?;
    let signups = Signup::for_race(&mut transaction, race_id).await?;
    let effective_role_bindings = EffectiveRoleBinding::for_event(&mut transaction, data.series, &data.event).await?;

    // Get active languages and determine selected language
    let active_languages = EffectiveRoleBinding::active_languages(&effective_role_bindings, data.default_volunteer_language);
    let current_language = selected_lang
        .filter(|l| active_languages.contains(l))
        .or_else(|| active_languages.iter().find(|&&l| l == data.default_volunteer_language).copied())
        .or_else(|| active_languages.first().copied())
        .unwrap_or(English);

    // Filter bindings by selected language
    let filtered_bindings: Vec<&EffectiveRoleBinding> = EffectiveRoleBinding::filter_by_language(&effective_role_bindings, current_language);
    let base_url = format!("/event/{}/{}/races/{}/signups", data.series.slug(), &data.event, race_id);

    // Get user's role requests if logged in
    let my_requests = if let Some(ref me) = me {
        Some(RoleRequest::for_user(&mut transaction, me.id).await?)
    } else {
        None
    };

    let content = if let Some(ref me) = me {
        let is_organizer = data.organizers(&mut transaction).await?.contains(me);
        let mut is_restreamer = data.restreamers(&mut transaction).await?.contains(me);
        if !is_restreamer {
            if let Some(game) = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)? {
                is_restreamer = game.is_restreamer_any_language(&mut transaction, me).await.map_err(Error::from)?;
            }
        }
        let can_manage = is_organizer || is_restreamer;

        html! {
            h2 : "Match Volunteer Signups";

            // Display form validation errors if any
            @for error in _ctx.errors() {
                div(class = "error") {
                    p : error;
                }
            }

            h3 {
                // For qualifier races without fixed entrants, show only the round name or "Qualifier"
                // For other races, show phase/round prefix and team matchup
                @if matches!(race.entrants, Entrants::Two(_) | Entrants::Three(_)) {
                    // Show phase and round prefix for team-based races
                    @if let Some(ref phase) = race.phase {
                        : phase;
                        : " ";
                    }
                    @if let Some(ref round) = race.round {
                        : round;
                        : " ";
                    }
                }
                : match &race.entrants {
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
                    _ => {
                        // For qualifier races without fixed entrants, show round or "Qualifier"
                        race.round.clone().unwrap_or_else(|| "Qualifier".to_string())
                    }
                };
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

            // Language tabs (only shown if multiple languages)
            : render_language_tabs(&active_languages, current_language, &base_url);

            // Start content box if we have tabs
            @if active_languages.len() > 1 {
                : render_language_content_box_start();
            }

            @if can_manage {
                h3 : "Manage Signups";
                @let inactive_signups = signups.iter()
                    .filter(|s| matches!(s.status, VolunteerSignupStatus::Declined | VolunteerSignupStatus::Aborted))
                    .collect::<Vec<_>>();
                @let active_signups = signups.iter()
                    .filter(|s| !matches!(s.status, VolunteerSignupStatus::Declined | VolunteerSignupStatus::Aborted))
                    .collect::<Vec<_>>();

                @for signup in &active_signups {
                    @if let Some(user) = User::from_id(&mut *transaction, signup.user_id).await? {
                        div(class = "signup-item") {
                            div(class = "signup-item-content") {
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
                                            @if active_languages.len() > 1 {
                                                input(type = "hidden", name = "lang", value = current_language.short_code());
                                            }
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
                                            @if active_languages.len() > 1 {
                                                input(type = "hidden", name = "lang", value = current_language.short_code());
                                            }
                                        },
                                        "Decline"
                                    );
                                    : errors;
                                    : decline_button;
                                }
                            } else if matches!(signup.status, VolunteerSignupStatus::Confirmed) {
                                div(class = "signup-actions") {
                                    @let (errors, revert_button) = button_form_ext(
                                        uri!(revoke_signup(data.series, &*data.event, race_id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        html! {
                                            input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                            @if active_languages.len() > 1 {
                                                input(type = "hidden", name = "lang", value = current_language.short_code());
                                            }
                                        },
                                        "Revert to Pending"
                                    );
                                    : errors;
                                    : revert_button;
                                }
                            }
                        }
                    }
                }

                @if !inactive_signups.is_empty() {
                    details {
                        summary : format!("Declined/Withdrawn Signups ({})", inactive_signups.len());
                        @for signup in &inactive_signups {
                            @if let Some(user) = User::from_id(&mut *transaction, signup.user_id).await? {
                                div(class = "signup-item") {
                                    div(class = "signup-item-content") {
                                        p {
                                            strong : user.display_name();
                                            : " - ";
                                            : signup.role_type_name;
                                            : " (";
                                            @match signup.status {
                                                VolunteerSignupStatus::Declined => : "Declined";
                                                VolunteerSignupStatus::Aborted => : "Withdrawn";
                                                _ => : "Inactive";
                                            }
                                            : ")";
                                        }
                                        @if let Some(ref notes) = signup.notes {
                                            p(class = "signup-notes") : notes;
                                        }
                                    }
                                    div(class = "signup-actions") {
                                        @let (errors, revert_button) = button_form_ext(
                                            uri!(revoke_signup(data.series, &*data.event, race_id)),
                                            csrf.as_ref(),
                                            Vec::new(),
                                            html! {
                                                input(type = "hidden", name = "signup_id", value = signup.id.to_string());
                                                @if active_languages.len() > 1 {
                                                    input(type = "hidden", name = "lang", value = current_language.short_code());
                                                }
                                            },
                                            "Revert to Pending"
                                        );
                                        : errors;
                                        : revert_button;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            h3 {
                : "Role Signups";
                : " (";
                : current_language.to_string();
                : ")";
            }
            @for binding in &filtered_bindings {
                div(class = "role-binding") {
                    h4 {
                        : binding.role_type_name;
                        : " (";
                        : binding.language.to_string();
                        : ")";
                    }
                    p {
                        @if binding.min_count == binding.max_count {
                            : "Required: ";
                            : binding.min_count;
                            @if binding.min_count == 1 {
                                : " volunteer";
                            } else {
                                : " volunteers";
                            }
                        } else {
                            : "Required: ";
                            : binding.min_count;
                            : " - ";
                            : binding.max_count;
                            : " volunteers";
                        }
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

                    @let is_approved = RoleRequest::approved_for_user(&mut transaction, binding.id, me.id).await?;
                    @let my_signup = role_signups.iter().find(|s| s.user_id == me.id && matches!(s.status, VolunteerSignupStatus::Pending | VolunteerSignupStatus::Confirmed));
                    @let has_pending_request = my_requests.as_ref().map(|reqs| {
                        reqs.iter().any(|req| req.role_binding_id == binding.id && matches!(req.status, RoleRequestStatus::Pending))
                    }).unwrap_or(false);
                    @if let Some(my_signup) = my_signup {
                        p : "You have already signed up for this role.";

                        // Allow withdrawal if:
                        // - Status is Pending (always), OR
                        // - Status is Confirmed AND race hasn't started yet
                        @let can_withdraw = match my_signup.status {
                            VolunteerSignupStatus::Pending => true,
                            VolunteerSignupStatus::Confirmed => {
                                match race.schedule {
                                    RaceSchedule::Live { start, .. } => start > chrono::Utc::now(),
                                    RaceSchedule::Async { start1, start2, start3, .. } => {
                                        [start1, start2, start3].iter()
                                            .filter_map(|s| *s)
                                            .all(|s| s > chrono::Utc::now())
                                    }
                                    _ => false,
                                }
                            }
                            _ => false,
                        };

                        @if can_withdraw {
                            : full_form_confirm(
                                uri!(withdraw_signup(data.series, &*data.event, race.id)),
                                csrf.as_ref(),
                                html! {
                                    input(type = "hidden", name = "signup_id", value = my_signup.id.to_string());
                                    @if active_languages.len() > 1 {
                                        input(type = "hidden", name = "lang", value = current_language.short_code());
                                    }
                                },
                                Vec::new(),
                                "Withdraw Signup",
                                "Are you sure you want to withdraw from this race?"
                            );
                        }
                    } else if is_approved {
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
                                @if active_languages.len() > 1 {
                                    input(type = "hidden", name = "lang", value = current_language.short_code());
                                }
                                : form_field("notes", &mut errors, html! {
                                    label(for = "notes") : "Notes:";
                                    input(type = "text", name = "notes", id = "notes", maxlength = "60", size = "30", placeholder = "Optional notes for organizers");
                                });
                            }, errors, &format!("Sign up for {}", binding.role_type_name));
                        }
                        @if let Some(reason) = reason {
                            p(class = "disabled-reason") : reason;
                        }
                    } else if has_pending_request {
                        p : "You requested this role — please wait for approval.";
                    } else {
                        p : "You need to be approved for this role before you can sign up.";
                        @if binding.has_event_override {
                            p {
                                a(href = uri!(volunteer_page_get(data.series, &*data.event, _))) : "Request this role";
                            }
                        } else {
                            @if let Some(ref game) = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)? {
                                p {
                                    : "Please request this role on the ";
                                    a(href = uri!(crate::games::get(&*game.name, _))) : "game page";
                                    : ".";
                                }
                            } else {
                                p : "Unable to request this role (game not found).";
                            }
                        }
                    }

                    @if !role_signups.is_empty() {
                        p(class = "signup-count") {
                            : format!("{}/{} confirmed volunteers", confirmed_signups.len(), binding.max_count);
                        }
                    }
                }
            }

            // Close content box if we have tabs
            @if active_languages.len() > 1 {
                : render_language_content_box_end();
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

#[rocket::get("/event/<series>/<event>/races/<race_id>/signups?<lang>")]
pub(crate) async fn match_signup_page_get(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    lang: Option<Language>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let ctx = Context::default();
    let uri = HttpOrigin::parse_owned(format!("/event/{}/{}/races/{}", series.slug(), event, race_id)).unwrap();
    Ok(match_signup_page(transaction, me, &Origin(uri.clone()), data, race_id, ctx, csrf, lang).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct WithdrawSignupForm {
    #[field(default = String::new())]
    csrf: String,
    signup_id: Id<Signups>,
    lang: Option<Language>,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RevokeSignupForm {
    #[field(default = String::new())]
    csrf: String,
    signup_id: Id<Signups>,
    lang: Option<Language>,
}

#[rocket::post("/event/<series>/<event>/races/<race_id>/withdraw-signup", data = "<form>")]
pub(crate) async fn withdraw_signup(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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

        // Check if signup can be withdrawn
        let can_withdraw = match signup.status {
            VolunteerSignupStatus::Pending => true,
            VolunteerSignupStatus::Confirmed => {
                // Allow confirmed withdrawals only if race hasn't started
                let race = Race::from_id(&mut transaction, &reqwest::Client::new(), race_id).await?;
                match race.schedule {
                    RaceSchedule::Live { start, .. } => start > Utc::now(),
                    RaceSchedule::Async { start1, start2, start3, .. } => {
                        [start1, start2, start3].iter()
                            .filter_map(|s| *s)
                            .all(|s| s > Utc::now())
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        if !can_withdraw {
            if matches!(signup.status, VolunteerSignupStatus::Confirmed) {
                form.context.push_error(form::Error::validation(
                    "You cannot withdraw after the race has started. Please contact organizers if you need help."
                ));
            } else {
                form.context.push_error(form::Error::validation(
                    "This signup cannot be withdrawn."
                ));
            }
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
                    value.lang,
                )
                .await?,
            )
        } else {
            // Update the signup status to Aborted
            Signup::update_status(&mut transaction, value.signup_id, VolunteerSignupStatus::Aborted).await?;
            transaction.commit().await?;

            // Update the volunteer info post to reflect the withdrawal
            let _ = volunteer_requests::update_volunteer_post_for_race(
                pool,
                &*discord_ctx.read().await,
                race_id,
            ).await;

            RedirectOrContent::Redirect(match value.lang {
                Some(lang) => Redirect::to(format!("/event/{}/{}/races/{}/signups?lang={}", series.slug(), event, race_id, lang.short_code())),
                None => Redirect::to(format!("/event/{}/{}/races/{}/signups", series.slug(), event, race_id)),
            })
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
                None,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/races/<race_id>/revoke-signup", data = "<form>")]
pub(crate) async fn revoke_signup(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
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
        let mut is_restreamer = data.restreamers(&mut transaction).await?.contains(&me);
        if !is_restreamer {
            if let Some(game) = game::Game::from_series(&mut transaction, data.series).await.map_err(Error::from)? {
                is_restreamer = game.is_restreamer_any_language(&mut transaction, &me).await.map_err(Error::from)?;
            }
        }

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

        // Only allow reverting confirmed or declined signups
        if !matches!(signup.status, VolunteerSignupStatus::Confirmed | VolunteerSignupStatus::Declined) {
            form.context.push_error(form::Error::validation(
                "You can only revert confirmed or declined signups",
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
                    value.lang,
                )
                .await?,
            )
        } else {
            // Update the signup status back to Pending
            Signup::update_status(&mut transaction, value.signup_id, VolunteerSignupStatus::Pending).await?;
            transaction.commit().await?;

            // Update the volunteer info post to reflect the revocation
            let _ = volunteer_requests::update_volunteer_post_for_race(
                pool,
                &*discord_ctx.read().await,
                race_id,
            ).await;

            RedirectOrContent::Redirect(match value.lang {
                Some(lang) => Redirect::to(format!("/event/{}/{}/races/{}/signups?lang={}", series.slug(), event, race_id, lang.short_code())),
                None => Redirect::to(format!("/event/{}/{}/races/{}/signups", series.slug(), event, race_id)),
            })
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
                None,
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
                )
                .await?,
            )
        } else {
            // Update the role request status to Aborted
            RoleRequest::update_status(&mut transaction, value.request_id, RoleRequestStatus::Aborted).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/revoke-role-request/<request>", data = "<form>")]
pub(crate) async fn revoke_role_request(
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
    let role_request = RoleRequest::from_id(&mut transaction, request).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    if role_request.series != Some(series) || role_request.event != Some(event.to_string()) {
        form.context.push_error(form::Error::validation(
            "Invalid role request for this event",
        ));
    }

    // Prevent revoking game role bindings
    if role_request.series.is_none() && role_request.event.is_none() {
        form.context.push_error(form::Error::validation(
            "Cannot revoke globally managed role assignments",
        ));
    }

    // Only allow revoking approved role requests
    if !matches!(role_request.status, RoleRequestStatus::Approved) {
        form.context.push_error(form::Error::validation(
            "You can only revoke approved role requests",
        ));
    }

    Ok(if form.context.errors().next().is_some() {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
            )
            .await?,
        )
    } else {
        // Update the role request status back to Pending
        RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Pending).await?;
        transaction.commit().await?;
        let redirect_url = format!("/event/{}/{}/roles?msg={}", series.slug(), event, urlencoding::encode("Role assignment revoked."));
        RedirectOrContent::Redirect(Redirect::to(redirect_url))
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
                    role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    discord_role_id,
                    created_at AS "created_at!",
                    updated_at AS "updated_at!"
                FROM event_discord_role_overrides
                WHERE series = $1 AND event = $2
                ORDER BY role_binding_id
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
        role_binding_id: Id<RoleBindings>,
        discord_role_id: i64,
    ) -> sqlx::Result<Id<EventDiscordRoleOverrides>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO event_discord_role_overrides (series, event, role_binding_id, discord_role_id)
               VALUES ($1, $2, $3, $4) RETURNING id"#,
            series as _,
            event,
            role_binding_id as _,
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

    pub(crate) async fn exists_for_role_binding(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_binding_id: Id<RoleBindings>,
    ) -> sqlx::Result<bool> {
        let result: Option<bool> = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM event_discord_role_overrides WHERE series = $1 AND event = $2 AND role_binding_id = $3)"#,
            series as _,
            event,
            role_binding_id as _
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(result.unwrap_or(false))
    }

    pub(crate) async fn delete_for_role_binding(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_binding_id: Id<RoleBindings>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"DELETE FROM event_discord_role_overrides WHERE series = $1 AND event = $2 AND role_binding_id = $3"#,
            series as _,
            event,
            role_binding_id as _
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
                    role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    created_at AS "created_at!"
                FROM event_disabled_role_bindings
                WHERE series = $1 AND event = $2
                ORDER BY role_binding_id
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
        role_binding_id: Id<RoleBindings>,
    ) -> sqlx::Result<Id<EventDisabledRoleBindings>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO event_disabled_role_bindings (series, event, role_binding_id)
               VALUES ($1, $2, $3) RETURNING id"#,
            series as _,
            event,
            role_binding_id as _
        )
        .fetch_one(&mut **pool)
        .await?;
        Ok(Id::from(id as i64))
    }

    pub(crate) async fn delete(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_binding_id: Id<RoleBindings>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"DELETE FROM event_disabled_role_bindings WHERE series = $1 AND event = $2 AND role_binding_id = $3"#,
            series as _,
            event,
            role_binding_id as _
        )
        .execute(&mut **pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn exists_for_binding(
        pool: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
        role_binding_id: Id<RoleBindings>,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM event_disabled_role_bindings WHERE series = $1 AND event = $2 AND role_binding_id = $3)"#,
            series as _,
            event,
            role_binding_id as _
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
        // Check if event uses custom role bindings
        let uses_custom_bindings = sqlx::query_scalar!(
            r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
            series as _,
            event
        )
        .fetch_optional(&mut **pool)
        .await?
        .unwrap_or(Some(true))
        .unwrap_or(true);

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
                    rb.language AS "language: Language",
                    false AS "is_game_binding!: bool",
                    false AS "has_event_override!: bool",
                    false AS "is_disabled!: bool"
                FROM role_bindings rb
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rb.series = $1 AND rb.event = $2
                ORDER BY rb.language, rt.name
            "#,
            series as _,
            event
        )
        .fetch_all(&mut **pool)
        .await?;

        // Get game role bindings only if not using custom bindings
        let game_bindings = if !uses_custom_bindings {
            let game = match game::Game::from_series(&mut *pool, series).await {
                Ok(game) => game,
                Err(_) => None, // If we can't get the game, just continue without game bindings
            };
            if let Some(game) = game {
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
                            rb.language AS "language: Language",
                            true AS "is_game_binding!: bool",
                            false AS "has_event_override!: bool",
                            false AS "is_disabled!: bool"
                        FROM role_bindings rb
                        JOIN role_types rt ON rb.role_type_id = rt.id
                        WHERE rb.game_id = $1
                        ORDER BY rb.language, rt.name
                    "#,
                    game.id
                )
                .fetch_all(&mut **pool)
                .await?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Get event Discord role overrides
        let discord_overrides = EventDiscordRoleOverride::for_event(&mut *pool, series, event).await?;
        let discord_override_map: HashMap<Id<RoleBindings>, i64> = discord_overrides
            .into_iter()
            .map(|override_| (override_.role_binding_id, override_.discord_role_id))
            .collect();

        // Get disabled role bindings for this event
        let disabled_bindings = EventDisabledRoleBinding::for_event(&mut *pool, series, event).await?;
        let disabled_binding_ids: HashSet<Id<RoleBindings>> = disabled_bindings
            .into_iter()
            .map(|binding| binding.role_binding_id)
            .collect();

        // Combine and process all bindings
        let mut all_bindings = Vec::new();

        // Add event-specific bindings
        for mut binding in event_bindings {
            binding.has_event_override = discord_override_map.contains_key(&binding.id);
            if binding.has_event_override {
                binding.discord_role_id = discord_override_map.get(&binding.id).copied();
            }
            all_bindings.push(binding);
        }

        // Add game bindings (excluding disabled ones)
        for mut binding in game_bindings {
            if !disabled_binding_ids.contains(&binding.id) {
                binding.has_event_override = discord_override_map.contains_key(&binding.id);
                if binding.has_event_override {
                    binding.discord_role_id = discord_override_map.get(&binding.id).copied();
                }
                all_bindings.push(binding);
            }
        }

        Ok(all_bindings)
    }

    /// Get unique languages from a list of effective role bindings, sorted
    /// Prioritizes the preferred language first (if it has bindings), else English (if it has bindings)
    pub(crate) fn active_languages(bindings: &[Self], preferred_language: Language) -> Vec<Language> {
        let mut languages: Vec<Language> = bindings
            .iter()
            .map(|b| b.language)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        languages.sort_by_key(|l| l.short_code().to_string());

        // Put preferred language first if it exists, else English
        if let Some(pref_idx) = languages.iter().position(|&l| l == preferred_language) {
            let pref = languages.remove(pref_idx);
            languages.insert(0, pref);
        } else if let Some(en_idx) = languages.iter().position(|&l| l == English) {
            let en = languages.remove(en_idx);
            languages.insert(0, en);
        }

        languages
    }

    /// Filter bindings by language
    pub(crate) fn filter_by_language(bindings: &[Self], language: Language) -> Vec<&Self> {
        bindings.iter().filter(|b| b.language == language).collect()
    }
}

/// Render language tabs with binder-register visual style
/// Returns (tabs_html, content_box_start, content_box_end) for wrapping content
pub(crate) fn render_language_tabs(
    active_languages: &[Language],
    selected_language: Language,
    base_url: &str,
) -> RawHtml<String> {
    if active_languages.len() <= 1 {
        return html! {};
    }

    html! {
        nav(class = "language-tabs") {
            @for lang in active_languages {
                @let is_active = *lang == selected_language;
                @let class = if is_active { "active" } else { "" };
                a(
                    href = format!("{}?lang={}", base_url, lang.short_code()),
                    class = class
                ) {
                    : lang.to_string();
                }
            }
        }
    }
}

/// Render the start of a content box that sits below the tabs
pub(crate) fn render_language_content_box_start() -> RawHtml<String> {
    RawHtml(r#"<div class="language-tab-content">"#.to_string())
}

/// Render the end of the content box
pub(crate) fn render_language_content_box_end() -> RawHtml<String> {
    RawHtml("</div>".to_string())
}

#[rocket::post("/event/<series>/<event>/role-bindings/<role_binding_id>/disable-binding")]
pub(crate) async fn disable_role_binding(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    series: Series,
    event: &str,
    role_binding_id: Id<RoleBindings>,
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
    // Verify the binding exists and is a game binding (not event-specific)
    let binding = sqlx::query!(
        r#"SELECT game_id FROM role_bindings WHERE id = $1 AND series IS NULL AND event IS NULL"#,
        role_binding_id as _
    )
    .fetch_optional(&mut *transaction)
    .await?;

    if binding.is_none() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    // Check if already disabled
    if EventDisabledRoleBinding::exists_for_binding(&mut transaction, series, event, role_binding_id).await? {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    // Disable the role binding
    EventDisabledRoleBinding::create(&mut transaction, series, event, role_binding_id).await?;

    transaction.commit().await?;

    // Update volunteer info messages to remove the disabled role
    let _ = volunteer_requests::update_volunteer_posts_for_event(
        pool,
        &*discord_ctx.read().await,
        series,
        event,
    ).await;

    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _)))))
}

#[rocket::post("/event/<series>/<event>/role-bindings/<role_binding_id>/enable-binding")]
pub(crate) async fn enable_role_binding(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    series: Series,
    event: &str,
    role_binding_id: Id<RoleBindings>,
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
    if !EventDisabledRoleBinding::exists_for_binding(&mut transaction, series, event, role_binding_id).await? {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    // Enable the role binding
    EventDisabledRoleBinding::delete(&mut transaction, series, event, role_binding_id).await?;

    transaction.commit().await?;

    // Update volunteer info messages to add the re-enabled role
    let _ = volunteer_requests::update_volunteer_posts_for_event(
        pool,
        &*discord_ctx.read().await,
        series,
        event,
    ).await;

    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _)))))
}



#[derive(FromForm, CsrfForm)]
pub(crate) struct AddDiscordOverrideForm {
    #[field(default = String::new())]
    csrf: String,
    discord_role_id: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddDiscordOverrideFromFormData {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
    discord_role_id: String,
}

#[rocket::post("/event/<series>/<event>/role-bindings/<role_binding_id>/discord-override", data = "<form>")]
pub(crate) async fn add_discord_override(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    series: Series,
    event: &str,
    role_binding_id: Id<RoleBindings>,
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

        // Verify the role binding exists and is a game binding
        let role_binding = sqlx::query!(
            r#"SELECT rb.game_id, rb.series, rb.event
               FROM role_bindings rb
               WHERE rb.id = $1"#,
            role_binding_id as _
        )
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(binding) = role_binding {
            if binding.series.is_some() || binding.event.is_some() {
                form.context.push_error(form::Error::validation(
                    "This role binding is not a game binding.",
                ));
            }
        } else {
            form.context.push_error(form::Error::validation(
                "Invalid role binding.",
            ));
        }

        // Check if an override already exists for this role binding
        if EventDiscordRoleOverride::exists_for_role_binding(&mut transaction, series, event, role_binding_id).await? {
            form.context.push_error(form::Error::validation(
                "A Discord role override already exists for this role binding.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
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
                            &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                            data,
                            form.context,
                            csrf,
                            None,
                            None,
                        )
                        .await?,
                    ));
                }
            };

            // Add the override
            EventDiscordRoleOverride::create(&mut transaction, series, event, role_binding_id, discord_role_id).await?;

            // Retroactively assign Discord roles to existing approved volunteers for this specific role binding
            let game = game::Game::from_series(&mut transaction, series).await?;
            let approved_requests = if let Some(game) = game {
                RoleRequest::for_game(&mut transaction, game.id).await?
                    .into_iter()
                    .filter(|req| req.status == RoleRequestStatus::Approved && req.role_binding_id == role_binding_id)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            // Assign Discord roles to all approved volunteers
            for request in approved_requests {
                let user = User::from_id(&mut *transaction, request.user_id).await?;
                if let Some(user) = user {
                    if let Some(discord_user) = user.discord {
                        // Get the Discord context and guild
                        let discord_ctx = discord_ctx.read().await;
                        if let Some(discord_guild) = data.discord_guild {
                            if let Ok(member) = discord_guild.member(&*discord_ctx, discord_user.id).await {
                                if let Err(e) = member.add_role(&*discord_ctx, RoleId::new(discord_role_id.try_into().unwrap())).await {
                                    eprintln!("Failed to retroactively assign Discord role {} to user {}: {}", discord_role_id, discord_user.id, e);
                                } else {
                                    eprintln!("Successfully retroactively assigned Discord role {} to user {} for role request {}", 
                                             discord_role_id, discord_user.id, request.id);
                                }
                            }
                        }
                    }
                }
            }

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
    })
}

#[rocket::post("/event/<series>/<event>/discord-override", data = "<form>")]
pub(crate) async fn add_discord_override_from_form_data(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, AddDiscordOverrideFromFormData>>,
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

        // Verify the role binding exists and is a game binding
        let role_binding = sqlx::query!(
            r#"SELECT rb.game_id, rb.series, rb.event
               FROM role_bindings rb
               WHERE rb.id = $1"#,
            value.role_binding_id as _
        )
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(binding) = role_binding {
            if binding.series.is_some() || binding.event.is_some() {
                form.context.push_error(form::Error::validation(
                    "This role binding is not a game binding.",
                ));
            }
        } else {
            form.context.push_error(form::Error::validation(
                "Invalid role binding.",
            ));
        }

        // Check if an override already exists for this role binding
        if EventDiscordRoleOverride::exists_for_role_binding(&mut transaction, series, event, value.role_binding_id).await? {
            form.context.push_error(form::Error::validation(
                "A Discord role override already exists for this role binding.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
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
                            &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                            data,
                            form.context,
                            csrf,
                            None,
                            None,
                        )
                        .await?,
                    ));
                }
            };

            // Add the override
            EventDiscordRoleOverride::create(&mut transaction, series, event, value.role_binding_id, discord_role_id).await?;

            // Retroactively assign Discord roles to existing approved volunteers for this specific role binding
            let game = game::Game::from_series(&mut transaction, series).await?;
            let approved_requests = if let Some(game) = game {
                RoleRequest::for_game(&mut transaction, game.id).await?
                    .into_iter()
                    .filter(|req| req.status == RoleRequestStatus::Approved && req.role_binding_id == value.role_binding_id)
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            // Assign Discord roles to all approved volunteers for this role binding
            for request in approved_requests {
                let user = User::from_id(&mut *transaction, request.user_id).await?;
                if let Some(user) = user {
                    if let Some(discord_user) = user.discord {
                        // Get the Discord context and guild
                        let discord_ctx = discord_ctx.read().await;
                        if let Some(discord_guild) = data.discord_guild {
                            if let Ok(member) = discord_guild.member(&*discord_ctx, discord_user.id).await {
                                if let Err(e) = member.add_role(&*discord_ctx, RoleId::new(discord_role_id.try_into().unwrap())).await {
                                    eprintln!("Failed to retroactively assign Discord role {} to user {}: {}", discord_role_id, discord_user.id, e);
                                } else {
                                    eprintln!("Successfully retroactively assigned Discord role {} to user {} for role request {}", 
                                             discord_role_id, discord_user.id, request.id);
                                }
                            }
                        }
                    }
                }
            }

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
    })
}

#[rocket::post("/event/<series>/<event>/role-bindings/<role_binding_id>/delete-discord-override")]
pub(crate) async fn delete_discord_override(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    role_binding_id: Id<RoleBindings>,
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
    EventDiscordRoleOverride::delete_for_role_binding(&mut transaction, series, event, role_binding_id).await?;

    transaction.commit().await?;
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _)))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct CopyVolunteersForm {
    #[field(default = String::new())]
    csrf: String,
    source_event: String,
}

#[rocket::post("/event/<series>/<event>/roles/copy-volunteers", data = "<form>")]
pub(crate) async fn copy_volunteers_from_event(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, CopyVolunteersForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref form_data) = form.value {
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
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
                )
                .await?,
            )
        } else {
            let source_event = &form_data.source_event;

            // Get all approved role requests from the source event
            let source_requests = RoleRequest::for_event(&mut transaction, series, source_event).await?
                .into_iter()
                .filter(|req| matches!(req.status, RoleRequestStatus::Approved))
                .collect::<Vec<_>>();

            // Get all existing approved role requests for the target event to avoid duplicates
            let existing_requests = RoleRequest::for_event(&mut transaction, series, event).await?
                .into_iter()
                .filter(|req| matches!(req.status, RoleRequestStatus::Approved))
                .collect::<Vec<_>>();

            // Create a set of (user_id, role_type_name) pairs that already exist
            let existing_pairs: HashSet<(Id<Users>, String)> = existing_requests
                .iter()
                .map(|req| (req.user_id, req.role_type_name.clone()))
                .collect();

            // Get role bindings for the target event to map role type names to binding IDs
            let target_bindings = RoleBinding::for_event(&mut transaction, series, event).await?;
            let role_name_to_binding: HashMap<String, &RoleBinding> = target_bindings
                .iter()
                .map(|binding| (binding.role_type_name.clone(), binding))
                .collect();

            let mut copied_count = 0;
            let mut skipped_count = 0;

            for source_req in source_requests {
                // Check if this role type exists in the target event (by name)
                if let Some(&target_binding) = role_name_to_binding.get(&source_req.role_type_name) {
                    // Check if user already has this role in the target event
                    let already_exists = existing_pairs.contains(&(source_req.user_id, source_req.role_type_name.clone()));

                    if !already_exists {
                        // Create new approved role request in target event
                        sqlx::query!(
                            r#"
                            INSERT INTO role_requests (role_binding_id, user_id, status, notes)
                            VALUES ($1, $2, 'approved', $3)
                            "#,
                            i64::from(target_binding.id) as i32,
                            i64::from(source_req.user_id),
                            source_req.notes
                        )
                        .execute(&mut *transaction)
                        .await?;

                        // Assign Discord role if configured
                        if let Some(discord_role_id) = target_binding.discord_role_id {
                            let user = User::from_id(&mut *transaction, source_req.user_id).await?;
                            if let Some(user) = user {
                                if let Some(discord_user) = user.discord {
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

                        copied_count += 1;
                    } else {
                        skipped_count += 1;
                    }
                }
            }

            transaction.commit().await?;

            eprintln!("Copied {} volunteers from {} to {}. Skipped {} duplicates.",
                     copied_count, source_event, event, skipped_count);

            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct VolunteerRequestSettingsForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = false)]
    volunteer_requests_enabled: bool,
    volunteer_request_lead_time_hours: i32,
    #[field(default = false)]
    volunteer_request_ping_enabled: bool,
}

#[rocket::post("/event/<series>/<event>/roles/volunteer-request-settings", data = "<form>")]
pub(crate) async fn update_volunteer_request_settings(
    pool: &State<PgPool>,
    me: User,
    series: Series,
    event: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, VolunteerRequestSettingsForm>>
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
                "This event has ended and can no longer be configured.",
            ));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            form.context.push_error(form::Error::validation(
                "You must be an organizer to manage volunteer request settings.",
            ));
        }
        if value.volunteer_request_lead_time_hours < 1 || value.volunteer_request_lead_time_hours > 168 {
            form.context.push_error(form::Error::validation(
                "Lead time must be between 1 and 168 hours.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                roles_page(
                    transaction,
                    Some(me),
                    &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                    data,
                    form.context,
                    csrf,
                    None,
                    None,
                )
                .await?,
            )
        } else {
            sqlx::query!(
                r#"UPDATE events SET
                    volunteer_requests_enabled = $1,
                    volunteer_request_lead_time_hours = $2,
                    volunteer_request_ping_enabled = $3
                WHERE series = $4 AND event = $5"#,
                value.volunteer_requests_enabled,
                value.volunteer_request_lead_time_hours,
                value.volunteer_request_ping_enabled,
                series as _,
                event
            )
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, _, _))))
        }
    } else {
        RedirectOrContent::Content(
            roles_page(
                transaction,
                Some(me),
                &Origin(HttpOrigin::parse_owned(format!("/event/{}/{}/roles", series.slug(), event)).unwrap()),
                data,
                form.context,
                csrf,
                None,
                None,
            )
            .await?,
        )
    })
}

#[rocket::post("/event/<series>/<event>/roles/trigger-volunteer-requests", data = "<form>")]
pub(crate) async fn trigger_volunteer_requests(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, EmptyForm>>
) -> Result<Redirect, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;

    // Check organizer permission
    if !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    // Validate CSRF
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_none() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    transaction.commit().await?;

    // Trigger the check
    let discord = discord_ctx.read().await;
    match volunteer_requests::check_and_post_for_event(pool.inner(), &*discord, series, event).await {
        Ok(volunteer_requests::CheckResult::Posted(count)) => {
            println!("Manually triggered volunteer requests for {}/{}: posted {} races", series.slug(), event, count);
        }
        Ok(volunteer_requests::CheckResult::NoRacesNeeded) => {
            println!("Manually triggered volunteer requests for {}/{}: no races needed", series.slug(), event);
        }
        Ok(volunteer_requests::CheckResult::NotEnabled) => {
            println!("Manually triggered volunteer requests for {}/{}: not enabled", series.slug(), event);
        }
        Ok(volunteer_requests::CheckResult::NoChannel) => {
            println!("Manually triggered volunteer requests for {}/{}: no channel configured", series.slug(), event);
        }
        Err(e) => {
            eprintln!("Error triggering volunteer requests for {}/{}: {}", series.slug(), event, e);
        }
    }

    Ok(Redirect::to(uri!(get(series, event, _, _))))
}
