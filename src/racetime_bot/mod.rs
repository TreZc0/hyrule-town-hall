use {
    std::{
        io::prelude::*,
        process::Stdio,
        sync::atomic::{
            self,
            AtomicBool,
            AtomicUsize,
        },
    },
    kuchiki::{
        NodeRef,
        traits::TendrilSink as _,
    },
    lazy_regex::regex_captures,
    mhstatus::OpenRoom,
    ootr_utils as rando,
    ootr_utils::spoiler::OcarinaNote,
    racetime::{
        Error,
        ResultExt as _,
        handler::{
            RaceContext,
            RaceHandler,
        },
        model::*,
    },
    rand::distr::{
        Alphanumeric,
        SampleString as _,
    },
    reqwest::StatusCode,
    semver::Version,
    serenity::all::{
        CreateAllowedMentions,
        CreateMessage,
    },
    smart_default::SmartDefault,
    tokio::{
        io::{
            AsyncBufReadExt as _,
            AsyncWriteExt as _,
            BufReader,
        },
        time::timeout,
    },
    wheel::{
        fs::File,
        traits::IoResultExt as _,
    },
    crate::{
        cal::Entrant,
        config::{Config, ConfigRaceTime},
        discord_bot::{ADMIN_USER, PgSnowflake},
        game::GameRacetimeConnection,
        hash_icon_db::HashIconData,
        prelude::*,
        weekly::WeeklySchedule,
    },
};
#[cfg(unix)] use async_proto::Protocol;
#[cfg(windows)] use directories::UserDirs;

mod report;
pub(crate) mod seed_gen_type;

#[cfg(unix)] const PYTHON: &str = "python3";
#[cfg(windows)] const PYTHON: &str = "py";

pub(crate) const CATEGORY: &str = "alttpr";

const OOTR_DISCORD_GUILD: GuildId = GuildId::new(274180765816848384);

static RSL_SEQUENCE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseUserError {
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("this seems to be neither a URL, nor a racetime.gg user ID, nor a Hyrule Town Hall user ID")]
    Format,
    #[error("there is no racetime.gg user with this ID (error 404)")]
    IdNotFound,
    #[error("this URL is not a racetime.gg user profile URL")]
    InvalidUrl,
    #[error("there is no Hyrule Town Hall user with this ID")]
    MidosHouseId,
    #[error("There is no racetime.gg account associated with this Hyrule Town Hall account. Ask the user to go to their profile and select “Connect a racetime.gg account”. You can also link to their racetime.gg profile directly.")]
    MidosHouseUserNoRacetime,
    #[error("there is no racetime.gg user with this URL (error 404)")]
    UrlNotFound,
}

/// Returns `None` if the user data can't be accessed. This may be because the user ID does not exist, or because the user profile is not public, see https://github.com/racetimeGG/racetime-app/blob/5892f8f80eb1bd9619244becc48bbc4607b76844/racetime/models/user.py#L274-L296
pub(crate) async fn user_data(http_client: &reqwest::Client, user_id: &str) -> wheel::Result<Option<UserProfile>> {
    match http_client.get(format!("https://{}/user/{user_id}/data", racetime_host()))
        .send().await?
        .detailed_error_for_status().await
    {
        Ok(response) => response.json_with_text_in_error().await.map(Some),
        Err(wheel::Error::ResponseStatus { inner, .. }) if inner.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
    }
}

pub(crate) async fn parse_user(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, id_or_url: &str) -> Result<String, ParseUserError> {
    if let Ok(id) = id_or_url.parse() {
        return if let Some(user) = User::from_id(&mut **transaction, id).await? {
            if let Some(racetime) = user.racetime {
                Ok(racetime.id)
            } else {
                Err(ParseUserError::MidosHouseUserNoRacetime)
            }
        } else {
            Err(ParseUserError::MidosHouseId)
        }
    }
    if regex_is_match!("^[0-9A-Za-z]+$", id_or_url) {
        return match user_data(http_client, id_or_url).await {
            Ok(Some(user_data)) => Ok(user_data.id),
            Ok(None) => Err(ParseUserError::IdNotFound),
            Err(e) => Err(e.into()),
        }
    }
    if let Ok(url) = Url::parse(id_or_url) {
        return if_chain! {
            if let Some("racetime.gg" | "www.racetime.gg") = url.host_str();
            if let Some(mut path_segments) = url.path_segments();
            if path_segments.next() == Some("user");
            if let Some(url_part) = path_segments.next();
            then {
                match user_data(http_client, url_part).await {
                    Ok(Some(user_data)) => Ok(user_data.id),
                    Ok(None) => Err(ParseUserError::UrlNotFound),
                    Err(e) => Err(e.into()),
                }
            } else {
                Err(ParseUserError::InvalidUrl)
            }
        }
    }
    Err(ParseUserError::Format)
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(unix, derive(Protocol))]
#[cfg_attr(unix, async_proto(via = String))]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum VersionedBranch {
    Pinned {
        version: rando::Version,
    },
    Latest {
        branch: rando::Branch,
    },
    #[serde(rename_all = "camelCase")]
    Custom {
        github_username: Cow<'static, str>,
        branch: Cow<'static, str>,
    },
    #[serde(rename_all = "camelCase")]
    Tww {
        identifier: Cow<'static, str>,
        github_url: Cow<'static, str>,
        #[serde(default)]
        tracker_link: Option<Cow<'static, str>>,
    },
}

#[cfg(unix)]
impl From<VersionedBranch> for String {
    fn from(branch: VersionedBranch) -> Self {
        String::from(&branch)
    }
}

#[cfg(unix)]
impl From<&VersionedBranch> for String {
    fn from(branch: &VersionedBranch) -> Self {
        use serde_json::json;
        let value = match branch {
            VersionedBranch::Pinned { version } => json!({
                "type": "pinned",
                "version": version.to_string()
            }),
            VersionedBranch::Latest { branch } => json!({
                "type": "latest",
                "branch": branch
            }),
            VersionedBranch::Custom { github_username, branch } => json!({
                "type": "custom",
                "githubUsername": github_username,
                "branch": branch
            }),
            VersionedBranch::Tww { identifier, github_url, tracker_link } => json!({
                "type": "tww",
                "identifier": identifier,
                "githubUrl": github_url,
                "trackerLink": tracker_link
            }),
        };
        serde_json::to_string(&value).expect("failed to serialize VersionedBranch")
    }
}

#[cfg(unix)]
impl From<String> for VersionedBranch {
    fn from(s: String) -> Self {
        use serde_json::Value;
        let value: Value = serde_json::from_str(&s).expect("failed to parse JSON");
        let type_str = value.get("type").and_then(|v| v.as_str()).expect("missing type field");
        match type_str {
            "pinned" => {
                let version_str = value.get("version").and_then(|v| v.as_str()).expect("missing version field");
                let version = version_str.parse().expect("failed to parse version");
                VersionedBranch::Pinned { version }
            }
            "latest" => {
                let branch = serde_json::from_value(value.get("branch").cloned().expect("missing branch field")).expect("failed to parse branch");
                VersionedBranch::Latest { branch }
            }
            "custom" => {
                let github_username = value.get("githubUsername").and_then(|v| v.as_str()).expect("missing githubUsername field").to_owned().into();
                let branch = value.get("branch").and_then(|v| v.as_str()).expect("missing branch field").to_owned().into();
                VersionedBranch::Custom { github_username, branch }
            }
            "tww" => {
                let identifier = value.get("identifier").and_then(|v| v.as_str()).expect("missing identifier field").to_owned().into();
                let github_url = value.get("githubUrl").and_then(|v| v.as_str()).expect("missing githubUrl field").to_owned().into();
                let tracker_link = value.get("trackerLink").and_then(|v| v.as_str()).map(|s| s.to_owned().into());
                VersionedBranch::Tww { identifier, github_url, tracker_link }
            }
            _ => panic!("unknown VersionedBranch type: {}", type_str),
        }
    }
}

impl VersionedBranch {
    pub(crate) fn branch(&self) -> Option<rando::Branch> {
        match self {
            Self::Pinned { version } => Some(version.branch()),
            Self::Latest { branch } => Some(*branch),
            Self::Custom { .. } | Self::Tww { .. } => None,
        }
    }
}

/// Determines how early the bot may start generating the seed for an official race.
///
/// There are two factors to consider here:
///
/// 1. If we start rolling the seed too late, players may have to wait for the seed to become available, which may delay the start of the race.
/// 2. If we start rolling the seed too early, players may be able to cheat by finding the seed's sequential ID on ootrandomizer.com
///    or by finding the seed in the list of recently rolled seeds on triforceblitz.com.
///    This is not an issue for seeds rolled locally, so the local generator will always be started immediately after the room is opened.
///
/// How early we should start rolling seeds therefore depends on how long seed generation is expected to take, which depends on the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PrerollMode {
    /// Do not preroll seeds.
    None,
    /// Preroll seeds within the 5 minutes before the deadline.
    Short,
    /// Start prerolling seeds between the time the room is opened and 15 minutes before the deadline.
    Medium,
    /// Always keep one seed in reserve until the end of the event. Fetch that seed or start rolling a new one immediately as the room is opened.
    Long,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) enum UnlockSpoilerLog {
    Now,
    Progression,
    After,
    Never,
}

pub(crate) enum SeedCommandParseResult {
    Alttpr,
    Rsl {
        preset: rsl::VersionedPreset,
        world_count: u8,
        unlock_spoiler_log: UnlockSpoilerLog,
        language: Language,
        article: &'static str,
        description: String,
    },
    Tfb {
        version: &'static str,
        unlock_spoiler_log: UnlockSpoilerLog,
        language: Language,
        article: &'static str,
        description: String,
    },
    Twwr {
        permalink: String,
        unlock_spoiler_log: UnlockSpoilerLog,
        language: Language,
        article: &'static str,
        description: String,
    },
    TfbDev {
        coop: bool,
        unlock_spoiler_log: UnlockSpoilerLog,
        language: Language,
        article: &'static str,
        description: String,
    },
    QueueExisting {
        data: seed::Data,
        language: Language,
        article: &'static str,
        description: String,
    },
    SendPresets {
        language: Language,
        msg: &'static str,
    },
    StartDraft {
        new_state: Draft,
        unlock_spoiler_log: UnlockSpoilerLog,
    },
    Error {
        language: Language,
        msg: Cow<'static, str>,
    },
}


impl seed_gen_type::SeedGenType {
    async fn send_presets(&self, ctx: &RaceContext<GlobalState>) -> Result<(), Error> {
        match self {
            Self::AlttprDoorRando { .. } | Self::AlttprAvianart => {
                ctx.say("!seed: Rolls this race’s seed.").await?;
            }
            Self::OotrTriforceBlitz => {
                ctx.say("!seed s4coop: Triforce Blitz season 4 co-op settings").await?;
                ctx.say("!seed s3: Triforce Blitz season 3 settings").await?;
                ctx.say("!seed jr: Jabu’s Revenge").await?;
                ctx.say("!seed s2: Triforce Blitz season 2 settings").await?;
                ctx.say("!seed daily: Triforce Blitz Seed of the Day").await?;
            }
            Self::OotrRsl => {
                for preset in all::<rsl::Preset>() {
                    ctx.say(format!("!seed{}: {}", match preset {
                        rsl::Preset::League => String::default(),
                        rsl::Preset::Multiworld => format!(" {} <worldcount>", preset.name()),
                        _ => format!(" {}", preset.name()),
                    }, match preset {
                        rsl::Preset::League => "official Random Settings League weights",
                        rsl::Preset::Beginner => "random settings for beginners, see https://zsr.link/mKzPO for details",
                        rsl::Preset::Intermediate => "a step between Beginner and League",
                        rsl::Preset::Ddr => "League but always normal damage and with cutscenes useful for tricks in the DDR ruleset",
                        rsl::Preset::CoOp => "weights tuned for co-op play",
                        rsl::Preset::Multiworld => "weights tuned for multiworld",
                    })).await?;
                }
                ctx.say("!seed draft: Pick the weights here in the chat.").await?;
                ctx.say("!seed draft lite: Pick the weights here in the chat, but limit picks to RSL-Lite.").await?;
            }
            Self::TWWR { .. } => {
                ctx.say("!seed: The permalink validation hash for the race.").await?;
            }
            Self::OoTR | Self::Mmr => {
                ctx.say("!seed: The settings used for this event.").await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn parse_seed_command(
        &self,
        _transaction: &mut Transaction<'_, Postgres>,
        global_state: &GlobalState,
        is_official: bool,
        spoiler_seed: bool,
        _no_password: bool,
        args: &[String],
    ) -> Result<SeedCommandParseResult, Error> {
        let unlock_spoiler_log = if spoiler_seed {
            UnlockSpoilerLog::Now
        } else if is_official {
            UnlockSpoilerLog::After
        } else {
            UnlockSpoilerLog::Never
        };
        Ok(match self {
            Self::AlttprDoorRando { .. } | Self::AlttprAvianart => match args {
                [] => SeedCommandParseResult::Alttpr,
                [arg] if arg == "base" => SeedCommandParseResult::Alttpr,
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn’t quite understand that" },
            },
            Self::OotrTriforceBlitz => match args {
                [] => SeedCommandParseResult::SendPresets { language: English, msg: "the preset is required" },
                [arg] if arg == "daily" => {
                    let (date, ordinal, file_hash) = {
                        let response = global_state.http_client
                            .get("https://www.triforceblitz.com/seed/daily/all")
                            .send().await?
                            .detailed_error_for_status().await.to_racetime()?;
                        let response_body = response.text().await?;
                        let latest = kuchiki::parse_html().one(response_body)
                            .select_first("main > section > div > div").map_err(|()| RollError::TfbHtml).to_racetime()?;
                        let latest = latest.as_node();
                        let a = latest.select_first("a").map_err(|()| RollError::TfbHtml).to_racetime()?;
                        let a_attrs = a.attributes.borrow();
                        let href = a_attrs.get("href").ok_or(RollError::TfbHtml).to_racetime()?;
                        let (_, ordinal) = regex_captures!("^/seed/daily/([0-9]+)$", href).ok_or(RollError::TfbHtml).to_racetime()?;
                        let ordinal = ordinal.parse().to_racetime()?;
                        let date = NaiveDate::parse_from_str(&a.text_contents(), "%B %-d, %Y").to_racetime()?;
                        let file_hash = latest.select_first(".hash-icons").map_err(|()| RollError::TfbHtml).to_racetime()?
                            .as_node()
                            .children()
                            .filter_map(NodeRef::into_element_ref)
                            .filter_map(|elt| elt.attributes.borrow().get("title").and_then(|title| title.parse().ok()))
                            .collect_vec()
                            .try_into().map_err(|_| RollError::TfbHtml).to_racetime()?;
                        (date, ordinal, file_hash)
                    };
                    SeedCommandParseResult::QueueExisting {
                        data: seed::Data {
                            file_hash: Some(file_hash),
                            password: None,
                            seed_data: Some(seed::Files::TfbSotd { date, ordinal }.to_seed_data_base()),
                            progression_spoiler: false,
                        },
                        language: English,
                        article: "the",
                        description: format!("Triforce Blitz seed of the day"),
                    }
                }
                [arg] if arg == "jr" => SeedCommandParseResult::Tfb { version: "v7.1.143-blitz-0.43", unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz: Jabu’s Revenge seed") },
                [arg] if arg == "s2" => SeedCommandParseResult::Tfb { version: "v7.1.3-blitz-0.42", unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz S2 seed") },
                [arg] if arg == "s3" => SeedCommandParseResult::Tfb { version: "LATEST", unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz S3 seed") },
                [arg] if arg == "s4coop" => SeedCommandParseResult::TfbDev { coop: true, unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz S4 co-op seed") },
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn’t quite understand that" },
            },
            Self::OotrRsl => {
                let (preset, world_count) = match args {
                    [] => (rsl::Preset::League, 1),
                    [preset] if preset == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(),
                            went_first: None,
                            skipped_bans: 0,
                            settings: HashMap::default(),
                        },
                        unlock_spoiler_log,
                    }),
                    [preset] => if let Ok(preset) = preset.parse() {
                        if let rsl::Preset::Multiworld = preset {
                            return Ok(SeedCommandParseResult::Error { language: English, msg: "Missing world count (e.g. \"!seed multiworld 2\" for 2 worlds)".into() })
                        } else {
                            (preset, 1)
                        }
                    } else {
                        return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don’t recognize that preset" })
                    },
                    [preset, lite] if preset == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(),
                            went_first: None,
                            skipped_bans: 0,
                            settings: collect![as HashMap<_, _>: Cow::Borrowed("preset") => Cow::Borrowed(if lite == "lite" { "lite" } else { "league" })],
                        },
                        unlock_spoiler_log,
                    }),
                    [preset, world_count] => if let Ok(preset) = preset.parse() {
                        if let rsl::Preset::Multiworld = preset {
                            if let Ok(world_count) = world_count.parse() {
                                if world_count < 2 {
                                    return Ok(SeedCommandParseResult::Error { language: English, msg: "the world count must be a number between 2 and 15.".into() })
                                } else if world_count > 15 {
                                    return Ok(SeedCommandParseResult::Error { language: English, msg: "I can currently only roll seeds with up to 15 worlds. Please download the RSL script from https://github.com/matthewkirby/plando-random-settings to roll seeds for more than 15 players.".into() })
                                } else {
                                    (preset, world_count)
                                }
                            } else {
                                return Ok(SeedCommandParseResult::Error { language: English, msg: "the world count must be a number between 2 and 255.".into() })
                            }
                        } else {
                            return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I didn’t quite understand that" })
                        }
                    } else {
                        return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don’t recognize that preset" })
                    },
                    [..] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I didn’t quite understand that" }),
                };
                let (article, description) = match preset {
                    rsl::Preset::League => ("a", format!("Random Settings League seed")),
                    rsl::Preset::Beginner => ("an", format!("RSL-Lite seed")),
                    rsl::Preset::Intermediate => ("a", format!("random settings Intermediate seed")),
                    rsl::Preset::Ddr => ("a", format!("random settings DDR seed")),
                    rsl::Preset::CoOp => ("a", format!("random settings co-op seed")),
                    rsl::Preset::Multiworld => ("a", format!("random settings multiworld seed for {world_count} players")),
                };
                SeedCommandParseResult::Rsl { preset: rsl::VersionedPreset::Xopar { version: None, preset }, world_count, unlock_spoiler_log, language: English, article, description }
            }
            Self::TWWR { .. } => match args {
                [] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "the permalink is required" }),
                [permalink] => SeedCommandParseResult::Twwr {
                    permalink: permalink.clone(),
                    unlock_spoiler_log,
                    language: English,
                    article: "the",
                    description: format!("seed for race"),
                },
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn’t quite understand that" },
            },
            Self::OoTR | Self::Mmr => return Ok(SeedCommandParseResult::Error { language: English, msg: "This seed type rolls settings from the event config; use the bot’s !seed command in the race room instead.".into() }),
        })
    }
}


#[derive(Clone)]
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) enum CleanShutdownUpdate {
    RoomOpened(OpenRoom),
    RoomClosed(OpenRoom),
    Empty,
}

#[derive(SmartDefault)]
pub(crate) struct CleanShutdown {
    pub(crate) requested: bool,
    pub(crate) block_new: bool,
    pub(crate) open_rooms: HashSet<OpenRoom>,
    #[default(broadcast::Sender::new(128))]
    pub(crate) updates: broadcast::Sender<CleanShutdownUpdate>,
}

impl CleanShutdown {
    fn should_handle_new(&self) -> bool {
        !self.requested || !self.block_new && !self.open_rooms.is_empty()
    }
}

impl TypeMapKey for CleanShutdown {
    type Value = Arc<Mutex<CleanShutdown>>;
}

#[derive(Default, Clone)]
pub(crate) struct SeedMetadata {
    pub(crate) locked_spoiler_log_path: Option<String>,
    pub(crate) progression_spoiler: bool,
}

#[derive(Deserialize)]
struct TwwrGenerateResponse {
    #[allow(dead_code)]
    file_name: String,
    permalink: String,
    seed_hash: String,
    #[allow(dead_code)]
    spoiler_log_url: Option<String>,
}

pub(crate) struct GlobalState {
    /// Locked while event rooms are being created. Wait with handling new rooms while it's held.
    new_room_lock: Arc<Mutex<()>>,
    host_info: racetime::HostInfo,
    racetime_config: ConfigRaceTime,
    pub(crate) db_pool: PgPool,
    pub(crate) http_client: reqwest::Client,
    insecure_http_client: reqwest::Client,
    league_api_key: String,
    startgg_token: String,
    ootr_api_client: Arc<ootr_web::ApiClient>,
    pub(crate) discord_ctx: RwFuture<DiscordCtx>,
    #[cfg_attr(not(unix), allow(dead_code))]
    clean_shutdown: Arc<Mutex<CleanShutdown>>,
    seed_metadata: Arc<RwLock<HashMap<String, SeedMetadata>>>,
    pub(crate) extra_room_senders: Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>,
    #[cfg_attr(not(unix), allow(dead_code))]
    avianart_api_key: Option<String>,
    #[allow(dead_code)]
    pub(crate) mmr_api_key: Option<String>,
    /// Set of (racetime.gg category slug, goal name, is_custom) for all events configured in the DB.
    /// Populated at startup; used by should_handle_inner to decide which rooms to handle.
    known_goals: Arc<HashSet<(String, String, bool)>>,
}

impl TypeMapKey for GlobalState {
    type Value = Arc<Self>;
}

impl GlobalState {
    pub(crate) async fn new(
        new_room_lock: Arc<Mutex<()>>,
        racetime_config: ConfigRaceTime,
        db_pool: PgPool,
        http_client: reqwest::Client,
        insecure_http_client: reqwest::Client,
        league_api_key: String,
        startgg_token: String,
        ootr_api_client: Arc<ootr_web::ApiClient>,
        discord_ctx: RwFuture<DiscordCtx>,
        clean_shutdown: Arc<Mutex<CleanShutdown>>,
        seed_metadata: Arc<RwLock<HashMap<String, SeedMetadata>>>,
        avianart_api_key: Option<String>,
        mmr_api_key: Option<String>,
    ) -> Self {
        let known_goals = Arc::new(
            sqlx::query!(
                "SELECT DISTINCT e.racetime_goal_slug, e.is_custom_goal, grc.category_slug \
                 FROM events e \
                 JOIN game_series gs ON gs.series = e.series \
                 JOIN game_racetime_connection grc ON grc.game_id = gs.game_id \
                 WHERE e.racetime_goal_slug IS NOT NULL"
            )
                .fetch_all(&db_pool).await.unwrap_or_default()
                .into_iter()
                .filter_map(|row| {
                    Some((row.category_slug, row.racetime_goal_slug?, row.is_custom_goal))
                })
                .collect::<HashSet<_>>()
        );
        Self {
            host_info: racetime::HostInfo {
                hostname: Cow::Borrowed(racetime_host()),
                ..racetime::HostInfo::default()
            },
            new_room_lock, racetime_config, db_pool, http_client, insecure_http_client, league_api_key, startgg_token, ootr_api_client, discord_ctx, clean_shutdown, seed_metadata, avianart_api_key, mmr_api_key, known_goals, extra_room_senders: Arc::new(RwLock::new(HashMap::default())),
        }
    }

    pub(crate) fn roll_twwr_seed(self: Arc<Self>, version: Option<VersionedBranch>, settings_string: String, unlock_spoiler_log: UnlockSpoilerLog) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        tokio::spawn(async move {
            update_tx.send(SeedRollUpdate::Started).await.allow_unreceived();
            let randomizer_path = if let Some(VersionedBranch::Tww { ref identifier, .. }) = version {
                &**identifier
            } else {
                "wwrando"
            };
            let generate_spoiler_log = match unlock_spoiler_log {
                UnlockSpoilerLog::Now | UnlockSpoilerLog::Progression | UnlockSpoilerLog::After => "true",
                UnlockSpoilerLog::Never => "false",
            };
            let url = format!(
                "https://seedbot.twwrando.com/generate?randomizer_path={}&permalink={}&prefix=HTH&generate_spoiler_log={}",
                randomizer_path, settings_string, generate_spoiler_log
            );
            match self.http_client.post(&url)
                .header("accept", "application/json")
                .send().await {
                Ok(response) => match response.detailed_error_for_status().await {
                    Ok(response) => match response.json::<TwwrGenerateResponse>().await {
                        Ok(TwwrGenerateResponse { permalink, seed_hash, .. }) => {
                            update_tx.send(SeedRollUpdate::Done {
                                seed: seed::Data {
                                    file_hash: None,
                                    password: None,
                                    seed_data: Some(seed::Files::TwwrPermalink { permalink, seed_hash }.to_seed_data_base()),
                                    progression_spoiler: false,
                                },
                                rsl_preset: None,
                                version: version.clone(),
                                unlock_spoiler_log,
                            }).await.allow_unreceived();
                        }
                        Err(e) => {
                            update_tx.send(SeedRollUpdate::Error(RollError::Twwr(format!("failed to parse TWWR API response: {e}")))).await.allow_unreceived();
                        }
                    },
                    Err(e) => {
                        update_tx.send(SeedRollUpdate::Error(RollError::Twwr(format!("TWWR API returned error: {e}")))).await.allow_unreceived();
                    }
                },
                Err(e) => {
                    update_tx.send(SeedRollUpdate::Error(RollError::Twwr(format!("failed to connect to TWWR API: {e}")))).await.allow_unreceived();
                }
            }
        });
        update_rx
    }

    pub(crate) fn roll_seed(self: Arc<Self>, preroll: PrerollMode, allow_web: bool, delay_until: Option<DateTime<Utc>>, version: VersionedBranch, mut settings: seed::Settings, unlock_spoiler_log: UnlockSpoilerLog) -> mpsc::Receiver<SeedRollUpdate> {
        let world_count = settings.get("world_count").map_or(1, |world_count| world_count.as_u64().expect("world_count setting wasn't valid u64").try_into().expect("too many worlds"));
        let password_lock = settings.get("password_lock").is_some_and(|password_lock| password_lock.as_bool().expect("password_lock setting wasn't a Boolean"));
        settings.insert(format!("create_spoiler"), json!(match unlock_spoiler_log {
            UnlockSpoilerLog::Now | UnlockSpoilerLog::Progression | UnlockSpoilerLog::After => true,
            UnlockSpoilerLog::Never => password_lock, // spoiler log needs to be generated so the backend can read the password
        }));
        let (update_tx, update_rx) = mpsc::channel(128);
        tokio::spawn(async move {
            if_chain! {
                if allow_web;
                if let Some(web_version) = self.ootr_api_client.can_roll_on_web(None, &version, world_count, unlock_spoiler_log).await;
                then {
                    // ootrandomizer.com seed IDs are sequential, making it easy to find a seed if you know when it was rolled.
                    // This is especially true for open races, whose rooms are opened an entire hour before start.
                    // To make this a bit more difficult, we delay the start of seed rolling depending on the goal.
                    match preroll {
                        // The type of seed being rolled is unlikely to require a long time or multiple attempts to generate,
                        // so we avoid the issue with sequential IDs by simply not rolling ahead of time.
                        PrerollMode::None => if let Some(sleep_duration) = delay_until.and_then(|delay_until| (delay_until - Utc::now()).to_std().ok()) {
                            sleep(sleep_duration).await;
                        },
                        // Middle-ground option. Start rolling the seed at a random point between 20 and 15 minutes before start.
                        PrerollMode::Short => if let Some(max_sleep_duration) = delay_until.and_then(|delay_until| (delay_until - Utc::now()).to_std().ok()) {
                            let min_sleep_duration = max_sleep_duration.saturating_sub(Duration::from_secs(5 * 60));
                            let sleep_duration = rng().random_range(min_sleep_duration..max_sleep_duration);
                            sleep(sleep_duration).await;
                        },
                        // The type of seed being rolled is fairly likely to require a long time and/or multiple attempts to generate.
                        // Start rolling the seed at a random point between the room being opened and 30 minutes before start.
                        PrerollMode::Medium => if let Some(max_sleep_duration) = delay_until.and_then(|delay_until| (delay_until - TimeDelta::minutes(15) - Utc::now()).to_std().ok()) {
                            let sleep_duration = rng().random_range(Duration::default()..max_sleep_duration);
                            sleep(sleep_duration).await;
                        },
                        // The type of seed being rolled is extremely likely to require a very long time and/or a large number of attempts to generate.
                        // Start rolling the seed immediately upon the room being opened.
                        PrerollMode::Long => {}
                    }
                    match self.ootr_api_client.roll_seed_with_retry(update_tx.clone(), delay_until, web_version, false, unlock_spoiler_log, settings).await {
                        Ok(ootr_web::SeedInfo { id, gen_time, file_hash, file_stem, password }) => update_tx.send(SeedRollUpdate::Done {
                            seed: seed::Data {
                                file_hash: Some(file_hash),
                                seed_data: Some(seed::Files::OotrWeb {
                                    file_stem: Cow::Owned(file_stem),
                                    id, gen_time,
                                }.to_seed_data_base()),
                                progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                                password,
                            },
                            rsl_preset: None,
                            version: Some(version),
                            unlock_spoiler_log,
                        }).await?,
                        Err(e) => update_tx.send(SeedRollUpdate::Error(e.into())).await?, //TODO fall back to rolling locally for network errors
                    }
                } else {
                    update_tx.send(SeedRollUpdate::Started).await?;
                    match roll_seed_locally(delay_until, version.clone(), match unlock_spoiler_log {
                        UnlockSpoilerLog::Now | UnlockSpoilerLog::Progression | UnlockSpoilerLog::After => true,
                        UnlockSpoilerLog::Never => password_lock, // spoiler log needs to be generated so the backend can read the password
                    }, settings).await {
                        Ok((patch_filename, spoiler_log_path)) => update_tx.send(match spoiler_log_path.map(|spoiler_log_path| spoiler_log_path.into_os_string().into_string()).transpose() {
                            Ok(locked_spoiler_log_path) => match regex_captures!(r"^(.+)\.zpfz?$", &patch_filename) {
                                Some((_, file_stem)) => SeedRollUpdate::Done {
                                    seed: seed::Data {
                                        file_hash: None, password: None, // will be read from spoiler log
                                        seed_data: Some(seed::Files::MidosHouse {
                                            file_stem: Cow::Owned(file_stem.to_owned()),
                                            locked_spoiler_log_path,
                                        }.to_seed_data_base()),
                                        progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                                    },
                                    rsl_preset: None,
                                    version: Some(version),
                                    unlock_spoiler_log,
                                },
                                None => SeedRollUpdate::Error(RollError::PatchPath),
                            },
                            Err(e) => SeedRollUpdate::Error(e.into())
                        }).await?,
                        Err(e) => update_tx.send(SeedRollUpdate::Error(e)).await?,
                    }
                }
            }
            Ok::<_, mpsc::error::SendError<_>>(())
        });
        update_rx
    }

    pub(crate) fn record_twwr_permalink(self: Arc<Self>, permalink: String, unlock_spoiler_log: UnlockSpoilerLog) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        tokio::spawn(async move {
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash: None,
                    password: None,
                    seed_data: Some(seed::Files::TwwrPermalink {
                        permalink,
                        seed_hash: String::new(),
                    }.to_seed_data_base()),
                    progression_spoiler: false,
                },
                rsl_preset: None,
                version: None,
                unlock_spoiler_log,
            }).await.allow_unreceived();
        });
        update_rx
    }

    #[cfg_attr(not(unix), allow(dead_code))]
    pub(crate) fn roll_avianart_seed(self: Arc<Self>, preset: String) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            let client = crate::avianart::AvianartClient::new(
                self.avianart_api_key.clone(),
                self.http_client.clone(),
            );
            let hash = client.generate_seed(&preset).await
                .map_err(|e| RollError::Avianart(e.to_string()))?;
            let seed_data = client.wait_for_seed(&hash).await
                .map_err(|e| RollError::Avianart(e.to_string()))?;
            let seed_hash = if let Some(ref spoiler) = seed_data.spoiler {
                Some(crate::avianart::parse_file_hash(&spoiler.meta.hash)
                    .map_err(|e| RollError::Avianart(e.to_string()))?)
            } else {
                None
            };
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash: None,
                    password: None,
                    seed_data: Some(seed::Files::AvianartSeed { hash, seed_hash }.to_seed_data_base()),
                    progression_spoiler: false,
                },
                rsl_preset: None,
                version: None,
                unlock_spoiler_log: UnlockSpoilerLog::Never,
            }).await.allow_unreceived();
            Ok::<_, RollError>(())
        }.then(|res| async move {
            if let Err(e) = res {
                update_tx2.send(SeedRollUpdate::Error(e)).await.allow_unreceived();
            }
        }));
        update_rx
    }

    pub(crate) fn roll_mysteryd20_seed(self: Arc<Self>) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            let uuid = Uuid::new_v4();

            // Download the weights YAML
            let weights_url = "https://zeldaspeedruns.com/assets/hth/miniturnier_doors.yaml";
            let response = reqwest::get(weights_url).await?;
            let weights_yaml_content = response.text().await?;
            
            let yaml_file = tempfile::Builder::new().prefix("alttpr_").suffix(".yml").tempfile().at_unknown()?;
            let yaml_path = yaml_file.path();
            tokio::fs::File::from_std(yaml_file.reopen().at(&yaml_file)?).write_all(weights_yaml_content.as_bytes()).await.at(&yaml_file)?;
            
            // Add retry logic with 2 retries
            const MAX_RETRIES: u8 = 2;
            
            for attempt in 0..=MAX_RETRIES {
                let output = match timeout(Duration::from_secs(180), async {
                    Command::new(PYTHON)
                        .current_dir("../alttpr")
                        .arg("Mystery.py")
                        .arg("--weights")
                        .arg(yaml_path)
                        .arg("--outputpath")
                        .arg("/var/www/midos.house/seed")
                        .arg("--outputname")
                        .arg(uuid.to_string())
                        .arg("--bps")
                        .arg("--spoiler")
                        .arg("full")
                        .arg("--suppress_rom")
                        .arg("--suppress_meta")
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .at_command("Mystery.py")?
                        .wait_with_output()
                        .await
                        .at_command("Mystery.py")
                }).await {
                    Ok(output) => output?,
                    Err(_) => {
                        // Timeout occurred - treat it like a retryable error
                        if attempt < MAX_RETRIES {
                            // Wait a bit before retrying (exponential backoff)
                            sleep(Duration::from_secs(2 + 2u64.pow(attempt as u32))).await;
                            continue;
                        }
                        // Max retries reached
                        return Err(RollError::Retries {
                            num_retries: MAX_RETRIES + 1,
                            last_error: Some("Command timed out after 180 seconds".to_string()),
                        });
                    }
                };
                
                match output.status.code() {
                    Some(0) => {
                        break;
                    }
                    Some(1) => {
                        // Randomizer failed to generate a seed, lets retry.
                        let last_error = Some(String::from_utf8_lossy(&output.stderr).into_owned());
                        if attempt < MAX_RETRIES {
                            // Wait a bit before retrying (exponential backoff)
                            sleep(Duration::from_secs(10 + 2u64.pow(attempt as u32))).await;
                            continue;
                        }
                        // Max retries reached
                        return Err(RollError::Retries {
                            num_retries: MAX_RETRIES + 1,
                            last_error,
                        });
                    }
                    _ => {
                        // Other error codes - fail immediately
                        return Err(RollError::Wheel(wheel::Error::CommandExit { 
                            name: Cow::Borrowed("Mystery.py"), 
                            output 
                        }));
                    }
                }
            }
            
            // This swallows the hash error and just makes it empty--maybe we should surface this somehow?
            let file_hash = Self::retrieve_hash_and_clean_up_spoiler(uuid).await.ok();
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash,
                    seed_data: Some(seed::Files::AlttprDoorRando { uuid }.to_seed_data_base()),
                    progression_spoiler: false,
                    password: None,
                },
                rsl_preset: None,
                version: None,
                unlock_spoiler_log: UnlockSpoilerLog::Never
            }).await.allow_unreceived();
            Ok(())
        }.then(|res| async move {
            match res {
                Ok(()) => {}
                Err(e) => update_tx2.send(SeedRollUpdate::Error(e)).await.allow_unreceived(),
            }
        }));
        update_rx
    }

    /// Roll an ALTTPR Door Randomizer seed from pre-built YAML content.
    ///
    /// Shared implementation for both Boothisman (AlttprDe9) and MutualChoices (Crosskeys) sources.
    /// `working_dir` is `"../ALttPDoorRandomizer"` for Boothisman or `"../alttpr"` for MutualChoices.
    /// `with_output_name` enables `--outputname uuid` and patch-file verification (AlttprDe9 only).
    pub(crate) fn roll_alttpr_dr_seed(self: Arc<Self>, yaml_content: String, uuid: Uuid, working_dir: &'static str, with_output_name: bool) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            let yaml_file = tempfile::Builder::new().prefix("alttpr_").suffix(".yml").tempfile().at_unknown()?;
            let yaml_path = yaml_file.path();
            tokio::fs::File::from_std(yaml_file.reopen().at(&yaml_file)?).write_all(yaml_content.as_bytes()).await.at(&yaml_file)?;
            let output_name_str = with_output_name.then(|| uuid.to_string());
            run_dungeon_randomizer(yaml_path, working_dir, output_name_str.as_deref()).await?;
            if with_output_name {
                let patch_path = format!("/var/www/midos.house/seed/DR_{uuid}.bps");
                if !tokio::fs::try_exists(&patch_path).await.at(&patch_path)? {
                    return Err(RollError::AlttprDe(format!("DungeonRandomizer.py exited successfully but patch file was not found at {}", patch_path)));
                }
            }
            let file_hash = if with_output_name {
                Some(Self::retrieve_hash_and_clean_up_spoiler(uuid).await?)
            } else {
                Self::retrieve_hash_and_clean_up_spoiler(uuid).await.ok()
            };
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash,
                    seed_data: Some(seed::Files::AlttprDoorRando { uuid }.to_seed_data_base()),
                    progression_spoiler: false,
                    password: None,
                },
                rsl_preset: None,
                version: None,
                unlock_spoiler_log: UnlockSpoilerLog::Never,
            }).await.allow_unreceived();
            Ok(())
        }.then(|res: Result<(), RollError>| async move {
            match res {
                Ok(()) => {}
                Err(e) => update_tx2.send(SeedRollUpdate::Error(e)).await.allow_unreceived(),
            }
        }));
        update_rx
    }

    /// Unified seed rolling dispatcher: rolls a seed for an event based on its `seed_gen_type`.
    ///
    /// Replaces the `match goal { ... }` dispatch blocks in `handle_race()` and elsewhere.
    /// Each `SeedGenType` variant calls the appropriate existing rolling method.
    pub(crate) async fn roll_seed_for_event(
        self: Arc<Self>,
        seed_gen_type: &seed_gen_type::SeedGenType,
        cal_event: &cal::Event,
        event: &event::Data<'_>,
    ) -> mpsc::Receiver<SeedRollUpdate> {
        use seed_gen_type::{SeedGenType, AlttprDrSource};
        let unlock_spoiler_log = match event.spoiler_unlock.as_str() {
            "after" => UnlockSpoilerLog::After,
            "immediately" => UnlockSpoilerLog::Now,
            _ => UnlockSpoilerLog::Never,
        };
        match seed_gen_type {
            SeedGenType::AlttprDoorRando { source: AlttprDrSource::Boothisman } => {
                let alttprde_options = AlttprDeRaceOptions::for_race(&self.db_pool, &cal_event.race, event.round_modes.as_ref()).await;
                let api_url = match alttprde_options.seed_url() {
                    Some(url) => url,
                    None => return alttpr_dr_error_receiver(RollError::AlttprDe("Mode not yet drafted - cannot roll seed".to_owned())),
                };
                let uuid = Uuid::new_v4();
                let yaml_content = match async {
                    self.http_client.get(&api_url).send().await?.text().await
                }.await {
                    Ok(y) => y,
                    Err(e) => return alttpr_dr_error_receiver(e.into()),
                };
                match inject_alttpr_dr_meta(&yaml_content, uuid) {
                    Ok(yaml) => self.roll_alttpr_dr_seed(yaml, uuid, "../ALttPDoorRandomizer", true),
                    Err(e) => alttpr_dr_error_receiver(e.into()),
                }
            }
            SeedGenType::AlttprDoorRando { source: AlttprDrSource::MutualChoices } => {
                let crosskeys_options = CrosskeysRaceOptions::for_race(&self.db_pool, &cal_event.race).await;
                let uuid = Uuid::new_v4();
                match build_crosskeys_yaml(&crosskeys_options, uuid) {
                    Ok(yaml_content) => self.roll_alttpr_dr_seed(yaml_content, uuid, "../alttpr", false),
                    Err(e) => alttpr_dr_error_receiver(e.into()),
                }
            }
            SeedGenType::AlttprDoorRando { source: AlttprDrSource::MysteryPool { .. } } => {
                self.roll_mysteryd20_seed()
            }
            SeedGenType::AlttprAvianart => {
                let game_num = cal_event.race.game.unwrap_or(1);
                let preset = cal_event.race.draft.as_ref()
                    .and_then(|d| d.settings.get(&*format!("game{game_num}_preset")))
                    .expect("Avianart async race missing preset in draft state")
                    .as_ref()
                    .to_owned();
                self.roll_avianart_seed(preset)
            }
            SeedGenType::TWWR { permalink } => {
                let version = event.rando_version.clone();
                self.roll_twwr_seed(version, permalink.clone(), unlock_spoiler_log)
            }
            _ => unimplemented!("async seed rolling not implemented for seed_gen_type {:?}", seed_gen_type),
        }
    }

    async fn retrieve_hash_and_clean_up_spoiler(uuid: Uuid) -> Result<[String; 5], RollError> {
        let spoiler_path = format!("/var/www/midos.house/seed/DR_{uuid}_Spoiler.txt");
        let destination_path = format!("/var/www/midos.house/spoilers/DR_{uuid}_Spoiler.txt");
        let mut file = BufReader::new(File::open(spoiler_path.clone()).await?);
        let mut line = String::default();
        let hash = loop {
            line.clear();
            if file.read_line(&mut line).await.at(spoiler_path.clone())? == 0 {
                return Err(RollError::AlttprHashLineNotFound)
            }
            if let Some((_, h1, h2, h3, h4, h5)) = regex_captures!("^Hash: (.+), (.+), (.+), (.+), (.+)\r?\n$", &line) {
                break [h1.to_string(), h2.to_string(), h3.to_string(), h4.to_string(), h5.to_string()]
            }
        };
        Command::new("mv").arg(spoiler_path).arg(destination_path).check("Moving spoiler file").await?;
        Ok(hash)
    }

    pub(crate) fn roll_rsl_seed(self: Arc<Self>, delay_until: Option<DateTime<Utc>>, preset: rsl::VersionedPreset, world_count: u8, unlock_spoiler_log: UnlockSpoilerLog) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            let rsl_script_path = preset.script_path().await?;
            // check RSL script version
            let rsl_version = Command::new(PYTHON)
                .arg("-c")
                .arg("import rslversion; print(rslversion.__version__)")
                .current_dir(&rsl_script_path)
                .check(PYTHON).await?
                .stdout;
            let rsl_version = String::from_utf8(rsl_version)?;
            let supports_plando_filename_base = if let Some((_, major, minor, patch, devmvp)) = regex_captures!(r"^([0-9]+)\.([0-9]+)\.([0-9]+) devmvp-([0-9]+)$", &rsl_version.trim()) {
                (Version::new(major.parse()?, minor.parse()?, patch.parse()?), devmvp.parse()?) >= (Version::new(2, 6, 3), 4)
            } else {
                rsl_version.parse::<Version>().is_ok_and(|rsl_version| rsl_version >= Version::new(2, 8, 2))
            };
            // check required randomizer version
            let randomizer_version = Command::new(PYTHON)
                .arg("-c")
                .arg("import rslversion; print(rslversion.randomizer_version)")
                .current_dir(&rsl_script_path)
                .check(PYTHON).await?
                .stdout;
            let randomizer_version = String::from_utf8(randomizer_version)?.trim().parse::<rando::Version>()?;
            let web_version = self.ootr_api_client.can_roll_on_web(Some(&preset), &VersionedBranch::Pinned { version: randomizer_version.clone() }, world_count, unlock_spoiler_log).await;
            // run the RSL script
            update_tx.send(SeedRollUpdate::Started).await.allow_unreceived();
            let outer_tries = if web_version.is_some() { 5 } else { 1 }; // when generating locally, retries are already handled by the RSL script
            let mut last_error = None;
            for attempt in 0.. {
                if attempt >= outer_tries && delay_until.is_none_or(|delay_until| Utc::now() >= delay_until) {
                    return Err(RollError::Retries {
                        num_retries: 3 * attempt,
                        last_error,
                    })
                }
                let mut rsl_cmd = Command::new(PYTHON);
                rsl_cmd.arg("RandomSettingsGenerator.py");
                rsl_cmd.arg("--no_log_errors");
                if supports_plando_filename_base {
                    // add a sequence ID to the names of temporary plando files to prevent name collisions
                    rsl_cmd.arg(format!("--plando_filename_base=mh_{}", RSL_SEQUENCE_ID.fetch_add(1, atomic::Ordering::Relaxed)));
                }
                let mut input = None;
                if !matches!(preset, rsl::VersionedPreset::Xopar { preset: rsl::Preset::League, .. }) {
                    match preset.name_or_weights() {
                        Either::Left(name) => {
                            rsl_cmd.arg(format!(
                                "--override={}{name}_override.json",
                                if preset.base_version().is_none_or(|version| *version >= Version::new(2, 3, 9)) { "weights/" } else { "" },
                            ));
                        }
                        Either::Right(weights) => {
                            rsl_cmd.arg("--override=-");
                            rsl_cmd.stdin(Stdio::piped());
                            input = Some(serde_json::to_vec(&weights)?);
                        }
                    }
                }
                if world_count > 1 {
                    rsl_cmd.arg(format!("--worldcount={world_count}"));
                }
                if web_version.is_some() {
                    rsl_cmd.arg("--no_seed");
                }
                let mut rsl_process = rsl_cmd
                    .current_dir(&rsl_script_path)
                    .stdout(Stdio::piped())
                    .spawn().at_command("RandomSettingsGenerator.py")?;
                if let Some(input) = input {
                    rsl_process.stdin.as_mut().expect("piped stdin missing").write_all(&input).await.at_command("RandomSettingsGenerator.py")?;
                }
                let output = rsl_process.wait_with_output().await.at_command("RandomSettingsGenerator.py")?;
                match output.status.code() {
                    Some(0) => {}
                    Some(2) => {
                        last_error = Some(String::from_utf8_lossy(&output.stderr).into_owned());
                        continue
                    }
                    _ => return Err(RollError::Wheel(wheel::Error::CommandExit { name: Cow::Borrowed("RandomSettingsGenerator.py"), output })),
                }
                if let Some(web_version) = web_version.clone() {
                    #[derive(Deserialize)]
                    struct Plando {
                        settings: seed::Settings,
                    }

                    let plando_filename = BufRead::lines(&*output.stdout)
                        .filter_map_ok(|line| Some(regex_captures!("^Plando File: (.+)$", &line)?.1.to_owned()))
                        .next().ok_or(RollError::RslScriptOutput { regex: "^Plando File: (.+)$" })?.at_command("RandomSettingsGenerator.py")?;
                    let plando_path = rsl_script_path.join("data").join(plando_filename);
                    let plando_file = fs::read_to_string(&plando_path).await?;
                    let settings = serde_json::from_str::<Plando>(&plando_file)?.settings;
                    fs::remove_file(plando_path).await?;
                    if let Some(max_sleep_duration) = delay_until.and_then(|delay_until| (delay_until - TimeDelta::minutes(15) - Utc::now()).to_std().ok()) {
                        // ootrandomizer.com seed IDs are sequential, making it easy to find a seed if you know when it was rolled.
                        // This is especially true for open races, whose rooms are opened an entire hour before start.
                        // To make this a bit more difficult, we start rolling the seed at a random point between the room being opened and 30 minutes before start.
                        let sleep_duration = rng().random_range(Duration::default()..max_sleep_duration);
                        sleep(sleep_duration).await;
                    }
                    let ootr_web::SeedInfo { id, gen_time, file_hash, file_stem, password } = match self.ootr_api_client.roll_seed_with_retry(update_tx.clone(), None /* always limit to 3 tries per settings */, web_version, true, unlock_spoiler_log, settings).await {
                        Ok(data) => data,
                        Err(ootr_web::Error::Retries { .. }) => continue,
                        Err(e) => return Err(e.into()), //TODO fall back to rolling locally for network errors
                    };
                    update_tx.send(SeedRollUpdate::Done {
                        seed: seed::Data {
                            file_hash: Some(file_hash),
                            seed_data: Some(seed::Files::OotrWeb {
                                file_stem: Cow::Owned(file_stem),
                                id, gen_time,
                            }.to_seed_data_base()),
                            progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                            password,
                        },
                        rsl_preset: if let rsl::VersionedPreset::Xopar { preset, .. } = preset { Some(preset) } else { None },
                        version: None,
                        unlock_spoiler_log,
                    }).await.allow_unreceived();
                    return Ok(())
                } else {
                    let patch_filename = BufRead::lines(&*output.stdout)
                        .filter_map_ok(|line| Some(regex_captures!("^Creating Patch File: (.+)$", &line)?.1.to_owned()))
                        .next().ok_or(RollError::RslScriptOutput { regex: "^Creating Patch File: (.+)$" })?.at_command("RandomSettingsGenerator.py")?;
                    let patch_path = rsl_script_path.join("patches").join(&patch_filename);
                    let spoiler_log_filename = BufRead::lines(&*output.stdout)
                        .filter_map_ok(|line| Some(regex_captures!("^Created spoiler log at: (.+)$", &line)?.1.to_owned()))
                        .next().ok_or(RollError::RslScriptOutput { regex: "^Created spoiler log at: (.+)$" })?.at_command("RandomSettingsGenerator.py")?;
                    let spoiler_log_path = rsl_script_path.join("patches").join(spoiler_log_filename);
                    let (_, file_stem) = regex_captures!(r"^(.+)\.zpfz?$", &patch_filename).ok_or(RollError::RslScriptOutput { regex: r"^(.+)\.zpfz?$" })?;
                    for extra_output_filename in [format!("{file_stem}_Cosmetics.json"), format!("{file_stem}_Distribution.json")] {
                        fs::remove_file(rsl_script_path.join("patches").join(extra_output_filename)).await.missing_ok()?;
                    }
                    fs::rename(patch_path, Path::new(seed::DIR).join(&patch_filename)).await?;
                    update_tx.send(match regex_captures!(r"^(.+)\.zpfz?$", &patch_filename) {
                        Some((_, file_stem)) => SeedRollUpdate::Done {
                            seed: seed::Data {
                                file_hash: None, password: None, // will be read from spoiler log
                                seed_data: Some(seed::Files::MidosHouse {
                                    file_stem: Cow::Owned(file_stem.to_owned()),
                                    locked_spoiler_log_path: Some(spoiler_log_path.into_os_string().into_string()?),
                                }.to_seed_data_base()),
                                progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                            },
                            rsl_preset: if let rsl::VersionedPreset::Xopar { preset, .. } = preset { Some(preset) } else { None },
                            version: None,
                            unlock_spoiler_log,
                        },
                        None => SeedRollUpdate::Error(RollError::PatchPath),
                    }).await.allow_unreceived();
                    return Ok(())
                }
            }
            Ok(())
        }.then(|res| async move {
            match res {
                Ok(()) => {}
                Err(e) => update_tx2.send(SeedRollUpdate::Error(e)).await.allow_unreceived(),
            }
        }));
        update_rx
    }

    pub(crate) fn roll_tfb_seed(self: Arc<Self>, delay_until: Option<DateTime<Utc>>, version: &'static str, room: Option<String>, unlock_spoiler_log: UnlockSpoilerLog) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            if let Some(max_sleep_duration) = delay_until.and_then(|delay_until| (delay_until - TimeDelta::minutes(15) - Utc::now()).to_std().ok()) {
                // triforceblitz.com has a list of recently rolled seeds, making it easy to find a seed if you know when it was rolled.
                // This is especially true for open races, whose rooms are opened an entire hour before start.
                // To make this a bit more difficult, we start rolling the seed at a random point between the room being opened and 30 minutes before start.
                let sleep_duration = rng().random_range(Duration::default()..max_sleep_duration);
                sleep(sleep_duration).await;
            }
            update_tx.send(SeedRollUpdate::Started).await.allow_unreceived();
            let form_data = match unlock_spoiler_log {
                UnlockSpoilerLog::Now => vec![
                    ("unlockSetting", "ALWAYS"),
                    ("version", version),
                ],
                UnlockSpoilerLog::Progression => panic!("progression spoiler mode not supported by triforceblitz.com"),
                UnlockSpoilerLog::After => if let Some(ref room) = room {
                    vec![
                        ("unlockSetting", "RACETIME"),
                        ("racetimeRoom", room),
                        ("version", version),
                    ]
                } else {
                    panic!("cannot set a Triforce Blitz seed to unlock after the race without a race room")
                },
                UnlockSpoilerLog::Never => vec![
                    ("unlockSetting", "NEVER"),
                    ("version", version),
                ],
            };
            let mut attempts = 0;
            let response = loop {
                attempts += 1;
                let response = self.http_client
                    .post("https://www.triforceblitz.com/generator")
                    .form(&form_data)
                    .timeout(Duration::from_secs(5 * 60))
                    .send().await?
                    .detailed_error_for_status().await;
                match response {
                    Ok(response) => break response,
                    Err(wheel::Error::ResponseStatus { inner, .. }) if attempts < 3 && inner.status().is_some_and(|status| status.is_server_error()) => continue,
                    Err(e) => return Err(e.into()),
                }
            };
            let (is_dev, uuid) = tfb::parse_seed_url(response.url()).ok_or_else(|| RollError::TfbUrl(response.url().clone()))?;
            debug_assert!(!is_dev);
            let response_body = response.text().await?;
            let file_hash = kuchiki::parse_html().one(response_body)
                .select_first(".hash-icons").map_err(|()| RollError::TfbHtml)?
                .as_node()
                .children()
                .filter_map(NodeRef::into_element_ref)
                .filter_map(|elt| elt.attributes.borrow().get("title").and_then(|title| title.parse().ok()))
                .collect_vec();
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash: Some(file_hash.try_into().map_err(|_| RollError::TfbHash)?),
                    password: None,
                    seed_data: Some(seed::Files::TriforceBlitz { is_dev, uuid }.to_seed_data_base()),
                    progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                },
                rsl_preset: None,
                version: None,
                unlock_spoiler_log,
            }).await.allow_unreceived();
            Ok(())
        }.then(|res| async move {
            match res {
                Ok(()) => {}
                Err(e) => update_tx2.send(SeedRollUpdate::Error(e)).await.allow_unreceived(),
            }
        }));
        update_rx
    }

    pub(crate) fn roll_tfb_dev_seed(self: Arc<Self>, delay_until: Option<DateTime<Utc>>, coop: bool, room: Option<String>, unlock_spoiler_log: UnlockSpoilerLog) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            if let Some(max_sleep_duration) = delay_until.and_then(|delay_until| (delay_until - TimeDelta::minutes(15) - Utc::now()).to_std().ok()) {
                // triforceblitz.com has a list of recently rolled seeds, making it easy to find a seed if you know when it was rolled.
                // This is especially true for open races, whose rooms are opened an entire hour before start.
                // To make this a bit more difficult, we start rolling the seed at a random point between the room being opened and 30 minutes before start.
                let sleep_duration = rng().random_range(Duration::default()..max_sleep_duration);
                sleep(sleep_duration).await;
            }
            update_tx.send(SeedRollUpdate::Started).await.allow_unreceived();
            let mut form_data = match unlock_spoiler_log {
                UnlockSpoilerLog::Now => vec![
                    ("unlockMode", "UNLOCKED"),
                ],
                UnlockSpoilerLog::Progression => panic!("progression spoiler mode not supported by triforceblitz.com"),
                UnlockSpoilerLog::After => if let Some(ref room) = room {
                    vec![
                        ("unlockMode", "RACETIME"),
                        ("racetimeUrl", room),
                    ]
                } else {
                    panic!("cannot set a Triforce Blitz seed to unlock after the race without a race room")
                },
                UnlockSpoilerLog::Never => vec![
                    ("unlockMode", "LOCKED"),
                ],
            };
            if coop {
                form_data.push(("cooperative", "true"));
            }
            let mut attempts = 0;
            let response = loop {
                attempts += 1;
                let response = self.insecure_http_client // dev.triforceblitz.com generates plain HTTP redirects
                    .post("https://dev.triforceblitz.com/seeds/generate")
                    .form(&form_data)
                    .timeout(Duration::from_secs(5 * 60))
                    .send().await?
                    .detailed_error_for_status().await;
                match response {
                    Ok(response) => break response,
                    Err(wheel::Error::ResponseStatus { inner, .. }) if attempts < 3 && inner.status().is_some_and(|status| status.is_server_error()) => continue,
                    Err(e) => return Err(e.into()),
                }
            };
            let (is_dev, uuid) = tfb::parse_seed_url(response.url()).ok_or_else(|| RollError::TfbUrl(response.url().clone()))?;
            debug_assert!(is_dev);
            /*
            let patch = self.http_client
                .get(format!("https://dev.triforceblitz.com/seeds/{uuid}/patch"))
                .send().await?
                .detailed_error_for_status().await?
                .bytes().await?;
            if coop {
                //TODO decode patch as zip, extract file hash from P1.zpf
            } else {
                //TODO extract file hash from patch, which is a .zpf
            }
            */
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash: None,
                    password: None,
                    seed_data: Some(seed::Files::TriforceBlitz { is_dev, uuid }.to_seed_data_base()),
                    progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                },
                rsl_preset: None,
                version: None,
                unlock_spoiler_log,
            }).await.allow_unreceived();
            Ok(())
        }.then(|res| async move {
            match res {
                Ok(()) => {}
                Err(e) => update_tx2.send(SeedRollUpdate::Error(e)).await.allow_unreceived(),
            }
        }));
        update_rx
    }
}

fn build_crosskeys_yaml(opts: &CrosskeysRaceOptions, uuid: Uuid) -> Result<String, serde_yml::Error> {
    let agreed = &opts.agreed;
    let meta = AlttprDoorRandoMeta { bps: true, name: uuid.to_string(), race: true, skip_playthrough: true, spoiler: "full", suppress_rom: true };
    let mut yaml = AlttprDoorRandoYaml { placements: HashMap::default(), settings: HashMap::default(), start_inventory: HashMap::default(), meta };
    let keydrop_mode = if agreed.contains("keydrop") { "keys" } else { "none" };
    let flute_mode = if agreed.contains("flute") { "active" } else { "normal" };
    let goal = if agreed.contains("all_dungeons") { "dungeons" } else { "crystals" };
    let mirrorscroll = if agreed.contains("mirror_scroll") { 1 } else { 0 };
    let world_state = if agreed.contains("inverted") { "inverted" } else { "open" };
    let pseudoboots = if agreed.contains("pseudoboots") { 1 } else { 0 };
    let skullwoods = if agreed.contains("zw") { "followlinked" } else { "original" };
    let settings = AlttprDoorRandoSetting {
        accessibility: "locations",
        bigkeyshuffle: 1,
        compassshuffle: 1,
        crystals_ganon: "7",
        crystals_gt: "7",
        dropshuffle: keydrop_mode,
        flute_mode,
        goal,
        item_functionality: "normal",
        key_logic_algorithm: "partial",
        keyshuffle: "wild",
        linked_drops: "unset",
        mapshuffle: 1,
        mirrorscroll,
        mode: world_state,
        pottery: keydrop_mode,
        pseudoboots,
        shuffle: "crossed",
        shuffletavern: 0,
        skullwoods,
    };
    if !agreed.contains("zw") {
        yaml.placements.insert(1, AlttprDoorRandoPlacements { pinball_room: "Small Key (Skull Woods)" });
    }
    if agreed.contains("flute") {
        yaml.start_inventory.insert(1, &["Ocarina (Activated)"]);
    }
    yaml.settings.insert(1, settings);
    serde_yml::to_string(&yaml)
}

fn inject_alttpr_dr_meta(yaml_content: &str, uuid: Uuid) -> Result<String, serde_yml::Error> {
    let mut yaml_value: serde_yml::Value = serde_yml::from_str(yaml_content)?;
    let meta = AlttprDoorRandoMeta { bps: true, name: uuid.to_string(), race: true, skip_playthrough: true, spoiler: "full", suppress_rom: true };
    if let serde_yml::Value::Mapping(ref mut map) = yaml_value {
        map.insert(serde_yml::Value::String("meta".to_string()), serde_yml::to_value(&meta)?);
    }
    serde_yml::to_string(&yaml_value)
}

fn alttpr_dr_error_receiver(e: RollError) -> mpsc::Receiver<SeedRollUpdate> {
    let (tx, rx) = mpsc::channel(1);
    let _ = tx.try_send(SeedRollUpdate::Error(e));
    rx
}

async fn run_dungeon_randomizer(yaml_path: &Path, working_dir: &str, output_name: Option<&str>) -> Result<(), RollError> {
    const MAX_RETRIES: u8 = 2;
    for attempt in 0..=MAX_RETRIES {
        let mut cmd = Command::new(PYTHON);
        cmd.current_dir(working_dir)
            .arg("DungeonRandomizer.py")
            .arg("--customizer")
            .arg(yaml_path)
            .arg("--outputpath")
            .arg("/var/www/midos.house/seed")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(name) = output_name {
            cmd.arg("--outputname").arg(name);
        }
        let output = cmd.spawn().at_command("DungeonRandomizer.py")?.wait_with_output().await.at_command("DungeonRandomizer.py")?;
        match output.status.code() {
            Some(0) => return Ok(()),
            Some(1) => {
                let last_error = Some(String::from_utf8_lossy(&output.stderr).into_owned());
                if attempt < MAX_RETRIES {
                    sleep(Duration::from_secs(10 + 2u64.pow(attempt as u32))).await;
                    continue;
                }
                return Err(RollError::Retries { num_retries: MAX_RETRIES + 1, last_error });
            }
            _ => return Err(RollError::Wheel(wheel::Error::CommandExit { name: Cow::Borrowed("DungeonRandomizer.py"), output })),
        }
    }
    unreachable!()
}

async fn roll_seed_locally(delay_until: Option<DateTime<Utc>>, version: VersionedBranch, unlock_spoiler_log: bool, mut settings: seed::Settings) -> Result<(String, Option<PathBuf>), RollError> {
    let allow_riir = match version {
        VersionedBranch::Pinned { ref version } => version.branch() == rando::Branch::DevFenhl && (version.base(), version.supplementary()) >= (&Version::new(8, 3, 16), Some(1)), // some versions older than this generate corrupted patch files
        VersionedBranch::Latest { branch } => branch == rando::Branch::DevFenhl,
        VersionedBranch::Custom { .. } => false,
        VersionedBranch::Tww { .. } => unreachable!(),
    };
    let rando_path = match version {
        VersionedBranch::Pinned { version } => {
            version.clone_repo(allow_riir).await?;
            version.dir(allow_riir)?
        }
        VersionedBranch::Latest { branch } => {
            branch.clone_repo(allow_riir).await?;
            branch.dir(allow_riir)?
        }
        VersionedBranch::Custom { github_username, branch } => {
            let parent = {
                #[cfg(unix)] { Path::new("/opt/git/github.com").join(&*github_username).join("OoT-Randomizer").join("branch") }
                #[cfg(windows)] { UserDirs::new().ok_or(RollError::UserDirs)?.home_dir().join("git").join("github.com").join(&*github_username).join("OoT-Randomizer").join("branch") }
            };
            let dir = parent.join(&*branch);
            if dir.exists() {
                //TODO hard reset to remote instead?
                //TODO use gix instead?
                Command::new("git").arg("pull").current_dir(&dir).check("git").await?;
            } else {
                fs::create_dir_all(&parent).await?;
                gix::prepare_clone(format!("https://github.com/{github_username}/OoT-Randomizer.git"), &dir)?
                    .with_ref_name(Some(&*branch))?
                    .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?.0
                    .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
            }
            dir
        }
        VersionedBranch::Tww { .. } => unreachable!(),
    };
    #[cfg(unix)] {
        settings.insert(format!("rom"), json!(BaseDirectories::new().find_data_file(Path::new("midos-house").join("oot-ntscu-1.0.z64")).ok_or(RollError::RomPath)?));
        if settings.get("language").and_then(|language| language.as_str()).is_some_and(|language| matches!(language, "french" | "german")) {
            settings.insert(format!("pal_rom"), json!(BaseDirectories::new().find_data_file(Path::new("midos-house").join("oot-pal-1.0.z64")).ok_or(RollError::RomPath)?));
        }
    }
    settings.insert(format!("create_patch_file"), json!(true));
    settings.insert(format!("create_compressed_rom"), json!(false));
    let mut last_error = None;
    for attempt in 0.. {
        if attempt >= 3 && delay_until.is_none_or(|delay_until| Utc::now() >= delay_until) {
            return Err(RollError::Retries {
                num_retries: attempt,
                last_error,
            })
        }
        let rust_cli_path = rando_path.join("target").join("release").join({
            #[cfg(windows)] { "ootr-cli.exe" }
            #[cfg(not(windows))] { "ootr-cli" }
        });
        let use_rust_cli = fs::exists(&rust_cli_path).await?;
        let command_name = if use_rust_cli { "target/release/ootr-cli" } else { PYTHON };
        let mut rando_cmd;
        if use_rust_cli {
            rando_cmd = Command::new(rust_cli_path);
            rando_cmd.arg("--no-log");
        } else {
            rando_cmd = Command::new(PYTHON);
            rando_cmd.arg("OoTRandomizer.py");
            rando_cmd.arg("--no_log");
        }
        let mut rando_process = rando_cmd.arg("--settings=-")
            .current_dir(&rando_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .at_command(command_name)?;
        rando_process.stdin.as_mut().expect("piped stdin missing").write_all(&serde_json::to_vec(&settings)?).await.at_command(command_name)?;
        let output = rando_process.wait_with_output().await.at_command(command_name)?;
        let stderr = if output.status.success() { BufRead::lines(&*output.stderr).try_collect::<_, Vec<_>, _>().at_command(command_name)? } else {
            last_error = Some(String::from_utf8_lossy(&output.stderr).into_owned());
            continue
        };
        let world_count = settings.get("world_count").map_or(1, |world_count| world_count.as_u64().expect("world_count setting wasn't valid u64").try_into().expect("too many worlds"));
        let patch_path_prefix = if world_count > 1 { "Created patch file archive at: " } else { "Creating Patch File: " };
        let patch_path = rando_path.join("Output").join(stderr.iter().rev().find_map(|line| line.strip_prefix(patch_path_prefix)).ok_or(RollError::PatchPath)?);
        let spoiler_log_path = if unlock_spoiler_log {
            Some(rando_path.join("Output").join(stderr.iter().rev().find_map(|line| line.strip_prefix("Created spoiler log at: ")).ok_or_else(|| RollError::SpoilerLogPath {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })?).to_owned())
        } else {
            None
        };
        let patch_filename = patch_path.file_name().expect("patch file path with no file name");
        fs::rename(&patch_path, Path::new(seed::DIR).join(patch_filename)).await?;
        return Ok((
            patch_filename.to_str().expect("non-UTF-8 patch filename").to_owned(),
            spoiler_log_path,
        ))
    }
    unreachable!()
}

#[derive(Debug, thiserror::Error)]
#[cfg_attr(unix, derive(Protocol))]
#[cfg_attr(unix, async_proto(via = (String, String)))]
pub(crate) enum RollError {
    #[error(transparent)] Clone(#[from] rando::CloneError),
    #[error(transparent)] Dir(#[from] rando::DirError),
    #[error(transparent)] GitCheckout(#[from] gix::clone::checkout::main_worktree::Error),
    #[error(transparent)] GitClone(#[from] gix::clone::Error),
    #[error(transparent)] GitCloneFetch(#[from] gix::clone::fetch::Error),
    #[error(transparent)] GitValidateRefName(#[from] gix::validate::reference::name::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] OotrWeb(#[from] ootr_web::Error),
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[cfg(unix)] #[error(transparent)] RaceTime(#[from] Error),
    #[error(transparent)] RandoVersion(#[from] rando::VersionParseError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] RslScriptPath(#[from] rsl::ScriptPathError),
    #[error(transparent)] SerdePlain(#[from] serde_plain::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[cfg(unix)] #[error(transparent)] Xdg(#[from] xdg::BaseDirectoriesError),
    #[error(transparent)] Yml(#[from] serde_yml::Error),
    #[error("no hash line found in spoiler log")] AlttprHashLineNotFound,
    #[error("{display}")]
    Cloned {
        debug: String,
        display: String,
    },
    #[error("there is nothing waiting for this seed anymore")]
    ChannelClosed,
    #[cfg(unix)]
    #[error("randomizer settings must be a JSON object")]
    NonObjectSettings,
    #[error("non-UTF-8 filename")]
    OsString(std::ffi::OsString),
    #[error("randomizer did not report patch location")]
    PatchPath,
    #[cfg(unix)]
    #[error("base rom not found")]
    RomPath,
    #[error("max retries exceeded")]
    Retries {
        num_retries: u8,
        last_error: Option<String>,
    },
    #[error("failed to parse random settings script output")]
    RslScriptOutput {
        regex: &'static str,
    },
    #[cfg(unix)]
    #[error("failed to parse randomizer version from RSL script")]
    RslVersion,
    #[error("randomizer did not report spoiler log location")]
    SpoilerLogPath {
        stdout: String,
        stderr: String,
    },
    #[error("didn't find 5 hash icons on Triforce Blitz seed page")]
    TfbHash,
    #[error("failed to parse Triforce Blitz seed page")]
    TfbHtml,
    #[error("Triforce Blitz website returned unexpected URL: {0}")]
    TfbUrl(Url),
    #[error("failed to generate TWWR seed: {0}")]
    Twwr(String),
    #[cfg(windows)]
    #[error("failed to access user directories")]
    UserDirs,
    #[error("{0}")]
    AlttprDe(String),
    #[cfg_attr(not(unix), allow(dead_code))]
    #[error("{0}")]
    Avianart(String),
}

impl From<mpsc::error::SendError<SeedRollUpdate>> for RollError {
    fn from(_: mpsc::error::SendError<SeedRollUpdate>) -> Self {
        Self::ChannelClosed
    }
}

impl From<std::ffi::OsString> for RollError {
    fn from(value: std::ffi::OsString) -> Self {
        Self::OsString(value)
    }
}

impl From<(String, String)> for RollError {
    fn from((debug, display): (String, String)) -> Self {
        Self::Cloned { debug, display }
    }
}

impl<'a> From<&'a RollError> for (String, String) {
    fn from(e: &RollError) -> Self {
        (e.to_string(), format!("{e:?}"))
    }
}

#[derive(Debug)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) enum SeedRollUpdate {
    /// The seed rollers are busy and the seed has been queued.
    Queued(u64),
    /// A seed in front of us is done and we've moved to a new position in the queue.
    MovedForward(u64),
    /// We've cleared the queue and are now being rolled.
    Started,
    /// The seed has been rolled successfully.
    Done {
        seed: seed::Data,
        rsl_preset: Option<rsl::Preset>,
        version: Option<VersionedBranch>,
        unlock_spoiler_log: UnlockSpoilerLog,
    },
    /// Seed rolling failed.
    Error(RollError),
    #[cfg(unix)]
    /// A custom message.
    Message(String),
}

impl SeedRollUpdate {
    async fn handle(self, db_pool: &PgPool, ctx: &RaceContext<GlobalState>, state: &ArcRwLock<RaceState>, official_data: Option<&OfficialRaceData>, language: Language, article: &'static str, description: &str) -> Result<(), Error> {
        match self {
            Self::Queued(0) => ctx.say("I'm already rolling other multiworld seeds so your seed has been queued. It is at the front of the queue so it will be rolled next.").await?,
            Self::Queued(1) => ctx.say("I'm already rolling other multiworld seeds so your seed has been queued. There is 1 seed in front of it in the queue.").await?,
            Self::Queued(pos) => ctx.say(format!("I'm already rolling other multiworld seeds so your seed has been queued. There are {pos} seeds in front of it in the queue.")).await?,
            Self::MovedForward(0) => ctx.say("The queue has moved and your seed is now at the front so it will be rolled next.").await?,
            Self::MovedForward(1) => ctx.say("The queue has moved and there is only 1 more seed in front of yours.").await?,
            Self::MovedForward(pos) => ctx.say(format!("The queue has moved and there are now {pos} seeds in front of yours.")).await?,
            Self::Started => ctx.say(if let French = language {
                format!("Génération d'{article} {description}…")
            } else {
                format!("Rolling {article} {description}…")
            }).await?,
            Self::Done { mut seed, rsl_preset, version, unlock_spoiler_log } => {
                if let Some(seed::Files::MidosHouse { file_stem, locked_spoiler_log_path }) = seed.files() {
                    lock!(@write seed_metadata = ctx.global_state.seed_metadata; seed_metadata.insert(file_stem.to_string(), SeedMetadata {
                        locked_spoiler_log_path: locked_spoiler_log_path.clone(),
                        progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                    }));
                    if unlock_spoiler_log == UnlockSpoilerLog::Now && locked_spoiler_log_path.is_some() {
                        fs::rename(locked_spoiler_log_path.as_ref().unwrap(), Path::new(seed::DIR).join(format!("{file_stem}_Spoiler.json"))).await.to_racetime()?;
                        if let Some(ref mut data) = seed.seed_data {
                            data["locked_spoiler_log_path"] = serde_json::Value::Null;
                        }
                    }
                }
                let extra = seed.extra(Utc::now()).await.to_racetime()?;
                if let Some(OfficialRaceData { cal_event, .. }) = official_data {
                    if matches!(seed.files(), Some(seed::Files::TfbSotd { .. })) {
                        unimplemented!("Triforce Blitz seed of the day not supported for official races");
                    }
                    // Merge file_hash from spoiler log into seed so to_seed_data() includes it
                    seed.file_hash = extra.file_hash.clone();
                    if let Some(seed_data_json) = seed.to_seed_data() {
                        sqlx::query!(
                            "UPDATE races SET seed_data = $1 WHERE id = $2",
                            seed_data_json, cal_event.race.id as _,
                        ).execute(db_pool).await.to_racetime()?;
                    }
                    if let Some([ref hash1, ref hash2, ref hash3, ref hash4, ref hash5]) = extra.file_hash {
                        sqlx::query!(
                            "UPDATE races SET hash1 = $1, hash2 = $2, hash3 = $3, hash4 = $4, hash5 = $5 WHERE id = $6",
                            hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _, cal_event.race.id as _,
                        ).execute(db_pool).await.to_racetime()?;
                        if let Some(preset) = rsl_preset {
                            match seed.files().expect("received seed with no files") {
                                seed::Files::AlttprDoorRando { .. } => unreachable!(), // ALTTPR Mystery not supported
                                seed::Files::MidosHouse { file_stem, .. } => {
                                    sqlx::query!(
                                        "INSERT INTO rsl_seeds (room, file_stem, preset, hash1, hash2, hash3, hash4, hash5) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                                        format!("https://{}{}", racetime_host(), ctx.data().await.url), &*file_stem, preset as _, hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _,
                                    ).execute(db_pool).await.to_racetime()?;
                                }
                                seed::Files::OotrWeb { id, gen_time, file_stem } => {
                                    sqlx::query!(
                                        "INSERT INTO rsl_seeds (room, file_stem, preset, web_id, web_gen_time, hash1, hash2, hash3, hash4, hash5) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                                        format!("https://{}{}", racetime_host(), ctx.data().await.url), &*file_stem, preset as _, id as i64, gen_time, hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _,
                                    ).execute(db_pool).await.to_racetime()?;
                                }
                                seed::Files::TriforceBlitz { .. } | seed::Files::TfbSotd { .. } => unreachable!(), // no such thing as random settings Triforce Blitz
                                seed::Files::TwwrPermalink { .. } => unreachable!(),
                                seed::Files::AvianartSeed { .. } => unreachable!(),
                            }
                        }
                    }
                    if let Some(password) = extra.password {
                        sqlx::query!("UPDATE races SET seed_password = $1 WHERE id = $2", password.into_iter().map(char::from).collect::<String>(), cal_event.race.id as _).execute(db_pool).await.to_racetime()?;
                    }
                }
                let seed_url = match seed.files().expect("received seed with no files") {
                    seed::Files::AlttprDoorRando { uuid } => {
                        let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").expect("wrong hardcoded URL");
                        patcher_url.query_pairs_mut().append_pair("patch", &format!("{}/seed/DR_{uuid}.bps", base_uri()));
                        patcher_url.to_string()
                    }
                    seed::Files::MidosHouse { file_stem, .. } => format!("{}/seed/{file_stem}", base_uri()),
                    seed::Files::OotrWeb { id, .. } => format!("https://ootrandomizer.com/seed/get?id={id}"),
                    seed::Files::TriforceBlitz { is_dev: false, uuid } => format!("https://www.triforceblitz.com/seed/{uuid}"),
                    seed::Files::TriforceBlitz { is_dev: true, uuid } => format!("https://dev.triforceblitz.com/seeds/{uuid}"),
                    seed::Files::TfbSotd { ordinal, .. } => format!("https://www.triforceblitz.com/seed/daily/{ordinal}"),
                    seed::Files::TwwrPermalink { permalink, .. } => format!("Permalink {permalink}"),
                    seed::Files::AvianartSeed { hash, .. } => format!("https://avianart.games/perm/{hash}"),
                };
                let message = if let French = language {
                    format!("@entrants Voici votre seed : {seed_url}")
                } else {
                    format!("@entrants Here is your seed: {seed_url}")
                };
                ctx.say(message).await?;

                let twwr_tracker_url = match (&version, &official_data) {
                    (Some(VersionedBranch::Tww { tracker_link: Some(tl), .. }), Some(OfficialRaceData { event, .. })) =>
                        event.settings_string.as_deref().map(|ss| format!("https://{tl}/#/tracker/new/{}", urlencoding::encode(ss))),
                    _ => None,
                };

                // Send hash to chat with proper emoji formatting (for TWWR and other seeds)
                let game_id_for_hash = if let Some(OfficialRaceData { event, .. }) = official_data {
                    let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                    let game_id = get_game_id_from_event(&mut transaction, &event.series.to_string()).await.to_racetime()?;
                    transaction.commit().await.to_racetime()?;
                    game_id
                } else {
                    1 // Default to OOTR if no official data
                };

                if let Some(ref file_hash) = extra.file_hash {
                    let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                    let formatted_hash = format_hash_with_game_id(file_hash.clone(), &mut transaction, game_id_for_hash).await.to_racetime()?;
                    transaction.commit().await.to_racetime()?;
                    ctx.say(if let French = language {
                        format!("Hash de la seed : {formatted_hash}")
                    } else {
                        format!("Seed Hash: {formatted_hash}")
                    }).await?;
                } else if let Some(seed::Files::TwwrPermalink { seed_hash, .. }) = seed.files() {
                    if !seed_hash.is_empty() {
                        ctx.say(if let French = language {
                            format!("Hash de la seed : {seed_hash}")
                        } else {
                            format!("Seed Hash: {seed_hash}")
                        }).await?;
                    }
                }

                if let Some(VersionedBranch::Tww { identifier, github_url, .. }) = version {
                    ctx.say(if let French = language {
                        format!("Cette course utilise la version '{identifier}' du randomizer TWW: {github_url}")
                    } else {
                        format!("This race uses TWW randomizer build '{identifier}': {github_url}")
                    }).await?;
                }
                if let Some(tracker_url) = twwr_tracker_url {
                    ctx.say(format!("Tracker: {tracker_url}")).await?;
                }
                match unlock_spoiler_log {
                    UnlockSpoilerLog::Now => ctx.say("The spoiler log is also available on the seed page.").await?,
                    UnlockSpoilerLog::Progression => ctx.say("The progression spoiler is also available on the seed page. The full spoiler will be available there after the race.").await?,
                    UnlockSpoilerLog::After => if let Some(seed::Files::TfbSotd { date, .. }) = seed.files() {
                        if let Some(unlock_date) = date.succ_opt().and_then(|next| next.succ_opt()) {
                            let unlock_time = Utc.from_utc_datetime(&unlock_date.and_hms_opt(20, 0, 0).expect("failed to construct naive datetime at 20:00:00"));
                            let unlock_time = (unlock_time - Utc::now()).to_std().expect("unlock time for current daily seed in the past");
                            ctx.say(format!("The spoiler log will be available on the seed page in {}.", English.format_duration(unlock_time, true))).await?;
                        } else {
                            unimplemented!("distant future Triforce Blitz SotD")
                        }
                    } else if matches!(seed.files(), Some(seed::Files::OotrWeb { .. }) | Some(seed::Files::MidosHouse { .. })) {
                        // Only show spoiler log message for OOTR seeds
                        ctx.say(if let French = language {
                            "Le spoiler log sera disponible sur le lien de la seed après la seed."
                        } else {
                            "The spoiler log will be available on the seed page after the race."
                        }).await?;
                    },
                    UnlockSpoilerLog::Never => {}
                }
                if extra.password.is_some() {
                    ctx.say("Please note that this seed is password protected. You will receive the password to start a file ingame as soon as the countdown starts.").await?;
                }

                // Set bot race info with database-driven hash icons
                let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                set_bot_raceinfo(ctx, &seed, rsl_preset, false, &mut transaction, game_id_for_hash).await?;
                transaction.commit().await.to_racetime()?;
                
                if let Some(OfficialRaceData { cal_event, event, restreams, .. }) = official_data {
                    // send multiworld rooms
                    let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                    let mut mw_rooms_created = 0;
                    for team in cal_event.active_teams() {
                        if let Some(mw::Impl::MidosHouse) = team.mw_impl {
                            let members = team.members_roles(&mut transaction).await.to_racetime()?;
                            let mut reply_to = String::default();
                            for (member, role) in &members {
                                if event.team_config.role_is_racing(*role) {
                                    if let Some(ref racetime) = member.racetime {
                                        if !reply_to.is_empty() {
                                            reply_to.push_str(", ");
                                        }
                                        reply_to.push_str(&racetime.display_name);
                                    } else {
                                        reply_to = team.name.clone().unwrap_or_else(|| format!("(unnamed team)"));
                                        break
                                    }
                                }
                            }
                            let mut mw_room_name = if let Ok(other_team) = cal_event.race.teams().filter(|iter_team| iter_team.id != team.id).exactly_one() {
                                format!(
                                    "{}vs. {}",
                                    if let Some(game) = cal_event.race.game { format!("game {game} ") } else { String::default() },
                                    other_team.name.as_deref().unwrap_or("unnamed team"),
                                )
                            } else {
                                let mut mw_room_name = match (&cal_event.race.phase, &cal_event.race.round) {
                                    (Some(phase), Some(round)) => format!("{phase} {round}"),
                                    (Some(phase), None) => phase.clone(),
                                    (None, Some(round)) => round.clone(),
                                    (None, None) => event.display_name.clone(),
                                };
                                if let Some(game) = cal_event.race.game {
                                    mw_room_name.push_str(&format!(", game {game}"));
                                }
                                mw_room_name
                            };
                            if mw_room_name.len() > 64 {
                                // maximum room name length in database is 64
                                let ellipsis = "[…]";
                                let split_at = (0..=64 - ellipsis.len()).rev().find(|&idx| mw_room_name.is_char_boundary(idx)).unwrap_or(0);
                                mw_room_name.truncate(split_at);
                                mw_room_name.push_str(ellipsis);
                            }
                            if let Some([ref hash1, ref hash2, ref hash3, ref hash4, ref hash5]) = extra.file_hash {
                                let tracker_room_name = restreams.values().any(|restream| restream.restreamer_racetime_id.is_some()).then(|| Alphanumeric.sample_string(&mut rng(), 32));
                                let mut cmd = Command::new("/usr/local/share/midos-house/bin/ootrmwd");
                                cmd.arg("create-tournament-room");
                                cmd.arg(&mw_room_name);
                                cmd.arg(hash1.to_string());
                                cmd.arg(hash2.to_string());
                                cmd.arg(hash3.to_string());
                                cmd.arg(hash4.to_string());
                                cmd.arg(hash5.to_string());
                                for (member, role) in members {
                                    if event.team_config.role_is_racing(role) {
                                        cmd.arg(member.id.to_string());
                                    }
                                }
                                if let Some(tracker_room_name) = &tracker_room_name {
                                    cmd.arg("--tracker-room-name");
                                    cmd.arg(tracker_room_name);
                                }
                                cmd.check("ootrmwd create-tournament-room").await.to_racetime()?;
                                ctx.say(format!("{reply_to}, your Hyrule Town Hall Multiworld room named “{mw_room_name}” is now open.")).await?;
                                if let Some(tracker_room_name) = tracker_room_name {
                                    let mut all_notified = true;
                                    for restream in restreams.values() {
                                        if let Some(racetime) = &restream.restreamer_racetime_id {
                                            ctx.send_direct_message(&format!("auto-tracker room for {reply_to}: `{tracker_room_name}`"), racetime).await?;
                                        } else {
                                            all_notified = false;
                                        }
                                    }
                                    if !all_notified {
                                        ADMIN_USER.create_dm_channel(&*ctx.global_state.discord_ctx.read().await).await.to_racetime()?.say(&*ctx.global_state.discord_ctx.read().await, format!("auto-tracker room for {reply_to}: `{tracker_room_name}`")).await.to_racetime()?;
                                    }
                                }
                                mw_rooms_created += 1;
                            } else {
                                ctx.say(format!("Sorry {reply_to}, there was an error creating your Hyrule Town Hall Multiworld room. Please create one manually.")).await?;
                            }
                        }
                    }
                    if mw_rooms_created > 0 {
                        ctx.say(format!("You can find your room{} at the top of the room list after signing in with racetime.gg or Discord from the multiworld app's settings screen.", if mw_rooms_created > 1 { "s" } else { "" })).await?;
                    }
                    transaction.commit().await.to_racetime()?;
                }
                lock!(@write state = state; *state = RaceState::Rolled(seed));
            }
            Self::Error(RollError::Retries { num_retries, last_error }) | Self::Error(RollError::OotrWeb(ootr_web::Error::Retries { num_retries, last_error })) => {
                if let Some(last_error) = last_error {
                    eprintln!("seed rolling failed {num_retries} times, sample error:\n{last_error}");
                } else {
                    eprintln!("seed rolling failed {num_retries} times, no sample error recorded");
                }
                ctx.say(if let French = language {
                    format!("Désolé @entrants, le randomizer a rapporté une erreur {num_retries} fois de suite donc je vais laisser tomber. Veuillez réessayer et, si l'erreur persiste, essayer de roll une seed de votre côté et contacter TreZc0_.")
                } else {
                    format!("Sorry @entrants, the randomizer reported an error {num_retries} times, so I'm giving up on rolling the seed. Please try again. If this error persists, please report it to TreZc0_.")
                }).await?; //TODO for official races, explain that retrying is done using !seed
                lock!(@write state = state; *state = RaceState::Init);
            }
            Self::Error(e) => {
                eprintln!("seed roll error in https://{}{}: {e} ({e:?})", racetime_host(), ctx.data().await.url);
                if let Environment::Production = Environment::default() {
                    log::error!("seed roll error in https://{}{}: {e} ({e:?})", racetime_host(), ctx.data().await.url);
                }
                ctx.say("Sorry @entrants, something went wrong while rolling the seed. Please report this error to TreZc0_ and if necessary roll the seed manually.").await?;
            }
            #[cfg(unix)] Self::Message(msg) => ctx.say(msg).await?,
        }
        Ok(())
    }
}

async fn get_game_id_from_event(transaction: &mut Transaction<'_, Postgres>, series: &str) -> Result<i32, sqlx::Error> {
    // Get the game_id for the event's series
    let game_id = sqlx::query_scalar!(
        r#"
            SELECT gs.game_id
            FROM game_series gs
            WHERE gs.series = $1
        "#,
        series
    )
    .fetch_optional(&mut **transaction)
    .await?;

    // Default to OOTR (game_id = 1) if no mapping found
    Ok(game_id.unwrap_or(Some(1)).unwrap_or(1))
}



async fn format_hash_with_game_id(file_hash: [String; 5], transaction: &mut Transaction<'_, Postgres>, game_id: i32) -> Result<String, sqlx::Error> {
    let mut emojis = Vec::new();
    for icon_name in file_hash {
        if let Some(hash_icon_data) = HashIconData::by_name(transaction, game_id, &icon_name).await? {
            if let Some(emoji) = hash_icon_data.racetime_emoji.as_ref() {
                emojis.push(emoji.clone());
            }
        }
    }
    Ok(emojis.join(" "))
}

fn format_password(password: [OcarinaNote; 6]) -> impl fmt::Display {
    password.into_iter().map(|icon| icon.to_racetime_emoji()).format(" ")
}

fn ocarina_note_to_ootr_discord_emoji(note: OcarinaNote) -> ReactionType {
    ReactionType::Custom {
        animated: false,
        id: EmojiId::new(match note {
            OcarinaNote::A => 658692216373379072,
            OcarinaNote::CDown => 658692230479085570,
            OcarinaNote::CRight => 658692260002791425,
            OcarinaNote::CLeft => 658692245771517962,
            OcarinaNote::CUp => 658692275152355349,
        }),
        name: Some(match note {
            OcarinaNote::A => format!("staffA"),
            OcarinaNote::CDown => format!("staffDown"),
            OcarinaNote::CRight => format!("staffRight"),
            OcarinaNote::CLeft => format!("staffLeft"),
            OcarinaNote::CUp => format!("staffUp"),
        }),
    }
}

async fn room_options(goal_str: String, goal_is_custom: bool, event: &event::Data<'_>, cal_event: &cal::Event, info_user: String, info_bot: String, auto_start: bool) -> racetime::StartRace {
    racetime::StartRace {
        goal: goal_str,
        goal_is_custom,
        team_race: event.team_config.is_racetime_team_format() && matches!(cal_event.kind, cal::EventKind::Normal),
        invitational: !matches!(cal_event.race.entrants, Entrants::Open),
        unlisted: cal_event.is_private_async_part(),
        partitionable: false,
        hide_entrants: event.hide_entrants,
        ranked: cal_event.is_private_async_part() || event.series != Series::TriforceBlitz && !matches!(cal_event.race.schedule, RaceSchedule::Async { .. }), //HACK: private async parts must be marked as ranked so they don't immediately get published on finish/cancel
        require_even_teams: true,
        start_delay: (if cal_event.race.entrants == Entrants::Open {
            event.start_delay_open.unwrap_or(event.start_delay)
        } else {
            event.start_delay
        }) as u8,
        time_limit: 24,
        time_limit_auto_complete: false,
        streaming_required: !Environment::default().is_dev() && !cal_event.is_private_async_part(),
        allow_comments: true,
        hide_comments: true,
        allow_prerace_chat: !event.restrict_chat_in_qualifiers || cal_event.race.phase.as_ref().is_none_or(|phase| phase != "Qualifier"),
        allow_midrace_chat: !event.restrict_chat_in_qualifiers || cal_event.race.phase.as_ref().is_none_or(|phase| phase != "Qualifier"),
        allow_non_entrant_chat: false, // only affects the race while it's ongoing, so !monitor still works
        chat_message_delay: 0,
        info_user, info_bot, auto_start,
    }
}

async fn set_bot_raceinfo(ctx: &RaceContext<GlobalState>, seed: &seed::Data, rsl_preset: Option<rsl::Preset>, show_password: bool, transaction: &mut Transaction<'_, Postgres>, game_id: i32) -> Result<(), Error> {
    let extra = seed.extra(Utc::now()).await.to_racetime()?;

    // For TWWR, we handle the format differently (permalink + seed hash on one line)
    let is_twwr = matches!(seed.files(), Some(seed::Files::TwwrPermalink { .. }));

    let file_hash_str = if !is_twwr && extra.file_hash.is_some() {
        format_hash_with_game_id(extra.file_hash.clone().unwrap(), transaction, game_id).await.to_racetime()?
    } else {
        String::new()
    };

    ctx.set_bot_raceinfo(&format!(
        "{rsl_preset}{file_hash}{sep}{password}{newline}{seed_url}",
        rsl_preset = rsl_preset.map(|preset| format!("{}\n", preset.race_info())).unwrap_or_default(),
        file_hash = file_hash_str,
        sep = if !is_twwr && extra.file_hash.is_some() && extra.password.is_some() && show_password { " | " } else { "" },
        password = extra.password.filter(|_| show_password).map(|password| format_password(password).to_string()).unwrap_or_default(),
        newline = if (!is_twwr && extra.file_hash.is_some()) || extra.password.is_some() && show_password { "\n" } else { "" },
        seed_url = match seed.files().expect("received seed with no files") {
            seed::Files::AlttprDoorRando { uuid } => {
                let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").expect("wrong hardcoded URL");
                patcher_url.query_pairs_mut().append_pair("patch", &format!("{}/seed/DR_{uuid}.bps", base_uri()));
                patcher_url.to_string()
            }
            seed::Files::MidosHouse { file_stem, .. } => format!("{}/seed/{file_stem}", base_uri()),
            seed::Files::OotrWeb { id, .. } => format!("https://ootrandomizer.com/seed/get?id={id}"),
            seed::Files::TriforceBlitz { is_dev: false, uuid } => format!("https://www.triforceblitz.com/seed/{uuid}"),
            seed::Files::TriforceBlitz { is_dev: true, uuid } => format!("https://dev.triforceblitz.com/seeds/{uuid}"),
            seed::Files::TfbSotd { ordinal, .. } => format!("https://www.triforceblitz.com/seed/daily/{ordinal}"),
            seed::Files::TwwrPermalink { permalink, seed_hash } => {
                format!("Permalink: {permalink} | Seed Hash: {seed_hash}")
            },
            seed::Files::AvianartSeed { hash, .. } => format!("https://avianart.games/perm/{hash}"),
        },
    )).await
}

#[derive(Clone, Copy)]
struct Breaks {
    duration: Duration,
    interval: Duration,
}

#[derive(Clone)]
pub(crate) struct CrosskeysRaceOptions {
    /// Custom_choices keys where every team agreed (all said "yes").
    agreed: HashSet<String>,
}

impl CrosskeysRaceOptions {
    /// Human-readable label for a seed-option choice key, or None if it is a race-rules key.
    fn key_to_seed_label(key: &str) -> Option<&'static str> {
        match key {
            "all_dungeons"  => Some("a goal of all dungeons"),
            "flute"         => Some("starting activated flute"),
            "inverted"      => Some("inverted world state"),
            "keydrop"       => Some("enemy and pot keydrop"),
            "mirror_scroll" => Some("starting mirror scroll"),
            "pseudoboots"   => Some("starting pseudoboots"),
            "zw"            => Some("zw enabled"),
            _               => None,
        }
    }

    pub(crate) fn as_seed_options_str(&self) -> String {
        // Emit labels in a stable order matching the original display order.
        let labels: Vec<&str> = ["all_dungeons", "flute", "inverted", "keydrop", "mirror_scroll", "pseudoboots", "zw"]
            .into_iter()
            .filter(|&k| self.agreed.contains(k))
            .filter_map(Self::key_to_seed_label)
            .collect();
        English.join_str_opt(labels).unwrap_or_else(|| "base settings".to_owned())
    }

    pub(crate) fn as_race_options_str(&self) -> String {
        let hovering = if self.agreed.contains("hovering") {
            "hovering and moldorm bouncing ALLOWED"
        } else {
            "hovering and moldorm bouncing BANNED"
        };
        let delay = if self.agreed.contains("no_delay") {
            "no stream delay"
        } else {
            "stream delay(10m)"
        };
        format!("{hovering} and {delay}")
    }

    pub(crate) fn as_race_options_str_no_delay(&self) -> String {
        if self.agreed.contains("hovering") {
            "hovering and moldorm bouncing ALLOWED".to_owned()
        } else {
            "hovering and moldorm bouncing BANNED".to_owned()
        }
    }

    pub(crate) async fn for_race(db_pool: &PgPool, race: &Race) -> Self {
        let teams = race.teams();
        let team_rows = sqlx::query!("SELECT custom_choices FROM teams WHERE id = ANY($1)", teams.map(|team| team.id).collect_vec() as _)
            .fetch_all(db_pool).await.expect("Database read failed");
        let num_teams = team_rows.len();
        let agreed = if num_teams == 0 {
            HashSet::default()
        } else {
            let mut counts: HashMap<String, usize> = HashMap::default();
            for row in &team_rows {
                if let Some(obj) = row.custom_choices.as_object() {
                    for (key, value) in obj {
                        if value == "yes" {
                            *counts.entry(key.clone()).or_default() += 1;
                        }
                    }
                }
            }
            counts.into_iter().filter(|(_, count)| *count >= num_teams).map(|(k, _)| k).collect()
        };
        CrosskeysRaceOptions { agreed }
    }
}

#[derive(Clone)]
pub(crate) struct AlttprDeRaceOptions {
    pub(crate) mode: Option<String>,
    /// Custom choices from player signups, merged together.
    pub(crate) custom_choices: HashMap<String, String>,
    /// Choices that affect race rules display but not seed rolling.
    pub(crate) display_only_choices: Vec<String>,
}

impl AlttprDeRaceOptions {
    /// Convert a custom choice key to a human-readable label
    fn key_to_label(key: &str) -> Option<&'static str> {
        match key {
            "pool_hard" => Some("Hard Item Pool"),
            "pool_expert" => Some("Expert Item Pool"),
            "pool" => Some("Item Pool"),
            "pots" | "lottery" => Some("Pottery Shuffle"),
            "all_dungeons" | "ad" => Some("All Dungeons"),
            "flute" => Some("Flute"),
            "hovering" => Some("Hovering"),
            "inverted" => Some("Inverted"),
            "keydrop" | "kds" => Some("Keydrop Shuffle"),
            "mirror_scroll" | "scroll" => Some("Mirror Scroll"),
            "no_delay" => Some("No Delay"),
            "pseudoboots" => Some("Pseudoboots"),
            "boots" => Some("Boots"),
            "zw" => Some("ZW"),
            "boss" => Some("Boss Shuffle"),
            "retro" => Some("Retro"),
            "bones" => Some("Bonk Rocks"),
            "shop" => Some("Shop Shuffle"),
            "keys" => Some("Key Shuffle"),
            "dmg" => Some("Damage Shuffle"),
            "bag" => Some("Progressive Bag"),
            "door" => Some("Door Shuffle"),
            "gtskips" => Some("GT Skips"),
            _ => None,
        }
    }

    /// Get the custom choices as human-readable labels
    pub(crate) fn custom_choices_labels(&self) -> Vec<String> {
        self.custom_choices
            .keys()
            .map(|k| Self::key_to_label(k).unwrap_or(k).to_string())
            .collect()
    }

    pub(crate) fn mode_display(&self) -> Option<String> {
        self.mode.as_ref().map(|mode| {
            let mut chars = mode.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
    }

    pub(crate) fn as_seed_options_str(&self) -> String {
        match self.mode_display() {
            Some(mode_display) => format!("{} mode", mode_display),
            None => format!("mode not yet drafted"),
        }
    }

    pub(crate) fn as_race_options_str(&self) -> String {
        if self.display_only_choices.is_empty() {
            format!("standard race rules")
        } else {
            format!("standard race rules ({})", self.display_only_choices.join(", "))
        }
    }

    /// Build the full URL for generating a seed from boothisman.de
    pub(crate) fn seed_url(&self) -> Option<String> {
        let mode = self.mode.as_ref()?;
        let base_url = format!("https://www.boothisman.de/Turnier/{}.php", mode);
        if self.custom_choices.is_empty() {
            Some(base_url)
        } else {
            let params: Vec<String> = self.custom_choices.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            Some(format!("{}?{}", base_url, params.join("&")))
        }
    }

    pub(crate) async fn for_race(db_pool: &PgPool, race: &Race, round_modes: Option<&HashMap<String, String>>) -> Self {
        // Check round_modes first (for swiss events with fixed mode per round)
        let mode = if let (Some(round_modes), Some(round)) = (round_modes, &race.round) {
            round_modes.get(round).cloned()
        } else {
            None
        }.or_else(|| {
            // Fall back to draft state
            race.draft.as_ref().and_then(|draft| {
                let game = race.game.unwrap_or(1);
                draft.settings.get(&*format!("game{game}_preset")).map(|s| s.to_string())
            })
        });

        // Get custom choices from teams
        let teams = race.teams();
        let team_rows = sqlx::query!("SELECT custom_choices FROM teams WHERE id = ANY($1)", teams.map(|team| team.id).collect_vec() as _)
            .fetch_all(db_pool).await.expect("Database read failed");

        // Collect raw choices from all teams (only include if BOTH players said yes)
        // For alttprde, a setting is only enabled if both players opted in
        let mut choice_counts: HashMap<String, u32> = HashMap::new();
        let num_teams = team_rows.len() as u32;

        for row in &team_rows {
            if let Some(obj) = row.custom_choices.as_object() {
                for (key, value) in obj {
                    let is_yes = match value {
                        serde_json::Value::String(s) => s == "yes" || s == "1" || s == "true",
                        serde_json::Value::Bool(b) => *b,
                        serde_json::Value::Number(n) => n.as_i64().is_some_and(|n| n != 0),
                        _ => false,
                    };
                    if is_yes {
                        *choice_counts.entry(key.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        // Build URL params - only include choices where both players said yes
        let mut custom_choices = HashMap::new();
        let mut display_only_choices = Vec::new();
        // Handle pool settings with explicit priority: expert (2) > hard (1)
        let expert_ok = choice_counts.get("pool_expert").is_some_and(|&c| c >= num_teams && num_teams > 0);
        let hard_ok = choice_counts.get("pool_hard").is_some_and(|&c| c >= num_teams && num_teams > 0);
        if expert_ok {
            custom_choices.insert("pool".to_owned(), "2".to_owned());
        } else if hard_ok {
            custom_choices.insert("pool".to_owned(), "1".to_owned());
        }
        for (key, count) in choice_counts {
            if count >= num_teams && num_teams > 0 {
                // Both players agreed to this option
                let url_value = match key.as_str() {
                    // Special case: pottery becomes "lottery"
                    "pots" => "lottery".to_owned(),
                    // Handled above with explicit priority
                    "pool_hard" | "pool_expert" => continue,
                    // Display-only choices: shown in race room but not passed to the seed API
                    "gtskips" => {
                        display_only_choices.push("GT Skips".to_owned());
                        continue;
                    }
                    // All other boolean choices become "1"
                    _ => "1".to_owned(),
                };
                custom_choices.insert(key, url_value);
            }
        }

        AlttprDeRaceOptions { mode, custom_choices, display_only_choices }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct AlttprDoorRandoYaml {
    placements: HashMap<u8, AlttprDoorRandoPlacements>,
    settings: HashMap<u8, AlttprDoorRandoSetting>,
    start_inventory: HashMap<u8, &'static [&'static str]>,
    meta: AlttprDoorRandoMeta
}

#[derive(Clone, Serialize)]
pub(crate) struct AlttprDoorRandoPlacements {
    #[serde(rename = "Skull Woods - Pinball Room")]
    pinball_room: &'static str,
}

#[derive(Clone, Serialize)]
pub(crate) struct AlttprDoorRandoSetting {
    accessibility: &'static str,
    bigkeyshuffle: u8,
    compassshuffle: u8,
    crystals_ganon: &'static str,
    crystals_gt: &'static str,
    dropshuffle: &'static str,
    flute_mode: &'static str,
    goal: &'static str,
    item_functionality: &'static str,
    key_logic_algorithm: &'static str,
    keyshuffle: &'static str,
    linked_drops: &'static str,
    mapshuffle: u8,
    mirrorscroll: u8,
    mode: &'static str,
    pottery: &'static str,
    pseudoboots: u8,
    shuffle: &'static str,
    shuffletavern: u8,
    skullwoods: &'static str,
}

#[derive(Clone, Serialize)]
pub(crate) struct AlttprDoorRandoMeta {
    bps: bool,
    name: String,
    race: bool,
    skip_playthrough: bool,
    spoiler: &'static str,
    suppress_rom: bool
}

impl Breaks {
    fn format(&self, language: Language) -> String {
        if let French = language {
            format!("{} toutes les {}", French.format_duration(self.duration, true), French.format_duration(self.interval, true))
        } else {
            format!("{} every {}", English.format_duration(self.duration, true), English.format_duration(self.interval, true))
        }
    }
}

impl FromStr for Breaks {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (_, duration, interval) = regex_captures!("^(.+?) ?e(?:very)? ?(.+?)$", s).ok_or(())?;
        Ok(Self {
            duration: parse_duration(duration, Some(DurationUnit::Minutes)).ok_or(())?,
            interval: parse_duration(interval, Some(DurationUnit::Hours)).ok_or(())?,
        })
    }
}

#[derive(Default)]
enum RaceState {
    #[default]
    Init,
    Draft {
        state: Draft,
        unlock_spoiler_log: UnlockSpoilerLog,
    },
    Rolling,
    Rolled(seed::Data),
    SpoilerSent,
}

#[derive(Clone)]
struct OfficialRaceData {
    cal_event: cal::Event,
    event: event::Data<'static>,
    restreams: HashMap<Url, RestreamState>,
    entrants: Vec<String>,
    fpa_invoked: bool,
    breaks_used: bool,
}

#[derive(Default, Clone)]
struct RestreamState {
    language: Option<Language>,
    restreamer_racetime_id: Option<String>,
    ready: bool,
}

struct Handler {
    official_data: Option<OfficialRaceData>,
    high_seed_name: String,
    low_seed_name: String,
    breaks: Option<Breaks>,
    break_notifications: Option<tokio::task::JoinHandle<()>>,
    fpa_enabled: bool,
    locked: bool,
    password_sent: bool,
    race_state: ArcRwLock<RaceState>,
    cleaned_up: Arc<AtomicBool>,
    finish_timeout: Option<tokio::task::JoinHandle<()>>,
}

impl Handler {
    /// For `existing_state`, `Some(None)` means this is an existing race room with unknown state, while `None` means this is a new race room.
    async fn should_handle_inner(race_data: &RaceData, global_state: Arc<GlobalState>, existing_state: Option<Option<&Self>>) -> bool {
        // Accept rooms with goals known to the DB OR custom goals (generic events use custom goals)
        let is_known = global_state.known_goals.contains(&(race_data.category.slug.clone(), race_data.goal.name.clone(), race_data.goal.custom));
        if !is_known && !race_data.goal.custom { return false }
        if let Some(existing_state) = existing_state {
            if let Some(existing_state) = existing_state {
                if let RaceStatusValue::Finished | RaceStatusValue::Cancelled = race_data.status.value { return !existing_state.cleaned_up.load(atomic::Ordering::SeqCst) && race_data.ended_at.is_none_or(|ended_at| Utc::now() - ended_at < TimeDelta::hours(1)) }
            } else {
                if let RaceStatusValue::Finished | RaceStatusValue::Cancelled = race_data.status.value { return false }
            }
        } else {
            if let RaceStatusValue::Finished | RaceStatusValue::Cancelled = race_data.status.value { return false }
            lock!(clean_shutdown = global_state.clean_shutdown; {
                if !clean_shutdown.should_handle_new() {
                    unlock!();
                    return false
                }
                let room = OpenRoom::RaceTime {
                    room_url: race_data.url.clone(),
                    public: !race_data.unlisted,
                };
                if !clean_shutdown.open_rooms.insert(room.clone()) {
                    // Previous handler is still mid-cleanup (handled_races was cleared but
                    // open_rooms hasn't been updated yet by our task() spawn). Skip this scan
                    // cycle; the next one will pick it up cleanly.
                    unlock!();
                    return false
                }
                clean_shutdown.updates.send(CleanShutdownUpdate::RoomOpened(room)).allow_unreceived();
            });
        }
        true
    }

    fn is_official(&self) -> bool { self.official_data.is_some() }

    /// Returns the language for this race, using the event's configured language.
    fn language(&self) -> Language {
        self.official_data.as_ref().map(|d| d.event.language).unwrap_or(English)
    }

    /// Returns the rando version for this race from event DB config.
    fn effective_rando_version(&self) -> VersionedBranch {
        if let Some(ref official_data) = self.official_data {
            official_data.event.rando_version.clone().unwrap_or(VersionedBranch::Latest { branch: rando::Branch::Dev })
        } else {
            VersionedBranch::Latest { branch: rando::Branch::Dev }
        }
    }

    /// Returns the unlock_spoiler_log mode for this race from event DB config.
    fn effective_unlock_spoiler_log(&self, spoiler_seed: bool) -> UnlockSpoilerLog {
        if spoiler_seed {
            UnlockSpoilerLog::Now
        } else if let Some(ref official_data) = self.official_data {
            match official_data.event.spoiler_unlock.as_str() {
                "after" => UnlockSpoilerLog::After,
                "immediately" => UnlockSpoilerLog::Now,
                _ => UnlockSpoilerLog::Never,
            }
        } else {
            UnlockSpoilerLog::Never
        }
    }

    /// Returns the preroll mode for this race from event DB config.
    fn effective_preroll_mode(&self) -> PrerollMode {
        if let Some(ref official_data) = self.official_data {
            match official_data.event.preroll_mode.as_str() {
                "short" => PrerollMode::Short,
                "medium" => PrerollMode::Medium,
                "long" => PrerollMode::Long,
                _ => PrerollMode::None,
            }
        } else {
            PrerollMode::None
        }
    }

    async fn can_monitor(&self, ctx: &RaceContext<GlobalState>, is_monitor: bool, msg: &ChatMessage) -> sqlx::Result<bool> {
        if is_monitor { return Ok(true) }
        if let Some(OfficialRaceData { ref event, .. }) = self.official_data {
            if let Some(UserData { ref id, .. }) = msg.user {
                if let Some(user) = User::from_racetime(&ctx.global_state.db_pool, id).await? {
                    return sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM organizers WHERE series = $1 AND event = $2 AND organizer = $3) AS "exists!""#, event.series as _, &event.event, user.id as _).fetch_one(&ctx.global_state.db_pool).await
                }
            }
        }
        Ok(false)
    }

    async fn send_settings(&self, ctx: &RaceContext<GlobalState>, preface: &str, reply_to: &str) -> Result<(), Error> {
        if let Some(draft_kind) = self.official_data.as_ref().and_then(|OfficialRaceData { event, .. }| event.draft_kind()) {
            let available_settings = lock!(@read state = self.race_state; if let RaceState::Draft { state: ref draft, .. } = *state {
                match draft.next_step(&draft_kind, self.official_data.as_ref().and_then(|OfficialRaceData { cal_event, .. }| cal_event.race.game), &mut draft::MessageContext::RaceTime { high_seed_name: &self.high_seed_name, low_seed_name: &self.low_seed_name, reply_to }).await.to_racetime()?.kind {
                    draft::StepKind::GoFirst => None,
                    draft::StepKind::Ban { available_settings, .. } => Some(available_settings.all().map(|setting| setting.description).collect()),
                    draft::StepKind::Pick { available_choices, .. } => Some(available_choices.all().map(|setting| setting.description).collect()),
                    draft::StepKind::BooleanChoice { .. } | draft::StepKind::Done(_) | draft::StepKind::DoneRsl { .. } | draft::StepKind::PickPreset { .. } => Some(Vec::default()),
                }
            } else {
                None
            });
            let available_settings = available_settings.unwrap_or_else(|| match draft_kind {
                draft::Kind::S7 => s::S7_SETTINGS.into_iter().map(|setting| Cow::Owned(setting.description())).collect(),
                draft::Kind::MultiworldS3 => mw::S3_SETTINGS.iter().copied().map(|mw::Setting { description, .. }| Cow::Borrowed(description)).collect(),
                draft::Kind::MultiworldS4 => mw::S4_SETTINGS.iter().copied().map(|mw::Setting { description, .. }| Cow::Borrowed(description)).collect(),
                draft::Kind::MultiworldS5 => mw::S5_SETTINGS.iter().copied().map(|mw::Setting { description, .. }| Cow::Borrowed(description)).collect(),
                draft::Kind::RslS7 => rsl::FORCE_OFF_SETTINGS.into_iter().map(|rsl::ForceOffSetting { name, .. }| Cow::Owned(format!("{name}: blocked or banned")))
                    .chain(rsl::FIFTY_FIFTY_SETTINGS.into_iter().chain(rsl::MULTI_OPTION_SETTINGS).map(|rsl::MultiOptionSetting { name, options, .. }| Cow::Owned(format!("{name}: {}", English.join_str_with("or", nonempty_collections::iter::once("blocked").chain(options.iter().map(|(name, _, _, _)| *name)))))))
                    .collect(),
                draft::Kind::TournoiFrancoS3 => fr::S3_SETTINGS.into_iter().map(|fr::Setting { description, .. }| Cow::Borrowed(description)).collect(),
                draft::Kind::TournoiFrancoS4 => fr::S4_SETTINGS.into_iter().map(|fr::Setting { description, .. }| Cow::Borrowed(description)).collect(),
                draft::Kind::TournoiFrancoS5 => fr::S5_SETTINGS.into_iter().map(|fr::Setting { description, .. }| Cow::Borrowed(description)).collect(),
                draft::Kind::PickOnly { options, .. } | draft::Kind::BanPick { options, .. } | draft::Kind::BanOnly { options, .. } => options.iter().map(|p| Cow::Owned(p.display_name.clone())).collect(),
            });
            if available_settings.is_empty() {
                ctx.say(if let French = self.language() {
                    format!("Désolé {reply_to}, aucun setting n'est demandé pour le moment.")
                } else {
                    format!("Sorry {reply_to}, no settings are currently available.")
                }).await?;
            } else {
                ctx.say(preface).await?;
                for setting in available_settings {
                    ctx.say(setting).await?;
                }
            }
        } else {
            ctx.say(format!("Sorry {reply_to}, this event doesn't have a settings draft.")).await?;
        }
        Ok(())
    }

    async fn advance_draft(&self, ctx: &RaceContext<GlobalState>, state: &RaceState) -> Result<(), Error> {
        let lang = self.language();
        let draft_kind = self.official_data.as_ref().and_then(|OfficialRaceData { event, .. }| event.draft_kind()).expect("advance_draft called without draft kind");
        let RaceState::Draft { state: ref draft, unlock_spoiler_log } = *state else { unreachable!() };
        let step = draft.next_step(&draft_kind, self.official_data.as_ref().and_then(|OfficialRaceData { cal_event, .. }| cal_event.race.game), &mut draft::MessageContext::RaceTime { high_seed_name: &self.high_seed_name, low_seed_name: &self.low_seed_name, reply_to: "friend" }).await.to_racetime()?;
        match step.kind {
            draft::StepKind::Done(settings) => {
                let (article, description) = if let French = lang {
                    ("une", format!("seed avec {}", step.message))
                } else {
                    ("a", format!("seed with {}", step.message))
                };
                // Dispatch seed rolling based on seed_gen_type from event DB config.
                let event_seed_gen_type = self.official_data.as_ref().and_then(|d| d.event.seed_gen_type.as_ref());
                match event_seed_gen_type {
                    Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::Boothisman }) => {
                        let cal_event = self.official_data.as_ref().expect("AlttprDoorRando/Boothisman must have official_data").cal_event.clone();
                        self.roll_alttprde9_seed(ctx, cal_event, lang, article).await;
                    }
                    Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::MutualChoices }) => {
                        let cal_event = self.official_data.as_ref().expect("AlttprDoorRando/MutualChoices must have official_data").cal_event.clone();
                        self.roll_crosskeys2025_seed(ctx, cal_event, lang, article).await;
                    }
                    Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::MysteryPool { .. } }) => {
                        let cal_event = self.official_data.as_ref().expect("AlttprDoorRando/MysteryPool must have official_data").cal_event.clone();
                        self.roll_mysteryd20_seed(ctx, cal_event, lang, article).await;
                    }
                    Some(seed_gen_type::SeedGenType::AlttprAvianart) => {
                        let cal_event = self.official_data.as_ref().expect("AlttprAvianart must have official_data").cal_event.clone();
                        let preset = settings.get("preset")
                            .and_then(|v| v.as_str())
                            .expect("Avianart Done settings missing preset")
                            .to_owned();
                        self.roll_rivals_cup_seed(ctx, cal_event, preset, lang, article).await;
                    }
                    _ => {
                        self.roll_seed(ctx, self.effective_preroll_mode(), self.effective_rando_version(), settings, unlock_spoiler_log, lang, article, description).await;
                    }
                }
            }
            draft::StepKind::DoneRsl { preset, world_count } => {
                let (article, description) = if let French = lang {
                    ("une", format!("seed avec {}", step.message))
                } else {
                    ("a", format!("seed with {}", step.message))
                };
                self.roll_rsl_seed(ctx, preset, world_count, unlock_spoiler_log, lang, article, description).await;
            }
            draft::StepKind::GoFirst | draft::StepKind::Ban { .. } | draft::StepKind::Pick { .. } | draft::StepKind::BooleanChoice { .. } | draft::StepKind::PickPreset { .. } => ctx.say(step.message).await?,
        }
        Ok(())
    }

    async fn draft_action(&self, ctx: &RaceContext<GlobalState>, sender: Option<&UserData>, action: draft::Action) -> Result<(), Error> {
        let reply_to = sender.map_or("friend", |user| &user.name);
        if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
            lock!(@write state = self.race_state; if let Some(draft_kind) = self.official_data.as_ref().and_then(|OfficialRaceData { event, .. }| event.draft_kind()) {
                match *state {
                    RaceState::Init => match draft_kind {
                        draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => ctx.say(format!("Sorry {reply_to}, no draft has been started. Use \"!seed draft\" to start one.")).await?,
                        draft::Kind::RslS7 => ctx.say(format!("Sorry {reply_to}, no draft has been started. Use \"!seed draft\" to start one. For more info about these options, use !presets")).await?,
                        draft::Kind::TournoiFrancoS3 => ctx.say(format!("Désolé {reply_to}, le draft n'a pas débuté. Utilisez \"!seed draft\" pour en commencer un. Pour plus d'infos, utilisez !presets")).await?,
                        draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => ctx.say(format!("Sorry {reply_to}, no draft has been started. Use \"!seed draft\" to start one. For more info about these options, use !presets / le draft n'a pas débuté. Utilisez \"!seed draft\" pour en commencer un. Pour plus d'infos, utilisez !presets")).await?,
                        draft::Kind::PickOnly { label, .. } | draft::Kind::BanPick { label, .. } | draft::Kind::BanOnly { label, .. } => ctx.say(format!("Sorry {reply_to}, the {label} draft for this event is done in Discord before the race starts.")).await?,
                    },
                    RaceState::Draft { state: ref mut draft, .. } => {
                        let is_active_team = if let Some(OfficialRaceData { ref cal_event, ref event, .. }) = self.official_data {
                            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                            let is_active_team = if_chain! {
                                if let Some(sender) = sender;
                                if let Some(user) = User::from_racetime(&mut *transaction, &sender.id).await.to_racetime()?;
                                if let Some(team) = Team::from_event_and_member(&mut transaction, event.series, &event.event, user.id).await.to_racetime()?;
                                then {
                                    draft.is_active_team(&draft_kind, cal_event.race.game, team.id).await.to_racetime()?
                                } else {
                                    false
                                }
                            };
                            transaction.commit().await.to_racetime()?;
                            is_active_team
                        } else {
                            true
                        };
                        if is_active_team {
                            match draft.apply(&draft_kind, self.official_data.as_ref().and_then(|OfficialRaceData { cal_event, .. }| cal_event.race.game), &mut draft::MessageContext::RaceTime { high_seed_name: &self.high_seed_name, low_seed_name: &self.low_seed_name, reply_to }, action).await.to_racetime()? {
                                Ok(_) => self.advance_draft(ctx, &state).await?,
                                Err(mut error_msg) => {
                                    unlock!();
                                    // can't send messages longer than 1000 characters
                                    while !error_msg.is_empty() {
                                        let mut idx = error_msg.len().min(1000);
                                        while !error_msg.is_char_boundary(idx) { idx -= 1 }
                                        let suffix = error_msg.split_off(idx);
                                        ctx.say(error_msg).await?;
                                        error_msg = suffix;
                                    }
                                    return Ok(())
                                }
                            }
                        } else {
                            match draft_kind {
                                draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => ctx.say(format!("Sorry {reply_to}, it's not your turn in the settings draft.")).await?,
                                draft::Kind::RslS7 => ctx.say(format!("Sorry {reply_to}, it's not your turn in the weights draft.")).await?,
                                draft::Kind::TournoiFrancoS3 => ctx.say(format!("Désolé {reply_to}, mais ce n'est pas votre tour.")).await?,
                                draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => ctx.say(format!("Sorry {reply_to}, it's not your turn in the settings draft. / mais ce n'est pas votre tour.")).await?,
                                draft::Kind::PickOnly { label, .. } | draft::Kind::BanPick { label, .. } | draft::Kind::BanOnly { label, .. } => ctx.say(format!("Sorry {reply_to}, it's not your turn in the {label} draft.")).await?,
                            }
                        }
                    }
                    RaceState::Rolling | RaceState::Rolled(_) | RaceState::SpoilerSent => match self.language() {
                        French => ctx.say(format!("Désolé {reply_to}, mais il n'y a pas de draft, ou la phase de pick&ban est terminée.")).await?,
                        _ => ctx.say(format!("Sorry {reply_to}, there is no settings draft this race or the draft is already completed.")).await?,
                    },
                }
            } else {
                ctx.say(format!("Sorry {reply_to}, this event doesn't have a settings draft.")).await?;
            });
        } else {
            match self.language() {
                French => ctx.say(format!("Désolé {reply_to}, mais la race a débuté.")).await?,
                _ => ctx.say(format!("Sorry {reply_to}, but the race has already started.")).await?,
            }
        }
        Ok(())
    }

    async fn roll_seed_inner(&self, ctx: &RaceContext<GlobalState>, delay_until: Option<DateTime<Utc>>, mut updates: mpsc::Receiver<SeedRollUpdate>, language: Language, article: &'static str, description: String, suppress_preamble: bool) {
        let db_pool = ctx.global_state.db_pool.clone();
        let ctx = ctx.clone();
        let state = self.race_state.clone();
        let official_data = self.official_data.clone();
        tokio::spawn(async move {
            lock!(@write state = state; *state = RaceState::Rolling); //TODO ensure only one seed is rolled at a time
            let mut seed_state = None::<SeedRollUpdate>;
            if let Some(delay) = delay_until.and_then(|delay_until| (delay_until - Utc::now()).to_std().ok()) {
                // don't want to give an unnecessarily exact estimate if the room was opened automatically 30 or 60 minutes ahead of start
                let display_delay = if delay > Duration::from_secs(14 * 60) && delay < Duration::from_secs(16 * 60) {
                    Duration::from_secs(15 * 60)
                } else if delay > Duration::from_secs(44 * 60) && delay < Duration::from_secs(46 * 60) {
                    Duration::from_secs(45 * 60)
                } else if delay > Duration::from_secs(19 * 60) && delay < Duration::from_secs(21 * 60) {
                    Duration::from_secs(20 * 60)
                } else {
                    delay
                };
                if !suppress_preamble {
                    ctx.say(if let French = language {
                        format!("Votre {description} sera postée dans {}.", French.format_duration(display_delay, true))
                    } else {
                        format!("Your {description} will be posted in {}.", English.format_duration(display_delay, true))
                    }).await?;
                }
                let mut sleep = pin!(sleep_until(Instant::now() + delay));
                loop {
                    select! {
                        () = &mut sleep => {
                            if let Some(update) = seed_state.take() {
                                update.handle(&db_pool, &ctx, &state, official_data.as_ref(), language, article, &description).await?;
                            }
                            while let Some(update) = updates.recv().await {
                                update.handle(&db_pool, &ctx, &state, official_data.as_ref(), language, article, &description).await?;
                            }
                            break
                        }
                        Some(update) = updates.recv() => seed_state = Some(update),
                    }
                }
            } else {
                while let Some(update) = updates.recv().await {
                    update.handle(&db_pool, &ctx, &state, official_data.as_ref(), language, article, &description).await?;
                }
            }
            Ok::<_, Error>(())
        });
    }

    async fn roll_seed(&self, ctx: &RaceContext<GlobalState>, preroll: PrerollMode, version: VersionedBranch, settings: seed::Settings, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_seed(preroll, true, delay_until, version, settings, unlock_spoiler_log), language, article, description, false).await;
    }

    async fn roll_alttprde9_seed(&self, ctx: &RaceContext<GlobalState>, cal_event: cal::Event, language: Language, article: &'static str) {
        let official_start = cal_event.start().expect("handling room for official race without start time");
        let delay_until = official_start - TimeDelta::minutes(10);

        // Get event to access round_modes for swiss events
        let mut transaction = ctx.global_state.db_pool.begin().await.expect("failed to start transaction");
        let event = event::Data::new(&mut transaction, cal_event.race.series, &*cal_event.race.event).await.expect("failed to load event").expect("event not found");
        transaction.commit().await.expect("failed to commit transaction");

        let alttprde_options = AlttprDeRaceOptions::for_race(&ctx.global_state.db_pool, &cal_event.race, event.round_modes.as_ref()).await;
        let seed_options_str = alttprde_options.as_seed_options_str();
        let race_options_str = alttprde_options.as_race_options_str();
        let uuid = Uuid::new_v4();
        let receiver = async {
            let url = alttprde_options.seed_url()
                .ok_or_else(|| RollError::AlttprDe("Mode not yet drafted - cannot roll seed".to_owned()))?;
            let yaml = ctx.global_state.http_client.get(&url).send().await?.text().await?;
            let yaml = inject_alttpr_dr_meta(&yaml, uuid)?;
            Ok::<_, RollError>(ctx.global_state.clone().roll_alttpr_dr_seed(yaml, uuid, "../ALttPDoorRandomizer", true))
        }.await;
        let receiver = match receiver {
            Ok(rx) => rx,
            Err(e) => alttpr_dr_error_receiver(e),
        };
        self.roll_seed_inner(ctx, Some(delay_until), receiver, language, article, format!("seed with {}", seed_options_str), false).await;
        ctx.send_message(format!("@entrants Remember: this race will be played with {}!",
                                    race_options_str
                                ), true, Vec::default()).await.expect("failed to send race options");
    }

    async fn roll_rivals_cup_seed(&self, ctx: &RaceContext<GlobalState>, cal_event: cal::Event, preset: String, language: Language, article: &'static str) {
        let official_start = cal_event.start().expect("handling room for official race without start time");
        let delay_until = official_start - TimeDelta::minutes(10);
        let preset_display = self.official_data.as_ref()
            .and_then(|OfficialRaceData { event, .. }| event.draft_kind())
            .and_then(|kind| match kind {
                draft::Kind::PickOnly { options, .. } | draft::Kind::BanPick { options, .. } | draft::Kind::BanOnly { options, .. } => {
                    options.into_iter().find(|p| p.preset == preset).map(|p| p.display_name)
                }
                _ => None,
            })
            .unwrap_or_else(|| preset.clone());
        self.roll_seed_inner(ctx, Some(delay_until), ctx.global_state.clone().roll_avianart_seed(preset), language, article, format!("{preset_display} seed"), false).await;
    }

    async fn roll_crosskeys2025_seed(&self, ctx: &RaceContext<GlobalState>, cal_event: cal::Event, language: Language, article: &'static str) {
        let official_start = cal_event.start().expect("handling room for official race without start time");
        let delay_until = official_start - TimeDelta::minutes(10);

        let crosskeys_options = CrosskeysRaceOptions::for_race(&ctx.global_state.db_pool, &cal_event.race).await;
        let seed_options_str = crosskeys_options.as_seed_options_str();
        let race_options_str = crosskeys_options.as_race_options_str();
        let uuid = Uuid::new_v4();
        let receiver = match build_crosskeys_yaml(&crosskeys_options, uuid) {
            Ok(yaml_content) => ctx.global_state.clone().roll_alttpr_dr_seed(yaml_content, uuid, "../alttpr", false),
            Err(e) => alttpr_dr_error_receiver(e.into()),
        };
        self.roll_seed_inner(ctx, Some(delay_until), receiver, language, article, format!("seed with {}", seed_options_str), false).await;
        ctx.send_message(format!("@entrants Remember: this race will be played with {}!",
                                    race_options_str
                                ), true, Vec::default()).await.expect("failed to send race options");
    }

    async fn roll_mysteryd20_seed(&self, ctx: &RaceContext<GlobalState>, cal_event: cal::Event, language: Language, article: &'static str) {
        let official_start = cal_event.start().expect("handling room for official race without start time");
        let delay_until = official_start - TimeDelta::minutes(10);

        self.roll_seed_inner(ctx, Some(delay_until), ctx.global_state.clone().roll_mysteryd20_seed(), language, article, "Mystery seed".to_string(), false).await;
    }

    async fn roll_twwr_seed(&self, ctx: &RaceContext<GlobalState>, permalink: String, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().record_twwr_permalink(permalink, unlock_spoiler_log), language, article, description, false).await;
    }

    async fn roll_twwr_seed_official(&self, ctx: &RaceContext<GlobalState>, cal_event: cal::Event, language: Language, article: &'static str) {
        let official_start = cal_event.start().expect("handling room for official race without start time");
        let delay_until = official_start - TimeDelta::minutes(15);
        let settings_string = self.official_data.as_ref().and_then(|data| data.event.settings_string.clone()).expect("TWWR event missing settings string");
        let version = self.effective_rando_version();
        self.roll_seed_inner(ctx, Some(delay_until), ctx.global_state.clone().roll_twwr_seed(Some(version), settings_string, UnlockSpoilerLog::Never), language, article, "seed".to_string(), false).await;
    }

    async fn roll_rsl_seed(&self, ctx: &RaceContext<GlobalState>, preset: rsl::VersionedPreset, world_count: u8, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_rsl_seed(delay_until, preset, world_count, unlock_spoiler_log), language, article, description, false).await;
    }

    async fn roll_tfb_seed(&self, ctx: &RaceContext<GlobalState>, version: &'static str, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        // Triforce Blitz website's auto unlock doesn't know about async parts so has to be disabled for asyncs
        let unlock_spoiler_log = if unlock_spoiler_log == UnlockSpoilerLog::After && self.official_data.as_ref().is_some_and(|official_data| official_data.cal_event.is_private_async_part()) { UnlockSpoilerLog::Never } else { unlock_spoiler_log };
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_tfb_seed(delay_until, version, Some(format!("https://{}{}", racetime_host(), ctx.data().await.url)), unlock_spoiler_log), language, article, description, false).await;
    }

    async fn roll_tfb_dev_seed(&self, ctx: &RaceContext<GlobalState>, coop: bool, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        // Triforce Blitz website's auto unlock doesn't know about async parts so has to be disabled for asyncs
        let unlock_spoiler_log = if unlock_spoiler_log == UnlockSpoilerLog::After && self.official_data.as_ref().is_some_and(|official_data| official_data.cal_event.is_private_async_part()) { UnlockSpoilerLog::Never } else { unlock_spoiler_log };
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_tfb_dev_seed(delay_until, coop, Some(format!("https://{}{}", racetime_host(), ctx.data().await.url)), unlock_spoiler_log), language, article, description, false).await;
    }

    async fn queue_existing_seed(&self, ctx: &RaceContext<GlobalState>, seed: seed::Data, language: Language, article: &'static str, description: String, suppress_preamble: bool) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        // version is only used to announce the TWWR randomizer build; for other seed types it's not needed
        let version = match seed.files() {
            Some(seed::Files::TwwrPermalink { .. }) => Some(self.effective_rando_version()),
            _ => None,
        };
        let unlock_spoiler_log = self.effective_unlock_spoiler_log(false);
        let (tx, rx) = mpsc::channel(1);
        tx.send(SeedRollUpdate::Done { rsl_preset: None, version, unlock_spoiler_log, seed }).await.unwrap();
        self.roll_seed_inner(ctx, delay_until, rx, language, article, description, suppress_preamble).await;
    }

    /// Returns `false` if this race was already finished/cancelled.
    async fn unlock_spoiler_log(&self, ctx: &RaceContext<GlobalState>) -> Result<bool, Error> {
        lock!(@write state = self.race_state; {
            match *state {
                RaceState::Rolled(ref seed) if seed.seed_data.is_some() => if self.official_data.as_ref().is_none_or(|official_data| !official_data.cal_event.is_private_async_part()) {
                    if let UnlockSpoilerLog::Progression | UnlockSpoilerLog::After = self.effective_unlock_spoiler_log(false /* we may try to unlock a log that's already unlocked, but other than that, this assumption doesn't break anything */) {
                        match seed.files() {
                            Some(seed::Files::AlttprDoorRando { .. }) => unreachable!(),
                            Some(seed::Files::MidosHouse { file_stem, locked_spoiler_log_path }) => if let Some(locked_spoiler_log_path) = locked_spoiler_log_path {
                                lock!(@write seed_metadata = ctx.global_state.seed_metadata; seed_metadata.remove(&*file_stem));
                                fs::rename(&locked_spoiler_log_path, Path::new(seed::DIR).join(format!("{file_stem}_Spoiler.json"))).await.to_racetime()?;
                            },
                            Some(seed::Files::OotrWeb { id, file_stem, .. }) => {
                                ctx.global_state.ootr_api_client.unlock_spoiler_log(id).await.to_racetime()?;
                                let spoiler_log = ctx.global_state.ootr_api_client.seed_details(id).await.to_racetime()?.spoiler_log;
                                fs::write(Path::new(seed::DIR).join(format!("{file_stem}_Spoiler.json")), &spoiler_log).await.to_racetime()?;
                            }
                            Some(seed::Files::TriforceBlitz { .. }) | Some(seed::Files::TfbSotd { .. }) => {} // automatically unlocked by triforceblitz.com
                            Some(seed::Files::TwwrPermalink { .. }) => {} // already handled by triforceblitz.com
                            Some(seed::Files::AvianartSeed { .. }) => {}
                            None => {}
                        }
                    }
                },
                RaceState::SpoilerSent => {
                    unlock!();
                    return Ok(false)
                }
                _ => {}
            }
            *state = RaceState::SpoilerSent;
        });
        Ok(true)
    }
}

#[async_trait]
impl RaceHandler<GlobalState> for Handler {
    async fn should_handle(race_data: &RaceData, global_state: Arc<GlobalState>) -> Result<bool, Error> {
        Ok(Self::should_handle_inner(race_data, global_state, None).await)
    }

    async fn should_stop(&mut self, ctx: &RaceContext<GlobalState>) -> Result<bool, Error> {
        Ok(!Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(Some(self))).await)
    }

    async fn task(global_state: Arc<GlobalState>, race_data: Arc<tokio::sync::RwLock<RaceData>>, join_handle: tokio::task::JoinHandle<()>) -> Result<(), Error> {
        let race_data = ArcRwLock::from(race_data);
        tokio::spawn(async move {
            lock!(@read data = race_data; println!("race handler for https://{}{} started", racetime_host(), data.url));
            let res = join_handle.await;
            lock!(@read data = race_data; {
                lock!(clean_shutdown = global_state.clean_shutdown; {
                    let room = OpenRoom::RaceTime {
                        room_url: data.url.clone(),
                        public: !data.unlisted,
                    };
                    assert!(clean_shutdown.open_rooms.remove(&room));
                    clean_shutdown.updates.send(CleanShutdownUpdate::RoomClosed(room)).allow_unreceived();
                    if clean_shutdown.open_rooms.is_empty() {
                        clean_shutdown.updates.send(CleanShutdownUpdate::Empty).allow_unreceived();
                    }
                });
                if let Ok(()) = res {
                    println!("race handler for https://{}{} stopped", racetime_host(), data.url);
                } else {
                    eprintln!("race handler for https://{}{} panicked", racetime_host(), data.url);
                    if let Environment::Production = Environment::default() {
                        log::error!("race handler for https://{}{} panicked", racetime_host(), data.url);
                    }
                }
            });
        });
        Ok(())
    }

    async fn new(ctx: &RaceContext<GlobalState>) -> Result<Self, Error> {
        let data = ctx.data().await;
        let (existing_seed, official_data, race_state, high_seed_name, low_seed_name, fpa_enabled) = lock!(new_room_lock = ctx.global_state.new_room_lock; { // make sure a new room isn't handled before it's added to the database
            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
            let new_data = if let Some(cal_event) = cal::Event::from_room(&mut transaction, &ctx.global_state.http_client, format!("https://{}{}", racetime_host(), ctx.data().await.url).parse()?).await.to_racetime()? {
                let event = cal_event.race.event(&mut transaction).await.to_racetime()?;
                let mut entrants = Vec::default();
                for member in cal_event.racetime_users_to_invite(&mut transaction, &*ctx.global_state.discord_ctx.read().await, &event).await.to_racetime()? {
                    match member {
                        Ok(member) => {
                            if let Some(entrant) = data.entrants.iter().find(|entrant| entrant.user.as_ref().is_some_and(|user| user.id == member)) {
                                match entrant.status.value {
                                    EntrantStatusValue::Requested => ctx.accept_request(&member).await?,
                                    EntrantStatusValue::Invited |
                                    EntrantStatusValue::Declined |
                                    EntrantStatusValue::Ready |
                                    EntrantStatusValue::NotReady |
                                    EntrantStatusValue::InProgress |
                                    EntrantStatusValue::Done |
                                    EntrantStatusValue::Dnf |
                                    EntrantStatusValue::Dq => {}
                                }
                            } else {
                                ctx.invite_user(&member).await?;
                            }
                            entrants.push(member);
                        }
                        Err(msg) => ctx.say(msg).await?,
                    }
                }
                ctx.send_message(&if_chain! {
                    if let French = event.language;
                    if !event.is_single_race();
                    if let (Some(phase), Some(round)) = (cal_event.race.phase.as_ref(), cal_event.race.round.as_ref());
                    if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut *transaction).await.to_racetime()?;
                    then {
                        format!(
                            "Bienvenue pour cette race de {phase_round} ! Pour plus d'informations : {}",
                            uri!(base_uri(), event::info(event.series, &*event.event)),
                        )
                    } else {
                        if let (true, Some(weekly_name)) = (cal_event.race.phase.is_none(), cal_event.race.round.as_deref().and_then(|round| round.strip_suffix(" Weekly"))) {
                            let settings_desc = if let Ok(Some(schedule)) = WeeklySchedule::for_round(&mut transaction, event.series, &event.event, weekly_name).await {
                                schedule.settings_description.unwrap_or_else(|| "standard".to_string())
                            } else {
                                "standard".to_string()
                            };
                            format!(
                                "Welcome to the {weekly_name} weekly! Current settings: {}. See {} for details.",
                                settings_desc,
                                uri!(base_uri(), event::info(event.series, &*event.event)),
                            )
                        } else {
                            format!(
                                "Welcome to {}! Learn more about the event at {}",
                                if event.is_single_race() {
                                    format!("the {}", event.display_name) //TODO remove “the” depending on event name
                                } else {
                                    match (cal_event.race.phase.as_deref(), cal_event.race.round.as_deref()) {
                                        (Some("Qualifier"), Some(round)) => format!("qualifier {round}"),
                                        (Some("Live Qualifier"), Some(round)) => format!("live qualifier {round}"),
                                        (Some(phase), Some(round)) => format!("this {phase} {round} race"),
                                        (Some(phase), None) => format!("this {phase} race"),
                                        (None, Some(round)) => format!("this {round} race"),
                                        (None, None) => format!("this {} race", event.display_name),
                                    }
                                },
                                uri!(base_uri(), event::info(event.series, &*event.event)),
                            )
                        }
                    }
                }, !matches!(event.seed_gen_type, Some(seed_gen_type::SeedGenType::AlttprDoorRando { .. } | seed_gen_type::SeedGenType::AlttprAvianart)), Vec::default()).await?;
                // Announce mode for events with round_modes set
                if matches!(event.seed_gen_type, Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::Boothisman })) {
                    if event.round_modes.is_some() {
                        let alttprde_options = AlttprDeRaceOptions::for_race(&ctx.global_state.db_pool, &cal_event.race, event.round_modes.as_ref()).await;
                        if let Some(mode_display) = alttprde_options.mode_display() {
                            ctx.say(format!("This race will be played in {} mode.", mode_display)).await?;
                        }
                    }
                }
                let (race_state, high_seed_name, low_seed_name) = if let Some(draft_kind) = event.draft_kind() {
                    if let Some(state) = cal_event.race.draft.clone() {
                        let [high_seed_name, low_seed_name] = if let draft::StepKind::Done(_) | draft::StepKind::DoneRsl { .. } = state.next_step(&draft_kind, cal_event.race.game, &mut draft::MessageContext::None).await.to_racetime()?.kind {
                            // we just need to roll the seed so player/team names are no longer required
                            [format!("Team A"), format!("Team B")]
                        } else {
                            match cal_event.race.entrants {
                                Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => [format!("Team A"), format!("Team B")],
                                Entrants::Two([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2)]) => {
                                    let name1 = if_chain! {
                                        if let Ok(member) = team1.members(&mut transaction).await.to_racetime()?.into_iter().exactly_one();
                                        if let Some(ref racetime) = member.racetime;
                                        then {
                                            racetime.display_name.clone()
                                        } else {
                                            team1.name(&mut transaction).await.to_racetime()?.map_or_else(|| format!("Team A"), Cow::into_owned)
                                        }
                                    };
                                    let name2 = if_chain! {
                                        if let Ok(member) = team2.members(&mut transaction).await.to_racetime()?.into_iter().exactly_one();
                                        if let Some(ref racetime) = member.racetime;
                                        then {
                                            racetime.display_name.clone()
                                        } else {
                                            team2.name(&mut transaction).await.to_racetime()?.map_or_else(|| format!("Team B"), Cow::into_owned)
                                        }
                                    };
                                    if team1.id == state.high_seed {
                                        [name1, name2]
                                    } else {
                                        [name2, name1]
                                    }
                                }
                                Entrants::Two([_, _]) => unimplemented!("draft with non-MH teams"),
                                Entrants::Three([_, _, _]) => unimplemented!("draft with 3 teams"),
                            }
                        };
                        (RaceState::Draft {
                            unlock_spoiler_log: match event.spoiler_unlock.as_str() {
                                "after" => UnlockSpoilerLog::After,
                                "immediately" => UnlockSpoilerLog::Now,
                                _ => UnlockSpoilerLog::Never,
                            },
                            state,
                        }, high_seed_name, low_seed_name)
                    } else {
                        let notif = format!(
                            "Race room https://{}{} ({}/{}) opened with no draft state. Fix it in the DB and use !reroll in the room to retry.",
                            racetime_host(), data.url, event.series.slug(), event.event,
                        );
                        {
                            let discord_ctx = ctx.global_state.discord_ctx.read().await;
                            if let Ok(dm) = ADMIN_USER.create_dm_channel(&*discord_ctx).await {
                                let _ = dm.say(&*discord_ctx, &notif).await;
                            }
                        }
                        ctx.say("Error: no draft state found for this race. A global admin has been notified. Use !reroll once the issue has been fixed.").await?;
                        (RaceState::Init, format!("Team A"), format!("Team B"))
                    }
                } else {
                    (RaceState::Init, format!("Team A"), format!("Team B"))
                };
                let restreams = cal_event.race.video_urls.iter().map(|(&language, video_url)| (video_url.clone(), RestreamState {
                    language: Some(language),
                    restreamer_racetime_id: cal_event.race.restreamers.get(&language).cloned(),
                    ready: false,
                })).collect();
                if !cal_event.race.video_urls.is_empty() {
                    ctx.say("@entrants This race is being restreamed. Please ensure at least one participant has clean audio (no desktop alerts, no gameplay overlays) on stream.").await?;
                }
                let stream_delay = cal_event.race.stream_delay(&event);
                if !stream_delay.is_zero() || event.emulator_settings_reminder || event.prevent_late_joins {
                    let delay_until = cal_event.start().expect("handling room for official race without start time") - stream_delay - TimeDelta::minutes(5);
                    if let Ok(delay) = (delay_until - Utc::now()).to_std() {
                        let ctx = ctx.clone();
                        tokio::spawn(async move {
                            sleep_until(Instant::now() + delay).await;
                            if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { return }
                            if !stream_delay.is_zero() {
                                ctx.say(format!("@entrants Remember to go live with a delay of {} ({} seconds)!",
                                    English.format_duration(stream_delay, true),
                                    stream_delay.as_secs(),
                                )).await.expect("failed to send stream delay notice");
                            }
                            if event.emulator_settings_reminder || event.prevent_late_joins {
                                sleep(stream_delay).await;
                                let data = ctx.data().await;
                                if !Self::should_handle_inner(&*data, ctx.global_state.clone(), Some(None)).await { return }
                                if event.prevent_late_joins && data.status.value == RaceStatusValue::Open {
                                    ctx.set_invitational().await.expect("failed to make the room invitational");
                                }
                                if event.emulator_settings_reminder {
                                    ctx.say("@entrants Remember to show your emulator settings!").await.expect("failed to send emulator settings notice");
                                }
                            }
                        });
                    }
                }
                let fpa_enabled = if !event.fpa_enabled {
                    false
                } else {
                    match data.status.value {
                        RaceStatusValue::Invitational => {
                            ctx.say(if let French = event.language {
                                "Le FPA est activé pour cette race. Les joueurs pourront utiliser !fpa pendant la race pour signaler d'un problème technique de leur côté. Les race monitors doivent activer les notifications en cliquant sur l'icône de cloche 🔔 sous le chat."
                            } else {
                                "Fair play agreement is active for this official race. Entrants may use the !fpa command during the race to notify of a crash. Race monitors (if any) should enable notifications using the bell 🔔 icon below chat."
                            }).await?; //TODO different message for monitorless FPA?
                            true
                        }
                        RaceStatusValue::Open => false,
                        _ => data.entrants.len() < 10, // guess based on entrant count, assuming an open race for 10 or more
                    }
                };
                (
                    cal_event.race.seed.clone(),
                    Some(OfficialRaceData {
                        fpa_invoked: cal_event.race.fpa_invoked,
                        breaks_used: cal_event.race.breaks_used,
                        cal_event, event, restreams, entrants,
                    }),
                    race_state,
                    high_seed_name,
                    low_seed_name,
                    fpa_enabled,
                )
            } else {
                let mut race_state = RaceState::Init;
                if let Some(ref info_bot) = data.info_bot {
                    for section in info_bot.split(" | ") {
                        if let Some((_, file_stem)) = regex_captures!(r"^Seed: https://midos\.house/seed/(.+)(?:\.zpfz?)?$", section) {
                            race_state = RaceState::Rolled(seed::Data {
                                file_hash: None,
                                password: None,
                                seed_data: Some(seed::Files::MidosHouse {
                                    file_stem: Cow::Owned(file_stem.to_owned()),
                                    locked_spoiler_log_path: None,
                                }.to_seed_data_base()),
                                progression_spoiler: false, //TODO
                            });
                            break
                        } else if let Some((_, seed_id)) = regex_captures!(r"^Seed: https://ootrandomizer\.com/seed/get?id=([0-9]+)$", section) {
                            let id = seed_id.parse().to_racetime()?;
                            race_state = RaceState::Rolled(seed::Data {
                                file_hash: None,
                                password: None, //TODO get from API
                                seed_data: Some(seed::Files::OotrWeb {
                                    gen_time: Utc::now(),
                                    file_stem: Cow::Owned(ctx.global_state.ootr_api_client.patch_file_stem(id).await.to_racetime()?),
                                    id,
                                }.to_seed_data_base()),
                                progression_spoiler: false, //TODO
                            });
                            break
                        }
                    }
                }
                if let RaceStatusValue::Pending | RaceStatusValue::InProgress = data.status.value { //TODO also check this in official races
                    if_chain! {
                        if let Ok(log) = ctx.global_state.http_client.get(format!("https://{}{}/log", racetime_host(), data.url)).send().await;
                        if let Ok(log) = log.detailed_error_for_status().await;
                        if let Ok(log) = log.text().await; //TODO stream response
                        if !log.to_ascii_lowercase().contains("break"); //TODO parse chatlog and recover breaks config instead of sending this
                        then {
                            // no breaks configured, can safely restart
                        } else {
                            ctx.say("@entrants I just restarted and it looks like the race is already in progress. If the !breaks command was used, break notifications may be broken now. Sorry about that.").await?;
                        }
                    }
                } else if let RaceState::Rolled(_) = race_state {
                    ctx.say("@entrants I just restarted. You may have to reconfigure !breaks and !fpa. Sorry about that.").await?;
                }
                (
                    seed::Data::default(),
                    None,
                    RaceState::default(),
                    format!("Team A"),
                    format!("Team B"),
                    false,
                )
            };
            transaction.commit().await.to_racetime()?;
            new_data
        });
        let this = Self {
            breaks: None, //TODO default breaks for restreamed matches?
            break_notifications: None,
            locked: false,
            password_sent: false,
            race_state: ArcRwLock::new(race_state),
            cleaned_up: Arc::default(),
            finish_timeout: None,
            official_data, high_seed_name, low_seed_name, fpa_enabled,
        };
        // Defer restreamer setup to background task to allow handler to start immediately
        if let Some(OfficialRaceData { ref event, ref restreams, ref cal_event, .. }) = this.official_data {
            if !restreams.is_empty() {
                let ctx_clone = ctx.clone();
                let restreams_clone = restreams.clone();
                let lang_clone = event.language;
                tokio::spawn(async move {
                    // Small delay to allow handler to fully initialize
                    sleep(Duration::from_millis(100)).await;
                    if !Self::should_handle_inner(&*ctx_clone.data().await, ctx_clone.global_state.clone(), Some(None)).await { return }

                    let restreams_text = restreams_clone.iter().map(|(video_url, state)| format!("in {} at {video_url}", state.language.expect("preset restreams should have languages assigned"))).join(" and "); // don't use English.join_str since racetime.gg parses the comma as part of the URL
                    for restreamer in restreams_clone.values().flat_map(|RestreamState { restreamer_racetime_id, .. }| restreamer_racetime_id) {
                        let data = ctx_clone.data().await;
                        if data.monitors.iter().find(|monitor| monitor.id == *restreamer).is_some() { continue }
                        if let Some(entrant) = data.entrants.iter().find(|entrant| entrant.user.as_ref().is_some_and(|user| user.id == *restreamer)) { //TODO keep track of pending changes to the entrant list made in this method and match accordingly, e.g. players who are also monitoring should not be uninvited
                            match entrant.status.value {
                                EntrantStatusValue::Requested => {
                                    let _ = ctx_clone.accept_request(restreamer).await;
                                    let _ = ctx_clone.add_monitor(restreamer).await;
                                    let _ = ctx_clone.remove_entrant(restreamer).await;
                                }
                                EntrantStatusValue::Invited |
                                EntrantStatusValue::Declined |
                                EntrantStatusValue::Ready |
                                EntrantStatusValue::NotReady |
                                EntrantStatusValue::InProgress |
                                EntrantStatusValue::Done |
                                EntrantStatusValue::Dnf |
                                EntrantStatusValue::Dq => {
                                    let _ = ctx_clone.add_monitor(restreamer).await;
                                }
                            }
                        } else {
                            let _ = ctx_clone.invite_user(restreamer).await;
                            let _ = ctx_clone.add_monitor(restreamer).await;
                            let _ = ctx_clone.remove_entrant(restreamer).await;
                        }
                    }
                    let text = if restreams_clone.values().any(|state| state.restreamer_racetime_id.is_none()) {
                        if_chain! {
                            if let French = lang_clone;
                            if let Ok((video_url, state)) = restreams_clone.iter().exactly_one();
                            if let Some(French) = state.language;
                            then {
                                format!("Cette race est restreamée en français chez {video_url} — l'auto-start est désactivé. Les organisateurs du tournoi peuvent utiliser '!monitor' pour devenir race monitor, puis pour inviter les restreamers en tant que race monitor et leur autoriser le force start.")
                            } else {
                                format!("This race is being restreamed {restreams_text} — auto-start is disabled. Tournament organizers can use '!monitor' to become race monitors, then invite the restreamer{0} as race monitor{0} to allow them to force-start.", if restreams_clone.len() == 1 { "" } else { "s" })
                            }
                        }
                    } else if let Ok((video_url, state)) = restreams_clone.iter().exactly_one() {
                        if_chain! {
                            if let French = lang_clone;
                            if let Some(French) = state.language;
                            then {
                                format!("Cette race est restreamée en français chez {video_url} — l'auto start est désactivé. Le restreamer peut utiliser '!ready' pour débloquer l'auto-start.")
                            } else {
                                format!("This race is being restreamed {restreams_text} — auto-start is disabled. The restreamer can use '!ready' to unlock auto-start.")
                            }
                        }
                    } else {
                        format!("This race is being restreamed {restreams_text} — auto-start is disabled. Restreamers can use '!ready' once the restream is ready. Auto-start will be unlocked once all restreams are ready.")
                    };
                    let _ = ctx_clone.send_message(&text, true, Vec::default()).await;
                });
            }
            lock!(@read state = this.race_state; {
                if existing_seed.files().is_some() {
                    this.queue_existing_seed(ctx, existing_seed, English, "a", format!("seed"), true).await; //TODO better article/description
                } else if event.seed_gen_type.is_some() || event.single_settings.is_some() || event.draft_kind().is_some() {
                    // Only roll seeds for events that have seed configuration
                    let _event_id = Some((event.series, &*event.event));
                    match *state {
                        RaceState::Init => match &event.seed_gen_type {
                            Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::Boothisman }) => {
                                if event.draft_kind().is_none() {
                                    this.roll_alttprde9_seed(ctx, cal_event.clone(), English, "a").await
                                }
                                // else: ban-pick draft event with missing draft state — error already
                                // reported at room open via the draft_kind check; do not roll
                            }
                            Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::MutualChoices }) => {
                                this.roll_crosskeys2025_seed(ctx, cal_event.clone(), English, "a").await
                            }
                            Some(seed_gen_type::SeedGenType::AlttprDoorRando { source: seed_gen_type::AlttprDrSource::MysteryPool { .. } }) => {
                                this.roll_mysteryd20_seed(ctx, cal_event.clone(), English, "a").await
                            }
                            Some(seed_gen_type::SeedGenType::AlttprAvianart) => {
                                ctx.say("@entrants WARNING: The preset draft for this match is not complete! Please complete the draft in the scheduling Discord thread before the race.").await.to_racetime()?;
                            }
                            Some(seed_gen_type::SeedGenType::TWWR { .. }) => {
                                this.roll_twwr_seed_official(ctx, cal_event.clone(), English, "a").await
                            }
                            Some(seed_gen_type::SeedGenType::OotrTriforceBlitz) => {
                                this.roll_tfb_dev_seed(ctx, true, this.effective_unlock_spoiler_log(false), English, "a", format!("Triforce Blitz S4 co-op seed")).await
                            }
                            Some(_) | None => {
                                if let Some(ref settings) = event.single_settings {
                                    let event_lang = event.language;
                                    let (article, desc) = if let French = event_lang { ("une", format!("seed")) } else { ("a", format!("seed")) };
                                    this.roll_seed(ctx, this.effective_preroll_mode(), this.effective_rando_version(), settings.clone(), this.effective_unlock_spoiler_log(false), event_lang, article, desc).await;
                                }
                            }
                        },
                        RaceState::Draft { state: ref draft_state, .. } => {
                            this.advance_draft(ctx, &state).await?;
                            // Warn if draft is incomplete
                            if let Some(draft_kind) = this.official_data.as_ref().and_then(|OfficialRaceData { event, .. }| event.draft_kind()) {
                                let step = draft_state.next_step(&draft_kind, cal_event.race.game, &mut draft::MessageContext::None).await.to_racetime()?;
                                if !matches!(step.kind, draft::StepKind::Done(_)) {
                                    ctx.say("@entrants WARNING: The mode draft for this match is not complete! Please complete the draft as soon as possible. The seed cannot be rolled until the draft is finished.").await.to_racetime()?;
                                }
                            }
                        },
                        RaceState::Rolling | RaceState::Rolled(_) | RaceState::SpoilerSent => {}
                    }
                }
            });
        }
        Ok(this)
    }

    async fn command(&mut self, ctx: &RaceContext<GlobalState>, cmd_name: String, args: Vec<String>, _is_moderator: bool, is_monitor: bool, msg: &ChatMessage) -> Result<(), Error> {
        let sgt = self.official_data.as_ref().and_then(|d| d.event.seed_gen_type.clone());
        let lang = self.language();
        let reply_to = msg.user.as_ref().map_or("friend", |user| &user.name);
        match &*cmd_name.to_ascii_lowercase() {
            cmd @ ("ban" | "block" | "draft" | "first" | "no" | "pick" | "second" | "skip" | "yes") => {
                let is_sgt_rsl = matches!(sgt.as_ref(), Some(seed_gen_type::SeedGenType::OotrRsl));
                if is_sgt_rsl {
                // RSL event via seed_gen_type (no matching Goal) — RSL draft conventions: block=ban, ban=pick
                match (cmd, &args[..]) {
                    ("block", []) => { self.send_settings(ctx, &format!("Sorry {reply_to}, the setting is required. Use one of the following:"), reply_to).await?; }
                    ("block", [setting]) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::Ban { setting: setting.clone() }).await?,
                    ("block", _) => ctx.say(format!("Sorry {reply_to}, only one setting can be banned at a time. Use \"!block <setting>\"")).await?,
                    ("ban", []) => { self.send_settings(ctx, &format!("Sorry {reply_to}, the setting is required. Use one of the following:"), reply_to).await?; }
                    ("ban", [_]) => ctx.say(format!("Sorry {reply_to}, the value is required.")).await?,
                    ("ban", [setting, value]) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::Pick { setting: setting.clone(), value: value.clone() }).await?,
                    ("ban", _) => ctx.say(format!("Sorry {reply_to}, only one setting can be drafted at a time. Use \"!ban <setting> <value>\"")).await?,
                    ("first", _) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::GoFirst(true)).await?,
                    ("second", _) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::GoFirst(false)).await?,
                    ("yes", _) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::BooleanChoice(true)).await?,
                    ("no", _) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::BooleanChoice(false)).await?,
                    ("skip", _) => self.draft_action(ctx, msg.user.as_ref(), draft::Action::Skip).await?,
                    (cmd, _) => ctx.say(format!("Sorry {reply_to}, unexpected draft command: {cmd}")).await?,
                }
            } else if self.official_data.as_ref().and_then(|d| d.event.draft_kind()).is_some() {
                // Generic event with a draft kind — parse commands directly (non-RSL convention)
                let action = match (cmd, &args[..]) {
                    ("ban" | "block", []) => {
                        self.send_settings(ctx, &format!("Sorry {reply_to}, the setting is required. Use one of the following:"), reply_to).await?;
                        return Ok(());
                    }
                    ("ban" | "block", [setting]) => draft::Action::Ban { setting: setting.clone() },
                    ("ban" | "block", _) => {
                        ctx.say(format!("Sorry {reply_to}, only one setting can be banned at a time. Use \"!ban <setting>\"")).await?;
                        return Ok(());
                    }
                    ("draft" | "pick", []) => {
                        self.send_settings(ctx, &format!("Sorry {reply_to}, the setting is required. Use one of the following:"), reply_to).await?;
                        return Ok(());
                    }
                    ("draft" | "pick", [_]) => {
                        ctx.say(format!("Sorry {reply_to}, the value is required.")).await?;
                        return Ok(());
                    }
                    ("draft" | "pick", [setting, value]) => draft::Action::Pick { setting: setting.clone(), value: value.clone() },
                    ("draft" | "pick", _) => {
                        ctx.say(format!("Sorry {reply_to}, only one setting can be drafted at a time. Use \"!pick <setting> <value>\"")).await?;
                        return Ok(());
                    }
                    ("first", _) => draft::Action::GoFirst(true),
                    ("second", _) => draft::Action::GoFirst(false),
                    ("yes", _) => draft::Action::BooleanChoice(true),
                    ("no", _) => draft::Action::BooleanChoice(false),
                    ("skip", _) => draft::Action::Skip,
                    _ => { ctx.say(format!("Sorry {reply_to}, unexpected draft command: {cmd}")).await?; return Ok(()); }
                };
                self.draft_action(ctx, msg.user.as_ref(), action).await?;
            } else {
                ctx.say(format!("Sorry {reply_to}, draft commands are not available for this event.")).await?;
            }
            },
            "breaks" | "break" => match args[..] {
                [] => if let Some(breaks) = self.breaks {
                    ctx.say(if let French = lang {
                        format!("Vous aurez une pause de {}. Vous pouvez les désactiver avec !breaks off.", breaks.format(French))
                    } else {
                        format!("Breaks are currently set to {}. Disable with !breaks off", breaks.format(English))
                    }).await?;
                } else {
                    ctx.say(if let French = lang {
                        "Les pauses sont actuellement désactivées. Exemple pour les activer : !breaks 5m every 2h30."
                    } else {
                        "Breaks are currently disabled. Example command to enable: !breaks 5m every 2h30"
                    }).await?;
                },
                [ref arg] if arg == "off" => if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                    self.breaks = None;
                    ctx.say(if let French = lang {
                        "Les pauses sont désormais désactivées."
                    } else {
                        "Breaks are now disabled."
                    }).await?;
                } else {
                    ctx.say(if let French = lang {
                        format!("Désolé {reply_to}, mais la race a débuté.")
                    } else {
                        format!("Sorry {reply_to}, but the race has already started.")
                    }).await?;
                },
                _ => if let Ok(breaks) = args.join(" ").parse::<Breaks>() {
                    if breaks.duration < Duration::from_secs(60) {
                        ctx.say(if let French = lang {
                            format!("Désolé {reply_to}, le temps minimum pour une pause (si active) est de 1 minute. Vous pouvez désactiver les pauses avec !breaks off")
                        } else {
                            format!("Sorry {reply_to}, minimum break time (if enabled at all) is 1 minute. You can disable breaks entirely with !breaks off")
                        }).await?;
                    } else if breaks.interval < breaks.duration + Duration::from_secs(5 * 60) {
                        ctx.say(if let French = lang {
                            format!("Désolé {reply_to}, il doit y avoir un minimum de 5 minutes entre les pauses.")
                        } else {
                            format!("Sorry {reply_to}, there must be a minimum of 5 minutes between breaks since I notify runners 5 minutes in advance.")
                        }).await?;
                    } else if breaks.duration + breaks.interval >= Duration::from_secs(24 * 60 * 60) {
                        ctx.say(if let French = lang {
                            format!("Désolé {reply_to}, vous ne pouvez pas faire de pauses si tard dans la race, vu que les race rooms se ferment au bout de 24 heures.")
                        } else {
                            format!("Sorry {reply_to}, race rooms are automatically closed after 24 hours so these breaks wouldn't work.")
                        }).await?;
                    } else {
                        self.breaks = Some(breaks);
                        ctx.say(if let French = lang {
                            format!("Vous aurez une pause de {}.", breaks.format(French))
                        } else {
                            format!("Breaks set to {}.", breaks.format(English))
                        }).await?;
                    }
                } else {
                    ctx.say(if let French = lang {
                        format!("Désolé {reply_to}, je ne reconnais pas ce format pour les pauses. Exemple pour les activer : !breaks 5m every 2h30.")
                    } else {
                        format!("Sorry {reply_to}, I don't recognize that format for breaks. Example commands: !breaks 5m every 2h30, !breaks off")
                    }).await?;
                },
            },
            "fpa" => match args[..] {
                [] => if self.fpa_enabled {
                    if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                        ctx.say(if let French = lang {
                            "Le FPA ne peut pas être appelé avant que la race ne commence."
                        } else {
                            "FPA cannot be invoked before the race starts."
                        }).await?;
                    } else {
                        if let Some(OfficialRaceData { ref cal_event, ref restreams, ref mut fpa_invoked, ref event, .. }) = self.official_data {
                            *fpa_invoked = true;
                            if restreams.is_empty() {
                                ctx.say(if_chain! {
                                    if let French = lang;
                                    if let TeamConfig::Solo = event.team_config;
                                    then {
                                        format!(
                                            "@everyone Le FPA a été appelé par {reply_to}.{} La race sera re-timée après le fin de celle-ci.",
                                            if let RaceSchedule::Async { .. } = cal_event.race.schedule { "" } else { " Le joueur qui ne l'a pas demandé peut continuer à jouer." },
                                        )
                                    } else {
                                        format!(
                                            "@everyone FPA has been invoked by {reply_to}. T{}he race will be retimed once completed.",
                                            if let RaceSchedule::Async { .. } = cal_event.race.schedule {
                                                String::default()
                                            } else {
                                                format!(
                                                    "he {player_team} that did not call FPA can continue playing; t",
                                                    player_team = if let TeamConfig::Solo = event.team_config { "player" } else { "team" },
                                                )
                                            },
                                        )
                                    }
                                }).await?;
                            } else {
                                ctx.say(if let French = lang {
                                    format!("@everyone Le FPA a été appelé par {reply_to}. Merci d'arrêter de jouer, la race étant restreamée.")
                                } else {
                                    format!("@everyone FPA has been invoked by {reply_to}. Please pause since this race is being restreamed.")
                                }).await?;
                            }
                        } else {
                            ctx.say(if let French = lang {
                                format!("@everyone Le FPA a été appelé par {reply_to}.")
                            } else {
                                format!("@everyone FPA has been invoked by {reply_to}.")
                            }).await?;
                        }
                    }
                } else {
                    ctx.say(if let French = lang {
                        "Le FPA n'est pas activé. Les Race Monitors peuvent l'activer avec !fpa on."
                    } else {
                        "Fair play agreement is not active. Race monitors may enable FPA for this race with !fpa on"
                    }).await?;
                },
                [ref arg] => match &*arg.to_ascii_lowercase() {
                    "on" => if self.is_official() {
                        ctx.say(if let French = lang {
                            "Le FPA est toujours activé dans les races officielles."
                        } else {
                            "Fair play agreement is always active in official races."
                        }).await?;
                    } else if !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                        ctx.say(if let French = lang {
                            format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                        } else {
                            format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                        }).await?;
                    } else if self.fpa_enabled {
                        ctx.say(if let French = lang {
                            "Le FPA est déjà activé."
                        } else {
                            "Fair play agreement is already activated."
                        }).await?;
                    } else {
                        self.fpa_enabled = true;
                        ctx.say(if let French = lang {
                            "Le FPA est désormais activé. Les joueurs pourront utiliser !fpa pendant la race pour signaler d'un problème technique de leur côté. Les race monitors doivent activer les notifications en cliquant sur l'icône de cloche 🔔 sous le chat."
                        } else {
                            "Fair play agreement is now active. @entrants may use the !fpa command during the race to notify of a crash. Race monitors should enable notifications using the bell 🔔 icon below chat."
                        }).await?;
                    },
                    "off" => if self.is_official() {
                        ctx.say(if let French = lang {
                            format!("Désolé {reply_to}, mais le FPA ne peut pas être désactivé pour les races officielles.")
                        } else {
                            format!("Sorry {reply_to}, but FPA can't be deactivated for official races.")
                        }).await?;
                    } else if !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                        ctx.say(if let French = lang {
                            format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                        } else {
                            format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                        }).await?;
                    } else if self.fpa_enabled {
                        self.fpa_enabled = false;
                        ctx.say(if let French = lang {
                            "Le FPA est désormais désactivé."
                        } else {
                            "Fair play agreement is now deactivated."
                        }).await?;
                    } else {
                        ctx.say(if let French = lang {
                            "Le FPA est déjà désactivé."
                        } else {
                            "Fair play agreement is not active."
                        }).await?;
                    },
                    _ => ctx.say(if let French = lang {
                        format!("Désolé {reply_to}, les seules commandes sont “!fpa on”, “!fpa off” ou “!fpa”.")
                    } else {
                        format!("Sorry {reply_to}, I don't recognize that subcommand. Use “!fpa on” or “!fpa off”, or just “!fpa” to invoke FPA.")
                    }).await?,
                },
                [..] => ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, les seules commandes sont “!fpa on”, “!fpa off” ou “!fpa”.")
                } else {
                    format!("Sorry {reply_to}, I didn't quite understand that. Use “!fpa on” or “!fpa off”, or just “!fpa” to invoke FPA.")
                }).await?,
            },
            "lock" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                self.locked = true;
                ctx.say(if_chain! {
                    if let French = lang;
                    if !self.is_official();
                    then {
                        format!("Race verrouillée. Je ne génèrerai une seed que pour les race monitors.")
                    } else {
                        format!("Lock initiated. I will now only roll seeds for {}.", if self.is_official() { "race monitors or tournament organizers" } else { "race monitors" })
                    }
                }).await?;
            } else {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                } else {
                    format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                }).await?;
            },
            "monitor" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                let monitor = &msg.user.as_ref().expect("received !monitor command from bot").id;
                if let Some(entrant) = ctx.data().await.entrants.iter().find(|entrant| entrant.user.as_ref().is_some_and(|user| user.id == *monitor)) {
                    match entrant.status.value {
                        EntrantStatusValue::Requested => {
                            ctx.accept_request(monitor).await?;
                            ctx.add_monitor(monitor).await?;
                            ctx.remove_entrant(monitor).await?;
                        }
                        EntrantStatusValue::Invited |
                        EntrantStatusValue::Declined |
                        EntrantStatusValue::Ready |
                        EntrantStatusValue::NotReady |
                        EntrantStatusValue::InProgress |
                        EntrantStatusValue::Done |
                        EntrantStatusValue::Dnf |
                        EntrantStatusValue::Dq => {
                            ctx.add_monitor(monitor).await?;
                        }
                    }
                } else {
                    ctx.invite_user(monitor).await?;
                    ctx.add_monitor(monitor).await?;
                    ctx.remove_entrant(monitor).await?;
                }
            } else if self.is_official() {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, seuls les organisateurs du tournoi peuvent faire cela.")
                } else {
                    format!("Sorry {reply_to}, only tournament organizers can do that.")
                }).await?;
            } else {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, cette commande n'est disponible que pour les races officielles.")
                } else {
                    format!("Sorry {reply_to}, this command is only available for official races.")
                }).await?;
            },
            "presets" => if let Some(ref sgt) = sgt {
                sgt.send_presets(ctx).await?;
            } else {
                ctx.say(format!("Sorry {reply_to}, presets are not available for this event.")).await?;
            },
            "ready" => if let Some(OfficialRaceData { ref mut restreams, ref cal_event, ref event, .. }) = self.official_data {
                if let Some(state) = restreams.values_mut().find(|state| state.restreamer_racetime_id.as_ref() == Some(&msg.user.as_ref().expect("received !ready command from bot").id)) {
                    state.ready = true;
                } else {
                    ctx.say(if let French = lang {
                        format!("Désolé {reply_to}, seuls les restreamers peuvent faire cela.")
                    } else {
                        format!("Sorry {reply_to}, only restreamers can do that.")
                    }).await?;
                    return Ok(())
                }
                if restreams.values().all(|state| state.ready) {
                    ctx.say(if_chain! {
                        if let French = lang;
                        if let Ok((_, state)) = restreams.iter().exactly_one();
                        if let Some(French) = state.language;
                        then {
                            "Restream prêt. Déverrouillage de l'auto-start."
                        } else {
                            "All restreams ready, unlocking auto-start…"
                        }
                    }).await?;
                    let race_url = ctx.data().await.url.clone();
                    let category_slug = race_url.trim_start_matches('/').split('/').next().unwrap_or(CATEGORY).to_owned();
                    let db_row = sqlx::query!("SELECT client_id, client_secret FROM game_racetime_connection WHERE category_slug = $1 LIMIT 1", category_slug)
                        .fetch_optional(&ctx.global_state.db_pool).await.to_racetime()?;
                    let (client_id, client_secret) = db_row.map_or_else(
                        || (ctx.global_state.racetime_config.client_id.clone(), ctx.global_state.racetime_config.client_secret.clone()),
                        |row| (row.client_id, row.client_secret),
                    );
                    let (access_token, _) = racetime::authorize_with_host(&ctx.global_state.host_info, &client_id, &client_secret, &ctx.global_state.http_client).await?;
                    let (goal_str, goal_is_custom) = if let Some(ref slug) = cal_event.race.racetime_goal_slug {
                        (slug.clone(), event.is_custom_goal)
                    } else {
                        let data = ctx.data().await;
                        (data.goal.name.clone(), event.is_custom_goal)
                    };
                    let data = ctx.data().await;
                    room_options(
                        goal_str, goal_is_custom, event, cal_event,
                        data.info_user.clone().unwrap_or_default(),
                        data.info_bot.clone().unwrap_or_default(),
                        true,
                    ).await.edit_with_host(&ctx.global_state.host_info, &access_token, &ctx.global_state.http_client, &category_slug, &data.slug).await?;
                } else {
                    ctx.say(format!("Restream ready, still waiting for other restreams.")).await?;
                }
            } else {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, cette commande n'est disponible que pour les races officielles.")
                } else {
                    format!("Sorry {reply_to}, this command is only available for official races.")
                }).await?;
            },
            "restreamer" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                if let Some(OfficialRaceData { ref mut restreams, ref cal_event, ref event, .. }) = self.official_data {
                    if let [restream_url, restreamer] = &args[..] {
                        let restream_url = if restream_url.contains('/') {
                            Url::parse(restream_url)
                        } else {
                            Url::parse(&format!("https://twitch.tv/{restream_url}"))
                        };
                        if let Ok(restream_url) = restream_url {
                            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                            match parse_user(&mut transaction, &ctx.global_state.http_client, restreamer).await {
                                Ok(restreamer_racetime_id) => {
                                    if restreams.is_empty() {
                                        let race_url = ctx.data().await.url.clone();
                                        let category_slug = race_url.trim_start_matches('/').split('/').next().unwrap_or(CATEGORY).to_owned();
                                        let db_row = sqlx::query!("SELECT client_id, client_secret FROM game_racetime_connection WHERE category_slug = $1 LIMIT 1", category_slug)
                                            .fetch_optional(&ctx.global_state.db_pool).await.to_racetime()?;
                                        let (client_id, client_secret) = db_row.map_or_else(
                                            || (ctx.global_state.racetime_config.client_id.clone(), ctx.global_state.racetime_config.client_secret.clone()),
                                            |row| (row.client_id, row.client_secret),
                                        );
                                        let (access_token, _) = racetime::authorize_with_host(&ctx.global_state.host_info, &client_id, &client_secret, &ctx.global_state.http_client).await?;
                                        let (goal_str, goal_is_custom) = if let Some(ref slug) = cal_event.race.racetime_goal_slug {
                                            (slug.clone(), event.is_custom_goal)
                                        } else {
                                            let data = ctx.data().await;
                                            (data.goal.name.clone(), event.is_custom_goal)
                                        };
                                        let data = ctx.data().await;
                                        room_options(
                                            goal_str, goal_is_custom, event, cal_event,
                                            data.info_user.clone().unwrap_or_default(),
                                            data.info_bot.clone().unwrap_or_default(),
                                            false,
                                        ).await.edit_with_host(&ctx.global_state.host_info, &access_token, &ctx.global_state.http_client, &category_slug, &data.slug).await?;
                                    }
                                    restreams.entry(restream_url).or_default().restreamer_racetime_id = Some(restreamer_racetime_id.clone());
                                    ctx.say("Restreamer assigned. Use “!ready” once the restream is ready. Auto-start will be unlocked once all restreams are ready.").await?; //TODO mention restreamer
                                }
                                Err(e) => ctx.say(format!("Sorry {reply_to}, I couldn't parse the restreamer: {e}")).await?,
                            }
                            transaction.commit().await.to_racetime()?;
                        } else {
                            ctx.say(format!("Sorry {reply_to}, that doesn't seem to be a valid URL or Twitch channel.")).await?;
                        }
                    } else {
                        ctx.say(format!("Sorry {reply_to}, I don't recognize that format for adding a restreamer.")).await?; //TODO better help message
                    }
                } else {
                    ctx.say(if let French = lang {
                        format!("Désolé {reply_to}, cette commande n'est disponible que pour les races officielles.")
                    } else {
                        format!("Sorry {reply_to}, this command is only available for official races.")
                    }).await?;
                }
            } else {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                } else {
                    format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                }).await?;
            },
            "seed" | "spoilerseed" => if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                if self.official_data.as_ref().is_some_and(|d| d.event.seed_gen_type.is_none() && d.event.single_settings.is_none() && d.event.draft_kind().is_none()) {
                    ctx.say(format!("Sorry {reply_to}, this race does not use randomizer seeds. You can start the race directly without rolling a seed.")).await?;
                } else {
                    lock!(@write state = self.race_state; match *state {
                        RaceState::Init => if self.locked && !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                            ctx.say(if let French = lang {
                                format!("Désolé {reply_to}, la race est verrouillée. Seuls {} peuvent générer une seed pour cette race.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                            } else {
                                format!("Sorry {reply_to}, seed rolling is locked. Only {} may roll a seed for this race.", if self.is_official() { "race monitors or tournament organizers" } else { "race monitors" })
                            }).await?;
                        } else if let Some(sgt) = sgt.as_ref().filter(|s| matches!(s,
                            seed_gen_type::SeedGenType::AlttprDoorRando { .. }
                            | seed_gen_type::SeedGenType::AlttprAvianart
                            | seed_gen_type::SeedGenType::OotrTriforceBlitz
                            | seed_gen_type::SeedGenType::OotrRsl
                            | seed_gen_type::SeedGenType::TWWR { .. }
                        )) {
                        let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                        match sgt.parse_seed_command(&mut transaction, &ctx.global_state, self.is_official(), cmd_name.eq_ignore_ascii_case("spoilerseed"), false, &args).await.to_racetime()? {
                            SeedCommandParseResult::Alttpr => {
                                // TODO THIS NEEDS TO BE IMPLEMENTED -- call door rando .py and roll seed with arguments
                                Command::new("echo").args(["hello", "world"]).check("echo").await.to_racetime()?;
                                unimplemented!()
                            }
                            SeedCommandParseResult::Rsl { preset, world_count, unlock_spoiler_log, language, article, description } => self.roll_rsl_seed(ctx, preset, world_count, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::Tfb { version, unlock_spoiler_log, language, article, description } => self.roll_tfb_seed(ctx, version, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::TfbDev { coop, unlock_spoiler_log, language, article, description } => self.roll_tfb_dev_seed(ctx, coop, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::Twwr { permalink, unlock_spoiler_log, language, article, description } => self.roll_twwr_seed(ctx, permalink, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::QueueExisting { data, language, article, description } => self.queue_existing_seed(ctx, data, language, article, description, false).await,
                            SeedCommandParseResult::SendPresets { language, msg } => {
                                ctx.say(if let French = language {
                                    format!("Désolé {reply_to}, {msg}. Veuillez utiliser un des suivants :")
                                } else {
                                    format!("Sorry {reply_to}, {msg}. Use one of the following:")
                                }).await?;
                                sgt.send_presets(ctx).await?;
                            }
                            SeedCommandParseResult::StartDraft { new_state, unlock_spoiler_log } => {
                                *state = RaceState::Draft {
                                    state: new_state,
                                    unlock_spoiler_log,
                                };
                                self.advance_draft(ctx, &state).await?;
                            }
                            SeedCommandParseResult::Error { language, msg } => ctx.say(if let French = language {
                                format!("Désolé {reply_to}, {msg}")
                            } else {
                                format!("Sorry {reply_to}, {msg}")
                            }).await?,
                        }
                        transaction.commit().await.to_racetime()?;
                    } else {
                        // Generic goal
                        let event_draft_kind = self.official_data.as_ref().and_then(|d| d.event.draft_kind());
                        if args.as_slice() == ["draft"] && event_draft_kind.is_some() {
                            let unlock_spoiler_log = self.effective_unlock_spoiler_log(false);
                            *state = RaceState::Draft {
                                state: Draft { high_seed: Id::dummy(), went_first: None, skipped_bans: 0, settings: HashMap::default() },
                                unlock_spoiler_log,
                            };
                            self.advance_draft(ctx, &state).await?;
                        } else if let Some(ref settings) = self.official_data.as_ref().and_then(|d| d.event.single_settings.clone()) {
                            let unlock_spoiler_log = self.effective_unlock_spoiler_log(cmd_name.eq_ignore_ascii_case("spoilerseed"));
                            let (article, desc) = if let French = lang { ("une", format!("seed")) } else { ("a", format!("seed")) };
                            self.roll_seed(ctx, self.effective_preroll_mode(), self.effective_rando_version(), settings.clone(), unlock_spoiler_log, lang, article, desc).await;
                        } else {
                            ctx.say(format!("Sorry {reply_to}, no settings are configured for this event. Please contact a tournament organizer.")).await?;
                        }
                    },
                    RaceState::Draft { .. } => ctx.say(format!("Sorry {reply_to}, settings are already being drafted.")).await?,
                    RaceState::Rolling => ctx.say(format!("Sorry {reply_to}, but I'm already rolling a seed for this room. Please wait.")).await?,
                    RaceState::Rolled(_) | RaceState::SpoilerSent => ctx.say(format!("Sorry {reply_to}, but I already rolled a seed. Check the race info!")).await?,
                    });
                }
            } else {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, mais la race a débuté.")
                } else {
                    format!("Sorry {reply_to}, but the race has already started.")
                }).await?;
            },
            "settings" => lock!(@read state = self.race_state; self.send_settings(ctx, if let RaceState::Draft { .. } = *state {
                if let French = lang {
                    "Settings pouvant être actuellement choisis :"
                } else {
                    "Currently draftable settings:"
                }
            } else {
                if let French = lang {
                    "Settings pouvant être choisis :"
                } else {
                    "Draftable settings:"
                }
            }, reply_to).await?),
            "reroll" => if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                if self.official_data.as_ref().is_some_and(|d| d.event.seed_gen_type.is_none() && d.event.single_settings.is_none() && d.event.draft_kind().is_none()) {
                    ctx.say(format!("Sorry {reply_to}, this race does not use randomizer seeds.")).await?;
                } else {
                    lock!(@write state = self.race_state; match *state {
                        RaceState::Init => {
                            // Check if user is an entrant or monitor
                            let data = ctx.data().await;
                            let is_entrant = msg.user.as_ref().is_some_and(|user|
                                data.entrants.iter().any(|e| e.user.as_ref().is_some_and(|u| u.id == user.id))
                            );

                            if !is_entrant && !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                                ctx.say(format!("Sorry {reply_to}, only @entrants or race monitors may use this command.")).await?;
                            } else {
                            let settings = self.official_data.as_ref().and_then(|d| d.event.single_settings.clone());
                            if let Some(settings) = settings {
                            // Goal has default settings, use them to roll
                            let unlock_spoiler_log = self.effective_unlock_spoiler_log(false);
                            ctx.say(format!("@entrants Rerolling seed...")).await?;
                            self.roll_seed(ctx, self.effective_preroll_mode(), self.effective_rando_version(), settings, unlock_spoiler_log, lang, "a", format!("seed")).await;
                        } else if self.official_data.as_ref().and_then(|d| d.event.draft_kind()).is_some() {
                            // Official draft event — try to reload draft state from DB (allows fixing and retrying after a DB fix)
                            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                            let room_url: Url = format!("https://{}{}", racetime_host(), ctx.data().await.url).parse().to_racetime()?;
                            let maybe_cal_event = cal::Event::from_room(&mut transaction, &ctx.global_state.http_client, room_url).await.to_racetime()?;
                            transaction.commit().await.to_racetime()?;
                            if let Some(cal_event) = maybe_cal_event {
                                if let Some(draft) = cal_event.race.draft.clone() {
                                    let unlock_spoiler_log = self.effective_unlock_spoiler_log(false);
                                    *state = RaceState::Draft { state: draft, unlock_spoiler_log };
                                    self.advance_draft(ctx, &state).await?;
                                } else {
                                    ctx.say(format!("Sorry {reply_to}, the draft state for this race is still missing in the database. Please contact a tournament organizer.")).await?;
                                }
                            } else {
                                ctx.say(format!("Sorry {reply_to}, failed to find this race in the database.")).await?;
                            }
                        } else {
                            // Goal requires parameters
                            ctx.say(format!("Sorry {reply_to}, this goal requires settings to be specified. Please use the !seed command with the appropriate parameters to roll a seed.")).await?;
                        }
                        }
                    },
                    RaceState::Draft { .. } => ctx.say(format!("Sorry {reply_to}, settings are currently being drafted. Please finish the draft first.")).await?,
                    RaceState::Rolling => ctx.say(format!("Sorry {reply_to}, I'm currently rolling a seed. Please wait for it to finish.")).await?,
                    RaceState::Rolled(_) | RaceState::SpoilerSent => ctx.say(format!("Sorry {reply_to}, a seed has already been rolled successfully. Check the race info!")).await?,
                    });
                }
            } else {
                ctx.say(format!("Sorry {reply_to}, but the race has already started.")).await?;
            },
            "unlock" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                self.locked = false;
                ctx.say(if let French = lang {
                    "Race déverrouillée. N'importe qui peut désormais générer une seed."
                } else {
                    "Lock released. Anyone may now roll a seed."
                }).await?;
            } else {
                ctx.say(if let French = lang {
                    format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                } else {
                    format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                }).await?;
            },
            _ => ctx.say(if let French = lang {
                format!("Désolé {reply_to}, je ne reconnais pas cette commande.")
            } else {
                format!("Sorry {reply_to}, I don't recognize that command.")
            }).await?, //TODO “did you mean”? list of available commands with !help?
        }
        Ok(())
    }

    async fn race_data(&mut self, ctx: &RaceContext<GlobalState>, _old_race_data: RaceData) -> Result<(), Error> {
        let data = ctx.data().await;
        let lang = self.language();
        if let Some(OfficialRaceData { ref entrants, .. }) = self.official_data {
            for entrant in &data.entrants {
                if let Some(user) = &entrant.user {
                    match entrant.status.value {
                        EntrantStatusValue::Requested => if entrants.contains(&user.id) {
                            ctx.accept_request(&user.id).await?;
                        },
                        _ => {}
                    }
                }
            }
        }
        match data.status.value {
            RaceStatusValue::Pending => if !self.password_sent {
                lock!(@read state = self.race_state; if let RaceState::Rolled(ref seed) = *state {
                    let extra = seed.extra(Utc::now()).await.to_racetime()?;
                    if let Some(password) = extra.password {
                        ctx.say(format!("This seed is password protected. To start a file, enter this password on the file select screen:\n{}\nYou are allowed to enter the password before the race starts.", format_password(password))).await?;
                        
                        // Get game_id for the event and call set_bot_raceinfo with database-driven hash icons
                        let game_id = if let Some(OfficialRaceData { event, .. }) = &self.official_data {
                            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                            let game_id = get_game_id_from_event(&mut transaction, &event.series.to_string()).await.to_racetime()?;
                            transaction.commit().await.to_racetime()?;
                            game_id
                        } else {
                            1 // Default to OOTR if no official data
                        };
                        
                        let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                        set_bot_raceinfo(ctx, seed, None /*TODO support RSL seeds with password lock? */, true, &mut transaction, game_id).await?;
                        transaction.commit().await.to_racetime()?;
                        if let Some(OfficialRaceData { cal_event, event, .. }) = &self.official_data {
                            if event.series == Series::Standard && event.event != "w" && cal_event.race.entrants == Entrants::Open && event.discord_guild == Some(OOTR_DISCORD_GUILD) {
                                // post password in #s8-prequal-chat as a contingency for racetime.gg issues in large qualifiers
                                let mut msg = MessageBuilder::default();
                                msg.push("Seed password: ");
                                msg.push_emoji(&ReactionType::Custom { animated: false, id: EmojiId::new(658692193338392614), name: Some(format!("staffClef")) });
                                for note in password {
                                    msg.push_emoji(&ocarina_note_to_ootr_discord_emoji(note));
                                }
                                ChannelId::new(1306254442298998884).say(&*ctx.global_state.discord_ctx.read().await, msg.build()).await.to_racetime()?; //TODO move channel ID to database
                            }
                        }
                    }
                });
                self.password_sent = true;
            },
            RaceStatusValue::InProgress => {
                if let Some(breaks) = self.breaks {
                    self.break_notifications.get_or_insert_with(|| {
                        let ctx = ctx.clone();
                        tokio::spawn(async move {
                            sleep(breaks.interval - Duration::from_secs(5 * 60)).await;
                            while Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await {
                                let (_, ()) = tokio::join!(
                                    ctx.say(if let French = lang {
                                        "@entrants Rappel : pause dans 5 minutes."
                                    } else {
                                        "@entrants Reminder: Next break in 5 minutes."
                                    }),
                                    sleep(Duration::from_secs(5 * 60)),
                                );
                                if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { break }
                                let msg = if let French = lang {
                                    format!("@entrants C'est l'heure de la pause ! Elle durera {}.", French.format_duration(breaks.duration, true))
                                } else {
                                    format!("@entrants Break time! Please pause for {}.", English.format_duration(breaks.duration, true))
                                };
                                let (_, ()) = tokio::join!(
                                    ctx.say(msg),
                                    sleep(breaks.duration),
                                );
                                if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { break }
                                let (_, ()) = tokio::join!(
                                    ctx.say(if let French = lang {
                                        "@entrants Fin de la pause. Vous pouvez recommencer à jouer."
                                    } else {
                                        "@entrants Break ended. You may resume playing."
                                    }),
                                    sleep(breaks.interval - breaks.duration - Duration::from_secs(5 * 60)),
                                );
                            }
                        })
                    });
                }
            }
            RaceStatusValue::Finished => if self.unlock_spoiler_log(ctx).await? {
                {
                    // Cancel any existing finish timeout if race finishes again
                    if let Some(task) = self.finish_timeout.take() {
                        task.abort();
                    }
                    ctx.say("Race finished! Results will be confirmed in 30 seconds. Runners can still undo their finish if needed.").await?;
                    let cleaned_up = self.cleaned_up.clone();
                    let official_data = self.official_data.clone();
                    let breaks_used = self.breaks.is_some();
                    let ctx_clone = ctx.clone();
                    self.finish_timeout = Some(tokio::spawn(async move {
                        sleep(Duration::from_secs(30)).await;
                        if !cleaned_up.load(atomic::Ordering::SeqCst) {
                            if let Some(OfficialRaceData { ref cal_event, ref event, fpa_invoked, breaks_used: official_breaks_used, .. }) = official_data {
                                let data = ctx_clone.data().await;
                                // Re-check that race is still finished after the 30 second delay
                                if let RaceStatusValue::Finished = data.status.value {
                                    // Use a dummy handler to call official_race_finished
                                    // We can't call self.official_race_finished directly because we've moved into the closure
                                    let dummy_handler = Handler {
                                        official_data: Some(OfficialRaceData {
                                            cal_event: cal_event.clone(),
                                            event: event.clone(),
                                            restreams: HashMap::new(),
                                            entrants: Vec::new(),
                                            fpa_invoked,
                                            breaks_used: official_breaks_used,
                                        }),
                                        high_seed_name: String::new(),
                                        low_seed_name: String::new(),
                                        breaks: None,
                                        break_notifications: None,
                                        fpa_enabled: false,
                                        locked: false,
                                        password_sent: false,
                                        race_state: ArcRwLock::new(RaceState::Init),
                                        cleaned_up: cleaned_up.clone(),
                                                                    finish_timeout: None,
                                    };
                                    let _ = dummy_handler.official_race_finished(&ctx_clone, data, cal_event, event, fpa_invoked, official_breaks_used || breaks_used).await;
                                    cleaned_up.store(true, atomic::Ordering::SeqCst);
                                }
                            }
                        }
                    }));
                }
            },
            RaceStatusValue::Cancelled => {
                if !self.password_sent {
                    lock!(@read state = self.race_state; if let RaceState::Rolled(ref seed) = *state {
                        let extra = seed.extra(Utc::now()).await.to_racetime()?;
                        if let Some(password) = extra.password {
                            ctx.say(format!("This seed is password protected. To start a file, enter this password on the file select screen:\n{}", format_password(password))).await?;
                            
                            // Get game_id for the event and call set_bot_raceinfo with database-driven hash icons
                            let game_id = if let Some(OfficialRaceData { event, .. }) = &self.official_data {
                                let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                                let game_id = get_game_id_from_event(&mut transaction, &event.series.to_string()).await.to_racetime()?;
                                transaction.commit().await.to_racetime()?;
                                game_id
                            } else {
                                1 // Default to OOTR if no official data
                            };
                            
                            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                            set_bot_raceinfo(ctx, seed, None /*TODO support RSL seeds with password lock? */, true, &mut transaction, game_id).await?;
                            transaction.commit().await.to_racetime()?;
                        }
                    });
                    self.password_sent = true;
                }
                if let Some(OfficialRaceData { ref cal_event, ref event, .. }) = self.official_data {
                    if let cal::Source::League { id } = cal_event.race.source {
                        let form = collect![as HashMap<_, _>:
                            "id" => id.to_string(),
                        ];
                        let request = ctx.global_state.http_client.post("https://league.ootrandomizer.com/reportCancelFromMidoHouse")
                            .bearer_auth(&ctx.global_state.league_api_key)
                            .form(&form);
                        println!("reporting cancel to League website: {:?}", serde_urlencoded::to_string(&form));
                        request.send().await?.detailed_error_for_status().await.to_racetime()?;
                    } else {
                        if let Some(organizer_channel) = event.discord_organizer_channel {
                            organizer_channel.say(&*ctx.global_state.discord_ctx.read().await, MessageBuilder::default()
                                .push("race cancelled: <https://")
                                .push(racetime_host())
                                .push(&ctx.data().await.url)
                                .push('>')
                                .build()
                            ).await.to_racetime()?;
                        }
                    }
                }
                self.unlock_spoiler_log(ctx).await?;
                if matches!(self.official_data.as_ref().and_then(|d| d.event.seed_gen_type.as_ref()), Some(seed_gen_type::SeedGenType::OotrRsl)) {
                    sqlx::query!("DELETE FROM rsl_seeds WHERE room = $1", format!("https://{}{}", racetime_host(), ctx.data().await.url)).execute(&ctx.global_state.db_pool).await.to_racetime()?;
                }
                self.cleaned_up.store(true, atomic::Ordering::SeqCst);
            }
            _ => {}
        }
        Ok(())
    }

    async fn error(&mut self, _: &RaceContext<GlobalState>, mut errors: Vec<String>) -> Result<(), Error> {
        errors.retain(|error|
            !error.ends_with(" is not allowed to join this race.") // failing to invite a user should not crash the race handler
            && !error.ends_with(" is already an entrant.") // failing to invite a user should not crash the race handler
            && error != "This user has not requested to join this race. Refresh to continue." // a join request may be accepted multiple times if multiple race data changes happen in quick succession
            && error != "Specified user is not a race entrant." // failing to remove a user as entrant should not crash the race handler
        );
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Server(errors))
        }
    }
}

pub(crate) async fn create_room(transaction: &mut Transaction<'_, Postgres>, discord_ctx: &DiscordCtx, host_info: &racetime::HostInfo, client_id: &str, client_secret: &str, http_client: &reqwest::Client, clean_shutdown: Arc<Mutex<CleanShutdown>>, extra_room_senders: &Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>, cal_event: &cal::Event, event: &event::Data<'static>) -> Result<Option<(bool, String, Option<PgSnowflake<ChannelId>>)>, Error> {
    // Get the game_id for the event's series
    let game_id = get_game_id_from_event(&mut *transaction, &event.series.to_string()).await.to_racetime()?;
    
    // Get the racetime connection for this game (use the first one if multiple, or add logic as needed)
    let racetime_connection = sqlx::query!(
        r#"SELECT id, game_id, category_slug, client_id, client_secret, created_at, updated_at
           FROM game_racetime_connection WHERE game_id = $1 LIMIT 1"#,
        game_id
    )
    .fetch_optional(&mut **transaction)
    .await.to_racetime()?
    .map(|row| GameRacetimeConnection {
        id: row.id,
        game_id: row.game_id.expect("game_id should not be null"),
        category_slug: row.category_slug,
        client_id: row.client_id,
        client_secret: row.client_secret,
        created_at: row.created_at.expect("created_at should not be null"),
        updated_at: row.updated_at.expect("updated_at should not be null"),
    });

    let (category_slug, client_id, client_secret) = if let Some(connection) = racetime_connection {
        (connection.category_slug.clone(), connection.client_id.clone(), connection.client_secret.clone())
    } else {
        ("ootr".to_string(), client_id.to_string(), client_secret.to_string())
    };
    
    let room_url = match cal_event.should_create_room(&mut *transaction, event).await.to_racetime()? {
        RaceHandleMode::None => return Ok(None),
        RaceHandleMode::Notify => Err("please get your equipment and report to the tournament room"),
        RaceHandleMode::RaceTime => match racetime::authorize_with_host(host_info, &client_id, &client_secret, http_client).await {
            Ok((access_token, _)) => {
                let info_user = if_chain! {
                    if let French = event.language;
                    if let (Some(phase), Some(round)) = (cal_event.race.phase.as_ref(), cal_event.race.round.as_ref());
                    if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut **transaction).await.to_racetime()?;
                    if cal_event.race.game.is_none();
                    if let Some(entrants) = match cal_event.race.entrants {
                        Entrants::Open | Entrants::Count { .. } => Some(None), // no text
                        Entrants::Named(ref entrants) => Some(Some(entrants.clone())),
                        Entrants::Two([ref team1, ref team2]) => match cal_event.kind {
                            cal::EventKind::Normal => if let (Some(team1), Some(team2)) = (team1.name(&mut *transaction, discord_ctx).await.to_racetime()?, team2.name(&mut *transaction, discord_ctx).await.to_racetime()?) {
                                Some(Some(format!("{team1} vs {team2}")))
                            } else {
                                None // no French translation available
                            },
                            cal::EventKind::Async1 | cal::EventKind::Async2 | cal::EventKind::Async3 => None,
                        },
                        Entrants::Three([ref team1, ref team2, ref team3]) => match cal_event.kind {
                            cal::EventKind::Normal => if let (Some(team1), Some(team2), Some(team3)) = (team1.name(&mut *transaction, discord_ctx).await.to_racetime()?, team2.name(&mut *transaction, discord_ctx).await.to_racetime()?, team3.name(&mut *transaction, discord_ctx).await.to_racetime()?) {
                                Some(Some(format!("{team1} vs {team2} vs {team3}")))
                            } else {
                                None // no French translation available
                            },
                            cal::EventKind::Async1 | cal::EventKind::Async2 | cal::EventKind::Async3 => None,
                        },
                    };
                    then {
                        if let Some(entrants) = entrants {
                            format!("{phase_round} : {entrants}")
                        } else {
                            phase_round
                        }
                    } else {
                        let info_prefix = match (&cal_event.race.phase, &cal_event.race.round) {
                            (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
                            (Some(phase), None) => Some(phase.clone()),
                            (None, Some(round)) => Some(round.clone()),
                            (None, None) => None,
                        };
                        let mut info_user = match cal_event.race.entrants {
                            Entrants::Open | Entrants::Count { .. } => info_prefix.clone().unwrap_or_default(),
                            Entrants::Named(ref entrants) => format!("{}{entrants}", info_prefix.as_ref().map(|prefix| format!("{prefix}: ")).unwrap_or_default()),
                            Entrants::Two([ref team1, ref team2]) => match cal_event.kind {
                                cal::EventKind::Normal => format!(
                                    "{}{} vs {}",
                                    info_prefix.as_ref().map(|prefix| format!("{prefix}: ")).unwrap_or_default(),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                                cal::EventKind::Async1 => format!(
                                    "{} (async): {} vs {}",
                                    info_prefix.clone().unwrap_or_default(),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                                cal::EventKind::Async2 => format!(
                                    "{} (async): {} vs {}",
                                    info_prefix.clone().unwrap_or_default(),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                                cal::EventKind::Async3 => unreachable!(),
                            },
                            Entrants::Three([ref team1, ref team2, ref team3]) => match cal_event.kind {
                                cal::EventKind::Normal => format!(
                                    "{}{} vs {} vs {}",
                                    info_prefix.as_ref().map(|prefix| format!("{prefix}: ")).unwrap_or_default(),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team3.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                                cal::EventKind::Async1 => format!(
                                    "{} (async): {} vs {} vs {}",
                                    info_prefix.clone().unwrap_or_default(),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team3.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                                cal::EventKind::Async2 => format!(
                                    "{} (async): {} vs {} vs {}",
                                    info_prefix.clone().unwrap_or_default(),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team3.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                                cal::EventKind::Async3 => format!(
                                    "{} (async): {} vs {} vs {}",
                                    info_prefix.clone().unwrap_or_default(),
                                    team3.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team1.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                    team2.name(&mut *transaction, discord_ctx).await.to_racetime()?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                ),
                            },
                        };
                        if let Some(game) = cal_event.race.game {
                            info_user.push_str(", game ");
                            info_user.push_str(&game.to_string());
                        }
                        info_user
                    }
                };
                let schedule_goal = if let Some(round) = cal_event.race.round.as_deref().and_then(|r| r.strip_suffix(" Weekly")) {
                    if let Ok(Some(schedule)) = WeeklySchedule::for_round(&mut *transaction, cal_event.race.series, &cal_event.race.event, round).await {
                        schedule.racetime_goal
                    } else {
                        None
                    }
                } else {
                    None
                };
                let (goal_str, goal_is_custom) = if let Some(ref g) = schedule_goal {
                    let is_custom = sqlx::query_scalar!(
                        "SELECT is_custom_goal FROM events WHERE racetime_goal_slug = $1 LIMIT 1",
                        g
                    ).fetch_optional(&mut **transaction).await.to_racetime()?.unwrap_or(true);
                    (g.clone(), is_custom)
                } else {
                    if let Some(ref slug) = cal_event.race.racetime_goal_slug {
                        (slug.clone(), event.is_custom_goal)
                    } else {
                        return Ok(None)
                    }
                };
                let race_slug = room_options(
                    goal_str, goal_is_custom, event, cal_event,
                    info_user,
                    String::default(),
                    cal_event.is_private_async_part() || cal_event.race.video_urls.is_empty(),
                ).await.start_with_host(host_info, &access_token, &http_client, &category_slug).await?;
                let room_url = Url::parse(&format!("https://{}/{}/{}", host_info.hostname, category_slug, race_slug))?;
                match cal_event.kind {
                    cal::EventKind::Normal => { sqlx::query!("UPDATE races SET room = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                    cal::EventKind::Async1 => { sqlx::query!("UPDATE races SET async_room1 = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                    cal::EventKind::Async2 => { sqlx::query!("UPDATE races SET async_room2 = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                    cal::EventKind::Async3 => { sqlx::query!("UPDATE races SET async_room3 = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                }
                // Notify the bot immediately so it can start the WebSocket connection now,
                // rather than waiting for the next 5-second poll tick. new_room_lock (held
                // by the caller) ensures Handler::new() won't run until after the
                // transaction commits, so this is safe to send before the commit.
                lock!(@read senders = extra_room_senders; {
                    if let Some(sender) = senders.get(&category_slug) {
                        sender.send(race_slug).await.ok();
                    }
                });
                Ok(room_url)
            }
            Err(Error::Reqwest(e)) if e.status().is_some_and(|status| status.is_server_error()) => {
                // racetime.gg's auth endpoint has been known to return server errors intermittently.
                // In that case, we simply try again in the next iteration of the sleep loop.
                return Ok(None)
            }
            Err(e) => return Err(e),
        },
        RaceHandleMode::Discord => {
            let task_clean_shutdown = clean_shutdown.clone();
            lock!(clean_shutdown = clean_shutdown; {
                if clean_shutdown.should_handle_new() {
                    let room = OpenRoom::Discord { id: cal_event.race.id.into(), kind: cal_event.kind };
                    assert!(clean_shutdown.open_rooms.insert(room.clone()));
                    clean_shutdown.updates.send(CleanShutdownUpdate::RoomOpened(room)).allow_unreceived();
                    let discord_ctx = discord_ctx.clone();
                    let cal_event = cal_event.clone();
                    let event = event.clone();
                    tokio::spawn(async move {
                        println!("Discord race handler started");
                        let res = tokio::spawn(crate::discord_bot::handle_race(discord_ctx, cal_event.clone(), event)).await;
                        lock!(clean_shutdown = task_clean_shutdown; {
                            let room = OpenRoom::Discord { id: cal_event.race.id.into(), kind: cal_event.kind };
                            assert!(clean_shutdown.open_rooms.remove(&room));
                            clean_shutdown.updates.send(CleanShutdownUpdate::RoomClosed(room)).allow_unreceived();
                            if clean_shutdown.open_rooms.is_empty() {
                                clean_shutdown.updates.send(CleanShutdownUpdate::Empty).allow_unreceived();
                            }
                        });
                        match res {
                            Ok(Ok(())) => println!("Discord race handler stopped"),
                            Ok(Err(e)) => {
                                eprintln!("Discord race handler errored: {e} ({e:?})");
                                if let Environment::Production = Environment::default() {
                                    log::error!("Discord race handler errored: {e} ({e:?})");
                                }
                            }
                            Err(_) => {
                                eprintln!("Discord race handler panicked");
                                if let Environment::Production = Environment::default() {
                                    log::error!("Discord race handler panicked");
                                }
                            }
                        }
                    });
                }
            });
            Err("remember to send your video to an organizer once you're done") //TODO “please check your direct messages” for private async parts, “will be handled here in the match thread” for public async parts
        }
    };
    let handle_mode = cal_event.should_create_room(&mut *transaction, event).await.ok();
    if matches!(handle_mode, Some(RaceHandleMode::Discord)) {
        return Ok(None);
    }
    
    // Check if this is a weekly race and get its notification channel/role from the database,
    // or if it's a qualifier race, get the event-level qualifier notification role.
    let (weekly_notification_channel, weekly_notification_role) = if cal_event.race.phase.is_none() {
        if let Some(round) = cal_event.race.round.as_deref().and_then(|r| r.strip_suffix(" Weekly")) {
            let schedule = WeeklySchedule::for_round(&mut *transaction, event.series, &event.event, round)
                .await
                .to_racetime()?;
            (
                schedule.as_ref().and_then(|s| s.notification_channel_id),
                schedule.and_then(|s| s.notification_role_id),
            )
        } else {
            (None, None)
        }
    } else if cal_event.race.phase.as_deref() == Some("Qualifier") {
        (
            None,
            event.qualifier_notification_role_id.map(|id| PgSnowflake(id)),
        )
    } else {
        (None, None)
    };

    let is_room_url = room_url.is_ok();
    let msg = if_chain! {
        if let French = event.language;
        if let Ok(ref room_url_fr) = room_url;
        if let (Some(phase), Some(round)) = (cal_event.race.phase.as_ref(), cal_event.race.round.as_ref());
        if let Some(Some(phase_round)) = sqlx::query_scalar!("SELECT display_fr FROM phase_round_options WHERE series = $1 AND event = $2 AND phase = $3 AND round = $4", event.series as _, &event.event, phase, round).fetch_optional(&mut **transaction).await.to_racetime()?;
        if cal_event.race.game.is_none();
        then {
            let mut msg = MessageBuilder::default();
            msg.push("La race commence ");
            msg.push_timestamp(cal_event.start().expect("opening room for official race without start time"), serenity_utils::message::TimestampStyle::Relative);
            msg.push(" : ");
            match cal_event.race.entrants {
                Entrants::Open | Entrants::Count { .. } => {
                    msg.push_safe(phase_round);
                },
                Entrants::Named(ref entrants) => {
                    msg.push_safe(phase_round);
                    msg.push(" : ");
                    msg.push_safe(entrants);
                }
                Entrants::Two([ref team1, ref team2]) => {
                    msg.push_safe(phase_round);
                    //TODO adjust for asyncs
                    msg.push(" : ");
                    msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                    msg.push(" vs ");
                    msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                }
                Entrants::Three([ref team1, ref team2, ref team3]) => {
                    msg.push_safe(phase_round);
                    //TODO adjust for asyncs
                    msg.push(" : ");
                    msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                    msg.push(" vs ");
                    msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                    msg.push(" vs ");
                    msg.mention_entrant(&mut *transaction, event.discord_guild, team3).await.to_racetime()?;
                }
            }
            msg.push(" <");
            msg.push(room_url_fr.to_string());
            msg.push('>');
            msg.build()
        } else {
            let info_prefix = match (&cal_event.race.phase, &cal_event.race.round) {
                (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
                (Some(phase), None) => Some(phase.clone()),
                (None, Some(round)) => Some(round.clone()),
                (None, None) => None,
            };
            let mut msg = MessageBuilder::default();
            msg.push("race starting ");
            msg.push_timestamp(cal_event.start().expect("opening room for official race without start time"), serenity_utils::message::TimestampStyle::Relative);
            msg.push(": ");
            match cal_event.race.entrants {
                Entrants::Open | Entrants::Count { .. } => if let Some(prefix) = info_prefix {
                    msg.push_safe(prefix);
                },
                Entrants::Named(ref entrants) => {
                    if let Some(prefix) = info_prefix {
                        msg.push_safe(prefix);
                        msg.push(": ");
                    }
                    msg.push_safe(entrants);
                }
                Entrants::Two([ref team1, ref team2]) => {
                    if let Some(prefix) = info_prefix {
                        msg.push_safe(prefix);
                        match cal_event.kind {
                            cal::EventKind::Normal => {
                                msg.push(": ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                            }
                            cal::EventKind::Async1 => {
                                msg.push(" (async): ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                            }
                            cal::EventKind::Async2 => {
                                msg.push(" (async): ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                            }
                            cal::EventKind::Async3 => unreachable!(),
                        }
                    } else {
                        //TODO adjust for asyncs
                        msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                        msg.push(" vs ");
                        msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                    }
                }
                Entrants::Three([ref team1, ref team2, ref team3]) => {
                    if let Some(prefix) = info_prefix {
                        msg.push_safe(prefix);
                        match cal_event.kind {
                            cal::EventKind::Normal => {
                                msg.push(": ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team3).await.to_racetime()?;
                            }
                            cal::EventKind::Async1 => {
                                msg.push(" (async): ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team3).await.to_racetime()?;
                            }
                            cal::EventKind::Async2 => {
                                msg.push(" (async): ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team3).await.to_racetime()?;
                            }
                            cal::EventKind::Async3 => {
                                msg.push(" (async): ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team3).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                                msg.push(" vs ");
                                msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                            }
                        }
                    } else {
                        //TODO adjust for asyncs
                        msg.mention_entrant(&mut *transaction, event.discord_guild, team1).await.to_racetime()?;
                        msg.push(" vs ");
                        msg.mention_entrant(&mut *transaction, event.discord_guild, team2).await.to_racetime()?;
                        msg.push(" vs ");
                        msg.mention_entrant(&mut *transaction, event.discord_guild, team3).await.to_racetime()?;
                    }
                }
            }
            if let Some(game) = cal_event.race.game {
                msg.push(", game ");
                msg.push(game.to_string());
            }
            match room_url {
                Ok(room_url) => {
                    msg.push(" <");
                    msg.push(room_url);
                    msg.push('>');
                }
                Err(notification) => if cal_event.race.notified {
                    return Ok(None)
                } else {
                    msg.push(" — ");
                    msg.push(notification);
                    sqlx::query!("UPDATE races SET notified = TRUE WHERE id = $1", cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?;
                },
            }
            msg.build()
        }
    };
    let msg = if let Some(PgSnowflake(role_id)) = weekly_notification_role {
        format!("<@&{}> {}", role_id.get(), msg)
    } else {
        msg
    };
    Ok(Some((is_room_url, msg, weekly_notification_channel)))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PrepareSeedsError {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] EventData(#[from] event::DataError),
    #[error(transparent)] Roll(#[from] RollError),
    #[error(transparent)] SeedData(#[from] seed::ExtraDataError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

async fn prepare_seeds(global_state: Arc<GlobalState>, mut shutdown: rocket::Shutdown) -> Result<(), PrepareSeedsError> {
    'outer: loop {
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND room IS NULL AND async_room1 IS NULL AND async_room2 IS NULL AND async_room3 IS NULL AND seed_data IS NULL"#).fetch_all(&global_state.db_pool).await? {
            let mut transaction = global_state.db_pool.begin().await?;
            let race = Race::from_id(&mut transaction, &global_state.http_client, id).await?;
            let event = race.event(&mut transaction).await?;
            if event.seed_gen_type.is_some() && event.preroll_mode == "long" {
                if let Some(settings) = race.single_settings(&mut transaction).await? {
                    let unlock_spoiler_log = match event.spoiler_unlock.as_str() {
                        "after" => UnlockSpoilerLog::After,
                        "immediately" => UnlockSpoilerLog::Now,
                        _ => UnlockSpoilerLog::Never,
                    };
                    let rando_version = event.rando_version.clone()
                        .unwrap_or(VersionedBranch::Latest { branch: rando::Branch::Dev });
                        transaction.commit().await?;
                        if race.seed.files().is_none()
                        && race
                            .cal_events()
                            .filter_map(|cal_event| cal_event.start())
                            .min()
                            .is_some_and(|start| start > Utc::now())
                        {
                            'seed: loop {
                                let mut seed_rx = global_state.clone().roll_seed(
                                    PrerollMode::Long,
                                    false,
                                    None,
                                    rando_version.clone(),
                                    settings.clone(),
                                    unlock_spoiler_log,
                                );
                                loop {
                                    select! {
                                        () = &mut shutdown => break 'outer,
                                        Some(update) = seed_rx.recv() => match update {
                                            SeedRollUpdate::Queued(_) |
                                            SeedRollUpdate::MovedForward(_) |
                                            SeedRollUpdate::Started => {}
                                            SeedRollUpdate::Done { mut seed, .. } => {
                                                let extra = seed.extra(Utc::now()).await?;
                                                seed.file_hash = extra.file_hash;
                                                seed.password = extra.password;
                                                // reload race data in case anything changed during seed rolling
                                                let mut transaction = global_state.db_pool.begin().await?;
                                                let mut race = Race::from_id(&mut transaction, &global_state.http_client, race.id).await?;
                                                if !race.has_any_room() {
                                                    race.seed = seed;
                                                    race.save(&mut transaction).await?;
                                                }
                                                transaction.commit().await?;
                                                break 'seed
                                            }
                                            SeedRollUpdate::Error(RollError::Retries { num_retries, last_error }) => {
                                                if let Some(last_error) = last_error {
                                                    eprintln!("seed rolling failed {num_retries} times, sample error:\n{last_error}");
                                                } else {
                                                    eprintln!("seed rolling failed {num_retries} times, no sample error recorded");
                                                }
                                                continue 'seed
                                            }
                                            SeedRollUpdate::Error(e) => return Err(e.into()),
                                            #[cfg(unix)] SeedRollUpdate::Message(_) => {}
                                        },
                                    }
                                }
                            }
                        }
                }
            }
        }
        // Global prerolled seed pool: query events that have fixed settings and long preroll.
        // This replaces the old all::<Goal>() iteration — seeds are now keyed by event.racetime_goal_slug.
        let pool_events = sqlx::query!(
            r#"SELECT racetime_goal_slug, preroll_mode AS "preroll_mode!", spoiler_unlock AS "spoiler_unlock!",
               rando_version AS "rando_version: sqlx::types::Json<VersionedBranch>",
               single_settings AS "single_settings: sqlx::types::Json<seed::Settings>"
               FROM events
               WHERE seed_gen_type IS NOT NULL AND preroll_mode = 'long'
               AND single_settings IS NOT NULL
               AND (end_time IS NULL OR end_time > NOW())"#
        ).fetch_all(&global_state.db_pool).await?;
        for row in pool_events {
            let goal_name = row.racetime_goal_slug.as_deref().unwrap_or("unknown");
            let settings = match row.single_settings { Some(sqlx::types::Json(s)) => s, None => continue };
            if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM prerolled_seeds WHERE goal_name = $1) AS "exists!""#, goal_name).fetch_one(&global_state.db_pool).await? { break }
            let rando_version = row.rando_version.map(|sqlx::types::Json(v)| v)
                .unwrap_or(VersionedBranch::Latest { branch: rando::Branch::Dev });
            let unlock_spoiler_log = match row.spoiler_unlock.as_str() {
                "after" => UnlockSpoilerLog::After,
                "immediately" => UnlockSpoilerLog::Now,
                _ => UnlockSpoilerLog::Never,
            };
            let progression_spoiler = unlock_spoiler_log == UnlockSpoilerLog::Progression;
            'seed: loop {
                let mut seed_rx = global_state.clone().roll_seed(
                    PrerollMode::Long,
                    false,
                    None,
                    rando_version.clone(),
                    settings.clone(),
                    unlock_spoiler_log,
                );
                loop {
                    select! {
                        () = &mut shutdown => break 'outer,
                        Some(update) = seed_rx.recv() => match update {
                            SeedRollUpdate::Queued(_) |
                            SeedRollUpdate::MovedForward(_) |
                            SeedRollUpdate::Started => {}
                            SeedRollUpdate::Done { seed, .. } => {
                                let extra = seed.extra(Utc::now()).await?;
                                let [hash1, hash2, hash3, hash4, hash5] = match extra.file_hash {
                                    Some(hash) => hash.map(Some),
                                    None => [const { None }; 5],
                                };
                                match seed.files() {
                                    Some(seed::Files::MidosHouse { file_stem, locked_spoiler_log_path }) => {
                                        sqlx::query!("INSERT INTO prerolled_seeds
                                            (goal_name, file_stem, locked_spoiler_log_path, hash1, hash2, hash3, hash4, hash5, seed_password, progression_spoiler)
                                        VALUES
                                            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                                        ",
                                            goal_name,
                                            &*file_stem,
                                            locked_spoiler_log_path,
                                            hash1 as _,
                                            hash2 as _,
                                            hash3 as _,
                                            hash4 as _,
                                            hash5 as _,
                                            extra.password.map(|password| password.into_iter().map(char::from).collect::<String>()),
                                            progression_spoiler,
                                        ).execute(&global_state.db_pool).await?;
                                    }
                                    _ => unimplemented!("unexpected seed files in prerolled seed"),
                                }
                                break 'seed
                            }
                            SeedRollUpdate::Error(RollError::Retries { num_retries, last_error }) => {
                                if let Some(last_error) = last_error {
                                    eprintln!("seed rolling failed {num_retries} times, sample error:\n{last_error}");
                                } else {
                                    eprintln!("seed rolling failed {num_retries} times, no sample error recorded");
                                }
                                continue 'seed
                            }
                            SeedRollUpdate::Error(e) => return Err(e.into()),
                            #[cfg(unix)] SeedRollUpdate::Message(_) => {}
                        },
                    }
                }
            }
        }
        select! {
            () = &mut shutdown => break,
            () = sleep(Duration::from_secs(60 * 60)) => {}
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CreateRoomsError {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] Discord(#[from] serenity::Error),
    #[error(transparent)] EventData(#[from] event::DataError),
    #[error(transparent)] RaceTime(#[from] Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

async fn create_rooms(global_state: Arc<GlobalState>, mut shutdown: rocket::Shutdown) -> Result<(), CreateRoomsError> {
    loop {
        select! {
            () = &mut shutdown => break,
            _ = sleep(Duration::from_secs(30)) => { //TODO exact timing (coordinate with everything that can change the schedule)
                // Query for rooms to open while holding the lock
                let rooms_to_open = lock!(new_room_lock = global_state.new_room_lock; {
                    let mut transaction = global_state.db_pool.begin().await?;
                    let rooms = cal::Event::rooms_to_open(&mut transaction, &global_state.http_client).await?;
                    transaction.commit().await?;
                    rooms
                });

                for cal_event in rooms_to_open {
                    // Create room while holding lock, commit transaction, then release lock immediately so handler can start
                    let (is_room_url, msg, notification_channel, event) = {
                        let mut transaction = global_state.db_pool.begin().await?;
                        let event = cal_event.race.event(&mut transaction).await?;
                        let result = lock!(new_room_lock = global_state.new_room_lock; {
                            let result = create_room(&mut transaction, &*global_state.discord_ctx.read().await, &global_state.host_info, &global_state.racetime_config.client_id, &global_state.racetime_config.client_secret, &global_state.http_client, global_state.clean_shutdown.clone(), &global_state.extra_room_senders, &cal_event, &event).await?;

                            if let Some((is_room_url, mut msg, notification_channel)) = result {
                                // Add warning if draft is incomplete
                                if let Some(draft_kind) = event.draft_kind() {
                                    if let Some(draft) = &cal_event.race.draft {
                                        if let Ok(step) = draft.next_step(&draft_kind, cal_event.race.game, &mut draft::MessageContext::None).await {
                                            if !matches!(step.kind, draft::StepKind::Done(_)) {
                                                msg.push_str("\n\n⚠️ **WARNING**: The mode draft for this match is not complete! Please complete the draft as soon as possible. The seed cannot be rolled until the draft is finished.");
                                            }
                                        }
                                    }
                                }
                                // Commit the transaction to save the room URL to the database before the bot handler queries for it
                                transaction.commit().await?;
                                // Lock is released here, allowing the handler to start immediately
                                Ok::<_, CreateRoomsError>(Some((is_room_url, msg, notification_channel, event)))
                            } else {
                                Ok(None)  // No room was created
                            }
                        })?;

                        match result {
                            Some(tuple) => tuple,
                            None => continue,  // No room was created, skip to next iteration
                        }
                    };

                    // Lock released here - handler can now start while we send Discord messages
                    let ctx = global_state.discord_ctx.read().await;
                    if is_room_url && cal_event.is_private_async_part() {
                        let msg = match cal_event.race.entrants {
                            Entrants::Two(_) => format!("unlisted room for first async half: {msg}"),
                            Entrants::Three(_) => format!("unlisted room for first/second async part: {msg}"),
                            _ => format!("unlisted room for async part: {msg}"),
                        };
                        if let Some(channel) = event.discord_organizer_channel {
                            channel.say(&*ctx, &msg).await?;
                        } else {
                            // DM Admin
                            ADMIN_USER.create_dm_channel(&*ctx).await?.say(&*ctx, &msg).await?;
                        }
                        // Start a new transaction for querying team members
                        let mut transaction = global_state.db_pool.begin().await?;
                        for team in cal_event.active_teams() {
                            for member in team.members(&mut transaction).await? {
                                if let Some(discord) = member.discord {
                                    discord.id.create_dm_channel(&*ctx).await?.say(&*ctx, &msg).await?;
                                }
                            }
                        }
                        transaction.commit().await?;
                    } else {
                        // For weekly races with a configured notification channel, use that instead
                        if let Some(PgSnowflake(channel_id)) = notification_channel {
                            if let Err(e) = channel_id.say(&*ctx, &msg).await {
                                eprintln!("Failed to post race message to weekly notification channel: {}", e);
                            }
                        } else {
                            if_chain! {
                                if !cal_event.is_private_async_part();
                                if let Some(channel) = event.discord_race_room_channel;
                                then {
                                    if let Some(thread) = cal_event.race.scheduling_thread {
                                        if let Err(e) = thread.say(&*ctx, &msg).await {
                                            eprintln!("Failed to post race message to scheduling thread: {}", e);
                                        }
                                        if let Err(e) = channel.send_message(&*ctx, CreateMessage::default().content(msg).allowed_mentions(CreateAllowedMentions::default())).await {
                                            eprintln!("Failed to post race message to Discord race room channel: {}", e);
                                        }
                                    } else {
                                        if let Err(e) = channel.say(&*ctx, msg).await {
                                            eprintln!("Failed to post race message to Discord race room channel: {}", e);
                                        }
                                    }
                                } else {
                                    if let Some(thread) = cal_event.race.scheduling_thread {
                                        thread.say(&*ctx, msg).await?;
                                    } else if let Some(channel) = event.discord_organizer_channel {
                                        channel.say(&*ctx, msg).await?;
                                    } else {
                                        // DM Admin
                                        ADMIN_USER.create_dm_channel(&*ctx).await?.say(&*ctx, msg).await?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HandleRoomsError {
    #[error(transparent)] RaceTime(#[from] Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
}

async fn handle_rooms(global_state: Arc<GlobalState>, shutdown: rocket::Shutdown) -> Result<(), HandleRoomsError> {
    let mut last_crash = Instant::now();
    let mut wait_time = Duration::from_secs(1);
    loop {
        // Get all racetime connections from the database
        let mut transaction = global_state.db_pool.begin().await.to_racetime()?;
        let racetime_connections = sqlx::query!(
            r#"SELECT id, game_id, category_slug, client_id, client_secret, created_at, updated_at
               FROM game_racetime_connection"#
        )
        .fetch_all(&mut *transaction)
        .await.to_racetime()?
        .into_iter()
        .map(|row| GameRacetimeConnection {
            id: row.id,
            game_id: row.game_id.expect("game_id should not be null"),
            category_slug: row.category_slug,
            client_id: row.client_id,
            client_secret: row.client_secret,
            created_at: row.created_at.expect("created_at should not be null"),
            updated_at: row.updated_at.expect("updated_at should not be null"),
        })
        .collect::<Vec<_>>();
        transaction.commit().await.to_racetime()?;
        
        if racetime_connections.is_empty() {
            // No database connections found - this is now an error
            eprintln!("No racetime connections found in database. Please add entries to game_racetime_connection table.");
            sleep(Duration::from_secs(60)).await; // Wait 1 minute before retrying
            continue;
        }
        
        // Create multiple bot instances, one for each category
        let mut bot_handles = Vec::new();
        
        for connection in racetime_connections {
            let global_state = global_state.clone();
            let shutdown = shutdown.clone();
            let category_slug = connection.category_slug.clone();
            
            println!("Creating bot for category '{}'", category_slug);
            
            match racetime::BotBuilder::new(&connection.category_slug, &connection.client_id, &connection.client_secret)
                .state(global_state.clone())
                .host(global_state.host_info.clone())
                .user_agent(concat!("HyruleTownHall/", env!("CARGO_PKG_VERSION"), " (https://github.com/TreZc0/hyrule-town-hall)"))
                .scan_races_every(Duration::from_secs(5))
                .build().await
            {
                Ok(bot) => {
                    let sender = bot.extra_room_sender();
                    lock!(@write senders = global_state.extra_room_senders; senders.insert(category_slug.clone(), sender));
                    let handle = tokio::spawn(async move {
                        let _ = bot.run_until::<Handler, _, _>(shutdown).await;
                    });
                    bot_handles.push(handle);
                }
                Err(e) => {
                    eprintln!("failed to create bot for category {}: {e} ({e:?})", connection.category_slug);
                    // Continue with other bots even if one fails
                }
            }
        }
        
        if bot_handles.is_empty() {
            // All bots failed to start
            if last_crash.elapsed() >= Duration::from_secs(60 * 60 * 24) {
                wait_time = Duration::from_secs(1);
            } else {
                wait_time *= 2;
            }
            eprintln!("failed to connect any racetime.gg bots (retrying in {}): {}", English.format_duration(wait_time, true), wait_time.as_secs());
            sleep(wait_time).await;
            last_crash = Instant::now();
        } else {
            // Wait for all bots to complete
            for handle in bot_handles {
                let _ = handle.await;
            }
            break Ok(())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MainError {
    #[error(transparent)] CreateRooms(#[from] CreateRoomsError),
    #[error(transparent)] HandleRooms(#[from] HandleRoomsError),
    #[error(transparent)] PrepareSeeds(#[from] PrepareSeedsError),
} 

pub(crate) async fn main(_config: Config, shutdown: rocket::Shutdown, global_state: Arc<GlobalState>) -> Result<(), MainError> {
    let ((), (), ()) = tokio::try_join!(
        prepare_seeds(global_state.clone(), shutdown.clone()).err_into::<MainError>(),
        create_rooms(global_state.clone(), shutdown.clone()).err_into(),
        handle_rooms(global_state, shutdown).err_into(),
    )?;
    Ok(())
}
