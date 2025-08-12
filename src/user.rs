use {
    convert_case::{
        Case,
        Casing as _,
    },
    sqlx::PgExecutor,
    crate::{
        auth::{
            DiscordUser,
            Discriminator,
            RaceTimeUser,
        },
        prelude::*,
    },
};

/// User preference that determines which external account a user's display name is be based on.
#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "user_display_source", rename_all = "lowercase")]
pub(crate) enum DisplaySource {
    RaceTime,
    Discord,
}

#[derive(Debug, Clone, Copy, sqlx::Type, Deserialize)]
#[sqlx(type_name = "racetime_pronouns", rename_all = "snake_case")]
pub(crate) enum RaceTimePronouns {
    #[serde(rename = "she/her")]
    She,
    #[serde(rename = "he/him")]
    He,
    #[serde(rename = "they/them")]
    They,
    #[serde(rename = "she/they")]
    SheThey,
    #[serde(rename = "he/they")]
    HeThey,
    #[serde(rename = "any/all")]
    AnyAll,
    #[serde(rename = "other/ask!")]
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct UserRaceTime {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) discriminator: Option<Discriminator>,
    pub(crate) pronouns: Option<RaceTimePronouns>,
}

#[derive(Debug, Clone)]
pub(crate) struct UserDiscord {
    pub(crate) id: UserId,
    pub(crate) display_name: String,
    pub(crate) username_or_discriminator: Either<String, Discriminator>,
}

#[derive(Debug, Clone)]
pub(crate) struct User {
    pub(crate) id: Id<Users>,
    display_source: DisplaySource, //TODO allow users with both accounts connected to set this in their preferences
    pub(crate) racetime: Option<UserRaceTime>,
    pub(crate) discord: Option<UserDiscord>,
    pub(crate) challonge_id: Option<String>,
    /// The start.gg user ID as returned by the GraphQL query `currentUser { id }` after OAuth.
    /// Not to be confused with the alphanumeric slug used in the profile page URL and on the profile page itself.
    pub(crate) startgg_id: Option<startgg::ID>,
    pub(crate) is_archivist: bool,
}

impl User {
    pub(crate) const GLOBAL_ADMIN_USER_IDS: &[u64] = &[
        16287394041462225947_u64,
    ];

    #[inline]
    pub(crate) fn is_global_admin(&self) -> bool {
        let my_id = u64::from(self.id);
        Self::GLOBAL_ADMIN_USER_IDS.iter().copied().any(|admin_id| admin_id == my_id)
    }

    pub(crate) async fn primary_global_admin<E>(executor: &mut E) -> sqlx::Result<Option<Self>>
    where
        for<'a> &'a mut E: PgExecutor<'a>,
    {
        for admin_id in Self::GLOBAL_ADMIN_USER_IDS {
            if let Some(user) = Self::from_id(&mut *executor, Id::<Users>::from(*admin_id)).await? {
					return Ok(Some(user));
				}
			}
			Ok(None)
    }
    
    fn from_row(
        id: Id<Users>,
        display_source: DisplaySource,
        racetime_id: Option<String>,
        racetime_display_name: Option<String>,
        racetime_discriminator: Option<Discriminator>,
        racetime_pronouns: Option<RaceTimePronouns>,
        discord_id: Option<PgSnowflake<UserId>>,
        discord_display_name: Option<String>,
        discord_discriminator: Option<Discriminator>,
        discord_username: Option<String>,
        challonge_id: Option<String>,
        startgg_id: Option<startgg::ID>,
        is_archivist: bool,
    ) -> Self {
        Self {
            racetime: match (racetime_id, racetime_display_name) {
                (Some(id), Some(display_name)) => Some(UserRaceTime {
                    discriminator: racetime_discriminator,
                    pronouns: racetime_pronouns,
                    id, display_name,
                }),
                (None, None) => None,
                (_, _) => unreachable!("database constraint"),
            },
            discord: match (discord_id, discord_display_name) {
                (Some(PgSnowflake(id)), Some(display_name)) => Some(UserDiscord {
                    username_or_discriminator: match (discord_username, discord_discriminator) {
                        (Some(username), None) => Either::Left(username),
                        (None, Some(discriminator)) => Either::Right(discriminator),
                        (_, _) => unreachable!("database constraint"),
                    },
                    id, display_name,
                }),
                (None, None) => None,
                (_, _) => unreachable!("database constraint"),
            },
            id, display_source, challonge_id, startgg_id, is_archivist,
        }
    }

    pub(crate) async fn from_id(pool: impl PgExecutor<'_>, id: Id<Users>) -> sqlx::Result<Option<Self>> {
        Ok(
            sqlx::query!(r#"SELECT
                display_source AS "display_source: DisplaySource",
                racetime_id,
                racetime_display_name,
                racetime_discriminator AS "racetime_discriminator: Discriminator",
                racetime_pronouns AS "racetime_pronouns: RaceTimePronouns",
                discord_id AS "discord_id: PgSnowflake<UserId>",
                discord_display_name,
                discord_discriminator AS "discord_discriminator: Discriminator",
                discord_username,
                challonge_id,
                startgg_id AS "startgg_id: startgg::ID",
                is_archivist
            FROM users WHERE id = $1"#, id as _).fetch_optional(pool).await?
            .map(|row| Self::from_row(
                id,
                row.display_source,
                row.racetime_id,
                row.racetime_display_name,
                row.racetime_discriminator,
                row.racetime_pronouns,
                row.discord_id,
                row.discord_display_name,
                row.discord_discriminator,
                row.discord_username,
                row.challonge_id,
                row.startgg_id,
                row.is_archivist,
            ))
        )
    }

    pub(crate) async fn from_racetime(pool: impl PgExecutor<'_>, racetime_id: &str) -> sqlx::Result<Option<Self>> {
        Ok(
            sqlx::query!(r#"SELECT
                id AS "id: Id<Users>",
                display_source AS "display_source: DisplaySource",
                racetime_display_name,
                racetime_discriminator AS "racetime_discriminator: Discriminator",
                racetime_pronouns AS "racetime_pronouns: RaceTimePronouns",
                discord_id AS "discord_id: PgSnowflake<UserId>",
                discord_display_name,
                discord_discriminator AS "discord_discriminator: Discriminator",
                discord_username,
                challonge_id,
                startgg_id AS "startgg_id: startgg::ID",
                is_archivist
            FROM users WHERE racetime_id = $1"#, racetime_id).fetch_optional(pool).await?
            .map(|row| Self::from_row(
                row.id,
                row.display_source,
                Some(racetime_id.to_owned()),
                row.racetime_display_name,
                row.racetime_discriminator,
                row.racetime_pronouns,
                row.discord_id,
                row.discord_display_name,
                row.discord_discriminator,
                row.discord_username,
                row.challonge_id,
                row.startgg_id,
                row.is_archivist,
            ))
        )
    }

    pub(crate) async fn from_discord(pool: impl PgExecutor<'_>, discord_id: UserId) -> sqlx::Result<Option<Self>> {
        Ok(
            sqlx::query!(r#"SELECT
                id AS "id: Id<Users>",
                display_source AS "display_source: DisplaySource",
                racetime_id,
                racetime_display_name,
                racetime_discriminator AS "racetime_discriminator: Discriminator",
                racetime_pronouns AS "racetime_pronouns: RaceTimePronouns",
                discord_display_name,
                discord_discriminator AS "discord_discriminator: Discriminator",
                discord_username,
                challonge_id,
                startgg_id AS "startgg_id: startgg::ID",
                is_archivist
            FROM users WHERE discord_id = $1"#, PgSnowflake(discord_id) as _).fetch_optional(pool).await?
            .map(|row| Self::from_row(
                row.id,
                row.display_source,
                row.racetime_id,
                row.racetime_display_name,
                row.racetime_discriminator,
                row.racetime_pronouns,
                Some(PgSnowflake(discord_id)),
                row.discord_display_name,
                row.discord_discriminator,
                row.discord_username,
                row.challonge_id,
                row.startgg_id,
                row.is_archivist,
            ))
        )
    }

    pub(crate) fn display_name(&self) -> &str {
        match self.display_source {
            DisplaySource::RaceTime => &self.racetime.as_ref().expect("user with racetime.gg display preference but no racetime.gg display name").display_name,
            DisplaySource::Discord => &self.discord.as_ref().expect("user with Discord display preference but no Discord display name").display_name,
        }
    }

    pub(crate) fn subjective_pronoun(&self) -> &'static str { //TODO also check start.gg genderPronoun field
        match self.racetime.as_ref().and_then(|racetime| racetime.pronouns) {
            Some(RaceTimePronouns::He | RaceTimePronouns::HeThey) => "he",
            Some(RaceTimePronouns::She | RaceTimePronouns::SheThey) => "she",
            Some(RaceTimePronouns::They | RaceTimePronouns::AnyAll | RaceTimePronouns::Other) | None => "they",
        }
    }

    pub(crate) fn subjective_pronoun_uses_plural_form(&self) -> bool { //TODO also check start.gg genderPronoun field
        match self.racetime.as_ref().and_then(|racetime| racetime.pronouns) {
            Some(RaceTimePronouns::He | RaceTimePronouns::HeThey) => false,
            Some(RaceTimePronouns::She | RaceTimePronouns::SheThey) => false,
            Some(RaceTimePronouns::They | RaceTimePronouns::AnyAll | RaceTimePronouns::Other) | None => true,
        }
    }

    pub(crate) fn objective_pronoun(&self) -> &'static str { //TODO also check start.gg genderPronoun field
        match self.racetime.as_ref().and_then(|racetime| racetime.pronouns) {
            Some(RaceTimePronouns::He | RaceTimePronouns::HeThey) => "him",
            Some(RaceTimePronouns::She | RaceTimePronouns::SheThey) => "her",
            Some(RaceTimePronouns::They | RaceTimePronouns::AnyAll | RaceTimePronouns::Other) | None => "them",
        }
    }

    pub(crate) fn possessive_determiner(&self) -> &'static str { //TODO also check start.gg genderPronoun field
        match self.racetime.as_ref().and_then(|racetime| racetime.pronouns) {
            Some(RaceTimePronouns::He | RaceTimePronouns::HeThey) => "his",
            Some(RaceTimePronouns::She | RaceTimePronouns::SheThey) => "her",
            Some(RaceTimePronouns::They | RaceTimePronouns::AnyAll | RaceTimePronouns::Other) | None => "their",
        }
    }

    /// Returns `Some(None)` if the user data can't be accessed. This may be because the user ID does not exist, or because the user profile is not public, see https://github.com/racetimeGG/racetime-app/blob/5892f8f80eb1bd9619244becc48bbc4607b76844/racetime/models/user.py#L274-L296
    pub(crate) async fn racetime_user_data(&self, http_client: &reqwest::Client) -> wheel::Result<Option<Option<racetime::model::UserProfile>>> {
        Ok(if let Some(ref racetime) = self.racetime {
            Some(racetime_bot::user_data(http_client, &racetime.id).await?)
        } else {
            None
        })
    }

    async fn events_organized(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<event::Data<'_>>, event::DataError> {
        let ids = sqlx::query!(r#"SELECT series AS "series: Series", event FROM organizers WHERE organizer = $1"#, self.id as _).fetch_all(&mut **transaction).await?;
        let mut buf = Vec::with_capacity(ids.len());
        for row in ids {
            buf.push(event::Data::new(&mut *transaction, row.series, row.event).await?.expect("event disappeared during transaction"));
        }
        Ok(buf)
    }

    async fn events_participated(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<event::Data<'static>>, event::DataError> {
        let teams = Team::for_member(&mut *transaction, self.id).await?;
        let mut buf = Vec::with_capacity(teams.len());
        for team in teams {
            buf.push(team.into_event(&mut *transaction).await?);
        }
        Ok(buf)
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl ToHtml for User {
    fn to_html(&self) -> RawHtml<String> {
        html! {
            a(href = uri!(profile(self.id))) {
                bdi : self.display_name();
            }
        }
    }
}

impl PartialEq for User {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for User {}

#[rocket::get("/user/<id>")]
pub(crate) async fn profile(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, racetime_user: Option<RaceTimeUser>, discord_user: Option<DiscordUser>, id: Id<Users>) -> Result<RawHtml<String>, StatusOrError<PageError>> {
    let mut transaction = pool.begin().await?;
    let user = if let Some(user) = User::from_id(&mut *transaction, id).await? {
        user
    } else {
        return Err(StatusOrError::Status(Status::NotFound))
    };
    let racetime = if let Some(ref racetime) = user.racetime {
        html! {
            p {
                : "racetime.gg: ";
                a(href = format!("https://{}/user/{}", racetime_host(), racetime.id)) {
                    bdi : racetime.display_name;
                    @if let Some(discriminator) = racetime.discriminator {
                        : "#";
                        : discriminator;
                    }
                }
                //TODO if this may be outdated, link to racetime.gg login page for refreshing
            }
        }
    } else if me.as_ref().is_some_and(|me| me.id == user.id) {
        if let Some(racetime_user) = racetime_user {
            if let Some(racetime_user) = User::from_racetime(&mut *transaction, &racetime_user.id).await? {
                let admin_user = User::from_id(&mut *transaction, Id::from(16287394041462225947_u64)).await?.ok_or(PageError::AdminUserData(3))?;
                html! {
                    p {
                        @let racetime = racetime_user.racetime.expect("racetime.gg user without racetime.gg ID");
                        : "You are also signed in via racetime.gg as ";
                        a(href = format!("https://{}/user/{}", racetime_host(), racetime.id)) {
                            bdi : racetime.display_name;
                            @if let Some(discriminator) = racetime.discriminator {
                                : "#";
                                : discriminator;
                            }
                        }
                        : " which belongs to a different Hyrule Town Hall account. ";
                        @if racetime_user.discord.is_some() {
                            : "That Hyrule Town Hall account is also connected to a Discord account. If you would like to merge your accounts, please contact ";
                            : admin_user;
                            : ".";
                        } else {
                            span { //HACK fix button styling (nth-child)
                                a(class = "button", href = uri!(crate::auth::merge_accounts)) : "Merge Accounts";
                            }
                        }
                    }
                }
            } else {
                html! {
                    p {
                        : "You are also signed in via racetime.gg as ";
                        a(href = format!("https://{}/user/{}", racetime_host(), racetime_user.id)) {
                            bdi : racetime_user.name;
                            @if let Some(discriminator) = racetime_user.discriminator {
                                : "#";
                                : discriminator;
                            }
                        }
                        : " which does not belong to a Hyrule Town Hall account. ";
                        span { //HACK fix button styling (nth-child)
                            a(class = "button", href = uri!(crate::auth::register_racetime)) : "Add this racetime.gg account to your Hyrule Town Hall account";
                        }
                    }
                }
            }
        } else {
            html! {
                p {
                    a(href = uri!(crate::auth::racetime_login(Some(uri!(profile(id)))))) : "Connect a racetime.gg account";
                }
            }
        }
    } else {
        html! {}
    };
    let discord = if let Some(ref discord) = user.discord {
        html! {
            p {
                : "Discord: ";
                a(href = format!("https://discord.com/users/{}", discord.id)) {
                    @match discord.username_or_discriminator {
                        Either::Left(ref username) => {
                            bdi : discord.display_name;
                            : " (@";
                            bdi : username;
                            : ")";
                        }
                        Either::Right(discriminator) => {
                            bdi : discord.display_name;
                            : "#";
                            : discriminator;
                        }
                    }
                }
                //TODO if this may be outdated, link to Discord login page for refreshing
            }
        }
    } else if me.as_ref().is_some_and(|me| me.id == user.id) {
        if let Some(discord_user) = discord_user {
            if let Some(discord_user) = User::from_discord(&mut *transaction, discord_user.id).await? {
                let admin_user = User::from_id(&mut *transaction, Id::from(16287394041462225947_u64)).await?.ok_or(PageError::AdminUserData(4))?;
                html! {
                    p {
                        @let discord = discord_user.discord.expect("Discord user without Discord ID");
                        : "You are also signed in via Discord as ";
                        a(href = format!("https://discord.com/users/{}", discord.id)) {
                            @match discord.username_or_discriminator {
                                Either::Left(ref username) => {
                                    bdi : discord.display_name;
                                    : " (@";
                                    bdi : username;
                                    : ")";
                                }
                                Either::Right(discriminator) => {
                                    bdi : discord.display_name;
                                    : "#";
                                    : discriminator;
                                }
                            }
                        }
                        : " which belongs to a different Hyrule Town Hall account. ";
                        @if discord_user.racetime.is_some() {
                            : "That Hyrule Town Hall account is also connected to a racetime.gg account. If you would like to merge your accounts, please contact ";
                            : admin_user;
                            : ".";
                        } else {
                            span { //HACK fix button styling (nth-child)
                                a(class = "button", href = uri!(crate::auth::merge_accounts)) : "Merge Accounts";
                            }
                        }
                    }
                }
            } else {
                html! {
                    p {
                        : "You are also signed in via Discord as ";
                        a(href = format!("https://discord.com/users/{}", discord_user.id)) {
                            @if let Some(discriminator) = discord_user.discriminator {
                                bdi : discord_user.username;
                                : "#";
                                : discriminator;
                            } else {
                                bdi : discord_user.global_name.unwrap_or_else(|| discord_user.username.clone());
                                : " (@";
                                bdi : discord_user.username;
                                : ")";
                            }
                        }
                        : " which does not belong to a Hyrule Town Hall account. ";
                        span { //HACK fix button styling (nth-child)
                            a(class = "button", href = uri!(crate::auth::register_discord)) : "Add this Discord account to your Hyrule Town Hall account";
                        }
                    }
                }
            }
        } else {
            html! {
                p {
                    a(href = uri!(crate::auth::discord_login(Some(uri!(profile(id)))))) : "Connect a Discord account";
                }
            }
        }
    } else {
        html! {}
    };
    let mut events_organized = user.events_organized(&mut transaction).await?;
    events_organized.retain(|event| event.listed);
    let mut events_participated = user.events_participated(&mut transaction).await?;
    events_participated.retain(|event| event.listed);
    Ok(page(transaction, &me, &uri, PageStyle { kind: if me.as_ref().is_some_and(|me| *me == user) { PageKind::MyProfile } else { PageKind::Other }, ..PageStyle::default() }, &format!("{} — Hyrule Town Hall", user.display_name()), html! {
        h1 {
            bdi : user.display_name();
        }
        p {
            : "Mido's House user ID: ";
            code : user.id.to_string();
        }
        : racetime;
        : discord;
        @if user.is_archivist {
            p {
                : "This user is an archivist: ";
                : user.subjective_pronoun().to_case(Case::Title);
                @if user.subjective_pronoun_uses_plural_form() {
                    : " help";
                } else {
                    : " helps";
                }
                : " with adding data like race room and restream links to past races.";
            }
        }
        p {
            : "Hyrule Town Hall user ID: ";
            code : user.id.to_string();
        }
        @if !events_organized.is_empty() {
            p : "This user has organized the following events:";
            ul {
                @for event in events_organized {
                    li : event;
                }
            }
        }
        @if !events_participated.is_empty() {
            p : "This user has participated in the following events:";
            ul {
                @for event in events_participated {
                    li : event;
                }
            }
        }
    }).await?)
}
