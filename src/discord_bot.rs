use {
    crate::{
        config::ConfigRaceTime,
        prelude::*,
        racetime_bot::{AlttprDeRaceOptions, CleanShutdown, CrosskeysRaceOptions, GlobalState},
        async_race::{AsyncRaceManager, Error as AsyncRaceError},
        volunteer_requests,
    }, serenity::all::{
        CacheHttp,
        CommandDataOptionValue,
        Content,
        CreateActionRow,
        CreateAllowedMentions,
        CreateButton,
        CreateCommand,
        CreateCommandOption,
        CreateForumPost,
        CreateInteractionResponse,
        CreateInteractionResponseMessage,
        CreateMessage,
        CreateThread,
        EditInteractionResponse,
        EditRole,
    }, serenity_utils::{
        builder::ErrorNotifier,
        handler::HandlerMethods as _,
    }, sqlx::{
        types::Json, Database, Decode, Encode, postgres::types::PgInterval
    }, std::{
        marker::Sync,
        cmp::Ordering::{Less, Greater, Equal},
    }
};

pub(crate) const ADMIN_USER: UserId = UserId::new(82783364175630336); // TreZ
const BUTTONS_PER_PAGE: usize = 25;

/// A wrapper around serenity's Discord snowflake types that can be stored in a PostgreSQL database as a BIGINT.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PgSnowflake<T>(pub(crate) T);

impl<'r, T: From<NonZero<u64>>, DB: Database> Decode<'r, DB> for PgSnowflake<T>
where i64: Decode<'r, DB> {
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let id = i64::decode(value)?;
        let id = NonZero::try_from(id as u64)?;
        Ok(Self(id.into()))
    }
}

impl<'q, T: Copy + Into<i64>, DB: Database> Encode<'q, DB> for PgSnowflake<T>
where i64: Encode<'q, DB> {
    fn encode_by_ref(&self, buf: &mut <DB as Database>::ArgumentBuffer<'q>) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        self.0.into().encode(buf)
    }

    fn encode(self, buf: &mut <DB as Database>::ArgumentBuffer<'q>) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        self.0.into().encode(buf)
    }

    fn produces(&self) -> Option<<DB as Database>::TypeInfo> {
        self.0.into().produces()
    }

    fn size_hint(&self) -> usize {
        Encode::size_hint(&self.0.into())
    }
}

impl<T, DB: Database> sqlx::Type<DB> for PgSnowflake<T>
where i64: sqlx::Type<DB> {
    fn type_info() -> <DB as Database>::TypeInfo {
        i64::type_info()
    }

    fn compatible(ty: &<DB as Database>::TypeInfo) -> bool {
        i64::compatible(ty)
    }
}

#[async_trait]
pub(crate) trait MessageBuilderExt {
    async fn mention_entrant(&mut self, transaction: &mut Transaction<'_, Postgres>, guild: Option<GuildId>, entrant: &Entrant) -> sqlx::Result<&mut Self>;
    async fn mention_team(&mut self, transaction: &mut Transaction<'_, Postgres>, guild: Option<GuildId>, team: &Team) -> sqlx::Result<&mut Self>;
    fn mention_user(&mut self, user: &User) -> &mut Self;
    fn push_emoji(&mut self, emoji: &ReactionType) -> &mut Self;
    fn push_named_link_no_preview(&mut self, name: impl Into<Content>, url: impl Into<Content>) -> &mut Self;
    fn push_named_link_safe_no_preview(&mut self, name: impl Into<Content>, url: impl Into<Content>) -> &mut Self;
}

#[async_trait]
impl MessageBuilderExt for MessageBuilder {
    async fn mention_entrant(&mut self, transaction: &mut Transaction<'_, Postgres>, guild: Option<GuildId>, entrant: &Entrant) -> sqlx::Result<&mut Self> {
        match entrant {
            Entrant::MidosHouseTeam(team) => { self.mention_team(transaction, guild, team).await?; }
            Entrant::Discord { id,  .. } => { self.mention(id); }
            Entrant::Named { name, .. } => { self.push_safe(name); }
        }
        Ok(self)
    }

    async fn mention_team(&mut self, transaction: &mut Transaction<'_, Postgres>, guild: Option<GuildId>, team: &Team) -> sqlx::Result<&mut Self> {
        if let Ok(member) = team.members(&mut *transaction).await?.into_iter().exactly_one() {
            self.mention_user(&member);
        } else {
            let team_role = if let (Some(guild), Some(racetime_slug)) = (guild, &team.racetime_slug) {
                sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND racetime_team = $2"#, PgSnowflake(guild) as _, racetime_slug).fetch_optional(&mut **transaction).await?
            } else {
                None
            };
            if let Some(PgSnowflake(team_role)) = team_role {
                self.role(team_role);
            } else if let Some(team_name) = team.name(transaction).await? {
                if let Some(ref racetime_slug) = team.racetime_slug {
                    self.push_named_link_safe_no_preview(team_name, format!("https://{}/team/{racetime_slug}", racetime_host()));
                } else {
                    self.push_italic_safe(team_name);
                }
            } else {
                if let Some(ref racetime_slug) = team.racetime_slug {
                    self.push_named_link_safe_no_preview("an unnamed team", format!("https://{}/team/{racetime_slug}", racetime_host()));
                } else {
                    self.push("an unnamed team");
                }
            }
        }
        Ok(self)
    }

    fn mention_user(&mut self, user: &User) -> &mut Self {
        if let Some(ref discord) = user.discord {
            self.mention(&discord.id)
        } else {
            self.push_safe(user.display_name())
        }
    }

    fn push_emoji(&mut self, emoji: &ReactionType) -> &mut Self {
        self.push(emoji.to_string())
    }

    fn push_named_link_no_preview(&mut self, name: impl Into<Content>, url: impl Into<Content>) -> &mut Self {
        self.push_named_link(name, format!("<{}>", url.into()))
    }

    fn push_named_link_safe_no_preview(&mut self, name: impl Into<Content>, url: impl Into<Content>) -> &mut Self {
        self.push_named_link_safe(name, format!("<{}>", url.into()))
    }
}

pub(crate) enum DbPool {}

impl TypeMapKey for DbPool {
    type Value = PgPool;
}

enum HttpClient {}

impl TypeMapKey for HttpClient {
    type Value = reqwest::Client;
}

enum RacetimeHost {}

impl TypeMapKey for RacetimeHost {
    type Value = racetime::HostInfo;
}

enum StartggToken {}

impl TypeMapKey for StartggToken {
    type Value = String;
}

enum ChallongeApiKey {}

impl TypeMapKey for ChallongeApiKey {
    type Value = String;
}

enum NewRoomLock {}

impl TypeMapKey for NewRoomLock {
    type Value = Arc<Mutex<()>>;
}



#[derive(Clone, Copy)]
pub(crate) struct CommandIds {
    pub(crate) ban: Option<CommandId>,
    delete_after: CommandId,
    draft: Option<CommandId>,
    pub(crate) first: Option<CommandId>,
    pub(crate) no: Option<CommandId>,
    pub(crate) pick: Option<CommandId>,
    post_status: CommandId,
    pronoun_roles: CommandId,
    racing_role: CommandId,
    reset_race: CommandId,
    pub(crate) schedule: CommandId,
    pub(crate) schedule_async: CommandId,
    pub(crate) result_async: CommandId,
    pub(crate) forfeit_async: CommandId,
    pub(crate) schedule_remove: CommandId,
    pub(crate) second: Option<CommandId>,
    pub(crate) skip: Option<CommandId>,
    status: CommandId,
    watch_roles: CommandId,
    pub(crate) yes: Option<CommandId>,
}

impl TypeMapKey for CommandIds {
    type Value = HashMap<GuildId, Option<CommandIds>>;
}

pub(crate) const MULTIWORLD_GUILD: GuildId = GuildId::new(826935332867276820);

#[cfg_attr(not(unix), allow(unused))] // only constructed in UNIX socket handler
#[derive(Clone, Copy, PartialEq, Eq, Sequence)]
pub(crate) enum Element {
    Light,
    Forest,
    Fire,
    Water,
    Shadow,
    Spirit,
}

impl Element {
    pub(crate) fn voice_channel(&self) -> ChannelId {
        match self {
            Self::Light => ChannelId::new(1096152882962768032),
            Self::Forest => ChannelId::new(1096153269933441064),
            Self::Fire => ChannelId::new(1096153203508260884),
            Self::Water => ChannelId::new(1096153240049025024),
            Self::Shadow => ChannelId::new(1242773533600387143),
            Self::Spirit => ChannelId::new(1242774260682985573),
        }
    }
}

impl TypeMapKey for Element {
    type Value = HashMap<UserId, Element>;
}

#[async_trait]
trait GenericInteraction {
    fn channel_id(&self) -> ChannelId;
    fn guild_id(&self) -> Option<GuildId>;
    fn user_id(&self) -> UserId;
    async fn create_response(&self, cache_http: impl CacheHttp, builder: CreateInteractionResponse) -> serenity::Result<()>;
    async fn edit_response(&self, cache_http: impl CacheHttp, builder: EditInteractionResponse) -> serenity::Result<Message>;
}

#[async_trait]
impl GenericInteraction for CommandInteraction {
    fn channel_id(&self) -> ChannelId { self.channel_id }
    fn guild_id(&self) -> Option<GuildId> { self.guild_id }
    fn user_id(&self) -> UserId { self.user.id }

    async fn create_response(&self, cache_http: impl CacheHttp, builder: CreateInteractionResponse) -> serenity::Result<()> {
        self.create_response(cache_http, builder).await
    }
    async fn edit_response(&self, cache_http: impl CacheHttp, builder: EditInteractionResponse) -> serenity::Result<Message> {
        self.edit_response(cache_http, builder).await
    }
}

#[async_trait]
impl GenericInteraction for ComponentInteraction {
    fn channel_id(&self) -> ChannelId { self.channel_id }
    fn guild_id(&self) -> Option<GuildId> { self.guild_id }
    fn user_id(&self) -> UserId { self.user.id }

    async fn create_response(&self, cache_http: impl CacheHttp, builder: CreateInteractionResponse) -> serenity::Result<()> {
        self.create_response(cache_http, builder).await
    }
    async fn edit_response(&self, cache_http: impl CacheHttp, builder: EditInteractionResponse) -> serenity::Result<Message> {
        self.edit_response(cache_http, builder).await
    }
}

//TODO refactor (MH admins should have permissions, room already being open should not remove permissions but only remove the team from return)
async fn check_scheduling_thread_permissions<'a>(ctx: &'a DiscordCtx, interaction: &impl GenericInteraction, game: Option<i16>, allow_rooms_for_other_teams: bool, alternative_instructions: Option<&str>, already_deferred: bool) -> Result<Option<(Transaction<'a, Postgres>, Race, Option<Team>)>, Box<dyn std::error::Error + Send + Sync>> {
    let (mut transaction, http_client) = {
        let data = ctx.data.read().await;
        (
            data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?,
            data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
        )
    };
    let mut applicable_races = Race::for_scheduling_channel(&mut transaction, &http_client, interaction.channel_id(), game, false).await?;
    if let Some(Some(min_game)) = applicable_races.iter().map(|race| race.game).min() {
        // None < Some(_) so this code only runs if all applicable races are best-of-N
        applicable_races.retain(|race| race.game == Some(min_game));
    }
    Ok(match applicable_races.into_iter().at_most_one() {
        Ok(None) => {
            let command_ids = ctx.data.read().await.get::<CommandIds>().and_then(|command_ids| command_ids.get(&interaction.guild_id()?))
                .expect("interaction called from outside registered guild")
                .expect("interaction called from guild with conflicting draft kinds");
            let mut content = MessageBuilder::default();
            match (Race::for_scheduling_channel(&mut transaction, &http_client, interaction.channel_id(), game, true).await?.is_empty(), game.is_some()) {
                (false, false) => {
                    content.push("Sorry, this thread is not associated with any upcoming races. ");
                    if let Some(alternative_instructions) = alternative_instructions {
                        content.push(alternative_instructions);
                        content.push(", or tournament organizers can use ");
                    } else {
                        content.push("Tournament organizers can use ");
                    }
                    content.mention_command(command_ids.reset_race, "reset-race");
                    content.push(" if necessary.");
                }
                (false, true) => {
                    content.push("Sorry, there don't seem to be any upcoming races with that game number associated with this thread. ");
                    if let Some(alternative_instructions) = alternative_instructions {
                        content.push(alternative_instructions);
                        content.push(", or tournament organizers can use ");
                    } else {
                        content.push("Tournament organizers can use ");
                    }
                    content.mention_command(command_ids.reset_race, "reset-race");
                    content.push(" if necessary.");
                }
                (true, false) => { content.push("Sorry, this thread is not associated with any upcoming races. Please contact a tournament organizer to fix this."); }
                (true, true) => { content.push("Sorry, there don't seem to be any upcoming races with that game number associated with this thread. If this seems wrong, please contact a tournament organizer to fix this."); }
            }
            if already_deferred {
                interaction.edit_response(ctx, EditInteractionResponse::new()
                    .content(content.build())
                ).await?;
            } else {
                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content(content.build())
                )).await?;
            }
            transaction.rollback().await?;
            None
        }
        Ok(Some(race)) => {
            let mut team = None;
            for iter_team in race.teams() {
                if iter_team.members(&mut transaction).await?.into_iter().any(|member| member.discord.is_some_and(|discord| discord.id == interaction.user_id())) {
                    team = Some(iter_team.clone());
                    break
                }
            }
            if let Some(ref team) = team {
                let blocked = if allow_rooms_for_other_teams {
                    race.has_room_for(team)
                } else {
                    race.has_any_room()
                };
                if blocked {
                    if already_deferred {
                        interaction.edit_response(ctx, EditInteractionResponse::new()
                            .content("Sorry, this command can't be used since a race room is already open. Please contact a tournament organizer if necessary.")
                        ).await?;
                    } else {
                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                            .ephemeral(true)
                            .content("Sorry, this command can't be used since a race room is already open. Please contact a tournament organizer if necessary.")
                        )).await?;
                    }
                    transaction.rollback().await?;
                    return Ok(None)
                }
            }
            Some((transaction, race, team))
        }
        Err(_) => {
            if already_deferred {
                interaction.edit_response(ctx, EditInteractionResponse::new()
                    .content("Sorry, this thread is associated with multiple upcoming races. Please contact a tournament organizer to fix this.")
                ).await?;
            } else {
                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content("Sorry, this thread is associated with multiple upcoming races. Please contact a tournament organizer to fix this.")
                )).await?;
            }
            transaction.rollback().await?;
            None
        }
    })
}

async fn check_draft_permissions<'a>(ctx: &'a DiscordCtx, interaction: &impl GenericInteraction) -> Result<Option<(event::Data<'static>, Race, draft::Kind, draft::MessageContext<'a>)>, Box<dyn std::error::Error + Send + Sync>> {
    let Some((mut transaction, race, team)) = check_scheduling_thread_permissions(ctx, interaction, None, false, Some("You can continue the draft in the race room"), false).await? else { return Ok(None) };
    let guild_id = interaction.guild_id().expect("Received interaction from outside of a guild");
    let event = race.event(&mut transaction).await?;
    Ok(if let Some(team) = team {
        if let Some(draft_kind) = event.draft_kind() {
            if let Some(ref draft) = race.draft {
                if draft.is_active_team(draft_kind, race.game, team.id).await? {
                    let msg_ctx = draft::MessageContext::Discord {
                        command_ids: ctx.data.read().await.get::<CommandIds>().and_then(|command_ids| command_ids.get(&guild_id))
                            .expect("draft action called from outside registered guild")
                            .expect("interaction called from guild with conflicting draft kinds"),
                        teams: race.teams().cloned().collect(),
                        transaction, guild_id, team,
                    };
                    Some((event, race, draft_kind, msg_ctx))
                } else {
                    let response_content = if let French = event.language {
                        format!("Désolé, mais ce n'est pas votre tour.")
                    } else {
                        format!("Sorry, it's not {} turn in the settings draft.", if let TeamConfig::Solo = event.team_config { "your" } else { "your team's" })
                    };
                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                        .ephemeral(true)
                        .content(response_content)
                    )).await?;
                    transaction.rollback().await?;
                    None
                }
            } else {
                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content("Sorry, this race's settings draft has not been initialized. Please contact a tournament organizer to fix this.")
                )).await?;
                transaction.rollback().await?;
                None
            }
        } else {
            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                .ephemeral(true)
                .content("Sorry, there is no settings draft for this event.")
            )).await?;
            transaction.rollback().await?;
            None
        }
    } else {
        let response_content = if let French = event.language {
            "Désolé, seuls les participants de la race peuvent utiliser cette commande."
        } else {
            "Sorry, only participants in this race can use this command."
        };
        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
            .ephemeral(true)
            .content(response_content)
        )).await?;
        transaction.rollback().await?;
        None
    })
}

async fn send_draft_settings_page(ctx: &DiscordCtx, interaction: &impl GenericInteraction, action: &str, page: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some((event, mut race, draft_kind, mut msg_ctx)) = check_draft_permissions(ctx, interaction).await? else { return Ok(()) };
    match race.draft.as_ref().unwrap().next_step(draft_kind, race.game, &mut msg_ctx).await?.kind {
        draft::StepKind::GoFirst | draft::StepKind::BooleanChoice { .. } | draft::StepKind::Done(_) | draft::StepKind::DoneRsl { .. } => match race.draft.as_mut().unwrap().apply(draft_kind, race.game, &mut msg_ctx, draft::Action::Pick { setting: format!("@placeholder"), value: format!("@placeholder") }).await? {
            Ok(_) => unreachable!(),
            Err(error_msg) => {
                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content(error_msg)
                )).await?;
                msg_ctx.into_transaction().rollback().await?;
                return Ok(())
            }
        },
        draft::StepKind::Ban { available_settings, rsl, .. } => {
            let response_content = if_chain! {
                if let French = event.language;
                if let Some(action) = match action {
                    "ban" => Some("ban"),
                    "draft" => Some("pick"),
                    _ => None,
                };
                then {
                    format!("Sélectionnez le setting à {action} :")
                } else {
                    format!("Select the setting to {}:", if rsl { "block" } else { action })
                }
            };
            let mut response_msg = CreateInteractionResponseMessage::new()
                .ephemeral(true)
                .content(response_content);
            if available_settings.num_settings() <= BUTTONS_PER_PAGE {
                for draft::BanSetting { name, display, .. } in available_settings.all() {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_setting_{name}")).label(display));
                }
            } else {
                if let Some((page_name, _)) = page.checked_sub(1).and_then(|prev_page| available_settings.page(prev_page)) {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_page_{}", page - 1)).label(page_name).style(ButtonStyle::Secondary));
                }
                for draft::BanSetting { name, display, .. } in available_settings.page(page).unwrap().1 {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_setting_{name}")).label(*display));
                }
                if let Some((page_name, _)) = page.checked_add(1).and_then(|next_page| available_settings.page(next_page)) {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_page_{}", page + 1)).label(page_name).style(ButtonStyle::Secondary));
                }
            }
            interaction.create_response(ctx, CreateInteractionResponse::Message(response_msg)).await?;
        }
        draft::StepKind::Pick { available_choices, rsl, .. } => {
            let response_content = if_chain! {
                if let French = event.language;
                if let Some(action) = match action {
                    "ban" => Some("ban"),
                    "draft" => Some("pick"),
                    _ => None,
                };
                then {
                    format!("Sélectionnez le setting à {action} :")
                } else {
                    format!("Select the setting to {}:", if rsl { "ban" } else { action })
                }
            };
            let mut response_msg = CreateInteractionResponseMessage::new()
                .ephemeral(true)
                .content(response_content);
            if available_choices.num_settings() <= BUTTONS_PER_PAGE {
                for draft::DraftSetting { name, display, .. } in available_choices.all() {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_setting_{name}")).label(display));
                }
            } else {
                if let Some((page_name, _)) = page.checked_sub(1).and_then(|prev_page| available_choices.page(prev_page)) {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_page_{}", page - 1)).label(page_name).style(ButtonStyle::Secondary));
                }
                for draft::DraftSetting { name, display, .. } in available_choices.page(page).unwrap().1 {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_setting_{name}")).label(*display));
                }
                if let Some((page_name, _)) = page.checked_add(1).and_then(|next_page| available_choices.page(next_page)) {
                    response_msg = response_msg.button(CreateButton::new(format!("{action}_page_{}", page + 1)).label(page_name).style(ButtonStyle::Secondary));
                }
            }
            interaction.create_response(ctx, CreateInteractionResponse::Message(response_msg)).await?;
        }
    }
    msg_ctx.into_transaction().commit().await?;
    Ok(())
}

async fn draft_action(ctx: &DiscordCtx, interaction: &impl GenericInteraction, action: draft::Action) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some((event, mut race, draft_kind, mut msg_ctx)) = check_draft_permissions(ctx, interaction).await? else { return Ok(()) };
    match race.draft.as_mut().unwrap().apply(draft_kind, race.game, &mut msg_ctx, action).await? {
        Ok(apply_response) => {
            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                .ephemeral(false)
                .content(apply_response)
            )).await?;
            if let Some(draft_kind) = event.draft_kind() {
                interaction.channel_id()
                    .say(ctx, race.draft.as_ref().unwrap().next_step(draft_kind, race.game, &mut msg_ctx).await?.message).await?;
            }
            let mut transaction = msg_ctx.into_transaction();
            sqlx::query!("UPDATE races SET draft_state = $1 WHERE id = $2", Json(race.draft.as_ref().unwrap()) as _, race.id as _).execute(&mut *transaction).await?;
            transaction.commit().await?;
        }
        Err(error_msg) => {
            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                .ephemeral(true)
                .content(error_msg)
            )).await?;
            msg_ctx.into_transaction().rollback().await?;
        }
    }
    Ok(())
}

fn parse_timestamp(timestamp: &str) -> Option<DateTime<Utc>> {
    regex_captures!("^<t:(-?[0-9]+)(?::[tTdDfFR])?>$", timestamp)
        .and_then(|(_, timestamp)| timestamp.parse().ok())
        .and_then(|timestamp| Utc.timestamp_opt(timestamp, 0).single())
}

pub(crate) fn configure_builder(discord_builder: serenity_utils::Builder, global_state: Arc<GlobalState>, db_pool: PgPool, http_client: reqwest::Client, config: Config, new_room_lock: Arc<Mutex<()>>, clean_shutdown: Arc<Mutex<CleanShutdown>>, shutdown: rocket::Shutdown) -> serenity_utils::Builder {
    discord_builder
        .error_notifier(ErrorNotifier::User(ADMIN_USER)) //TODO also print to stderr and/or report to night
        .data::<GlobalState>(global_state)
        .data::<DbPool>(db_pool)
        .data::<HttpClient>(http_client)
        .data::<RacetimeHost>(racetime::HostInfo {
            hostname: Cow::Borrowed(racetime_host()),
            ..racetime::HostInfo::default()
        })
        .data::<ConfigRaceTime>(config.racetime_bot.clone())
        .data::<StartggToken>(config.startgg)
        .data::<ChallongeApiKey>(config.challonge_api_key.clone())
        .data::<NewRoomLock>(new_room_lock)
        .data::<CleanShutdown>(clean_shutdown)
        .on_guild_create(false, |ctx, guild, _| Box::pin(async move {
            let mut transaction = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?;
            let guild_event_rows = sqlx::query!(r#"SELECT series AS "series: Series", event FROM events WHERE discord_guild = $1 AND (end_time IS NULL OR end_time > NOW())"#, PgSnowflake(guild.id) as _).fetch_all(&mut *transaction).await?;
            let mut guild_events = Vec::with_capacity(guild_event_rows.len());
            for row in guild_event_rows {
                guild_events.push(event::Data::new(&mut transaction, row.series, row.event).await?.expect("just received from database"));
            }
            let mut commands = Vec::default();
            let mut draft_kind = None;
            for event in &guild_events {
                if let Some(new_kind) = event.draft_kind() {
                    if draft_kind.is_some_and(|prev_kind| prev_kind != new_kind) {
                        #[derive(Debug, thiserror::Error)]
                        #[error("multiple conflicting draft kinds in the same Discord guild")]
                        struct DraftKindsError;

                        ctx.data.write().await.entry::<CommandIds>().or_default().insert(guild.id, None);
                        return Err(Box::new(DraftKindsError) as Box<dyn std::error::Error + Send + Sync>)
                    }
                    draft_kind = Some(new_kind);
                }
            }
            let ban = draft_kind.map(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 => CreateCommand::new("ban")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Bans a mode from being played in this match."),
                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => CreateCommand::new("ban")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Locks a setting for this race to its default value."),
                    draft::Kind::RslS7 => CreateCommand::new("block")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Blocks the weights of a setting from being changed."),
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("ban")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Verrouille un setting à sa valeur par défaut.")
                        .description_localized("en-GB", "Locks a setting for this race to its default value.")
                        .description_localized("en-US", "Locks a setting for this race to its default value."),
                });
                idx
            });
            let delete_after = {
                let idx = commands.len();
                commands.push(CreateCommand::new("delete-after")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Deletes games of the match that are not required.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "game",
                        "The last game number within the match that should be kept.",
                    )
                        .min_int_value(1)
                        .max_int_value(255)
                        .required(true)
                    )
                );
                idx
            };
            let draft = draft_kind.and_then(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 => CreateCommand::new("draft")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Picks a mode for a game (same as /pick)."),
                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => CreateCommand::new("draft")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Chooses a setting for this race (same as /pick)."),
                    draft::Kind::RslS7 => return None, // command is called /ban, no alias necessary
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("draft")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Choisit un setting pour la race (identique à /pick).")
                        .description_localized("en-GB", "Chooses a setting for this race (same as /pick).")
                        .description_localized("en-US", "Chooses a setting for this race (same as /pick)."),
                });
                Some(idx)
            });
            let first = draft_kind.and_then(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 => return None, // AlttprDe9 doesn't have a first/second choice, turn order is fixed
                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => CreateCommand::new("first")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Go first in the settings draft."),
                    draft::Kind::RslS7 => CreateCommand::new("first")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Go first in the weights draft.")
                        .add_option(CreateCommandOption::new(
                            CommandOptionType::Boolean,
                            "lite",
                            "Use RSL-Lite weights",
                        )
                            .required(false)
                        ),
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("first")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Partir premier dans la phase de pick&ban.")
                        .description_localized("en-GB", "Go first in the settings draft.")
                        .description_localized("en-US", "Go first in the settings draft.")
                        .add_option(CreateCommandOption::new(
                            CommandOptionType::Integer,
                            "mq",
                            "Nombre de donjons MQ",
                        )
                            .description_localized("en-GB", "Number of MQ dungeons")
                            .description_localized("en-US", "Number of MQ dungeons")
                            .min_int_value(0)
                            .max_int_value(12)
                            .required(false)
                        ),
                });
                Some(idx)
            });
            let no = draft_kind.and_then(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 | draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 | draft::Kind::RslS7 => return None,
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("no")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Répond à la négative dans une question fermée.")
                        .description_localized("en-GB", "Answers no to a yes/no question in the settings draft.")
                        .description_localized("en-US", "Answers no to a yes/no question in the settings draft."),
                });
                Some(idx)
            });
            let pick = draft_kind.map(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 => CreateCommand::new("pick")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Picks a mode for a game in the match."),
                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => CreateCommand::new("pick")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Chooses a setting for this race."),
                    draft::Kind::RslS7 => CreateCommand::new("ban")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Sets a weight of a setting to 0."),
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("pick")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Choisit un setting pour la race.")
                        .description_localized("en-GB", "Chooses a setting for this race.")
                        .description_localized("en-US", "Chooses a setting for this race."),
                });
                idx
            });
            let post_status = {
                let idx = commands.len();
                commands.push(CreateCommand::new("post-status")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Posts this race's status to the thread, pinging the team whose turn it is in the settings draft.")
                );
                idx
            };
            let pronoun_roles = {
                let idx = commands.len();
                commands.push(CreateCommand::new("pronoun-roles")
                    .kind(CommandType::ChatInput)
                    .default_member_permissions(Permissions::ADMINISTRATOR)
                    .add_context(InteractionContext::Guild)
                    .description("Creates gender pronoun roles and posts a message here that allows members to self-assign them.")
                );
                idx
            };
            let racing_role = {
                let idx = commands.len();
                commands.push(CreateCommand::new("racing-role")
                    .kind(CommandType::ChatInput)
                    .default_member_permissions(Permissions::ADMINISTRATOR)
                    .add_context(InteractionContext::Guild)
                    .description("Creates a racing role and posts a message here that allows members to self-assign it.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Channel,
                        "race-planning-channel",
                        "Will be linked to from the description message.",
                    )
                        .required(true)
                        .channel_types(vec![ChannelType::Text, ChannelType::News])
                    )
                );
                idx
            };
            let reset_race = {
                let idx = commands.len();
                let mut command = CreateCommand::new("reset-race")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Deletes selected data from a race.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "game",
                        "The game number within the match.",
                    )
                        .min_int_value(1)
                        .max_int_value(255)
                        .required(false)
                    );
                if draft_kind.is_some() {
                    command = command.add_option(CreateCommandOption::new(
                        CommandOptionType::Boolean,
                        "draft",
                        "Reset the settings draft.",
                    )
                        .required(false)
                    );
                }
                command = command.add_option(CreateCommandOption::new(
                    CommandOptionType::Boolean,
                    "schedule",
                    "Reset the schedule, race room, and seed.",
                )
                    .required(false)
                );
                commands.push(command);
                idx
            };
            let schedule = {
                let idx = commands.len();
                commands.push(CreateCommand::new("schedule")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Submits a starting time for this race.")
                    .description_localized("fr", "Planifie une date/heure pour une race.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "start",
                        "The starting time as a Discord timestamp",
                    )
                        .description_localized("fr", "La date de début comme timestamp de Discord")
                        .required(true)
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "game",
                        "The game number within the match. Defaults to the next upcoming game.",
                    )
                        .min_int_value(1)
                        .max_int_value(255)
                        .required(false)
                    )
                );
                idx
            };
            let schedule_async = {
                let idx = commands.len();
                commands.push(CreateCommand::new("schedule-async")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Submits a starting time for your half of this race.")
                    .description_localized("fr", "Planifie votre partie de l'async.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "start",
                        "The starting time as a Discord timestamp",
                    )
                        .description_localized("fr", "La date de début comme timestamp de Discord")
                        .required(true)
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "game",
                        "The game number within the match. Defaults to the next upcoming game.",
                    )
                        .min_int_value(1)
                        .max_int_value(255)
                        .required(false)
                    )
                );
                idx
            };
            let result_async = {
                let idx = commands.len();
                commands.push(CreateCommand::new("result-async")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Records finish time for async race part. Only time needed in async thread.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "time",
                        "Finish time in format hh:mm:ss",
                    )
                        .required(true)
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "race_id",
                        "The ID of the race (optional when used in async thread)",
                    )
                        .required(false)
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "async_part",
                        "The async part number (1, 2, or 3) (optional when used in async thread)",
                    )
                        .min_int_value(1)
                        .max_int_value(3)
                        .required(false)
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "link",
                        "Link to the recording/VoD for this async part (optional)",
                    )
                        .required(false)
                    )
                );
                idx
            };
            let forfeit_async = {
                let idx = commands.len();
                commands.push(CreateCommand::new("forfeit-async")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Marks a player as forfeiting in an async race part. Only for organizers.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String,
                        "race_id",
                        "The ID of the race (optional when used in async thread)",
                    )
                        .required(false)
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "async_part",
                        "The async part number (1, 2, or 3) (optional when used in async thread)",
                    )
                        .min_int_value(1)
                        .max_int_value(3)
                        .required(false)
                    )
                );
                idx
            };
            let schedule_remove = {
                let idx = commands.len();
                commands.push(CreateCommand::new("schedule-remove")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Removes the starting time(s) for this race from the schedule.")
                    .description_localized("fr", "Supprime le(s) date(s) de début sur le document des races planifiées.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "game",
                        "The game number within the match. Defaults to the next upcoming game.",
                    )
                        .min_int_value(1)
                        .max_int_value(255)
                        .required(false)
                    )
                );
                idx
            };
            let second = draft_kind.and_then(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 => return None, // AlttprDe9 doesn't have a first/second choice, turn order is fixed
                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => CreateCommand::new("second")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Go second in the settings draft."),
                    draft::Kind::RslS7 => CreateCommand::new("second")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Go second in the weights draft.")
                        .add_option(CreateCommandOption::new(
                            CommandOptionType::Boolean,
                            "lite",
                            "Use RSL-Lite weights",
                        )
                            .required(false)
                        ),
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("second")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Partir second dans la phase de pick&ban.")
                        .description_localized("en-GB", "Go second in the settings draft.")
                        .description_localized("en-US", "Go second in the settings draft.")
                        .add_option(CreateCommandOption::new(
                            CommandOptionType::Integer,
                            "mq",
                            "Nombre de donjons MQ",
                        )
                            .description_localized("en-GB", "Number of MQ dungeons")
                            .description_localized("en-US", "Number of MQ dungeons")
                            .min_int_value(0)
                            .max_int_value(12)
                            .required(false)
                        ),
                });
                Some(idx)
            });
            let skip = draft_kind.and_then(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 => return None, // AlttprDe9 doesn't allow skipping
                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => CreateCommand::new("skip")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Skips your current turn of the settings draft."),
                    draft::Kind::RslS7 => CreateCommand::new("skip")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Skips your current turn of the weights draft."),
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("skip")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Skip le dernier pick du draft.")
                        .description_localized("en-GB", "Skips the final pick of the settings draft.")
                        .description_localized("en-US", "Skips the final pick of the settings draft."),
                });
                Some(idx)
            });
            let status = {
                let idx = commands.len();
                commands.push(CreateCommand::new("status")
                    .kind(CommandType::ChatInput)
                    .add_context(InteractionContext::Guild)
                    .description("Shows you this race's current scheduling and settings draft status.")
                    .description_localized("fr", "Montre l'avancement de la planification de votre race, avec les détails.")
                );
                idx
            };
            let watch_roles = {
                let idx = commands.len();
                commands.push(CreateCommand::new("watch-roles")
                    .kind(CommandType::ChatInput)
                    .default_member_permissions(Permissions::ADMINISTRATOR)
                    .add_context(InteractionContext::Guild)
                    .description("Creates watch notification roles and posts a message here that allows members to self-assign them.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Channel,
                        "watch-party-channel",
                        "Will be linked to from the description message.",
                    )
                        .required(true)
                        .channel_types(vec![ChannelType::Voice, ChannelType::Stage])
                    )
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Channel,
                        "race-rooms-channel",
                        "Will be linked to from the description message.",
                    )
                        .required(true)
                        .channel_types(vec![ChannelType::Text, ChannelType::News])
                    )
                );
                idx
            };
            let yes = draft_kind.and_then(|draft_kind| {
                let idx = commands.len();
                commands.push(match draft_kind {
                    draft::Kind::AlttprDe9 | draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 | draft::Kind::RslS7 => return None,
                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => CreateCommand::new("yes")
                        .kind(CommandType::ChatInput)
                        .add_context(InteractionContext::Guild)
                        .description("Répond à l'affirmative dans une question fermée.")
                        .description_localized("en-GB", "Answers yes to a yes/no question in the settings draft.")
                        .description_localized("en-US", "Answers yes to a yes/no question in the settings draft."),
                });
                Some(idx)
            });
            let commands = guild.set_commands(ctx, commands).await?;
            ctx.data.write().await.entry::<CommandIds>().or_default().insert(guild.id, Some(CommandIds {
                ban: ban.map(|idx| commands[idx].id),
                delete_after: commands[delete_after].id,
                draft: draft.map(|idx| commands[idx].id),
                first: first.map(|idx| commands[idx].id),
                no: no.map(|idx| commands[idx].id),
                pick: pick.map(|idx| commands[idx].id),
                post_status: commands[post_status].id,
                pronoun_roles: commands[pronoun_roles].id,
                racing_role: commands[racing_role].id,
                reset_race: commands[reset_race].id,
                schedule: commands[schedule].id,
                schedule_async: commands[schedule_async].id,
                result_async: commands[result_async].id,
                forfeit_async: commands[forfeit_async].id,
                schedule_remove: commands[schedule_remove].id,
                second: second.map(|idx| commands[idx].id),
                skip: skip.map(|idx| commands[idx].id),
                status: commands[status].id,
                watch_roles: commands[watch_roles].id,
                yes: yes.map(|idx| commands[idx].id),
            }));
            transaction.commit().await?;
            Ok(())
        }))
        .on_interaction_create(|ctx, interaction| Box::pin(async move {
            match interaction {
                Interaction::Command(interaction) => {
                    let guild_id = interaction.guild_id.expect("Discord slash command called outside of a guild");
                    if let Some(&Some(command_ids)) = ctx.data.read().await.get::<CommandIds>().and_then(|command_ids| command_ids.get(&guild_id)) {
                        if Some(interaction.data.id) == command_ids.ban {
                            send_draft_settings_page(ctx, interaction, "ban", 0).await?;
                        } else if interaction.data.id == command_ids.delete_after {
                            let Some(parent_channel) = interaction.channel.as_ref().and_then(|thread| thread.parent_id) else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("Sorry, this command can only be used inside threads and forum posts.")
                                )).await?;
                                return Ok(())
                            };
                            let mut transaction = ctx.data.read().await.get::<DbPool>().as_ref().expect("database connection pool missing from Discord context").begin().await?;
                            if let Some(event_row) = sqlx::query!(r#"SELECT series AS "series: Series", event FROM events WHERE discord_scheduling_channel = $1 AND end_time IS NULL"#, PgSnowflake(parent_channel) as _).fetch_optional(&mut *transaction).await? {
                                let event = event::Data::new(&mut transaction, event_row.series, event_row.event).await?.expect("just received from database");
                                match event.match_source() {
                                    MatchSource::Manual | MatchSource::Challonge { .. } => {}
                                    MatchSource::StartGG(_) => {} //TODO automate
                                    MatchSource::League => {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("Sorry, this command is not available for events sourcing their match schedule from league.ootrandomizer.com")
                                        )).await?;
                                        return Ok(())
                                    }
                                };
                                if !event.organizers(&mut transaction).await?.into_iter().any(|organizer| organizer.discord.is_some_and(|discord| discord.id == interaction.user.id)) {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Sorry, only event organizers can use this command.")
                                    )).await?;
                                    return Ok(())
                                }
                                let after_game = match interaction.data.options[0].value {
                                    CommandDataOptionValue::Integer(game) => i16::try_from(game).expect("game number out of range"),
                                    _ => panic!("unexpected slash command option type"),
                                };
                                let races_deleted = sqlx::query_scalar!(r#"DELETE FROM races WHERE scheduling_thread = $1 AND NOT ignored AND GAME > $2"#, PgSnowflake(interaction.channel_id) as _, after_game).execute(&mut *transaction).await?
                                    .rows_affected();
                                transaction.commit().await?;
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content(if races_deleted == 0 {
                                        format!("Sorry, looks like that didn't delete any races.")
                                    } else {
                                        format!("{races_deleted} race{} deleted from the schedule.", if races_deleted == 1 { "" } else { "s" })
                                    })
                                )).await?;
                            } else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("Sorry, this channel is not configured as the scheduling channel for any ongoing Hyrule Town Hall events.")
                                )).await?;
                            }
                        } else if Some(interaction.data.id) == command_ids.draft || Some(interaction.data.id) == command_ids.pick {
                            send_draft_settings_page(ctx, interaction, "draft", 0).await?;
                        } else if Some(interaction.data.id) == command_ids.first {
                              if let Some((_, mut race, draft_kind, msg_ctx)) = check_draft_permissions(ctx, interaction).await? {
                                match draft_kind {
                                    draft::Kind::RslS7 => {
                                        let settings = &mut race.draft.as_mut().unwrap().settings;
                                        let lite = interaction.data.options.get(0).map(|option| match option.value {
                                            CommandDataOptionValue::Boolean(lite) => lite,
                                            _ => panic!("unexpected slash command option type"),
                                        });
                                        if settings.get("lite_ok").map(|lite_ok| &**lite_ok).unwrap_or("no") == "ok" {
                                            let mut transaction = msg_ctx.into_transaction();
                                            if let Some(lite) = lite {
                                                settings.insert(Cow::Borrowed("preset"), Cow::Borrowed(if lite { "lite" } else { "league" }));
                                                sqlx::query!("UPDATE races SET draft_state = $1 WHERE id = $2", Json(race.draft.as_ref().unwrap()) as _, race.id as _).execute(&mut *transaction).await?;
                                                transaction.commit().await?;
                                            } else {
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content(MessageBuilder::default().push("Sorry, please specify the ").push_mono("lite").push(" parameter.").build())
                                                )).await?;
                                                transaction.rollback().await?;
                                                return Ok(())
                                            }
                                        } else {
                                            if lite.is_some_and(identity) {
                                                //TODO different error messages depending on which player(s) didn't opt into RSL-Lite
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("Sorry, either you or your opponent didn't opt into RSL-Lite.")
                                                )).await?;
                                                return Ok(())
                                            }
                                        }
                                    }
                                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => {
                                        let settings = &mut race.draft.as_mut().unwrap().settings;
                                        let mq = interaction.data.options.get(0).map(|option| match option.value {
                                            CommandDataOptionValue::Integer(mq) => u8::try_from(mq).expect("MQ count out of range"),
                                            _ => panic!("unexpected slash command option type"),
                                        });
                                        if settings.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" {
                                            let mut transaction = msg_ctx.into_transaction();
                                            if let Some(mq) = mq {
                                                settings.insert(Cow::Borrowed("mq_dungeons_count"), Cow::Owned(mq.to_string()));
                                                sqlx::query!("UPDATE races SET draft_state = $1 WHERE id = $2", Json(race.draft.as_ref().unwrap()) as _, race.id as _).execute(&mut *transaction).await?;
                                                transaction.commit().await?;
                                            } else {
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("Désolé, veuillez entrer le nombre de donjons MQ d'abord.")
                                                )).await?;
                                                transaction.rollback().await?;
                                                return Ok(())
                                            }
                                        } else {
                                            if mq.is_some_and(|mq| mq != 0) {
                                                //TODO different error messages depending on which player(s) didn't opt into MQ
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("Désolé, mais l'un d'entre vous n'a pas choisi les donjons MQ.")
                                                )).await?;
                                                return Ok(())
                                            }
                                        }
                                    }
                                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 | draft::Kind::AlttprDe9 => {}
                                }
                                draft_action(ctx, interaction, draft::Action::GoFirst(true)).await?;
                            }
                        } else if Some(interaction.data.id) == command_ids.no {
                            draft_action(ctx, interaction, draft::Action::BooleanChoice(false)).await?;
                        } else if interaction.data.id == command_ids.post_status {
                            if let Some((mut transaction, race, team)) = check_scheduling_thread_permissions(ctx, interaction, None, true, None, false).await? {
                                let event = race.event(&mut transaction).await?;
                                if event.organizers(&mut transaction).await?.into_iter().any(|organizer| organizer.discord.is_some_and(|discord| discord.id == interaction.user.id)) {
                                    if let Some(draft_kind) = event.draft_kind() {
                                        if let Some(ref draft) = race.draft {
                                            let mut msg_ctx = draft::MessageContext::Discord {
                                                teams: race.teams().cloned().collect(),
                                                team: team.unwrap_or_else(Team::dummy),
                                                transaction, guild_id, command_ids,
                                            };
                                            let message_content = MessageBuilder::default()
                                                //TODO include scheduling status, both for regular races and for asyncs
                                                .push(draft.next_step(draft_kind, race.game, &mut msg_ctx).await?.message)
                                                .build();
                                            interaction.channel.as_ref().expect("received draft action outside channel")
                                                .id
                                                .say(ctx, message_content).await?;
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content("done")
                                            )).await?;
                                            msg_ctx.into_transaction().commit().await?;
                                        } else {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content("Sorry, this race's settings draft has not been initialized. Please contact a tournament organizer to fix this.")
                                            )).await?;
                                            transaction.rollback().await?;
                                        }
                                    } else {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("Sorry, this command is currently only available for events with settings drafts.") //TODO
                                        )).await?;
                                        transaction.rollback().await?;
                                    }
                                } else {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content(if let French = event.language {
                                            "Désolé, seuls les organisateurs du tournoi peuvent utiliser cette commande."
                                        } else {
                                            "Sorry, only organizers can use this command."
                                        })
                                    )).await?;
                                    transaction.rollback().await?;
                                }
                            }
                        } else if interaction.data.id == command_ids.pronoun_roles {
                            guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(false)
                                .name("he/him")
                                .permissions(Permissions::empty())
                            ).await?;
                            guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(false)
                                .name("she/her")
                                .permissions(Permissions::empty())
                            ).await?;
                            guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(false)
                                .name("they/them")
                                .permissions(Permissions::empty())
                            ).await?;
                            guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(false)
                                .name("other pronouns")
                                .permissions(Permissions::empty())
                            ).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(false)
                                .content("Click a button below to get a gender pronoun role. Click again to remove it. Multiple selections allowed.")
                                .button(CreateButton::new("pronouns_he").label("he/him"))
                                .button(CreateButton::new("pronouns_she").label("she/her"))
                                .button(CreateButton::new("pronouns_they").label("they/them"))
                                .button(CreateButton::new("pronouns_other").label("other"))
                            )).await?;
                        } else if interaction.data.id == command_ids.racing_role {
                            let race_planning_channel = match interaction.data.options[0].value {
                                CommandDataOptionValue::Channel(channel) => channel,
                                _ => panic!("unexpected slash command option type"),
                            };
                            guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(true)
                                .name("racing")
                                .permissions(Permissions::empty())
                            ).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(false)
                                .content(MessageBuilder::default()
                                    .push("Click the button below to get notified when a race is being planned. Click again to remove it. Ping this role in ")
                                    .mention(&race_planning_channel)
                                    .push(" when planning a race.")
                                    .build()
                                )
                                .button(CreateButton::new("racingrole").label("racing"))
                            )).await?;
                        } else if interaction.data.id == command_ids.reset_race {
                            let Some(_parent_channel) = interaction.channel.as_ref().and_then(|thread| thread.parent_id) else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("Sorry, this command can only be used inside threads and forum posts.")
                                )).await?;
                                return Ok(())
                            };
                            let mut transaction = ctx.data.read().await.get::<DbPool>().as_ref().expect("database connection pool missing from Discord context").begin().await?;

                            // Parse options first
                            let mut game = None;
                            let mut reset_draft = false;
                            let mut reset_schedule = false;
                            for option in &interaction.data.options {
                                match &*option.name {
                                    "draft" => match option.value {
                                        CommandDataOptionValue::Boolean(value) => reset_draft = value,
                                        _ => panic!("unexpected slash command option type"),
                                    },
                                    "game" => match option.value {
                                        CommandDataOptionValue::Integer(value) => game = Some(i16::try_from(value).expect("game number out of range")),
                                        _ => panic!("unexpected slash command option type"),
                                    },
                                    "schedule" => match option.value {
                                        CommandDataOptionValue::Boolean(value) => reset_schedule = value,
                                        _ => panic!("unexpected slash command option type"),
                                    },
                                    name => panic!("unexpected option for /reset-race: {name}"),
                                }
                            }
                            let Some(aspects_reset) = NEVec::try_from_vec(reset_draft.then_some("draft").into_iter().chain(reset_schedule.then_some("schedule")).collect_vec()) else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("Please specify at least one thing to delete using the slash command's options.")
                                )).await?;
                                return Ok(())
                            };

                            let http_client = {
                                let data = ctx.data.read().await;
                                data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone()
                            };
                            let mut races = Race::for_scheduling_channel(&mut transaction, &http_client, interaction.channel_id(), game, true).await?;

                            if let Some(first_race) = races.first() {
                                let event = first_race.event(&mut transaction).await?;
                                if !event.organizers(&mut transaction).await?.into_iter().any(|organizer| organizer.discord.is_some_and(|discord| discord.id == interaction.user.id)) {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Sorry, only event organizers can use this command.")
                                    )).await?;
                                    return Ok(())
                                }

                                // For AlttprDe9 events, allow resetting draft across all games without specifying game number
                                let is_alttprde9_draft_reset = game.is_none() && reset_draft && event.draft_kind() == Some(draft::Kind::AlttprDe9);

                                if races.len() > 1 && is_alttprde9_draft_reset {
                                    // Reset draft for all races in AlttprDe9 BO3
                                    let new_draft = if_chain! {
                                        if let Some(draft_kind) = event.draft_kind();
                                        if let Some(draft) = races[0].draft.clone();
                                        if let Entrants::Two(entrants) = &races[0].entrants;
                                        if let Ok(low_seed) = entrants.iter()
                                            .filter_map(as_variant!(Entrant::MidosHouseTeam))
                                            .filter(|team| team.id != draft.high_seed)
                                            .exactly_one();
                                        then {
                                            Some(Draft::for_next_game(&mut transaction, draft_kind, draft.high_seed, low_seed.id).await?)
                                        } else {
                                            None
                                        }
                                    };

                                    if new_draft.is_some() {
                                        // Apply the reset draft to all races
                                        for race in &mut races {
                                            race.draft = new_draft.clone();
                                            race.save(&mut transaction).await?;
                                        }

                                        transaction.commit().await?;
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(false)
                                            .content("Draft has been reset.")
                                        )).await?;
                                    } else {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("Sorry, unable to reset draft for this race.")
                                        )).await?;
                                        transaction.rollback().await?;
                                    }
                                } else {
                                    match races.into_iter().at_most_one() {
                                        Ok(None) => {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content(if game.is_some() {
                                                    "Sorry, there don't seem to be any races with that game number associated with this thread."
                                                } else {
                                                    "Sorry, this thread is not associated with any races."
                                                })
                                            )).await?;
                                            transaction.rollback().await?;
                                        }
                                        Ok(Some(race)) => {
                                        let race = Race {
                                            schedule: if reset_schedule { RaceSchedule::Unscheduled } else { race.schedule },
                                            schedule_updated_at: if reset_schedule { Some(Utc::now()) } else { race.schedule_updated_at },
                                            fpa_invoked: if reset_schedule { false } else { race.fpa_invoked },
                                            breaks_used: if reset_schedule { false } else { race.breaks_used },
                                            draft: if reset_draft {
                                                if_chain! {
                                                    if let Some(draft_kind) = event.draft_kind();
                                                    if let Some(draft) = race.draft;
                                                    if let Entrants::Two(entrants) = &race.entrants;
                                                    if let Ok(low_seed) = entrants.iter()
                                                        .filter_map(as_variant!(Entrant::MidosHouseTeam))
                                                        .filter(|team| team.id != draft.high_seed)
                                                        .exactly_one();
                                                    then {
                                                        Some(Draft::for_next_game(&mut transaction, draft_kind, draft.high_seed, low_seed.id).await?)
                                                    } else {
                                                        None
                                                    }
                                                }
                                            } else {
                                                race.draft
                                            },
                                            seed: if reset_schedule { seed::Data::default() } else { race.seed },
                                            notified: race.notified && !reset_schedule,
                                            async_notified_1: race.async_notified_1 && !reset_schedule,
                                            async_notified_2: race.async_notified_2 && !reset_schedule,
                                            async_notified_3: race.async_notified_3 && !reset_schedule,
                                            // explicitly listing remaining fields here instead of using `..race` so if the fields change they're kept/reset correctly
                                            id: race.id,
                                            series: race.series,
                                            event: race.event,
                                            source: race.source,
                                            entrants: race.entrants,
                                            phase: race.phase,
                                            round: race.round,
                                            game: race.game,
                                            scheduling_thread: race.scheduling_thread,
                                            video_urls: race.video_urls,
                                            restreamers: race.restreamers,
                                            last_edited_by: race.last_edited_by,
                                            last_edited_at: race.last_edited_at,
                                            ignored: race.ignored,
                                            schedule_locked: race.schedule_locked,
                                            discord_scheduled_event_id: race.discord_scheduled_event_id,
                                            volunteer_request_sent: race.volunteer_request_sent,
                                            volunteer_request_message_id: race.volunteer_request_message_id,
                                        };
                                        race.save(&mut transaction).await?;

                                        // Reset async fields in database when resetting schedule
                                        if reset_schedule {
                                            sqlx::query!(
                                                "UPDATE races SET async_thread1 = NULL, async_thread2 = NULL, async_thread3 = NULL, async_seed1 = FALSE, async_seed2 = FALSE, async_seed3 = FALSE, async_ready1 = FALSE, async_ready2 = FALSE, async_ready3 = FALSE WHERE id = $1",
                                                race.id as _
                                            ).execute(&mut *transaction).await?;

                                            // Delete async_times records when resetting schedule
                                            sqlx::query!("DELETE FROM async_times WHERE race_id = $1", race.id as _)
                                                .execute(&mut *transaction).await?;
                                        }

                                        transaction.commit().await?;
                                        let verb = if aspects_reset.len() == NonZero::<usize>::MIN { " has" } else { " have" };
                                        let response_content = MessageBuilder::default()
                                            .push(English.join_str(aspects_reset))
                                            .push(verb)
                                            .push(" been reset.")
                                            .build();
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(false)
                                            .content(response_content)
                                        )).await?;
                                    }
                                        Err(_) => {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content("Sorry, this thread is associated with multiple races. Please specify the game number.")
                                            )).await?;
                                            transaction.rollback().await?;
                                        }
                                    }
                                }
                            } else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("Sorry, this thread is not associated with any races.")
                                )).await?;
                                transaction.rollback().await?;
                            }
                        } else if interaction.data.id == command_ids.schedule {
                            let game = interaction.data.options.get(1).map(|option| match option.value {
                                CommandDataOptionValue::Integer(game) => i16::try_from(game).expect("game number out of range"),
                                _ => panic!("unexpected slash command option type"),
                            });

                            // Defer the response immediately to prevent timeout
                            interaction.create_response(ctx, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()
                                .ephemeral(false)
                            )).await?;

                            if let Some((mut transaction, mut race, team)) = check_scheduling_thread_permissions(ctx, interaction, game, false, None, true).await? {
                                let event = race.event(&mut transaction).await?;
                                let is_organizer = event.organizers(&mut transaction).await?.into_iter().any(|organizer| organizer.discord.is_some_and(|discord| discord.id == interaction.user.id));
                                let was_scheduled = !matches!(race.schedule, RaceSchedule::Unscheduled);
                                if let Some(speedgaming_slug) = &event.speedgaming_slug {
                                    let response_content = if was_scheduled {
                                        format!("Please contact a tournament organizer to reschedule this race.")
                                    } else {
                                        MessageBuilder::default()
                                            .push("Please use <https://speedgaming.org/")
                                            .push(speedgaming_slug)
                                            .push("/submit> to schedule races for this event.")
                                            .build()
                                    };
                                    interaction.edit_response(ctx, EditInteractionResponse::new()
                                        .content(response_content)
                                    ).await?;
                                    transaction.rollback().await?;
                                } else if team.is_some() || is_organizer {
                                    let start = match interaction.data.options[0].value {
                                        CommandDataOptionValue::String(ref start) => start,
                                        _ => panic!("unexpected slash command option type"),
                                    };
                                    if let Some(start) = parse_timestamp(start) {
                                        if (start - Utc::now()).to_std().map_or(true, |schedule_notice| schedule_notice > Duration::from_secs(365 * 24 * 60 * 60)) {
                                            interaction.edit_response(ctx, EditInteractionResponse::new()
                                                .content(if let French = event.language {
                                                    format!("Désolé, les races ne peuvent pas être planifiées plus d'un an à l'avance.")
                                                } else {
                                                    format!("Sorry, races cannot be scheduled more than 1 year in advance.")
                                                })
                                            ).await?;
                                            transaction.rollback().await?;
                                        } else if (start - Utc::now()).to_std().map_or(true, |schedule_notice| schedule_notice < event.min_schedule_notice) {
                                            interaction.edit_response(ctx, EditInteractionResponse::new()
                                                .content(if event.min_schedule_notice <= Duration::default() {
                                                    if let French = event.language {
                                                        format!("Désolé mais cette date est dans le passé.")
                                                    } else {
                                                        format!("Sorry, that timestamp is in the past.")
                                                    }
                                                } else {
                                                    if let French = event.language {
                                                        format!("Désolé, les races doivent être planifiées au moins {} en avance.", French.format_duration(event.min_schedule_notice, true))
                                                    } else {
                                                        format!("Sorry, races must be scheduled at least {} in advance.", English.format_duration(event.min_schedule_notice, true))
                                                    }
                                                })
                                            ).await?;
                                            transaction.rollback().await?;
                                        } else {
                                            race.schedule.set_live_start(start);
                                            race.schedule_updated_at = Some(Utc::now());
                                            race.save(&mut transaction).await?;
                                            // Create or update Discord scheduled event
                                            let http_client = {
                                                let data = ctx.data.read().await;
                                                data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone()
                                            };
                                            match crate::discord_scheduled_events::create_discord_scheduled_event(ctx, &mut transaction, &mut race, &event, &http_client).await {
                                                Ok(()) => {
                                                    race.save(&mut transaction).await?; // Save updated discord_scheduled_event_id
                                                }
                                                Err(e) => {
                                                    eprintln!("Failed to create Discord scheduled event for race {}: {}", race.id, e);
                                                }
                                            }
                                            let mut cal_event = cal::Event { kind: cal::EventKind::Normal, race };
                                            if start - Utc::now() < TimeDelta::minutes(30) {
                                                // Commit transaction BEFORE creating room so race handler can find it in database
                                                transaction.commit().await?;

                                                let (http_client, new_room_lock, racetime_host, racetime_config, clean_shutdown) = {
                                                    let data = ctx.data.read().await;
                                                    (
                                                        data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
                                                        data.get::<NewRoomLock>().expect("new room lock missing from Discord context").clone(),
                                                        data.get::<RacetimeHost>().expect("racetime.gg host missing from Discord context").clone(),
                                                        data.get::<ConfigRaceTime>().expect("racetime.gg config missing from Discord context").clone(),
                                                        data.get::<CleanShutdown>().expect("clean shutdown state missing from Discord context").clone(),
                                                    )
                                                };

                                                // Start a new transaction for room creation
                                                let mut transaction = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?;
                                                lock!(new_room_lock = new_room_lock; {
                                                    if let Some((_, msg, _notification_channel)) = racetime_bot::create_room(&mut transaction, ctx, &racetime_host, &racetime_config.client_id, &racetime_config.client_secret, &http_client, clean_shutdown, &cal_event, &event).await? {
                                                        if let Some(channel) = event.discord_race_room_channel {
                                                            if let Err(e) = channel.say(ctx, &msg).await {
                                                                eprintln!("Failed to post race message to Discord race room channel: {}", e);
                                                            }
                                                        }
                                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                                            .content(msg)
                                                        ).await?;
                                                    } else {
                                                        let mut response_content = MessageBuilder::default();
                                                        response_content.push(if let Some(game) = cal_event.race.game { format!("Game {game}") } else { format!("This race") });
                                                        response_content.push(if was_scheduled { " has been rescheduled for " } else { " is now scheduled for " });
                                                        response_content.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                        let response_content = response_content
                                                            .push(". The race room will be opened momentarily.")
                                                            .build();
                                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                                            .content(response_content)
                                                        ).await?;
                                                    }
                                                    transaction.commit().await?;
                                                });

                                                // Update volunteer post if this was a reschedule
                                                if was_scheduled {
                                                    let pool = {
                                                        let data = ctx.data.read().await;
                                                        data.get::<DbPool>().expect("database connection pool missing from Discord context").clone()
                                                    };
                                                    let _ = volunteer_requests::update_volunteer_post_for_race(
                                                        &pool,
                                                        ctx,
                                                        cal_event.race.id,
                                                    ).await;

                                                    // Send reschedule notification DMs to volunteers
                                                    let mut transaction = pool.begin().await?;
                                                    let signups = event::roles::Signup::for_race(&mut transaction, cal_event.race.id).await?;
                                                    let affected_signups: Vec<_> = signups.iter()
                                                        .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Pending | event::roles::VolunteerSignupStatus::Confirmed))
                                                        .collect();

                                                    // Build race description
                                                    let race_description = if cal_event.race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                                                        match (&cal_event.race.round, &cal_event.race.phase) {
                                                            (Some(round), _) => round.clone(),
                                                            (None, Some(phase)) => phase.clone(),
                                                            (None, None) => "Qualifier".to_string(),
                                                        }
                                                    } else {
                                                        match &cal_event.race.entrants {
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
                                                            _ => cal_event.race.round.clone().or_else(|| cal_event.race.phase.clone()).unwrap_or_else(|| "Race".to_string()),
                                                        }
                                                    };

                                                    // Send DM to each affected volunteer
                                                    for signup in affected_signups {
                                                        if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                                                            if let Some(discord) = user.discord {
                                                                let discord_user_id = UserId::new(discord.id.get());

                                                                let mut msg = MessageBuilder::default();
                                                                msg.push("**Race Rescheduled**\n\n");
                                                                msg.push("The race ");
                                                                msg.push_mono(&race_description);
                                                                msg.push(" in ");
                                                                msg.push(&event.display_name);
                                                                msg.push(" has been rescheduled.\n\n");
                                                                msg.push("**New time:** ");
                                                                msg.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                                msg.push("\n\n");
                                                                msg.push("If you're no longer available, you can withdraw your signup using the button below or on the website: <");
                                                                msg.push(&format!("{}/event/{}/{}/races/{}/signups",
                                                                    base_uri(), cal_event.race.series.slug(), cal_event.race.event, u64::from(cal_event.race.id)));
                                                                msg.push(">");

                                                                // Create withdraw button
                                                                let button = CreateButton::new(format!("volunteer_withdraw_{}", u64::from(signup.id)))
                                                                    .label("Withdraw Signup")
                                                                    .style(ButtonStyle::Danger);
                                                                let row = CreateActionRow::Buttons(vec![button]);

                                                                // Send DM
                                                                if let Ok(dm_channel) = discord_user_id.create_dm_channel(ctx).await {
                                                                    if let Err(e) = dm_channel.send_message(ctx,
                                                                        CreateMessage::new()
                                                                            .content(msg.build())
                                                                            .components(vec![row])
                                                                    ).await {
                                                                        eprintln!("Failed to send reschedule notification DM to user {}: {}", signup.user_id, e);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    transaction.commit().await?;
                                                }
                                            } else {
                                                // Create Discord scheduled event for races scheduled > 30 minutes in advance
                                                let http_client = {
                                                    let data = ctx.data.read().await;
                                                    data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone()
                                                };
                                                match crate::discord_scheduled_events::create_discord_scheduled_event(ctx, &mut transaction, &mut cal_event.race, &event, &http_client).await {
                                                    Ok(()) => {
                                                        cal_event.race.save(&mut transaction).await?;
                                                    }
                                                    Err(e) => {
                                                        eprintln!("Failed to create Discord scheduled event for race {}: {}", cal_event.race.id, e);
                                                    }
                                                }
                                                let overlapping_maintenance_windows = if let RaceHandleMode::RaceTime = cal_event.should_create_room(&mut transaction, &event).await? {
                                                    sqlx::query_as!(Range::<DateTime<Utc>>, r#"SELECT start, end_time AS "end" FROM racetime_maintenance WHERE start < $1 AND end_time > $2"#, start + event.series.default_race_duration(), start - TimeDelta::minutes(30)).fetch_all(&mut *transaction).await?
                                                } else {
                                                    Vec::default()
                                                };
                                                transaction.commit().await?;

                                                // Update volunteer post if this was a reschedule
                                                if was_scheduled {
                                                    let pool = {
                                                        let data = ctx.data.read().await;
                                                        data.get::<DbPool>().expect("database connection pool missing from Discord context").clone()
                                                    };
                                                    let _ = volunteer_requests::update_volunteer_post_for_race(
                                                        &pool,
                                                        ctx,
                                                        cal_event.race.id,
                                                    ).await;

                                                    // Send reschedule notification DMs to volunteers
                                                    let mut transaction = pool.begin().await?;
                                                    let signups = event::roles::Signup::for_race(&mut transaction, cal_event.race.id).await?;
                                                    let affected_signups: Vec<_> = signups.iter()
                                                        .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Pending | event::roles::VolunteerSignupStatus::Confirmed))
                                                        .collect();

                                                    // Build race description
                                                    let race_description = if cal_event.race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                                                        match (&cal_event.race.round, &cal_event.race.phase) {
                                                            (Some(round), _) => round.clone(),
                                                            (None, Some(phase)) => phase.clone(),
                                                            (None, None) => "Qualifier".to_string(),
                                                        }
                                                    } else {
                                                        match &cal_event.race.entrants {
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
                                                            _ => cal_event.race.round.clone().or_else(|| cal_event.race.phase.clone()).unwrap_or_else(|| "Race".to_string()),
                                                        }
                                                    };

                                                    // Send DM to each affected volunteer
                                                    for signup in affected_signups {
                                                        if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                                                            if let Some(discord) = user.discord {
                                                                let discord_user_id = UserId::new(discord.id.get());

                                                                let mut msg = MessageBuilder::default();
                                                                msg.push("**Race Rescheduled**\n\n");
                                                                msg.push("The race ");
                                                                msg.push_mono(&race_description);
                                                                msg.push(" in ");
                                                                msg.push(&event.display_name);
                                                                msg.push(" has been rescheduled.\n\n");
                                                                msg.push("**New time:** ");
                                                                msg.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                                msg.push("\n\n");
                                                                msg.push("If you're no longer available, you can withdraw your signup using the button below or on the website: <");
                                                                msg.push(&format!("{}/event/{}/{}/races/{}/signups",
                                                                    base_uri(), cal_event.race.series.slug(), cal_event.race.event, u64::from(cal_event.race.id)));
                                                                msg.push(">");

                                                                // Create withdraw button
                                                                let button = CreateButton::new(format!("volunteer_withdraw_{}", u64::from(signup.id)))
                                                                    .label("Withdraw Signup")
                                                                    .style(ButtonStyle::Danger);
                                                                let row = CreateActionRow::Buttons(vec![button]);

                                                                // Send DM
                                                                if let Ok(dm_channel) = discord_user_id.create_dm_channel(ctx).await {
                                                                    if let Err(e) = dm_channel.send_message(ctx,
                                                                        CreateMessage::new()
                                                                            .content(msg.build())
                                                                            .components(vec![row])
                                                                    ).await {
                                                                        eprintln!("Failed to send reschedule notification DM to user {}: {}", signup.user_id, e);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    transaction.commit().await?;
                                                }

                                                let response_content = if_chain! {
                                                    if let French = event.language;
                                                    if cal_event.race.game.is_none();
                                                    if overlapping_maintenance_windows.is_empty();
                                                    then {
                                                        MessageBuilder::default()
                                                            .push("Votre race a été planifiée pour le ")
                                                            .push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime)
                                                            .push('.')
                                                            .build()
                                                    } else {
                                                        let mut response_content = MessageBuilder::default();
                                                        response_content.push(if let Some(game) = cal_event.race.game { format!("Game {game}") } else { format!("This race") });
                                                        response_content.push(if was_scheduled { " has been rescheduled for " } else { " is now scheduled for " });
                                                        response_content.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                        response_content.push('.');
                                                        for window in overlapping_maintenance_windows {
                                                            response_content.push_line("");
                                                            response_content.push_bold("Warning:");
                                                            response_content.push(" this race may overlap with racetime.gg maintenance planned for ");
                                                            response_content.push_timestamp(window.start, serenity_utils::message::TimestampStyle::ShortDateTime);
                                                            response_content.push(" until ");
                                                            response_content.push_timestamp(window.end, serenity_utils::message::TimestampStyle::ShortDateTime);
                                                            response_content.push('.');
                                                        }
                                                        response_content.build()
                                                    }
                                                };
                                                interaction.edit_response(ctx, EditInteractionResponse::new()
                                                    .content(response_content)
                                                ).await?;
                                            }
                                        }
                                    } else {
                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                            .content(if let French = event.language {
                                                "Désolé, cela n'est pas un timestamp au format de Discord. Vous pouvez utiliser @time ou <https://hammertime.cyou/> pour en générer un."
                                            } else {
                                                "Sorry, that doesn't look like a Discord timestamp. You can type `@time` or use <https://hammertime.cyou/> to generate one."
                                            })
                                        ).await?;
                                        transaction.rollback().await?;
                                    }
                                } else {
                                    interaction.edit_response(ctx, EditInteractionResponse::new()
                                        .content(if let French = event.language {
                                            "Désolé, seuls les participants de cette race et les organisateurs peuvent utiliser cette commande."
                                        } else {
                                            "Sorry, only participants in this race and organizers can use this command."
                                        })
                                    ).await?;
                                    transaction.rollback().await?;
                                }
                            }
                        } else if interaction.data.id == command_ids.result_async {
                            result_async_command(ctx, &interaction).await?;
                        } else if interaction.data.id == command_ids.forfeit_async {
                            forfeit_async_command(ctx, &interaction).await?;
                        } else if interaction.data.id == command_ids.schedule_async {
                            let game = interaction.data.options.get(1).map(|option| match option.value {
                                CommandDataOptionValue::Integer(game) => i16::try_from(game).expect("game number out of range"),
                                _ => panic!("unexpected slash command option type"),
                            });
                            if let Some((mut transaction, mut race, team)) = check_scheduling_thread_permissions(ctx, interaction, game, true, None, false).await? {
                                let event = race.event(&mut transaction).await?;
                                let is_organizer = event.organizers(&mut transaction).await?.into_iter().any(|organizer| organizer.discord.is_some_and(|discord| discord.id == interaction.user.id));
                                let was_scheduled = !matches!(race.schedule, RaceSchedule::Unscheduled);
                                if let Some(speedgaming_slug) = &event.speedgaming_slug {
                                    let response_content = if was_scheduled {
                                        format!("Please contact a tournament organizer to reschedule this race.")
                                    } else {
                                        MessageBuilder::default()
                                            .push("Please use <https://speedgaming.org/")
                                            .push(speedgaming_slug)
                                            .push("/submit> to schedule races for this event.")
                                            .build()
                                    };
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content(response_content)
                                    )).await?;
                                    transaction.rollback().await?;
                                } else if team.is_some() && event.asyncs_allowed() || is_organizer {
                                    let start = match interaction.data.options[0].value {
                                        CommandDataOptionValue::String(ref start) => start,
                                        _ => panic!("unexpected slash command option type"),
                                    };
                                    if let Some(start) = parse_timestamp(start) {
                                        if (start - Utc::now()).to_std().map_or(true, |schedule_notice| schedule_notice > Duration::from_secs(365 * 24 * 60 * 60)) {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content(if let French = event.language {
                                                    format!("Désolé, les races ne peuvent pas être planifiées plus d'un an à l'avance.")
                                                } else {
                                                    format!("Sorry, races cannot be scheduled more than 1 year in advance.")
                                                })
                                            )).await?;
                                            transaction.rollback().await?;
                                        } else if (start - Utc::now()).to_std().map_or(true, |schedule_notice| schedule_notice < event.min_schedule_notice) {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content(if event.min_schedule_notice <= Duration::default() {
                                                    if let French = event.language {
                                                        format!("Désolé mais cette date est dans le passé.")
                                                    } else {
                                                        format!("Sorry, that timestamp is in the past.")
                                                    }
                                                } else {
                                                    if let French = event.language {
                                                        format!("Désolé, les races doivent être planifiées au moins {} en avance.", French.format_duration(event.min_schedule_notice, true))
                                                    } else {
                                                        format!("Sorry, races must be scheduled at least {} in advance.", English.format_duration(event.min_schedule_notice, true))
                                                    }
                                                })
                                            )).await?;
                                            transaction.rollback().await?;
                                        } else {
                                            let (kind, was_scheduled) = match race.entrants {
                                                Entrants::Two([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2)]) => {
                                                    if team.as_ref().is_some_and(|team| team1 == team) {
                                                        let was_scheduled = race.schedule.set_async_start1(start).is_some();
                                                        race.schedule_updated_at = Some(Utc::now());
                                                        race.save(&mut transaction).await?;
                                                        (cal::EventKind::Async1, was_scheduled)
                                                    } else if team.as_ref().is_some_and(|team| team2 == team) {
                                                        let was_scheduled = race.schedule.set_async_start2(start).is_some();
                                                        race.schedule_updated_at = Some(Utc::now());
                                                        race.save(&mut transaction).await?;
                                                        (cal::EventKind::Async2, was_scheduled)
                                                    } else {
                                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                            .ephemeral(false)
                                                            .content("Sorry, only participants in this race can use this command for now. Please contact TreZ to edit the schedule.") //TODO allow TOs to schedule as async (with team parameter)
                                                        )).await?;
                                                        transaction.rollback().await?;
                                                        return Ok(())
                                                    }
                                                }
                                                Entrants::Three([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2), Entrant::MidosHouseTeam(ref team3)]) => {
                                                    if team.as_ref().is_some_and(|team| team1 == team) {
                                                        let was_scheduled = race.schedule.set_async_start1(start).is_some();
                                                        race.schedule_updated_at = Some(Utc::now());
                                                        race.save(&mut transaction).await?;
                                                        (cal::EventKind::Async1, was_scheduled)
                                                    } else if team.as_ref().is_some_and(|team| team2 == team) {
                                                        let was_scheduled = race.schedule.set_async_start2(start).is_some();
                                                        race.schedule_updated_at = Some(Utc::now());
                                                        race.save(&mut transaction).await?;
                                                        (cal::EventKind::Async2, was_scheduled)
                                                    } else if team.as_ref().is_some_and(|team| team3 == team) {
                                                        let was_scheduled = race.schedule.set_async_start3(start).is_some();
                                                        race.schedule_updated_at = Some(Utc::now());
                                                        race.save(&mut transaction).await?;
                                                        (cal::EventKind::Async3, was_scheduled)
                                                    } else {
                                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                            .ephemeral(false)
                                                            .content("Sorry, only participants in this race can use this command for now. Please contact TreZ to edit the schedule.") //TODO allow TOs to schedule as async (with team parameter)
                                                        )).await?;
                                                        transaction.rollback().await?;
                                                        return Ok(())
                                                    }
                                                }
                                                _ => panic!("tried to schedule race with not 2 or 3 MH teams as async"),
                                            };
                                            let cal_event = cal::Event { race, kind };
                                            if start - Utc::now() < TimeDelta::minutes(30) {
                                                let (http_client, new_room_lock, racetime_host, racetime_config, clean_shutdown) = {
                                                    let data = ctx.data.read().await;
                                                    (
                                                        data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
                                                        data.get::<NewRoomLock>().expect("new room lock missing from Discord context").clone(),
                                                        data.get::<RacetimeHost>().expect("racetime.gg host missing from Discord context").clone(),
                                                        data.get::<ConfigRaceTime>().expect("racetime.gg config missing from Discord context").clone(),
                                                        data.get::<CleanShutdown>().expect("clean shutdown state missing from Discord context").clone(),
                                                    )
                                                };
                                                lock!(new_room_lock = new_room_lock; {
                                                    let should_post_regular_response = if let Some((is_room_url, mut msg, _notification_channel)) = racetime_bot::create_room(&mut transaction, ctx, &racetime_host, &racetime_config.client_id, &racetime_config.client_secret, &http_client, clean_shutdown, &cal_event, &event).await? {
                                                        if is_room_url && cal_event.is_private_async_part() {
                                                            msg = match cal_event.race.entrants {
                                                                Entrants::Two(_) => format!("unlisted room for first async half: {msg}"),
                                                                Entrants::Three(_) => format!("unlisted room for first/second async part: {msg}"),
                                                                _ => format!("unlisted room for async part: {msg}"),
                                                            };
                                                            if let Some(channel) = event.discord_organizer_channel {
                                                                channel.say(ctx, &msg).await?;
                                                            } else {
                                                                // DM Ad
                                                                ADMIN_USER.create_dm_channel(ctx).await?.say(ctx, &msg).await?;
                                                            }
                                                        } else {
                                                            if let Some(channel) = event.discord_race_room_channel {
                                                                if let Err(e) = channel.send_message(ctx, CreateMessage::default().content(&msg).allowed_mentions(CreateAllowedMentions::default())).await {
                                                                    eprintln!("Failed to post race message to Discord race room channel: {}", e);
                                                                }
                                                            }
                                                        }
                                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                            .ephemeral(cal_event.is_private_async_part()) //TODO create public response without room link
                                                            .content(msg)
                                                        )).await?;
                                                        cal_event.is_private_async_part()
                                                    } else {
                                                        true
                                                    };
                                                    if should_post_regular_response {
                                                        let mut response_content = MessageBuilder::default();
                                                        response_content.push(if let Entrants::Two(_) = cal_event.race.entrants { "Your half of " } else { "Your part of " });
                                                        response_content.push(if let Some(game) = cal_event.race.game { format!("game {game}") } else { format!("this race") });
                                                        response_content.push(if was_scheduled { " has been rescheduled for " } else { " is now scheduled for " });
                                                        response_content.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                        let response_content = response_content
                                                            .push(". The async thread will be opened momentarily.")
                                                            .build();
                                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                            .ephemeral(false)
                                                            .content(response_content)
                                                        )).await?;
                                                    }
                                                    transaction.commit().await?;
                                                });

                                                // Update volunteer post if this was a reschedule
                                                if was_scheduled {
                                                    let pool = {
                                                        let data = ctx.data.read().await;
                                                        data.get::<DbPool>().expect("database connection pool missing from Discord context").clone()
                                                    };
                                                    let _ = volunteer_requests::update_volunteer_post_for_race(
                                                        &pool,
                                                        ctx,
                                                        cal_event.race.id,
                                                    ).await;

                                                    // Send reschedule notification DMs to volunteers
                                                    let mut transaction = pool.begin().await?;
                                                    let signups = event::roles::Signup::for_race(&mut transaction, cal_event.race.id).await?;
                                                    let affected_signups: Vec<_> = signups.iter()
                                                        .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Pending | event::roles::VolunteerSignupStatus::Confirmed))
                                                        .collect();

                                                    // Build race description
                                                    let race_description = if cal_event.race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                                                        match (&cal_event.race.round, &cal_event.race.phase) {
                                                            (Some(round), _) => round.clone(),
                                                            (None, Some(phase)) => phase.clone(),
                                                            (None, None) => "Qualifier".to_string(),
                                                        }
                                                    } else {
                                                        match &cal_event.race.entrants {
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
                                                            _ => cal_event.race.round.clone().or_else(|| cal_event.race.phase.clone()).unwrap_or_else(|| "Race".to_string()),
                                                        }
                                                    };

                                                    // Send DM to each affected volunteer
                                                    for signup in affected_signups {
                                                        if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                                                            if let Some(discord) = user.discord {
                                                                let discord_user_id = UserId::new(discord.id.get());

                                                                let mut msg = MessageBuilder::default();
                                                                msg.push("**Race Rescheduled**\n\n");
                                                                msg.push("The race ");
                                                                msg.push_mono(&race_description);
                                                                msg.push(" in ");
                                                                msg.push(&event.display_name);
                                                                msg.push(" has been rescheduled.\n\n");
                                                                msg.push("**New time:** ");
                                                                msg.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                                msg.push("\n\n");
                                                                msg.push("If you're no longer available, you can withdraw your signup using the button below or on the website: <");
                                                                msg.push(&format!("{}/event/{}/{}/races/{}/signups",
                                                                    base_uri(), cal_event.race.series.slug(), cal_event.race.event, u64::from(cal_event.race.id)));
                                                                msg.push(">");

                                                                // Create withdraw button
                                                                let button = CreateButton::new(format!("volunteer_withdraw_{}", u64::from(signup.id)))
                                                                    .label("Withdraw Signup")
                                                                    .style(ButtonStyle::Danger);
                                                                let row = CreateActionRow::Buttons(vec![button]);

                                                                // Send DM
                                                                if let Ok(dm_channel) = discord_user_id.create_dm_channel(ctx).await {
                                                                    if let Err(e) = dm_channel.send_message(ctx,
                                                                        CreateMessage::new()
                                                                            .content(msg.build())
                                                                            .components(vec![row])
                                                                    ).await {
                                                                        eprintln!("Failed to send reschedule notification DM to user {}: {}", signup.user_id, e);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    transaction.commit().await?;
                                                }
                                            } else {
                                                cal_event.race.save(&mut transaction).await?;
                                                let overlapping_maintenance_windows = if let RaceHandleMode::RaceTime = cal_event.should_create_room(&mut transaction, &event).await? {
                                                    sqlx::query_as!(Range::<DateTime<Utc>>, r#"SELECT start, end_time AS "end" FROM racetime_maintenance WHERE start < $1 AND end_time > $2"#, start + event.series.default_race_duration(), start - TimeDelta::minutes(30)).fetch_all(&mut *transaction).await?
                                                } else {
                                                    Vec::default()
                                                };
                                                transaction.commit().await?;

                                                // Update volunteer post if this was a reschedule
                                                if was_scheduled {
                                                    let pool = {
                                                        let data = ctx.data.read().await;
                                                        data.get::<DbPool>().expect("database connection pool missing from Discord context").clone()
                                                    };
                                                    let _ = volunteer_requests::update_volunteer_post_for_race(
                                                        &pool,
                                                        ctx,
                                                        cal_event.race.id,
                                                    ).await;

                                                    // Send reschedule notification DMs to volunteers
                                                    let mut transaction = pool.begin().await?;
                                                    let signups = event::roles::Signup::for_race(&mut transaction, cal_event.race.id).await?;
                                                    let affected_signups: Vec<_> = signups.iter()
                                                        .filter(|s| matches!(s.status, event::roles::VolunteerSignupStatus::Pending | event::roles::VolunteerSignupStatus::Confirmed))
                                                        .collect();

                                                    // Build race description
                                                    let race_description = if cal_event.race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                                                        match (&cal_event.race.round, &cal_event.race.phase) {
                                                            (Some(round), _) => round.clone(),
                                                            (None, Some(phase)) => phase.clone(),
                                                            (None, None) => "Qualifier".to_string(),
                                                        }
                                                    } else {
                                                        match &cal_event.race.entrants {
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
                                                            _ => cal_event.race.round.clone().or_else(|| cal_event.race.phase.clone()).unwrap_or_else(|| "Race".to_string()),
                                                        }
                                                    };

                                                    // Send DM to each affected volunteer
                                                    for signup in affected_signups {
                                                        if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                                                            if let Some(discord) = user.discord {
                                                                let discord_user_id = UserId::new(discord.id.get());

                                                                let mut msg = MessageBuilder::default();
                                                                msg.push("**Race Rescheduled**\n\n");
                                                                msg.push("The race ");
                                                                msg.push_mono(&race_description);
                                                                msg.push(" in ");
                                                                msg.push(&event.display_name);
                                                                msg.push(" has been rescheduled.\n\n");
                                                                msg.push("**New time:** ");
                                                                msg.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                                msg.push("\n\n");
                                                                msg.push("If you're no longer available, you can withdraw your signup using the button below or on the website: <");
                                                                msg.push(&format!("{}/event/{}/{}/races/{}/signups",
                                                                    base_uri(), cal_event.race.series.slug(), cal_event.race.event, u64::from(cal_event.race.id)));
                                                                msg.push(">");

                                                                // Create withdraw button
                                                                let button = CreateButton::new(format!("volunteer_withdraw_{}", u64::from(signup.id)))
                                                                    .label("Withdraw Signup")
                                                                    .style(ButtonStyle::Danger);
                                                                let row = CreateActionRow::Buttons(vec![button]);

                                                                // Send DM
                                                                if let Ok(dm_channel) = discord_user_id.create_dm_channel(ctx).await {
                                                                    if let Err(e) = dm_channel.send_message(ctx,
                                                                        CreateMessage::new()
                                                                            .content(msg.build())
                                                                            .components(vec![row])
                                                                    ).await {
                                                                        eprintln!("Failed to send reschedule notification DM to user {}: {}", signup.user_id, e);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    transaction.commit().await?;
                                                }

                                                let response_content = if_chain! {
                                                    if let French = event.language;
                                                    if cal_event.race.game.is_none();
                                                    if overlapping_maintenance_windows.is_empty();
                                                    then {
                                                        MessageBuilder::default()
                                                            .push("La partie de votre async a été planifiée pour le ")
                                                            .push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime)
                                                            .push('.')
                                                            .build()
                                                    } else {
                                                        let mut response_content = MessageBuilder::default();
                                                        response_content.push(if let Entrants::Two(_) = cal_event.race.entrants { "Your half of " } else { "Your part of " });
                                                        response_content.push(if let Some(game) = cal_event.race.game { format!("game {game}") } else { format!("this race") });
                                                        response_content.push(if was_scheduled { " has been rescheduled for " } else { " is now scheduled for " });
                                                        response_content.push_timestamp(start, serenity_utils::message::TimestampStyle::LongDateTime);
                                                        response_content.push('.');
                                                        for window in overlapping_maintenance_windows {
                                                            response_content.push_line("");
                                                            response_content.push_bold("Warning:");
                                                            if let Entrants::Two(_) = cal_event.race.entrants {
                                                                response_content.push(" this async half may overlap with racetime.gg maintenance planned for ");
                                                            } else {
                                                                response_content.push(" this async part may overlap with racetime.gg maintenance planned for ");
                                                            }
                                                            response_content.push_timestamp(window.start, serenity_utils::message::TimestampStyle::ShortDateTime);
                                                            response_content.push(" until ");
                                                            response_content.push_timestamp(window.end, serenity_utils::message::TimestampStyle::ShortDateTime);
                                                            response_content.push('.');
                                                        }
                                                        response_content.build()
                                                    }
                                                };
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(false)
                                                    .content(response_content)
                                                )).await?;
                                            }
                                        }
                                    } else {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content(if let French = event.language {
                                                "Désolé, cela n'est pas un timestamp au format de Discord. Vous pouvez utiliser `@time` ou <https://hammertime.cyou/> pour en générer un."
                                            } else {
                                                "Sorry, that doesn't look like a Discord timestamp. You can type `@time` or use <https://hammertime.cyou/> to generate one."
                                            })
                                        )).await?;
                                        transaction.rollback().await?;
                                    }
                                } else {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content(if event.asyncs_allowed() {
                                            if let French = event.language {
                                                "Désolé, seuls les participants de cette race et les organisateurs peuvent utiliser cette commande."
                                            } else {
                                                "Sorry, only participants in this race and organizers can use this command."
                                            }
                                        } else {
                                            "Sorry, asyncing races is not allowed for this event."
                                        })
                                    )).await?;
                                    transaction.rollback().await?;
                                }
                            }
                        } else if interaction.data.id == command_ids.schedule_remove {
                            let game = interaction.data.options.get(0).map(|option| match option.value {
                                CommandDataOptionValue::Integer(game) => i16::try_from(game).expect("game number out of range"),
                                _ => panic!("unexpected slash command option type"),
                            });
                            if let Some((mut transaction, race, team)) = check_scheduling_thread_permissions(ctx, interaction, game, true, None, false).await? {
                                let event = race.event(&mut transaction).await?;
                                let is_organizer = event.organizers(&mut transaction).await?.into_iter().any(|organizer| organizer.discord.is_some_and(|discord| discord.id == interaction.user.id));
                                if event.speedgaming_slug.is_some() {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Please contact a tournament organizer to reschedule this race.")
                                    )).await?;
                                    transaction.rollback().await?;
                                } else if team.is_some() || is_organizer {
                                    match race.schedule {
                                        RaceSchedule::Unscheduled => {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content(if let French = event.language {
                                                    "Désolé, cette race n'a pas de date de début prévue."
                                                } else {
                                                    "Sorry, this race already doesn't have a starting time."
                                                })
                                            )).await?;
                                            transaction.rollback().await?;
                                        }
                                        RaceSchedule::Live { .. } => {
                                            sqlx::query!("UPDATE races SET start = NULL, schedule_updated_at = NOW() WHERE id = $1", race.id as _).execute(&mut *transaction).await?;
                                            // Delete Discord scheduled event
                                            let mut race_mut = race.clone();
                                            if let Err(e) = crate::discord_scheduled_events::delete_discord_scheduled_event(ctx, &mut transaction, &mut race_mut, &event).await {
                                                eprintln!("Failed to delete Discord scheduled event for race {}: {}", race.id, e);
                                            }
                                            if race_mut.discord_scheduled_event_id.is_none() && race.discord_scheduled_event_id.is_some() {
                                                // Event was deleted, update the database
                                                race_mut.save(&mut transaction).await?;
                                            }
                                            transaction.commit().await?;
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                .ephemeral(false)
                                                .content(if let Some(game) = race.game {
                                                    format!("Game {game}'s starting time has been removed from the schedule.")
                                                } else {
                                                    if let French = event.language {
                                                        format!("L'horaire pour cette race ou cette async a été correctement retirée.")
                                                    } else {
                                                        format!("This race's starting time has been removed from the schedule.")
                                                    }
                                                })
                                            )).await?;
                                        }
                                        RaceSchedule::Async { .. } => match race.entrants {
                                            Entrants::Two([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2)]) => {
                                                if team.as_ref().is_some_and(|team| team1 == team) {
                                                    sqlx::query!("UPDATE races SET async_start1 = NULL, schedule_updated_at = NOW() WHERE id = $1", race.id as _).execute(&mut *transaction).await?;
                                                } else if team.as_ref().is_some_and(|team| team2 == team) {
                                                    sqlx::query!("UPDATE races SET async_start2 = NULL, schedule_updated_at = NOW() WHERE id = $1", race.id as _).execute(&mut *transaction).await?;
                                                } else {
                                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                        .ephemeral(true)
                                                        .content("Sorry, only participants in this race can use this command for now. Please contact TreZ to edit the schedule.") //TODO allow TOs to edit asynced schedules (with team parameter)
                                                    )).await?;
                                                    transaction.rollback().await?;
                                                    return Ok(())
                                                }
                                                transaction.commit().await?;
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(false)
                                                    .content(if let Some(game) = race.game {
                                                        format!("The starting time for your half of game {game} has been removed from the schedule.")
                                                    } else {
                                                        format!("The starting time for your half of this race has been removed from the schedule.")
                                                    })
                                                )).await?;
                                            }
                                            Entrants::Three([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2), Entrant::MidosHouseTeam(ref team3)]) => {
                                                if team.as_ref().is_some_and(|team| team1 == team) {
                                                    sqlx::query!("UPDATE races SET async_start1 = NULL, schedule_updated_at = NOW() WHERE id = $1", race.id as _).execute(&mut *transaction).await?;
                                                } else if team.as_ref().is_some_and(|team| team2 == team) {
                                                    sqlx::query!("UPDATE races SET async_start2 = NULL, schedule_updated_at = NOW() WHERE id = $1", race.id as _).execute(&mut *transaction).await?;
                                                } else if team.as_ref().is_some_and(|team| team3 == team) {
                                                    sqlx::query!("UPDATE races SET async_start3 = NULL, schedule_updated_at = NOW() WHERE id = $1", race.id as _).execute(&mut *transaction).await?;
                                                } else {
                                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                        .ephemeral(true)
                                                        .content("Sorry, only participants in this race can use this command for now. Please contact TreZ to edit the schedule.") //TODO allow TOs to edit asynced schedules (with team parameter)
                                                    )).await?;
                                                    transaction.rollback().await?;
                                                    return Ok(())
                                                }
                                                transaction.commit().await?;
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(false)
                                                    .content(if let Some(game) = race.game {
                                                        format!("The starting time for your part of game {game} has been removed from the schedule.")
                                                    } else {
                                                        format!("The starting time for your part of this race has been removed from the schedule.")
                                                    })
                                                )).await?;
                                            }
                                            _ => panic!("found race with not 2 or 3 MH teams scheduled as async"),
                                        },
                                    }
                                } else {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content(if let French = event.language {
                                            "Désolé, seuls les participants de cette race et les organisateurs peuvent utiliser cette commande."
                                        } else {
                                            "Sorry, only participants in this race and organizers can use this command."
                                        })
                                    )).await?;
                                    transaction.rollback().await?;
                                }
                            }
                        } else if Some(interaction.data.id) == command_ids.second {
                            if let Some((_, mut race, draft_kind, msg_ctx)) = check_draft_permissions(ctx, interaction).await? {
                                match draft_kind {
                                    draft::Kind::RslS7 => {
                                        let settings = &mut race.draft.as_mut().unwrap().settings;
                                        let lite = interaction.data.options.get(0).map(|option| match option.value {
                                            CommandDataOptionValue::Boolean(lite) => lite,
                                            _ => panic!("unexpected slash command option type"),
                                        });
                                        if settings.get("lite_ok").map(|lite_ok| &**lite_ok).unwrap_or("no") == "ok" {
                                            let mut transaction = msg_ctx.into_transaction();
                                            if let Some(lite) = lite {
                                                settings.insert(Cow::Borrowed("preset"), Cow::Borrowed(if lite { "lite" } else { "league" }));
                                                sqlx::query!("UPDATE races SET draft_state = $1 WHERE id = $2", Json(race.draft.as_ref().unwrap()) as _, race.id as _).execute(&mut *transaction).await?;
                                                transaction.commit().await?;
                                            } else {
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content(MessageBuilder::default().push("Sorry, please specify the ").push_mono("lite").push(" parameter.").build())
                                                )).await?;
                                                transaction.rollback().await?;
                                                return Ok(())
                                            }
                                        } else {
                                            if lite.is_some_and(identity) {
                                                //TODO different error messages depending on which player(s) didn't opt into RSL-Lite
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("Sorry, either you or your opponent didn't opt into RSL-Lite.")
                                                )).await?;
                                                return Ok(())
                                            }
                                        }
                                    }
                                    draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => {
                                        let settings = &mut race.draft.as_mut().unwrap().settings;
                                        let mq = interaction.data.options.get(0).map(|option| match option.value {
                                            CommandDataOptionValue::Integer(mq) => u8::try_from(mq).expect("MQ count out of range"),
                                            _ => panic!("unexpected slash command option type"),
                                        });
                                        if settings.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" {
                                            let mut transaction = msg_ctx.into_transaction();
                                            if let Some(mq) = mq {
                                                settings.insert(Cow::Borrowed("mq_dungeons_count"), Cow::Owned(mq.to_string()));
                                                sqlx::query!("UPDATE races SET draft_state = $1 WHERE id = $2", Json(race.draft.as_ref().unwrap()) as _, race.id as _).execute(&mut *transaction).await?;
                                                transaction.commit().await?;
                                            } else {
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("Désolé, veuillez entrer le nombre de donjons MQ d'abord.")
                                                )).await?;
                                                transaction.rollback().await?;
                                                return Ok(())
                                            }
                                        } else {
                                            if mq.is_some_and(|mq| mq != 0) {
                                                //TODO different error messages depending on which player(s) didn't opt into MQ
                                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("Désolé, mais l'un d'entre vous n'a pas choisi les donjons MQ.")
                                                )).await?;
                                                return Ok(())
                                            }
                                        }
                                    }
                                    draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 | draft::Kind::AlttprDe9 => {}
                                }
                                draft_action(ctx, interaction, draft::Action::GoFirst(false)).await?;
                            }
                        } else if Some(interaction.data.id) == command_ids.skip {
                            draft_action(ctx, interaction, draft::Action::Skip).await?;
                        } else if interaction.data.id == command_ids.status {
                            if let Some((mut transaction, race, team)) = check_scheduling_thread_permissions(ctx, interaction, None, true, None, false).await? {
                                let event = race.event(&mut transaction).await?;
                                if let Some(draft_kind) = event.draft_kind() {
                                    if let Some(ref draft) = race.draft {
                                        let mut msg_ctx = draft::MessageContext::Discord {
                                            teams: race.teams().cloned().collect(),
                                            team: team.unwrap_or_else(Team::dummy),
                                            transaction, guild_id, command_ids,
                                        };
                                        let response_content = MessageBuilder::default()
                                            //TODO include scheduling status, both for regular races and for asyncs
                                            .push(draft.next_step(draft_kind, race.game, &mut msg_ctx).await?.message)
                                            .build();
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content(response_content)
                                        )).await?;
                                        msg_ctx.into_transaction().commit().await?;
                                    } else {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("Sorry, this race's settings draft has not been initialized. Please contact a tournament organizer to fix this.")
                                        )).await?;
                                        transaction.rollback().await?;
                                    }
                                } else {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Sorry, this command is currently only available for events with settings drafts.") //TODO
                                    )).await?;
                                    transaction.rollback().await?;
                                }
                            }
                        } else if interaction.data.id == command_ids.watch_roles {
                            let watch_party_channel = match interaction.data.options[0].value {
                                CommandDataOptionValue::Channel(channel) => channel,
                                _ => panic!("unexpected slash command option type"),
                            };
                            let race_rooms_channel = match interaction.data.options[1].value {
                                CommandDataOptionValue::Channel(channel) => channel,
                                _ => panic!("unexpected slash command option type"),
                            };
                            guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(false)
                                .name("restream watcher")
                                .permissions(Permissions::empty())
                            ).await?;
                            let watch_party_role = guild_id.create_role(ctx, EditRole::new()
                                .hoist(false)
                                .mentionable(true)
                                .name("watch party watcher")
                                .permissions(Permissions::empty())
                            ).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(false)
                                .content(MessageBuilder::default()
                                    .push("Click a button below to get notified when a restream or Discord watch party is about to start. Click again to remove it. Multiple selections allowed. If you start watching a race in ")
                                    .mention(&watch_party_channel)
                                    .push(", please ping ")
                                    .mention(&watch_party_role)
                                    .push(". To get notified for ")
                                    .push_italic("all")
                                    .push(" matches, set notifications for ")
                                    .mention(&race_rooms_channel)
                                    .push(" to all messages.")
                                    .build()
                                )
                                .button(CreateButton::new("watchrole_restream").label("restream watcher"))
                                .button(CreateButton::new("watchrole_party").label("watch party watcher"))
                            )).await?;
                        } else if Some(interaction.data.id) == command_ids.yes {
                            draft_action(ctx, interaction, draft::Action::BooleanChoice(true)).await?;
                        } else {
                            panic!("unexpected slash command")
                        }
                    }
                }
                Interaction::Component(interaction) => match &*interaction.data.custom_id {
                    "async_ready" => {
                        // Handle async ready button
                        let mut transaction = {
                            let discord_data = ctx.data.read().await;
                            discord_data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?
                        };

                        // Extract race_id and async_part from the button's custom_id
                        // We'll need to encode this in the button's custom_id
                        // For now, we'll need to find the race by thread ID
                        let thread_id = interaction.channel_id.get() as i64;

                        // Find the race and async part for this thread
                        let race_info = sqlx::query!(
                            r#"
                            SELECT id,
                                   CASE
                                       WHEN async_thread1 = $1 THEN 1
                                       WHEN async_thread2 = $1 THEN 2
                                       WHEN async_thread3 = $1 THEN 3
                                       ELSE NULL
                                   END as async_part
                            FROM races
                            WHERE async_thread1 = $1 OR async_thread2 = $1 OR async_thread3 = $1
                            "#,
                            thread_id
                        ).fetch_optional(&mut *transaction).await?;

                        if let Some(race_info) = race_info {
                            if let Some(async_part) = race_info.async_part {
                                match AsyncRaceManager::handle_ready_button(
                                    &ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context"),
                                    ctx,
                                    race_info.id,
                                    async_part as u8,
                                    interaction.user.id,
                                ).await {
                                    Ok(()) => {
                                        // Remove the button completely by editing the original message
                                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                            CreateInteractionResponseMessage::new()
                                                .components(vec![]) // Empty components removes all buttons
                                        )).await?;
                                    }
                                    Err(e) => {
                                        let error_msg = match e {
                                            AsyncRaceError::UnauthorizedUser => "You are not authorized to click this button.",
                                            AsyncRaceError::AlreadyReady => "You have already clicked ready for this race.",
                                            _ => {
                                                eprintln!("Async ready error: {:?}", e);
                                                "An error occurred while processing your ready status."
                                            },
                                        };
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(
                                            CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content(error_msg)
                                        )).await?;
                                    }
                                }
                            } else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Could not determine which async part this thread is for.")
                                )).await?;
                            }
                        } else {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("This thread is not associated with an async race.")
                            )).await?;
                        }

                        transaction.rollback().await?;
                    }
                    "async_start_countdown" => {
                        // Handle async start countdown button
                        let mut transaction = {
                            let discord_data = ctx.data.read().await;
                            discord_data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?
                        };

                        let thread_id = interaction.channel_id.get() as i64;

                        // Find the race and async part for this thread
                        let race_info = sqlx::query!(
                            r#"
                            SELECT id,
                                   CASE
                                       WHEN async_thread1 = $1 THEN 1
                                       WHEN async_thread2 = $1 THEN 2
                                       WHEN async_thread3 = $1 THEN 3
                                       ELSE NULL
                                   END as async_part
                            FROM races
                            WHERE async_thread1 = $1 OR async_thread2 = $1 OR async_thread3 = $1
                            "#,
                            thread_id
                        ).fetch_optional(&mut *transaction).await?;

                        if let Some(race_info) = race_info {
                            if let Some(async_part) = race_info.async_part {
                                // Defer the interaction to prevent timeout
                                interaction.defer(&ctx.http).await?;

                                match AsyncRaceManager::handle_start_countdown_button(
                                    &ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context"),
                                    ctx,
                                    race_info.id,
                                    async_part as u8,
                                    interaction.user.id,
                                ).await {
                                    Ok(()) => {
                                        // Remove the button completely by editing the original message
                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                            .components(vec![]) // Empty components removes all buttons
                                        ).await?;
                                    }
                                    Err(e) => {
                                        let error_msg = match e {
                                            AsyncRaceError::UnauthorizedUser => "You are not authorized to click this button.",
                                            AsyncRaceError::AlreadyStarted => "The countdown has already been started for this race.",
                                            _ => {
                                                eprintln!("Async countdown error: {:?}", e);
                                                "An error occurred while processing your countdown request."
                                            },
                                        };
                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                            .content(error_msg)
                                        ).await?;
                                    }
                                }
                            } else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Could not determine which async part this thread is for.")
                                )).await?;
                            }
                        } else {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("This thread is not associated with an async race.")
                            )).await?;
                        }

                        transaction.rollback().await?;
                    }
                    "async_finish" => {
                        // Handle async finish button - start revert period
                        let mut transaction = {
                            let discord_data = ctx.data.read().await;
                            discord_data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?
                        };

                        let thread_id = interaction.channel_id.get() as i64;

                        // Find the race and async part for this thread
                        let race_info = sqlx::query!(
                            r#"
                            SELECT id,
                                   CASE
                                       WHEN async_thread1 = $1 THEN 1
                                       WHEN async_thread2 = $1 THEN 2
                                       WHEN async_thread3 = $1 THEN 3
                                       ELSE NULL
                                   END as async_part
                            FROM races
                            WHERE async_thread1 = $1 OR async_thread2 = $1 OR async_thread3 = $1
                            "#,
                            thread_id
                        ).fetch_optional(&mut *transaction).await?;

                        if let Some(race_info) = race_info {
                            if let Some(async_part) = race_info.async_part {
                                // Defer the interaction to prevent timeout
                                interaction.defer(&ctx.http).await?;

                                match AsyncRaceManager::handle_finish_button(
                                    &ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context"),
                                    ctx,
                                    race_info.id,
                                    async_part as u8,
                                    interaction.user.id,
                                ).await {
                                    Ok(formatted_time) => {
                                        // Replace FINISH/FORFEIT buttons with REVERT button
                                        let revert_button = CreateActionRow::Buttons(vec![
                                            CreateButton::new("async_revert")
                                                .label("REVERT")
                                                .style(ButtonStyle::Secondary)
                                        ]);

                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                            .content(format!("✅ **Finished in {}**\nYou have 30 seconds to revert if needed.", formatted_time))
                                            .components(vec![revert_button])
                                        ).await?;

                                        // Spawn a background task to finalize after 30 seconds
                                        let ctx_clone = ctx.clone();
                                        let pool_clone = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").clone();
                                        let race_id = race_info.id;
                                        let async_part = async_part as u8;
                                        let channel_id = interaction.channel_id;
                                        let message_id = interaction.get_response(&ctx.http).await?.id;

                                        tokio::spawn(async move {
                                            sleep(Duration::from_secs(30)).await;

                                            // Check if the message still has the REVERT button (wasn't clicked)
                                            if let Ok(message) = channel_id.message(&ctx_clone, message_id).await {
                                                // Check if the message still has exactly 1 component row with 1 button (REVERT),
                                                // the content still says "30 seconds to revert", AND the displayed time
                                                // matches this task's time. The time check prevents a stale task from
                                                // firing if the user reverted and finished again (which spawns a new task).
                                                let has_revert_button = message.components.len() == 1
                                                    && message.components.first()
                                                        .map_or(false, |row| row.components.len() == 1)
                                                    && message.content.contains("30 seconds to revert")
                                                    && message.content.contains(&formatted_time);

                                                if has_revert_button {
                                                    // Finalize the finish
                                                    let _ = AsyncRaceManager::finalize_finish(
                                                        &pool_clone,
                                                        &ctx_clone,
                                                        race_id,
                                                        async_part,
                                                        formatted_time.clone(),
                                                    ).await;

                                                    // Remove the REVERT button
                                                    let _ = channel_id.edit_message(&ctx_clone, message_id, serenity::all::EditMessage::new()
                                                        .components(vec![])
                                                    ).await;
                                                }
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        let error_msg = match e {
                                            AsyncRaceError::UnauthorizedUser => "You are not authorized to click this button.",
                                            AsyncRaceError::NotStarted => "You must start the countdown before finishing.",
                                            AsyncRaceError::AlreadyFinished => "You have already finished this race.",
                                            _ => {
                                                eprintln!("Async finish error: {:?}", e);
                                                "An error occurred while processing your finish request."
                                            },
                                        };
                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                            .content(error_msg)
                                        ).await?;
                                    }
                                }
                            } else {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Could not determine which async part this thread is for.")
                                )).await?;
                            }
                        } else {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("This thread is not associated with an async race.")
                            )).await?;
                        }

                        transaction.rollback().await?;
                    }
                    "async_revert" => {
                        // Handle REVERT button - restore FINISH/FORFEIT buttons by editing the ORIGINAL message
                        // This is important because the spawned task is watching the original message
                        let race_buttons = CreateActionRow::Buttons(vec![
                            CreateButton::new("async_finish")
                                .label("FINISH")
                                .style(ButtonStyle::Danger),
                            CreateButton::new("async_forfeit")
                                .label("FORFEIT")
                                .style(ButtonStyle::Secondary),
                        ]);

                        // Update the message that contains the REVERT button (the one the user clicked)
                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .content("**Finish reverted!** Continue playing and click the FINISH button once you have completed your run.\nIf you need to forfeit, click the FORFEIT button.")
                                .components(vec![race_buttons])
                        )).await?;
                    }
                    "async_forfeit" => {
                        // Show confirmation for bracket async forfeit
                        let confirm_button = CreateActionRow::Buttons(vec![
                            CreateButton::new("async_forfeit_confirm")
                                .label("Yes, forfeit")
                                .style(ButtonStyle::Danger),
                            CreateButton::new("async_forfeit_cancel")
                                .label("Cancel")
                                .style(ButtonStyle::Secondary),
                        ]);

                        interaction.create_response(ctx, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("⚠️ **Are you sure you want to forfeit?**\n\nThis will notify the organizers that you are forfeiting this async race.")
                                .components(vec![confirm_button])
                        )).await?;
                    }
                    "async_forfeit_confirm" => {
                        // Handle confirmed bracket async forfeit
                        let mut transaction = {
                            let discord_data = ctx.data.read().await;
                            discord_data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?
                        };

                        let thread_id = interaction.channel_id.get() as i64;

                        // Find the race for this thread
                        let race_info = sqlx::query!(
                            r#"
                            SELECT id, async_thread1, async_thread2, async_thread3
                            FROM races
                            WHERE async_thread1 = $1 OR async_thread2 = $1 OR async_thread3 = $1
                            "#,
                            thread_id
                        ).fetch_optional(&mut *transaction).await?;

                        if let Some(race_info) = race_info {
                            let async_part = if race_info.async_thread1 == Some(thread_id) {
                                1
                            } else if race_info.async_thread2 == Some(thread_id) {
                                2
                            } else if race_info.async_thread3 == Some(thread_id) {
                                3
                            } else {
                                0
                            };

                            if async_part > 0 {
                                // Send @here ping to the thread informing of forfeit
                                let mut msg = MessageBuilder::default();
                                msg.push("@here - ");
                                msg.mention(&interaction.user);
                                msg.push(" has indicated they want to **forfeit** this async race.\n\n");
                                msg.push("**Organizers:** To confirm this forfeit, use the `/forfeit-async` command in this thread.");

                                interaction.channel_id.say(ctx, msg.build()).await?;

                                // Find and edit the message with FINISH/FORFEIT buttons to remove them
                                // Get messages from the channel and find the one with the buttons
                                // We look for the message that contains the text "Good luck!" which is sent with the FINISH/FORFEIT buttons
                                let messages = interaction.channel_id.messages(ctx, serenity::all::GetMessages::new().limit(50)).await?;
                                for mut message in messages {
                                    // Check if this message contains "Good luck!" and has components
                                    if !message.components.is_empty() && message.content.contains("Good luck!") {
                                        // Edit this message to remove the buttons
                                        let mut edit_msg = serenity::all::EditMessage::new();
                                        edit_msg = edit_msg.components(vec![]);
                                        let _ = message.edit(ctx, edit_msg).await;
                                        break;
                                    }
                                }

                                // Update the ephemeral response to confirm
                                interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                    CreateInteractionResponseMessage::new()
                                        .content("✅ Forfeit request sent. An organizer will confirm it shortly.")
                                        .components(vec![])
                                )).await?;
                            } else {
                                interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                    CreateInteractionResponseMessage::new()
                                        .content("Could not determine which async part this thread is for.")
                                        .components(vec![])
                                )).await?;
                            }
                        } else {
                            interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                CreateInteractionResponseMessage::new()
                                    .content("This thread is not associated with an async race.")
                                    .components(vec![])
                            )).await?;
                        }

                        transaction.rollback().await?;
                    }
                    "async_forfeit_cancel" => {
                        // Cancel the forfeit
                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .content("Forfeit cancelled.")
                                .components(vec![])
                        )).await?;
                    }
                    "pronouns_he" => {
                        let member = interaction.member.clone().expect("/pronoun-roles called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "he/him").expect("missing 'he/him' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    "pronouns_she" => {
                        let member = interaction.member.clone().expect("/pronoun-roles called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "she/her").expect("missing 'she/her' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    "pronouns_they" => {
                        let member = interaction.member.clone().expect("/pronoun-roles called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "they/them").expect("missing 'they/them' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    "pronouns_other" => {
                        let member = interaction.member.clone().expect("/pronoun-roles called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "other pronouns").expect("missing 'other pronouns' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    "racingrole" => {
                        let member = interaction.member.clone().expect("/racing-role called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "racing").expect("missing 'racing' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    "watchrole_restream" => {
                        let member = interaction.member.clone().expect("/watch-roles called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "restream watcher").expect("missing 'restream watcher' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    "watchrole_party" => {
                        let member = interaction.member.clone().expect("/watch-roles called outside of a guild");
                        let role = member.guild_id.roles(ctx).await?.into_values().find(|role| role.name == "watch party watcher").expect("missing 'watch party watcher' role");
                        if member.roles(ctx).expect("failed to look up member roles").contains(&role) {
                            member.remove_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role removed.")
                            )).await?;
                        } else {
                            member.add_role(ctx, role).await?;
                            interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Role added.")
                            )).await?;
                        }
                    }
                    custom_id => if let Some(page) = custom_id.strip_prefix("ban_page_") {
                        send_draft_settings_page(ctx, interaction, "ban", page.parse().unwrap()).await?;
                    } else if let Some(setting) = custom_id.strip_prefix("ban_setting_") {
                        draft_action(ctx, interaction, draft::Action::Ban { setting: setting.to_owned() }).await?;
                    } else if let Some(page) = custom_id.strip_prefix("draft_page_") {
                        send_draft_settings_page(ctx, interaction, "draft", page.parse().unwrap()).await?;
                    } else if let Some(setting) = custom_id.strip_prefix("draft_setting_") {
                        let Some((event, mut race, draft_kind, mut msg_ctx)) = check_draft_permissions(ctx, interaction).await? else { return Ok(()) };
                        match race.draft.as_ref().unwrap().next_step(draft_kind, race.game, &mut msg_ctx).await?.kind {
                            draft::StepKind::Ban { available_settings, .. } if available_settings.get(setting).is_some() => {
                                let setting = available_settings.get(setting).unwrap(); // `if let` guards are experimental
                                msg_ctx.into_transaction().commit().await?;
                                let response_content = if let French = event.language {
                                    format!("Sélectionnez la configuration du setting {} :", setting.display)
                                } else {
                                    format!("Select the value for the {} setting:", setting.display)
                                };
                                interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content(response_content)
                                    .button(CreateButton::new(format!("draft_option_{}__{}", setting.name, setting.default)).label(setting.default_display))
                                    .button(CreateButton::new("draft_page_0").label(if let French = event.language { "Retour" } else { "Back" }).style(ButtonStyle::Secondary)) //TODO remember page?
                                )).await?;
                            }
                            draft::StepKind::Pick { available_choices, .. } if available_choices.get(setting).is_some() => {
                                let setting = available_choices.get(setting).unwrap(); // `if let` guards are experimental
                                // For simple mode picks (like AlttprDe9) where there's only one option matching the setting name, skip the "select value" dialog and apply directly
                                if setting.options.len() == 1 && setting.options[0].name == setting.name {
                                    drop(msg_ctx);
                                    draft_action(ctx, interaction, draft::Action::Pick { setting: setting.name.to_owned(), value: setting.name.to_owned() }).await?;
                                } else {
                                    msg_ctx.into_transaction().commit().await?;
                                    let response_content = if let French = event.language {
                                        format!("Sélectionnez la configuration du setting {} :", setting.display)
                                    } else {
                                        format!("Select the value for the {} setting:", setting.display)
                                    };
                                    let mut response_msg = CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content(response_content);
                                    for option in setting.options {
                                        response_msg = response_msg.button(CreateButton::new(format!("draft_option_{}__{}", setting.name, option.name)).label(option.display));
                                    }
                                    response_msg = response_msg.button(CreateButton::new("draft_page_0").label(if let French = event.language { "Retour" } else { "Back" }).style(ButtonStyle::Secondary)); //TODO remember page?
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(response_msg)).await?;
                                }
                            }
                            | draft::StepKind::GoFirst
                            | draft::StepKind::Ban { .. }
                            | draft::StepKind::Pick { .. }
                            | draft::StepKind::BooleanChoice { .. }
                            | draft::StepKind::Done(_)
                            | draft::StepKind::DoneRsl { .. }
                                => match race.draft.as_mut().unwrap().apply(draft_kind, race.game, &mut msg_ctx, draft::Action::Pick { setting: format!("@placeholder"), value: format!("@placeholder") }).await? {
                                    Ok(_) => unreachable!(),
                                    Err(error_msg) => {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content(error_msg)
                                        )).await?;
                                        msg_ctx.into_transaction().rollback().await?;
                                    }
                                },
                        }
                    } else if let Some((setting, value)) = custom_id.strip_prefix("draft_option_").and_then(|setting_value| setting_value.split_once("__")) {
                        draft_action(ctx, interaction, draft::Action::Pick { setting: setting.to_owned(), value: value.to_owned() }).await?;
                    } else if let Some(speedgaming_id) = custom_id.strip_prefix("sgdisambig_") {
                        let (mut transaction, http_client) = {
                            let data = ctx.data.read().await;
                            (
                                data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?,
                                data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
                            )
                        };
                        let speedgaming_id = speedgaming_id.parse()?;
                        let ComponentInteractionDataKind::StringSelect { ref values } = interaction.data.kind else { panic!("sgdisambig interaction with unexpected payload") };
                        let race_id = values.iter().exactly_one().expect("sgdisambig interaction with unexpected payload").parse()?;
                        let mut race = Race::from_id(&mut transaction, &http_client, race_id).await?;
                        let Some(speedgaming_slug) = race.event(&mut transaction).await?.speedgaming_slug else { panic!("sgdisambig interaction for race from non-SpeedGaming event") };
                        let schedule = sgl::schedule(&http_client, &speedgaming_slug).await?;
                        let restream = schedule.into_iter().find(|restream| restream.matches().any(|restream_match| restream_match.id == speedgaming_id)).expect("no such SpeedGaming match ID");
                        restream.update_race(&mut race, speedgaming_id)?;
                        race.save(&mut transaction).await?;
                        transaction.commit().await?;
                    } else if let Some(params) = custom_id.strip_prefix("async_ready_qualifier_") {
                        // Handle qualifier READY button: format is "async_ready_qualifier_{team_id}_{async_kind}"
                        if let Some((team_id_str, async_kind_str)) = params.split_once('_') {
                            if let (Ok(team_id), Ok(async_kind_int)) = (team_id_str.parse::<u64>(), async_kind_str.parse::<i32>()) {
                                let Some(async_kind) = AsyncKind::from_i32(async_kind_int) else { return Ok(()) };
                                let pool = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").clone();
                                let mut transaction = pool.begin().await?;

                                // Verify user is a member of the team
                                let is_team_member = sqlx::query_scalar!(
                                    r#"SELECT EXISTS (
                                        SELECT 1 FROM team_members tm
                                        JOIN users u ON tm.member = u.id
                                        WHERE tm.team = $1 AND u.discord_id = $2
                                    ) AS "exists!""#,
                                    team_id as i64,
                                    interaction.user.id.get() as i64
                                ).fetch_one(&mut *transaction).await?;

                                if !is_team_member {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(
                                        CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("You are not authorized to click this button.")
                                    )).await?;
                                } else {
                                    // Get seed info and distribute it
                                    let seed_info = sqlx::query!(
                                        r#"
                                        SELECT a.web_id, a.tfb_uuid, a.xkeys_uuid, a.file_stem,
                                               a.hash1, a.hash2, a.hash3, a.hash4, a.hash5, a.seed_password,
                                               a.seed_data,
                                               t.series AS "series: Series", t.event
                                        FROM async_teams at
                                        JOIN teams t ON at.team = t.id
                                        JOIN asyncs a ON t.series = a.series AND t.event = a.event AND at.kind = a.kind
                                        WHERE at.team = $1 AND at.kind = $2
                                        "#,
                                        team_id as i64,
                                        async_kind as _
                                    ).fetch_optional(&mut *transaction).await?;

                                    if let Some(seed) = seed_info {
                                        // Build seed message
                                        let mut seed_msg = MessageBuilder::default();
                                        seed_msg.push("**Your seed is ready!**\n\n");

                                        if let Some(web_id) = seed.web_id {
                                            seed_msg.push(format!("Seed URL: https://ootrandomizer.com/seed/get?id={}\n", web_id));
                                        }
                                        if let Some(tfb_uuid) = seed.tfb_uuid {
                                            seed_msg.push(format!("Triforce Blitz Seed: https://tfb.midos.house/seed/{}\n", tfb_uuid));
                                        }
                                        if let Some(xkeys_uuid) = seed.xkeys_uuid {
                                            let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").unwrap();
                                            patcher_url.query_pairs_mut().append_pair("patch", &format!("{}/seed/DR_{xkeys_uuid}.bps", base_uri()));
                                            seed_msg.push(format!("Door Rando Seed: {}\n", patcher_url));
                                        }
                                        if let Some(file_stem) = &seed.file_stem {
                                            seed_msg.push(format!("Seed file: {}/seed/{}.zpfz\n", base_uri(), file_stem));
                                        }

                                        // Add hash if available
                                        if seed.hash1.is_some() {
                                            seed_msg.push("\nHash: ");
                                            if let (Some(h1), Some(h2), Some(h3), Some(h4), Some(h5)) = (&seed.hash1, &seed.hash2, &seed.hash3, &seed.hash4, &seed.hash5) {
                                                seed_msg.push(format!("{}, {}, {}, {}, {}\n", h1, h2, h3, h4, h5));
                                            }
                                        }

                                        if let Some(ref seed_data) = seed.seed_data {
                                            if let Some(permalink) = seed_data.get("permalink").and_then(|v| v.as_str()) {
                                                if !permalink.is_empty() {
                                                    seed_msg.push(format!("**Permalink:** `{}`\n", permalink));
                                                }
                                            }
                                            if let Some(seed_hash) = seed_data.get("seed_hash").and_then(|v| v.as_str()) {
                                                if !seed_hash.is_empty() {
                                                    seed_msg.push(format!("**Seed Hash:** {}\n", seed_hash));
                                                }
                                            }
                                        }

                                        if let Some(password) = &seed.seed_password {
                                            seed_msg.push(format!("\nPassword: {}\n", password));
                                        }

                                        // Send seed message and START button
                                        let start_button = CreateActionRow::Buttons(vec![
                                            CreateButton::new(format!("async_start_qualifier_{}_{}", team_id, async_kind as i32))
                                                .label("START COUNTDOWN")
                                                .style(ButtonStyle::Success)
                                        ]);

                                        interaction.channel_id.send_message(ctx, CreateMessage::new()
                                            .content(seed_msg.build())
                                            .components(vec![start_button])
                                        ).await?;

                                        // Remove the READY button
                                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                            CreateInteractionResponseMessage::new()
                                                .components(vec![])
                                        )).await?;
                                    } else {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(
                                            CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content("Could not find seed information for this qualifier.")
                                        )).await?;
                                    }
                                }
                                transaction.commit().await?;
                            }
                        }
                    } else if let Some(params) = custom_id.strip_prefix("async_start_qualifier_") {
                        // Handle qualifier START COUNTDOWN button
                        if let Some((team_id_str, async_kind_str)) = params.split_once('_') {
                            if let (Ok(team_id), Ok(async_kind_int)) = (team_id_str.parse::<u64>(), async_kind_str.parse::<i32>()) {
                                let Some(async_kind) = AsyncKind::from_i32(async_kind_int) else { return Ok(()) };
                                let pool = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").clone();
                                let mut transaction = pool.begin().await?;

                                // Verify user is a member of the team
                                let is_team_member = sqlx::query_scalar!(
                                    r#"SELECT EXISTS (
                                        SELECT 1 FROM team_members tm
                                        JOIN users u ON tm.member = u.id
                                        WHERE tm.team = $1 AND u.discord_id = $2
                                    ) AS "exists!""#,
                                    team_id as i64,
                                    interaction.user.id.get() as i64
                                ).fetch_one(&mut *transaction).await?;

                                if !is_team_member {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(
                                        CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("You are not authorized to click this button.")
                                    )).await?;
                                    transaction.rollback().await?;
                                } else {
                                    // Check if already started
                                    let already_started = sqlx::query_scalar!(
                                        r#"SELECT start_time IS NOT NULL AS "started!" FROM async_teams WHERE team = $1 AND kind = $2"#,
                                        team_id as i64,
                                        async_kind as _
                                    ).fetch_optional(&mut *transaction).await?.unwrap_or(false);

                                    if already_started {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(
                                            CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content("The countdown has already been started.")
                                        )).await?;
                                        transaction.rollback().await?;
                                    } else {
                                        // Defer interaction for countdown
                                        interaction.defer(&ctx.http).await?;

                                        // Send countdown messages
                                        interaction.channel_id.say(ctx, "**Your async is about to start!**").await?;
                                        sleep(Duration::from_secs(1)).await;

                                        for i in (1..=5).rev() {
                                            interaction.channel_id.say(ctx, format!("**{}**", i)).await?;
                                            sleep(Duration::from_secs(1)).await;
                                        }

                                        interaction.channel_id.say(ctx, "**GO!** 🏃‍♂️").await?;

                                        // Record start time
                                        let now = Utc::now();
                                        sqlx::query!(
                                            "UPDATE async_teams SET start_time = $1 WHERE team = $2 AND kind = $3",
                                            now,
                                            team_id as i64,
                                            async_kind as _
                                        ).execute(&mut *transaction).await?;

                                        // Send FINISH and FORFEIT buttons
                                        let race_buttons = CreateActionRow::Buttons(vec![
                                            CreateButton::new(format!("async_finish_qualifier_{}_{}", team_id, async_kind as i32))
                                                .label("FINISH")
                                                .style(ButtonStyle::Danger),
                                            CreateButton::new(format!("async_forfeit_qualifier_{}_{}", team_id, async_kind as i32))
                                                .label("FORFEIT")
                                                .style(ButtonStyle::Secondary),
                                        ]);

                                        interaction.channel_id.send_message(ctx, CreateMessage::new()
                                            .content("**Good luck!** Click the FINISH button once you have completed your run.\nIf you need to forfeit, click the FORFEIT button.")
                                            .components(vec![race_buttons])
                                        ).await?;

                                        // Remove the START button
                                        interaction.edit_response(ctx, EditInteractionResponse::new()
                                            .components(vec![])
                                        ).await?;

                                        transaction.commit().await?;
                                    }
                                }
                            }
                        }
                    } else if let Some(params) = custom_id.strip_prefix("async_finish_qualifier_") {
                        // Handle qualifier FINISH button
                        if let Some((team_id_str, async_kind_str)) = params.split_once('_') {
                            if let (Ok(team_id), Ok(async_kind_int)) = (team_id_str.parse::<u64>(), async_kind_str.parse::<i32>()) {
                                let Some(async_kind) = AsyncKind::from_i32(async_kind_int) else { return Ok(()) };
                                let pool = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").clone();
                                let mut transaction = pool.begin().await?;

                                // Verify user is a member of the team
                                let is_team_member = sqlx::query_scalar!(
                                    r#"SELECT EXISTS (
                                        SELECT 1 FROM team_members tm
                                        JOIN users u ON tm.member = u.id
                                        WHERE tm.team = $1 AND u.discord_id = $2
                                    ) AS "exists!""#,
                                    team_id as i64,
                                    interaction.user.id.get() as i64
                                ).fetch_one(&mut *transaction).await?;

                                if !is_team_member {
                                    interaction.create_response(ctx, CreateInteractionResponse::Message(
                                        CreateInteractionResponseMessage::new()
                                            .ephemeral(true)
                                            .content("You are not authorized to click this button.")
                                    )).await?;
                                    transaction.rollback().await?;
                                } else {
                                    // Get start time and check state
                                    let async_team = sqlx::query!(
                                        r#"SELECT start_time, finish_time FROM async_teams WHERE team = $1 AND kind = $2"#,
                                        team_id as i64,
                                        async_kind as _
                                    ).fetch_optional(&mut *transaction).await?;

                                    if let Some(record) = async_team {
                                        if record.start_time.is_none() {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                                CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("You must start the countdown before finishing.")
                                            )).await?;
                                            transaction.rollback().await?;
                                        } else if record.finish_time.is_some() {
                                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                                CreateInteractionResponseMessage::new()
                                                    .ephemeral(true)
                                                    .content("You have already clicked finish.")
                                            )).await?;
                                            transaction.rollback().await?;
                                        } else {
                                            // Calculate estimated finish time
                                            let now = Utc::now();
                                            let start_time = record.start_time.unwrap();
                                            let duration = now.signed_duration_since(start_time);

                                            let total_seconds = duration.num_seconds();
                                            let hours = total_seconds / 3600;
                                            let minutes = (total_seconds % 3600) / 60;
                                            let seconds = total_seconds % 60;
                                            let formatted_time = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

                                            // Replace FINISH/FORFEIT buttons with REVERT button
                                            let revert_button = CreateActionRow::Buttons(vec![
                                                CreateButton::new(format!("async_revert_qualifier_{}_{}", team_id, async_kind as i32))
                                                    .label("REVERT")
                                                    .style(ButtonStyle::Secondary)
                                            ]);

                                            interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                                CreateInteractionResponseMessage::new()
                                                    .content(format!("✅ **Finished in {}**\nYou have 30 seconds to revert if needed.", formatted_time))
                                                    .components(vec![revert_button])
                                            )).await?;

                                            // Spawn a background task to finalize after 30 seconds
                                            let ctx_clone = ctx.clone();
                                            let channel_id = interaction.channel_id;
                                            let message_id = interaction.message.id;

                                            tokio::spawn(async move {
                                                sleep(Duration::from_secs(30)).await;

                                                // Check if the message still has the REVERT button (wasn't clicked)
                                                if let Ok(message) = channel_id.message(&ctx_clone, message_id).await {
                                                    let has_revert_button = message.components.len() == 1
                                                        && message.components.first()
                                                            .map_or(false, |row| row.components.len() == 1)
                                                        && message.content.contains("30 seconds to revert")
                                                        && message.content.contains(&formatted_time);

                                                    if has_revert_button {
                                                        // Remove the REVERT button
                                                        let _ = channel_id.edit_message(&ctx_clone, message_id, serenity::all::EditMessage::new()
                                                            .components(vec![])
                                                        ).await;

                                                        // Send completion message
                                                        let mut msg = MessageBuilder::default();
                                                        msg.push("@here - **Qualifier run complete!**\n\n");
                                                        msg.push(format!("**Estimated finish time:** {}\n\n", formatted_time));
                                                        msg.push("Please provide:\n");
                                                        msg.push("• A link to your VOD/recording\n");
                                                        msg.push("• A screenshot of your final time/collection rate\n\n");
                                                        msg.push("Staff will verify and record your official time using `/result-async`.");

                                                        let _ = channel_id.say(&ctx_clone, msg.build()).await;
                                                    }
                                                }
                                            });

                                            transaction.rollback().await?;
                                        }
                                    } else {
                                        interaction.create_response(ctx, CreateInteractionResponse::Message(
                                            CreateInteractionResponseMessage::new()
                                                .ephemeral(true)
                                                .content("Could not find your qualifier submission.")
                                        )).await?;
                                        transaction.rollback().await?;
                                    }
                                }
                            }
                        }
                    } else if let Some(params) = custom_id.strip_prefix("async_revert_qualifier_") {
                        // Handle qualifier REVERT button - restore FINISH/FORFEIT buttons
                        if let Some((team_id_str, async_kind_str)) = params.split_once('_') {
                            if let (Ok(team_id), Ok(async_kind_int)) = (team_id_str.parse::<u64>(), async_kind_str.parse::<i32>()) {
                                let race_buttons = CreateActionRow::Buttons(vec![
                                    CreateButton::new(format!("async_finish_qualifier_{}_{}", team_id, async_kind_int))
                                        .label("FINISH")
                                        .style(ButtonStyle::Danger),
                                    CreateButton::new(format!("async_forfeit_qualifier_{}_{}", team_id, async_kind_int))
                                        .label("FORFEIT")
                                        .style(ButtonStyle::Secondary),
                                ]);

                                interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                    CreateInteractionResponseMessage::new()
                                        .content("**Finish reverted!** Continue playing and click the FINISH button once you have completed your run.\nIf you need to forfeit, click the FORFEIT button.")
                                        .components(vec![race_buttons])
                                )).await?;
                            }
                        }
                    } else if custom_id == "async_forfeit_qualifier_cancel" {
                        // Cancel the qualifier forfeit
                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .content("Forfeit cancelled.")
                                .components(vec![])
                        )).await?;
                    } else if let Some(params) = custom_id.strip_prefix("async_forfeit_qualifier_confirm_") {
                        // Handle confirmed qualifier forfeit
                        if let Some((team_id_str, async_kind_str)) = params.split_once('_') {
                            if let (Ok(team_id), Ok(_async_kind_int)) = (team_id_str.parse::<u64>(), async_kind_str.parse::<i32>()) {
                                let pool = ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").clone();
                                let mut transaction = pool.begin().await?;

                                // Verify user is a member of the team
                                let is_team_member = sqlx::query_scalar!(
                                    r#"SELECT EXISTS (
                                        SELECT 1 FROM team_members tm
                                        JOIN users u ON tm.member = u.id
                                        WHERE tm.team = $1 AND u.discord_id = $2
                                    ) AS "exists!""#,
                                    team_id as i64,
                                    interaction.user.id.get() as i64
                                ).fetch_one(&mut *transaction).await?;

                                if !is_team_member {
                                    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                        CreateInteractionResponseMessage::new()
                                            .content("You are not authorized to forfeit for this team.")
                                            .components(vec![])
                                    )).await?;
                                    transaction.rollback().await?;
                                } else {
                                    let mut msg = MessageBuilder::default();
                                    msg.push("@here - ");
                                    msg.mention(&interaction.user);
                                    msg.push(" has indicated they want to **forfeit** this qualifier.\n\n");
                                    msg.push("**Organizers:** To confirm this forfeit, use the `/forfeit-async` command in this thread.");

                                    interaction.channel_id.say(ctx, msg.build()).await?;

                                    // Find and edit the message with FINISH/FORFEIT buttons to remove them
                                    // Look for the message that contains "Good luck!" which is sent with the FINISH/FORFEIT buttons
                                    let messages = interaction.channel_id.messages(ctx, serenity::all::GetMessages::new().limit(50)).await?;
                                    for mut message in messages {
                                        // Check if this message contains "Good luck!" and has components
                                        if !message.components.is_empty() && message.content.contains("Good luck!") {
                                            // Edit this message to remove the buttons
                                            let mut edit_msg = serenity::all::EditMessage::new();
                                            edit_msg = edit_msg.components(vec![]);
                                            let _ = message.edit(ctx, edit_msg).await;
                                            break;
                                        }
                                    }

                                    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                                        CreateInteractionResponseMessage::new()
                                            .content("Forfeit request sent. An organizer will confirm it shortly.")
                                            .components(vec![])
                                    )).await?;

                                    transaction.rollback().await?;
                                }
                            }
                        }
                    } else if let Some(params) = custom_id.strip_prefix("async_forfeit_qualifier_") {
                        // Handle qualifier FORFEIT button - show confirmation
                        if let Some((team_id_str, async_kind_str)) = params.split_once('_') {
                            if let (Ok(team_id), Ok(async_kind_int)) = (team_id_str.parse::<u64>(), async_kind_str.parse::<i32>()) {
                                let confirm_button = CreateActionRow::Buttons(vec![
                                    CreateButton::new(format!("async_forfeit_qualifier_confirm_{}_{}", team_id, async_kind_int))
                                        .label("Yes, forfeit")
                                        .style(ButtonStyle::Danger),
                                    CreateButton::new("async_forfeit_qualifier_cancel")
                                        .label("Cancel")
                                        .style(ButtonStyle::Secondary),
                                ]);

                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("**Are you sure you want to forfeit?**\n\nThis will notify the organizers that you are forfeiting this qualifier.")
                                        .components(vec![confirm_button])
                                )).await?;
                            }
                        }
                    } else if let Some(race_id_str) = custom_id.strip_prefix("volunteer_signup_") {
                        // Handle volunteer signup button - shows available roles for the user
                        let (pool, http_client) = {
                            let data = ctx.data.read().await;
                            (
                                data.get::<DbPool>().expect("database connection pool missing from Discord context").clone(),
                                data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
                            )
                        };
                        let mut transaction = pool.begin().await?;

                        let race_id: Id<Races> = match race_id_str.parse::<u64>() {
                            Ok(id) => Id::from(id),
                            Err(_) => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Invalid race ID.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Get the user from Discord ID
                        let user = match User::from_discord(&mut *transaction, interaction.user.id).await? {
                            Some(u) => u,
                            None => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content(format!("You need to link your Discord account on the website first.\nVisit: <{}>", base_uri()))
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Get the race
                        let race = match Race::from_id(&mut transaction, &http_client, race_id).await {
                            Ok(r) => r,
                            Err(_) => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Could not find this race.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Check if event uses custom role bindings or game-level bindings
                        let uses_custom_bindings = sqlx::query_scalar!(
                            r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
                            race.series as _,
                            &race.event
                        )
                        .fetch_optional(&mut *transaction)
                        .await?
                        .unwrap_or(Some(true))
                        .unwrap_or(true);

                        // Get user's approved role requests
                        let approved_requests = if uses_custom_bindings {
                            event::roles::RoleRequest::for_user_and_event(
                                &mut transaction,
                                user.id,
                                race.series,
                                &race.event,
                            ).await?
                        } else {
                            // Get game-level role requests
                            if let Some(game) = crate::game::Game::from_series(&mut transaction, race.series).await? {
                                event::roles::RoleRequest::for_user_and_game(
                                    &mut transaction,
                                    user.id,
                                    game.id,
                                ).await?
                            } else {
                                Vec::new()
                            }
                        };

                        if approved_requests.is_empty() {
                            let apply_url = if uses_custom_bindings {
                                // Event uses custom role bindings - send to event volunteer page
                                format!("{}/event/{}/{}/volunteer-roles", base_uri(), race.series.slug(), race.event)
                            } else {
                                // Event uses game-level bindings - send to game page
                                format!("{}/games/{}", base_uri(), race.series.slug())
                            };

                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content(format!(
                                        "You don't have any confirmed volunteer roles for this event.\nApply at: <{}>",
                                        apply_url
                                    ))
                            )).await?;
                            return Ok(());
                        }

                        // Get role bindings and current signups for this race
                        let bindings = event::roles::EffectiveRoleBinding::for_event(&mut transaction, race.series, &race.event).await?;
                        let signups = event::roles::Signup::for_race(&mut transaction, race_id).await?;

                        // Build buttons for each approved role that's still available
                        let mut buttons = Vec::new();
                        for request in &approved_requests {
                            // Find the matching binding
                            let binding = bindings.iter().find(|b| b.id == request.role_binding_id);
                            if let Some(binding) = binding {
                                if binding.is_disabled {
                                    continue;
                                }

                                // Check if already signed up for this role (only count active signups)
                                let already_signed = signups.iter().any(|s|
                                    s.user_id == user.id &&
                                    s.role_binding_id == binding.id &&
                                    matches!(s.status, event::roles::VolunteerSignupStatus::Pending | event::roles::VolunteerSignupStatus::Confirmed)
                                );
                                if already_signed {
                                    continue;
                                }

                                // Check if role is full (confirmed count >= max)
                                let confirmed_count = signups.iter()
                                    .filter(|s| s.role_binding_id == binding.id && matches!(s.status, event::roles::VolunteerSignupStatus::Confirmed))
                                    .count() as i32;
                                if confirmed_count >= binding.max_count {
                                    continue;
                                }

                                // Add button for this role
                                let label = format!("{} [{}]", binding.role_type_name, binding.language.short_code());
                                buttons.push(
                                    CreateButton::new(format!("volunteer_role_{}_{}", u64::from(race_id), u64::from(binding.id)))
                                        .label(label)
                                        .style(ButtonStyle::Success)
                                );

                                // Discord limit: max 5 buttons per row, we'll use one row
                                if buttons.len() >= 5 {
                                    break;
                                }
                            }
                        }

                        if buttons.is_empty() {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("All roles you're qualified for are either full or you're already signed up.")
                            )).await?;
                            return Ok(());
                        }

                        // Show role selection
                        interaction.create_response(ctx, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Select the role you want to sign up for:")
                                .components(vec![CreateActionRow::Buttons(buttons)])
                        )).await?;

                        transaction.commit().await?;
                    } else if let Some(params) = custom_id.strip_prefix("volunteer_role_") {
                        // Handle volunteer role selection - creates the signup
                        let (pool, http_client) = {
                            let data = ctx.data.read().await;
                            (
                                data.get::<DbPool>().expect("database connection pool missing from Discord context").clone(),
                                data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
                            )
                        };
                        let mut transaction = pool.begin().await?;

                        // Parse race_id and binding_id from custom_id
                        let (race_id_str, binding_id_str) = match params.split_once('_') {
                            Some((r, b)) => (r, b),
                            None => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Invalid button format.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        let race_id: Id<Races> = match race_id_str.parse::<u64>() {
                            Ok(id) => Id::from(id),
                            Err(_) => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Invalid race ID.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        let binding_id: Id<RoleBindings> = match binding_id_str.parse::<u64>() {
                            Ok(id) => Id::from(id),
                            Err(_) => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Invalid role binding ID.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Get the user from Discord ID
                        let user = match User::from_discord(&mut *transaction, interaction.user.id).await? {
                            Some(u) => u,
                            None => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("You need to link your Discord account on the website first.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Get the race (validates it exists)
                        let _race = match Race::from_id(&mut transaction, &http_client, race_id).await {
                            Ok(r) => r,
                            Err(_) => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Could not find this race.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Verify user still has an approved request for this role binding
                        let has_approval = event::roles::RoleRequest::approved_for_user(
                            &mut transaction,
                            binding_id,
                            user.id,
                        ).await?;

                        if !has_approval {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("You don't have permission for this role.")
                            )).await?;
                            return Ok(());
                        }

                        // Check if already signed up
                        let already_signed = event::roles::Signup::active_for_user(
                            &mut transaction,
                            race_id,
                            binding_id,
                            user.id,
                        ).await?;

                        if already_signed {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("You're already signed up for this role on this race.")
                            )).await?;
                            return Ok(());
                        }

                        // Create the signup
                        event::roles::Signup::create(
                            &mut transaction,
                            race_id,
                            binding_id,
                            user.id,
                            None,
                        ).await?;

                        transaction.commit().await?;

                        // Update the volunteer info post to reflect the new signup
                        let _ = volunteer_requests::update_volunteer_post_for_race(
                            &pool,
                            ctx,
                            race_id,
                        ).await;

                        // Confirm success - update the original message to remove buttons
                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .content("Thank you for signing up. You will be informed once your signup has been processed by the team.")
                                .components(vec![])
                        )).await?;
                    } else if let Some(signup_id_str) = custom_id.strip_prefix("volunteer_withdraw_") {
                        // Handle volunteer withdrawal button
                        let (pool, http_client) = {
                            let data = ctx.data.read().await;
                            (
                                data.get::<DbPool>().expect("database connection pool missing from Discord context").clone(),
                                data.get::<HttpClient>().expect("HTTP client missing from Discord context").clone(),
                            )
                        };
                        let mut transaction = pool.begin().await?;

                        // Parse signup ID
                        let signup_id: Id<Signups> = match signup_id_str.parse::<u64>() {
                            Ok(id) => Id::from(id),
                            Err(_) => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("Invalid signup ID.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Get signup from database
                        let signup = match event::roles::Signup::from_id(&mut transaction, signup_id).await? {
                            Some(s) => s,
                            None => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("This signup no longer exists.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        // Verify user owns this signup
                        let user = match User::from_discord(&mut *transaction, interaction.user.id).await? {
                            Some(u) => u,
                            None => {
                                interaction.create_response(ctx, CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .ephemeral(true)
                                        .content("You need to link your Discord account first.")
                                )).await?;
                                return Ok(());
                            }
                        };

                        if signup.user_id != user.id {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("This signup doesn't belong to you.")
                            )).await?;
                            return Ok(());
                        }

                        // Check if already withdrawn
                        if matches!(signup.status, event::roles::VolunteerSignupStatus::Aborted) {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("You have already withdrawn from this race.")
                            )).await?;
                            return Ok(());
                        }

                        // Get race to check if started
                        let race = Race::from_id(&mut transaction, &http_client, signup.race_id).await?;

                        // Check race hasn't started
                        let race_started = match race.schedule {
                            RaceSchedule::Live { start, .. } => start <= Utc::now(),
                            RaceSchedule::Async { start1, start2, start3, .. } => {
                                [start1, start2, start3].iter().filter_map(|s| *s).any(|s| s <= Utc::now())
                            }
                            _ => false,
                        };

                        if race_started {
                            interaction.create_response(ctx, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .ephemeral(true)
                                    .content("Sorry, you cannot withdraw after the race has started.")
                            )).await?;
                            return Ok(());
                        }

                        // Update signup status to Aborted
                        event::roles::Signup::update_status(&mut transaction, signup_id, event::roles::VolunteerSignupStatus::Aborted).await?;
                        transaction.commit().await?;

                        // Update the volunteer info post
                        let _ = volunteer_requests::update_volunteer_post_for_race(
                            &pool,
                            ctx,
                            signup.race_id,
                        ).await;

                        // Respond with confirmation (update the message to remove button)
                        interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .content("✓ You have successfully withdrawn from this race. Thank you for letting us know!")
                                .components(vec![])
                        )).await?;
                    } else {
                        panic!("received message component interaction with unknown custom ID {custom_id:?}")
                    },
                },
                _ => {}
            }
            Ok(())
        }))
        .on_voice_state_update(|ctx, _, new| Box::pin(async move {
            if let Some(source_channel) = new.channel_id {
                if new.guild_id == Some(MULTIWORLD_GUILD) && all::<Element>().any(|region| region.voice_channel() == source_channel) {
                    let target_channel = ctx.data.read().await.get::<Element>().and_then(|regions| regions.get(&new.user_id)).copied().unwrap_or(Element::Light).voice_channel();
                    if source_channel != target_channel {
                        MULTIWORLD_GUILD.move_member(ctx, new.user_id, target_channel).await?;
                    }
                }
            }
            Ok(())
        }))
        .on_guild_member_addition(|ctx, new_member| Box::pin(async move {
                          if let Err(e) = crate::discord_role_manager::handle_member_join(ctx, new_member.guild_id, new_member.user.id).await {
                eprintln!("Failed to handle member join for user {}: {}", new_member.user.id, e);
            }
            Ok(())
        }))
        .task(|ctx_fut, _| async move {
            let db_pool = ctx_fut.read().await.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context").clone();

            let mut shutdown = shutdown;
            // Clean up expired invites every hour
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = crate::discord_role_manager::cleanup_expired_invites(&db_pool).await {
                            eprintln!("Failed to cleanup expired Discord invites: {}", e);
                        }
                    }
                    () = &mut shutdown => break,
                }
            }
            serenity_utils::shut_down(&*ctx_fut.read().await).await;
        })
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Draft(#[from] draft::Error),
    #[error(transparent)] EventData(#[from] event::DataError),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error("attempted to create scheduling thread in Discord guild that hasn't been initialized yet")]
    UninitializedDiscordGuild(GuildId),
    #[error("attempted to create scheduling thread in Discord guild without command IDs")]
    UnregisteredDiscordGuild(GuildId),
}

pub(crate) async fn create_scheduling_thread<'a>(ctx: &DiscordCtx, mut transaction: Transaction<'a, Postgres>, race: &mut Race, game_count: i16) -> Result<Transaction<'a, Postgres>, Error> {
    let event = race.event(&mut transaction).await?;
    let (Some(guild_id), Some(scheduling_channel)) = (event.discord_guild, event.discord_scheduling_channel) else { return Ok(transaction) };
    let command_ids = match ctx.data.read().await.get::<CommandIds>().and_then(|command_ids| command_ids.get(&guild_id).copied()) {
        None => return Err(Error::UninitializedDiscordGuild(guild_id)),
        Some(None) => return Err(Error::UnregisteredDiscordGuild(guild_id)),
        Some(Some(command_ids)) => command_ids,
    };
    let mut title = if_chain! {
        if let French = event.language;
        if let (Some(phase), Some(round)) = (race.phase.as_ref(), race.round.as_ref());
        if let Some(Some(info_prefix)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut *transaction).await?;
        then {
            match race.entrants {
                Entrants::Open | Entrants::Count { .. } => info_prefix,
                Entrants::Named(ref entrants) => format!("{info_prefix} : {entrants}"),
                Entrants::Two([ref team1, ref team2]) => format!(
                    "{info_prefix} : {} vs {}",
                    team1.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                    team2.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                ),
                Entrants::Three([ref team1, ref team2, ref team3]) => format!(
                    "{info_prefix} : {} vs {} vs {}",
                    team1.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                    team2.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                    team3.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                ),
            }
        } else {
            let info_prefix = format!("{}{}{}",
                race.phase.as_deref().unwrap_or(""),
                if race.phase.is_none() || race.round.is_none() { "" } else { " " },
                race.round.as_deref().unwrap_or(""),
            );
            match race.entrants {
                Entrants::Open | Entrants::Count { .. } => if info_prefix.is_empty() { format!("Untitled Race") } else { info_prefix },
                Entrants::Named(ref entrants) => format!("{info_prefix}{}{entrants}", if info_prefix.is_empty() { "" } else { ": " }),
                Entrants::Two([ref team1, ref team2]) => format!(
                    "{info_prefix}{}{} vs {}",
                    if info_prefix.is_empty() { "" } else { ": " },
                    team1.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                    team2.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                ),
                Entrants::Three([ref team1, ref team2, ref team3]) => format!(
                    "{info_prefix}{}{} vs {} vs {}",
                    if info_prefix.is_empty() { "" } else { ": " },
                    team1.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                    team2.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                    team3.name(&mut transaction, ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                ),
            }
        }
    };
    let mut content = MessageBuilder::default();
    if_chain! {
        if let French = event.language;
        if let (Some(phase), Some(round)) = (race.phase.as_ref(), race.round.as_ref());
        if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut *transaction).await?;
        if game_count == 1;
        if event.asyncs_allowed();
        if let None | Some(draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4) = event.draft_kind();
        then {
            for team in race.teams() {
                content.mention_team(&mut transaction, Some(guild_id), team).await?;
                content.push(' ');
            }
            content.push("Bienvenue dans votre ");
            content.push_safe(phase_round);
            content.push(". Veuillez utiliser ");
            content.mention_command(command_ids.schedule, "schedule");
            content.push(" pour schedule votre race en live ou ");
            content.mention_command(command_ids.schedule_async, "schedule-async");
            content.push(" pour schedule votre async. Vous devez insérer un timestamp Discord que vous pouvez créer sur <https://hammertime.cyou/> ou en tapant `@time`.");
        } else {
            for team in race.teams() {
                content.mention_team(&mut transaction, Some(guild_id), team).await?;
                content.push(' ');
            }
            content.push("Welcome to your ");
            if let Some(ref phase) = race.phase {
                content.push_safe(phase.clone());
                content.push(' ');
            }
            if let Some(ref round) = race.round {
                content.push_safe(round.clone());
                content.push(' ');
            }
            content.push("match. Use ");
            if let Some(speedgaming_slug) = &event.speedgaming_slug {
                content.push("<https://speedgaming.org/");
                content.push(speedgaming_slug);
                if game_count > 1 {
                    content.push("/submit> to schedule your races.");
                } else {
                    content.push("/submit> to schedule your race.");
                }
            } else {
                content.mention_command(command_ids.schedule, "schedule");
                if event.asyncs_allowed() {
                    content.push(" to schedule as a live race or ");
                    content.mention_command(command_ids.schedule_async, "schedule-async");
                    content.push(" to schedule as an async. These commands take a Discord timestamp, which you can generate by typing `@time` or at <https://hammertime.cyou/>.");
                } else {
                    content.push(" to schedule your race. This command takes a Discord timestamp, which you can generate by typing `@time` or at <https://hammertime.cyou/>.");
                }
                if game_count > 1 {
                    content.push(" You can use the ");
                    content.push_mono("game:");
                    content.push(" parameter with these commands to schedule subsequent games ahead of time.");
                }
            }
        }
    };
    if title.len() > 100 {
        // Discord thread titles are limited to 100 characters, unclear on specifics, limit to 100 bytes to be safe
        let mut cutoff = 100 - "[…]".len();
        while !title.is_char_boundary(cutoff) { cutoff -= 1 }
        title.truncate(cutoff);
        title.push_str("[…]");
    }
    if let Some(draft_kind) = event.draft_kind() {
        if let Some(ref draft) = race.draft {
            let mut msg_ctx = draft::MessageContext::Discord {
                teams: race.teams().cloned().collect(),
                team: Team::dummy(),
                transaction, guild_id, command_ids,
            };
            content.push_line("");
            content.push_line("");
            content.push(draft.next_step(draft_kind, race.game, &mut msg_ctx).await?.message);
            transaction = msg_ctx.into_transaction();
        }
    }
    if matches!(racetime_bot::Goal::for_event(race.series, &race.event).expect("Goal not found for event"), racetime_bot::Goal::AlttprDe9Bracket | racetime_bot::Goal::AlttprDe9SwissA | racetime_bot::Goal::AlttprDe9SwissB) {
        let alttprde_options = AlttprDeRaceOptions::for_race(ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context"), race, event.round_modes.as_ref()).await;
        content.push_line("");
        content.push_line("");
        if let Some(mode_display) = alttprde_options.mode_display() {
            content.push(format!("This race will be played in {} mode.", mode_display));
        }
        // Mode not yet determined - draft will show separately
    }
    if let racetime_bot::Goal::Crosskeys2025 = racetime_bot::Goal::for_event(race.series, &race.event).expect("Goal not found for event") {
        let crosskeys_options = CrosskeysRaceOptions::for_race(ctx.data.read().await.get::<DbPool>().expect("database connection pool missing from Discord context"), race).await;
        content.push_line("");
        content.push_line("");
        content.push(format!("This race will be played with {} as settings.\n\nThis race will be played with {}.", crosskeys_options.as_seed_options_str(), crosskeys_options.as_race_options_str()));
    }
    race.scheduling_thread = Some(if let Some(ChannelType::Forum) = scheduling_channel.to_channel(ctx).await?.guild().map(|c| c.kind) {
        scheduling_channel.create_forum_post(ctx, CreateForumPost::new(
            title,
            CreateMessage::default().content(content.build()),
        ).auto_archive_duration(AutoArchiveDuration::OneWeek)).await?.id
    } else {
        let thread = scheduling_channel.create_thread(ctx, CreateThread::new(
            title,
        ).kind(ChannelType::PublicThread).auto_archive_duration(AutoArchiveDuration::OneWeek)).await?;
        thread.say(ctx, content.build()).await?;
        thread.id
    });
    Ok(transaction)
}

pub(crate) async fn handle_race(discord_ctx: DiscordCtx, cal_event: cal::Event, event: event::Data<'_>) -> Result<(),Error > {
    // This is a temporary implementation. It checks the race and sees if a seed is rolled.
    // If it is not, it rolls a seed and adds it to the database.
    // If it is, it pulls the seed from the database instead.
    // It posts in the event.discord_organizer_channel channel a link to the seed, the player who is playing in the async, and gives admin instructions.
    // Use previous mechanisms (async channels/etc) to manage race manually.
    // If the race is the second half, remind admin in the message to post the race result when it's over and report it on start.gg
    // Set "notified" on the race to avoid this being called again.

    let discord_ctx = discord_ctx.clone();
    let cal_event = cal_event.clone();
    let event = event.clone();

    let mut transaction = {
        let discord_data = discord_ctx.data.read().await;
        discord_data.get::<DbPool>().expect("database connection pool missing from Discord context").begin().await?
    };

    // Check if this is the second part of an async (seed already exists)
    let is_second_part = cal_event.race.seed.files.is_some();

    // If no seed exists, roll a new one
    if !is_second_part {
        let discord_data = discord_ctx.data.read().await;
        let global_state = discord_data.get::<GlobalState>().expect("Global State missing from Discord context");
        let goal = racetime_bot::Goal::for_event(cal_event.race.series, &cal_event.race.event).expect("Goal not found for event");
        let mut updates = match goal {
            racetime_bot::Goal::AlttprDe9Bracket | racetime_bot::Goal::AlttprDe9SwissA | racetime_bot::Goal::AlttprDe9SwissB => {
                let alttprde_options = AlttprDeRaceOptions::for_race(&global_state.db_pool, &cal_event.race, event.round_modes.as_ref()).await;
                global_state.clone().roll_alttprde9_seed(alttprde_options)
            }
            racetime_bot::Goal::Crosskeys2025 => {
                let crosskeys_options = CrosskeysRaceOptions::for_race(&global_state.db_pool, &cal_event.race).await;
                global_state.clone().roll_crosskeys2025_seed(crosskeys_options)
            }
            racetime_bot::Goal::MysteryD20 => {
                global_state.clone().roll_mysteryd20_seed()
            }
            racetime_bot::Goal::TwwrMainWeekly | racetime_bot::Goal::TwwrMainMiniblins26 => {
                let settings_string = event.settings_string.clone().expect("TWWR event missing settings string");
                let version = goal.rando_version(Some(&event));
                global_state.clone().roll_twwr_seed(Some(version), settings_string, UnlockSpoilerLog::Never)
            }
            _ => unimplemented!("async seed rolling not implemented for this event"),
        };

        // Loop until we get an update saying the seed data is done rolling.
        let seed = loop {
            match updates.recv().await {
                Some(racetime_bot::SeedRollUpdate::Done { seed, .. }) => break seed,
                Some(racetime_bot::SeedRollUpdate::Error(e)) => panic!("error rolling seed: {e} ({e:?})"),
                None => panic!(),
                _ => {}
            }
        };

        match seed.files {
            Some(seed::Files::AlttprDoorRando { ref uuid }) => {
                let (hash1, hash2, hash3, hash4, hash5) = match seed.file_hash {
                    Some([ref hash1, ref hash2, ref hash3, ref hash4, ref hash5]) => (Some(hash1), Some(hash2), Some(hash3), Some(hash4), Some(hash5)),
                    None => (None, None, None, None, None)
                };
                sqlx::query!("UPDATE races SET xkeys_uuid = $1, hash1 = $2, hash2 = $3, hash3 = $4, hash4 = $5, hash5 = $6 WHERE id = $7", uuid, hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _, cal_event.race.id as _,).execute(&mut *transaction).await?;
            }
            Some(seed::Files::TwwrPermalink { ref permalink, ref seed_hash }) => {
                sqlx::query!("UPDATE races SET seed_data = $1 WHERE id = $2", serde_json::json!({ "permalink": permalink, "seed_hash": seed_hash }), cal_event.race.id as _,).execute(&mut *transaction).await?;
            }
            _ => unimplemented!("unexpected seed type for async rolling"),
        }
    }

    for team in cal_event.active_teams() {
        let mut content = MessageBuilder::default();
        content.push("Async starting for ");
        content.mention_team(&mut transaction, event.discord_guild, team).await?;

        if is_second_part {
            content.push(". **This is the second part of the async.** The runner will receive the previously generated seed as soon as they hit the READY button. Please work with them in their async channel in case of issues.");
        } else {
            content.push(". A Seed has been generated and will be distributed to the runner as soon as they hit the READY button. Please work with them in their async channel in case of issues.");
        }

        let msg = content.build();
        if let Some(channel) = event.discord_organizer_channel {
            channel.say(&discord_ctx, msg).await?;
        }
    }

    match cal_event.kind {
        cal::EventKind::Async1 => {
            sqlx::query!("UPDATE races SET async_notified_1 = TRUE WHERE id = $1", cal_event.race.id as _).execute(&mut *transaction).await?;
        },
        cal::EventKind::Async2 => {
            sqlx::query!("UPDATE races SET async_notified_2 = TRUE WHERE id = $1", cal_event.race.id as _).execute(&mut *transaction).await?;
        }
        cal::EventKind::Async3 => {
            sqlx::query!("UPDATE races SET async_notified_3 = TRUE WHERE id = $1", cal_event.race.id as _).execute(&mut *transaction).await?;
        },
        cal::EventKind::Normal => panic!("Why are we having a normal race in an async"),
    };

    transaction.commit().await?;
    Ok(())
}

/// Helper function to find race and async part from thread ID
async fn find_race_from_thread(
    transaction: &mut Transaction<'_, Postgres>,
    thread_id: i64,
) -> Result<Option<(i64, i32)>, sqlx::Error> {
    // Check each async thread column to find which one matches
    let race_row = sqlx::query!(
        r#"
        SELECT id,
               CASE
                   WHEN async_thread1 = $1 THEN 1
                   WHEN async_thread2 = $1 THEN 2
                   WHEN async_thread3 = $1 THEN 3
                   ELSE NULL
               END as async_part
        FROM races
        WHERE async_thread1 = $1 OR async_thread2 = $1 OR async_thread3 = $1
        "#,
        thread_id
    ).fetch_optional(&mut **transaction).await?;

    Ok(race_row.map(|row| (row.id, row.async_part.unwrap_or(0))))
}

/// Helper function to find qualifier team and kind from thread ID
async fn find_qualifier_from_thread(
    transaction: &mut Transaction<'_, Postgres>,
    thread_id: i64,
) -> Result<Option<(Id<Teams>, AsyncKind)>, sqlx::Error> {
    sqlx::query!(
        r#"SELECT team AS "team: Id<Teams>", kind AS "kind: event::AsyncKind"
           FROM async_teams
           WHERE discord_thread = $1"#,
        thread_id
    ).fetch_optional(&mut **transaction).await
        .map(|row| row.map(|r| (r.team, r.kind)))
}

pub(crate) async fn result_async_command(
    ctx: &DiscordCtx,
    interaction: &CommandInteraction,
) -> Result<(), Error> {
    handle_async_command(ctx, interaction, false).await
}

pub(crate) async fn forfeit_async_command(
    ctx: &DiscordCtx,
    interaction: &CommandInteraction,
) -> Result<(), Error> {
    handle_async_command(ctx, interaction, true).await
}

// Helper function for external reporting (start.gg, challonge, etc.)
async fn report_async_race_to_external_platforms(
    ctx: &DiscordCtx,
    race: &Race,
    async_times: &[(i32, Option<PgInterval>)],
    results: &[(i32, Duration)],
) -> Result<(), Error> {
    // --- Begin external reporting code ---
    let cal_event = cal::Event { race: race.clone(), kind: cal::EventKind::Normal };
    let discord_data = ctx.data.read().await;
    let http_client = discord_data.get::<HttpClient>().expect("HTTP client missing from Discord context");
    let startgg_token = discord_data.get::<StartggToken>().expect("start.gg token missing from Discord context");
    let challonge_api_key = discord_data.get::<ChallongeApiKey>().expect("Challonge API key missing from Discord context");
    // Report to start.gg if applicable
    if let Ok(Some(startgg_set_url)) = cal_event.race.startgg_set_url() {
        let mut total_times: Vec<(i32, Option<Duration>)> = results.iter()
            .map(|(part, time)| (*part, Some(*time)))
            .collect();
        for (async_part, finish_time) in async_times {
            if finish_time.is_none() {
                total_times.push((*async_part, None));
            }
        }
        total_times.sort_by(|a, b| {
            match (a.1, b.1) {
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
                (Some(_), None) => Less,
                (None, Some(_)) => Greater,
                (None, None) => Equal,
            }
        });
        if let Some((winner_part, _)) = total_times.first() {
            let winner_team = match winner_part {
                1 => race.teams().next(),
                2 => race.teams().nth(1),
                3 => race.teams().nth(2),
                _ => None,
            };
            if let Some(winner_team) = winner_team {
                if let Some(startgg_id) = &winner_team.startgg_id {
                    let set_id = if let Some(set_id) = startgg_set_url.path_segments()
                        .and_then(|segments| segments.last())
                        .and_then(|last| last.parse::<u64>().ok())
                    {
                        startgg::ID(set_id.to_string())
                    } else {
                        startgg::ID(startgg_set_url.to_string())
                    };
                    match startgg::query_uncached::<startgg::ReportOneGameResultMutation>(
                        http_client,
                        startgg_token,
                        startgg::report_one_game_result_mutation::Variables {
                            set_id,
                            winner_entrant_id: startgg_id.clone(),
                        }
                    ).await {
                        Ok(_) => {},
                        Err(e) => {
                            eprintln!("Failed to report async race result to start.gg: {:?}", e);
                        }
                    }
                }
            }
        }
    }
    // Report to challonge if applicable
    if let cal::Source::Challonge { ref id } = cal_event.race.source {
        let mut total_times: Vec<(i32, Option<Duration>)> = results.iter()
            .map(|(part, time)| (*part, Some(*time)))
            .collect();
        for (async_part, finish_time) in async_times {
            if finish_time.is_none() {
                total_times.push((*async_part, None));
            }
        }
        total_times.sort_by(|a, b| {
            match (a.1, b.1) {
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
                (Some(_), None) => Less,
                (None, Some(_)) => Greater,
                (None, None) => Equal,
            }
        });
        if let Some((winner_part, _)) = total_times.first() {
            let winner_team = match winner_part {
                1 => race.teams().next(),
                2 => race.teams().nth(1),
                3 => race.teams().nth(2),
                _ => None,
            };
            if let Some(winner_team) = winner_team {
                let match_id = id.clone();
                let winner_id = winner_team.challonge_id.clone();
                if let Some(winner_id) = winner_id {
                    let endpoint = format!("https://api.challonge.com/v2/matches/{}/report", match_id);
                    let payload = serde_json::json!({
                        "match": {
                            "winner_id": winner_id,
                            "scores_csv": "1-0"
                        }
                    });
                    match http_client.put(&endpoint)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .header(reqwest::header::CONTENT_TYPE, "application/vnd.api+json")
                        .header("Authorization-Type", "v1")
                        .header(reqwest::header::AUTHORIZATION, challonge_api_key)
                        .json(&payload)
                        .send()
                        .await {
                            Ok(_) => {},
                            Err(e) => {
                                eprintln!("Failed to report async race result to challonge: {:?}", e);
                            }
                        }
                }
            }
        }
    }
    Ok(())
}

fn get_display_order(race: &Race, async_part: i32) -> i32 {
    match &race.schedule {
        RaceSchedule::Async { start1, start2, start3, .. } => {
            // Get all scheduled start times that are not None
            let mut scheduled_times = Vec::new();
            if let Some(time) = start1 { scheduled_times.push((1, *time)); }
            if let Some(time) = start2 { scheduled_times.push((2, *time)); }
            if let Some(time) = start3 { scheduled_times.push((3, *time)); }

            // Sort by start time
            scheduled_times.sort_by_key(|&(_, time)| time);

            // Find the position of this async part in the sorted list
            if let Some(position) = scheduled_times.iter().position(|&(part, _)| part == async_part as u8) {
                (position + 1) as i32 // Convert to 1-based display order
            } else {
                // Fallback to async_part number if not found
                async_part
            }
        }
        _ => async_part, // Fallback
    }
}

async fn handle_async_command(
    ctx: &DiscordCtx,
    interaction: &CommandInteraction,
    is_forfeit: bool,
) -> Result<(), Error> {
    // Defer the response immediately to prevent timeout
    interaction.create_response(ctx, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()
        .ephemeral(false)
    )).await?;

    let mut transaction = ctx.data.read().await.get::<DbPool>().as_ref().expect("database connection pool missing from Discord context").begin().await?;

    // Check if user is an organizer
    let user_id = interaction.user.id;
    let is_organizer = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM organizers eo
            JOIN users u ON eo.organizer = u.id
            WHERE u.discord_id = $1
        ) as "exists!"
        "#,
        user_id.get() as i64
    ).fetch_one(&mut *transaction).await?.exists;

    if !is_organizer {
        interaction.edit_response(ctx, EditInteractionResponse::new()
            .content("You must be an event organizer to use this command.")
        ).await?;
        transaction.rollback().await?;
        return Ok(());
    }

    // Try to get race_id and async_part from command options first (for backward compatibility)
    let (race_id, async_part) = if let (Some(race_id_opt), Some(async_part_opt)) = (
        interaction.data.options.iter()
            .find(|opt| opt.name == "race_id")
            .and_then(|opt| opt.value.as_str())
            .and_then(|s| s.parse::<i64>().ok()),
        interaction.data.options.iter()
            .find(|opt| opt.name == "async_part")
            .and_then(|opt| match opt.value {
                CommandDataOptionValue::Integer(part) => Some(part),
                _ => None,
            })
    ) {
        (race_id_opt, async_part_opt)
    } else {
        // Try to get from thread context
        let thread_id = interaction.channel_id.get() as i64;
        match find_race_from_thread(&mut transaction, thread_id).await? {
            Some((race_id, async_part)) => (race_id, async_part as i64),
            None => {
                // Check if it's a qualifier thread
                if let Some((team_id, async_kind)) = find_qualifier_from_thread(&mut transaction, thread_id).await? {
                    // Check if request has link parameter
                    let link: Option<String> = interaction.data.options.iter()
                        .find(|opt| opt.name == "link")
                        .and_then(|opt| opt.value.as_str())
                        .map(|s| s.to_string());

                    let team = Team::from_id(&mut transaction, team_id).await?.ok_or(sqlx::Error::RowNotFound)?;
                    let team_name = team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into());

                    if is_forfeit {
                        sqlx::query!("UPDATE async_teams SET submitted = NOW(), finish_time = NULL WHERE team = $1 AND kind = $2", team_id as _, async_kind as _).execute(&mut *transaction).await?;

                        // Insert forfeit records into async_players so they're counted
                        let members = team.members(&mut transaction).await?;
                        for member in members {
                            sqlx::query!(
                                "INSERT INTO async_players (series, event, player, kind, time, vod) VALUES ($1, $2, $3, $4, NULL, NULL) ON CONFLICT (series, event, player, kind) DO UPDATE SET time = EXCLUDED.time, vod = EXCLUDED.vod",
                                team.series as _,
                                team.event,
                                member.id as _,
                                async_kind as _
                            ).execute(&mut *transaction).await?;
                        }

                        interaction.edit_response(ctx, EditInteractionResponse::new()
                            .content(format!("Forfeit recorded for {}.", team_name))
                        ).await?;
                    } else {
                        let time_str = interaction.data.options.iter()
                            .find(|opt| opt.name == "time")
                            .and_then(|opt| opt.value.as_str())
                            .ok_or_else(|| Error::Sql(sqlx::Error::RowNotFound))?;

                        let time_parts: Vec<&str> = time_str.split(':').collect();
                        if time_parts.len() != 3 {
                            interaction.edit_response(ctx, EditInteractionResponse::new()
                                .content("Time must be in format hh:mm:ss")
                            ).await?;
                            transaction.rollback().await?;
                            return Ok(());
                        }

                        let hours: i32 = time_parts[0].parse().map_err(|_| Error::Sql(sqlx::Error::RowNotFound))?;
                        let minutes: i32 = time_parts[1].parse().map_err(|_| Error::Sql(sqlx::Error::RowNotFound))?;
                        let seconds: i32 = time_parts[2].parse().map_err(|_| Error::Sql(sqlx::Error::RowNotFound))?;

                        let total_seconds = hours * 3600 + minutes * 60 + seconds;
                        let milliseconds = (total_seconds as i64) * 1_000;
                        let pg_interval = PgInterval {
                            months: 0,
                            days: 0,
                            microseconds: milliseconds * 1_000,
                        };

                        sqlx::query!("UPDATE async_teams SET submitted = NOW(), finish_time = $1 WHERE team = $2 AND kind = $3", pg_interval, team_id as _, async_kind as _).execute(&mut *transaction).await?;

                        let members = team.members(&mut transaction).await?;
                        for member in members {
                             sqlx::query!(
                                "INSERT INTO async_players (series, event, player, kind, time, vod) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (series, event, player, kind) DO UPDATE SET time = EXCLUDED.time, vod = EXCLUDED.vod",
                                team.series as _,
                                team.event,
                                member.id as _,
                                async_kind as _,
                                pg_interval,
                                link
                            ).execute(&mut *transaction).await?;
                        }

                        interaction.edit_response(ctx, EditInteractionResponse::new()
                            .content(format!("Time recorded for {}: {}", team_name, time_str))
                        ).await?;
                    }

                    transaction.commit().await?;
                    return Ok(());
                }

                interaction.edit_response(ctx, EditInteractionResponse::new()
                    .content("This command must be used in an async race thread, or you must provide race_id and async_part parameters.")
                ).await?;
                transaction.rollback().await?;
                return Ok(());
            }
        }
    };

    // Get the optional link parameter
    let link: Option<String> = interaction.data.options.iter()
        .find(|opt| opt.name == "link")
        .and_then(|opt| opt.value.as_str())
        .map(|s| s.to_string());

    // Get the user who ran the command
    let user = User::from_discord(&mut *transaction, user_id).await?.ok_or_else(|| Error::Sql(sqlx::Error::RowNotFound))?;

    // Load race data early so we can use it for display order
    let race = Race::from_id(&mut transaction, &reqwest::Client::new(), Id::from(race_id as u64)).await.map_err(|_e| Error::Sql(sqlx::Error::RowNotFound))?;

    if is_forfeit {
        // Record forfeit (finish_time = NULL)
        sqlx::query!(
            r#"
            INSERT INTO async_times (race_id, async_part, finish_time, recorded_by, link)
            VALUES ($1, $2, NULL, $3, $4)
            ON CONFLICT (race_id, async_part) DO UPDATE SET
                finish_time = NULL,
                recorded_at = NOW(),
                recorded_by = EXCLUDED.recorded_by,
                link = EXCLUDED.link
            "#,
            race_id,
            async_part as i32,
            user.id as _,
            link,
        ).execute(&mut *transaction).await?;

        // Send immediate response
        let display_order = get_display_order(&race, async_part as i32);
        let ordinal = match display_order {
            1 => "1st",
            2 => "2nd",
            3 => "3rd",
            n => &format!("{}th", n),
        };

        let content = format!("Forfeit recorded for {} half of this async.", ordinal);

        interaction.edit_response(ctx, EditInteractionResponse::new()
            .content(content)
        ).await?;
    } else {
        // Record time result
        let time_str = interaction.data.options.iter()
            .find(|opt| opt.name == "time")
            .and_then(|opt| opt.value.as_str())
            .ok_or_else(|| Error::Sql(sqlx::Error::RowNotFound))?;

        // Parse the time (format: hh:mm:ss)
        let time_parts: Vec<&str> = time_str.split(':').collect();
        if time_parts.len() != 3 {
            interaction.edit_response(ctx, EditInteractionResponse::new()
                .content("Time must be in format hh:mm:ss")
            ).await?;
            transaction.rollback().await?;
            return Ok(());
        }

        let hours: i32 = time_parts[0].parse().map_err(|_| Error::Sql(sqlx::Error::RowNotFound))?;
        let minutes: i32 = time_parts[1].parse().map_err(|_| Error::Sql(sqlx::Error::RowNotFound))?;
        let seconds: i32 = time_parts[2].parse().map_err(|_| Error::Sql(sqlx::Error::RowNotFound))?;

        let total_seconds = hours * 3600 + minutes * 60 + seconds;

        // Insert the async time
        let pg_interval = PgInterval {
            months: 0,
            days: 0,
            microseconds: (total_seconds as i64) * 1_000_000,
        };

        sqlx::query!(
            r#"
            INSERT INTO async_times (race_id, async_part, finish_time, recorded_by, link)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (race_id, async_part) DO UPDATE SET
                finish_time = EXCLUDED.finish_time,
                recorded_at = NOW(),
                recorded_by = EXCLUDED.recorded_by,
                link = EXCLUDED.link
            "#,
            race_id,
            async_part as i32,
            pg_interval,
            user.id as _,
            link,
        ).execute(&mut *transaction).await?;

        // Send immediate response
        let display_order = get_display_order(&race, async_part as i32);
        let ordinal = match display_order {
            1 => "1st",
            2 => "2nd",
            3 => "3rd",
            n => &format!("{}th", n),
        };

        let content = format!("Time recorded for {} half of this async: {}", ordinal, time_str);

        interaction.edit_response(ctx, EditInteractionResponse::new()
            .content(content)
        ).await?;
    }

    // Check if both async parts are complete (only count records that have been properly recorded)
    let async_times = sqlx::query!(
        r#"
        SELECT async_part, finish_time, link FROM async_times
        WHERE race_id = $1 AND recorded_by IS NOT NULL
        ORDER BY async_part
        "#,
        race_id
    ).fetch_all(&mut *transaction).await?;

    let expected_parts = race.teams().count();
    if async_times.len() >= expected_parts {
        // All parts are complete, finalize the race
        let event_name = race.event.clone();
        let event = event::Data::new(&mut transaction, race.series, &event_name).await?
            .ok_or_else(|| Error::Sql(sqlx::Error::RowNotFound))?;

        // Update race end times
        for async_time in &async_times {
            // Calculate end time based on start time + finish time
            let start_time = match async_time.async_part {
                1 => sqlx::query_scalar!("SELECT async_start1 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
                2 => sqlx::query_scalar!("SELECT async_start2 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
                3 => sqlx::query_scalar!("SELECT async_start3 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
                _ => continue,
            };

            if let Some(start_time) = start_time {
                // Calculate finish time in seconds
                if let Some(finish_time) = &async_time.finish_time {
                    let finish_seconds = finish_time.microseconds / 1_000_000
                        + (finish_time.days as i64) * 86400
                        + (finish_time.months as i64) * 30 * 86400;

                    let end_time = start_time + chrono::Duration::seconds(finish_seconds);

                    match async_time.async_part {
                        1 => sqlx::query!("UPDATE races SET async_end1 = $1 WHERE id = $2", end_time, race_id).execute(&mut *transaction).await?,
                        2 => sqlx::query!("UPDATE races SET async_end2 = $1 WHERE id = $2", end_time, race_id).execute(&mut *transaction).await?,
                        3 => sqlx::query!("UPDATE races SET async_end3 = $1 WHERE id = $2", end_time, race_id).execute(&mut *transaction).await?,
                        _ => return Ok(()),
                    };
                }
            }
        }

        // Report the results
        let results = async_times.iter().filter_map(|at| {
            // finish_time is Option<PgInterval>, calculate total seconds
            if let Some(finish_time) = &at.finish_time {
                let seconds = finish_time.microseconds / 1_000_000
                    + (finish_time.days as i64) * 86400
                    + (finish_time.months as i64) * 30 * 86400;
                Some((at.async_part, Duration::from_secs(seconds as u64)))
            } else {
                None // Skip records without finish times (forfeits)
            }
        }).collect::<Vec<_>>();

        // Find the winning and losing players
        let mut total_times: Vec<(i32, Option<Duration>, &Team)> = results.iter()
            .map(|(part, time)| {
                let team = match part {
                    1 => race.teams().next(),
                    2 => race.teams().nth(1),
                    3 => race.teams().nth(2),
                    _ => None,
                };
                (*part, Some(*time), team.unwrap())
            })
            .collect();

        // Add forfeiting players with None time
        for async_time in &async_times {
            if async_time.finish_time.is_none() {
                let team = match async_time.async_part {
                    1 => race.teams().next(),
                    2 => race.teams().nth(1),
                    3 => race.teams().nth(2),
                    _ => None,
                };
                if let Some(team) = team {
                    total_times.push((async_time.async_part, None, team));
                }
            }
        }

        total_times.sort_by(|a, b| {
            // Sort by finish time, with None (forfeits) coming last
            match (a.1, b.1) {
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
                (Some(_), None) => Less,
                (None, Some(_)) => Greater,
                (None, None) => Equal,
            }
        });

        let (_winner_part, winner_time, winner_team) = &total_times[0];
        let (_loser_part, loser_time, loser_team) = &total_times[1];

        // Get player names
        let winner_player = winner_team.members(&mut transaction).await?.into_iter().next()
            .ok_or_else(|| Error::Sql(sqlx::Error::RowNotFound))?;
        let loser_player = loser_team.members(&mut transaction).await?.into_iter().next()
            .ok_or_else(|| Error::Sql(sqlx::Error::RowNotFound))?;

        // Format the results message like live races
        let mut content = MessageBuilder::default();
        content.push("Async results for ");

        if let Some(phase) = &race.phase {
            content.push_safe(phase.clone());
            content.push(' ');
        }
        if let Some(round) = &race.round {
            content.push_safe(round.clone());
            content.push(' ');
        }

        content.mention_user(&winner_player);
        content.push(" (");
        if let Some(winner_time) = winner_time {
            content.push(format!("{:02}:{:02}:{:02}",
                winner_time.as_secs() / 3600,
                (winner_time.as_secs() % 3600) / 60,
                winner_time.as_secs() % 60
            ));
        } else { // this should never happen.
            content.push("DNF");
        }
        content.push(") defeats ");
        content.mention_user(&loser_player);
        content.push(" (");
        if let Some(loser_time) = loser_time {
            content.push(format!("{:02}:{:02}:{:02}",
                loser_time.as_secs() / 3600,
                (loser_time.as_secs() % 3600) / 60,
                loser_time.as_secs() % 60
            ));
        } else {
            content.push("DNF");
        }
        content.push(")");

        // Add links if available
        let mut links_content = MessageBuilder::default();
        let mut has_links = false;

        for async_time in &async_times {
            if let Some(link) = &async_time.link {
                if !has_links {
                    links_content.push_line("");
                    links_content.push("**Recordings:**");
                    has_links = true;
                }
                links_content.push_line("");
                let player = match async_time.async_part {
                    1 => race.teams().next(),
                    2 => race.teams().nth(1),
                    3 => race.teams().nth(2),
                    _ => None,
                };
                if let Some(player) = player {
                    let player_name = player.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Player".to_string().into());
                    links_content.push_safe(player_name);
                    links_content.push(": <");
                    links_content.push(link);
                    links_content.push(">");
                }
            }
        }

        if has_links {
            content.push(links_content.build());
        }

        // Send to race results channel
        if let Some(results_channel) = event.discord_race_results_channel {
            results_channel.say(ctx, content.build()).await?;
        }

        // Send to scheduling thread
        if let Some(scheduling_thread) = race.scheduling_thread {
            scheduling_thread.say(ctx, content.build()).await?;
        }

        // Extract the fields we need for external reporting
        let async_times_parsed: Vec<(i32, Option<PgInterval>)> = async_times.iter()
            .map(|at| (at.async_part, at.finish_time.clone()))
            .collect();

        if let Err(e) = report_async_race_to_external_platforms(ctx, &race, &async_times_parsed, &results).await {
            transaction.rollback().await?;
            return Err(e);
        }
    }

    transaction.commit().await?;
    Ok(())
}
