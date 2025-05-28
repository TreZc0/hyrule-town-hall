use {
    std::{
        io::prelude::*,
        process::Stdio,
        sync::atomic::{
            self,
            AtomicUsize,
        },
    },
    kuchiki::{
        NodeRef,
        traits::TendrilSink as _,
    },
    mhstatus::OpenRoom,
    ootr_utils as rando,
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
        io::AsyncWriteExt as _,
        time::timeout,
    },
    crate::{
        cal::Entrant,
        config::ConfigRaceTime,
        discord_bot::ADMIN_USER,
        prelude::*,
    },
};
#[cfg(unix)] use async_proto::Protocol;
#[cfg(windows)] use directories::UserDirs;

mod report;

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
}

impl VersionedBranch {
    pub(crate) fn branch(&self) -> Option<rando::Branch> {
        match self {
            Self::Pinned { version } => Some(version.branch()),
            Self::Latest { branch } => Some(*branch),
            Self::Custom { .. } => None,
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Clone, Copy, PartialEq, Eq, Sequence)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) enum Goal {
    Cc7,
    CoOpS3,
    CopaDoBrasil,
    Crosskeys2025,
    LeagueS8,
    MixedPoolsS2,
    MixedPoolsS3,
    MixedPoolsS4,
    Mq,
    MultiworldS3,
    MultiworldS4,
    MultiworldS5,
    NineDaysOfSaws,
    Pic7,
    PicRs2,
    Rsl,
    Sgl2023,
    Sgl2024,
    SongsOfHope,
    StandardRuleset,
    TournoiFrancoS3,
    TournoiFrancoS4,
    TournoiFrancoS5,
    TriforceBlitz,
    TriforceBlitzProgressionSpoiler,
    WeTryToBeBetterS1,
    WeTryToBeBetterS2,
}

#[derive(Debug, thiserror::Error)]
#[error("this racetime.gg goal is not handled by Mido")]
pub(crate) struct GoalFromStrError;

impl Goal {
    pub(crate) fn for_event(series: Series, event: &str) -> Option<Self> {
        all::<Self>().find(|goal| goal.matches_event(series, event))
    }

    fn from_race_data(race_data: &RaceData) -> Option<Self> {
        let Ok(bot_goal) = race_data.goal.name.parse::<Self>() else { return None };
        if race_data.goal.custom != bot_goal.is_custom() { return None }
        if let (Goal::StandardRuleset | Goal::Crosskeys2025, Some(_)) = (bot_goal, &race_data.opened_by) { return None }
        Some(bot_goal)
    }

    fn matches_event(&self, series: Series, event: &str) -> bool {
        match self {
            Self::Cc7 => series == Series::Standard && event == "7cc",
            Self::CoOpS3 => series == Series::CoOp && event == "3",
            Self::CopaDoBrasil => series == Series::CopaDoBrasil && event == "1",
            Self::Crosskeys2025 => series == Series::Crosskeys && event == "2025",
            Self::LeagueS8 => series == Series::League && event == "8",
            Self::MixedPoolsS2 => series == Series::MixedPools && event == "2",
            Self::MixedPoolsS3 => series == Series::MixedPools && event == "3",
            Self::MixedPoolsS4 => series == Series::MixedPools && event == "4",
            Self::Mq => series == Series::Mq && event == "1",
            Self::MultiworldS3 => series == Series::Multiworld && event == "3",
            Self::MultiworldS4 => series == Series::Multiworld && event == "4",
            Self::MultiworldS5 => series == Series::Multiworld && event == "5",
            Self::NineDaysOfSaws => series == Series::NineDaysOfSaws,
            Self::Pic7 => series == Series::Pictionary && event == "7",
            Self::PicRs2 => series == Series::Pictionary && event == "rs2",
            Self::Rsl => series == Series::Rsl,
            Self::Sgl2023 => series == Series::SpeedGaming && event.starts_with("2023"),
            Self::Sgl2024 => series == Series::SpeedGaming && event.starts_with("2024"),
            Self::SongsOfHope => series == Series::SongsOfHope && event == "1",
            Self::StandardRuleset => series == Series::Standard && matches!(event, "w" | "8" | "8cc"),
            Self::TournoiFrancoS3 => series == Series::TournoiFrancophone && event == "3",
            Self::TournoiFrancoS4 => series == Series::TournoiFrancophone && event == "4",
            Self::TournoiFrancoS5 => series == Series::TournoiFrancophone && event == "5",
            Self::TriforceBlitz => series == Series::TriforceBlitz,
            Self::TriforceBlitzProgressionSpoiler => false, // possible future tournament but no concrete plans
            Self::WeTryToBeBetterS1 => series == Series::WeTryToBeBetter && event == "1",
            Self::WeTryToBeBetterS2 => series == Series::WeTryToBeBetter && event == "2",
        }
    }

    pub(crate) fn is_custom(&self) -> bool {
        match self {
            | Self::Rsl
            | Self::StandardRuleset
            | Self::TriforceBlitz
                => false,
            | Self::Cc7
            | Self::CoOpS3
            | Self::CopaDoBrasil
            | Self::Crosskeys2025
            | Self::LeagueS8
            | Self::MixedPoolsS2
            | Self::MixedPoolsS3
            | Self::MixedPoolsS4
            | Self::Mq
            | Self::MultiworldS3
            | Self::MultiworldS4
            | Self::MultiworldS5
            | Self::NineDaysOfSaws
            | Self::Pic7
            | Self::PicRs2
            | Self::Sgl2023
            | Self::Sgl2024
            | Self::SongsOfHope
            | Self::TournoiFrancoS3
            | Self::TournoiFrancoS4
            | Self::TournoiFrancoS5
            | Self::TriforceBlitzProgressionSpoiler
            | Self::WeTryToBeBetterS1
            | Self::WeTryToBeBetterS2
                => true,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Cc7 => "Standard Tournament Season 7 Challenge Cup",
            Self::CoOpS3 => "Co-op Tournament Season 3",
            Self::CopaDoBrasil => "Copa do Brasil",
            Self::Crosskeys2025 => "ALttP Randomizer Crosskeys 2025",
            Self::LeagueS8 => "League Season 8",
            Self::MixedPoolsS2 => "2nd Mixed Pools Tournament",
            Self::MixedPoolsS3 => "3rd Mixed Pools Tournament",
            Self::MixedPoolsS4 => "4th Mixed Pools Tournament",
            Self::Mq => "12 MQ Tournament",
            Self::MultiworldS3 => "3rd Multiworld Tournament",
            Self::MultiworldS4 => "4th Multiworld Tournament",
            Self::MultiworldS5 => "5th Multiworld Tournament",
            Self::NineDaysOfSaws => "9 Days of SAWS",
            Self::Pic7 => "7th Pictionary Spoiler Log Race",
            Self::PicRs2 => "2nd Random Settings Pictionary Spoiler Log Race",
            Self::Rsl => "Random settings league",
            Self::Sgl2023 => "SGL 2023",
            Self::Sgl2024 => "SGL 2024",
            Self::SongsOfHope => "Songs of Hope",
            Self::StandardRuleset => "Standard Ruleset",
            Self::TournoiFrancoS3 => "Tournoi Francophone Saison 3",
            Self::TournoiFrancoS4 => "Tournoi Francophone Saison 4",
            Self::TournoiFrancoS5 => "Tournoi Francophone Saison 5",
            Self::TriforceBlitz => "Triforce Blitz",
            Self::TriforceBlitzProgressionSpoiler => "Triforce Blitz Progression Spoiler",
            Self::WeTryToBeBetterS1 => "WeTryToBeBetter",
            Self::WeTryToBeBetterS2 => "WeTryToBeBetter Season 2",
        }
    }

    fn language(&self) -> Language {
        match self {
            | Self::Cc7
            | Self::CoOpS3
            | Self::Crosskeys2025
            | Self::LeagueS8
            | Self::MixedPoolsS2
            | Self::MixedPoolsS3
            | Self::MixedPoolsS4
            | Self::Mq
            | Self::MultiworldS3
            | Self::MultiworldS4
            | Self::MultiworldS5
            | Self::NineDaysOfSaws
            | Self::Pic7
            | Self::PicRs2
            | Self::Rsl
            | Self::Sgl2023
            | Self::Sgl2024
            | Self::SongsOfHope
            | Self::StandardRuleset
            | Self::TriforceBlitz
            | Self::TriforceBlitzProgressionSpoiler
                => English,
            | Self::TournoiFrancoS4
            | Self::TournoiFrancoS5
                => English, //TODO change to bilingual English/French
            | Self::TournoiFrancoS3
            | Self::WeTryToBeBetterS1
            | Self::WeTryToBeBetterS2
                => French,
            | Self::CopaDoBrasil
                => Portuguese,
        }
    }

    fn draft_kind(&self) -> Option<draft::Kind> {
        match self {
            Self::Cc7 => Some(draft::Kind::S7),
            Self::MultiworldS3 => Some(draft::Kind::MultiworldS3),
            Self::MultiworldS4 => Some(draft::Kind::MultiworldS4),
            Self::MultiworldS5 => Some(draft::Kind::MultiworldS5),
            Self::Rsl => Some(draft::Kind::RslS7),
            Self::TournoiFrancoS3 => Some(draft::Kind::TournoiFrancoS3),
            Self::TournoiFrancoS4 => Some(draft::Kind::TournoiFrancoS4),
            Self::TournoiFrancoS5 => Some(draft::Kind::TournoiFrancoS5),
            | Self::CoOpS3
            | Self::CopaDoBrasil
            | Self::Crosskeys2025
            | Self::LeagueS8
            | Self::MixedPoolsS2
            | Self::MixedPoolsS3
            | Self::MixedPoolsS4
            | Self::Mq
            | Self::NineDaysOfSaws
            | Self::Pic7
            | Self::PicRs2
            | Self::Sgl2023
            | Self::Sgl2024
            | Self::SongsOfHope
            | Self::StandardRuleset
            | Self::TriforceBlitz
            | Self::TriforceBlitzProgressionSpoiler
            | Self::WeTryToBeBetterS1
            | Self::WeTryToBeBetterS2
                => None,
        }
    }

    /// See the [`PrerollMode`] docs.
    pub(crate) fn preroll_seeds(&self, event: Option<(Series, &str)>) -> PrerollMode {
        match self {
            | Self::Crosskeys2025
            | Self::Sgl2023
            | Self::Sgl2024
            | Self::TriforceBlitz
                => PrerollMode::None,
            | Self::Cc7
            | Self::CoOpS3
                => PrerollMode::Short,
            | Self::CopaDoBrasil
            | Self::LeagueS8
            | Self::Mq
            | Self::MultiworldS3
            | Self::MultiworldS4
            | Self::MultiworldS5
            | Self::NineDaysOfSaws
            | Self::Pic7
            | Self::PicRs2
            | Self::Rsl
            | Self::SongsOfHope
            | Self::TournoiFrancoS3
            | Self::TournoiFrancoS4
            | Self::TournoiFrancoS5
            | Self::TriforceBlitzProgressionSpoiler
            | Self::WeTryToBeBetterS1
            | Self::WeTryToBeBetterS2
                => PrerollMode::Medium,
            | Self::MixedPoolsS2
            | Self::MixedPoolsS3
            | Self::MixedPoolsS4
                => PrerollMode::Long,
            Self::StandardRuleset => if let Some((Series::Standard, "8" | "8cc")) = event {
                PrerollMode::Short
            } else {
                s::WEEKLY_PREROLL_MODE //TODO allow weekly organizers to configure this
            },
        }
    }

    pub(crate) fn unlock_spoiler_log(&self, official_race: bool, spoiler_seed: bool) -> UnlockSpoilerLog {
        if spoiler_seed {
            UnlockSpoilerLog::Now
        } else {
            match self {
                | Self::Pic7
                | Self::PicRs2
                    => UnlockSpoilerLog::Now,
                | Self::TriforceBlitzProgressionSpoiler
                    => UnlockSpoilerLog::Progression,
                | Self::CopaDoBrasil
                | Self::LeagueS8
                | Self::MixedPoolsS2
                | Self::MixedPoolsS3
                | Self::MixedPoolsS4
                | Self::Mq
                | Self::MultiworldS3
                | Self::MultiworldS4
                | Self::MultiworldS5
                | Self::NineDaysOfSaws
                | Self::Rsl
                | Self::Sgl2023
                | Self::Sgl2024
                | Self::SongsOfHope
                | Self::TournoiFrancoS3
                | Self::TournoiFrancoS4
                | Self::TournoiFrancoS5
                | Self::TriforceBlitz
                | Self::WeTryToBeBetterS1
                | Self::WeTryToBeBetterS2
                    => UnlockSpoilerLog::After,
                | Self::Cc7
                | Self::CoOpS3
                | Self::StandardRuleset
                    => if official_race { UnlockSpoilerLog::Never } else { UnlockSpoilerLog::After },
                | Self::Crosskeys2025
                    => UnlockSpoilerLog::Never
            }
        }
    }

    pub(crate) fn rando_version(&self, event: Option<(Series, &str)>) -> VersionedBranch {
        match self {
            Self::Cc7 => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 1, 0) },
            Self::CoOpS3 => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 1, 0) },
            Self::CopaDoBrasil => VersionedBranch::Pinned { version: rando::Version::from_dev(7, 1, 143) },
            Self::LeagueS8 => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 2, 57) },
            Self::MixedPoolsS2 => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevFenhl, 7, 1, 117, 17) },
            Self::MixedPoolsS3 => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevFenhl, 8, 1, 76, 4) },
            Self::MixedPoolsS4 => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevFenhl, 8, 2, 76, 10) },
            Self::Mq => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 2, 0) },
            Self::MultiworldS3 => VersionedBranch::Pinned { version: rando::Version::from_dev(6, 2, 205) },
            Self::MultiworldS4 => VersionedBranch::Pinned { version: rando::Version::from_dev(7, 1, 199) },
            Self::MultiworldS5 => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 2, 76) },
            Self::NineDaysOfSaws => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevFenhl, 6, 9, 14, 2) },
            Self::Pic7 => VersionedBranch::Custom { github_username: Cow::Borrowed("fenhl"), branch: Cow::Borrowed("frogs2-melody") },
            Self::Sgl2023 => VersionedBranch::Latest { branch: rando::Branch::Sgl2023 },
            Self::Sgl2024 => VersionedBranch::Latest { branch: rando::Branch::Sgl2024 },
            Self::SongsOfHope => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 1, 0) },
            Self::StandardRuleset => if let Some((Series::Standard, "8" | "8cc")) = event {
                VersionedBranch::Pinned { version: rando::Version::from_dev(8, 2, 0) }
            } else {
                s::WEEKLY_RANDO_VERSION //TODO allow weekly organizers to configure this
            },
            Self::TournoiFrancoS3 => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevR, 7, 1, 143, 1) },
            Self::TournoiFrancoS4 => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevRob, 8, 1, 45, 105) },
            Self::TournoiFrancoS5 => VersionedBranch::Pinned { version: rando::Version::from_branch(rando::Branch::DevRob, 8, 2, 64, 135) },
            Self::TriforceBlitz => VersionedBranch::Latest { branch: rando::Branch::DevBlitz },
            Self::TriforceBlitzProgressionSpoiler => VersionedBranch::Latest { branch: rando::Branch::DevBlitz },
            Self::WeTryToBeBetterS1 => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 0, 11) },
            Self::WeTryToBeBetterS2 => VersionedBranch::Pinned { version: rando::Version::from_dev(8, 2, 0) },
            Self::Crosskeys2025 => panic!("randomizer version for this goal is unused"),
            Self::PicRs2 | Self::Rsl => panic!("randomizer version for this goal must be parsed from RSL script"),
        }
    }

    /// Only returns a value for goals that only have one possible set of settings.
    pub(crate) fn single_settings(&self) -> Option<seed::Settings> {
        match self {
            Self::Cc7 => None, // settings draft
            Self::CoOpS3 => Some(coop::s3_settings()),
            Self::CopaDoBrasil => Some(br::s1_settings()),
            Self::Crosskeys2025 => None, // per-race settings
            Self::LeagueS8 => Some(league::s8_settings()),
            Self::MixedPoolsS2 => Some(mp::s2_settings()),
            Self::MixedPoolsS3 => Some(mp::s3_settings()),
            Self::MixedPoolsS4 => Some(mp::s4_settings()),
            Self::Mq => Some(mq::s1_settings()),
            Self::MultiworldS3 => None, // settings draft
            Self::MultiworldS4 => None, // settings draft
            Self::MultiworldS5 => None, // settings draft
            Self::NineDaysOfSaws => None, // per-event settings
            Self::Pic7 => Some(pic::race7_settings()),
            Self::PicRs2 => None, // random settings
            Self::Rsl => None, // random settings
            Self::Sgl2023 => Some(sgl::settings_2023()),
            Self::Sgl2024 => Some(sgl::settings_2024()),
            Self::SongsOfHope => Some(soh::settings()),
            Self::StandardRuleset => None, // per-event settings
            Self::TournoiFrancoS3 => None, // settings draft
            Self::TournoiFrancoS4 => None, // settings draft
            Self::TournoiFrancoS5 => None, // settings draft
            Self::TriforceBlitz => None, // per-event settings
            Self::TriforceBlitzProgressionSpoiler => Some(tfb::progression_spoiler_settings()),
            Self::WeTryToBeBetterS1 => Some(wttbb::s1_settings()),
            Self::WeTryToBeBetterS2 => Some(wttbb::s2_settings()),
        }
    }

    async fn send_presets(&self, ctx: &RaceContext<GlobalState>) -> Result<(), Error> {
        match self {
            | Self::Pic7
                => ctx.say("!seed: The settings used for the race").await?,
            | Self::PicRs2
                => ctx.say("!seed: The weights used for the race").await?,
            | Self::LeagueS8
                => ctx.say("!seed: The settings used for the season").await?,
            | Self::CoOpS3
            | Self::CopaDoBrasil
            | Self::MixedPoolsS2
            | Self::MixedPoolsS3
            | Self::MixedPoolsS4
            | Self::Mq
            | Self::Sgl2023
            | Self::Sgl2024
            | Self::SongsOfHope
                => ctx.say("!seed: The settings used for the tournament").await?,
            | Self::WeTryToBeBetterS1
            | Self::WeTryToBeBetterS2
                => ctx.say("!seed : Les settings utilisés pour le tournoi").await?,
            Self::Cc7 => {
                ctx.say("!seed base: The tournament's base settings.").await?;
                ctx.say("!seed random: Simulate a settings draft with both players picking randomly. The settings are posted along with the seed.").await?;
                ctx.say("!seed draft: Pick the settings here in the chat.").await?;
                ctx.say("!seed <setting> <value> <setting> <value>... (e.g. !seed deku open camc off): Pick a set of draftable settings without doing a full draft. Use “!settings” for a list of available settings.").await?;
            }
            | Self::Crosskeys2025
                => ctx.say("!seed base: The tournament's base settings.").await?,
            Self::MultiworldS3 => {
                ctx.say("!seed base: The settings used for the qualifier and tiebreaker asyncs.").await?;
                ctx.say("!seed random: Simulate a settings draft with both teams picking randomly. The settings are posted along with the seed.").await?;
                ctx.say("!seed draft: Pick the settings here in the chat.").await?;
                ctx.say("!seed <setting> <value> <setting> <value>... (e.g. !seed trials 2 wincon scrubs): Pick a set of draftable settings without doing a full draft. Use “!settings” for a list of available settings.").await?;
            }
            Self::MultiworldS4 | Self::MultiworldS5 => {
                ctx.say("!seed base: The settings used for the qualifier and tiebreaker asyncs.").await?;
                ctx.say("!seed random: Simulate a settings draft with both teams picking randomly. The settings are posted along with the seed.").await?;
                ctx.say("!seed draft: Pick the settings here in the chat.").await?;
                ctx.say("!seed <setting> <value> <setting> <value>... (e.g. !seed trials 2 gbk stones): Pick a set of draftable settings without doing a full draft. Use “!settings” for a list of available settings.").await?;
            }
            Self::NineDaysOfSaws => {
                ctx.say("!seed day1: S6").await?;
                ctx.say("!seed day2: Beginner").await?;
                ctx.say("!seed day3: Advanced").await?;
                ctx.say("!seed day4: S5 + one bonk KO").await?;
                ctx.say("!seed day5: Beginner + mixed pools").await?;
                ctx.say("!seed day6: Beginner 3-player multiworld").await?;
                ctx.say("!seed day7: Beginner").await?;
                ctx.say("!seed day8: S6 + dungeon ER").await?;
                ctx.say("!seed day9: S6").await?;
            }
            Self::Rsl => {
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
            Self::StandardRuleset => {
                ctx.say("!seed s8: The settings for season 8 of the main tournament").await?;
                ctx.say("!seed weekly: The current weekly settings").await?;
            }
            Self::TournoiFrancoS3 => {
                ctx.say("!seed base : Settings de base.").await?;
                ctx.say("!seed random : Simule en draft en sélectionnant des settings au hasard pour les deux joueurs. Les settings seront affichés avec la seed.").await?;
                ctx.say("!seed draft : Vous fait effectuer un draft dans le chat.").await?;
                ctx.say("!seed <setting> <configuration> <setting> <configuration>... ex : !seed trials random bridge ad : Créé une seed avec les settings que vous définissez. Tapez “!settings” pour obtenir la liste des settings.").await?;
                ctx.say("Utilisez “!seed random advanced” ou “!seed draft advanced” pour autoriser les settings difficiles.").await?;
                ctx.say("Activez les donjons Master Quest en utilisant par exemple : “!seed base 6mq” ou “!seed draft advanced 12mq”").await?;
            }
            Self::TournoiFrancoS4 | Self::TournoiFrancoS5 => {
                ctx.say("!seed base: The tournament's base settings / Settings de base.").await?;
                ctx.say("!seed random: Simulate a settings draft with both players picking randomly. The settings are posted along with the seed. / Simule en draft en sélectionnant des settings au hasard pour les deux joueurs. Les settings seront affichés avec la seed.").await?;
                ctx.say("!seed draft: Pick the settings here in the chat. / Vous fait effectuer un draft dans le chat.").await?;
                ctx.say("!seed <setting> <value> <setting> <value>... (e.g. !seed trials random bridge ad): Pick a set of draftable settings without doing a full draft. Use “!settings” for a list of available settings. / Créé une seed avec les settings que vous définissez. Tapez “!settings” pour obtenir la liste des settings.").await?;
                ctx.say("Use “!seed random advanced” or “!seed draft advanced” to allow advanced settings. / Utilisez “!seed random advanced” ou “!seed draft advanced” pour autoriser les settings difficiles.").await?;
                ctx.say("Enable Master Quest using e.g. “!seed base 6mq” or “!seed draft advanced 12mq” / Activez les donjons Master Quest en utilisant par exemple : “!seed base 6mq” ou “!seed draft advanced 12mq”").await?;
            }
            Self::TriforceBlitz => {
                ctx.say("!seed s4coop: Triforce Blitz season 4 co-op settings").await?;
                ctx.say("!seed s3: Triforce Blitz season 3 settings").await?;
                ctx.say("!seed jr: Jabu's Revenge").await?;
                ctx.say("!seed s2: Triforce Blitz season 2 settings").await?;
                ctx.say("!seed daily: Triforce Blitz Seed of the Day").await?;
            }
            Self::TriforceBlitzProgressionSpoiler => ctx.say("!seed: The current settings for the mode").await?,
        }
        Ok(())
    }

    fn parse_draft_command(&self, cmd: &str, args: &[String]) -> DraftCommandParseResult {
        match (*self == Self::Rsl, cmd) {
            (false, "ban") | (true, "block") => match args[..] {
                [] => DraftCommandParseResult::SendSettings {
                    language: self.language(),
                    msg: Cow::Borrowed(if let French = self.language() {
                        "un setting doit être choisi. Utilisez un des suivants :"
                    } else {
                        "the setting is required. Use one of the following:"
                    }),
                },
                [ref setting] => DraftCommandParseResult::Action(draft::Action::Ban { setting: setting.clone() }),
                [..] => DraftCommandParseResult::Error {
                    language: self.language(),
                    msg: Cow::Borrowed(if let French = self.language() {
                        "seul un setting peut être ban à la fois. Veuillez seulement utiliser “!ban <setting>”"
                    } else {
                        "only one setting can be banned at a time. Use “!ban <setting>”"
                    }),
                },
            },
            (false, "draft" | "pick") | (true, "ban") => match args[..] {
                [] => DraftCommandParseResult::SendSettings {
                    language: self.language(),
                    msg: Cow::Borrowed(if let French = self.language() {
                        "un setting doit être choisi. Utilisez un des suivants :"
                    } else {
                        "the setting is required. Use one of the following:"
                    })
                },
                [_] => DraftCommandParseResult::Error {
                    language: self.language(),
                    msg: Cow::Borrowed(if let French = self.language() {
                        "une configuration est requise."
                    } else {
                        "the value is required."
                    }), //TODO list available values
                },
                [ref setting, ref value] => DraftCommandParseResult::Action(draft::Action::Pick { setting: setting.clone(), value: value.clone() }),
                [..] => DraftCommandParseResult::Error {
                    language: self.language(),
                    msg: Cow::Borrowed(if let French = self.language() {
                        "vous ne pouvez pick qu'un setting à la fois. Veuillez seulement utiliser “!draft <setting> <configuration>”"
                    } else {
                        "only one setting can be drafted at a time. Use “!draft <setting> <value>”"
                    }),
                },
            },
            (_, "first") => DraftCommandParseResult::Action(draft::Action::GoFirst(true)),
            (_, "no") => DraftCommandParseResult::Action(draft::Action::BooleanChoice(false)),
            (_, "second") => DraftCommandParseResult::Action(draft::Action::GoFirst(false)),
            (_, "skip") => DraftCommandParseResult::Action(draft::Action::Skip),
            (_, "yes") => DraftCommandParseResult::Action(draft::Action::BooleanChoice(true)),
            (_, cmd) => DraftCommandParseResult::Error { language: English, msg: Cow::Owned(format!("Unexpected draft command: {cmd}")) },
        }
    }

    pub(crate) async fn parse_seed_command(&self, transaction: &mut Transaction<'_, Postgres>, global_state: &GlobalState, is_official: bool, spoiler_seed: bool, args: &[String]) -> Result<SeedCommandParseResult, Error> {
        let unlock_spoiler_log = self.unlock_spoiler_log(is_official, spoiler_seed);
        Ok(match self {
            | Self::CoOpS3
            | Self::CopaDoBrasil
            | Self::LeagueS8
            | Self::MixedPoolsS2
            | Self::MixedPoolsS3
            | Self::MixedPoolsS4
            | Self::Mq
            | Self::Pic7
            | Self::Sgl2023
            | Self::Sgl2024
            | Self::SongsOfHope
            | Self::TriforceBlitzProgressionSpoiler
            | Self::WeTryToBeBetterS1
            | Self::WeTryToBeBetterS2
                => {
                    let (article, description) = match self.language() {
                        French => ("une", format!("seed")),
                        _ => ("a", format!("seed")),
                    };
                    if let Some(row) = sqlx::query!(r#"DELETE FROM prerolled_seeds WHERE ctid IN (SELECT ctid FROM prerolled_seeds WHERE goal_name = $1 ORDER BY timestamp ASC NULLS FIRST LIMIT 1) RETURNING
                        goal_name,
                        file_stem,
                        locked_spoiler_log_path,
                        hash1 AS "hash1: HashIcon",
                        hash2 AS "hash2: HashIcon",
                        hash3 AS "hash3: HashIcon",
                        hash4 AS "hash4: HashIcon",
                        hash5 AS "hash5: HashIcon",
                        seed_password,
                        progression_spoiler
                    "#, self.as_str()).fetch_optional(&mut **transaction).await.to_racetime()? {
                        let _ = global_state.seed_cache_tx.send(());
                        SeedCommandParseResult::QueueExisting {
                            data: seed::Data::from_db(
                                None,
                                None,
                                None,
                                None,
                                row.file_stem,
                                row.locked_spoiler_log_path,
                                None,
                                None,
                                false,
                                None,
                                row.hash1,
                                row.hash2,
                                row.hash3,
                                row.hash4,
                                row.hash5,
                                row.seed_password.as_deref(),
                                row.progression_spoiler,
                            ),
                            language: self.language(),
                            article, description,
                        }
                    } else {
                        SeedCommandParseResult::Regular { settings: self.single_settings().expect("goal has no single settings"), unlock_spoiler_log, language: self.language(), article, description }
                    }
                }
            Self::Cc7 => {
                let settings = match args {
                    [] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "the preset is required" }),
                    [arg] if arg == "base" => HashMap::default(),
                    [arg] if arg == "random" => Draft {
                        high_seed: Id::dummy(), // Draft::complete_randomly doesn't check for active team
                        went_first: None,
                        skipped_bans: 0,
                        settings: HashMap::default(),
                    }.complete_randomly(draft::Kind::S7).await.to_racetime()?,
                    [arg] if arg == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(), // racetime.gg bot doesn't check for active team
                            went_first: None,
                            skipped_bans: 0,
                            settings: HashMap::default(),
                        },
                        unlock_spoiler_log,
                    }),
                    [arg] if s::S7_SETTINGS.into_iter().any(|s::Setting { name, .. }| name == arg) => {
                        return Ok(SeedCommandParseResult::SendSettings { language: English, msg: "you need to pair each setting with a value.".into() })
                    }
                    [_] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don't recognize that preset" }),
                    args => {
                        let args = args.iter().map(|arg| arg.to_owned()).collect_vec();
                        let mut settings = HashMap::default();
                        let mut tuples = args.into_iter().tuples();
                        for (setting, value) in &mut tuples {
                            if let Some(s::Setting { other, .. }) = s::S7_SETTINGS.into_iter().find(|s::Setting { name, .. }| **name == setting) {
                                if value == "default" || other.iter().any(|(other, _, _)| value == **other) {
                                    settings.insert(Cow::Owned(setting), Cow::Owned(value));
                                } else {
                                    return Ok(SeedCommandParseResult::Error { language: English, msg: format!("I don't recognize that value for the {setting} setting. Use {}", iter::once("default").chain(other.iter().map(|&(other, _, _)| other)).join(" or ")).into() })
                                }
                            } else {
                                return Ok(SeedCommandParseResult::Error { language: English, msg: format!(
                                    "I don't recognize {}. Use one of the following:",
                                    if setting.chars().all(|c| c.is_ascii_alphanumeric()) { Cow::Owned(format!("the setting “{setting}”")) } else { Cow::Borrowed("one of those settings") },
                                ).into() })
                            }
                        }
                        if tuples.into_buffer().next().is_some() {
                            return Ok(SeedCommandParseResult::SendSettings { language: English, msg: "you need to pair each setting with a value.".into() })
                        } else {
                            settings
                        }
                    }
                };
                SeedCommandParseResult::Regular { settings: s::resolve_s7_draft_settings(&settings), unlock_spoiler_log, language: English, article: "a", description: format!("seed with {}", s::display_s7_draft_picks(&settings)) }
            }
            Self::Crosskeys2025 => match args {
                [] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "the preset is required" }),
                [arg] if arg == "base" => SeedCommandParseResult::Alttpr,
                [_] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don't recognize that preset" }),
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn't quite understand that" },
            },
            Self::MultiworldS3 | Self::MultiworldS4 | Self::MultiworldS5 => {
                let available_settings = match self {
                    Self::MultiworldS3 => mw::S3_SETTINGS,
                    Self::MultiworldS4 => mw::S4_SETTINGS,
                    Self::MultiworldS5 => mw::S5_SETTINGS,
                    _ => unreachable!("checked in outer match"),
                };
                let settings = match args {
                    [] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "the preset is required" }),
                    [arg] if arg == "base" => HashMap::default(),
                    [arg] if arg == "random" => Draft {
                        high_seed: Id::dummy(), // Draft::complete_randomly doesn't check for active team
                        went_first: None,
                        skipped_bans: 0,
                        settings: HashMap::default(),
                    }.complete_randomly(self.draft_kind().expect("multiworld tournament goal should have a draft kind")).await.to_racetime()?,
                    [arg] if arg == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(), // racetime.gg bot doesn't check for active team
                            went_first: None,
                            skipped_bans: 0,
                            settings: HashMap::default(),
                        },
                        unlock_spoiler_log,
                    }),
                    [arg] if available_settings.iter().copied().any(|mw::Setting { name, .. }| name == arg) => {
                        return Ok(SeedCommandParseResult::SendSettings { language: English, msg: "you need to pair each setting with a value.".into() })
                    }
                    [_] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don't recognize that preset" }),
                    args => {
                        let args = args.iter().map(|arg| arg.to_owned()).collect_vec();
                        let mut settings = HashMap::default();
                        let mut tuples = args.into_iter().tuples();
                        for (setting, value) in &mut tuples {
                            if let Some(mw::Setting { default, other, .. }) = available_settings.iter().copied().find(|mw::Setting { name, .. }| **name == setting) {
                                if value == default || other.iter().any(|(other, _)| value == **other) {
                                    settings.insert(Cow::Owned(setting), Cow::Owned(value));
                                } else {
                                    return Ok(SeedCommandParseResult::Error { language: English, msg: format!("I don't recognize that value for the {setting} setting. Use {}", iter::once(default).chain(other.iter().map(|&(other, _)| other)).join(" or ")).into() })
                                }
                            } else {
                                return Ok(SeedCommandParseResult::Error { language: English, msg: format!(
                                    "I don't recognize {}. Use one of the following:",
                                    if setting.chars().all(|c| c.is_ascii_alphanumeric()) { Cow::Owned(format!("the setting “{setting}”")) } else { Cow::Borrowed("one of those settings") },
                                ).into() })
                            }
                        }
                        if tuples.into_buffer().next().is_some() {
                            return Ok(SeedCommandParseResult::SendSettings { language: English, msg: "you need to pair each setting with a value.".into() })
                        } else {
                            settings
                        }
                    }
                };
                let (settings, display) = match self {
                    Self::MultiworldS3 => (mw::resolve_s3_draft_settings(&settings), mw::display_s3_draft_picks(&settings)),
                    Self::MultiworldS4 => (mw::resolve_s4_draft_settings(&settings), mw::display_s4_draft_picks(&settings)),
                    Self::MultiworldS5 => (mw::resolve_s5_draft_settings(&settings), mw::display_s5_draft_picks(&settings)),
                    _ => unreachable!("checked in outer match"),
                };
                SeedCommandParseResult::Regular { settings, unlock_spoiler_log, language: English, article: "a", description: format!("seed with {display}") }
            }
            Self::NineDaysOfSaws => match args {
                [] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "the preset is required" }),
                [arg] => if let Some((description, mut settings)) = match &**arg {
                    "day1" | "day9" => Some(("SAWS (S6)", ndos::s6_preset())),
                    "day2" | "day7" => Some(("SAWS (Beginner)", ndos::beginner_preset())),
                    "day3" => Some(("SAWS (Advanced)", ndos::advanced_preset())),
                    "day4" => Some(("SAWS (S5) + one bonk KO", {
                        let mut settings = ndos::s6_preset();
                        settings.insert(format!("dungeon_shortcuts_choice"), json!("off"));
                        settings.insert(format!("shuffle_child_spawn"), json!("balanced"));
                        settings.insert(format!("fix_broken_drops"), json!(false));
                        settings.insert(format!("item_pool_value"), json!("minimal"));
                        settings.insert(format!("blue_fire_arrows"), json!(false));
                        settings.insert(format!("junk_ice_traps"), json!("off"));
                        settings.insert(format!("deadly_bonks"), json!("ohko"));
                        settings.insert(format!("hint_dist_user"), json!({
                            "name":                  "tournament",
                            "gui_name":              "Tournament",
                            "description":           "Hint Distribution for the S5 Tournament. 5 Goal Hints, 3 Barren Hints, 5 Sometimes hints, 7 Always hints (including skull mask).",
                            "add_locations":         [
                                { "location": "Deku Theater Skull Mask", "types": ["always"] },
                            ],
                            "remove_locations":      [
                                {"location": "Ganons Castle Shadow Trial Golden Gauntlets Chest", "types": ["sometimes"] },
                            ],
                            "add_items":             [],
                            "remove_items":          [
                                { "item": "Zeldas Lullaby", "types": ["goal"] },
                            ],
                            "dungeons_woth_limit":   2,
                            "dungeons_barren_limit": 1,
                            "named_items_required":  true,
                            "vague_named_items":     false,
                            "use_default_goals":     true,
                            "distribution":          {
                                "trial":           {"order": 1, "weight": 0.0, "fixed":   0, "copies": 2},
                                "entrance_always": {"order": 2, "weight": 0.0, "fixed":   0, "copies": 2},
                                "always":          {"order": 3, "weight": 0.0, "fixed":   0, "copies": 2},
                                "goal":            {"order": 4, "weight": 0.0, "fixed":   5, "copies": 2},
                                "barren":          {"order": 5, "weight": 0.0, "fixed":   3, "copies": 2},
                                "entrance":        {"order": 6, "weight": 0.0, "fixed":   4, "copies": 2},
                                "sometimes":       {"order": 7, "weight": 0.0, "fixed": 100, "copies": 2},
                                "random":          {"order": 8, "weight": 9.0, "fixed":   0, "copies": 2},
                                "named-item":      {"order": 9, "weight": 0.0, "fixed":   0, "copies": 2},
                                "item":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                                "song":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                                "overworld":       {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                                "dungeon":         {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                                "junk":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                                "woth":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                                "dual_always":     {"order": 0, "weight": 0.0, "fixed":   0, "copies": 0},
                                "dual":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 0},
                            },
                        }));
                        settings
                    })),
                    "day5" => Some(("SAWS (Beginner) + mixed pools", {
                        let mut settings = ndos::beginner_preset();
                        settings.insert(format!("shuffle_interior_entrances"), json!("all"));
                        settings.insert(format!("shuffle_grotto_entrances"), json!(true));
                        settings.insert(format!("shuffle_dungeon_entrances"), json!("all"));
                        settings.insert(format!("shuffle_overworld_entrances"), json!(true));
                        settings.insert(format!("mix_entrance_pools"), json!([
                            "Interior",
                            "GrottoGrave",
                            "Dungeon",
                            "Overworld",
                        ]));
                        settings.insert(format!("shuffle_child_spawn"), json!("full"));
                        settings.insert(format!("shuffle_adult_spawn"), json!("full"));
                        settings.insert(format!("shuffle_gerudo_valley_river_exit"), json!("full"));
                        settings.insert(format!("owl_drops"), json!("full"));
                        settings.insert(format!("warp_songs"), json!("full"));
                        settings.insert(format!("blue_warps"), json!("dungeon"));
                        settings
                    })),
                    "day6" => Some(("SAWS (Beginner) 3-player multiworld", {
                        let mut settings = ndos::beginner_preset();
                        settings.insert(format!("world_count"), json!(3));
                        settings
                    })),
                    "day8" => Some(("SAWS (S6) + dungeon ER", {
                        let mut settings = ndos::s6_preset();
                        settings.insert(format!("shuffle_dungeon_entrances"), json!("simple"));
                        settings.insert(format!("blue_warps"), json!("dungeon"));
                        settings
                    })),
                    _ => None,
                } {
                    settings.insert(format!("user_message"), json!(format!("9 Days of SAWS: day {}", &arg[3..])));
                    SeedCommandParseResult::Regular { settings, unlock_spoiler_log, language: English, article: "a", description: format!("{description} seed") }
                } else {
                    SeedCommandParseResult::SendPresets { language: English, msg: "I don't recognize that preset" }
                },
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn't quite understand that" },
            }
            Self::PicRs2 => SeedCommandParseResult::Rsl { preset: rsl::VersionedPreset::Fenhl {
                version: Some((Version::new(2, 3, 8), 10)),
                preset: rsl::DevFenhlPreset::Pictionary,
            }, world_count: 1, unlock_spoiler_log, language: English, article: "a", description: format!("seed") },
            Self::Rsl => {
                let (preset, world_count) = match args {
                    [] => (rsl::Preset::League, 1),
                    [preset] if preset == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(), // racetime.gg bot doesn't check for active team
                            went_first: None,
                            skipped_bans: 0,
                            settings: HashMap::default(),
                        },
                        unlock_spoiler_log,
                    }),
                    [preset] => if let Ok(preset) = preset.parse() {
                        if let rsl::Preset::Multiworld = preset {
                            return Ok(SeedCommandParseResult::Error { language: English, msg: "Missing world count (e.g. “!seed multiworld 2” for 2 worlds)".into() })
                        } else {
                            (preset, 1)
                        }
                    } else {
                        return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don't recognize that preset" })
                    },
                    [preset, lite] if preset == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(), // racetime.gg bot doesn't check for active team
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
                            return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I didn't quite understand that" })
                        }
                    } else {
                        return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I don't recognize that preset" })
                    },
                    [..] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "I didn't quite understand that" }),
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
            Self::StandardRuleset => match args {
                [] => return Ok(SeedCommandParseResult::SendPresets { language: English, msg: "the preset is required" }),
                [arg] if arg == "s8" => SeedCommandParseResult::Regular { settings: s::s8_settings(), unlock_spoiler_log, language: English, article: "an", description: format!("S8 seed") },
                [arg] if arg == "weekly" => {
                    let mut settings = s::weekly_settings();
                    settings.insert(format!("password_lock"), json!(true));
                    SeedCommandParseResult::Regular { settings, unlock_spoiler_log, language: English, article: "a", description: format!("weekly seed") }
                }
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn't quite understand that" },
            },
            Self::TournoiFrancoS3 | Self::TournoiFrancoS4 | Self::TournoiFrancoS5 => {
                let all_settings = match self {
                    Self::TournoiFrancoS3 => &fr::S3_SETTINGS[..],
                    Self::TournoiFrancoS4 => &fr::S4_SETTINGS[..],
                    Self::TournoiFrancoS5 => &fr::S5_SETTINGS[..],
                    _ => unreachable!(),
                };
                let mut args = args.to_owned();
                let mut mq_dungeons_count = None::<u8>;
                let mut hard_settings_ok = false;
                args.retain(|arg| if arg == "advanced" && !hard_settings_ok {
                    hard_settings_ok = true;
                    false
                } else if let (None, Some(mq)) = (mq_dungeons_count, regex_captures!("^([0-9]+)mq$"i, arg).and_then(|(_, mq)| mq.parse().ok())) {
                    mq_dungeons_count = Some(mq);
                    false
                } else {
                    true
                });
                let settings = match &*args {
                    [] => return Ok(SeedCommandParseResult::SendPresets { language: French, msg: "un preset doit être défini" }),
                    [arg] if arg == "base" => HashMap::default(),
                    [arg] if arg == "random" => Draft {
                        high_seed: Id::dummy(), // Draft::complete_randomly doesn't check for active team
                        went_first: None,
                        skipped_bans: 0,
                        settings: collect![as HashMap<_, _>:
                            Cow::Borrowed("hard_settings_ok") => Cow::Borrowed(if hard_settings_ok { "ok" } else { "no" }),
                            Cow::Borrowed("mq_ok") => Cow::Borrowed(if mq_dungeons_count.is_some() { "ok" } else { "no" }),
                            Cow::Borrowed("mq_dungeons_count") => Cow::Owned(mq_dungeons_count.unwrap_or_default().to_string()),
                        ],
                    }.complete_randomly(self.draft_kind().unwrap()).await.to_racetime()?,
                    [arg] if arg == "draft" => return Ok(SeedCommandParseResult::StartDraft {
                        new_state: Draft {
                            high_seed: Id::dummy(), // racetime.gg bot doesn't check for active team
                            went_first: None,
                            skipped_bans: 0,
                            settings: collect![as HashMap<_, _>:
                                Cow::Borrowed("hard_settings_ok") => Cow::Borrowed(if hard_settings_ok { "ok" } else { "no" }),
                                Cow::Borrowed("mq_ok") => Cow::Borrowed(if mq_dungeons_count.is_some() { "ok" } else { "no" }),
                                Cow::Borrowed("mq_dungeons_count") => Cow::Owned(mq_dungeons_count.unwrap_or_default().to_string()),
                            ],
                        },
                        unlock_spoiler_log,
                    }),
                    [arg] if all_settings.iter().any(|&fr::Setting { name, .. }| name == arg) => return Ok(SeedCommandParseResult::SendSettings { language: French, msg: "vous devez associer un setting avec une configuration.".into() }),
                    [_] => return Ok(SeedCommandParseResult::SendPresets { language: French, msg: "je ne reconnais pas ce preset" }),
                    args => {
                        let args = args.iter().map(|arg| arg.to_owned()).collect_vec();
                        let mut settings = HashMap::default();
                        let mut tuples = args.into_iter().tuples();
                        for (setting, value) in &mut tuples {
                            if let Some(&fr::Setting { default, other, .. }) = all_settings.iter().find(|&fr::Setting { name, .. }| **name == setting) {
                                if setting == "dungeon-er" && value == "mixed" {
                                    settings.insert(Cow::Borrowed("dungeon-er"), Cow::Borrowed("on"));
                                    settings.insert(Cow::Borrowed("mixed-dungeons"), Cow::Borrowed("mixed"));
                                } else if value == default || other.iter().any(|(other, _, _)| value == **other) {
                                    settings.insert(Cow::Owned(setting), Cow::Owned(value));
                                } else {
                                    return Ok(SeedCommandParseResult::Error { language: French, msg: format!("je ne reconnais pas cette configuration pour {setting}. Utilisez {}", iter::once(default).chain(other.iter().map(|&(other, _, _)| other)).join(" ou ")).into() })
                                }
                            } else {
                                return Ok(SeedCommandParseResult::SendSettings { language: French, msg: format!("je ne reconnais pas {}. Utilisez cette liste :",
                                    if setting.chars().all(|c| c.is_ascii_alphanumeric()) { Cow::Owned(format!("le setting « {setting} »")) } else { Cow::Borrowed("un des settings") },
                                ).into() })
                            }
                        }
                        if tuples.into_buffer().next().is_some() {
                            return Ok(SeedCommandParseResult::SendSettings { language: French, msg: "vous devez associer un setting avec une configuration.".into() })
                        } else {
                            settings.insert(Cow::Borrowed("mq_dungeons_count"), Cow::Owned(mq_dungeons_count.unwrap_or_default().to_string()));
                            settings
                        }
                    }
                };
                SeedCommandParseResult::Regular {
                    settings: match self {
                        Self::TournoiFrancoS3 => fr::resolve_s3_draft_settings(&settings),
                        Self::TournoiFrancoS4 => fr::resolve_s4_draft_settings(&settings),
                        Self::TournoiFrancoS5 => fr::resolve_s5_draft_settings(&settings),
                        _ => unreachable!(),
                    },
                    unlock_spoiler_log,
                    language: self.language(),
                    article: if let French = self.language() { "une" } else { "a" },
                    description: format!("seed {} {}", if let French = self.language() { "avec" } else { "with" }, fr::display_draft_picks(self.language(), all_settings, &settings)),
                }
            }
            Self::TriforceBlitz => match args {
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
                    SeedCommandParseResult::QueueExisting { data: seed::Data {
                        file_hash: Some(file_hash),
                        password: None,
                        files: Some(seed::Files::TfbSotd { date, ordinal }),
                        progression_spoiler: false,
                    }, language: English, article: "the", description: format!("Triforce Blitz seed of the day") }
                }
                [arg] if arg == "jr" => SeedCommandParseResult::Tfb { version: "v7.1.143-blitz-0.43", unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz: Jabu's Revenge seed") },
                [arg] if arg == "s2" => SeedCommandParseResult::Tfb { version: "v7.1.3-blitz-0.42", unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz S2 seed") },
                [arg] if arg == "s3" => SeedCommandParseResult::Tfb { version: "LATEST", unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz S3 seed") },
                [arg] if arg == "s4coop" => SeedCommandParseResult::TfbDev { coop: true, unlock_spoiler_log, language: English, article: "a", description: format!("Triforce Blitz S4 co-op seed") },
                [..] => SeedCommandParseResult::SendPresets { language: English, msg: "I didn't quite understand that" },
            },
        })
    }
}

enum DraftCommandParseResult {
    Action(draft::Action),
    SendSettings {
        language: Language,
        msg: Cow<'static, str>,
    },
    Error {
        language: Language,
        msg: Cow<'static, str>,
    },
}

pub(crate) enum SeedCommandParseResult {
    Alttpr,
    Regular {
        settings: seed::Settings,
        unlock_spoiler_log: UnlockSpoilerLog,
        language: Language,
        article: &'static str,
        description: String,
    },
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
    SendSettings {
        language: Language,
        msg: Cow<'static, str>,
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

impl FromStr for Goal {
    type Err = GoalFromStrError;

    fn from_str(s: &str) -> Result<Self, GoalFromStrError> {
        all::<Self>().find(|goal| goal.as_str() == s).ok_or(GoalFromStrError)
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

pub(crate) struct GlobalState {
    /// Locked while event rooms are being created. Wait with handling new rooms while it's held.
    new_room_lock: Arc<Mutex<()>>,
    host_info: racetime::HostInfo,
    racetime_config: ConfigRaceTime,
    extra_room_tx: Arc<RwLock<mpsc::Sender<String>>>,
    pub(crate) db_pool: PgPool,
    pub(crate) http_client: reqwest::Client,
    insecure_http_client: reqwest::Client,
    league_api_key: String,
    startgg_token: String,
    ootr_api_client: Arc<ootr_web::ApiClient>,
    pub(crate) discord_ctx: RwFuture<DiscordCtx>,
    clean_shutdown: Arc<Mutex<CleanShutdown>>,
    seed_cache_tx: watch::Sender<()>,
    seed_metadata: Arc<RwLock<HashMap<String, SeedMetadata>>>,
}

impl GlobalState {
    pub(crate) async fn new(
        new_room_lock: Arc<Mutex<()>>,
        racetime_config: ConfigRaceTime,
        extra_room_tx: Arc<RwLock<mpsc::Sender<String>>>,
        db_pool: PgPool,
        http_client: reqwest::Client,
        insecure_http_client: reqwest::Client,
        league_api_key: String,
        startgg_token: String,
        ootr_api_client: Arc<ootr_web::ApiClient>,
        discord_ctx: RwFuture<DiscordCtx>,
        clean_shutdown: Arc<Mutex<CleanShutdown>>,
        seed_cache_tx: watch::Sender<()>,
        seed_metadata: Arc<RwLock<HashMap<String, SeedMetadata>>>,
    ) -> Self {
        Self {
            host_info: racetime::HostInfo {
                hostname: Cow::Borrowed(racetime_host()),
                ..racetime::HostInfo::default()
            },
            new_room_lock, racetime_config, extra_room_tx, db_pool, http_client, insecure_http_client, league_api_key, startgg_token, ootr_api_client, discord_ctx, clean_shutdown, seed_cache_tx, seed_metadata,
        }
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
                                files: Some(seed::Files::OotrWeb {
                                    file_stem: Cow::Owned(file_stem),
                                    id, gen_time,
                                }),
                                progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                                password,
                            },
                            rsl_preset: None,
                            unlock_spoiler_log,
                        }).await?,
                        Err(e) => update_tx.send(SeedRollUpdate::Error(e.into())).await?, //TODO fall back to rolling locally for network errors
                    }
                } else {
                    update_tx.send(SeedRollUpdate::Started).await?;
                    match roll_seed_locally(delay_until, version, match unlock_spoiler_log {
                        UnlockSpoilerLog::Now | UnlockSpoilerLog::Progression | UnlockSpoilerLog::After => true,
                        UnlockSpoilerLog::Never => password_lock, // spoiler log needs to be generated so the backend can read the password
                    }, settings).await {
                        Ok((patch_filename, spoiler_log_path)) => update_tx.send(match spoiler_log_path.map(|spoiler_log_path| spoiler_log_path.into_os_string().into_string()).transpose() {
                            Ok(locked_spoiler_log_path) => match regex_captures!(r"^(.+)\.zpfz?$", &patch_filename) {
                                Some((_, file_stem)) => SeedRollUpdate::Done {
                                    seed: seed::Data {
                                        file_hash: None, password: None, // will be read from spoiler log
                                        files: Some(seed::Files::MidosHouse {
                                            file_stem: Cow::Owned(file_stem.to_owned()),
                                            locked_spoiler_log_path,
                                        }),
                                        progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                                    },
                                    rsl_preset: None,
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

    pub(crate) fn roll_crosskeys2025_seed(self: Arc<Self>, ctx: RaceContext<GlobalState>, _delay_until: Option<DateTime<Utc>>, cal_event: cal::Event) -> mpsc::Receiver<SeedRollUpdate> {
        let (update_tx, update_rx) = mpsc::channel(128);
        let update_tx2 = update_tx.clone();
        tokio::spawn(async move {
            let teams = cal_event.race.teams();
            let team_rows = sqlx::query!("SELECT all_dungeons_ok, flute_ok, inverted_ok, keydrop_ok, mirror_scroll_ok, pb_ok, zw_ok FROM teams WHERE id = ANY($1)", teams.map(|team| team.id).collect_vec() as _).fetch_all(&ctx.global_state.db_pool).await?;

            let crosskeys_options = CrosskeysRaceOptions {
                all_dungeons_ok: team_rows.iter().all(|row| row.all_dungeons_ok),
                flute_ok: team_rows.iter().all(|row| row.flute_ok),
                inverted_ok: team_rows.iter().all(|row| row.inverted_ok),
                keydrop_ok: team_rows.iter().all(|row| row.keydrop_ok),
                mirror_scroll_ok: team_rows.iter().all(|row| row.mirror_scroll_ok),
                pb_ok: team_rows.iter().all(|row| row.pb_ok),
                zw_ok: team_rows.iter().all(|row| row.zw_ok),
            };
            let crosskeys_meta = CrosskeysMeta {
                bps: true,
                name: cal_event.race.id.to_string(),
                race: true,
                skip_playthrough: true,
                spoiler: "none",
                suppress_rom: true,
            };
            let mut crosskeys_yaml = CrosskeysYaml {
                placements: HashMap::default(),
                settings: HashMap::default(),
                start_inventory: HashMap::default(),
                meta: crosskeys_meta,
            };

            let keydrop_mode = if crosskeys_options.keydrop_ok { "keys" } else { "none" };
            let pottery_mode = if crosskeys_options.keydrop_ok { "keys" } else { "none" };
            let flute_mode = if crosskeys_options.flute_ok { "active" } else { "normal" };
            let goal = if crosskeys_options.all_dungeons_ok { "dungeons" } else { "crystals" };
            let mirrorscroll = if crosskeys_options.mirror_scroll_ok { 1 } else { 0 };
            let world_state = if crosskeys_options.inverted_ok { "inverted" } else { "open" };
            let pseudoboots = if crosskeys_options.pb_ok { 1 } else { 0 };
            let skullwoods = if crosskeys_options.zw_ok { "followlinked" } else { "original" };

            let crosskeys_settings = CrosskeysSetting {
                accessibility: "locations",
                bigkeyshuffle: 1,
                compassshuffle: 1,
                crystals_ganon: "7",
                crystals_gt: "7",
                dropshuffle: keydrop_mode,
                flute_mode: flute_mode,
                goal: goal,
                item_functionality: "normal",
                key_logic_algorithm: "partial",
                keyshuffle: "wild",
                linked_drops: "unset",
                mapshuffle: 1,
                mirrorscroll: mirrorscroll,
                mode: world_state,
                pottery: pottery_mode,
                pseudoboots: pseudoboots,
                shuffle: "crossed",
                shuffletavern: 0,
                skullwoods: skullwoods,
            };

            if crosskeys_options.zw_ok {
                let crosskeys_placements = CrosskeysPlacements {
                    pinball_room: "Small Key (Skull Woods)",
                };
                crosskeys_yaml.placements.insert(1, crosskeys_placements);
            }

            if crosskeys_options.flute_ok {
                let starting_flute = &["Ocarina (Activated)"];
                crosskeys_yaml.start_inventory.insert(1, starting_flute);
            }

            crosskeys_yaml.settings.insert(1, crosskeys_settings);
            let yaml_file = tempfile::Builder::new().prefix("alttpr_").suffix(".yml").tempfile().at_unknown()?;
            let yaml_path = yaml_file.path();
            tokio::fs::File::from_std(yaml_file.reopen().at(&yaml_file)?).write_all(serde_yml::to_string(&crosskeys_yaml)?.as_bytes()).await.at(&yaml_file)?;
            Command::new(PYTHON).arg("../alttpr/DungeonRandomizer.py").arg("--customizer").arg(yaml_path).arg("--outputpath").arg("/var/www/midos.house/seed").check("DungeonRandomizer.py").await?;
            update_tx.send(SeedRollUpdate::Done {
                seed: seed::Data {
                    file_hash: None,
                    files: Some(seed::Files::AlttprDoorRando {
                        race_id: cal_event.race.id
                    }),
                    progression_spoiler: false,
                    password: None,
                },
                rsl_preset: None,
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
                            files: Some(seed::Files::OotrWeb {
                                file_stem: Cow::Owned(file_stem),
                                id, gen_time,
                            }),
                            progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                            password,
                        },
                        rsl_preset: if let rsl::VersionedPreset::Xopar { preset, .. } = preset { Some(preset) } else { None },
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
                                files: Some(seed::Files::MidosHouse {
                                    file_stem: Cow::Owned(file_stem.to_owned()),
                                    locked_spoiler_log_path: Some(spoiler_log_path.into_os_string().into_string()?),
                                }),
                                progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                            },
                            rsl_preset: if let rsl::VersionedPreset::Xopar { preset, .. } = preset { Some(preset) } else { None },
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
                    files: Some(seed::Files::TriforceBlitz { is_dev, uuid }),
                    progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                },
                rsl_preset: None,
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
                    files: Some(seed::Files::TriforceBlitz { is_dev, uuid }),
                    progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                },
                rsl_preset: None,
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

async fn roll_seed_locally(delay_until: Option<DateTime<Utc>>, version: VersionedBranch, unlock_spoiler_log: bool, mut settings: seed::Settings) -> Result<(String, Option<PathBuf>), RollError> {
    let allow_riir = false; //TODO debug broken patch files generated by riir branch, then reenable
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
                //TODO use git2 or gix instead?
                Command::new("git").arg("pull").current_dir(&dir).check("git").await?;
            } else {
                fs::create_dir_all(&parent).await?;
                let mut command = Command::new("git"); //TODO use git2 or gix instead? (git2 doesn't support shallow clones, gix is very low level)
                command.arg("clone");
                command.arg(format!("https://github.com/{github_username}/OoT-Randomizer.git"));
                command.arg(format!("--branch={branch}"));
                command.arg(&*branch);
                command.current_dir(parent);
                command.check("git").await?;
            }
            dir
        }
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
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] OotrWeb(#[from] ootr_web::Error),
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[cfg(unix)] #[error(transparent)] RaceTime(#[from] Error),
    #[error(transparent)] RandoVersion(#[from] rando::VersionParseError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] RslScriptPath(#[from] rsl::ScriptPathError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[cfg(unix)] #[error(transparent)] Xdg(#[from] xdg::BaseDirectoriesError),
    #[error(transparent)] Yml(#[from] serde_yml::Error),
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
    #[cfg(windows)]
    #[error("failed to access user directories")]
    UserDirs,
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
            Self::Done { mut seed, rsl_preset, unlock_spoiler_log } => {
                if let Some(seed::Files::MidosHouse { ref file_stem, ref mut locked_spoiler_log_path }) = seed.files {
                    lock!(@write seed_metadata = ctx.global_state.seed_metadata; seed_metadata.insert(file_stem.to_string(), SeedMetadata {
                        locked_spoiler_log_path: locked_spoiler_log_path.clone(),
                        progression_spoiler: unlock_spoiler_log == UnlockSpoilerLog::Progression,
                    }));
                    if unlock_spoiler_log == UnlockSpoilerLog::Now && locked_spoiler_log_path.is_some() {
                        fs::rename(locked_spoiler_log_path.as_ref().unwrap(), Path::new(seed::DIR).join(format!("{file_stem}_Spoiler.json"))).await.to_racetime()?;
                        *locked_spoiler_log_path = None;
                    }
                }
                let extra = seed.extra(Utc::now()).await.to_racetime()?;
                if let Some(OfficialRaceData { cal_event, .. }) = official_data {
                    match seed.files.as_ref().expect("received seed with no files") {
                        seed::Files::MidosHouse { file_stem, .. } => {
                            sqlx::query!(
                                "UPDATE races SET file_stem = $1 WHERE id = $2",
                                file_stem, cal_event.race.id as _,
                            ).execute(db_pool).await.to_racetime()?;
                        }
                        seed::Files::OotrWeb { id, gen_time, file_stem } => {
                            sqlx::query!(
                                "UPDATE races SET web_id = $1, web_gen_time = $2, file_stem = $3 WHERE id = $4",
                                *id as i64, gen_time, file_stem, cal_event.race.id as _,
                            ).execute(db_pool).await.to_racetime()?;
                        }
                        seed::Files::TriforceBlitz { is_dev, uuid } => {
                            sqlx::query!(
                                "UPDATE races SET is_tfb_dev = $1, tfb_uuid = $2 WHERE id = $3",
                                is_dev, uuid, cal_event.race.id as _,
                            ).execute(db_pool).await.to_racetime()?;
                        }
                        seed::Files::AlttprDoorRando { .. } => {
                            // race_id is already the primary key of this table, we can do nothing here
                        }
                        seed::Files::TfbSotd { .. } => unimplemented!("Triforce Blitz seed of the day not supported for official races"),
                    }
                    if let Some([hash1, hash2, hash3, hash4, hash5]) = extra.file_hash {
                        sqlx::query!(
                            "UPDATE races SET hash1 = $1, hash2 = $2, hash3 = $3, hash4 = $4, hash5 = $5 WHERE id = $6",
                            hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _, cal_event.race.id as _,
                        ).execute(db_pool).await.to_racetime()?;
                        if let Some(preset) = rsl_preset {
                            match seed.files.as_ref().expect("received seed with no files") {
                                seed::Files::AlttprDoorRando { .. } => unreachable!(), // ALTTPR Mystery not supported
                                seed::Files::MidosHouse { file_stem, .. } => {
                                    sqlx::query!(
                                        "INSERT INTO rsl_seeds (room, file_stem, preset, hash1, hash2, hash3, hash4, hash5) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                                        format!("https://{}{}", racetime_host(), ctx.data().await.url), &file_stem, preset as _, hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _,
                                    ).execute(db_pool).await.to_racetime()?;
                                }
                                seed::Files::OotrWeb { id, gen_time, file_stem } => {
                                    sqlx::query!(
                                        "INSERT INTO rsl_seeds (room, file_stem, preset, web_id, web_gen_time, hash1, hash2, hash3, hash4, hash5) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                                        format!("https://{}{}", racetime_host(), ctx.data().await.url), &file_stem, preset as _, *id as i64, gen_time, hash1 as _, hash2 as _, hash3 as _, hash4 as _, hash5 as _,
                                    ).execute(db_pool).await.to_racetime()?;
                                }
                                seed::Files::TriforceBlitz { .. } | seed::Files::TfbSotd { .. } => unreachable!(), // no such thing as random settings Triforce Blitz
                            }
                        }
                    }
                    if let Some(password) = extra.password {
                        sqlx::query!("UPDATE races SET seed_password = $1 WHERE id = $2", password.into_iter().map(char::from).collect::<String>(), cal_event.race.id as _).execute(db_pool).await.to_racetime()?;
                    }
                }
                let seed_url = match seed.files.as_ref().expect("received seed with no files") {
                    seed::Files::AlttprDoorRando { race_id } => {
                        let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").expect("wrong hardcoded URL");
                        // TODO need to link to patch URL here
                        patcher_url.query_pairs_mut().append_pair("patch", &format!("https://hth.zeldaspeedruns.com/seed/DR_{race_id}.bps"));
                        patcher_url.to_string()
                    }  
                    seed::Files::MidosHouse { file_stem, .. } => format!("{}/seed/{file_stem}", base_uri()),
                    seed::Files::OotrWeb { id, .. } => format!("https://ootrandomizer.com/seed/get?id={id}"),
                    seed::Files::TriforceBlitz { is_dev: false, uuid } => format!("https://www.triforceblitz.com/seed/{uuid}"),
                    seed::Files::TriforceBlitz { is_dev: true, uuid } => format!("https://dev.triforceblitz.com/seeds/{uuid}"),
                    seed::Files::TfbSotd { ordinal, .. } => format!("https://www.triforceblitz.com/seed/daily/{ordinal}"),
                };
                ctx.say(if let French = language {
                    format!("@entrants Voici votre seed : {seed_url}")
                } else {
                    format!("@entrants Here is your seed: {seed_url}")
                }).await?;
                if let Some(file_hash) = extra.file_hash {
                    ctx.say(format_hash(file_hash)).await?;
                }
                match unlock_spoiler_log {
                    UnlockSpoilerLog::Now => ctx.say("The spoiler log is also available on the seed page.").await?,
                    UnlockSpoilerLog::Progression => ctx.say("The progression spoiler is also available on the seed page. The full spoiler will be available there after the race.").await?,
                    UnlockSpoilerLog::After => if let Some(seed::Files::TfbSotd { date, .. }) = seed.files {
                        if let Some(unlock_date) = date.succ_opt().and_then(|next| next.succ_opt()) {
                            let unlock_time = Utc.from_utc_datetime(&unlock_date.and_hms_opt(20, 0, 0).expect("failed to construct naive datetime at 20:00:00"));
                            let unlock_time = (unlock_time - Utc::now()).to_std().expect("unlock time for current daily seed in the past");
                            ctx.say(format!("The spoiler log will be available on the seed page in {}.", English.format_duration(unlock_time, true))).await?;
                        } else {
                            unimplemented!("distant future Triforce Blitz SotD")
                        }
                    } else {
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
                set_bot_raceinfo(ctx, &seed, rsl_preset, false).await?;
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
                            if let Some([hash1, hash2, hash3, hash4, hash5]) = extra.file_hash {
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
            Self::Error(RollError::Retries { num_retries, last_error }) => {
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
                    wheel::night_report(&format!("{}/error", night_path()), Some(&format!("seed roll error in https://{}{}: {e} ({e:?})", racetime_host(), ctx.data().await.url))).await.to_racetime()?;
                }
                ctx.say("Sorry @entrants, something went wrong while rolling the seed. Please report this error to TreZc0_ and if necessary roll the seed manually.").await?;
            }
            #[cfg(unix)] Self::Message(msg) => ctx.say(msg).await?,
        }
        Ok(())
    }
}

fn format_hash(file_hash: [HashIcon; 5]) -> impl fmt::Display {
    file_hash.into_iter().map(|icon| icon.to_racetime_emoji()).format(" ")
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

async fn room_options(goal: Goal, event: &event::Data<'_>, cal_event: &cal::Event, info_user: String, info_bot: String, auto_start: bool) -> racetime::StartRace {
    racetime::StartRace {
        goal: goal.as_str().to_owned(),
        goal_is_custom: goal.is_custom(),
        team_race: event.team_config.is_racetime_team_format() && matches!(cal_event.kind, cal::EventKind::Normal),
        invitational: !matches!(cal_event.race.entrants, Entrants::Open),
        unlisted: cal_event.is_private_async_part(),
        ranked: cal_event.is_private_async_part() || event.series != Series::TriforceBlitz && !matches!(cal_event.race.schedule, RaceSchedule::Async { .. }), //HACK: private async parts must be marked as ranked so they don't immediately get published on finish/cancel
        require_even_teams: true,
        start_delay: if event.series == Series::Standard && event.event != "w" && cal_event.race.entrants == Entrants::Open { 30 } else { 15 },
        time_limit: 24,
        time_limit_auto_complete: false,
        streaming_required: !Environment::default().is_dev() && !cal_event.is_private_async_part(),
        allow_comments: true,
        hide_comments: true,
        allow_prerace_chat: event.series != Series::Standard || event.event != "8" || cal_event.race.phase.as_ref().is_none_or(|phase| phase != "Qualifier"),
        allow_midrace_chat: event.series != Series::Standard || event.event != "8" || cal_event.race.phase.as_ref().is_none_or(|phase| phase != "Qualifier"),
        allow_non_entrant_chat: false, // only affects the race while it's ongoing, so !monitor still works
        chat_message_delay: 0,
        info_user, info_bot, auto_start,
    }
}

async fn set_bot_raceinfo(ctx: &RaceContext<GlobalState>, seed: &seed::Data, rsl_preset: Option<rsl::Preset>, show_password: bool) -> Result<(), Error> {
    let extra = seed.extra(Utc::now()).await.to_racetime()?;
    ctx.set_bot_raceinfo(&format!(
        "{rsl_preset}{file_hash}{sep}{password}{newline}{seed_url}",
        rsl_preset = rsl_preset.map(|preset| format!("{}\n", preset.race_info())).unwrap_or_default(),
        file_hash = extra.file_hash.map(|hash| format_hash(hash).to_string()).unwrap_or_default(),
        sep = if extra.file_hash.is_some() && extra.password.is_some() && show_password { " | " } else { "" },
        password = extra.password.filter(|_| show_password).map(|password| format_password(password).to_string()).unwrap_or_default(),
        newline = if extra.file_hash.is_some() || extra.password.is_some() && show_password { "\n" } else { "" },
        seed_url = match seed.files.as_ref().expect("received seed with no files") {
                seed::Files::AlttprDoorRando { race_id } => {
                let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").expect("wrong hardcoded URL");
                // TODO need to link to patch URL here
                patcher_url.query_pairs_mut().append_pair("patch", &format!("https://hth.zeldaspeedruns.com/seed/DR_{race_id}.bps"));
                patcher_url.to_string()
            }
            seed::Files::MidosHouse { file_stem, .. } => format!("{}/seed/{file_stem}", base_uri()),
            seed::Files::OotrWeb { id, .. } => format!("https://ootrandomizer.com/seed/get?id={id}"),
            seed::Files::TriforceBlitz { is_dev: false, uuid } => format!("https://www.triforceblitz.com/seed/{uuid}"),
            seed::Files::TriforceBlitz { is_dev: true, uuid } => format!("https://dev.triforceblitz.com/seeds/{uuid}"),
            seed::Files::TfbSotd { ordinal, .. } => format!("https://www.triforceblitz.com/seed/daily/{ordinal}"),
        },
    )).await
}

#[derive(Clone, Copy)]
struct Breaks {
    duration: Duration,
    interval: Duration,
}

struct CrosskeysRaceOptions {
    all_dungeons_ok: bool,
    flute_ok: bool,
    inverted_ok: bool,
    keydrop_ok: bool,
    mirror_scroll_ok: bool,
    pb_ok: bool,
    zw_ok: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct CrosskeysYaml {
    placements: HashMap<u8, CrosskeysPlacements>,
    settings: HashMap<u8, CrosskeysSetting>,
    start_inventory: HashMap<u8, &'static [&'static str]>,
    meta: CrosskeysMeta
}

#[derive(Clone, Serialize)]
pub(crate) struct CrosskeysPlacements {
    #[serde(rename = "Skull Woods - Pinball Room")]
    pinball_room: &'static str,
}

#[derive(Clone, Serialize)]
pub(crate) struct CrosskeysSetting {
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
pub(crate) struct CrosskeysMeta {
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
            duration: parse_duration(duration, DurationUnit::Minutes).ok_or(())?,
            interval: parse_duration(interval, DurationUnit::Hours).ok_or(())?,
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
    goal: Goal,
    restreams: HashMap<Url, RestreamState>,
    entrants: Vec<String>,
    fpa_invoked: bool,
    breaks_used: bool,
    /// Keys are racetime.gg team slugs if is_racetime_team_format, racetime.gg user IDs otherwise
    scores: HashMap<String, Option<tfb::Score>>,
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
    goal_notifications: Option<tokio::task::JoinHandle<()>>,
    start_saved: bool,
    fpa_enabled: bool,
    locked: bool,
    password_sent: bool,
    race_state: ArcRwLock<RaceState>,
    cleaned_up: bool,
}

impl Handler {
    /// For `existing_state`, `Some(None)` means this is an existing race room with unknown state, while `None` means this is a new race room.
    async fn should_handle_inner(race_data: &RaceData, global_state: Arc<GlobalState>, existing_state: Option<Option<&Self>>) -> bool {
        if Goal::from_race_data(race_data).is_none() { return false }
        if let Some(existing_state) = existing_state {
            if let Some(existing_state) = existing_state {
                if let Some(ref official_data) = existing_state.official_data {
                    if race_data.entrants.iter().any(|entrant| entrant.status.value == EntrantStatusValue::Done && {
                        let key = if let Some(ref team) = entrant.team { &team.slug } else { &entrant.user.id };
                        official_data.scores.get(key).is_some_and(|score| score.is_none())
                    }) {
                        return true
                    }
                }
                if let RaceStatusValue::Finished | RaceStatusValue::Cancelled = race_data.status.value { return !existing_state.cleaned_up && race_data.ended_at.is_none_or(|ended_at| Utc::now() - ended_at < TimeDelta::hours(1)) }
            } else {
                if let RaceStatusValue::Finished | RaceStatusValue::Cancelled = race_data.status.value { return false }
            }
        } else {
            lock!(clean_shutdown = global_state.clean_shutdown; {
                if !clean_shutdown.should_handle_new() {
                    unlock!();
                    return false
                }
                let room = OpenRoom::RaceTime {
                    room_url: race_data.url.clone(),
                    public: !race_data.unlisted,
                };
                assert!(clean_shutdown.open_rooms.insert(room.clone()), "should_handle_inner called for new race room {} but clean_shutdown.open_rooms already contained this room", race_data.url);
                clean_shutdown.updates.send(CleanShutdownUpdate::RoomOpened(room)).allow_unreceived();
            });
            if let RaceStatusValue::Finished | RaceStatusValue::Cancelled = race_data.status.value { return false }
        }
        true
    }

    fn is_official(&self) -> bool { self.official_data.is_some() }

    async fn goal(&self, ctx: &RaceContext<GlobalState>) -> Result<Goal, GoalFromStrError> {
        if let Some(OfficialRaceData { goal, .. }) = self.official_data {
            Ok(goal)
        } else {
            ctx.data().await.goal.name.parse()
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
        let goal = self.goal(ctx).await.to_racetime()?;
        if let Some(draft_kind) = goal.draft_kind() {
            let available_settings = lock!(@read state = self.race_state; if let RaceState::Draft { state: ref draft, .. } = *state {
                match draft.next_step(draft_kind, self.official_data.as_ref().and_then(|OfficialRaceData { cal_event, .. }| cal_event.race.game), &mut draft::MessageContext::RaceTime { high_seed_name: &self.high_seed_name, low_seed_name: &self.low_seed_name, reply_to }).await.to_racetime()?.kind {
                    draft::StepKind::GoFirst => None,
                    draft::StepKind::Ban { available_settings, .. } => Some(available_settings.all().map(|setting| setting.description).collect()),
                    draft::StepKind::Pick { available_choices, .. } => Some(available_choices.all().map(|setting| setting.description).collect()),
                    draft::StepKind::BooleanChoice { .. } | draft::StepKind::Done(_) | draft::StepKind::DoneRsl { .. } => Some(Vec::default()),
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
            });
            if available_settings.is_empty() {
                ctx.say(if let French = goal.language() {
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
        let goal = self.goal(ctx).await.to_racetime()?;
        let Some(draft_kind) = goal.draft_kind() else { unreachable!() };
        let RaceState::Draft { state: ref draft, unlock_spoiler_log } = *state else { unreachable!() };
        let step = draft.next_step(draft_kind, self.official_data.as_ref().and_then(|OfficialRaceData { cal_event, .. }| cal_event.race.game), &mut draft::MessageContext::RaceTime { high_seed_name: &self.high_seed_name, low_seed_name: &self.low_seed_name, reply_to: "friend" }).await.to_racetime()?;
        match step.kind {
            draft::StepKind::Done(settings) => {
                let (article, description) = if let French = goal.language() {
                    ("une", format!("seed avec {}", step.message))
                } else {
                    ("a", format!("seed with {}", step.message))
                };
                let event_id = self.official_data.as_ref().map(|OfficialRaceData { event, .. }| (event.series, &*event.event));
                self.roll_seed(ctx, goal.preroll_seeds(event_id), goal.rando_version(event_id), settings, unlock_spoiler_log, goal.language(), article, description).await;
            }
            draft::StepKind::DoneRsl { preset, world_count } => {
                let (article, description) = if let French = goal.language() {
                    ("une", format!("seed avec {}", step.message))
                } else {
                    ("a", format!("seed with {}", step.message))
                };
                self.roll_rsl_seed(ctx, preset, world_count, unlock_spoiler_log, goal.language(), article, description).await;
            }
            draft::StepKind::GoFirst | draft::StepKind::Ban { .. } | draft::StepKind::Pick { .. } | draft::StepKind::BooleanChoice { .. } => ctx.say(step.message).await?,
        }
        Ok(())
    }

    async fn draft_action(&self, ctx: &RaceContext<GlobalState>, sender: Option<&UserData>, action: draft::Action) -> Result<(), Error> {
        let goal = self.goal(ctx).await.to_racetime()?;
        let reply_to = sender.map_or("friend", |user| &user.name);
        if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
            lock!(@write state = self.race_state; if let Some(draft_kind) = goal.draft_kind() {
                match *state {
                    RaceState::Init => match draft_kind {
                        draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5 => ctx.say(format!("Sorry {reply_to}, no draft has been started. Use “!seed draft” to start one.")).await?,
                        draft::Kind::RslS7 => ctx.say(format!("Sorry {reply_to}, no draft has been started. Use “!seed draft” to start one. For more info about these options, use !presets")).await?,
                        draft::Kind::TournoiFrancoS3 => ctx.say(format!("Désolé {reply_to}, le draft n'a pas débuté. Utilisez “!seed draft” pour en commencer un. Pour plus d'infos, utilisez !presets")).await?,
                        draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5 => ctx.say(format!("Sorry {reply_to}, no draft has been started. Use “!seed draft” to start one. For more info about these options, use !presets / le draft n'a pas débuté. Utilisez “!seed draft” pour en commencer un. Pour plus d'infos, utilisez !presets")).await?,
                    },
                    RaceState::Draft { state: ref mut draft, .. } => {
                        let is_active_team = if let Some(OfficialRaceData { ref cal_event, ref event, .. }) = self.official_data {
                            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                            let is_active_team = if_chain! {
                                if let Some(sender) = sender;
                                if let Some(user) = User::from_racetime(&mut *transaction, &sender.id).await.to_racetime()?;
                                if let Some(team) = Team::from_event_and_member(&mut transaction, event.series, &event.event, user.id).await.to_racetime()?;
                                then {
                                    draft.is_active_team(draft_kind, cal_event.race.game, team.id).await.to_racetime()?
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
                            match draft.apply(draft_kind, self.official_data.as_ref().and_then(|OfficialRaceData { cal_event, .. }| cal_event.race.game), &mut draft::MessageContext::RaceTime { high_seed_name: &self.high_seed_name, low_seed_name: &self.low_seed_name, reply_to }, action).await.to_racetime()? {
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
                            }
                        }
                    }
                    RaceState::Rolling | RaceState::Rolled(_) | RaceState::SpoilerSent => match goal.language() {
                        French => ctx.say(format!("Désolé {reply_to}, mais il n'y a pas de draft, ou la phase de pick&ban est terminée.")).await?,
                        _ => ctx.say(format!("Sorry {reply_to}, there is no settings draft this race or the draft is already completed.")).await?,
                    },
                }
            } else {
                ctx.say(format!("Sorry {reply_to}, this event doesn't have a settings draft.")).await?;
            });
        } else {
            match goal.language() {
                French => ctx.say(format!("Désolé {reply_to}, mais la race a débuté.")).await?,
                _ => ctx.say(format!("Sorry {reply_to}, but the race has already started.")).await?,
            }
        }
        Ok(())
    }

    async fn roll_seed_inner(&self, ctx: &RaceContext<GlobalState>, delay_until: Option<DateTime<Utc>>, mut updates: mpsc::Receiver<SeedRollUpdate>, language: Language, article: &'static str, description: String) {
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
                } else {
                    delay
                };
                ctx.say(if let French = language {
                    format!("Votre {description} sera postée dans {}.", French.format_duration(display_delay, true))
                } else {
                    format!("Your {description} will be posted in {}.", English.format_duration(display_delay, true))
                }).await?;
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
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_seed(preroll, true, delay_until, version, settings, unlock_spoiler_log), language, article, description).await;
    }

    async fn roll_crosskeys2025_seed(&self, ctx: &RaceContext<GlobalState>, cal_event: cal::Event, language: Language, article: &'static str, description: String) {
        let official_start = cal_event.start().expect("handling room for official race without start time");
        let delay_until = official_start - TimeDelta::minutes(10);
        self.roll_seed_inner(ctx, Some(delay_until), ctx.global_state.clone().roll_crosskeys2025_seed(ctx.clone(), Some(delay_until), cal_event), language, article, description).await;
    }

    async fn roll_rsl_seed(&self, ctx: &RaceContext<GlobalState>, preset: rsl::VersionedPreset, world_count: u8, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_rsl_seed(delay_until, preset, world_count, unlock_spoiler_log), language, article, description).await;
    }

    async fn roll_tfb_seed(&self, ctx: &RaceContext<GlobalState>, version: &'static str, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        // Triforce Blitz website's auto unlock doesn't know about async parts so has to be disabled for asyncs
        let unlock_spoiler_log = if unlock_spoiler_log == UnlockSpoilerLog::After && self.official_data.as_ref().is_some_and(|official_data| official_data.cal_event.is_private_async_part()) { UnlockSpoilerLog::Never } else { unlock_spoiler_log };
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_tfb_seed(delay_until, version, Some(format!("https://{}{}", racetime_host(), ctx.data().await.url)), unlock_spoiler_log), language, article, description).await;
    }

    async fn roll_tfb_dev_seed(&self, ctx: &RaceContext<GlobalState>, coop: bool, unlock_spoiler_log: UnlockSpoilerLog, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        // Triforce Blitz website's auto unlock doesn't know about async parts so has to be disabled for asyncs
        let unlock_spoiler_log = if unlock_spoiler_log == UnlockSpoilerLog::After && self.official_data.as_ref().is_some_and(|official_data| official_data.cal_event.is_private_async_part()) { UnlockSpoilerLog::Never } else { unlock_spoiler_log };
        self.roll_seed_inner(ctx, delay_until, ctx.global_state.clone().roll_tfb_dev_seed(delay_until, coop, Some(format!("https://{}{}", racetime_host(), ctx.data().await.url)), unlock_spoiler_log), language, article, description).await;
    }

    async fn queue_existing_seed(&self, ctx: &RaceContext<GlobalState>, seed: seed::Data, language: Language, article: &'static str, description: String) {
        let official_start = self.official_data.as_ref().map(|official_data| official_data.cal_event.start().expect("handling room for official race without start time"));
        let delay_until = official_start.map(|start| start - TimeDelta::minutes(15));
        let (tx, rx) = mpsc::channel(1);
        tx.send(SeedRollUpdate::Done { rsl_preset: None, unlock_spoiler_log: UnlockSpoilerLog::After, seed }).await.unwrap();
        self.roll_seed_inner(ctx, delay_until, rx, language, article, description).await;
    }

    /// Returns `false` if this race was already finished/cancelled.
    async fn unlock_spoiler_log(&self, ctx: &RaceContext<GlobalState>, goal: Goal) -> Result<bool, Error> {
        lock!(@write state = self.race_state; {
            match *state {
                RaceState::Rolled(seed::Data { files: Some(ref files), .. }) => if self.official_data.as_ref().is_none_or(|official_data| !official_data.cal_event.is_private_async_part()) {
                    if let UnlockSpoilerLog::Progression | UnlockSpoilerLog::After = goal.unlock_spoiler_log(self.is_official(), false /* we may try to unlock a log that's already unlocked, but other than that, this assumption doesn't break anything */) {
                        match files {
                            seed::Files::AlttprDoorRando { .. } => unreachable!(),
                            seed::Files::MidosHouse { file_stem, locked_spoiler_log_path } => if let Some(locked_spoiler_log_path) = locked_spoiler_log_path {
                                lock!(@write seed_metadata = ctx.global_state.seed_metadata; seed_metadata.remove(&**file_stem));
                                fs::rename(locked_spoiler_log_path, Path::new(seed::DIR).join(format!("{file_stem}_Spoiler.json"))).await.to_racetime()?;
                            },
                            seed::Files::OotrWeb { id, file_stem, .. } => {
                                ctx.global_state.ootr_api_client.unlock_spoiler_log(*id).await.to_racetime()?;
                                let spoiler_log = ctx.global_state.ootr_api_client.seed_details(*id).await.to_racetime()?.spoiler_log;
                                fs::write(Path::new(seed::DIR).join(format!("{file_stem}_Spoiler.json")), &spoiler_log).await.to_racetime()?;
                            }
                            seed::Files::TriforceBlitz { .. } | seed::Files::TfbSotd { .. } => {} // automatically unlocked by triforceblitz.com
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
                        let _ = wheel::night_report(&format!("{}/error", night_path()), Some(&format!("race handler for https://{}{} panicked", racetime_host(), data.url))).await;
                    }
                }
            });
        });
        Ok(())
    }

    async fn new(ctx: &RaceContext<GlobalState>) -> Result<Self, Error> {
        let data = ctx.data().await;
        let goal = data.goal.name.parse::<Goal>().to_racetime()?;
        let (existing_seed, official_data, race_state, high_seed_name, low_seed_name, fpa_enabled) = lock!(new_room_lock = ctx.global_state.new_room_lock; { // make sure a new room isn't handled before it's added to the database
            let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
            let new_data = if let Some(cal_event) = cal::Event::from_room(&mut transaction, &ctx.global_state.http_client, format!("https://{}{}", racetime_host(), ctx.data().await.url).parse()?).await.to_racetime()? {
                let event = cal_event.race.event(&mut transaction).await.to_racetime()?;
                let mut entrants = Vec::default();
                for member in cal_event.racetime_users_to_invite(&mut transaction, &*ctx.global_state.discord_ctx.read().await, &event).await.to_racetime()? {
                    match member {
                        Ok(member) => {
                            if let Some(entrant) = data.entrants.iter().find(|entrant| entrant.user.id == member) {
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
                    if let French = goal.language();
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
                            format!(
                                "Welcome to the {weekly_name} weekly! Current settings: {}. See {} for details.",
                                s::SHORT_WEEKLY_SETTINGS,
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
                }, true, Vec::default()).await?;
                let (race_state, high_seed_name, low_seed_name) = if let Some(draft_kind) = event.draft_kind() {
                    let state = cal_event.race.draft.clone().expect("missing draft state");
                    let [high_seed_name, low_seed_name] = if let draft::StepKind::Done(_) | draft::StepKind::DoneRsl { .. } = state.next_step(draft_kind, cal_event.race.game, &mut draft::MessageContext::None).await.to_racetime()?.kind {
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
                        unlock_spoiler_log: goal.unlock_spoiler_log(true, false),
                        state,
                    }, high_seed_name, low_seed_name)
                } else {
                    (RaceState::Init, format!("Team A"), format!("Team B"))
                };
                let restreams = cal_event.race.video_urls.iter().map(|(&language, video_url)| (video_url.clone(), RestreamState {
                    language: Some(language),
                    restreamer_racetime_id: cal_event.race.restreamers.get(&language).cloned(),
                    ready: false,
                })).collect();
                let stream_delay = match cal_event.race.entrants {
                    Entrants::Open | Entrants::Count { .. } => event.open_stream_delay,
                    Entrants::Two(_) | Entrants::Three(_) | Entrants::Named(_) => event.invitational_stream_delay,
                };
                let emulator_settings_reminder = event.series == Series::Standard && event.event != "w" || event.series == Series::League; //TODO move to database
                let prevent_late_joins = event.series == Series::SpeedGaming || event.series == Series::Standard && event.event == "8"; //TODO move to database
                if !stream_delay.is_zero() || emulator_settings_reminder || prevent_late_joins {
                    let delay_until = cal_event.start().expect("handling room for official race without start time") - stream_delay - TimeDelta::minutes(5);
                    if let Ok(delay) = (delay_until - Utc::now()).to_std() {
                        let ctx = ctx.clone();
                        let requires_emote_only = event.series == Series::SpeedGaming && cal_event.race.phase.as_ref().is_some_and(|phase| phase == "Bracket");
                        tokio::spawn(async move {
                            sleep_until(Instant::now() + delay).await;
                            if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { return }
                            if !stream_delay.is_zero() {
                                ctx.say(format!("@entrants Remember to go live with a delay of {} ({} seconds){}!",
                                    English.format_duration(stream_delay, true),
                                    stream_delay.as_secs(),
                                    if requires_emote_only { " and set your chat to emote only" } else { "" },
                                )).await.expect("failed to send stream delay notice");
                            }
                            if emulator_settings_reminder || prevent_late_joins {
                                sleep(stream_delay).await;
                                let data = ctx.data().await;
                                if !Self::should_handle_inner(&*data, ctx.global_state.clone(), Some(None)).await { return }
                                if prevent_late_joins && data.status.value == RaceStatusValue::Open {
                                    ctx.set_invitational().await.expect("failed to make the room invitational");
                                }
                                if emulator_settings_reminder {
                                    ctx.say("@entrants Remember to show your emulator settings!").await.expect("failed to send emulator settings notice");
                                }
                            }
                        });
                    }
                }
                let fpa_enabled = match data.status.value {
                    RaceStatusValue::Invitational => {
                        ctx.say(if let French = goal.language() {
                            "Le FPA est activé pour cette race. Les joueurs pourront utiliser !fpa pendant la race pour signaler d'un problème technique de leur côté. Les race monitors doivent activer les notifications en cliquant sur l'icône de cloche 🔔 sous le chat."
                        } else {
                            "Fair play agreement is active for this official race. Entrants may use the !fpa command during the race to notify of a crash. Race monitors (if any) should enable notifications using the bell 🔔 icon below chat."
                        }).await?; //TODO different message for monitorless FPA?
                        true
                    }
                    RaceStatusValue::Open => false,
                    _ => data.entrants.len() < 10, // guess based on entrant count, assuming an open race for 10 or more
                };
                (
                    cal_event.race.seed.clone(),
                    Some(OfficialRaceData {
                        fpa_invoked: cal_event.race.fpa_invoked,
                        breaks_used: cal_event.race.breaks_used,
                        scores: HashMap::default(),
                        cal_event, event, goal, restreams, entrants,
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
                                files: Some(seed::Files::MidosHouse {
                                    file_stem: Cow::Owned(file_stem.to_owned()),
                                    locked_spoiler_log_path: None,
                                }),
                                progression_spoiler: false, //TODO
                            });
                            break
                        } else if let Some((_, seed_id)) = regex_captures!(r"^Seed: https://ootrandomizer\.com/seed/get?id=([0-9]+)$", section) {
                            let id = seed_id.parse().to_racetime()?;
                            race_state = RaceState::Rolled(seed::Data {
                                file_hash: None,
                                password: None,
                                files: Some(seed::Files::OotrWeb {
                                    gen_time: Utc::now(),
                                    file_stem: Cow::Owned(ctx.global_state.ootr_api_client.patch_file_stem(id).await.to_racetime()?),
                                    id,
                                }),
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
                } else {
                    match race_state {
                        RaceState::Init => match goal {
                            Goal::Cc7 => ctx.send_message(
                                "Welcome! This is a practice room for the S7 Challenge Cup. Learn more about the tournament at https://midos.house/event/s/7cc",
                                true,
                                vec![
                                    ("Roll seed (base settings)", ActionButton::Message {
                                        message: format!("!seed base"),
                                        help_text: Some(format!("Create a seed with the tournament's base settings.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                    ("Roll seed (random settings)", ActionButton::Message {
                                        message: format!("!seed random"),
                                        help_text: Some(format!("Simulate a settings draft with both players picking randomly. The settings are posted along with the seed.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                    ("Roll seed (custom settings)", ActionButton::Message {
                                        message: format!("!seed {}", s::S7_SETTINGS.into_iter().map(|setting| format!("{0} ${{{0}}}", setting.name)).format(" ")),
                                        help_text: Some(format!("Pick a set of draftable settings without doing a full draft.")),
                                        survey: Some(s::S7_SETTINGS.into_iter().map(|setting| SurveyQuestion {
                                            name: setting.name.to_owned(),
                                            label: setting.display.to_owned(),
                                            default: Some(format!("default")),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Radio,
                                            placeholder: None,
                                            options: iter::once((format!("default"), setting.default_display.to_owned()))
                                                .chain(setting.other.iter().map(|(name, display, _)| (name.to_string(), display.to_string())))
                                                .collect(),
                                        }).collect()),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Start settings draft", ActionButton::Message {
                                        message: format!("!seed draft"),
                                        help_text: Some(format!("Pick the settings here in the chat.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::CoOpS3 => ctx.send_message(
                                "Welcome! This is a practice room for the 3rd co-op tournament. Learn more about the tournament at https://midos.house/event/coop/3",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::CopaDoBrasil => ctx.send_message(
                                "Welcome! This is a practice room for the Copa do Brasil. Learn more about the tournament at https://midos.house/event/br/1",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::Crosskeys2025 => unreachable!("attempted to handle a user-opened Crosskeys2025 room"),
                            Goal::LeagueS8 => ctx.send_message(
                                "Welcome! This is a practice room for League Season 8. Learn more about the event at https://midos.house/event/league/8",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the current practice settings.")), //TODO update when league/8 settings are final
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::MixedPoolsS2 => ctx.send_message(
                                "Welcome! This is a practice room for the 2nd Mixed Pools Tournament. Learn more about the tournament at https://midos.house/event/mp/2",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::MixedPoolsS3 => ctx.send_message(
                                "Welcome! This is a practice room for the 3rd Mixed Pools Tournament. Learn more about the tournament at https://midos.house/event/mp/3",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::MixedPoolsS4 => ctx.send_message(
                                "Welcome! This is a practice room for the 4th Mixed Pools Tournament. Learn more about the tournament at https://midos.house/event/mp/4",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::Mq => ctx.send_message(
                                "Welcome! This is a practice room for the 12 MQ Tournament. Learn more about the tournament at https://midos.house/event/mq/1",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::MultiworldS3 | Goal::MultiworldS4 | Goal::MultiworldS5 => {
                                let (ordinal, event, available_settings) = match goal {
                                    Goal::MultiworldS3 => ("3rd", "3", mw::S3_SETTINGS),
                                    Goal::MultiworldS4 => ("4th", "4", mw::S4_SETTINGS),
                                    Goal::MultiworldS5 => ("5th", "5", mw::S5_SETTINGS),
                                    _ => unreachable!("checked in outer match"),
                                };
                                ctx.send_message(
                                    format!("Welcome! This is a practice room for the {ordinal} Multiworld Tournament. Learn more about the tournament at https://midos.house/event/mw/{event}"),
                                    true,
                                    vec![
                                        ("Roll seed (base settings)", ActionButton::Message {
                                            message: format!("!seed base"),
                                            help_text: Some(format!("Create a seed with the settings used for the qualifier and tiebreaker asyncs.")),
                                            survey: None,
                                            submit: None,
                                        }),
                                        ("Roll seed (random settings)", ActionButton::Message {
                                            message: format!("!seed random"),
                                            help_text: Some(format!("Simulate a settings draft with both teams picking randomly. The settings are posted along with the seed.")),
                                            survey: None,
                                            submit: None,
                                        }),
                                        ("Roll seed (custom settings)", ActionButton::Message {
                                            message: format!("!seed {}", available_settings.iter().map(|setting| format!("{0} ${{{0}}}", setting.name)).format(" ")),
                                            help_text: Some(format!("Pick a set of draftable settings without doing a full draft.")),
                                            survey: Some(available_settings.into_iter().map(|setting| SurveyQuestion {
                                                name: setting.name.to_owned(),
                                                label: setting.display.to_owned(),
                                                default: Some(setting.default.to_owned()),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Radio,
                                                placeholder: None,
                                                options: iter::once((setting.default.to_owned(), setting.default_display.to_owned()))
                                                    .chain(setting.other.iter().map(|(name, display)| (name.to_string(), display.to_string())))
                                                    .collect(),
                                            }).collect()),
                                            submit: Some(format!("Roll")),
                                        }),
                                        ("Start settings draft", ActionButton::Message {
                                            message: format!("!seed draft"),
                                            help_text: Some(format!("Pick the settings here in the chat.")),
                                            survey: None,
                                            submit: None,
                                        }),
                                    ],
                                ).await?;
                            }
                            Goal::NineDaysOfSaws => ctx.send_message(
                                "Welcome! This is a practice room for 9 Days of SAWS. Learn more about the event at https://docs.google.com/document/d/1xELThZtIctwN-vYtYhUqtd88JigNzabk8OZHANa0gqY/edit",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed ${{preset}}"),
                                        help_text: Some(format!("Select a preset and create a seed.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("preset"),
                                                label: format!("Preset"),
                                                default: None,
                                                help_text: Some(format!("Days 7 and 9 are identical to days 2 and 1, respectively. They are listed for the sake of convenience.")),
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: vec![
                                                    (format!("day1"), format!("Day 1: S6")),
                                                    (format!("day2"), format!("Day 2: Beginner")),
                                                    (format!("day3"), format!("Day 3: Advanced")),
                                                    (format!("day4"), format!("Day 4: S5 + one bonk KO")),
                                                    (format!("day5"), format!("Day 5: Beginner + mixed pools")),
                                                    (format!("day6"), format!("Day 6: Beginner 3-player multiworld")),
                                                    (format!("day7"), format!("Day 7: Beginner")),
                                                    (format!("day8"), format!("Day 8: S6 + dungeon ER")),
                                                    (format!("day9"), format!("Day 9: S6")),
                                                ],
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                ],
                            ).await?,
                            Goal::Pic7 => ctx.send_message(
                                "Welcome! This is a practice room for the 7th Pictionary Spoiler Log Race. Learn more about the race at https://midos.house/event/pic/7",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the race.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::PicRs2 => ctx.send_message(
                                "Welcome! This is a practice room for the 2nd Random Settings Pictionary Spoiler Log Race. Learn more about the race at https://midos.house/event/pic/rs2",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the weights used for the race.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::Rsl => ctx.send_message(
                                "Welcome to the OoTR Random Settings League! Learn more at https://rsl.one/",
                                true,
                                vec![
                                    ("Roll RSL seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with official Random Settings League weights.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                    ("Start weights draft", ActionButton::Message {
                                        message: format!("!seed draft ${{lite}}"),
                                        help_text: Some(format!("Ban and block weights here in the chat.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("lite"),
                                                label: format!("Use RSL-Lite weights"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                        ]),
                                        submit: Some(format!("Start Draft")),
                                    }),
                                    ("Roll multiworld seed", ActionButton::Message {
                                        message: format!("!seed mw ${{worldcount}}"),
                                        help_text: Some(format!("Create a random settings multiworld seed. Supports up to 15 players.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("worldcount"),
                                                label: format!("World count"),
                                                default: None,
                                                help_text: Some(format!("Please download the RSL script from https://github.com/matthewkirby/plando-random-settings if you want to roll seeds for more than 15 players.")),
                                                kind: SurveyQuestionKind::Radio,
                                                placeholder: None,
                                                options: (2..=15).map(|world_count| (world_count.to_string(), world_count.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("More presets", ActionButton::Message {
                                        message: format!("!seed ${{preset}}"),
                                        help_text: Some(format!("Select a preset and create a seed.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("preset"),
                                                label: format!("Preset"),
                                                default: None,
                                                help_text: Some(format!("Use !presets for more info.")),
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: all()
                                                    .filter(|preset| !matches!(preset, rsl::Preset::League | rsl::Preset::Multiworld))
                                                    .map(|preset| (preset.name().to_owned(), preset.race_info().to_owned()))
                                                    .collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                ],
                            ).await?,
                            Goal::Sgl2023 => ctx.send_message(
                                "Welcome! This is a practice room for SpeedGaming Live 2023. Learn more about the tournaments at https://docs.google.com/document/d/1EACqBl8ZOreD6xT5jQ2HrdLOnpBpKyjS3FUYK8XFeqg/edit",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournaments.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::Sgl2024 => ctx.send_message(
                                "Welcome! This is a practice room for SpeedGaming Live 2024. Learn more about the tournaments at https://docs.google.com/document/d/1I0IcnGMqKr3QaCgg923SR_SxVu0iytIA_lOhN2ybj9w/edit",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournaments.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::SongsOfHope => ctx.send_message(
                                "Welcome! This is a practice room for Songs of Hope, a charity tournament for the Autism of Society of America. Learn more about the tournament at https://midos.house/event/soh/1",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the settings used for the tournament.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::StandardRuleset => unreachable!("attempted to handle a user-opened Standard Ruleset room"),
                            Goal::TournoiFrancoS3 => ctx.send_message(
                                "Bienvenue ! Ceci est une practice room pour le tournoi francophone saison 3. Vous pouvez obtenir des renseignements supplémentaires ici : https://midos.house/event/fr/3",
                                true,
                                vec![
                                    ("Roll seed (settings de base)", ActionButton::Message {
                                        message: format!("!seed base ${{mq}}mq"),
                                        help_text: Some(format!("Roll une seed avec les settings de base, sans setting additionnel.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Donjons Master Quest"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (settings aléatoires)", ActionButton::Message {
                                        message: format!("!seed random ${{advanced}} ${{mq}}mq"),
                                        help_text: Some(format!("Simule en draft en sélectionnant des settings au hasard.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("advanced"),
                                                label: format!("Active les settings difficiles"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Donjons Master Quest"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (settings à choisir)", ActionButton::Message {
                                        message: format!("!seed {} ${{mq}}mq", fr::S3_SETTINGS.into_iter().map(|setting| format!("{0} ${{{0}}}", setting.name)).format(" ")),
                                        help_text: Some(format!("Vous laisse sélectionner les settings que vous voulez dans votre seed.")),
                                        survey: Some(fr::S3_SETTINGS.into_iter().map(|setting| SurveyQuestion {
                                            name: setting.name.to_owned(),
                                            label: setting.display.to_owned(),
                                            default: Some(setting.default.to_owned()),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Radio,
                                            placeholder: None,
                                            options: iter::once((setting.default.to_owned(), setting.default_display.to_owned()))
                                                .chain(setting.other.iter().map(|(name, _, display)| (name.to_string(), display.to_string())))
                                                .chain((setting.name == "dungeon-er").then(|| (format!("mixed"), format!("dungeon ER (mixés)"))))
                                                .collect(),
                                        }).chain(iter::once(SurveyQuestion {
                                            name: format!("mq"),
                                            label: format!("Donjons Master Quest"),
                                            default: Some(format!("0")),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Select,
                                            placeholder: None,
                                            options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                        })).collect()),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (avec draft)", ActionButton::Message {
                                        message: format!("!seed draft ${{advanced}} ${{mq}}mq"),
                                        help_text: Some(format!("Vous fait effectuer un draft dans le chat racetime.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("advanced"),
                                                label: format!("Active les settings difficiles"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Donjons Master Quest"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                ],
                            ).await?,
                            Goal::TournoiFrancoS4 => ctx.send_message( //TODO post welcome message in both English and French
                                "Welcome! This is a practice room for the Tournoi Francophone Saison 4. Learn more about the tournament at https://midos.house/event/fr/4",
                                true,
                                vec![
                                    ("Roll seed (base settings)", ActionButton::Message {
                                        message: format!("!seed base ${{mq}}mq"),
                                        help_text: Some(format!("Create a seed with the base settings.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Master Quest Dungeons"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (random settings)", ActionButton::Message {
                                        message: format!("!seed random ${{advanced}} ${{mq}}mq"),
                                        help_text: Some(format!("Simulate a settings draft with both teams picking randomly. The settings are posted along with the seed.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("advanced"),
                                                label: format!("Allow advanced settings"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Master Quest Dungeons"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (custom settings)", ActionButton::Message {
                                        message: format!("!seed {} ${{mq}}mq", fr::S4_SETTINGS.into_iter().map(|setting| format!("{0} ${{{0}}}", setting.name)).format(" ")),
                                        help_text: Some(format!("Pick a set of draftable settings without doing a full draft.")),
                                        survey: Some(fr::S4_SETTINGS.into_iter().map(|setting| SurveyQuestion {
                                            name: setting.name.to_owned(),
                                            label: setting.display.to_owned(),
                                            default: Some(setting.default.to_owned()),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Radio,
                                            placeholder: None,
                                            options: iter::once((setting.default.to_owned(), setting.default_display.to_owned()))
                                                .chain(setting.other.iter().map(|(name, _, display)| (name.to_string(), display.to_string())))
                                                .chain((setting.name == "dungeon-er").then(|| (format!("mixed"), format!("dungeon ER (mixed)"))))
                                                .collect(),
                                        }).chain(iter::once(SurveyQuestion {
                                            name: format!("mq"),
                                            label: format!("Master Quest Dungeons"),
                                            default: Some(format!("0")),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Select,
                                            placeholder: None,
                                            options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                        })).collect()),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Start settings draft", ActionButton::Message {
                                        message: format!("!seed draft ${{advanced}} ${{mq}}mq"),
                                        help_text: Some(format!("Pick the settings here in the chat.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("advanced"),
                                                label: format!("Allow advanced settings"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Master Quest Dungeons"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Start Draft")),
                                    }),
                                ],
                            ).await?,
                            Goal::TournoiFrancoS5 => ctx.send_message( //TODO post welcome message in both English and French
                                "Welcome! This is a practice room for the Tournoi Francophone Saison 5. Learn more about the tournament at https://midos.house/event/fr/5",
                                true,
                                vec![
                                    ("Roll seed (base settings)", ActionButton::Message {
                                        message: format!("!seed base ${{mq}}mq"),
                                        help_text: Some(format!("Create a seed with the base settings.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Master Quest Dungeons"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (random settings)", ActionButton::Message {
                                        message: format!("!seed random ${{advanced}} ${{mq}}mq"),
                                        help_text: Some(format!("Simulate a settings draft with both teams picking randomly. The settings are posted along with the seed.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("advanced"),
                                                label: format!("Allow advanced settings"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Master Quest Dungeons"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Roll seed (custom settings)", ActionButton::Message {
                                        message: format!("!seed {} ${{mq}}mq", fr::S5_SETTINGS.into_iter().map(|setting| format!("{0} ${{{0}}}", setting.name)).format(" ")),
                                        help_text: Some(format!("Pick a set of draftable settings without doing a full draft.")),
                                        survey: Some(fr::S5_SETTINGS.into_iter().map(|setting| SurveyQuestion {
                                            name: setting.name.to_owned(),
                                            label: setting.display.to_owned(),
                                            default: Some(setting.default.to_owned()),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Radio,
                                            placeholder: None,
                                            options: iter::once((setting.default.to_owned(), setting.default_display.to_owned()))
                                                .chain(setting.other.iter().map(|(name, _, display)| (name.to_string(), display.to_string())))
                                                .chain((setting.name == "dungeon-er").then(|| (format!("mixed"), format!("dungeon ER (mixed)"))))
                                                .collect(),
                                        }).chain(iter::once(SurveyQuestion {
                                            name: format!("mq"),
                                            label: format!("Master Quest Dungeons"),
                                            default: Some(format!("0")),
                                            help_text: None,
                                            kind: SurveyQuestionKind::Select,
                                            placeholder: None,
                                            options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                        })).collect()),
                                        submit: Some(format!("Roll")),
                                    }),
                                    ("Start settings draft", ActionButton::Message {
                                        message: format!("!seed draft ${{advanced}} ${{mq}}mq"),
                                        help_text: Some(format!("Pick the settings here in the chat.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("advanced"),
                                                label: format!("Allow advanced settings"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Bool,
                                                placeholder: None,
                                                options: Vec::default(),
                                            },
                                            SurveyQuestion {
                                                name: format!("mq"),
                                                label: format!("Master Quest Dungeons"),
                                                default: Some(format!("0")),
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: (0..=12).map(|mq| (mq.to_string(), mq.to_string())).collect(),
                                            },
                                        ]),
                                        submit: Some(format!("Start Draft")),
                                    }),
                                ],
                            ).await?,
                            Goal::TriforceBlitz => ctx.send_message(
                                "Welcome to Triforce Blitz! Learn more at https://triforceblitz.com/",
                                true,
                                vec![
                                    ("Roll S4 co-op seed", ActionButton::Message {
                                        message: format!("!seed s4coop"),
                                        help_text: Some(format!("Create a Triforce Blitz season 4 co-op seed.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                    ("Seed of the day", ActionButton::Message {
                                        message: format!("!seed daily"),
                                        help_text: Some(format!("Link the current seed of the day.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                    ("More presets", ActionButton::Message {
                                        message: format!("!seed ${{preset}}"),
                                        help_text: Some(format!("Select a preset and create a seed.")),
                                        survey: Some(vec![
                                            SurveyQuestion {
                                                name: format!("preset"),
                                                label: format!("Preset"),
                                                default: None,
                                                help_text: None,
                                                kind: SurveyQuestionKind::Select,
                                                placeholder: None,
                                                options: vec![
                                                    (format!("s3"), format!("S3")),
                                                    (format!("jr"), format!("Jabu's Revenge")),
                                                    (format!("s2"), format!("S2")),
                                                ],
                                            },
                                        ]),
                                        submit: Some(format!("Roll")),
                                    }),
                                ],
                            ).await?,
                            Goal::TriforceBlitzProgressionSpoiler => ctx.send_message(
                                "Welcome to Triforce Blitz Progression Spoiler!",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Create a seed with the current settings for the mode.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::WeTryToBeBetterS1 => ctx.send_message(
                                "Bienvenue ! Ceci est une practice room pour le tournoi WeTryToBeBetter. Vous pouvez obtenir des renseignements supplémentaires ici : https://midos.house/event/wttbb/1",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Roll une seed avec les settings utilisés pour le tournoi.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                            Goal::WeTryToBeBetterS2 => ctx.send_message(
                                "Bienvenue ! Ceci est une practice room pour le tournoi WeTryToBeBetter saison 2. Vous pouvez obtenir des renseignements supplémentaires ici : https://midos.house/event/wttbb/2",
                                true,
                                vec![
                                    ("Roll seed", ActionButton::Message {
                                        message: format!("!seed"),
                                        help_text: Some(format!("Roll une seed avec les settings utilisés pour le tournoi.")),
                                        survey: None,
                                        submit: None,
                                    }),
                                ],
                            ).await?,
                        },
                        RaceState::Rolled(_) => ctx.say("@entrants I just restarted. You may have to reconfigure !breaks and !fpa. Sorry about that.").await?,
                        RaceState::Draft { .. } | RaceState::Rolling | RaceState::SpoilerSent => unreachable!(),
                    }
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
            goal_notifications: None,
            start_saved: false,
            locked: false,
            password_sent: false,
            race_state: ArcRwLock::new(race_state),
            cleaned_up: false,
            official_data, high_seed_name, low_seed_name, fpa_enabled,
        };
        if let Some(OfficialRaceData { ref event, ref restreams, ref cal_event, .. }) = this.official_data {
            if !restreams.is_empty() {
                let restreams_text = restreams.iter().map(|(video_url, state)| format!("in {} at {video_url}", state.language.expect("preset restreams should have languages assigned"))).join(" and "); // don't use English.join_str since racetime.gg parses the comma as part of the URL
                for restreamer in restreams.values().flat_map(|RestreamState { restreamer_racetime_id, .. }| restreamer_racetime_id) {
                    let data = ctx.data().await;
                    if data.monitors.iter().find(|monitor| monitor.id == *restreamer).is_some() { continue }
                    if let Some(entrant) = data.entrants.iter().find(|entrant| entrant.user.id == *restreamer) { //TODO keep track of pending changes to the entrant list made in this method and match accordingly, e.g. players who are also monitoring should not be uninvited
                        match entrant.status.value {
                            EntrantStatusValue::Requested => {
                                ctx.accept_request(restreamer).await?;
                                ctx.add_monitor(restreamer).await?;
                                ctx.remove_entrant(restreamer).await?;
                            }
                            EntrantStatusValue::Invited |
                            EntrantStatusValue::Declined |
                            EntrantStatusValue::Ready |
                            EntrantStatusValue::NotReady |
                            EntrantStatusValue::InProgress |
                            EntrantStatusValue::Done |
                            EntrantStatusValue::Dnf |
                            EntrantStatusValue::Dq => {
                                ctx.add_monitor(restreamer).await?;
                            }
                        }
                    } else {
                        ctx.invite_user(restreamer).await?;
                        ctx.add_monitor(restreamer).await?;
                        ctx.remove_entrant(restreamer).await?;
                    }
                }
                let text = if restreams.values().any(|state| state.restreamer_racetime_id.is_none()) {
                    if_chain! {
                        if let French = goal.language();
                        if let Ok((video_url, state)) = restreams.iter().exactly_one();
                        if let Some(French) = state.language;
                        then {
                            format!("Cette race est restreamée en français chez {video_url} — l'auto-start est désactivé. Les organisateurs du tournoi peuvent utiliser “!monitor” pour devenir race monitor, puis pour inviter les restreamers en tant que race monitor et leur autoriser le force start.")
                        } else {
                            format!("This race is being restreamed {restreams_text} — auto-start is disabled. Tournament organizers can use “!monitor” to become race monitors, then invite the restreamer{0} as race monitor{0} to allow them to force-start.", if restreams.len() == 1 { "" } else { "s" })
                        }
                    }
                } else if let Ok((video_url, state)) = restreams.iter().exactly_one() {
                    if_chain! {
                        if let French = goal.language();
                        if let Some(French) = state.language;
                        then {
                            format!("Cette race est restreamée en français chez {video_url} — l'auto start est désactivé. Le restreamer peut utiliser “!ready” pour débloquer l'auto-start.")
                        } else {
                            format!("This race is being restreamed {restreams_text} — auto-start is disabled. The restreamer can use “!ready” to unlock auto-start.")
                        }
                    }
                } else {
                    format!("This race is being restreamed {restreams_text} — auto-start is disabled. Restreamers can use “!ready” once the restream is ready. Auto-start will be unlocked once all restreams are ready.")
                };
                ctx.send_message(&text, true, Vec::default()).await?;
            }
            lock!(@read state = this.race_state; {
                if existing_seed.files.is_some() {
                    this.queue_existing_seed(ctx, existing_seed, English, "a", format!("seed")).await; //TODO better article/description
                } else {
                    let event_id = Some((event.series, &*event.event));
                    match *state {
                        RaceState::Init => match goal {
                            | Goal::CoOpS3
                            | Goal::CopaDoBrasil
                            | Goal::LeagueS8
                            | Goal::MixedPoolsS2
                            | Goal::MixedPoolsS3
                            | Goal::MixedPoolsS4
                            | Goal::Mq
                            | Goal::Pic7
                            | Goal::Sgl2023
                            | Goal::Sgl2024
                            | Goal::SongsOfHope
                            | Goal::TriforceBlitzProgressionSpoiler
                                => this.roll_seed(ctx, goal.preroll_seeds(event_id), goal.rando_version(event_id), goal.single_settings().expect("goal has no single settings"), goal.unlock_spoiler_log(true, false), English, "a", format!("seed")).await,
                            | Goal::WeTryToBeBetterS1
                            | Goal::WeTryToBeBetterS2
                                => this.roll_seed(ctx, goal.preroll_seeds(event_id), goal.rando_version(event_id), goal.single_settings().expect("goal has no single settings"), goal.unlock_spoiler_log(true, false), French, "une", format!("seed")).await,
                            | Goal::Cc7
                            | Goal::MultiworldS3
                            | Goal::MultiworldS4
                            | Goal::MultiworldS5
                            | Goal::Rsl
                            | Goal::TournoiFrancoS3
                            | Goal::TournoiFrancoS4
                            | Goal::TournoiFrancoS5
                                => unreachable!("should have draft state set"),
                            | Goal::Crosskeys2025
                                // TODO: Don't propogate this error, handle the failed seed roll either here or below
                                // TODO: Don't pass the description in here: generate it from the settings
                                => this.roll_crosskeys2025_seed(ctx, cal_event.clone(), English, "a", format!("Crosskeys Tournament 2025 Seed")).await,
                            Goal::NineDaysOfSaws => unreachable!("9dos series has concluded"),
                            Goal::PicRs2 => this.roll_rsl_seed(ctx, rsl::VersionedPreset::Fenhl {
                                version: Some((Version::new(2, 3, 8), 10)),
                                preset: rsl::DevFenhlPreset::Pictionary,
                            }, 1, goal.unlock_spoiler_log(true, false), English, "a", format!("seed")).await,
                            Goal::StandardRuleset => if let (Series::Standard, "8" | "8cc") = (event.series, &*event.event) {
                                this.roll_seed(ctx, goal.preroll_seeds(event_id), goal.rando_version(event_id), s::s8_settings(), goal.unlock_spoiler_log(true, false), English, "an", format!("S8 seed")).await
                            } else {
                                let mut settings = s::weekly_settings();
                                settings.insert(format!("password_lock"), json!(true));
                                this.roll_seed(ctx, goal.preroll_seeds(event_id), goal.rando_version(event_id), settings, goal.unlock_spoiler_log(true, false), English, "a", format!("weekly seed")).await
                            },
                            Goal::TriforceBlitz => this.roll_tfb_dev_seed(ctx, true, goal.unlock_spoiler_log(true, false), English, "a", format!("Triforce Blitz S4 co-op seed")).await,
                        },
                        RaceState::Draft { .. } => this.advance_draft(ctx, &state).await?,
                        RaceState::Rolling | RaceState::Rolled(_) | RaceState::SpoilerSent => {}
                    }
                }
            });
        }
        Ok(this)
    }

    async fn command(&mut self, ctx: &RaceContext<GlobalState>, cmd_name: String, args: Vec<String>, _is_moderator: bool, is_monitor: bool, msg: &ChatMessage) -> Result<(), Error> {
        let goal = self.goal(ctx).await.to_racetime()?;
        let reply_to = msg.user.as_ref().map_or("friend", |user| &user.name);
        match &*cmd_name.to_ascii_lowercase() {
            cmd @ ("ban" | "block" | "draft" | "first" | "no" | "pick" | "second" | "skip" | "yes") => match goal.parse_draft_command(cmd, &args) {
                DraftCommandParseResult::Action(action) => self.draft_action(ctx, msg.user.as_ref(), action).await?,
                DraftCommandParseResult::SendSettings { language, msg } => self.send_settings(ctx, &if let French = language {
                    format!("Désolé {reply_to}, {msg}")
                } else {
                    format!("Sorry {reply_to}, {msg}")
                }, reply_to).await?,
                DraftCommandParseResult::Error { language, msg } => ctx.say(if let French = language {
                    format!("Désolé {reply_to}, {msg}")
                } else {
                    format!("Sorry {reply_to}, {msg}")
                }).await?,
            },
            "breaks" | "break" => match args[..] {
                [] => if let Some(breaks) = self.breaks {
                    ctx.say(if let French = goal.language() {
                        format!("Vous aurez une pause de {}. Vous pouvez les désactiver avec !breaks off.", breaks.format(French))
                    } else {
                        format!("Breaks are currently set to {}. Disable with !breaks off", breaks.format(English))
                    }).await?;
                } else {
                    ctx.say(if let French = goal.language() {
                        "Les pauses sont actuellement désactivées. Exemple pour les activer : !breaks 5m every 2h30."
                    } else {
                        "Breaks are currently disabled. Example command to enable: !breaks 5m every 2h30"
                    }).await?;
                },
                [ref arg] if arg == "off" => if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                    self.breaks = None;
                    ctx.say(if let French = goal.language() {
                        "Les pauses sont désormais désactivées."
                    } else {
                        "Breaks are now disabled."
                    }).await?;
                } else {
                    ctx.say(if let French = goal.language() {
                        format!("Désolé {reply_to}, mais la race a débuté.")
                    } else {
                        format!("Sorry {reply_to}, but the race has already started.")
                    }).await?;
                },
                _ => if let Ok(breaks) = args.join(" ").parse::<Breaks>() {
                    if breaks.duration < Duration::from_secs(60) {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, le temps minimum pour une pause (si active) est de 1 minute. Vous pouvez désactiver les pauses avec !breaks off")
                        } else {
                            format!("Sorry {reply_to}, minimum break time (if enabled at all) is 1 minute. You can disable breaks entirely with !breaks off")
                        }).await?;
                    } else if breaks.interval < breaks.duration + Duration::from_secs(5 * 60) {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, il doit y avoir un minimum de 5 minutes entre les pauses.")
                        } else {
                            format!("Sorry {reply_to}, there must be a minimum of 5 minutes between breaks since I notify runners 5 minutes in advance.")
                        }).await?;
                    } else if breaks.duration + breaks.interval >= Duration::from_secs(24 * 60 * 60) {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, vous ne pouvez pas faire de pauses si tard dans la race, vu que les race rooms se ferment au bout de 24 heures.")
                        } else {
                            format!("Sorry {reply_to}, race rooms are automatically closed after 24 hours so these breaks wouldn't work.")
                        }).await?;
                    } else {
                        self.breaks = Some(breaks);
                        ctx.say(if let French = goal.language() {
                            format!("Vous aurez une pause de {}.", breaks.format(French))
                        } else {
                            format!("Breaks set to {}.", breaks.format(English))
                        }).await?;
                    }
                } else {
                    ctx.say(if let French = goal.language() {
                        format!("Désolé {reply_to}, je ne reconnais pas ce format pour les pauses. Exemple pour les activer : !breaks 5m every 2h30.")
                    } else {
                        format!("Sorry {reply_to}, I don't recognize that format for breaks. Example commands: !breaks 5m every 2h30, !breaks off")
                    }).await?;
                },
            },
            "fpa" => match args[..] {
                [] => if self.fpa_enabled {
                    if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                        ctx.say(if let French = goal.language() {
                            "Le FPA ne peut pas être appelé avant que la race ne commence."
                        } else {
                            "FPA cannot be invoked before the race starts."
                        }).await?;
                    } else {
                        if let Some(OfficialRaceData { ref cal_event, ref restreams, ref mut fpa_invoked, ref event, .. }) = self.official_data {
                            *fpa_invoked = true;
                            if restreams.is_empty() {
                                ctx.say(if_chain! {
                                    if let French = goal.language();
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
                                ctx.say(if let French = goal.language() {
                                    format!("@everyone Le FPA a été appelé par {reply_to}. Merci d'arrêter de jouer, la race étant restreamée.")
                                } else {
                                    format!("@everyone FPA has been invoked by {reply_to}. Please pause since this race is being restreamed.")
                                }).await?;
                            }
                        } else {
                            ctx.say(if let French = goal.language() {
                                format!("@everyone Le FPA a été appelé par {reply_to}.")
                            } else {
                                format!("@everyone FPA has been invoked by {reply_to}.")
                            }).await?;
                        }
                    }
                } else {
                    ctx.say(if let French = goal.language() {
                        "Le FPA n'est pas activé. Les Race Monitors peuvent l'activer avec !fpa on."
                    } else {
                        "Fair play agreement is not active. Race monitors may enable FPA for this race with !fpa on"
                    }).await?;
                },
                [ref arg] => match &*arg.to_ascii_lowercase() {
                    "on" => if self.is_official() {
                        ctx.say(if let French = goal.language() {
                            "Le FPA est toujours activé dans les races officielles."
                        } else {
                            "Fair play agreement is always active in official races."
                        }).await?;
                    } else if !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                        } else {
                            format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                        }).await?;
                    } else if self.fpa_enabled {
                        ctx.say(if let French = goal.language() {
                            "Le FPA est déjà activé."
                        } else {
                            "Fair play agreement is already activated."
                        }).await?;
                    } else {
                        self.fpa_enabled = true;
                        ctx.say(if let French = goal.language() {
                            "Le FPA est désormais activé. Les joueurs pourront utiliser !fpa pendant la race pour signaler d'un problème technique de leur côté. Les race monitors doivent activer les notifications en cliquant sur l'icône de cloche 🔔 sous le chat."
                        } else {
                            "Fair play agreement is now active. @entrants may use the !fpa command during the race to notify of a crash. Race monitors should enable notifications using the bell 🔔 icon below chat."
                        }).await?;
                    },
                    "off" => if self.is_official() {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, mais le FPA ne peut pas être désactivé pour les races officielles.")
                        } else {
                            format!("Sorry {reply_to}, but FPA can't be deactivated for official races.")
                        }).await?;
                    } else if !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                        } else {
                            format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                        }).await?;
                    } else if self.fpa_enabled {
                        self.fpa_enabled = false;
                        ctx.say(if let French = goal.language() {
                            "Le FPA est désormais désactivé."
                        } else {
                            "Fair play agreement is now deactivated."
                        }).await?;
                    } else {
                        ctx.say(if let French = goal.language() {
                            "Le FPA est déjà désactivé."
                        } else {
                            "Fair play agreement is not active."
                        }).await?;
                    },
                    _ => ctx.say(if let French = goal.language() {
                        format!("Désolé {reply_to}, les seules commandes sont “!fpa on”, “!fpa off” ou “!fpa”.")
                    } else {
                        format!("Sorry {reply_to}, I don't recognize that subcommand. Use “!fpa on” or “!fpa off”, or just “!fpa” to invoke FPA.")
                    }).await?,
                },
                [..] => ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, les seules commandes sont “!fpa on”, “!fpa off” ou “!fpa”.")
                } else {
                    format!("Sorry {reply_to}, I didn't quite understand that. Use “!fpa on” or “!fpa off”, or just “!fpa” to invoke FPA.")
                }).await?,
            },
            "lock" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                self.locked = true;
                ctx.say(if_chain! {
                    if let French = goal.language();
                    if !self.is_official();
                    then {
                        format!("Race verrouillée. Je ne génèrerai une seed que pour les race monitors.")
                    } else {
                        format!("Lock initiated. I will now only roll seeds for {}.", if self.is_official() { "race monitors or tournament organizers" } else { "race monitors" })
                    }
                }).await?;
            } else {
                ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                } else {
                    format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                }).await?;
            },
            "monitor" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                let monitor = &msg.user.as_ref().expect("received !monitor command from bot").id;
                if let Some(entrant) = ctx.data().await.entrants.iter().find(|entrant| entrant.user.id == *monitor) {
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
                ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, seuls les organisateurs du tournoi peuvent faire cela.")
                } else {
                    format!("Sorry {reply_to}, only tournament organizers can do that.")
                }).await?;
            } else {
                ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, cette commande n'est disponible que pour les races officielles.")
                } else {
                    format!("Sorry {reply_to}, this command is only available for official races.")
                }).await?;
            },
            "presets" => goal.send_presets(ctx).await?,
            "ready" => if let Some(OfficialRaceData { ref mut restreams, ref cal_event, ref event, .. }) = self.official_data {
                if let Some(state) = restreams.values_mut().find(|state| state.restreamer_racetime_id.as_ref() == Some(&msg.user.as_ref().expect("received !ready command from bot").id)) {
                    state.ready = true;
                } else {
                    ctx.say(if let French = goal.language() {
                        format!("Désolé {reply_to}, seuls les restreamers peuvent faire cela.")
                    } else {
                        format!("Sorry {reply_to}, only restreamers can do that.")
                    }).await?;
                    return Ok(())
                }
                if restreams.values().all(|state| state.ready) {
                    ctx.say(if_chain! {
                        if let French = goal.language();
                        if let Ok((_, state)) = restreams.iter().exactly_one();
                        if let Some(French) = state.language;
                        then {
                            "Restream prêt. Déverrouillage de l'auto-start."
                        } else {
                            "All restreams ready, unlocking auto-start…"
                        }
                    }).await?;
                    let (access_token, _) = racetime::authorize_with_host(&ctx.global_state.host_info, &ctx.global_state.racetime_config.client_id, &ctx.global_state.racetime_config.client_secret, &ctx.global_state.http_client).await?;
                    room_options(
                        goal, event, cal_event,
                        ctx.data().await.info_user.clone().unwrap_or_default(),
                        ctx.data().await.info_bot.clone().unwrap_or_default(),
                        true,
                    ).await.edit_with_host(&ctx.global_state.host_info, &access_token, &ctx.global_state.http_client, CATEGORY, &ctx.data().await.slug).await?;
                } else {
                    ctx.say(format!("Restream ready, still waiting for other restreams.")).await?;
                }
            } else {
                ctx.say(if let French = goal.language() {
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
                                        let (access_token, _) = racetime::authorize_with_host(&ctx.global_state.host_info, &ctx.global_state.racetime_config.client_id, &ctx.global_state.racetime_config.client_secret, &ctx.global_state.http_client).await?;
                                        room_options(
                                            goal, event, cal_event,
                                            ctx.data().await.info_user.clone().unwrap_or_default(),
                                            ctx.data().await.info_bot.clone().unwrap_or_default(),
                                            false,
                                        ).await.edit_with_host(&ctx.global_state.host_info, &access_token, &ctx.global_state.http_client, CATEGORY, &ctx.data().await.slug).await?;
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
                    ctx.say(if let French = goal.language() {
                        format!("Désolé {reply_to}, cette commande n'est disponible que pour les races officielles.")
                    } else {
                        format!("Sorry {reply_to}, this command is only available for official races.")
                    }).await?;
                }
            } else {
                ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                } else {
                    format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                }).await?;
            },
            "score" => if_chain! {
                if let Goal::TriforceBlitz | Goal::TriforceBlitzProgressionSpoiler = goal;
                if let Some(OfficialRaceData { ref event, ref mut scores, .. }) = self.official_data;
                then {
                    if let Some(UserData { mut ref id, .. }) = msg.user {
                        let data = ctx.data().await;
                        if let Some(entrant) = data.entrants.iter().find(|entrant| entrant.user.id == *id) {
                            if let Some(ref team) = entrant.team {
                                id = &team.slug;
                            }
                        }
                        if let Some(score) = scores.get_mut(id) {
                            let old_score = *score;
                            if_chain! {
                                if let Some((pieces, duration)) = args.split_first();
                                if let Ok(pieces) = pieces.parse();
                                if pieces <= tfb::piece_count(event.team_config);
                                then {
                                    let new_score = tfb::Score {
                                        team_config: event.team_config,
                                        last_collection_time: if pieces == 0 {
                                            Duration::default()
                                        } else {
                                            let Some(last_collection_time) = parse_duration(&duration.join(" "), DurationUnit::Hours) else {
                                                ctx.say(format!("Sorry {reply_to}, I don't recognize that time format. Example format: 1h23m45s")).await?;
                                                return Ok(())
                                            };
                                            last_collection_time
                                        },
                                        pieces,
                                    };
                                    *score = Some(new_score);
                                    ctx.say(if let Some(old_score) = old_score {
                                        format!("Score edited: {new_score} (was: {old_score})")
                                    } else {
                                        format!("Score reported: {new_score}")
                                    }).await?;
                                    if self.check_tfb_finish(ctx).await? {
                                        self.cleaned_up = true;
                                    }
                                } else {
                                    ctx.send_message(
                                        &format!("Sorry {reply_to}, I didn't quite understand that. Please use this button to try again:"),
                                        false,
                                        vec![tfb::report_score_button(event.team_config, None)],
                                    ).await?;
                                }
                            }
                        } else {
                            ctx.say(format!("Sorry {reply_to}, only entrants who have already finished can do that.")).await?;
                        }
                    } else {
                        ctx.say(format!("Sorry {reply_to}, I was unable to read your user ID.")).await?;
                    }
                } else {
                    ctx.say(format!("Sorry {reply_to}, this command is only available for official Triforce Blitz races.")).await?;
                }
            },
            "seed" | "spoilerseed" => if let RaceStatusValue::Open | RaceStatusValue::Invitational = ctx.data().await.status.value {
                lock!(@write state = self.race_state; match *state {
                    RaceState::Init => if self.locked && !self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                        ctx.say(if let French = goal.language() {
                            format!("Désolé {reply_to}, la race est verrouillée. Seuls {} peuvent générer une seed pour cette race.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                        } else {
                            format!("Sorry {reply_to}, seed rolling is locked. Only {} may roll a seed for this race.", if self.is_official() { "race monitors or tournament organizers" } else { "race monitors" })
                        }).await?;
                    } else {
                        let mut transaction = ctx.global_state.db_pool.begin().await.to_racetime()?;
                        match goal.parse_seed_command(&mut transaction, &ctx.global_state, self.is_official(), cmd_name.to_ascii_lowercase() == "spoilerseed", &args).await.to_racetime()? {
                            SeedCommandParseResult::Alttpr => {
                                // TODO THIS NEEDS TO BE IMPLEMENTED -- call door rando .py and roll seed with arguments
                                Command::new("echo").args(["hello", "world"]).check("echo").await.to_racetime()?;
                                unimplemented!()
                            }
                            SeedCommandParseResult::Regular { settings, unlock_spoiler_log, language, article, description } => {
                                let event_id = self.official_data.as_ref().map(|OfficialRaceData { event, .. }| (event.series, &*event.event));
                                self.roll_seed(ctx, goal.preroll_seeds(event_id), goal.rando_version(event_id), settings, unlock_spoiler_log, language, article, description).await
                            },
                            SeedCommandParseResult::Rsl { preset, world_count, unlock_spoiler_log, language, article, description } => self.roll_rsl_seed(ctx, preset, world_count, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::Tfb { version, unlock_spoiler_log, language, article, description } => self.roll_tfb_seed(ctx, version, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::TfbDev { coop, unlock_spoiler_log, language, article, description } => self.roll_tfb_dev_seed(ctx, coop, unlock_spoiler_log, language, article, description).await,
                            SeedCommandParseResult::QueueExisting { data, language, article, description } => self.queue_existing_seed(ctx, data, language, article, description).await,
                            SeedCommandParseResult::SendPresets { language, msg } => {
                                ctx.say(if let French = language {
                                    format!("Désolé {reply_to}, {msg}. Veuillez utiliser un des suivants :")
                                } else {
                                    format!("Sorry {reply_to}, {msg}. Use one of the following:")
                                }).await?;
                                goal.send_presets(ctx).await?;
                            }
                            SeedCommandParseResult::SendSettings { language, msg } => {
                                unlock!();
                                self.send_settings(ctx, &if let French = language {
                                    format!("Désolé {reply_to}, {msg}")
                                } else {
                                    format!("Sorry {reply_to}, {msg}")
                                }, reply_to).await?;
                                return Ok(())
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
                    },
                    RaceState::Draft { .. } => ctx.say(format!("Sorry {reply_to}, settings are already being drafted.")).await?,
                    RaceState::Rolling => ctx.say(format!("Sorry {reply_to}, but I'm already rolling a seed for this room. Please wait.")).await?,
                    RaceState::Rolled(_) | RaceState::SpoilerSent => ctx.say(format!("Sorry {reply_to}, but I already rolled a seed. Check the race info!")).await?,
                });
            } else {
                ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, mais la race a débuté.")
                } else {
                    format!("Sorry {reply_to}, but the race has already started.")
                }).await?;
            },
            "settings" => lock!(@read state = self.race_state; self.send_settings(ctx, if let RaceState::Draft { .. } = *state {
                if let French = goal.language() {
                    "Settings pouvant être actuellement choisis :"
                } else {
                    "Currently draftable settings:"
                }
            } else {
                if let French = goal.language() {
                    "Settings pouvant être choisis :"
                } else {
                    "Draftable settings:"
                }
            }, reply_to).await?),
            "unlock" => if self.can_monitor(ctx, is_monitor, msg).await.to_racetime()? {
                self.locked = false;
                ctx.say(if let French = goal.language() {
                    "Race déverrouillée. N'importe qui peut désormais générer une seed."
                } else {
                    "Lock released. Anyone may now roll a seed."
                }).await?;
            } else {
                ctx.say(if let French = goal.language() {
                    format!("Désolé {reply_to}, seuls {} peuvent faire cela.", if self.is_official() { "les race monitors et les organisateurs du tournoi" } else { "les race monitors" })
                } else {
                    format!("Sorry {reply_to}, only {} can do that.", if self.is_official() { "race monitors and tournament organizers" } else { "race monitors" })
                }).await?;
            },
            _ => ctx.say(if let French = goal.language() {
                format!("Désolé {reply_to}, je ne reconnais pas cette commande.")
            } else {
                format!("Sorry {reply_to}, I don't recognize that command.")
            }).await?, //TODO “did you mean”? list of available commands with !help?
        }
        Ok(())
    }

    async fn race_data(&mut self, ctx: &RaceContext<GlobalState>, _old_race_data: RaceData) -> Result<(), Error> {
        let data = ctx.data().await;
        let goal = self.goal(ctx).await.to_racetime()?;
        if let Some(OfficialRaceData { ref event, ref entrants, ref mut scores, .. }) = self.official_data {
            for entrant in &data.entrants {
                match entrant.status.value {
                    EntrantStatusValue::Requested => if entrants.contains(&entrant.user.id) {
                        ctx.accept_request(&entrant.user.id).await?;
                    },
                    EntrantStatusValue::Done => if let Goal::TriforceBlitz | Goal::TriforceBlitzProgressionSpoiler = goal {
                        let (key, reply_to) = if let Some(ref team) = entrant.team {
                            (team.slug.clone(), &team.name)
                        } else {
                            (entrant.user.id.clone(), &entrant.user.name)
                        };
                        if let hash_map::Entry::Vacant(entry) = scores.entry(key) {
                            ctx.send_message(
                                &format!("{reply_to}, please report your score:"),
                                false,
                                vec![tfb::report_score_button(event.team_config, entrant.finish_time)],
                            ).await?;
                            entry.insert(None);
                        }
                    },
                    _ => {}
                }
            }
        }
        if !self.start_saved {
            if let (Goal::Rsl, Some(start)) = (goal, data.started_at) {
                sqlx::query!("UPDATE rsl_seeds SET start = $1 WHERE room = $2", start, format!("https://{}{}", racetime_host(), ctx.data().await.url)).execute(&ctx.global_state.db_pool).await.to_racetime()?;
                self.start_saved = true;
            }
        }
        match data.status.value {
            RaceStatusValue::Pending => if !self.password_sent {
                lock!(@read state = self.race_state; if let RaceState::Rolled(ref seed) = *state {
                    let extra = seed.extra(Utc::now()).await.to_racetime()?;
                    if let Some(password) = extra.password {
                        ctx.say(format!("This seed is password protected. To start a file, enter this password on the file select screen:\n{}\nYou are allowed to enter the password before the race starts.", format_password(password))).await?;
                        set_bot_raceinfo(ctx, seed, None /*TODO support RSL seeds with password lock? */, true).await?;
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
                                    ctx.say(if let French = goal.language() {
                                        "@entrants Rappel : pause dans 5 minutes."
                                    } else {
                                        "@entrants Reminder: Next break in 5 minutes."
                                    }),
                                    sleep(Duration::from_secs(5 * 60)),
                                );
                                if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { break }
                                let msg = if let French = goal.language() {
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
                                    ctx.say(if let French = goal.language() {
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
                match goal {
                    Goal::Pic7 | Goal::PicRs2 => {
                        self.goal_notifications.get_or_insert_with(|| {
                            let ctx = ctx.clone();
                            tokio::spawn(async move {
                                let initial_wait = ctx.data().await.started_at.expect("in-progress race with no start time") + TimeDelta::minutes(match goal {
                                    Goal::Pic7 => 10,
                                    Goal::PicRs2 => 25,
                                    _ => unreachable!(),
                                }) - Utc::now();
                                if let Ok(initial_wait) = initial_wait.to_std() {
                                    sleep(initial_wait).await;
                                    if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { return }
                                    let (_, ()) = tokio::join!(
                                        ctx.say("@entrants Reminder: 5 minutes until you can start drawing/playing."),
                                        sleep(Duration::from_secs(5 * 60)),
                                    );
                                    let _ = ctx.say("@entrants You may now start drawing/playing.").await;
                                }
                            })
                        });
                    }
                    Goal::TriforceBlitz => if ctx.data().await.time_limit > Duration::from_secs(2 * 60 * 60) {
                        self.goal_notifications.get_or_insert_with(|| {
                            let ctx = ctx.clone();
                            tokio::spawn(async move {
                                let initial_wait = ctx.data().await.started_at.expect("in-progress race with no start time") + TimeDelta::hours(2) - Utc::now();
                                if let Ok(initial_wait) = initial_wait.to_std() {
                                    sleep(initial_wait).await;
                                    let is_1v1 = {
                                        let data = ctx.data().await;
                                        if !Self::should_handle_inner(&*data, ctx.global_state.clone(), Some(None)).await { return }
                                        data.entrants_count == 2
                                    };
                                    let _ = ctx.say(if is_1v1 {
                                        "@entrants Time limit reached. If anyone has found at least 1 Triforce piece, please .done. If neither player has any pieces, please continue and .done when one is found."
                                    } else {
                                        "@entrants Time limit reached. If you've found at least 1 Triforce piece, please mark yourself as done. If you haven't, you may continue playing until you find one."
                                    }).await;
                                }
                            })
                        });
                    },
                    Goal::TriforceBlitzProgressionSpoiler => {
                        self.goal_notifications.get_or_insert_with(|| {
                            let ctx = ctx.clone();
                            tokio::spawn(async move {
                                let initial_wait = ctx.data().await.started_at.expect("in-progress race with no start time") + TimeDelta::minutes(10) - Utc::now();
                                if let Ok(initial_wait) = initial_wait.to_std() {
                                    sleep(initial_wait).await;
                                    if !Self::should_handle_inner(&*ctx.data().await, ctx.global_state.clone(), Some(None)).await { return }
                                    let (_, ()) = tokio::join!(
                                        ctx.say("@entrants Reminder: 5 minutes until you can start playing."),
                                        sleep(Duration::from_secs(5 * 60)),
                                    );
                                    let (_, ()) = tokio::join!(
                                        ctx.say("@entrants You may now start playing."),
                                        sleep(Duration::from_secs((60 + 45) * 60)),
                                    );
                                    let is_1v1 = {
                                        let data = ctx.data().await;
                                        if !Self::should_handle_inner(&*data, ctx.global_state.clone(), Some(None)).await { return }
                                        data.entrants_count == 2
                                    };
                                    let _ = ctx.say(if is_1v1 {
                                        "@entrants Time limit reached. If anyone has found at least 1 Triforce piece, please .done. If neither player has any pieces, please continue and .done when one is found."
                                    } else {
                                        "@entrants Time limit reached. If you've found at least 1 Triforce piece, please mark yourself as done. If you haven't, you may continue playing until you find one."
                                    }).await;
                                }
                            })
                        });
                    }
                    | Goal::Cc7
                    | Goal::CoOpS3
                    | Goal::CopaDoBrasil
                    | Goal::Crosskeys2025
                    | Goal::LeagueS8
                    | Goal::MixedPoolsS2
                    | Goal::MixedPoolsS3
                    | Goal::MixedPoolsS4
                    | Goal::Mq
                    | Goal::MultiworldS3
                    | Goal::MultiworldS4
                    | Goal::MultiworldS5
                    | Goal::NineDaysOfSaws
                    | Goal::Rsl
                    | Goal::Sgl2023
                    | Goal::Sgl2024
                    | Goal::SongsOfHope
                    | Goal::StandardRuleset
                    | Goal::TournoiFrancoS3
                    | Goal::TournoiFrancoS4
                    | Goal::TournoiFrancoS5
                    | Goal::WeTryToBeBetterS1
                    | Goal::WeTryToBeBetterS2
                        => {}
                }
            }
            RaceStatusValue::Finished => if self.unlock_spoiler_log(ctx, goal).await? {
                if let Goal::TriforceBlitz | Goal::TriforceBlitzProgressionSpoiler = goal {
                    if self.check_tfb_finish(ctx).await? {
                        self.cleaned_up = true;
                    }
                } else {
                    self.cleaned_up = true;
                    if let Some(OfficialRaceData { ref cal_event, ref event, fpa_invoked, breaks_used, .. }) = self.official_data {
                        self.official_race_finished(ctx, data, cal_event, event, fpa_invoked, breaks_used || self.breaks.is_some(), None).await?;
                    }
                }
            },
            RaceStatusValue::Cancelled => {
                if !self.password_sent {
                    lock!(@read state = self.race_state; if let RaceState::Rolled(ref seed) = *state {
                        let extra = seed.extra(Utc::now()).await.to_racetime()?;
                        if let Some(password) = extra.password {
                            ctx.say(format!("This seed is password protected. To start a file, enter this password on the file select screen:\n{}", format_password(password))).await?;
                            set_bot_raceinfo(ctx, seed, None /*TODO support RSL seeds with password lock? */, true).await?;
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
                                //TODO mention organizer role
                                .push("race cancelled: <https://")
                                .push(racetime_host())
                                .push(&ctx.data().await.url)
                                .push('>')
                                .build()
                            ).await.to_racetime()?;
                        }
                    }
                }
                self.unlock_spoiler_log(ctx, goal).await?;
                if let Goal::Rsl = goal {
                    sqlx::query!("DELETE FROM rsl_seeds WHERE room = $1", format!("https://{}{}", racetime_host(), ctx.data().await.url)).execute(&ctx.global_state.db_pool).await.to_racetime()?;
                }
                self.cleaned_up = true;
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
        );
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Server(errors))
        }
    }
}

pub(crate) async fn create_room(transaction: &mut Transaction<'_, Postgres>, discord_ctx: &DiscordCtx, host_info: &racetime::HostInfo, client_id: &str, client_secret: &str, extra_room_tx: &RwLock<mpsc::Sender<String>>, http_client: &reqwest::Client, clean_shutdown: Arc<Mutex<CleanShutdown>>, cal_event: &cal::Event, event: &event::Data<'_>) -> Result<Option<(bool, String)>, Error> {
    let room_url = match cal_event.should_create_room(&mut *transaction, event).await.to_racetime()? {
        cal::RaceHandleMode::None => return Ok(None),
        cal::RaceHandleMode::Notify => Err("please get your equipment and report to the tournament room"),
        cal::RaceHandleMode::RaceTime => match racetime::authorize_with_host(host_info, client_id, client_secret, http_client).await {
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
                let Some(goal) = Goal::for_event(cal_event.race.series, &cal_event.race.event) else { return Ok(None) };
                let race_slug = room_options(
                    goal, event, cal_event,
                    info_user,
                    String::default(),
                    cal_event.is_private_async_part() || cal_event.race.video_urls.is_empty(),
                ).await.start_with_host(host_info, &access_token, &http_client, CATEGORY).await?;
                let room_url = Url::parse(&format!("https://{}/{CATEGORY}/{race_slug}", host_info.hostname))?;
                match cal_event.kind {
                    cal::EventKind::Normal => { sqlx::query!("UPDATE races SET room = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                    cal::EventKind::Async1 => { sqlx::query!("UPDATE races SET async_room1 = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                    cal::EventKind::Async2 => { sqlx::query!("UPDATE races SET async_room2 = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                    cal::EventKind::Async3 => { sqlx::query!("UPDATE races SET async_room3 = $1 WHERE id = $2", room_url.to_string(), cal_event.race.id as _).execute(&mut **transaction).await.to_racetime()?; }
                }
                lock!(@read extra_room_tx = extra_room_tx; extra_room_tx.send(race_slug).await.allow_unreceived());
                Ok(room_url)
            }
            Err(Error::Reqwest(e)) if e.status().is_some_and(|status| status.is_server_error()) => {
                // racetime.gg's auth endpoint has been known to return server errors intermittently.
                // In that case, we simply try again in the next iteration of the sleep loop.
                return Ok(None)
            }
            Err(e) => return Err(e),
        },
        cal::RaceHandleMode::Discord => {
            let task_clean_shutdown = clean_shutdown.clone();
            lock!(clean_shutdown = clean_shutdown; {
                if clean_shutdown.should_handle_new() {
                    let room = OpenRoom::Discord { id: cal_event.race.id.into(), kind: cal_event.kind };
                    assert!(clean_shutdown.open_rooms.insert(room.clone()));
                    clean_shutdown.updates.send(CleanShutdownUpdate::RoomOpened(room)).allow_unreceived();
                    let cal_event = cal_event.clone();
                    tokio::spawn(async move {
                        println!("Discord race handler started");
                        let res = tokio::spawn(crate::discord_bot::handle_race()).await;
                        lock!(clean_shutdown = task_clean_shutdown; {
                            let room = OpenRoom::Discord { id: cal_event.race.id.into(), kind: cal_event.kind };
                            assert!(clean_shutdown.open_rooms.remove(&room));
                            clean_shutdown.updates.send(CleanShutdownUpdate::RoomClosed(room)).allow_unreceived();
                            if clean_shutdown.open_rooms.is_empty() {
                                clean_shutdown.updates.send(CleanShutdownUpdate::Empty).allow_unreceived();
                            }
                        });
                        if let Ok(()) = res {
                            println!("Discord race handler stopped");
                        } else {
                            eprintln!("Discord race handler panicked");
                            if let Environment::Production = Environment::default() {
                                let _ = wheel::night_report(&format!("{}/error", night_path()), Some("Discord race handler panicked")).await;
                            }
                        }
                    });
                }
            });
            Err("remember to send your video to an organizer once you're done") //TODO “please check your direct messages” for private async parts, “will be handled here in the match thread” for public async parts
        }
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
            let ping_standard = event.series == Series::Standard && cal_event.race.entrants == Entrants::Open && event.discord_guild == Some(OOTR_DISCORD_GUILD);
            let info_prefix = match (&cal_event.race.phase, &cal_event.race.round) {
                (Some(phase), Some(round)) => Some(format!("{phase} {round}")),
                (Some(phase), None) => Some(phase.clone()),
                (None, Some(round)) => Some(round.clone()),
                (None, None) => None,
            };
            let mut msg = MessageBuilder::default();
            if ping_standard {
                msg.mention(&RoleId::new(640750480246571014)); // @Standard
                msg.push(' ');
            }
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
                    msg.push(' ');
                    if !ping_standard {
                        msg.push('<');
                    }
                    msg.push(room_url);
                    if !ping_standard {
                        msg.push('>');
                    }
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
    Ok(Some((is_room_url, msg)))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PrepareSeedsError {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] EventData(#[from] event::DataError),
    #[error(transparent)] Roll(#[from] RollError),
    #[error(transparent)] SeedData(#[from] seed::ExtraDataError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

async fn prepare_seeds(global_state: Arc<GlobalState>, mut seed_cache_rx: watch::Receiver<()>, mut shutdown: rocket::Shutdown) -> Result<(), PrepareSeedsError> {
    'outer: loop {
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND room IS NULL AND async_room1 IS NULL AND async_room2 IS NULL AND async_room3 IS NULL AND file_stem IS NULL AND tfb_uuid IS NULL"#).fetch_all(&global_state.db_pool).await? {
            let mut transaction = global_state.db_pool.begin().await?;
            let race = Race::from_id(&mut transaction, &global_state.http_client, id).await?;
            let event = race.event(&mut transaction).await?;
            if let Some(goal) = Goal::for_event(event.series, &*event.event) {
                if let PrerollMode::Long = goal.preroll_seeds(Some((event.series, &*event.event))) {
                    if let Some(settings) = race.single_settings(&mut transaction).await? {
                        transaction.commit().await?;
                        if race.seed.files.is_none()
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
                                    goal.rando_version(Some((race.series, &race.event))),
                                    settings.clone(),
                                    goal.unlock_spoiler_log(true, false),
                                );
                                loop {
                                    select! {
                                        () = &mut shutdown => break 'outer,
                                        Some(update) = seed_rx.recv() => match update {
                                            SeedRollUpdate::Queued(_) |
                                            SeedRollUpdate::MovedForward(_) |
                                            SeedRollUpdate::Started => {}
                                            SeedRollUpdate::Done { mut seed, rsl_preset: _, unlock_spoiler_log: _ } => {
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
        }
        let event_rows = sqlx::query!(r#"SELECT series AS "series: Series", event FROM events WHERE end_time IS NULL OR end_time > NOW()"#).fetch_all(&global_state.db_pool).await?;
        for goal in all::<Goal>() {
            if let Some(settings) = goal.single_settings() { //TODO preroll for official races with single settings even if their goals don't have single settings
                if goal.preroll_seeds(None) == PrerollMode::Long && event_rows.iter().any(|row| goal.matches_event(row.series, &row.event)) {
                    if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM prerolled_seeds WHERE goal_name = $1) AS "exists!""#, goal.as_str()).fetch_one(&global_state.db_pool).await? { break }
                    'seed: loop {
                        let mut seed_rx = global_state.clone().roll_seed(
                            PrerollMode::Long,
                            false,
                            None,
                            goal.rando_version(None),
                            settings.clone(),
                            goal.unlock_spoiler_log(false, false),
                        );
                        loop {
                            select! {
                                () = &mut shutdown => break 'outer,
                                Some(update) = seed_rx.recv() => match update {
                                    SeedRollUpdate::Queued(_) |
                                    SeedRollUpdate::MovedForward(_) |
                                    SeedRollUpdate::Started => {}
                                    SeedRollUpdate::Done { seed, rsl_preset: _, unlock_spoiler_log: _ } => {
                                        let extra = seed.extra(Utc::now()).await?;
                                        let [hash1, hash2, hash3, hash4, hash5] = match extra.file_hash {
                                            Some(hash) => hash.map(Some),
                                            None => [None; 5],
                                        };
                                        match seed.files {
                                            Some(seed::Files::MidosHouse { file_stem, locked_spoiler_log_path }) => {
                                                sqlx::query!("INSERT INTO prerolled_seeds
                                                    (goal_name, file_stem, locked_spoiler_log_path, hash1, hash2, hash3, hash4, hash5, seed_password, progression_spoiler)
                                                VALUES
                                                    ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                                                ",
                                                    goal.as_str(),
                                                    &file_stem,
                                                    locked_spoiler_log_path,
                                                    hash1 as _,
                                                    hash2 as _,
                                                    hash3 as _,
                                                    hash4 as _,
                                                    hash5 as _,
                                                    extra.password.map(|password| password.into_iter().map(char::from).collect::<String>()),
                                                    goal.unlock_spoiler_log(false, false) == UnlockSpoilerLog::Progression,
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
            }
        }
        select! {
            () = &mut shutdown => break,
            res = timeout(Duration::from_secs(60 * 60), seed_cache_rx.changed().then(|res| if let Ok(()) = res { Either::Left(future::ready(())) } else { Either::Right(future::pending()) })) => {
                let (Ok(()) | Err(_)) = res;
            }
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
                lock!(new_room_lock = global_state.new_room_lock; { // make sure a new room isn't handled before it's added to the database
                    let mut transaction = global_state.db_pool.begin().await?;
                    for cal_event in cal::Event::rooms_to_open(&mut transaction, &global_state.http_client).await? {
                        let event = cal_event.race.event(&mut transaction).await?;
                        if let Some((is_room_url, msg)) = create_room(&mut transaction, &*global_state.discord_ctx.read().await, &global_state.host_info, &global_state.racetime_config.client_id, &global_state.racetime_config.client_secret, &global_state.extra_room_tx, &global_state.http_client, global_state.clean_shutdown.clone(), &cal_event, &event).await? {
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
                                for team in cal_event.active_teams() {
                                    for member in team.members(&mut transaction).await? {
                                        if let Some(discord) = member.discord {
                                            discord.id.create_dm_channel(&*ctx).await?.say(&*ctx, &msg).await?;
                                        }
                                    }
                                }
                            } else {
                                if_chain! {
                                    if !cal_event.is_private_async_part();
                                    if let Some(channel) = event.discord_race_room_channel;
                                    then {
                                        if let Some(thread) = cal_event.race.scheduling_thread {
                                            thread.say(&*ctx, &msg).await?;
                                            channel.send_message(&*ctx, CreateMessage::default().content(msg).allowed_mentions(CreateAllowedMentions::default())).await?;
                                        } else {
                                            channel.say(&*ctx, msg).await?;
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
                    transaction.commit().await?;
                });
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

async fn handle_rooms(global_state: Arc<GlobalState>, racetime_config: &ConfigRaceTime, shutdown: rocket::Shutdown) -> Result<(), HandleRoomsError> {
    let mut last_crash = Instant::now();
    let mut wait_time = Duration::from_secs(1);
    loop {
        match racetime::BotBuilder::new(CATEGORY, &racetime_config.client_id, &racetime_config.client_secret)
            .state(global_state.clone())
            .host(global_state.host_info.clone())
            .user_agent(concat!("MidosHouse/", env!("CARGO_PKG_VERSION"), " (https://github.com/midoshouse/midos.house)"))
            .scan_races_every(Duration::from_secs(5))
            .build().await
        {
            Ok(bot) => {
                lock!(@write extra_room_tx = global_state.extra_room_tx; *extra_room_tx = bot.extra_room_sender());
                let () = bot.run_until::<Handler, _, _>(shutdown).await?;
                break Ok(())
            }
            Err(e) if e.is_network_error() => {
                if last_crash.elapsed() >= Duration::from_secs(60 * 60 * 24) {
                    wait_time = Duration::from_secs(1); // reset wait time after no crash for a day
                } else {
                    wait_time *= 2; // exponential backoff
                }
                eprintln!("failed to connect to racetime.gg (retrying in {}): {e} ({e:?})", English.format_duration(wait_time, true));
                if wait_time >= Duration::from_secs(16) {
                    wheel::night_report(&format!("{}/error", night_path()), Some(&format!("failed to connect to racetime.gg (retrying in {}): {e} ({e:?})", English.format_duration(wait_time, true)))).await?;
                }
                sleep(wait_time).await;
                last_crash = Instant::now();
            }
            Err(e) => {
                wheel::night_report(&format!("{}/error", night_path()), Some(&format!("error handling racetime.gg rooms: {e} ({e:?})"))).await?;
                break Err(e.into())
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MainError {
    #[error(transparent)] CreateRooms(#[from] CreateRoomsError),
    #[error(transparent)] HandleRooms(#[from] HandleRoomsError),
    #[error(transparent)] PrepareSeeds(#[from] PrepareSeedsError),
}

pub(crate) async fn main(config: Config, shutdown: rocket::Shutdown, global_state: Arc<GlobalState>, seed_cache_rx: watch::Receiver<()>) -> Result<(), MainError> {
    let ((), (), ()) = tokio::try_join!(
        prepare_seeds(global_state.clone(), seed_cache_rx, shutdown.clone()).err_into::<MainError>(),
        create_rooms(global_state.clone(), shutdown.clone()).err_into(),
        handle_rooms(global_state, if Environment::default().is_dev() { &config.racetime_bot_dev } else { &config.racetime_bot_production }, shutdown).err_into(),
    )?;
    Ok(())
}
