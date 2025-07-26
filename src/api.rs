use {
    async_graphql::{
        Context,
        EmptySubscription,
        Error,
        Guard,
        ID as GqlId,
        InputValueError,
        InputValueResult,
        Object,
        Result,
        Scalar,
        ScalarType,
        Schema,
        SimpleObject,
        Value,
        http::{
            GraphQLPlaygroundConfig,
            playground_source,
        },
    },
    async_graphql_rocket::{
        GraphQLQuery,
        GraphQLRequest,
        GraphQLResponse,
    },
    rocket::http::ContentType,
    rocket::serde::json::Json,
    crate::{
        auth::Discriminator,
        event::teams,
        lang::Language,
        prelude::*,
    },
};

macro_rules! db {
    ($db:ident = $ctx:expr; $expr:expr) => {
        lock!($db = $ctx.data_unchecked::<ArcTransaction>(); $expr)
    };
}

#[derive(Default, PartialEq, Eq)]
struct Scopes {
    entrants_read: bool,
    user_search: bool,
    write: bool,
}

impl Scopes {
    async fn validate(&self, transaction: &mut Transaction<'_, Postgres>, api_key: &str) -> sqlx::Result<Option<user::User>> {
        let Some(key_scope) = sqlx::query_as!(Self, "SELECT entrants_read, user_search, write FROM api_keys WHERE key = $1", api_key).fetch_optional(&mut **transaction).await? else { return Ok(None) };
        if key_scope >= *self {
            let user_id = sqlx::query_scalar!(r#"SELECT user_id AS "user_id: Id<Users>" FROM api_keys WHERE key = $1"#, api_key).fetch_one(&mut **transaction).await?;
            user::User::from_id(&mut **transaction, user_id).await
        } else {
            Ok(None)
        }
    }
}

impl PartialOrd for Scopes {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let mut any_less = false;
        let mut any_greater = false;

        macro_rules! compare_fields {
            ($($field:ident,)+) => {
                let Self { $($field),+ } = *self;
                $(
                    match ($field, other.$field) {
                        (false, false) | (true, true) => {}
                        (false, true) => any_less = true,
                        (true, false) => any_greater = true,
                    }
                )+
            };
        }

        compare_fields![
            entrants_read,
            user_search,
            write,
        ];
        match (any_less, any_greater) {
            (false, false) => Some(Equal),
            (false, true) => Some(Greater),
            (true, false) => Some(Less),
            (true, true) => None,
        }
    }
}

impl fmt::Display for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { entrants_read, user_search, write } = *self;
        let mut scopes = Vec::default();
        if entrants_read { scopes.push("entrants_read") }
        if user_search { scopes.push("user_search") }
        if write { scopes.push("write") }
        let plural = scopes.len() != 1;
        if let Some(scopes) = English.join_str_opt(scopes) {
            scopes.fmt(f)?;
            if plural {
                write!(f, " scopes")
            } else {
                write!(f, " scope")
            }
        } else {
            write!(f, "any scope")
        }
    }
}

impl Guard for Scopes {
    async fn check(&self, ctx: &Context<'_>) -> Result<()> {
        if ctx.data::<ApiKey>().map_err(|e| Error {
            message: format!("This query requires an API key with {self}. Provide one using the X-API-Key header."),
            source: Some(Arc::new(e)),
            extensions: None,
        })?.scopes >= *self {
            Ok(())
        } else {
            Err(format!("Your API key is missing scopes, it needs {self}.").into())
        }
    }
}

struct ShowRestreamConsent<'a>(&'a cal::Race);

impl Guard for ShowRestreamConsent<'_> {
    async fn check(&self, ctx: &Context<'_>) -> Result<()> {
        let me = &ctx.data::<ApiKey>().map_err(|e| Error {
            message: format!("This query requires an API key. Provide one using the X-API-Key header."),
            source: Some(Arc::new(e)),
            extensions: None,
        })?.user;
        db!(db = ctx; {
            let event = self.0.event(&mut *db).await?;
            if event.organizers(&mut *db).await?.contains(me) || event.restreamers(&mut *db).await?.contains(me) {
                Ok(())
            } else {
                Err("Only event organizers and restream coordinators can view restream consent info.".into())
            }
        })
    }
}

struct EditRace(GqlId);

impl Guard for EditRace {
    async fn check(&self, ctx: &Context<'_>) -> Result<()> {
        let me = &ctx.data::<ApiKey>().map_err(|e| Error {
            message: format!("This query requires an API key. Provide one using the X-API-Key header."),
            source: Some(Arc::new(e)),
            extensions: None,
        })?.user;
        db!(db = ctx; if me.is_archivist {
            Ok(())
        } else {
            let race = cal::Race::from_id(&mut *db, ctx.data_unchecked(), (&self.0).try_into()?).await?;
            let event = race.event(&mut *db).await?;
            if event.organizers(&mut *db).await?.contains(me) || event.restreamers(&mut *db).await?.contains(me) {
                Ok(())
            } else {
                Err("Only archivists, event organizers, and restream coordinators can edit races.".into())
            }
        })
    }
}

type ArcTransaction = Arc<Mutex<Transaction<'static, Postgres>>>;

impl<T: crate::id::Table> TryFrom<GqlId> for Id<T> {
    type Error = std::num::ParseIntError;

    fn try_from(value: GqlId) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl<'a, T: crate::id::Table> TryFrom<&'a GqlId> for Id<T> {
    type Error = std::num::ParseIntError;

    fn try_from(value: &GqlId) -> Result<Self, Self::Error> {
        Ok(value.parse::<u64>()?.into())
    }
}

struct UtcTimestamp(DateTime<Utc>);

impl<Z: TimeZone> From<DateTime<Z>> for UtcTimestamp {
    fn from(value: DateTime<Z>) -> Self {
        Self(value.to_utc())
    }
}

#[Scalar]
/// A date and time in UTC, formatted per ISO 8601.
impl ScalarType for UtcTimestamp {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(s) = value {
            Ok(Self(DateTime::parse_from_rfc3339(&s).map_err(InputValueError::custom)?.to_utc()))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_rfc3339_opts(SecondsFormat::AutoSi, true))
    }

    fn is_valid(value: &Value) -> bool {
        if let Value::String(s) = value {
            DateTime::parse_from_rfc3339(s).is_ok()
        } else {
            false
        }
    }
}

#[derive(SimpleObject)]
struct SwissStanding {
    placement: usize,
    name: String,
    wins: usize,
    losses: usize,
}

type MidosHouseSchema = Schema<Query, Mutation, EmptySubscription>;

pub(crate) struct Query;

#[derive(Debug, thiserror::Error)]
enum UserFromDiscordError {
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

#[Object] impl Query {
    /// Custom racetime.gg goals in the OoTR category handled by Mido instead of RandoBot.
    async fn goal_names(&self) -> Vec<&'static str> {
        all::<racetime_bot::Goal>().filter(|goal| goal.is_custom()).map(|goal| goal.as_str()).collect()
    }

    /// Returns a series (group of events) by its URL part.
    async fn series(&self, name: String) -> Option<Series> {
        name.parse().ok().map(Series)
    }

    /// Returns the Mido's House user connected to the given racetime.gg user ID, if any.
    /// Requires an API key with `user_search` scope.
    #[graphql(guard = Scopes { user_search: true, ..Scopes::default() })]
    async fn user_from_racetime(&self, ctx: &Context<'_>, id: GqlId) -> sqlx::Result<Option<User>> {
        Ok(db!(db = ctx; user::User::from_racetime(&mut **db, id.as_str()).await?).map(User))
    }

    /// Returns the Mido's House user connected to the given Discord user snowflake ID, if any.
    /// Requires an API key with `user_search` scope.
    #[graphql(guard = Scopes { user_search: true, ..Scopes::default() })]
    async fn user_from_discord(&self, ctx: &Context<'_>, id: GqlId) -> Result<Option<User>, UserFromDiscordError> {
        Ok(db!(db = ctx; user::User::from_discord(&mut **db, id.parse()?).await?).map(User))
    }
}

pub(crate) struct Mutation;

#[Object] impl Mutation {
    /// Requires permission to edit races and an API key with the `write` scope.
    #[graphql(guard = Scopes { write: true, ..Scopes::default() }.and(EditRace(id.clone())))]
    async fn set_race_restream_url(&self, ctx: &Context<'_>, id: GqlId, language: Language, restream_url: String) -> Result<Race> {
        db!(db = ctx; {
            let mut race = cal::Race::from_id(&mut *db, ctx.data_unchecked(), id.try_into()?).await?;
            race.video_urls.insert(language, restream_url.parse()?);
            let me = &ctx.data::<ApiKey>().map_err(|e| Error {
                message: format!("This query requires an API key. Provide one using the X-API-Key header."),
                source: Some(Arc::new(e)),
                extensions: None,
            })?.user;
            race.last_edited_by = Some(me.id);
            race.last_edited_at = Some(Utc::now());
            race.save(&mut *db).await?;
            Ok(Race(race))
        })
    }

    /// `restreamer` must be a racetime.gg profile URL, racetime.gg user ID, or Mido's House user ID.
    /// Requires permission to edit races and an API key with the `write` scope.
    #[graphql(guard = Scopes { write: true, ..Scopes::default() }.and(EditRace(id.clone())))]
    async fn set_race_restreamer(&self, ctx: &Context<'_>, id: GqlId, language: Language, restreamer: String) -> Result<Race> {
        db!(db = ctx; {
            let mut race = cal::Race::from_id(&mut *db, ctx.data_unchecked(), id.try_into()?).await?;
            race.restreamers.insert(language, crate::racetime_bot::parse_user(&mut *db, ctx.data_unchecked(), &restreamer).await?);
            let me = &ctx.data::<ApiKey>().map_err(|e| Error {
                message: format!("This query requires an API key. Provide one using the X-API-Key header."),
                source: Some(Arc::new(e)),
                extensions: None,
            })?.user;
            race.last_edited_by = Some(me.id);
            race.last_edited_at = Some(Utc::now());
            race.save(&mut *db).await?;
            Ok(Race(race))
        })
    }
}

struct Series(crate::series::Series);

#[Object] impl Series {
    /// Returns an event by its URL part.
    async fn event(&self, ctx: &Context<'_>, name: String) -> Result<Option<Event>, event::DataError> {
        Ok(db!(db = ctx; event::Data::new(&mut *db, self.0, name).await?).map(Event))
    }
}

struct Event(event::Data<'static>);

#[Object] impl Event {
    /// All past, upcoming, and unscheduled races for this event, sorted chronologically.
    async fn races(&self, ctx: &Context<'_>) -> Result<Vec<Race>, cal::Error> {
        Ok(db!(db = ctx; cal::Race::for_event(&mut *db, ctx.data_unchecked(), &self.0).await?).into_iter().map(Race).collect())
    }

    #[graphql(guard = Scopes { entrants_read: true, ..Scopes::default() })]
    async fn swiss_standings(&self, ctx: &Context<'_>) -> Result<Option<Vec<SwissStanding>>, event::DataError> {
        let http_client = ctx.data_unchecked::<reqwest::Client>();
        let config = ctx.data_unchecked::<Config>();
        match db!(db = ctx; self.0.swiss_standings(&mut *db, http_client, config).await) {
            Ok(Some(standings)) => Ok(Some(standings.into_iter().map(|s| SwissStanding {
                placement: s.placement,
                name: s.name,
                wins: s.wins,
                losses: s.losses,
            }).collect())),
            Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }
}

struct Race(cal::Race);

#[Object] impl Race {
    /// The race's internal ID. Unique across all series, but only for races (e.g. a user may have the same ID as a race).
    async fn id(&self) -> GqlId { self.0.id.into() }

    /// The scheduled starting time. Null if this race is asynced or not yet scheduled.
    async fn start(&self) -> Option<UtcTimestamp> {
        if let RaceSchedule::Live { start, .. } = self.0.schedule {
            Some(start.into())
        } else {
            None
        }
    }

    /// The time the scheduling of this race was last changed. Null if this race's schedule has not been touched or if the event does not use Mido's House to schedule races.
    /// This info was not tracked before 2024-01-04 so this is also null for races whose schedule was last changed before then.
    async fn schedule_updated_at(&self, ctx: &Context<'_>) -> sqlx::Result<Option<UtcTimestamp>> {
        Ok(db!(db = ctx; sqlx::query_scalar!("SELECT schedule_updated_at FROM races WHERE id = $1", self.0.id as _).fetch_one(&mut **db).await?).map(UtcTimestamp::from))
    }

    /// The race room URL. Null if no room has been opened yet or if this race is asynced.
    async fn room(&self) -> Option<&str> {
        if let RaceSchedule::Live { ref room, .. } = self.0.schedule {
            room.as_ref().map(|url| url.as_str())
        } else {
            None
        }
    }

    /// A categorization of races within the event, e.g. “Swiss”, “Challenge Cup”, “Live Qualifier”, “Top 8”, “Groups”, or “Bracket”. Combine with round, entrants, and game for a human-readable description of the race.
    /// Null if this event only has one phase or for the main phase of the event (e.g. Standard top 64 as opposed to Challenge Cup).
    async fn phase(&self) -> Option<&str> { self.0.phase.as_deref() }

    /// A categorization of races within the phase, e.g. “Round 1”, “Openers”, or “Losers Quarterfinal”. Combine with phase, entrants, and game for a human-readable description of the race.
    /// Null if this phase only has one match or if all matches in this phase are equivalent (e.g. a leaderboard phase).
    async fn round(&self) -> Option<&str> { self.0.round.as_deref() }

    /// If this race is part of a best-of-N-races match, the ordinal of the race within the match, counting from 1. Null for best-of-1 matches.
    async fn game(&self) -> Option<i16> { self.0.game }

    /// All teams participating in this race. For solo events, these will be single-member teams.
    /// Null if the race is open (not invitational) or if the event does not use Mido's House to manage entrants.
    async fn teams(&self, ctx: &Context<'_>) -> Result<Option<Vec<Team>>, event::DataError> {
        let event = db!(db = ctx; self.0.event(&mut *db).await?);
        Ok(self.0.teams_opt().map(|teams| teams.map(|team| Team { inner: team.clone(), event: event.clone() }).collect()))
    }

    /// Whether all teams in this race have consented to be restreamed.
    /// Null if the race is open (not invitational) or if the event does not use Mido's House to manage entrants.
    /// Requires permission to view restream consent and an API key with `entrants_read` scope.
    #[graphql(guard = Scopes { entrants_read: true, ..Scopes::default() }.and(ShowRestreamConsent(&self.0)))]
    async fn restream_consent(&self) -> Option<bool> {
        self.0.teams_opt().map(|mut teams| teams.all(|team| team.restream_consent))
    }

    /// All restream URLs for this race, organized by language.
    /// Null if the race is open (not invitational) or if the event does not use Mido's House to manage entrants.
    /// Requires permission to view restream consent and an API key with `entrants_read` scope.
    #[graphql(guard = Scopes { entrants_read: true, ..Scopes::default() }.and(ShowRestreamConsent(&self.0)))]
    async fn restream_urls(&self) -> Option<Vec<RestreamUrl>> {
        self.0.teams_opt().map(|_| {
            self.0.video_urls.iter().map(|(language, url)| RestreamUrl {
                language: *language,
                url: url.to_string(),
            }).collect()
        })
    }

    /// All restreamers for this race, organized by language.
    /// Null if the race is open (not invitational) or if the event does not use Mido's House to manage entrants.
    /// Requires permission to view restream consent and an API key with `entrants_read` scope.
    #[graphql(guard = Scopes { entrants_read: true, ..Scopes::default() }.and(ShowRestreamConsent(&self.0)))]
    async fn restreamers(&self) -> Option<Vec<Restreamer>> {
        self.0.teams_opt().map(|_| {
            self.0.restreamers.iter().map(|(language, restreamer)| Restreamer {
                language: *language,
                restreamer: restreamer.clone(),
            }).collect()
        })
    }

    /// All confirmed volunteers for this race.
    /// Null if the race is open (not invitational) or if the event does not use Mido's House to manage entrants.
    /// Requires permission to view restream consent and an API key with `entrants_read` scope.
    #[graphql(guard = Scopes { entrants_read: true, ..Scopes::default() }.and(ShowRestreamConsent(&self.0)))]
    async fn confirmed_volunteers(&self, ctx: &Context<'_>) -> sqlx::Result<Option<Vec<ConfirmedVolunteer>>> {
        if self.0.teams_opt().is_none() {
            return Ok(None);
        }

        let signups = db!(db = ctx; event::roles::Signup::for_race(&mut *db, self.0.id).await?);
        let confirmed_signups: Vec<_> = signups.into_iter()
            .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Confirmed))
            .collect();

        if confirmed_signups.is_empty() {
            return Ok(Some(vec![]));
        }

        let mut volunteers = Vec::new();
        for signup in confirmed_signups {
            if let Some(user) = db!(db = ctx; user::User::from_id(&mut **db, signup.user_id).await?) {
                volunteers.push(ConfirmedVolunteer {
                    user: User(user),
                    role_type_name: signup.role_type_name,
                    notes: signup.notes,
                });
            }
        }

        Ok(Some(volunteers))
    }
}

struct Team {
    inner: team::Team,
    event: event::Data<'static>,
}

#[Object] impl Team {
    /// The team's internal ID. Unique across all series, but only for teams (e.g. a race may have the same ID as a team).
    async fn id(&self) -> GqlId { self.inner.id.into() }

    /// The team's display name. Null for solo events or if the team did not specify a name.
    async fn name(&self) -> Option<&str> { self.inner.name.as_deref() }

    /// Members are guaranteed to be listed in a consistent order depending on the team configuration of the event, e.g. pictionary events will always list the runner first and the pilot second.
    async fn members(&self, ctx: &Context<'_>) -> sqlx::Result<Vec<TeamMember>> {
        let team_config = self.event.team_config;
        let members = db!(db = ctx; self.inner.members(&mut *db).await?);
        let roles = team_config.roles();
        Ok(
            members.into_iter().zip_eq(roles)
                .map(|(user, (_, display_name))| TeamMember {
                    role: (!matches!(team_config, TeamConfig::Solo)).then(|| (*display_name).to_owned()),
                    user: User(user),
                })
                .collect()
        )
    }
}

#[derive(SimpleObject)]
struct TeamMember {
    /// The role of the player within this team for team formats with distinct roles (e.g. multiworld, pictionary).
    /// For team formats without distinct roles (e.g. co-op), this can be used to get a consistent ordering of the members of a team.
    /// Null for solo events.
    role: Option<String>,
    user: User,
}

struct User(user::User);

#[Object] impl User {
    /// The user's internal ID. Only unique for users (e.g. a team may have the same ID as a user).
    async fn id(&self) -> GqlId { self.0.id.into() }

    /// The user's Mido's House display name.
    async fn display_name(&self) -> &str { self.0.display_name() }


    /// Returns the user's connected racetime.gg user ID, if any.
    async fn racetime_id(&self) -> Option<GqlId> {
        self.0.racetime.as_ref().map(|racetime| GqlId::from(&racetime.id))
    }

    /// Returns the user's connected Discord user snowflake ID, if any.
    async fn discord_id(&self) -> Option<GqlId> {
        self.0.discord.as_ref().map(|discord| discord.id.into())
    }
}

#[derive(SimpleObject)]
struct RestreamUrl {
    /// The language for this restream URL.
    language: crate::lang::Language,
    /// The restream URL.
    url: String,
}

#[derive(SimpleObject)]
struct Restreamer {
    /// The language for this restreamer.
    language: crate::lang::Language,
    /// The restreamer's racetime.gg user ID.
    restreamer: String,
}

#[derive(SimpleObject)]
struct ConfirmedVolunteer {
    /// The volunteer user.
    user: User,
    /// The type of role the volunteer is performing (e.g., "Commentary", "Tracking", "Race Monitoring").
    role_type_name: String,
    /// Optional notes about the volunteer's role.
    notes: Option<String>,
}

pub(crate) fn schema(db_pool: PgPool) -> MidosHouseSchema {
    Schema::build(Query, Mutation, EmptySubscription)
        .data(db_pool)
        .finish()
}

pub(crate) struct ApiKey {
    scopes: Scopes,
    user: user::User,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiKeyFromRequestError {
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error("failed to get database connection pool")]
    DbPool,
    #[error("the X-API-Key header was not specified")]
    MissingHeader,
    #[error("the X-API-Key header was specified multiple times")]
    MultipleHeaders,
    #[error("the given API key does not exist")]
    NoSuchApiKey,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiKey {
    type Error = ApiKeyFromRequestError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let db_pool = match <&State<PgPool>>::from_request(req).await {
            request::Outcome::Success(db_pool) => db_pool,
            request::Outcome::Forward(status) => return request::Outcome::Forward(status),
            request::Outcome::Error((status, ())) => return request::Outcome::Error((status, ApiKeyFromRequestError::DbPool)),
        };
        match req.headers().get("X-API-Key").at_most_one() {
            Ok(Some(api_key)) => match sqlx::query!(r#"SELECT
                entrants_read,
                user_search,
                write,
                user_id AS "user_id: Id<Users>"
            FROM api_keys WHERE key = $1"#, api_key).fetch_optional(&**db_pool).await {
                Ok(Some(row)) => request::Outcome::Success(Self {
                    scopes: Scopes {
                        entrants_read: row.entrants_read,
                        user_search: row.user_search,
                        write: row.write,
                    },
                    user: match user::User::from_id(&**db_pool, row.user_id).await {
                        Ok(user) => user.expect("database constraint validated: API keys belong to existing users"),
                        Err(e) => return request::Outcome::Error((Status::InternalServerError, ApiKeyFromRequestError::Sql(e))),
                    },
                }),
                Ok(None) => request::Outcome::Error((Status::Unauthorized, ApiKeyFromRequestError::NoSuchApiKey)),
                Err(e) => request::Outcome::Error((Status::InternalServerError, ApiKeyFromRequestError::Sql(e))),
            },
            Ok(None) => request::Outcome::Error((Status::Unauthorized, ApiKeyFromRequestError::MissingHeader)),
            Err(_) => request::Outcome::Error((Status::Unauthorized, ApiKeyFromRequestError::MultipleHeaders)),
        }
    }
}

#[rocket::get("/api/v1/graphql?<query..>")]
pub(crate) async fn graphql_query(config: &State<Config>, db_pool: &State<PgPool>, http_client: &State<reqwest::Client>, schema: &State<MidosHouseSchema>, api_key: Option<ApiKey>, query: GraphQLQuery) -> Result<GraphQLResponse, rocket_util::Error<sqlx::Error>> {
    let transaction = Arc::new(Mutex::new(db_pool.begin().await?));
    let mut request = GraphQLRequest::from(query)
        .data::<Config>((*config).clone())
        .data::<ArcTransaction>(transaction.clone())
        .data::<reqwest::Client>((*http_client).clone());
    if let Some(api_key) = api_key {
        request = request.data::<ApiKey>(api_key);
    }
    let response = request.execute(&**schema).await;
    Arc::try_unwrap(transaction).expect("query data still live after execution").into_inner().commit().await?;
    Ok(response)
}

#[rocket::post("/api/v1/graphql", data = "<request>", format = "application/json")]
pub(crate) async fn graphql_request(config: &State<Config>, db_pool: &State<PgPool>, http_client: &State<reqwest::Client>, schema: &State<MidosHouseSchema>, api_key: Option<ApiKey>, request: GraphQLRequest) -> Result<GraphQLResponse, rocket_util::Error<sqlx::Error>> {
    let transaction = Arc::new(Mutex::new(db_pool.begin().await?));
    let mut request = request
        .data::<Config>((*config).clone())
        .data::<ArcTransaction>(transaction.clone())
        .data::<reqwest::Client>((*http_client).clone());
    if let Some(api_key) = api_key {
        request = request.data::<ApiKey>(api_key);
    }
    let response = request.execute(&**schema).await;
    Arc::try_unwrap(transaction).expect("query data still live after execution").into_inner().commit().await?;
    Ok(response)
}

#[rocket::get("/api/v1/graphql")]
pub(crate) fn graphql_playground() -> RawHtml<String> {
    RawHtml(playground_source(GraphQLPlaygroundConfig::new("/api/v1/graphql")))
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum CsvError {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] Csv(#[from] csv::Error),
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] EventData(#[from] event::DataError),
    #[error(transparent)] IntoInner(#[from] csv::IntoInnerError<csv::Writer<Vec<u8>>>),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
}

impl<E: Into<CsvError>> From<E> for StatusOrError<CsvError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

#[rocket::get("/api/v1/event/<series>/<event>/entrants.csv?<api_key>")]
pub(crate) async fn entrants_csv(db_pool: &State<PgPool>, http_client: &State<reqwest::Client>, series: crate::series::Series, event: &str, api_key: &str) -> Result<(ContentType, Vec<u8>), StatusOrError<CsvError>> {
    let mut transaction = db_pool.begin().await?;
    let me = Scopes { entrants_read: true, ..Scopes::default() }.validate(&mut transaction, api_key).await?.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let is_organizer = event.organizers(&mut transaction).await?.contains(&me);
    if !is_organizer && !event.restreamers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden))
    }
    let qualifier_kind = teams::QualifierKind::Single { //TODO adjust to match teams::get?
        show_times: event.show_qualifier_times && event.is_started(&mut transaction).await?,
    };
    let signups = teams::signups_sorted(&mut transaction, &mut teams::Cache::new(http_client.inner().clone()), None, &event, is_organizer, qualifier_kind, None).await?;
    let mut csv = csv::Writer::from_writer(Vec::default());
    for (i, teams::SignupsTeam { team, .. }) in signups.into_iter().enumerate() {
        if let Some(team) = team {
            for member in team.members(&mut transaction).await? {
                #[derive(Serialize)]
                struct Row<'a> {
                    id: Id<Users>,
                    display_name: &'a str,
                    twitch_display_name: Option<String>,
                    discord_display_name: Option<&'a str>,
                    discord_discriminator: Option<Discriminator>,
                    racetime_id: Option<&'a str>,
                    qualifier_rank: usize,
                    restream_consent: bool,
                    discord_username: Option<&'a str>,
                }

                csv.serialize(Row {
                    id: member.id,
                    display_name: member.display_name(),
                    twitch_display_name: member.racetime_user_data(http_client).await?.and_then(identity).and_then(|racetime_user_data| racetime_user_data.twitch_display_name),
                    discord_display_name: member.discord.as_ref().map(|discord| &*discord.display_name),
                    discord_discriminator: member.discord.as_ref().and_then(|discord| discord.username_or_discriminator.as_ref().right()).copied(),
                    racetime_id: member.racetime.as_ref().map(|racetime| &*racetime.id),
                    qualifier_rank: i + 1,
                    restream_consent: team.restream_consent,
                    discord_username: member.discord.as_ref().and_then(|discord| discord.username_or_discriminator.as_ref().left()).map(|username| &**username),
                })?;
            }
        }
    }
    Ok((ContentType::CSV, csv.into_inner()?))
}

#[rocket::get("/api/v1/event/<series>/<event>/swiss-standings?<api_key>")]
pub(crate) async fn swiss_standings_endpoint(
    db_pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    config: &State<Config>,
    series: crate::series::Series,
    event: &str,
    api_key: &str,
) -> Result<Json<Vec<startgg::SwissStanding>>, StatusOrError<CsvError>> {
    use crate::event::Data;
    let mut transaction = db_pool.begin().await?;
    let _me = Scopes { entrants_read: true, ..Scopes::default() }.validate(&mut transaction, api_key).await?.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    // Only allow for Startgg events
    if !matches!(event_data.match_source(), MatchSource::StartGG(_)) {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    let startgg_token = if Environment::default().is_dev() {
        &config.startgg_dev
    } else {
        &config.startgg_production
    };
    // Extract slug from the event's URL
    let slug = match event_data.url.as_ref().and_then(|url| url.path().strip_prefix('/').map(|s| s.to_string())) {
        Some(s) if !s.is_empty() => s,
        _ => return Err(StatusOrError::Status(Status::NotFound)),
    };
    let standings = startgg::swiss_standings(
        http_client.inner(),
        &*config,
        &slug,
        startgg_token,
    ).await.map_err(|_| StatusOrError::Status(Status::NotFound))?;
    Ok(Json(standings))
}
