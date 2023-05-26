use {
    std::{
        fmt,
        time::Duration,
    },
    futures::future::TryFutureExt as _,
    lazy_regex::regex_is_match,
    rocket::{
        State,
        http::{
            Cookie,
            CookieJar,
            SameSite,
            Status,
            ext::IntoOwned as _,
        },
        outcome::Outcome,
        request::{
            self,
            FromRequest,
            Request,
        },
        response::{
            Redirect,
            content::RawHtml,
        },
        uri,
    },
    rocket_oauth2::{
        OAuth2,
        TokenResponse,
    },
    rocket_util::{
        Error,
        Origin,
        ToHtml,
        html,
    },
    serde::Deserialize,
    serde_plain::derive_serialize_from_display,
    serenity::model::prelude::*,
    sqlx::PgPool,
    wheel::traits::ReqwestResponseExt as _,
    crate::{
        Environment,
        http::{
            PageError,
            PageKind,
            PageResult,
            PageStyle,
            page,
        },
        user::{
            RaceTimePronouns,
            User,
        },
        util::{
            Id,
            IdTable,
        },
    },
};

macro_rules! guard_try {
    ($res:expr) => {
        match $res {
            Ok(x) => x,
            Err(e) => return Outcome::Failure((Status::InternalServerError, e.into())),
        }
    };
}

pub(crate) enum RaceTime {}
pub(crate) enum Discord {}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum UserFromRequestError {
    #[error(transparent)] OAuth(#[from] rocket_oauth2::Error),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Time(#[from] rocket::time::error::ConversionRange),
    #[error(transparent)] TryFromInt(#[from] std::num::TryFromIntError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("neither racetime_token cookie nor discord_token cookie present")]
    Cookie,
    #[error("missing database connection")]
    Database,
    #[error("missing HTTP client")]
    HttpClient,
    #[error("failed to get racetime.gg host from environment")]
    RaceTimeHost,
    #[error("user to view as does not exist")]
    ViewAsNoSuchUser,
}

async fn handle_racetime_token_response(env: &State<Environment>, client: &reqwest::Client, cookies: &CookieJar<'_>, token: &TokenResponse<RaceTime>) -> Result<RaceTimeUser, UserFromRequestError> {
    let mut cookie = Cookie::build("racetime_token", token.access_token().to_owned())
        .same_site(SameSite::Lax);
    if let Some(expires_in) = token.expires_in() {
        cookie = cookie.max_age(Duration::from_secs(u64::try_from(expires_in)?.saturating_sub(60)).try_into()?);
    }
    cookies.add_private(cookie.finish());
    if let Some(refresh_token) = token.refresh_token() {
        cookies.add_private(Cookie::build("racetime_refresh_token", refresh_token.to_owned())
            .same_site(SameSite::Lax)
            .permanent()
            .finish());
    }
    Ok(client.get(format!("https://{}/o/userinfo", env.racetime_host()))
        .bearer_auth(token.access_token())
        .send().await?
        .detailed_error_for_status().await?
        .json_with_text_in_error().await?)
}

async fn handle_discord_token_response(client: &reqwest::Client, cookies: &CookieJar<'_>, token: &TokenResponse<Discord>) -> Result<DiscordUser, UserFromRequestError> {
    let mut cookie = Cookie::build("discord_token", token.access_token().to_owned())
        .same_site(SameSite::Lax);
    if let Some(expires_in) = token.expires_in() {
        cookie = cookie.max_age(Duration::from_secs(u64::try_from(expires_in)?.saturating_sub(60)).try_into()?);
    }
    cookies.add_private(cookie.finish());
    if let Some(refresh_token) = token.refresh_token() {
        cookies.add_private(Cookie::build("discord_refresh_token", refresh_token.to_owned())
            .same_site(SameSite::Lax)
            .permanent()
            .finish());
    }
    Ok(client.get("https://discord.com/api/v10/users/@me")
        .bearer_auth(token.access_token())
        .send().await?
        .detailed_error_for_status().await?
        .json_with_text_in_error().await?)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SerdeDiscriminator {
    Number(i16),
    String(String),
}

#[derive(Debug, thiserror::Error)]
enum InvalidDiscriminator {
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[error("discriminator must be between 0 and 10000, got {0}")]
    Range(i16),
    #[error("discriminator must be 4 digits 0-9")]
    StringPattern,
}

impl TryFrom<SerdeDiscriminator> for Discriminator {
    type Error = InvalidDiscriminator;

    fn try_from(value: SerdeDiscriminator) -> Result<Self, InvalidDiscriminator> {
        let number = match value {
            SerdeDiscriminator::Number(n) => n,
            SerdeDiscriminator::String(s) => if regex_is_match!("^[0-9]{4}$", &s) {
                s.parse()?
            } else {
                return Err(InvalidDiscriminator::StringPattern)
            },
        };
        if number > 9999 { return Err(InvalidDiscriminator::Range(number)) }
        Ok(Self(number))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, sqlx::Type)]
#[serde(try_from = "SerdeDiscriminator")]
#[sqlx(transparent)]
pub(crate) struct Discriminator(i16);

impl fmt::Display for Discriminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.0)
    }
}

derive_serialize_from_display!(Discriminator);

impl ToHtml for Discriminator {
    fn to_html(&self) -> RawHtml<String> {
        RawHtml(self.to_string())
    }
}

#[derive(Deserialize)]
pub(crate) struct RaceTimeUser {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) discriminator: Option<Discriminator>,
    pronouns: Option<RaceTimePronouns>,
}

#[derive(Deserialize)]
pub(crate) struct DiscordUser {
    pub(crate) id: UserId,
    pub(crate) username: String,
    pub(crate) discriminator: Discriminator,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for RaceTimeUser {
    type Error = UserFromRequestError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, UserFromRequestError> {
        match req.guard::<&CookieJar<'_>>().await {
            Outcome::Success(cookies) => match req.guard::<&State<reqwest::Client>>().await {
                Outcome::Success(client) => match req.guard::<&State<Environment>>().await {
                    Outcome::Success(env) => if let Some(token) = cookies.get_private("racetime_token") {
                        match client.get(format!("https://{}/o/userinfo", env.racetime_host()))
                            .bearer_auth(token.value())
                            .send()
                            .err_into::<UserFromRequestError>()
                            .and_then(|response| response.detailed_error_for_status().err_into())
                            .await
                        {
                            Ok(response) => Outcome::Success(guard_try!(response.json_with_text_in_error().await)),
                            Err(e) => Outcome::Failure((Status::BadGateway, e.into())),
                        }
                    } else if let Some(token) = cookies.get_private("racetime_refresh_token") {
                        match req.guard::<OAuth2<RaceTime>>().await {
                            Outcome::Success(oauth) => Outcome::Success(guard_try!(handle_racetime_token_response(env, client, cookies, &guard_try!(oauth.refresh(token.value()).await)).await)),
                            Outcome::Failure((status, ())) => Outcome::Failure((status, UserFromRequestError::Cookie)),
                            Outcome::Forward(()) => Outcome::Forward(()),
                        }
                    } else {
                        Outcome::Failure((Status::Unauthorized, UserFromRequestError::Cookie))
                    },
                    Outcome::Failure((status, ())) => Outcome::Failure((status, UserFromRequestError::RaceTimeHost)),
                    Outcome::Forward(()) => Outcome::Forward(()),
                },
                Outcome::Failure((status, ())) => Outcome::Failure((status, UserFromRequestError::HttpClient)),
                Outcome::Forward(()) => Outcome::Forward(()),
            },
            Outcome::Failure((_, never)) => match never {},
            Outcome::Forward(()) => Outcome::Forward(()),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for DiscordUser {
    type Error = UserFromRequestError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, UserFromRequestError> {
        match req.guard::<&CookieJar<'_>>().await {
            Outcome::Success(cookies) => match req.guard::<&State<reqwest::Client>>().await {
                Outcome::Success(client) => if let Some(token) = cookies.get_private("discord_token") {
                    match client.get("https://discord.com/api/v10/users/@me")
                        .bearer_auth(token.value())
                        .send()
                        .err_into::<UserFromRequestError>()
                        .and_then(|response| response.detailed_error_for_status().err_into())
                        .await
                    {
                        Ok(response) => Outcome::Success(guard_try!(response.json_with_text_in_error().await)),
                        Err(e) => Outcome::Failure((Status::BadGateway, e.into())),
                    }
                } else if let Some(token) = cookies.get_private("discord_refresh_token") {
                    match req.guard::<OAuth2<Discord>>().await {
                        Outcome::Success(oauth) => Outcome::Success(guard_try!(handle_discord_token_response(client, cookies, &guard_try!(oauth.refresh(token.value()).await)).await)),
                        Outcome::Failure((status, ())) => Outcome::Failure((status, UserFromRequestError::Cookie)),
                        Outcome::Forward(()) => Outcome::Forward(()),
                    }
                } else {
                    Outcome::Failure((Status::Unauthorized, UserFromRequestError::Cookie))
                },
                Outcome::Failure((status, ())) => Outcome::Failure((status, UserFromRequestError::HttpClient)),
                Outcome::Forward(()) => Outcome::Forward(()),
            },
            Outcome::Failure((_, never)) => match never {},
            Outcome::Forward(()) => Outcome::Forward(()),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = UserFromRequestError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, UserFromRequestError> {
        match req.guard::<&State<PgPool>>().await {
            Outcome::Success(pool) => {
                let mut found_user = Err((Status::Unauthorized, UserFromRequestError::Cookie));
                match req.guard::<RaceTimeUser>().await {
                    Outcome::Success(racetime_user) => if let Some(user) = guard_try!(User::from_racetime(&**pool, &racetime_user.id).await) {
                        guard_try!(sqlx::query!("UPDATE users SET racetime_display_name = $1, racetime_discriminator = $2, racetime_pronouns = $3 WHERE id = $4", racetime_user.name, racetime_user.discriminator as _, racetime_user.pronouns as _, user.id as _).execute(&**pool).await);
                        found_user = found_user.or(Ok(user));
                    },
                    Outcome::Forward(()) => {}
                    Outcome::Failure(e) => found_user = found_user.or(Err(e)),
                }
                match req.guard::<DiscordUser>().await {
                    Outcome::Success(discord_user) => if let Some(user) = guard_try!(User::from_discord(&**pool, discord_user.id).await) {
                        guard_try!(sqlx::query!("UPDATE users SET discord_display_name = $1, discord_discriminator = $2 WHERE id = $3", discord_user.username, discord_user.discriminator as _, user.id as _).execute(&**pool).await);
                        found_user = found_user.or(Ok(user));
                    },
                    Outcome::Forward(()) => {},
                    Outcome::Failure(e) => found_user = found_user.or(Err(e)),
                };
                match found_user {
                    Ok(user) => if let Some(user_id) = guard_try!(sqlx::query_scalar!(r#"SELECT view_as AS "view_as: Id" FROM view_as WHERE viewer = $1"#, user.id as _).fetch_optional(&**pool).await) {
                        if let Some(user) = guard_try!(User::from_id(&**pool, user_id).await) {
                            Outcome::Success(user)
                        } else {
                            Outcome::Failure((Status::InternalServerError, UserFromRequestError::ViewAsNoSuchUser))
                        }
                    } else {
                        Outcome::Success(user)
                    },
                    Err(e) => Outcome::Failure(e),
                }
            }
            Outcome::Failure((status, ())) => Outcome::Failure((status, UserFromRequestError::Database)),
            Outcome::Forward(()) => Outcome::Forward(()),
        }
    }
}

#[rocket::get("/login?<redirect_to>")]
pub(crate) async fn login(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, redirect_to: Option<Origin<'_>>) -> PageResult {
    page(pool.begin().await?, &me, &uri, PageStyle { kind: PageKind::Login, ..PageStyle::default() }, "Login — Mido's House", if let Some(ref me) = me {
        html! {
            p {
                : "You are already signed in as ";
                : me;
                : ".";
            }
            ul {
                @if me.racetime_id.is_none() {
                    li {
                        a(href = uri!(racetime_login(redirect_to.clone())).to_string()) : "Connect a racetime.gg account";
                    }
                }
                @if me.discord_id.is_none() {
                    li {
                        a(href = uri!(discord_login(redirect_to.clone())).to_string()) : "Connect a Discord account";
                    }
                }
                li {
                    a(href = uri!(logout(redirect_to)).to_string()) : "Sign out";
                }
            }
        }
    } else {
        html! {
            p : "To sign in or create a new account, please sign in via one of the following services:";
            ul {
                li {
                    a(href = uri!(racetime_login(redirect_to.clone())).to_string()) : "Sign in with racetime.gg";
                }
                li {
                    a(href = uri!(discord_login(redirect_to)).to_string()) : "Sign in with Discord";
                }
            }
        }
    }).await
}

#[rocket::get("/login/racetime?<redirect_to>")]
pub(crate) fn racetime_login(oauth: OAuth2<RaceTime>, cookies: &CookieJar<'_>, redirect_to: Option<Origin<'_>>) -> Result<Redirect, Error<rocket_oauth2::Error>> {
    if let Some(redirect_to) = redirect_to {
        if redirect_to.0.path() != uri!(racetime_callback).path() && redirect_to.0.path() != uri!(discord_callback).path() { // prevent showing login error page on login success
            cookies.add(Cookie::build("redirect_to", redirect_to).same_site(SameSite::Lax).finish());
        }
    }
    oauth.get_redirect(cookies, &["read"]).map_err(Error)
}

#[rocket::get("/login/discord?<redirect_to>")]
pub(crate) fn discord_login(oauth: OAuth2<Discord>, cookies: &CookieJar<'_>, redirect_to: Option<Origin<'_>>) -> Result<Redirect, Error<rocket_oauth2::Error>> {
    if let Some(redirect_to) = redirect_to {
        if redirect_to.0.path() != uri!(racetime_callback).path() && redirect_to.0.path() != uri!(discord_callback).path() { // prevent showing login error page on login success
            cookies.add(Cookie::build("redirect_to", redirect_to).same_site(SameSite::Lax).finish());
        }
    }
    oauth.get_redirect(cookies, &["identify"]).map_err(Error)
}

#[derive(Debug, thiserror::Error, Error)]
pub(crate) enum RaceTimeCallbackError {
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Register(#[from] RegisterError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] UserFromRequest(#[from] UserFromRequestError),
}

#[rocket::get("/auth/racetime")]
pub(crate) async fn racetime_callback(env: &State<Environment>, pool: &State<PgPool>, me: Option<User>, client: &State<reqwest::Client>, token: TokenResponse<RaceTime>, cookies: &CookieJar<'_>) -> Result<Redirect, RaceTimeCallbackError> {
    let mut transaction = pool.begin().await?;
    let racetime_user = handle_racetime_token_response(env, client, cookies, &token).await?;
    let redirect_uri = cookies.get("redirect_to").and_then(|cookie| rocket::http::uri::Origin::try_from(cookie.value()).ok()).map_or_else(|| uri!(crate::http::index), |uri| uri.into_owned());
    Ok(if User::from_racetime(&mut transaction, &racetime_user.id).await?.is_some() {
        Redirect::to(redirect_uri)
    } else {
        register_racetime_inner(pool, me, Some(racetime_user), Some(redirect_uri)).await?
    })
}

#[derive(Debug, thiserror::Error, Error)]
pub(crate) enum DiscordCallbackError {
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)] Register(#[from] RegisterError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] UserFromRequest(#[from] UserFromRequestError),
}

#[rocket::get("/auth/discord")]
pub(crate) async fn discord_callback(pool: &State<PgPool>, me: Option<User>, client: &State<reqwest::Client>, token: TokenResponse<Discord>, cookies: &CookieJar<'_>) -> Result<Redirect, DiscordCallbackError> {
    let mut transaction = pool.begin().await?;
    let discord_user = handle_discord_token_response(client, cookies, &token).await?;
    let redirect_uri = cookies.get("redirect_to").and_then(|cookie| rocket::http::uri::Origin::try_from(cookie.value()).ok()).map_or_else(|| uri!(crate::http::index), |uri| uri.into_owned());
    Ok(if User::from_discord(&mut transaction, discord_user.id).await?.is_some() {
        Redirect::to(redirect_uri)
    } else {
        register_discord_inner(pool, me, Some(discord_user), Some(redirect_uri)).await?
    })
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum RegisterError {
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error("there is already an account associated with this Discord account")]
    ExistsDiscord,
    #[error("there is already an account associated with this racetime.gg account")]
    ExistsRaceTime,
}

async fn register_racetime_inner(pool: &State<PgPool>, me: Option<User>, racetime_user: Option<RaceTimeUser>, redirect_uri: Option<rocket::http::uri::Origin<'static>>) -> Result<Redirect, RegisterError> {
    Ok(if let Some(racetime_user) = racetime_user {
        let mut transaction = pool.begin().await?;
        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM users WHERE racetime_id = $1) AS "exists!""#, racetime_user.id).fetch_one(&mut transaction).await? {
            return Err(RegisterError::ExistsRaceTime) //TODO user-facing error message
        } else if let Some(me) = me {
            sqlx::query!("UPDATE users SET racetime_id = $1, racetime_display_name = $2, racetime_discriminator = $3, racetime_pronouns = $4 WHERE id = $5", racetime_user.id, racetime_user.name, racetime_user.discriminator as _, racetime_user.pronouns as _, me.id as _).execute(&mut transaction).await?;
            transaction.commit().await?;
            Redirect::to(redirect_uri.unwrap_or_else(|| uri!(crate::user::profile(me.id))))
        } else {
            let id = Id::new(&mut transaction, IdTable::Users).await?;
            sqlx::query!("INSERT INTO users (id, display_source, racetime_id, racetime_display_name, racetime_discriminator, racetime_pronouns) VALUES ($1, 'racetime', $2, $3, $4, $5)", id as _, racetime_user.id, racetime_user.name, racetime_user.discriminator as _, racetime_user.pronouns as _).execute(&mut transaction).await?;
            transaction.commit().await?;
            Redirect::to(redirect_uri.unwrap_or_else(|| uri!(crate::user::profile(id))))
        }
    } else {
        Redirect::to(uri!(racetime_login(_)))
    })
}

async fn register_discord_inner(pool: &State<PgPool>, me: Option<User>, discord_user: Option<DiscordUser>, redirect_uri: Option<rocket::http::uri::Origin<'static>>) -> Result<Redirect, RegisterError> {
    Ok(if let Some(discord_user) = discord_user {
        let mut transaction = pool.begin().await?;
        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM users WHERE discord_id = $1) AS "exists!""#, i64::from(discord_user.id)).fetch_one(&mut transaction).await? {
            return Err(RegisterError::ExistsDiscord) //TODO user-facing error message
        } else if let Some(me) = me {
            sqlx::query!("UPDATE users SET discord_id = $1, discord_display_name = $2, discord_discriminator = $3 WHERE id = $4", i64::from(discord_user.id), discord_user.username, discord_user.discriminator as _, me.id as _).execute(&mut transaction).await?;
            transaction.commit().await?;
            Redirect::to(redirect_uri.unwrap_or_else(|| uri!(crate::user::profile(me.id))))
        } else {
            let id = Id::new(&mut transaction, IdTable::Users).await?;
            sqlx::query!("INSERT INTO users (id, display_source, discord_id, discord_display_name, discord_discriminator) VALUES ($1, 'discord', $2, $3, $4)", id as _, i64::from(discord_user.id), discord_user.username, discord_user.discriminator as _).execute(&mut transaction).await?;
            transaction.commit().await?;
            Redirect::to(redirect_uri.unwrap_or_else(|| uri!(crate::user::profile(id))))
        }
    } else {
        Redirect::to(uri!(discord_login(_)))
    })
}

#[rocket::get("/register/racetime")]
pub(crate) async fn register_racetime(pool: &State<PgPool>, me: Option<User>, racetime_user: Option<RaceTimeUser>) -> Result<Redirect, RegisterError> {
    register_racetime_inner(pool, me, racetime_user, None).await
}

#[rocket::get("/register/discord")]
pub(crate) async fn register_discord(pool: &State<PgPool>, me: Option<User>, discord_user: Option<DiscordUser>) -> Result<Redirect, RegisterError> {
    register_discord_inner(pool, me, discord_user, None).await
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum MergeAccountsError {
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error("accounts already merged")]
    AlreadyMerged,
    #[error("failed to merge accounts")]
    Other,
}

#[rocket::get("/merge-accounts")]
pub(crate) async fn merge_accounts(pool: &State<PgPool>, me: User, racetime_user: Option<RaceTimeUser>, discord_user: Option<DiscordUser>) -> Result<Redirect, MergeAccountsError> {
    let mut transaction = pool.begin().await?;
    match (me.racetime_id, me.discord_id) {
        (Some(_), Some(_)) => return Err(MergeAccountsError::AlreadyMerged),
        (Some(_), None) => if let Some(discord_user) = discord_user {
            if let Ok(Some(discord_user)) = User::from_discord(&mut transaction, discord_user.id).await {
                if discord_user.racetime_id.is_none() {
                    sqlx::query!("UPDATE users SET discord_id = $1, discord_display_name = $2, discord_discriminator = $3 WHERE id = $4", i64::from(discord_user.discord_id.expect("Discord user without Discord ID")), discord_user.discord_display_name, discord_user.discord_discriminator as _, me.id as _).execute(&mut transaction).await?;
                    sqlx::query!("DELETE FROM users WHERE id = $1", discord_user.id as _).execute(&mut transaction).await?;
                    transaction.commit().await?;
                    return Ok(Redirect::to(uri!(crate::user::profile(me.id))))
                }
            }
        },
        (None, Some(_)) => if let Some(racetime_user) = racetime_user {
            if let Ok(Some(racetime_user)) = User::from_racetime(&mut transaction, &racetime_user.id).await {
                if racetime_user.discord_id.is_none() {
                    sqlx::query!("UPDATE users SET racetime_id = $1, racetime_display_name = $2, racetime_discriminator = $3, racetime_pronouns = $4 WHERE id = $5", racetime_user.racetime_id, racetime_user.racetime_display_name, racetime_user.racetime_discriminator as _, racetime_user.racetime_pronouns as _, me.id as _).execute(&mut transaction).await?;
                    sqlx::query!("DELETE FROM users WHERE id = $1", racetime_user.id as _).execute(&mut transaction).await?;
                    transaction.commit().await?;
                    return Ok(Redirect::to(uri!(crate::user::profile(me.id))))
                }
            }
        },
        (None, None) => unreachable!("signed in but nether account connected"),
    }
    transaction.rollback().await?;
    Err(MergeAccountsError::Other)
}

#[rocket::get("/logout?<redirect_to>")]
pub(crate) fn logout(cookies: &CookieJar<'_>, redirect_to: Option<Origin<'_>>) -> Redirect {
    cookies.remove_private(Cookie::named("racetime_token"));
    cookies.remove_private(Cookie::named("discord_token"));
    cookies.remove_private(Cookie::named("racetime_refresh_token"));
    cookies.remove_private(Cookie::named("discord_refresh_token"));
    Redirect::to(redirect_to.map_or_else(|| uri!(crate::http::index), |uri| uri.0.into_owned()))
}
