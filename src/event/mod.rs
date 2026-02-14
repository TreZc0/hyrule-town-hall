use {
    serenity::all::{
        CreateMessage,
        EditMember,
        EditRole,
    },
    sqlx::{
        PgPool,
        types::Json,
    },
    crate::{
        game,

        notification::SimpleNotificationKind,
        prelude::*,
        racetime_bot::VersionedBranch,
    },
};

pub(crate) mod configure;
pub(crate) mod enter;
pub(crate) mod setup;
pub(crate) mod teams;
pub(crate) mod roles;
pub(crate) mod asyncs;
pub(crate) mod qualifiers;
pub(crate) mod zsr_export;

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "signup_status", rename_all = "snake_case")]
pub(crate) enum SignupStatus {
    Created,
    Confirmed,
    Unconfirmed,
}

impl SignupStatus {
    fn is_confirmed(&self) -> bool {
        matches!(self, Self::Created | Self::Confirmed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, FromFormField)]
#[sqlx(type_name = "team_role", rename_all = "snake_case")]
pub(crate) enum Role {
    /// For solo events.
    None,
    /// Player 1 of 2. 'Runner' in Pictionary.
    Sheikah,
    /// Player 2 of 2. 'Pilot' in Pictionary.
    Gerudo,
    /// Player 1 of 3.
    Power,
    /// Player 2 of 3.
    Wisdom,
    /// Player 3 of 3.
    Courage,
}

impl Role {
    pub(crate) fn from_css_class(css_class: &str) -> Option<Self> {
        match css_class {
            "sheikah" => Some(Self::Sheikah),
            "gerudo" => Some(Self::Gerudo),
            "power" => Some(Self::Power),
            "wisdom" => Some(Self::Wisdom),
            "courage" => Some(Self::Courage),
            _ => None,
        }
    }

    fn css_class(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Sheikah => Some("sheikah"),
            Self::Gerudo => Some("gerudo"),
            Self::Power => Some("power"),
            Self::Wisdom => Some("wisdom"),
            Self::Courage => Some("courage"),
        }
    }
}

#[derive(PartialEq, Eq)]
pub(crate) enum MatchSource<'a> {
    Manual,
    Challonge {
        community: Option<&'a str>,
        tournament: &'a str,
    },
    League,
    StartGG(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "qualifier_score_hiding", rename_all = "snake_case")]
pub(crate) enum QualifierScoreHiding {
    None,
    AsyncOnly,
    FullPoints,
    FullPointsCounts,
    FullComplete,
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "team_config", rename_all = "lowercase")]
pub(crate) enum TeamConfig {
    Solo,
    CoOp,
    TfbCoOp,
    Pictionary,
    Multiworld,
}

impl TeamConfig {
    pub(crate) fn roles(&self) -> &'static [(Role, &'static str)] {
        match self {
            Self::Solo => &[
                (Role::None, "Runner"),
            ],
            Self::CoOp => &[
                (Role::Sheikah, "Player 1"),
                (Role::Gerudo, "Player 2"),
            ],
            Self::TfbCoOp => &[
                (Role::Sheikah, "World 1"),
                (Role::Gerudo, "World 2"),
            ],
            Self::Pictionary => &[
                (Role::Sheikah, "Runner"),
                (Role::Gerudo, "Pilot"),
            ],
            Self::Multiworld => &[
                (Role::Power, "World 1"),
                (Role::Wisdom, "World 2"),
                (Role::Courage, "World 3"),
            ],
        }
    }

    /// Whether team members with the given role should be invited to race rooms.
    pub(crate) fn role_is_racing(&self, role: Role) -> bool {
        !matches!(self, Self::Pictionary) || matches!(role, Role::Sheikah)
    }

    pub(crate) fn is_racetime_team_format(&self) -> bool {
        self.roles().iter().filter(|&&(role, _)| self.role_is_racing(role)).count() > 1
    }

    pub(crate) fn has_distinct_roles(&self) -> bool {
        match self {
            | Self::Solo
            | Self::CoOp
                => false,
            | Self::TfbCoOp
            | Self::Pictionary
            | Self::Multiworld
                => true,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Data<'a> {
    pub(crate) series: Series,
    pub(crate) event: Cow<'a, str>,
    pub(crate) display_name: String,
    pub(crate) short_name: Option<String>,
    /// The event's originally scheduled starting time, not accounting for the 24-hour deadline extension in the event of an odd number of teams for events with qualifier asyncs.
    pub(crate) base_start: Option<DateTime<Utc>>,
    pub(crate) end: Option<DateTime<Utc>>,
    pub(crate) url: Option<Url>,
    pub(crate) challonge_community: Option<String>,
    pub(crate) speedgaming_slug: Option<String>,
    pub(crate) hide_races_tab: bool,
    pub(crate) hide_teams_tab: bool,
    pub(crate) teams_url: Option<Url>,
    pub(crate) enter_url: Option<Url>,
    pub(crate) video_url: Option<Url>,
    pub(crate) discord_guild: Option<GuildId>,
    pub(crate) discord_invite_url: Option<Url>,
    pub(crate) discord_race_room_channel: Option<ChannelId>,
    pub(crate) discord_race_results_channel: Option<ChannelId>,
    pub(crate) discord_organizer_channel: Option<ChannelId>,
    pub(crate) discord_scheduling_channel: Option<ChannelId>,
    pub(crate) discord_volunteer_info_channel: Option<ChannelId>,
    pub(crate) discord_async_channel: Option<ChannelId>,
    pub(crate) rando_version: Option<VersionedBranch>,
    pub(crate) settings_string: Option<String>,
    pub(crate) single_settings: Option<seed::Settings>,
    pub(crate) team_config: TeamConfig,
    enter_flow: Option<enter::Flow>,
    pub(crate) show_opt_out: bool,
    pub(crate) show_qualifier_times: bool,
    pub(crate) default_game_count: i16,
    pub(crate) min_schedule_notice: Duration,
    pub(crate) open_stream_delay: Duration,
    pub(crate) invitational_stream_delay: Duration,
    pub(crate) retime_window: Duration,
    pub(crate) auto_import: bool,
    pub(crate) emulator_settings_reminder: bool,
    pub(crate) prevent_late_joins: bool,
    pub(crate) manual_reporting_with_breaks: bool,
    pub(crate) language: Language,
    #[allow(dead_code)] // Will be used for tabbed UI
    pub(crate) default_volunteer_language: Language,
    pub(crate) asyncs_active: bool,
    pub(crate) swiss_standings: bool,
    pub(crate) discord_events_enabled: bool,
    pub(crate) discord_events_require_restream: bool,
    pub(crate) listed: bool,
    /// Maps round names to mode names for swiss events where mode is fixed per round.
    /// Example: {"Round 1": "ambrozia", "Round 2": "crosskeys"}
    pub(crate) round_modes: Option<HashMap<String, String>>,
    /// When true, qualifier requests create Discord threads with READY/countdown/FINISH buttons
    /// instead of using web forms for submission.
    pub(crate) automated_asyncs: bool,
    /// When true, automatic volunteer request posts are enabled for this event.
    pub(crate) volunteer_requests_enabled: bool,
    /// How many hours in advance to post volunteer request announcements.
    pub(crate) volunteer_request_lead_time_hours: i32,
    /// When true, role pings are included in volunteer request posts when below min_count.
    pub(crate) volunteer_request_ping_enabled: bool,
    /// When true, uses event-specific role bindings. When false, uses game-level role bindings.
    pub(crate) force_custom_role_binding: bool,
    /// Controls when qualifier scores are hidden on the teams page.
    pub(crate) qualifier_score_hiding: QualifierScoreHiding,
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum DataError {
    #[error(transparent)] PgInterval(#[from] PgIntervalDecodeError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Game(#[from] game::GameError),
    #[error("no event with this series and identifier")]
    Missing,
    #[error("team with nonexistent user")]
    NonexistentUser,
}

impl<'a> Data<'a> {
    pub(crate) async fn new(transaction: &mut Transaction<'_, Postgres>, series: Series, event: impl Into<Cow<'a, str>>) -> Result<Option<Data<'a>>, DataError> {
        let event = event.into();
        sqlx::query!(r#"SELECT
            display_name,
            short_name,
            start,
            end_time,
            url,
            challonge_community,
            speedgaming_slug,
            hide_races_tab,
            hide_teams_tab,
            teams_url,
            enter_url,
            video_url,
            discord_guild AS "discord_guild: PgSnowflake<GuildId>",
            discord_invite_url,
            discord_race_room_channel AS "discord_race_room_channel: PgSnowflake<ChannelId>",
            discord_race_results_channel AS "discord_race_results_channel: PgSnowflake<ChannelId>",
            discord_organizer_channel AS "discord_organizer_channel: PgSnowflake<ChannelId>",
            discord_scheduling_channel AS "discord_scheduling_channel: PgSnowflake<ChannelId>",
            discord_volunteer_info_channel AS "discord_volunteer_info_channel: PgSnowflake<ChannelId>",
            discord_async_channel AS "discord_async_channel: PgSnowflake<ChannelId>",
            rando_version AS "rando_version: Json<VersionedBranch>",
            settings_string,
            single_settings AS "single_settings: Json<seed::Settings>",
            team_config AS "team_config: TeamConfig",
            enter_flow AS "enter_flow: Json<enter::Flow>",
            show_opt_out,
            show_qualifier_times,
            default_game_count,
            min_schedule_notice,
            open_stream_delay,
            invitational_stream_delay,
            retime_window,
            auto_import,
            emulator_settings_reminder,
            prevent_late_joins,
            manual_reporting_with_breaks,
            language AS "language: Language",
            default_volunteer_language AS "default_volunteer_language: Language",
            asyncs_active,
            swiss_standings,
            discord_events_enabled,
            discord_events_require_restream,
            listed,
            round_modes AS "round_modes: Json<HashMap<String, String>>",
            automated_asyncs,
            volunteer_requests_enabled,
            volunteer_request_lead_time_hours,
            volunteer_request_ping_enabled,
            force_custom_role_binding,
            qualifier_score_hiding AS "qualifier_score_hiding: QualifierScoreHiding"
        FROM events WHERE series = $1 AND event = $2"#, series as _, &event).fetch_optional(&mut **transaction).await?
            .map(|row| Ok::<_, DataError>(Self {
                display_name: row.display_name,
                short_name: row.short_name,
                base_start: row.start,
                end: row.end_time,
                url: row.url.map(|url| url.parse()).transpose()?,
                challonge_community: row.challonge_community,
                speedgaming_slug: row.speedgaming_slug,
                hide_races_tab: row.hide_races_tab,
                hide_teams_tab: row.hide_teams_tab,
                teams_url: row.teams_url.map(|url| url.parse()).transpose()?,
                enter_url: row.enter_url.map(|url| url.parse()).transpose()?,
                video_url: row.video_url.map(|url| url.parse()).transpose()?,
                discord_guild: row.discord_guild.map(|PgSnowflake(id)| id),
                discord_invite_url: row.discord_invite_url.map(|url| url.parse()).transpose()?,
                discord_race_room_channel: row.discord_race_room_channel.map(|PgSnowflake(id)| id),
                discord_race_results_channel: row.discord_race_results_channel.map(|PgSnowflake(id)| id),
                discord_organizer_channel: row.discord_organizer_channel.map(|PgSnowflake(id)| id),
                discord_scheduling_channel: row.discord_scheduling_channel.map(|PgSnowflake(id)| id),
                discord_volunteer_info_channel: row.discord_volunteer_info_channel.map(|PgSnowflake(id)| id),
                discord_async_channel: row.discord_async_channel.map(|PgSnowflake(id)| id),
                rando_version: row.rando_version.map(|Json(rando_version)| rando_version),
                settings_string: row.settings_string,
                single_settings: if series == Series::CopaDoBrasil && event == "1" {
                    Some(br::s1_settings()) // support for randomized starting song
                } else {
                    row.single_settings.map(|Json(single_settings)| single_settings)
                },
                team_config: row.team_config,
                enter_flow: row.enter_flow.map(|Json(flow)| flow),
                show_opt_out: row.show_opt_out,
                show_qualifier_times: row.show_qualifier_times,
                default_game_count: row.default_game_count,
                min_schedule_notice: decode_pginterval(row.min_schedule_notice)?,
                open_stream_delay: decode_pginterval(row.open_stream_delay)?,
                invitational_stream_delay: decode_pginterval(row.invitational_stream_delay)?,
                retime_window: decode_pginterval(row.retime_window)?,
                auto_import: row.auto_import,
                emulator_settings_reminder: row.emulator_settings_reminder,
                prevent_late_joins: row.prevent_late_joins,
                manual_reporting_with_breaks: row.manual_reporting_with_breaks,
                language: row.language,
                default_volunteer_language: row.default_volunteer_language,
                asyncs_active: row.asyncs_active,
                swiss_standings: row.swiss_standings,
                discord_events_enabled: row.discord_events_enabled,
                discord_events_require_restream: row.discord_events_require_restream,
                series, event,
                listed: row.listed,
                round_modes: row.round_modes.map(|Json(round_modes)| round_modes),
                automated_asyncs: row.automated_asyncs,
                volunteer_requests_enabled: row.volunteer_requests_enabled,
                volunteer_request_lead_time_hours: row.volunteer_request_lead_time_hours,
                volunteer_request_ping_enabled: row.volunteer_request_ping_enabled,
                force_custom_role_binding: row.force_custom_role_binding.unwrap_or(true),
                qualifier_score_hiding: row.qualifier_score_hiding,
            }))
            .transpose()
    }

    pub(crate) fn short_name(&self) -> &str {
        self.short_name.as_deref().unwrap_or(&self.display_name)
    }

    /// Weights for chest appearances in Mido's house in this event, generated using <https://github.com/fenhl/ootrstats>
    pub(crate) async fn chests(&self) -> wheel::Result<ChestAppearances> {
        macro_rules! from_file {
            ($path:literal) => {{
                static WEIGHTS: LazyLock<Vec<(ChestAppearances, usize)>> = LazyLock::new(|| serde_json::from_str(include_str!($path)).expect("failed to parse chest weights"));

                WEIGHTS.choose_weighted(&mut rng(), |(_, weight)| *weight).expect("failed to choose random chest textures").0
            }};
        }
        //TODO parse weights at compile time

        Ok(match (self.series, &*self.event) {
            (Series::BattleRoyale, "1") => from_file!("../../assets/event/ohko/chests-1-8.0.json"), //TODO reroll with the plando
            (Series::CoOp, "3") => ChestAppearances::VANILLA,
            (Series::CopaDoBrasil, "1") => from_file!("../../assets/event/br/chests-1-7.1.143.json"),
            (Series::League, "4") => from_file!("../../assets/event/league/chests-4-7.1.94.json"),
            (Series::League, "5") => from_file!("../../assets/event/league/chests-4-7.1.94.json"), //TODO S5 was generated on Dev versions between 7.1.184 and 7.1.200
            (Series::League, "6") => from_file!("../../assets/event/league/chests-6-8.0.22.json"),
            (Series::League, "7") => from_file!("../../assets/event/league/chests-7-8.1.69.json"),
            (Series::League, "8") => from_file!("../../assets/event/league/chests-8-8.2.55.json"),
            (Series::MixedPools, "1") => from_file!("../../assets/event/mp/chests-1-6.2.100-fenhl.4.json"),
            (Series::MixedPools, "2") => from_file!("../../assets/event/mp/chests-2-7.1.117-fenhl.17.json"),
            (Series::MixedPools, "3") => from_file!("../../assets/event/mp/chests-3-8.1.36-fenhl.6.riir.4.json"),
            (Series::MixedPools, "4") => from_file!("../../assets/event/mp/chests-4-8.2.69-fenhl.4.riir.5.json"),
            (Series::Mq, "1") => from_file!("../../assets/event/mq/chests-1-8.2.json"),
            (Series::Multiworld, "1" | "2") => ChestAppearances::VANILLA, // CAMC off or classic and no keys in overworld
            (Series::Multiworld, "3") => mw::s3_chests(&Draft {
                high_seed: Id::dummy(), // Draft::complete_randomly doesn't check for active team
                went_first: None,
                skipped_bans: 0,
                settings: HashMap::default(),
            }.complete_randomly(draft::Kind::MultiworldS3).await.unwrap()),
            (Series::Multiworld, "4") => from_file!("../../assets/event/mw/chests-4-7.1.198.json"),
            (Series::Multiworld, "5") => from_file!("../../assets/event/mw/chests-5-8.2.63.json"),
            (Series::NineDaysOfSaws, _) => ChestAppearances::VANILLA, // no CAMC in SAWS
            (Series::Pictionary, _) => ChestAppearances::VANILLA, // no CAMC in Pictionary
            (Series::Rsl, "1") => from_file!("../../assets/event/rsl/chests-1-4c526c2.json"),
            (Series::Rsl, "2") => from_file!("../../assets/event/rsl/chests-2-7028072.json"),
            (Series::Rsl, "3") => from_file!("../../assets/event/rsl/chests-3-a0f568b.json"),
            (Series::Rsl, "4") => from_file!("../../assets/event/rsl/chests-4-da4dae5.json"),
            (Series::Rsl, "5") => {
                // rsl/5 moved from version 20cd31a of the RSL script to version 05bfcd2 after the first two races of the first Swiss round.
                // For the sake of simplicity, only the new version is used for chests weights right now.
                //TODO After the event, the version should be randomized based on the total number of races played on each version.
                from_file!("../../assets/event/rsl/chests-5-05bfcd2.json")
            }
            (Series::Rsl, "6") => from_file!("../../assets/event/rsl/chests-6-248f8b5.json"),
            (Series::Rsl, "7") => from_file!("../../assets/event/rsl/chests-7-104253e.json"), //TODO include RSL-Lite, adjust for simulated drafts
            (Series::Scrubs, "5") => from_file!("../../assets/event/scrubs/chests-5-7.1.198.json"),
            (Series::Scrubs, "6") => from_file!("../../assets/event/scrubs/chests-6-8.1.73.json"),
            (Series::SongsOfHope, "1") => from_file!("../../assets/event/soh/chests-1-8.1.json"),
            (Series::SpeedGaming, "2023onl" | "2023live") => from_file!("../../assets/event/sgl/chests-2023-42da4aa.json"),
            (Series::SpeedGaming, "2024onl" | "2024live") => from_file!("../../assets/event/sgl/chests-2024-ee4d35b.json"),
            (Series::Standard, "w") => s::weekly_chest_appearances(),
            (Series::Standard, "6") => from_file!("../../assets/event/s/chests-6-6.9.10.json"),
            (Series::Standard, "7" | "7cc") => from_file!("../../assets/event/s/chests-7-7.1.198.json"),
            (Series::Standard, "8" | "8cc") => from_file!("../../assets/event/s/chests-8-8.2.json"),
            (Series::TournoiFrancophone, "3") => from_file!("../../assets/event/fr/chests-3-7.1.83-r.1.json"),
            (Series::TournoiFrancophone, "4") => from_file!("../../assets/event/fr/chests-4-8.1.45-rob.105.json"),
            (Series::TournoiFrancophone, "5") => from_file!("../../assets/event/fr/chests-5-8.2.64-rob.135.json"),
            (Series::TriforceBlitz, "2") => from_file!("../../assets/event/tfb/chests-2-7.1.3-blitz.42.json"),
            (Series::TriforceBlitz, "3") => from_file!("../../assets/event/tfb/chests-3-8.1.32-blitz.57.json"),
            (Series::TriforceBlitz, "4coop") => from_file!("../../assets/event/tfb/chests-4coop-8.2.64-blitz.87.json"),
            (Series::WeTryToBeBetter, "1") => from_file!("../../assets/event/scrubs/chests-5-7.1.198.json"),
            (Series::WeTryToBeBetter, "2") => from_file!("../../assets/event/wttbb/chests-2-8.2.json"),
            (_series, _event) => {
                ChestAppearances::random()
            }
        })
    }

    pub(crate) fn asyncs_allowed(&self) -> bool {
        self.asyncs_active
    }

    pub(crate) fn is_single_race(&self) -> bool {
        match self.series {
            Series::AlttprDe => false,
            Series::BattleRoyale => false,
            Series::CoOp => false,
            Series::CopaDoBrasil => false,
            Series::Crosskeys => false,
            Series::League => false,
            Series::MixedPools => false,
            Series::Mq => false,
            Series::Multiworld => false,
            Series::MysteryD => false,
            Series::NineDaysOfSaws => true,
            Series::Pictionary => true,
            Series::Rsl => false,
            Series::Scrubs => false,
            Series::SongsOfHope => false,
            Series::SpeedGaming => false,
            Series::Standard => false,
            Series::TournoiFrancophone => false,
            Series::TriforceBlitz => false,
            Series::WeTryToBeBetter => false,
            Series::TwwrMain => false,
        }
    }

    pub(crate) fn match_source(&self) -> MatchSource<'_> {
        if let Some(ref url) = self.url {
            match url.host_str() {
                Some("challonge.com" | "www.challonge.com") => MatchSource::Challonge {
                    community: self.challonge_community.as_deref(),
                    tournament: &url.path()[1..],
                },
                Some("league.ootrandomizer.com") => MatchSource::League,
                Some("start.gg" | "www.start.gg") => MatchSource::StartGG(&url.path()[1..]),
                _ => MatchSource::Manual,
            }
        } else {
            MatchSource::Manual
        }
    }

    pub(crate) async fn qualifier_kind(&self, transaction: &mut Transaction<'_, Postgres>, me: Option<&User>) -> Result<QualifierKind, DataError> {
        Ok(match (self.series, &*self.event) {
            (Series::SongsOfHope, "1") => QualifierKind::SongsOfHope,
            (Series::SpeedGaming, "2023onl" | "2024onl" | "2025onl") | (Series::Standard, "8") | (Series::TwwrMain, "miniblins26") => {
                QualifierKind::Score(match (self.series, &*self.event) {
                    (Series::SpeedGaming, "2023onl") => teams::QualifierScoreKind::Sgl2023Online,
                    (Series::SpeedGaming, "2024onl") => teams::QualifierScoreKind::Sgl2024Online,
                    (Series::SpeedGaming, "2025onl") => teams::QualifierScoreKind::Sgl2025Online,
                    (Series::Standard, "8") => teams::QualifierScoreKind::Standard,
                    (Series::TwwrMain, "miniblins26") => teams::QualifierScoreKind::TwwrMiniblins26,
                    _ => unreachable!("checked by outer match"),
                })
            }
            (_, _) => if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE series = $1 AND event = $2 AND qualifier_rank IS NOT NULL) AS "exists!""#, self.series as _, &*self.event).fetch_one(&mut **transaction).await? {
                QualifierKind::Rank
            } else if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM asyncs WHERE series = $1 AND event = $2 AND kind = 'qualifier') AS "exists!""#, self.series as _, &*self.event).fetch_one(&mut **transaction).await? {
                QualifierKind::Single {
                    show_times: self.show_qualifier_times && (
                        sqlx::query_scalar!(r#"SELECT submitted IS NOT NULL AS "qualified!" FROM teams, async_teams, team_members WHERE async_teams.team = teams.id AND teams.series = $1 AND teams.event = $2 AND async_teams.team = team_members.team AND member = $3 AND kind = 'qualifier'"#, self.series as _, &*self.event, me.map(|me| PgSnowflake(me.id)) as _).fetch_optional(&mut **transaction).await?.unwrap_or(false)
                        || self.is_started(transaction).await?
                    ),
                }
            } else {
                QualifierKind::None
            },
        })
    }

    pub(crate) fn draft_kind(&self) -> Option<draft::Kind> {
        match (self.series, &*self.event) {
            // AlttprDe events: only need draft if round_modes is not set
            (Series::AlttprDe, "9bracket" | "9swissa" | "9swissb") => {
                if self.round_modes.is_some() {
                    None // Mode is fixed per round, no draft needed
                } else {
                    Some(draft::Kind::AlttprDe9)
                }
            }
            (Series::Multiworld, "3") => Some(draft::Kind::MultiworldS3),
            (Series::Multiworld, "4") => Some(draft::Kind::MultiworldS4),
            (Series::Multiworld, "5") => Some(draft::Kind::MultiworldS5),
            (Series::Rsl, "7") => Some(draft::Kind::RslS7),
            (Series::Standard, "7" | "7cc") => Some(draft::Kind::S7),
            (Series::TournoiFrancophone, "3") => Some(draft::Kind::TournoiFrancoS3),
            (Series::TournoiFrancophone, "4") => Some(draft::Kind::TournoiFrancoS4),
            (Series::TournoiFrancophone, "5") => Some(draft::Kind::TournoiFrancoS5),
            (_, _) => None,
        }
    }

    pub(crate) async fn start(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Option<DateTime<Utc>>, DataError> {
        Ok(if let Some(mut start) = self.base_start {
            if let Some(max_delay) = sqlx::query_scalar!("SELECT max_delay FROM asyncs WHERE series = $1 AND event = $2 AND kind = 'qualifier'", self.series as _, &self.event).fetch_optional(&mut **transaction).await? {
                let mut num_qualified_teams = 0;
                let mut last_submission_time = None::<DateTime<Utc>>;
                let mut teams = sqlx::query_scalar!(r#"SELECT submitted AS "submitted!" FROM teams LEFT OUTER JOIN async_teams ON (id = team) WHERE
                    series = $1
                    AND event = $2
                    AND NOT resigned
                    AND submitted IS NOT NULL
                    AND kind = 'qualifier'
                "#, self.series as _, &self.event).fetch(&mut **transaction);
                while let Some(submitted) = teams.try_next().await? {
                    num_qualified_teams += 1;
                    last_submission_time = Some(if let Some(last_submission_time) = last_submission_time {
                        last_submission_time.max(submitted)
                    } else {
                        submitted
                    });
                }
                if num_qualified_teams % 2 == 0 {
                    if let Some(last_submission_time) = last_submission_time {
                        start = start.max(last_submission_time);
                    }
                } else {
                    if start <= Utc::now() {
                        start += TimeDelta::from_std(decode_pginterval(max_delay)?).expect("max delay on async too long");
                    }
                }
            }
            Some(start)
        } else {
            None
        })
    }

    pub(crate) async fn is_started(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<bool, DataError> {
        Ok(self.start(transaction).await?.is_some_and(|start| start <= Utc::now()))
    }

    fn is_ended(&self) -> bool {
        self.end.is_some_and(|end| end <= Utc::now())
    }

    #[allow(dead_code)]
    pub(crate) async fn game(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Option<game::Game>, DataError> {
        game::Game::from_series(transaction, self.series).await.map_err(DataError::from)
    }

    pub(crate) async fn organizers(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<User>, Error> {
        let mut buf = Vec::<User>::default();
        for id in sqlx::query_scalar!(r#"SELECT organizer AS "organizer: Id<Users>" FROM organizers WHERE series = $1 AND event = $2"#, self.series as _, &self.event).fetch_all(&mut **transaction).await? {
            let user = User::from_id(&mut **transaction, id).await?.ok_or(Error::OrganizerUserData)?;
            let (Ok(idx) | Err(idx)) = buf.binary_search_by(|probe| probe.display_name().cmp(user.display_name()).then_with(|| probe.id.cmp(&user.id)));
            buf.insert(idx, user);
        }
        Ok(buf)
    }

    pub(crate) async fn restreamers(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<User>, Error> {
        let mut buf = Vec::<User>::default();
        for id in sqlx::query_scalar!(r#"SELECT restreamer AS "restreamer: Id<Users>" FROM restreamers WHERE series = $1 AND event = $2"#, self.series as _, &self.event).fetch_all(&mut **transaction).await? {
            let user = User::from_id(&mut **transaction, id).await?.ok_or(Error::RestreamerUserData)?;
            let (Ok(idx) | Err(idx)) = buf.binary_search_by(|probe| probe.display_name().cmp(user.display_name()).then_with(|| probe.id.cmp(&user.id)));
            buf.insert(idx, user);
        }
        Ok(buf)
    }

    pub(crate) async fn active_async(&self, transaction: &mut Transaction<'_, Postgres>, team_id: Option<Id<Teams>>) -> Result<Option<AsyncKind>, DataError> {
        for kind in sqlx::query_scalar!(r#"SELECT kind AS "kind: AsyncKind" FROM asyncs WHERE series = $1 AND event = $2 AND (start IS NULL OR start <= NOW()) AND (end_time IS NULL OR end_time > NOW())"#, self.series as _, &self.event).fetch_all(&mut **transaction).await? {
            match kind {
                AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 => if !self.is_started(&mut *transaction).await? {
                    // Skip qualifiers the team has already submitted
                    if let Some(team_id) = team_id {
                        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM async_teams WHERE team = $1 AND kind = $2 AND submitted IS NOT NULL) AS "submitted!""#, team_id as _, kind as _).fetch_one(&mut **transaction).await? {
                            continue;
                        }
                    }
                    return Ok(Some(kind))
                },
                AsyncKind::Seeding => return Ok(Some(kind)),
                AsyncKind::Tiebreaker1 | AsyncKind::Tiebreaker2 => if let Some(team_id) = team_id {
                    if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM async_teams WHERE team = $1 AND kind = $2) AS "exists!""#, team_id as _, kind as _).fetch_one(&mut **transaction).await? {
                        return Ok(Some(kind))
                    }
                },
            }
        }
        Ok(None)
    }

    pub(crate) async fn has_role_bindings(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<bool, Error> {
        // Check for event-specific role bindings
        let event_count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM role_bindings WHERE series = $1 AND event = $2"#,
            self.series as _,
            &self.event
        )
        .fetch_one(&mut **transaction)
        .await?
        .unwrap_or(0);

        if event_count > 0 {
            return Ok(true);
        }

        // Check if event uses custom bindings
        let uses_custom_bindings = sqlx::query_scalar!(
            r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
            self.series as _,
            &self.event
        )
        .fetch_optional(&mut **transaction)
        .await?
        .unwrap_or(Some(true))
        .unwrap_or(true);

        // If using custom bindings, we already checked event bindings above
        if uses_custom_bindings {
            return Ok(false);
        }

        // Check for game-level role bindings
        let game = game::Game::from_series(&mut *transaction, self.series).await?;
        if let Some(game) = game {
            let game_count = sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM role_bindings WHERE game_id = $1"#,
                game.id
            )
            .fetch_one(&mut **transaction)
            .await?
            .unwrap_or(0);
            Ok(game_count > 0)
        } else {
            Ok(false)
        }
    }

    /// Returns Swiss standings for this event, if it's a Startgg event
    pub(crate) async fn swiss_standings(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, config: &Config) -> Result<Option<Vec<startgg::SwissStanding>>, Error> {
        if !matches!(self.match_source(), MatchSource::StartGG(_)) {
            return Ok(None);
        }

        // Extract the Startgg slug from the event URL
        let slug = match self.url.as_ref().and_then(|url| url.path().strip_prefix('/').map(|s| s.to_string())) {
            Some(s) if !s.is_empty() => s,
            _ => return Ok(None),
        };

        // Get the Startgg token
        let startgg_token = &config.startgg;

        // Get resigned teams for this event to exclude them from bye prediction
        let resigned_entrant_ids = sqlx::query!(
            r#"SELECT startgg_id FROM teams 
               WHERE series = $1 AND event = $2 AND resigned = TRUE AND startgg_id IS NOT NULL"#,
            self.series as _,
            &self.event
        )
        .fetch_all(&mut **transaction)
        .await
        .ok()
        .map(|rows| rows.into_iter()
            .filter_map(|row| row.startgg_id)
            .map(|id| id.to_string())
            .collect::<HashSet<_>>());

        // Fetch Swiss standings
        match startgg::swiss_standings(http_client, config, &slug, startgg_token, resigned_entrant_ids.as_ref()).await {
            Ok(standings) => Ok(Some(standings)),
            Err(startgg::Error::GraphQL(errors)) => {
                // Check if it's a query complexity error
                if errors.iter().any(|e| e.message.contains("query complexity is too high")) {
                    log::warn!("Startgg API query complexity too high for event {}", slug);
                }
                Ok(None) // Return None if API call fails
            },
            Err(_) => Ok(None), // Return None for other errors
        }
    }

    pub(crate) async fn header(&self, transaction: &mut Transaction<'_, Postgres>, me: Option<&User>, tab: Tab, is_subpage: bool) -> Result<RawHtml<String>, Error> {
        let signed_up = if let Some(me) = me {
            sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
                id = team
                AND series = $1
                AND event = $2
                AND member = $3
                AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
            ) AS "exists!""#, self.series as _, &self.event, me.id as _).fetch_one(&mut **transaction).await?
        } else {
            false
        };
        let has_zsr_backends = sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM zsr_restreaming_backends) AS "exists!""#)
            .fetch_one(&mut **transaction).await?;
        Ok(html! {
            h1 {
                a(class = "nav", href? = (!matches!(tab, Tab::Info) || is_subpage).then(|| uri!(info(self.series, &*self.event)))) : &self.display_name;
            }
            @if let Some(start) = self.start(&mut *transaction).await? {
                h4 {
                    : "Event Start: ";
                    : start.format("%Y-%m-%d").to_string();
                }
                p(class = "timezone-info") {
                    : timezone_info_html();
                }
            }
            div(class = "button-row") {
                @if let Tab::Info = tab {
                    a(class = "button selected", href? = is_subpage.then(|| uri!(info(self.series, &*self.event)))) : "Info";
                } else {
                    a(class = "button", href = uri!(info(self.series, &*self.event))) : "Info";
                }
                @let teams_label = if let TeamConfig::Solo = self.team_config { "Entrants" } else { "Teams" };
                @if !self.hide_teams_tab {
                    @if let Tab::Teams = tab {
                        a(class = "button selected", href? = is_subpage.then(|| uri!(teams::get(self.series, &*self.event)))) : teams_label;
                    } else if let Some(ref teams_url) = self.teams_url {
                        a(class = "button", href = teams_url.to_string()) {
                            : favicon(teams_url);
                            : teams_label;
                        }
                    } else {
                        a(class = "button", href = uri!(teams::get(self.series, &*self.event))) : teams_label;
                    }
                }
                @if !self.hide_races_tab && !self.is_single_race() {
                    @if let Tab::Races = tab {
                        a(class = "button selected", href? = is_subpage.then(|| uri!(races(self.series, &*self.event)))) : "Races";
                    } else {
                        a(class = "button", href = uri!(races(self.series, &*self.event))) : "Races";
                    }
                }
                @if matches!(self.match_source(), MatchSource::StartGG(_)) && self.swiss_standings {
                    @if let Tab::SwissStandings = tab {
                        a(class = "button selected", href? = is_subpage.then(|| uri!(swiss_standings(self.series, &*self.event)))) : "Swiss Standings";
                    } else {
                        a(class = "button", href = uri!(swiss_standings(self.series, &*self.event))) : "Swiss Standings";
                    }
                }
                @if signed_up {
                    @if let Tab::MyStatus = tab {
                        a(class = "button selected", href? = is_subpage.then(|| uri!(status(self.series, &*self.event)))) : "My Status";
                    } else {
                        a(class = "button", href = uri!(status(self.series, &*self.event))) : "My Status";
                    }
                } else if !self.is_started(&mut *transaction).await? {
                    @if let Tab::Enter = tab {
                        a(class = "button selected", href? = is_subpage.then(|| uri!(enter::get(self.series, &*self.event, _, _)))) : "Enter";
                    } else if let Some(ref enter_url) = self.enter_url {
                        a(class = "button", href = enter_url.to_string()) {
                            : favicon(enter_url);
                            : "Enter";
                        }
                    } else {
                        a(class = "button", href = uri!(enter::get(self.series, &*self.event, _, _))) : "Enter";
                    }
                    @if !matches!(self.team_config, TeamConfig::Solo) {
                        @if let Tab::FindTeam = tab {
                            a(class = "button selected", href? = is_subpage.then(|| uri!(find_team(self.series, &*self.event)))) : "Find Teammates";
                        } else {
                            a(class = "button", href = uri!(find_team(self.series, &*self.event))) : "Find Teammates";
                        }
                    }
                }
                @let is_ootr = self.game(&mut *transaction).await?.map(|g| g.name == "ootr").unwrap_or(false);
                @let practice_seed_url = (is_ootr && self.single_settings.is_some()).then(|| uri!(practice_seed(self.series, &*self.event)));
                @let practice_race_url = if_chain! {
                    if is_ootr;
                    if let Some(goal) = racetime_bot::Goal::for_event(self.series, &self.event);
                    if goal.is_custom(); //TODO also support non-custom goals, see https://github.com/racetimeGG/racetime-app/issues/215
                    then {
                        let mut practice_url = Url::parse(&format!("https://{}/{}/startrace", racetime_host(), racetime_bot::CATEGORY))?;
                        practice_url
                            .query_pairs_mut()
                            .append_pair(if goal.is_custom() { "custom_goal" } else { "goal" }, goal.as_str())
                            .extend_pairs(self.team_config.is_racetime_team_format().then_some([("team_race", "1"), ("require_even_teams", "1")]).into_iter().flatten())
                            .append_pair("hide_comments", "1")
                            .finish();
                        Some(practice_url)
                    } else {
                        None
                    }
                };
                @let practice_seed_button = practice_seed_url.map(|url| html! {
                    a(class = "button", href = url, target = "_blank") {
                        : favicon(&Url::parse("https://ootrandomizer.com/").unwrap()); //TODO adjust based on seed host
                        @if practice_race_url.is_some() {
                            : "Roll Seed";
                        } else {
                            : "Practice";
                        }
                    }
                });
                @let practice_race_button = practice_race_url.map(|url| html! {
                    a(class = "button", href = url.to_string(), target = "_blank") {
                        : favicon(&url);
                        @if practice_seed_button.is_some() {
                            : "Start Race";
                        } else {
                            : "Practice";
                        }
                    }
                });
                @match (practice_seed_button, practice_race_button) {
                    (None, None) => {}
                    (None, Some(button)) | (Some(button), None) => : button;
                    (Some(practice_seed_button), Some(practice_race_button)) => div(class = "popover-wrapper") {
                        div(id = "practice-menu", popover); //HACK workaround for lack of cross-browser support for CSS overlay property
                        div(class = "menu") {
                            : practice_seed_button;
                            : practice_race_button;
                        }
                        button(popovertarget = "practice-menu") : "Practice ⯆";
                    }
                }
                @if self.has_role_bindings(transaction).await? && !self.is_ended() {
                    @if let Tab::Volunteer = tab {
                        a(class = "button selected", href? = is_subpage.then(|| uri!(roles::volunteer_page_get(self.series, &*self.event, _)))) : "Volunteer";
                    } else {
                        a(class = "button", href = uri!(roles::volunteer_page_get(self.series, &*self.event, _))) : "Volunteer";
                    }
                }
                @if let Some(ref video_url) = self.video_url {
                    a(class = "button", href = video_url.to_string(), target = "_blank") {
                        : favicon(video_url);
                        : "Watch";
                    }
                }
                @if let Some(ref url) = self.url {
                    a(class = "button", href = url.to_string(), target = "_blank") {
                        : favicon(url);
                        @match url.host_str() {
                            Some("racetime.gg" | "racetime.midos.house") => : "Race Room";
                            Some("challonge.com" | "www.challonge.com" | "start.gg" | "www.start.gg") => : "Brackets";
                            _ => : "Website";
                        }
                    }
                }
                @if let Some(ref discord_invite_url) = self.discord_invite_url {
                    a(class = "button", href = discord_invite_url.to_string(), target = "_blank") {
                        : favicon(discord_invite_url);
                        : "Discord Server";
                    }
                }
                @if let Some(me) = me {
                    @let is_organizer_or_global = self.organizers(transaction).await?.contains(me) || me.is_global_admin();
                    @let is_game_admin = if let Some(game) = self.game(&mut *transaction).await? { game.is_admin(&mut *transaction, me).await.map_err(Error::from)? } else { false };
                    @if !self.is_ended() && (is_organizer_or_global || is_game_admin) {
                        @if let Tab::Configure = tab {
                            a(class = "button selected", href? = is_subpage.then(|| uri!(configure::get(self.series, &*self.event)))) : "Configure";
                        } else {
                            a(class = "button", href = uri!(configure::get(self.series, &*self.event))) : "Configure";
                        }
                    }
                    @if !self.is_ended() && is_organizer_or_global {
                        @if let Tab::Roles = tab {
                            a(class = "button selected", href? = is_subpage.then(|| uri!(roles::get(self.series, &*self.event, _, _)))) : "Volunteer Setup";
                        } else {
                            a(class = "button", href = uri!(roles::get(self.series, &*self.event, _, _))) : "Volunteer Setup";
                        }
                        @if self.asyncs_active {
                            @if let Tab::Asyncs = tab {
                                a(class = "button selected", href? = is_subpage.then(|| uri!(asyncs::get(self.series, &*self.event, None::<String>)))) : "Asyncs";
                            } else {
                                a(class = "button", href = uri!(asyncs::get(self.series, &*self.event, None::<String>))) : "Asyncs";
                            }
                        }
                        @if let Tab::Qualifiers = tab {
                            a(class = "button selected", href? = is_subpage.then(|| uri!(qualifiers::get(self.series, &*self.event)))) : "Qualifiers";
                        } else {
                            a(class = "button", href = uri!(qualifiers::get(self.series, &*self.event))) : "Qualifiers";
                        }
                    }
                    @if !self.is_ended() && me.is_global_admin() {
                        @if let Tab::Setup = tab {
                            a(class = "button selected", href? = is_subpage.then(|| uri!(setup::get(self.series, &*self.event)))) : "Setup";
                        } else {
                            a(class = "button", href = uri!(setup::get(self.series, &*self.event))) : "Setup";
                        }
                        @if has_zsr_backends {
                            @if let Tab::ZsrExport = tab {
                                a(class = "button selected", href? = is_subpage.then(|| uri!(zsr_export::get(self.series, &*self.event)))) : "ZSR Export";
                            } else {
                                a(class = "button", href = uri!(zsr_export::get(self.series, &*self.event))) : "ZSR Export";
                            }
                        }
                    }
                }
            }
        })
    }
}

impl ToHtml for Data<'_> {
    fn to_html(&self) -> RawHtml<String> {
        html! {
            a(href = uri!(info(self.series, &*self.event))) {
                bdi : self.display_name;
            }
        }
    }
}

pub(crate) enum Tab {
    Info,
    Teams,
    Races,
    MyStatus,
    Enter,
    FindTeam,
    Volunteer,
    Configure,
    Roles,
    SwissStandings,
    Setup,
    Asyncs,
    Qualifiers,
    ZsrExport,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Calendar(#[from] cal::Error),
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Discord(#[from] crate::discord_bot::Error),
    #[error(transparent)] Game(#[from] game::GameError),
    #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] OotrWeb(#[from] ootr_web::Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] SeedData(#[from] seed::ExtraDataError),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("missing user data for an event organizer")]
    OrganizerUserData,
    #[error("missing user data for a restreamer")]
    RestreamerUserData,
}

impl<E: Into<Error>> From<E> for StatusOrError<Error> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Calendar(e) => e.is_network_error(),
            Self::Data(_) => false,
            Self::Discord(_) => false,
            Self::Game(_) => false,
            Self::Io(e) => e.is_network_error(),
            Self::Json(_) => false,
            Self::OotrWeb(e) => e.is_network_error(),
            Self::Page(e) => e.is_network_error(),
            Self::Reqwest(e) => e.is_network_error(),
            Self::SeedData(e) => e.is_network_error(),
            Self::Serenity(_) => false,
            Self::Sql(_) => false,
            Self::Url(_) => false,
            Self::Wheel(e) => e.is_network_error(),
            Self::OrganizerUserData => false,
            Self::RestreamerUserData => false,
        }
    }
}

impl<'r> rocket::response::Responder<'r, 'static> for Error {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'static> {
        let status = if self.is_network_error() {
            Status::BadGateway //TODO different status codes (e.g. GatewayTimeout for timeout errors)?
        } else {
            Status::InternalServerError
        };
        eprintln!("responded with {status} to request to {}", request.uri());
        eprintln!("display: {self}");
        eprintln!("debug: {self:?}");
        Err(status)
    }
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum InfoError {
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Event(#[from] Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] SeedData(#[from] seed::ExtraDataError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
}

impl<E: Into<InfoError>> From<E> for StatusOrError<InfoError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

#[rocket::get("/event/<series>/<event>")]
pub(crate) async fn info(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<InfoError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let header = data.header(&mut transaction, me.as_ref(), Tab::Info, false).await?;
    let content = match data.series {
        Series::AlttprDe => alttprde::info(&mut transaction, &data).await?,
        Series::BattleRoyale => ohko::info(&mut transaction, &data).await?,
        Series::CoOp => coop::info(&mut transaction, &data).await?,
        Series::CopaDoBrasil => br::info(&mut transaction, &data).await?,
        Series::Crosskeys => xkeys::info(&mut transaction, &data).await?,
        Series::League => league::info(&mut transaction, &data).await?,
        Series::MixedPools => mp::info(&mut transaction, &data).await?,
        Series::Mq => None,
        Series::Multiworld => mw::info(&mut transaction, &data).await?,
        Series::MysteryD => mysteryd::info(&mut transaction, &data).await?,
        Series::NineDaysOfSaws => Some(ndos::info(&mut transaction, &data).await?),
        Series::Pictionary => pic::info(&mut transaction, &data).await?,
        Series::Rsl => rsl::info(&mut transaction, &data).await?,
        Series::Scrubs => scrubs::info(&mut transaction, &data).await?,
        Series::SongsOfHope => soh::info(&mut transaction, &data).await?,
        Series::SpeedGaming => sgl::info(&mut transaction, &data).await?,
        Series::Standard => s::info(&mut transaction, &data).await?,
        Series::TournoiFrancophone => fr::info(&mut transaction, &data).await?,
        Series::TriforceBlitz => tfb::info(&mut transaction, &data).await?,
        Series::TwwrMain => twwrmain::info(&mut transaction, &data).await?,
        Series::WeTryToBeBetter => wttbb::info(&mut transaction, &data).await?,
    };
    let content = html! {
        : header;
        @if let Some(content) = content {
            : content;
        } else if let Some(organizers) = English.join_html_opt(data.organizers(&mut transaction).await?) {
            article {
                p {
                    : "This event ";
                    @if data.is_ended() {
                        : "was";
                    } else {
                        : "is";
                    }
                    : " organized by ";
                    : organizers;
                    : ".";
                }
            }
        } else {
            article {
                p : "No information about this event available yet.";
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &data.display_name, content).await?)
}

#[rocket::get("/event/<series>/<event>/races")]
pub(crate) async fn races(discord_ctx: &State<RwFuture<DiscordCtx>>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: Option<User>, uri: Origin<'_>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let header = data.header(&mut transaction, me.as_ref(), Tab::Races, false).await?;
    let (mut past_races, ongoing_and_upcoming_races) = Race::for_event(&mut transaction, http_client, &data).await?
        .into_iter()
        .partition::<Vec<_>, _>(|race| race.is_ended());
    past_races.reverse();
    let any_races_ongoing_or_upcoming = !ongoing_and_upcoming_races.is_empty();
    let (can_create, show_restream_consent, can_edit) = if let Some(ref me) = me {
        let is_organizer = data.organizers(&mut transaction).await?.contains(me);
        let can_create = is_organizer && match data.match_source() {
            MatchSource::League => false,
            MatchSource::Manual | MatchSource::Challonge { .. } | MatchSource::StartGG(_) => true,
        };
        let show_restream_consent = is_organizer || data.restreamers(&mut transaction).await?.contains(me);
        let can_edit = show_restream_consent || me.is_archivist;
        (can_create, show_restream_consent, can_edit)
    } else {
        (false, false, false)
    };
    let content = html! {
        : header;
        //TODO copiable calendar link (with link to index for explanation?)
        @if any_races_ongoing_or_upcoming {
            //TODO split into ongoing and upcoming, show headers for both
            @if let Some(ref me) = me {
               @let my_approved_roles = {
                   // Check if event uses custom role bindings
                   let uses_custom_bindings = sqlx::query_scalar!(
                       r#"SELECT force_custom_role_binding FROM events WHERE series = $1 AND event = $2"#,
                       data.series as _,
                       &data.event
                   )
                   .fetch_optional(&mut *transaction)
                   .await?
                   .unwrap_or(Some(true)).unwrap_or(true);

                   if uses_custom_bindings {
                       // For custom bindings, show event-specific approved roles
                       roles::RoleRequest::for_event(&mut transaction, data.series, &data.event).await
                           .map(|reqs| reqs.into_iter().filter(|req| req.user_id == me.id && matches!(req.status, roles::RoleRequestStatus::Approved)).map(|req| req.role_binding_id).collect::<Vec<_>>())
                           .unwrap_or_default()
                   } else {
                       // For game bindings, show all approved roles for the user
                       roles::RoleRequest::for_user(&mut transaction, me.id).await
                           .map(|reqs| reqs.into_iter().filter(|req| matches!(req.status, roles::RoleRequestStatus::Approved)).map(|req| req.role_binding_id).collect::<Vec<_>>())
                           .unwrap_or_default()
                   }
               };
                : cal::race_table(&mut transaction, &*discord_ctx.read().await, http_client, &uri, Some(&data), cal::RaceTableOptions { game_count: false, show_multistreams: true, can_create, can_edit, show_restream_consent, challonge_import_ctx: None }, &ongoing_and_upcoming_races, Some(me), Some(&my_approved_roles)).await?;
            } else {
                : cal::race_table(&mut transaction, &*discord_ctx.read().await, http_client, &uri, Some(&data), cal::RaceTableOptions { game_count: false, show_multistreams: true, can_create, can_edit, show_restream_consent, challonge_import_ctx: None }, &ongoing_and_upcoming_races, None, None).await?;
            }
        }
        @if !past_races.is_empty() {
            @if any_races_ongoing_or_upcoming {
                h2 : "Past races";
            }
            : cal::race_table(&mut transaction, &*discord_ctx.read().await, http_client, &uri, Some(&data), cal::RaceTableOptions { game_count: false, show_multistreams: false, can_create: can_create && !any_races_ongoing_or_upcoming, can_edit, show_restream_consent: false, challonge_import_ctx: None }, &past_races, None, None).await?;
        } else if can_create && !any_races_ongoing_or_upcoming {
            div(class = "button-row") {
                @match data.match_source() {
                    MatchSource::Manual | MatchSource::Challonge { .. } => a(class = "button", href = uri!(crate::cal::create_race(series, event, _))) : "New Race";
                    //MatchSource::Challonge { .. } => a(class = "button", href = uri!(crate::cal::import_races(series, event))) : "Import"; // disabled due to Challonge pagination bug
                    MatchSource::League => {}
                    MatchSource::StartGG(_) => @if !data.auto_import {
                        a(class = "button", href = uri!(crate::cal::import_races(series, event))) : "Import";
                    }
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Races — {}", data.display_name), content).await?)
}

pub(crate) enum StatusContext<'v> {
    None,
    RequestAsync(Context<'v>),
    SubmitAsync(Context<'v>),
    Edit(Context<'v>),
}

impl<'v> StatusContext<'v> {
    pub(crate) fn take_request_async(&mut self) -> Context<'v> {
        match mem::replace(self, Self::None) {
            Self::RequestAsync(ctx) => ctx,
            old_val => {
                *self = old_val;
                Context::default()
            }
        }
    }

    pub(crate) fn take_submit_async(&mut self) -> Context<'v> {
        match mem::replace(self, Self::None) {
            Self::SubmitAsync(ctx) => ctx,
            old_val => {
                *self = old_val;
                Context::default()
            }
        }
    }
    fn take_edit(&mut self) -> Context<'v> {
        match mem::replace(self, Self::None) {
            Self::Edit(ctx) => ctx,
            old_val => {
                *self = old_val;
                Context::default()
            }
        }
    }
}

async fn status_page(mut transaction: Transaction<'_, Postgres>, http_client: &reqwest::Client, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, mut ctx: StatusContext<'_>) -> Result<RawHtml<String>, Error> {
    let header = data.header(&mut transaction, me.as_ref(), Tab::MyStatus, false).await?;
    let content = if let Some(ref me) = me {
        if let Some(row) = sqlx::query!(r#"SELECT id AS "id: Id<Teams>", name, racetime_slug, role AS "role: Role", resigned, restream_consent, custom_choices AS "custom_choices: Json<HashMap<String, String>>" FROM teams, team_members WHERE
            id = team
            AND series = $1
            AND event = $2
            AND member = $3
            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        "#, data.series as _, &data.event, me.id as _).fetch_optional(&mut *transaction).await? {
            html! {
                : header;
                @if !matches!(data.team_config, TeamConfig::Solo) {
                    p {
                        : "You are signed up as part of ";
                        //TODO use Team type
                        @if let Some(racetime_slug) = row.racetime_slug {
                            a(href = format!("https://{}/team/{racetime_slug}", racetime_host())) {
                                @if let Some(name) = row.name {
                                    i {
                                        bdi : name;
                                    }
                                } else {
                                    : "an unnamed team";
                                }
                            }
                        } else {
                            @if let Some(name) = row.name {
                                i {
                                    bdi : name;
                                }
                            } else {
                                : "an unnamed team";
                            }
                        }
                        //TODO list teammates
                        : ".";
                    }
                }
                @if row.resigned {
                    p : "You have resigned from this event.";
                } else {
                    @let qualifier_progress = {
                        let qualifier_kind = data.qualifier_kind(&mut transaction, Some(me)).await?;
                        if let QualifierKind::Score(score_kind) = qualifier_kind {
                            let live_qualifier_count = usize::try_from(sqlx::query_scalar!(
                                r#"SELECT COUNT(*) FROM races WHERE series = $1 AND event = $2 AND phase = 'Qualifier'"#,
                                data.series as _,
                                &data.event
                            ).fetch_one(&mut *transaction).await?.unwrap_or(0)).unwrap_or_default();
                            let async_qualifier_count = usize::try_from(sqlx::query_scalar!(
                                r#"SELECT COUNT(*) FROM asyncs WHERE series = $1 AND event = $2 AND kind IN ('qualifier', 'qualifier2', 'qualifier3')"#,
                                data.series as _,
                                &data.event
                            ).fetch_one(&mut *transaction).await?.unwrap_or(0)).unwrap_or_default();
                            let mut qualifier_data = data.clone();
                            qualifier_data.qualifier_score_hiding = QualifierScoreHiding::None;
                            let signups = teams::signups_sorted(&mut transaction, &mut teams::Cache::new(http_client.clone()), None, &qualifier_data, false, qualifier_kind, None, true).await?;
                            signups.iter().find_map(|teams::SignupsTeam { team, qualification, .. }| {
                                if team.as_ref().is_some_and(|team| team.id == row.id) {
                                    if let teams::Qualification::Multiple { num_entered, num_finished, score, .. } = qualification {
                                        Some((
                                            *num_entered,
                                            *num_finished,
                                            score_kind.max_qualifiers_that_count(),
                                            score_kind.required_qualifiers(),
                                            *score,
                                            live_qualifier_count,
                                            async_qualifier_count,
                                        ))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    };
                    @if let Some((num_entered, num_finished, max_qualifiers, required_qualifiers, score, live_qualifier_count, async_qualifier_count)) = qualifier_progress {
                        h3 : "Qualifier Progress";
                        @let total_qualifier_count = live_qualifier_count + async_qualifier_count;
                        div(class = "bg-surface") {
                            p {
                                : "You need to finish at least ";
                                : required_qualifiers;
                                : " qualifiers to qualify. ";
                                @if max_qualifiers == usize::MAX {
                                    : "All entered qualifiers count toward scoring.";
                                } else {
                                    : "Only your first ";
                                    : max_qualifiers;
                                    : " entered qualifiers count toward scoring.";
                                }
                            }
                            p {
                                : "Qualifier opportunities: ";
                                : total_qualifier_count;
                                : " total (";
                                : live_qualifier_count;
                                : " live/sync, ";
                                : async_qualifier_count;
                                : " async).";
                            }
                            p {
                                : "Qualifiers entered: ";
                                : num_entered;
                                @if max_qualifiers != usize::MAX {
                                    : " / ";
                                    : max_qualifiers;
                                }
                            }
                            p {
                                : "Required Qualifiers finished: ";
                                : num_finished;
                                : " / ";
                                : required_qualifiers;
                            }
                            @if data.qualifier_score_hiding == QualifierScoreHiding::None {
                                p : format!("Qualifier points: {score:.2}");
                            }
                        }
                    }
                    @let async_info = if let Some(async_kind) = data.active_async(&mut transaction, Some(row.id)).await? {
                        let async_row = sqlx::query!(r#"SELECT is_tfb_dev, tfb_uuid, xkeys_uuid, web_id, web_gen_time, file_stem, hash1, hash2, hash3, hash4, hash5, seed_password, seed_data FROM asyncs WHERE series = $1 AND event = $2 AND kind = $3"#, data.series as _, &data.event, async_kind as _).fetch_one(&mut *transaction).await?;
                        if let Some(team_row) = sqlx::query!(r#"SELECT requested AS "requested!", submitted, discord_thread FROM async_teams WHERE team = $1 AND KIND = $2 AND requested IS NOT NULL"#, row.id as _, async_kind as _).fetch_optional(&mut *transaction).await? {
                            if team_row.submitted.is_some() {
                                None
                            } else if let Some(thread_id) = team_row.discord_thread {
                                Some(html! {
                                    h3 : "Async";
                                    div(class = "bg-surface") {
                                        p {
                                            : "You requested an async on ";
                                            : format_datetime(team_row.requested, DateTimeFormat { long: true, running_text: true });
                                            : ".";
                                        }
                                        p {
                                            : "Your async qualifier is being handled via Discord. ";
                                            a(href = format!("https://discord.com/channels/{}/{}",
                                                data.discord_guild.map(|g| g.get()).unwrap_or(0),
                                                thread_id)) : "Open Thread";
                                        }
                                    }
                                })
                            } else if data.automated_asyncs {
                                Some(html! {
                                    h3 : "Async";
                                    div(class = "bg-surface") {
                                        p : "Your async request has been received. A Discord thread will be created for you shortly.";
                                    }
                                })
                            } else {
                                let seed = seed::Data::from_db(
                                    None,
                                    None,
                                    None,
                                    None,
                                    async_row.file_stem,
                                    None,
                                    async_row.web_id,
                                    async_row.web_gen_time,
                                    async_row.is_tfb_dev,
                                    async_row.tfb_uuid,
                                    async_row.xkeys_uuid,
                                    async_row.seed_data,
                                                        async_row.hash1,
                    async_row.hash2,
                    async_row.hash3,
                    async_row.hash4,
                    async_row.hash5,
                                    async_row.seed_password.as_deref(),
                                    false, // no official races with progression spoilers so far
                                );
                                // Get game_id for the event's series
                                let game_id = sqlx::query_scalar!(
                                    r#"
                                        SELECT gs.game_id
                                        FROM game_series gs
                                        WHERE gs.series = $1
                                    "#,
                                    data.series.to_string()
                                )
                                .fetch_optional(&mut *transaction)
                                .await?
                                .flatten()
                                .unwrap_or(1); // Default to OOTR if no mapping found
                                
                                let extra = seed.extra(Utc::now()).await?;
                                let seed_table = seed::table(stream::iter(iter::once(seed)), false, &mut transaction, game_id).await?;
                                let ctx = ctx.take_submit_async();
                                let mut errors = ctx.errors().collect_vec();
                                Some(html! {
                                    h3 : "Async";
                                    div(class = "bg-surface") {
                                        p {
                                            : "You requested an async on ";
                                            : format_datetime(team_row.requested, DateTimeFormat { long: true, running_text: true });
                                            : ".";
                                        };
                                        : seed_table;
                                        @if let Some(password) = extra.password {
                                            p { //TODO replace this hack with password support in seed::table
                                                : "Password: ";
                                                @for note in password {
                                                    : char::from(note);
                                                }
                                            };
                                        }
                                        p : "After playing the async, fill out the form below.";
                                        : full_form(uri!(event::submit_async(data.series, &*data.event)), csrf, html! {
                                            @match data.team_config {
                                                TeamConfig::Solo => {
                                                    @if let Series::TriforceBlitz = data.series {
                                                        : form_field("pieces", &mut errors, html! {
                                                            label(for = "pieces") : "Number of Triforce Pieces found:";
                                                            input(type = "number", min = "0", max = tfb::piece_count(data.team_config), name = "pieces", value? = ctx.field_value("pieces"));
                                                        });
                                                        : form_field("time1", &mut errors, html! {
                                                            label(for = "time1") : "Time at which you found the most recent piece:";
                                                            input(type = "text", name = "time1", value? = ctx.field_value("time1")); //TODO h:m:s fields?
                                                            label(class = "help") : "(If you did not find any, leave this field blank.)";
                                                        });
                                                    } else {
                                                        : form_field("time1", &mut errors, html! {
                                                            label(for = "time1") : "Finishing Time:";
                                                            input(type = "text", name = "time1", value? = ctx.field_value("time1")); //TODO h:m:s fields?
                                                            label(class = "help") : "(If you did not finish, leave this field blank.)";
                                                        });
                                                    }
                                                    : form_field("vod1", &mut errors, html! {
                                                        label(for = "vod1") : "VoD:";
                                                        input(type = "text", name = "vod1", value? = ctx.field_value("vod1"));
                                                        label(class = "help") : "(You must submit a link to an unlisted YouTube video upload. The link to a YouTube video becomes available as soon as you begin the upload process.)";
                                                    });
                                                }
                                                TeamConfig::Pictionary => @unimplemented
                                                TeamConfig::CoOp => {
                                                    : form_field("time1", &mut errors, html! {
                                                        label(for = "time1") : "Player 1 Finishing Time:";
                                                        input(type = "text", name = "time1", value? = ctx.field_value("time1")); //TODO h:m:s fields?
                                                        label(class = "help") : "(If player 1 did not finish, leave this field blank.)";
                                                    });
                                                    : form_field("vod1", &mut errors, html! {
                                                        label(for = "vod1") : "Player 1 VoD:";
                                                        input(type = "text", name = "vod1", value? = ctx.field_value("vod1"));
                                                        label(class = "help") : "(You must submit a link to an unlisted YouTube video upload. The link to a YouTube video becomes available as soon as you begin the upload process.)";
                                                    });
                                                    : form_field("time2", &mut errors, html! {
                                                        label(for = "time2") : "Player 2 Finishing Time:";
                                                        input(type = "text", name = "time2", value? = ctx.field_value("time2")); //TODO h:m:s fields?
                                                        label(class = "help") : "(If player 2 did not finish, leave this field blank.)";
                                                    });
                                                    : form_field("vod2", &mut errors, html! {
                                                        label(for = "vod2") : "Player 2 VoD:";
                                                        input(type = "text", name = "vod2", value? = ctx.field_value("vod2"));
                                                        label(class = "help") : "(You must submit a link to an unlisted YouTube video upload. The link to a YouTube video becomes available as soon as you begin the upload process.)";
                                                    });
                                                }
                                                TeamConfig::TfbCoOp => @unimplemented
                                                TeamConfig::Multiworld => {
                                                    : form_field("time1", &mut errors, html! {
                                                        label(for = "time1", class = "power") : "Player 1 Finishing Time:";
                                                        input(type = "text", name = "time1", value? = ctx.field_value("time1")); //TODO h:m:s fields?
                                                        label(class = "help") : "(If player 1 did not finish, leave this field blank.)";
                                                    });
                                                    : form_field("vod1", &mut errors, html! {
                                                        label(for = "vod1", class = "power") : "Player 1 VoD:";
                                                        input(type = "text", name = "vod1", value? = ctx.field_value("vod1"));
                                                        label(class = "help") : "(The link to a YouTube video becomes available as soon as you begin the upload process. Other upload methods such as Twitch highlights are also allowed.)";
                                                    });
                                                    : form_field("time2", &mut errors, html! {
                                                        label(for = "time2", class = "wisdom") : "Player 2 Finishing Time:";
                                                        input(type = "text", name = "time2", value? = ctx.field_value("time2")); //TODO h:m:s fields?
                                                        label(class = "help") : "(If player 2 did not finish, leave this field blank.)";
                                                    });
                                                    : form_field("vod2", &mut errors, html! {
                                                        label(for = "vod2", class = "wisdom") : "Player 2 VoD:";
                                                        input(type = "text", name = "vod2", value? = ctx.field_value("vod2"));
                                                        label(class = "help") : "(The link to a YouTube video becomes available as soon as you begin the upload process. Other upload methods such as Twitch highlights are also allowed.)";
                                                    });
                                                    : form_field("time3", &mut errors, html! {
                                                        label(for = "time3", class = "courage") : "Player 3 Finishing Time:";
                                                        input(type = "text", name = "time3", value? = ctx.field_value("time3")); //TODO h:m:s fields?
                                                        label(class = "help") : "(If player 3 did not finish, leave this field blank.)";
                                                    });
                                                    : form_field("vod3", &mut errors, html! {
                                                        label(for = "vod3", class = "courage") : "Player 3 VoD:";
                                                        input(type = "text", name = "vod3", value? = ctx.field_value("vod3"));
                                                        label(class = "help") : "(The link to a YouTube video becomes available as soon as you begin the upload process. Other upload methods such as Twitch highlights are also allowed.)";
                                                    });
                                                }
                                            }
                                            : form_field("fpa", &mut errors, html! {
                                                label(for = "fpa") {
                                                    : "If you would like to invoke the ";
                                                    a(href = "https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub") : "Fair Play Agreement";
                                                    : ", describe the break(s) you took below. Include the reason, starting time, and duration.";
                                                }
                                                textarea(name = "fpa") : ctx.field_value("fpa");
                                            });
                                        }, errors, "Submit");
                                    }
                                })
                            }
                        } else {
                            let ctx = ctx.take_request_async();
                            let mut errors = ctx.errors().collect_vec();
                            let qualifier_kind = data.qualifier_kind(&mut transaction, Some(me)).await?;
                            let signups = teams::signups_sorted(&mut transaction, &mut teams::Cache::new(http_client.clone()), None, &data, false, qualifier_kind, None, true).await?;
                            let (qualified, maxed_out, num_finished, required) = if let Some(teams::SignupsTeam { qualification, .. }) = signups.iter().find(|teams::SignupsTeam { team, .. }| team.as_ref().is_some_and(|team| team.id == row.id)) {
                                match qualification {
                                    teams::Qualification::Single { qualified } | teams::Qualification::TriforceBlitz { qualified, .. } => (*qualified, false, 0, 1),
                                    teams::Qualification::Multiple { num_entered, num_finished, .. } => {
                                        if let QualifierKind::Score(score_kind) = qualifier_kind {
                                            let required = score_kind.required_qualifiers();
                                            (*num_finished >= required, *num_entered >= score_kind.max_qualifiers_that_count(), *num_finished, required)
                                        } else {
                                            (*num_finished >= 2, false, *num_finished, 2) // fallback
                                        }
                                    }
                                }
                            } else {
                                (false, false, 0, 2)
                            };
                            Some(html! {
                                h3 : "Async";
                                div(class = "bg-surface") {
                                    @match async_kind {
                                        AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 => @if maxed_out {
                                            p : "You have already entered the maximum number of qualifiers that count.";
                                        } else if qualified {
                                            p : "You are already qualified, but you can play this async to improve your seeding.";
                                        } else {
                                            p {
                                                : format!("You have finished {} out of {} required qualifiers. ", num_finished, required);
                                                : "Play this async to qualify for the tournament.";
                                            }
                                        }
                                        AsyncKind::Seeding => p : "If you would like to play the seeding async, you can request it here.";
                                        AsyncKind::Tiebreaker1 | AsyncKind::Tiebreaker2 => p : "Play the tiebreaker async to qualify for the bracket stage of the tournament.";
                                    }
                                    @match data.series {
                                        Series::CoOp => : coop::async_rules(async_kind);
                                        Series::MixedPools => : mp::async_rules(&data);
                                        Series::Multiworld => : mw::async_rules(&data, async_kind);
                                        _ => {}
                                    }
                                    : full_form(uri!(event::request_async(data.series, &*data.event)), csrf, html! {
                                        : form_field("confirm", &mut errors, html! {
                                            input(type = "checkbox", id = "confirm", name = "confirm");
                                            label(for = "confirm") {
                                                @if let Series::CoOp | Series::Multiworld = data.series {
                                                    : "We have read the above and are ready to play the seed";
                                                } else {
                                                    @if let TeamConfig::Solo = data.team_config {
                                                        : "I am ready to play the seed";
                                                    } else {
                                                        : "We are ready to play the seed";
                                                    }
                                                }
                                            }
                                        });
                                    }, errors, "Request Now");
                                }
                            })
                        }
                    } else {
                        None
                    };
                    @if let Some(async_info) = async_info {
                        : async_info;
                    } else {
                        @match data.series {
                            | Series::AlttprDe
                            | Series::CoOp
                            | Series::CopaDoBrasil
                            | Series::Crosskeys
                            | Series::MixedPools
                            | Series::Mq
                            | Series::MysteryD
                            | Series::Rsl
                            | Series::Standard
                            | Series::TournoiFrancophone
                            | Series::WeTryToBeBetter
                            | Series::TwwrMain
                                => @if let French = data.language {
                                    p : "Planifiez vos matches dans les fils du canal dédié.";
                                } else {
                                    p : "Please schedule your matches using the Discord match threads.";
                                }
                            | Series::BattleRoyale
                            | Series::League
                            | Series::Scrubs
                                => @unimplemented // no signups on Mido's House
                            Series::Multiworld => @if data.is_started(&mut transaction).await? {
                                //TODO adjust for other match data sources?
                                //TODO get this team's known matchup(s) from start.gg
                                p : "Please schedule your matches using Discord threads in the scheduling channel.";
                                //TODO form to submit matches
                            } else {
                                //TODO if any vods are still missing, show form to add them
                                p : "Waiting for the start of the tournament and round 1 pairings. Keep an eye out for an announcement on Discord."; //TODO include start date?
                            }
                            Series::NineDaysOfSaws => @if data.is_ended() {
                                p : "This race has been completed."; //TODO ranking and finish time
                            } else if let Some(ref race_room) = data.url {
                                p {
                                    : "Please join ";
                                    a(href = race_room.to_string()) : "the race room";
                                    : " as soon as possible. You will receive further instructions there.";
                                }
                            } else {
                                : "Waiting for the race room to be opened, which should happen around 30 minutes before the scheduled starting time. Keep an eye out for an announcement on Discord.";
                            }
                            Series::Pictionary => @if data.is_ended() {
                                p : "This race has been completed."; //TODO ranking and finish time
                            } else if let Some(ref race_room) = data.url {
                                @match row.role.try_into().expect("non-Pictionary role in Pictionary team") {
                                    pic::Role::Sheikah => p {
                                        : "Please join ";
                                        a(href = race_room.to_string()) : "the race room";
                                        : " as soon as possible. You will receive further instructions there.";
                                    }
                                    pic::Role::Gerudo => p {
                                        : "Please keep an eye on ";
                                        a(href = race_room.to_string()) : "the race room";
                                        : " (but do not join). The spoiler log will be posted there.";
                                    }
                                }
                            } else {
                                : "Waiting for the race room to be opened, which should happen around 30 minutes before the scheduled starting time. Keep an eye out for an announcement on Discord.";
                            }
                            Series::SongsOfHope => @if data.is_started(&mut transaction).await? {
                                p : "Please schedule your matches using Discord threads in the scheduling channel.";
                            } else {
                                p { //TODO indicate whether qualified?
                                    : "Please see the rules document for how to qualify, and "; //TODO linkify
                                    a(href = uri!(races(data.series, &*data.event))) : "the race schedule";
                                    : " for upcoming qualifiers.";
                                }
                            }
                            Series::SpeedGaming => p { //TODO indicate whether qualified?
                                : "Please see the rules document for how to qualify, and "; //TODO linkify
                                a(href = uri!(races(data.series, &*data.event))) : "the race schedule";
                                : " for upcoming qualifiers.";
                            }
                            Series::TriforceBlitz => @if data.is_started(&mut transaction).await? {
                                //TODO get this entrant's known matchup(s)
                                p : "Please schedule your matches using Discord threads in the scheduling channel.";
                            } else {
                                //TODO if any vods are still missing, show form to add them
                                p : "Waiting for the start of the tournament and round 1 pairings. Keep an eye out for an announcement on Discord."; //TODO include start date?
                            }
                        }
                    }
                    @if !data.is_ended() {
                        h2 : "Options";
                        @let ctx = ctx.take_edit();
                        @let mut errors = ctx.errors().collect_vec();
                        : full_form(uri!(status_post(data.series, &*data.event)), csrf, html! {
                            : form_field("restream_consent", &mut errors, html! {
                                input(type = "checkbox", id = "restream_consent", name = "restream_consent", checked? = ctx.field_value("restream_consent").map_or(row.restream_consent, |value| value == "on"));
                                label(for = "restream_consent") {
                                    @if let TeamConfig::Solo = data.team_config {
                                        : "I am okay with being restreamed.";
                                    } else {
                                        : "We are okay with being restreamed.";
                                    }
                                }
                            });
                            @if let Some(ref enter_flow) = data.enter_flow {
                                @for requirement in &enter_flow.requirements {
                                    @if let enter::Requirement::BooleanChoice { key, label } = requirement {
                                        @let field_name = format!("custom_choices[{key}]");
                                        @let field_id_yes = format!("custom_choices[{key}]-yes");
                                        @let field_id_no = format!("custom_choices[{key}]-no");
                                        @let yes_checked = ctx.field_value(&*field_name).map_or_else(|| row.custom_choices.get(key).is_some_and(|v| v == "yes"), |value| value == "yes");
                                        @let no_checked = ctx.field_value(&*field_name).map_or_else(|| row.custom_choices.get(key).is_some_and(|v| v == "no"), |value| value == "no");
                                        : form_field(&field_name, &mut errors, html! {
                                            label(for = &field_name) : label;
                                            br;
                                            input(id = &field_id_yes, type = "radio", name = &field_name, value = "yes", checked? = yes_checked);
                                            label(for = &field_id_yes) : "Yes";
                                            input(id = &field_id_no, type = "radio", name = &field_name, value = "no", checked? = no_checked);
                                            label(for = &field_id_no) : "No";
                                        });
                                    }
                                }
                            }
                            //TODO options to change team name or swap roles
                        }, errors, "Save");
                        p {
                            a(href = uri!(resign(data.series, &*data.event, row.id))) : "Resign";
                        }
                    }
                }
            }
        } else {
            html! {
                : header;
                article {
                    p : "You are not signed up for this event.";
                    p {
                        : "If you want to change that, please see ";
                        a(href = uri!(enter::get(data.series, &*data.event, _, _))) : "the Enter tab";
                        : ".";
                    }
                    @if !matches!(data.team_config, TeamConfig::Solo) {
                        p {
                            : "You can accept, decline, or retract unconfirmed team invitations on ";
                            a(href = uri!(teams::get(data.series, &*data.event))) : "the Teams tab";
                            : ".";
                        }
                    }
                }
            }
        }
    } else {
        html! {
            : header;
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(status(data.series, &*data.event)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to view your status for this event.";
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("My Status — {}", data.display_name), content).await?)
}

#[rocket::get("/event/<series>/<event>/status")]
pub(crate) async fn status(pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(status_page(transaction, http_client, me, uri, csrf.as_ref(), data, StatusContext::None).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct StatusForm {
    #[field(default = String::new())]
    csrf: String,
    restream_consent: bool,
    custom_choices: HashMap<String, String>,
}

#[rocket::post("/event/<series>/<event>/status", data = "<form>")]
pub(crate) async fn status_post(pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, StatusForm>>) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if data.is_ended() {
        form.context.push_error(form::Error::validation("This event has already ended."));
    }
    let row = sqlx::query!(r#"SELECT id AS "id: Id<Teams>", restream_consent FROM teams, team_members WHERE
        id = team
        AND series = $1
        AND event = $2
        AND member = $3
        AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        AND NOT resigned
    "#, data.series as _, &data.event, me.id as _).fetch_one(&mut *transaction).await?;
    Ok(if let Some(ref value) = form.value {
        if row.restream_consent && !value.restream_consent {
            //TODO check if restream consent can still be revoked according to tournament rules, offer to resign if not
            if Race::for_event(&mut transaction, http_client, &data).await?.into_iter().any(|race| !race.is_ended() && !race.video_urls.is_empty()) {
                form.context.push_error(form::Error::validation("There is a restream planned for one of your upcoming races. Please contact an event organizer if you would like to cancel.").with_name("restream_consent"));
            }
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(status_page(transaction, http_client, Some(me), uri, csrf.as_ref(), data, StatusContext::Edit(form.context)).await?)
        } else {
            sqlx::query!("UPDATE teams SET restream_consent = $1, custom_choices = $2 WHERE id = $3", value.restream_consent, Json(&value.custom_choices) as _, row.id as _).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(status(series, event))))
        }
    } else {
        RedirectOrContent::Content(status_page(transaction, http_client, Some(me), uri, csrf.as_ref(), data, StatusContext::Edit(form.context)).await?)
    })
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum FindTeamError {
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Event(#[from] Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("unknown user")]
    UnknownUser,
}

impl<E: Into<FindTeamError>> From<E> for StatusOrError<FindTeamError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

async fn find_team_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, FindTeamError> {
    Ok(match data.team_config {
        TeamConfig::Solo => {
            let header = data.header(&mut transaction, me.as_ref(), Tab::FindTeam, false).await?;
            page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Find Teammates — {}", data.display_name), html! {
                : header;
                : "This is a solo event.";
            }).await?
        }
        TeamConfig::Pictionary => pic::find_team_form(transaction, me, uri, csrf, data, ctx).await?,
        TeamConfig::CoOp | TeamConfig::TfbCoOp | TeamConfig::Multiworld => mw::find_team_form(transaction, me, uri, csrf, data, ctx).await?,
    })
}

#[rocket::get("/event/<series>/<event>/find-team")]
pub(crate) async fn find_team(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<FindTeamError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(find_team_form(transaction, me, uri, csrf.as_ref(), data, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct FindTeamForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    availability: String,
    #[field(default = String::new())]
    notes: String,
    role: Option<pic::RolePreference>,
}

#[rocket::post("/event/<series>/<event>/find-team", data = "<form>")]
pub(crate) async fn find_team_post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, FindTeamForm>>) -> Result<RedirectOrContent, StatusOrError<FindTeamError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if data.is_started(&mut transaction).await? {
        form.context.push_error(form::Error::validation("You can no longer enter this event since it has already started."));
    }
    Ok(if let Some(ref value) = form.value {
        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM looking_for_team WHERE
            series = $1
            AND event = $2
            AND user_id = $3
        ) AS "exists!""#, series as _, event, me.id as _).fetch_one(&mut *transaction).await? {
            form.context.push_error(form::Error::validation("You are already on the list."));
        }
        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
            id = team
            AND series = $1
            AND event = $2
            AND member = $3
            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        ) AS "exists!""#, series as _, event, me.id as _).fetch_one(&mut *transaction).await? {
            form.context.push_error(form::Error::validation("You are already signed up for this event."));
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(find_team_form(transaction, Some(me), uri, csrf.as_ref(), data, form.context).await?)
        } else {
            sqlx::query!("INSERT INTO looking_for_team (series, event, user_id, role, availability, notes) VALUES ($1, $2, $3, $4, $5, $6)", series as _, event, me.id as _, value.role.unwrap_or_default() as _, value.availability, value.notes).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(find_team(series, event))))
        }
    } else {
        RedirectOrContent::Content(find_team_form(transaction, Some(me), uri, csrf.as_ref(), data, form.context).await?)
    })
}

/// Metadata to ensure the correct page is displayed on form validation failure.
#[derive(FromFormField)]
pub(crate) enum AcceptFormSource {
    Enter,
    Notifications,
    Teams,
}

impl ToHtml for AcceptFormSource {
    fn to_html(&self) -> RawHtml<String> {
        html! {
            input(type = "hidden", name = "source", value = match self {
                Self::Enter => "enter",
                Self::Notifications => "notifications",
                Self::Teams => "teams",
            });
        }
    }
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AcceptForm {
    #[field(default = String::new())]
    csrf: String,
    source: AcceptFormSource,
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum AcceptError {
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Discord(#[from] serenity::Error),
    #[error(transparent)] Enter(#[from] enter::Error),
    #[error(transparent)] Notification(#[from] crate::notification::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Teams(#[from] teams::Error),
    #[error("invalid form data")]
    FormValue,
}

impl<E: Into<AcceptError>> From<E> for StatusOrError<AcceptError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

#[rocket::post("/event/<series>/<event>/confirm/<team>", data = "<form>")]
pub(crate) async fn confirm_signup(pool: &State<PgPool>, http_client: &State<reqwest::Client>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, team: Id<Teams>, form: Form<Contextual<'_, AcceptForm>>) -> Result<RedirectOrContent, StatusOrError<AcceptError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(ref value) = form.value {
        if data.is_started(&mut transaction).await? {
            form.context.push_error(form::Error::validation("You can no longer enter this event since it has already started."));
        }
        let role = sqlx::query_scalar!(r#"SELECT role AS "role: Role" FROM team_members WHERE team = $1 AND member = $2 AND status = 'unconfirmed'"#, team as _, me.id as _).fetch_optional(&mut *transaction).await?;
        if let Some(role) = role {
            if data.team_config.role_is_racing(role) && me.racetime.is_none() {
                form.context.push_error(form::Error::validation("A racetime.gg account is required to enter as runner."));
            }
        } else {
            form.context.push_error(form::Error::validation("You haven't been invited to this team."));
        }
        Ok(if form.context.errors().next().is_some() {
            RedirectOrContent::Content(match value.source {
                AcceptFormSource::Enter => enter::enter_form(transaction, http_client, discord_ctx, Some(me), uri, csrf.as_ref(), data, pic::EnterFormDefaults::Context(form.context)).await?,
                AcceptFormSource::Notifications => {
                    transaction.rollback().await?;
                    crate::notification::list(pool, Some(me), uri, csrf.as_ref(), form.context).await?
                }
                AcceptFormSource::Teams => {
                    transaction.rollback().await?;
                    teams::list(pool, http_client, Some(me), uri, csrf, form.context, series, event).await.map_err(|e| match e {
                        StatusOrError::Status(status) => StatusOrError::Status(status),
                        StatusOrError::Err(e) => e.into(),
                    })?
                }
            })
        } else {
            for member in sqlx::query_scalar!(r#"SELECT member AS "id: Id<Users>" FROM team_members WHERE team = $1 AND (status = 'created' OR status = 'confirmed')"#, team as _).fetch_all(&mut *transaction).await? {
                let id = Id::<Notifications>::new(&mut transaction).await?;
                sqlx::query!("INSERT INTO notifications (id, rcpt, kind, series, event, sender) VALUES ($1, $2, 'accept', $3, $4, $5)", id as _, member as _, series as _, event, me.id as _).execute(&mut *transaction).await?;
            }
            sqlx::query!("UPDATE team_members SET status = 'confirmed' WHERE team = $1 AND member = $2", team as _, me.id as _).execute(&mut *transaction).await?;
            if !sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM team_members WHERE team = $1 AND status = 'unconfirmed') AS "exists!""#, team as _).fetch_one(&mut *transaction).await? {
                // this confirms the team
                // remove all members from looking_for_team
                sqlx::query!("DELETE FROM looking_for_team WHERE EXISTS (SELECT 1 FROM team_members WHERE team = $1 AND member = user_id)", team as _).execute(&mut *transaction).await?;
                //TODO also remove all other teams with member overlap, and notify
                // create and assign Discord roles
                if let Some(discord_guild) = data.discord_guild {
                    let discord_ctx = discord_ctx.read().await;
                    for row in sqlx::query!(r#"SELECT discord_id AS "discord_id!: PgSnowflake<UserId>", role AS "role: Role" FROM users, team_members WHERE id = member AND discord_id IS NOT NULL AND team = $1"#, team as _).fetch_all(&mut *transaction).await? {
                        if let Ok(mut member) = discord_guild.member(&*discord_ctx, row.discord_id.0).await {
                            let mut roles_to_assign = member.roles.iter().copied().collect::<HashSet<_>>();
                            if let Some(PgSnowflake(participant_role)) = sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND series = $2 AND event = $3"#, PgSnowflake(discord_guild) as _, series as _, event).fetch_optional(&mut *transaction).await? {
                                roles_to_assign.insert(participant_role);
                            }
                            if let Some(PgSnowflake(role_role)) = sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND role = $2"#, PgSnowflake(discord_guild) as _, row.role as _).fetch_optional(&mut *transaction).await? {
                                roles_to_assign.insert(role_role);
                            }
                            if let Some(racetime_slug) = sqlx::query_scalar!("SELECT racetime_slug FROM teams WHERE id = $1", team as _).fetch_one(&mut *transaction).await? {
                                if let Some(PgSnowflake(team_role)) = sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND racetime_team = $2"#, PgSnowflake(discord_guild) as _, racetime_slug).fetch_optional(&mut *transaction).await? {
                                    roles_to_assign.insert(team_role);
                                } else {
                                    let team_name = sqlx::query_scalar!(r#"SELECT name AS "name!" FROM teams WHERE id = $1"#, team as _).fetch_one(&mut *transaction).await?;
                                    let team_role = discord_guild.create_role(&*discord_ctx, EditRole::new().hoist(false).mentionable(true).name(team_name).permissions(Permissions::empty())).await?.id;
                                    sqlx::query!("INSERT INTO discord_roles (id, guild, racetime_team) VALUES ($1, $2, $3)", PgSnowflake(team_role) as _, PgSnowflake(discord_guild) as _, racetime_slug).execute(&mut *transaction).await?;
                                    roles_to_assign.insert(team_role);
                                }
                            }
                            member.edit(&*discord_ctx, EditMember::new().roles(roles_to_assign)).await?;
                        }
                    }
                }
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(teams::get(series, event))))
        })
    } else {
        Err(StatusOrError::Err(AcceptError::FormValue))
    }
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum ResignError {
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Discord(#[from] serenity::Error),
    #[error(transparent)] Event(#[from] Error),
    #[error(transparent)] Enter(#[from] enter::Error),
    #[error(transparent)] Notification(#[from] crate::notification::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Teams(#[from] teams::Error),
    #[error("invalid form data")]
    FormValue,
}

impl<E: Into<ResignError>> From<E> for StatusOrError<ResignError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

async fn resign_page(pool: &PgPool, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, ctx: Context<'_>, series: Series, event: &str, team: Id<Teams>) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    if data.is_ended() {
        return Err(StatusOrError::Status(Status::Forbidden))
    }
    let is_started = data.is_started(&mut transaction).await?;
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Resign — {}", data.display_name), html! {
        p {
            @if is_started {
                @if let TeamConfig::Solo = data.team_config {
                    : "Are you sure you want to resign from ";
                    : data;
                    : "?";
                } else {
                    : "Are you sure you want to remove your team from ";
                    : data;
                    : "?";
                }
            } else {
                @if let TeamConfig::Solo = data.team_config {
                    : "Are you sure you want to retract your registration from ";
                    : data;
                    : "?";
                } else {
                    : "Are you sure you want to retract your team's registration from ";
                    : data;
                    : "? If you change your mind later, you will need to invite your teammates again.";
                }
            }
        }
        @let (errors, button) = button_form_ext(uri!(crate::event::resign_post(series, event, team)), csrf.as_ref(), ctx.errors().collect(), ResignFormSource::Resign, "Yes, resign");
        : errors;
        div(class = "button-row") : button;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/resign/<team>")]
pub(crate) async fn resign(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, team: Id<Teams>) -> Result<RawHtml<String>, StatusOrError<Error>> {
    resign_page(pool, me, uri, csrf, Context::default(), series, event, team).await
}

/// Metadata to ensure the correct page is displayed on form validation failure.
#[derive(FromFormField)]
pub(crate) enum ResignFormSource {
    Enter,
    Notifications,
    Resign,
    Teams,
}

impl ToHtml for ResignFormSource {
    fn to_html(&self) -> RawHtml<String> {
        html! {
            input(type = "hidden", name = "source", value = match self {
                Self::Enter => "enter",
                Self::Notifications => "notifications",
                Self::Resign => "resign",
                Self::Teams => "teams",
            });
        }
    }
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ResignForm {
    #[field(default = String::new())]
    csrf: String,
    source: ResignFormSource,
}

#[rocket::post("/event/<series>/<event>/resign/<team>", data = "<form>")]
pub(crate) async fn resign_post(pool: &State<PgPool>, http_client: &State<reqwest::Client>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, team: Id<Teams>, form: Form<Contextual<'_, ResignForm>>) -> Result<RedirectOrContent, StatusOrError<ResignError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let team = Team::from_id(&mut transaction, team).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("You can no longer resign from this event since it has already ended."));
        }
        let keep_record = data.is_started(&mut transaction).await? || sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM async_teams WHERE team = $1) AS "exists!""#, team.id as _).fetch_one(&mut *transaction).await?;
        let msg = MessageBuilder::default()
            .mention_team(&mut transaction, data.discord_guild, &team).await?
            .push(if team.name_is_plural() { " have resigned from " } else { " has resigned from " })
            .push_safe(&data.display_name)
            .push(".")
            .build();
        let members = if keep_record {
            sqlx::query!(r#"UPDATE teams SET resigned = TRUE WHERE id = $1"#, team.id as _).execute(&mut *transaction).await?;
            sqlx::query!(r#"SELECT member AS "id: Id<Users>", status AS "status: SignupStatus" FROM team_members WHERE team = $1"#, team.id as _).fetch(&mut *transaction)
                .map_ok(|row| (row.id, row.status))
                .try_collect::<Vec<_>>().await?
        } else {
            sqlx::query!(r#"DELETE FROM team_members WHERE team = $1 RETURNING member AS "id: Id<Users>", status AS "status: SignupStatus""#, team.id as _).fetch(&mut *transaction)
                .map_ok(|row| (row.id, row.status))
                .try_collect().await?
        };
        let mut me_in_team = false;
        let mut notification_kind = SimpleNotificationKind::Resign;
        for &(member_id, status) in &members {
            if member_id == me.id {
                me_in_team = true;
                if !status.is_confirmed() { notification_kind = SimpleNotificationKind::Decline }
                break
            }
        }
        if !me_in_team {
            form.context.push_error(form::Error::validation("Can't delete teams you're not part of."));
        }
        Ok(if form.context.errors().next().is_some() {
            RedirectOrContent::Content(match value.source {
                ResignFormSource::Enter => enter::enter_form(transaction, http_client, discord_ctx, Some(me), uri, csrf.as_ref(), data, pic::EnterFormDefaults::Context(form.context)).await?,
                ResignFormSource::Notifications => {
                    transaction.rollback().await?;
                    crate::notification::list(pool, Some(me), uri, csrf.as_ref(), form.context).await?
                }
                ResignFormSource::Resign => {
                    transaction.rollback().await?;
                    resign_page(pool, Some(me), uri, csrf, form.context, series, event, team.id).await.map_err(|e| match e {
                        StatusOrError::Status(status) => StatusOrError::Status(status),
                        StatusOrError::Err(e) => e.into(),
                    })?
                }
                ResignFormSource::Teams => {
                    transaction.rollback().await?;
                    teams::list(pool, http_client, Some(me), uri, csrf, form.context, series, event).await.map_err(|e| match e {
                        StatusOrError::Status(status) => StatusOrError::Status(status),
                        StatusOrError::Err(e) => e.into(),
                    })?
                }
            })
        } else {
            for (member_id, status) in members {
                if member_id != me.id && status.is_confirmed() {
                    let notification_id = Id::<Notifications>::new(&mut transaction).await?;
                    sqlx::query!("INSERT INTO notifications (id, rcpt, kind, series, event, sender) VALUES ($1, $2, $3, $4, $5, $6)", notification_id as _, member_id as _, notification_kind as _, series as _, event, me.id as _).execute(&mut *transaction).await?;
                }
            }
            if let (Some(discord_guild), Some(PgSnowflake(participant_role))) = (data.discord_guild, sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND series = $2 AND event = $3"#, PgSnowflake(data.discord_guild.unwrap()) as _, series as _, event).fetch_optional(&mut *transaction).await?) {
                let discord_ctx = discord_ctx.read().await;
                let team_members = team.members(&mut transaction).await?;
                for user in team_members {
                    if let Some(discord_user) = user.discord.as_ref() {
                        if let Ok(member) = discord_guild.member(&*discord_ctx, discord_user.id).await {
                            let _ = member.remove_role(&*discord_ctx, participant_role).await;
                        }
                    }
                }
            }
            if let Some(organizer_channel) = data.discord_organizer_channel {
                //TODO don't post this message for unconfirmed (or unqualified?) teams
                organizer_channel.say(&*discord_ctx.read().await, msg).await?;
            }
            if !keep_record {
                sqlx::query!("DELETE FROM teams WHERE id = $1", team.id as _).execute(&mut *transaction).await?;
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(teams::get(series, event))))
        })
    } else {
        Err(StatusOrError::Err(ResignError::FormValue))
    }
}

async fn opt_out_page(pool: &PgPool, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, ctx: Context<'_>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    if data.is_ended() {
        return Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Opt Out — {}", data.display_name), html! {
            p {
                : "You can no longer opt out of participating in ";
                : data;
                : " since it has already ended.";
            }
        }).await?)
    }
    if let Some(ref me) = me {
        if me.racetime.is_none() {
            return Err(StatusOrError::Status(Status::Forbidden)) //TODO ask to connect a racetime.gg account
        }
    } else {
        return Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Opt Out — {}", data.display_name), html! {
            p {
                a(href = uri!(auth::login(Some(uri!(opt_out(series, event)))))) : "Sign in or create a Hyrule Town Hall account";
                : " to opt out of participating in ";
                : data;
                : ".";
            }
        }).await?)
    }
    let opted_out = if let Some(racetime) = me.as_ref().and_then(|me| me.racetime.as_ref()) {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM opt_outs WHERE series = $1 AND event = $2 AND racetime_id = $3) AS "exists!""#, data.series as _, &data.event, racetime.id).fetch_one(&mut *transaction).await?
    } else {
        false
    };
    let entered = if let Some(ref me) = me {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
            id = team
            AND series = $1
            AND event = $2
            AND member = $3
            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        ) AS "exists!""#, data.series as _, &data.event, me.id as _).fetch_one(&mut *transaction).await?
    } else {
        false
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Opt Out — {}", data.display_name), html! {
        @if opted_out {
            p : "You have already opted out.";
        } else if entered {
            p : "You can no longer opt out since you have already entered this event. You can resign from your status page."; //TODO direct link or redirect to resign page
        } else {
            p {
                : "Are you sure you want to opt out of participating in ";
                : data;
                : "?";
            }
            @let (errors, button) = button_form(uri!(crate::event::opt_out_post(series, event)), csrf.as_ref(), ctx.errors().collect(), "Yes, opt out");
            : errors;
            div(class = "button-row") : button;
        }
    }).await?)
}

#[rocket::get("/event/<series>/<event>/opt-out")]
pub(crate) async fn opt_out(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<Error>> {
    opt_out_page(pool, me, uri, csrf, Context::default(), series, event).await
}

#[rocket::post("/event/<series>/<event>/opt-out", data = "<form>")]
pub(crate) async fn opt_out_post(pool: &State<PgPool>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, EmptyForm>>) -> Result<RedirectOrContent, StatusOrError<ResignError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("You can no longer opt out from this event since it has already ended."));
        }
        if let Some(racetime) = me.racetime.as_ref() {
            if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM opt_outs WHERE series = $1 AND event = $2 AND racetime_id = $3) AS "exists!""#, data.series as _, &data.event, racetime.id).fetch_one(&mut *transaction).await? {
                form.context.push_error(form::Error::validation("You have already resigned from this event."));
            }
        }
        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
            id = team
            AND series = $1
            AND event = $2
            AND member = $3
            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        ) AS "exists!""#, data.series as _, &data.event, me.id as _).fetch_one(&mut *transaction).await? {
            form.context.push_error(form::Error::validation("You can no longer opt out since you have already entered this event."));
        }
        if me.racetime.is_none() {
            form.context.push_error(form::Error::validation("Connect a racetime.gg account to your Hyrule Town Hall account to opt out."));
        }
        Ok(if form.context.errors().next().is_some() {
            transaction.rollback().await?;
            RedirectOrContent::Content(opt_out_page(pool, Some(me), uri, csrf, form.context, series, event).await.map_err(|e| match e {
                StatusOrError::Status(status) => StatusOrError::Status(status),
                StatusOrError::Err(e) => e.into(),
            })?)
        } else {
            let racetime = me.racetime.as_ref().expect("validated");
            sqlx::query!(r#"INSERT INTO opt_outs (series, event, racetime_id) VALUES ($1, $2, $3)"#, series as _, event, racetime.id).execute(&mut *transaction).await?;
            if let Some(organizer_channel) = data.discord_organizer_channel {
                organizer_channel.say(&*discord_ctx.read().await, MessageBuilder::default()
                    .mention_user(&me)
                    .push(" has opted out from ")
                    .push_safe(data.display_name)
                    .push(".")
                    .build(),
                ).await?;
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(crate::http::index)))
        })
    } else {
        Err(StatusOrError::Err(ResignError::FormValue))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, sqlx::Type, FromFormField, Sequence)]
#[sqlx(type_name = "async_kind", rename_all = "lowercase")]
pub(crate) enum AsyncKind {
    #[sqlx(rename = "qualifier")]
    Qualifier1,
    Qualifier2,
    Qualifier3,
    /// Like qualifier but not required to enter
    Seeding,
    /// The tiebreaker for the highest Swiss points group with more than one team.
    Tiebreaker1,
    /// The tiebreaker for the 2nd-highest Swiss points group with more than one team.
    Tiebreaker2,
}

impl AsyncKind {
    pub(crate) fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Qualifier1),
            1 => Some(Self::Qualifier2),
            2 => Some(Self::Qualifier3),
            3 => Some(Self::Seeding),
            4 => Some(Self::Tiebreaker1),
            5 => Some(Self::Tiebreaker2),
            _ => None,
        }
    }
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RequestAsyncForm {
    #[field(default = String::new())]
    csrf: String,
    confirm: bool,
}

#[rocket::post("/event/<series>/<event>/request-async", data = "<form>")]
pub(crate) async fn request_async(pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, RequestAsyncForm>>) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        let team = sqlx::query_as!(Team, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, teams.startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams, team_members WHERE
            id = team
            AND series = $1
            AND event = $2
            AND member = $3
            AND NOT resigned
            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        "#, series as _, event, me.id as _).fetch_optional(&mut *transaction).await?;
        let async_kind = if let Some(ref team) = team {
            if let Some(async_kind) = data.active_async(&mut transaction, Some(team.id)).await? {
                let requested = sqlx::query_scalar!(r#"SELECT requested IS NOT NULL AS "requested!" FROM async_teams WHERE team = $1 AND kind = $2"#, team.id as _, async_kind as _).fetch_optional(&mut *transaction).await?;
                if requested.is_some_and(identity) {
                    form.context.push_error(form::Error::validation("Your team has already requested this async."));
                }
                Some(async_kind)
            } else {
                form.context.push_error(form::Error::validation("There is no active async for your team."));
                None
            }
        } else {
            //TODO if this is a solo event, check signup requirements and sign up?
            form.context.push_error(form::Error::validation("You are not signed up for this event."));
            None
        };
        if !value.confirm {
            form.context.push_error(form::Error::validation("This field is required.").with_name("confirm"));
        }
        if form.context.errors().next().is_some() {
            transaction.rollback().await?;
            RedirectOrContent::Content(status_page(pool.begin().await?, http_client, Some(me), uri, csrf.as_ref(), data, StatusContext::RequestAsync(form.context)).await?)
        } else {
            let team = team.expect("validated");
            let async_kind = async_kind.expect("validated");
            sqlx::query!("INSERT INTO async_teams (team, kind, requested) VALUES ($1, $2, NOW()) ON CONFLICT (team, kind) DO UPDATE SET requested = EXCLUDED.requested", team.id as _, async_kind as _).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(status(series, event))))
        }
    } else {
        transaction.rollback().await?;
        RedirectOrContent::Content(status_page(pool.begin().await?, http_client, Some(me), uri, csrf.as_ref(), data, StatusContext::RequestAsync(form.context)).await?)
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct SubmitAsyncForm {
    #[field(default = String::new())]
    csrf: String,
    pieces: Option<i16>,
    #[field(default = String::new())]
    time1: String,
    #[field(default = String::new())]
    vod1: String,
    #[field(default = String::new())]
    time2: String,
    #[field(default = String::new())]
    vod2: String,
    #[field(default = String::new())]
    time3: String,
    #[field(default = String::new())]
    vod3: String,
    #[field(default = String::new())]
    fpa: String,
}

#[rocket::post("/event/<series>/<event>/submit-async", data = "<form>")]
pub(crate) async fn submit_async(pool: &State<PgPool>, http_client: &State<reqwest::Client>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, SubmitAsyncForm>>) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        let team = sqlx::query_as!(Team, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, teams.startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams, team_members WHERE
            id = team
            AND series = $1
            AND event = $2
            AND member = $3
            AND NOT resigned
            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
        "#, series as _, event, me.id as _).fetch_optional(&mut *transaction).await?;
        let async_kind = if let Some(ref team) = team {
            if let Some(async_kind) = data.active_async(&mut transaction, Some(team.id)).await? {
                let row = sqlx::query!(r#"SELECT requested IS NOT NULL AS "requested!", submitted IS NOT NULL AS "submitted!" FROM async_teams WHERE team = $1 AND kind = $2"#, team.id as _, async_kind as _).fetch_optional(&mut *transaction).await?;
                if row.as_ref().is_some_and(|row| row.submitted) {
                    form.context.push_error(form::Error::validation("You have already submitted times for this async. To make a correction or add vods, please contact the tournament organizers.")); //TODO allow adding vods via form but no other edits
                }
                if !row.is_some_and(|row| row.requested) {
                    form.context.push_error(form::Error::validation("You have not requested this async yet."));
                }
                Some(async_kind)
            } else {
                form.context.push_error(form::Error::validation("There is no active async for your team."));
                None
            }
        } else {
            form.context.push_error(form::Error::validation("You are not signed up for this event."));
            None
        };
        if let Series::TriforceBlitz = series {
            if let Some(pieces) = value.pieces {
                if pieces < 0 || pieces > i16::from(tfb::piece_count(data.team_config)) {
                    form.context.push_error(form::Error::validation(format!("Must be a number from 0 to {}.", tfb::piece_count(data.team_config))).with_name("pieces"));
                }
            } else {
                form.context.push_error(form::Error::validation("This field is required.").with_name("pieces"));
            }
        }
        let times = vec![
            if value.time1.is_empty() {
                None
            } else if let Some(time) = parse_duration(&value.time1, None) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'. Leave blank to indicate DNF.").with_name("time1"));
                None
            },
            if value.time2.is_empty() {
                None
            } else if let Some(time) = parse_duration(&value.time2, None) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'. Leave blank to indicate DNF.").with_name("time2"));
                None
            },
            if value.time3.is_empty() {
                None
            } else if let Some(time) = parse_duration(&value.time3, None) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'. Leave blank to indicate DNF.").with_name("time3"));
                None
            },
        ];
        let vods = vec![
            value.vod1.clone(),
            value.vod2.clone(),
            value.vod3.clone(),
        ];
        if form.context.errors().next().is_some() {
            transaction.rollback().await?;
            RedirectOrContent::Content(status_page(pool.begin().await?, http_client, Some(me), uri, csrf.as_ref(), data, StatusContext::SubmitAsync(form.context)).await?)
        } else {
            let team = team.expect("validated");
            let async_kind = async_kind.expect("validated");
            sqlx::query!("UPDATE async_teams SET submitted = NOW(), pieces = $1, fpa = $2 WHERE team = $3 AND kind = $4", value.pieces, (!value.fpa.is_empty()).then(|| &value.fpa), team.id as _, async_kind as _).execute(&mut *transaction).await?;
            let mut players = Vec::default();
            for (((role, _), time), vod) in data.team_config.roles().iter().zip(&times).zip(&vods) {
                let player = sqlx::query_scalar!(r#"SELECT member AS "member: Id<Users>" FROM team_members WHERE team = $1 AND role = $2"#, team.id as _, role as _).fetch_one(&mut *transaction).await?;
                sqlx::query!("INSERT INTO async_players (series, event, player, kind, time, vod) VALUES ($1, $2, $3, $4, $5, $6)", series as _, event, player as _, async_kind as _, time as _, (!vod.is_empty()).then_some(vod)).execute(&mut *transaction).await?;
                players.push(player);
            }
            if let Some(discord_guild) = data.discord_guild {
                let asyncs_row = sqlx::query!(r#"SELECT discord_role AS "discord_role: PgSnowflake<RoleId>", discord_channel AS "discord_channel: PgSnowflake<ChannelId>" FROM asyncs WHERE series = $1 AND event = $2 AND kind = $3"#, series as _, event, async_kind as _).fetch_one(&mut *transaction).await?;
                let members = sqlx::query_scalar!(r#"SELECT discord_id AS "discord_id!: PgSnowflake<UserId>" FROM users, team_members WHERE id = member AND discord_id IS NOT NULL AND team = $1"#, team.id as _).fetch_all(&mut *transaction).await?;
                if let Some(PgSnowflake(discord_role)) = asyncs_row.discord_role {
                    for &PgSnowflake(user_id) in &members {
                        if let Ok(member) = discord_guild.member(&*discord_ctx.read().await, user_id).await {
                            member.add_role(&*discord_ctx.read().await, discord_role).await?;
                        }
                    }
                }
                let result_channel = if let Some(PgSnowflake(discord_channel)) = asyncs_row.discord_channel {
                    Some((discord_channel, false))
                } else if let Some(organizer_channel) = data.discord_organizer_channel {
                    Some((organizer_channel, true))
                } else {
                    None
                };
                if let Some((discord_channel, private)) = result_channel {
                    let mut message = MessageBuilder::default();
                    if private {
                        message.push(match async_kind {
                            AsyncKind::Qualifier1 => "qualifier async 1",
                            AsyncKind::Qualifier2 => "qualifier async 2",
                            AsyncKind::Qualifier3 => "qualifier async 3",
                            AsyncKind::Seeding => "seeding async",
                            AsyncKind::Tiebreaker1 => "tiebreaker async 1",
                            AsyncKind::Tiebreaker2 => "tiebreaker async 2",
                        });
                        message.push(": ");
                    } else {
                        message.push("Please welcome ");
                    }
                    message.mention_team(&mut transaction, Some(discord_guild), &team).await?;
                    if !private {
                        message.push(" who");
                    }
                    if let Some(sum) = times.iter().take(players.len()).try_fold(Duration::default(), |acc, &time| Some(acc + time?)) {
                        message.push(" finished with a time of ");
                        message.push(English.format_duration(sum / u32::try_from(players.len()).expect("too many players in team"), true));
                        message.push('!');
                    } else {
                        message.push(" did not finish.");
                    }
                    match players.into_iter().zip(&times).zip(&vods).exactly_one() {
                        Ok(((_, _), vod)) => if vod.is_empty() {
                            message.push_line("");
                        } else {
                            message.push(' ');
                            message.push_line_safe(vod);
                        },
                        Err(data) => {
                            message.push_line("");
                            for (i, ((player, time), vod)) in data.enumerate() {
                                if let Some(player) = User::from_id(&mut *transaction, player).await? {
                                    message.mention_user(&player);
                                } else {
                                    message.push("player ");
                                    message.push((i + 1).to_string());
                                }
                                message.push(": ");
                                if let Some(time) = *time {
                                    message.push(English.format_duration(time, false));
                                } else {
                                    message.push("DNF");
                                }
                                if vod.is_empty() {
                                    message.push_line("");
                                } else {
                                    message.push(' ');
                                    message.push_line_safe(vod);
                                }
                            }
                        }
                    }
                    if !value.fpa.is_empty() {
                        message.push("FPA call:");
                        message.quote_rest();
                        message.push_safe(&value.fpa);
                    }
                    discord_channel.send_message(&*discord_ctx.read().await, CreateMessage::default()
                        .content(message.build())
                        .flags(MessageFlags::SUPPRESS_EMBEDS)
                    ).await?;
                }
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(status(series, event))))
        }
    } else {
        transaction.rollback().await?;
        RedirectOrContent::Content(status_page(pool.begin().await?, http_client, Some(me), uri, csrf.as_ref(), data, StatusContext::SubmitAsync(form.context)).await?)
    })
}

#[rocket::get("/event/<series>/<event>/practice")]
pub(crate) async fn practice_seed(pool: &State<PgPool>, ootr_api_client: &State<Arc<ootr_web::ApiClient>>, series: Series, event: &str) -> Result<Redirect, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    transaction.commit().await?;
    let version = data.rando_version.ok_or(StatusOrError::Status(Status::NotFound))?;
    let settings = data.single_settings.ok_or(StatusOrError::Status(Status::NotFound))?;
    let world_count = settings.get("world_count").map_or(1, |world_count| world_count.as_u64().expect("world_count setting wasn't valid u64").try_into().expect("too many worlds"));
    let web_version = ootr_api_client.can_roll_on_web(None, &version, world_count, UnlockSpoilerLog::Now).await.ok_or(StatusOrError::Status(Status::NotFound))?;
    let id = Arc::clone(ootr_api_client).roll_practice_seed(web_version, false, settings).await?;
    Ok(Redirect::to(format!("https://ootrandomizer.com/seed/get?id={id}")))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SwissStandingsError {
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Event(#[from] Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

impl<E: Into<SwissStandingsError>> From<E> for StatusOrError<SwissStandingsError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

impl IsNetworkError for SwissStandingsError {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Data(_) => false,
            Self::Event(e) => e.is_network_error(),
            Self::Page(e) => e.is_network_error(),
            Self::Reqwest(e) => e.is_network_error(),
            Self::Sql(_) => false,
        }
    }
}

impl<'r> rocket::response::Responder<'r, 'static> for SwissStandingsError {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'static> {
        let status = if self.is_network_error() {
            Status::BadGateway
        } else {
            Status::InternalServerError
        };
        eprintln!("responded with {status} to request to {}", request.uri());
        eprintln!("display: {self}");
        eprintln!("debug: {self:?}");
        Err(status)
    }
}

#[rocket::get("/event/<series>/<event>/swiss-standings")]
pub(crate) async fn swiss_standings(
    pool: &State<PgPool>,
    http_client: &State<reqwest::Client>,
    config: &State<Config>,
    me: Option<User>,
    uri: Origin<'_>,
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<SwissStandingsError>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Only show for Startgg events with swiss_standings enabled
    if !matches!(data.match_source(), MatchSource::StartGG(_)) || !data.swiss_standings {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    
    let header = data.header(&mut transaction, me.as_ref(), Tab::SwissStandings, false).await?;
    
    // Extract the Startgg slug from the event URL
    let slug = match data.url.as_ref().and_then(|url| url.path().strip_prefix('/').map(|s| s.to_string())) {
        Some(s) if !s.is_empty() => s,
        _ => return Err(StatusOrError::Status(Status::NotFound)),
    };
    
    // Get the Startgg token
    let startgg_token = &config.startgg;
    
    // Get resigned teams for this event to exclude them from bye prediction
    let resigned_entrant_ids = sqlx::query!(
        r#"SELECT startgg_id FROM teams 
           WHERE series = $1 AND event = $2 AND resigned = TRUE AND startgg_id IS NOT NULL"#,
        series as _,
        event
    )
            .fetch_all(&mut *transaction)
    .await
    .ok()
    .map(|rows| rows.into_iter()
        .filter_map(|row| row.startgg_id)
        .map(|id| id.to_string())
        .collect::<HashSet<_>>());

    // Fetch Swiss standings
    let standings = match startgg::swiss_standings(
        http_client.inner(),
        &*config,
        &slug,
        startgg_token,
        resigned_entrant_ids.as_ref(),
    ).await {
        Ok(standings) => standings,
        Err(_) => {
            // Return empty standings if API call fails
            Vec::new()
        }
    };
    
    let content = html! {
        : header;
        h2 : "Swiss Standings";
        p(style = "font-style: italic; color: var(--text-muted); margin-bottom: 1rem;") : "This page automatically updates every 30 minutes.";
        @if standings.is_empty() {
            p : "No Swiss standings available at this time.";
        } else {
            table {
                thead {
                    tr {
                        th : "Placement";
                        th : "Name";
                        th : "Swiss Result";
                    }
                }
                tbody {
                    @for standing in &standings {
                        tr {
                            td : standing.placement.to_string();
                            td : &standing.name;
                            td : format!("{}:{}", standing.wins, standing.losses);
                        }
                    }
                }
            }
        }
    };
    
    Ok(page(transaction, &me, &uri, PageStyle::default(), "Swiss Standings", content).await?)
}
