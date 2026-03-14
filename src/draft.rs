use {
    std::cmp::{
        max_by_key,
        min_by_key,
    },
    crate::{
        event::teams::{
            self,
            SignupsTeam,
        },
        prelude::*,
    },
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] RslScriptPath(#[from] rsl::ScriptPathError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("unexpected type of `extra_` option in RSL override")]
    RslExtraType,
    #[error("unexpected type of `remove_` option in RSL override")]
    RslRemoveType,
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::RslScriptPath(_) => false,
            Self::Sql(_) => false,
            Self::Wheel(e) => e.is_network_error(),
            Self::RslExtraType => false,
            Self::RslRemoveType => false,
        }
    }
}

pub(crate) type Picks = HashMap<Cow<'static, str>, Cow<'static, str>>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Team {
    HighSeed,
    LowSeed,
}

impl Team {
    pub(crate) fn choose<T>(&self, high_seed: T, low_seed: T) -> T {
        match self {
            Self::HighSeed => high_seed,
            Self::LowSeed => low_seed,
        }
    }
}

impl fmt::Display for Team {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HighSeed => write!(f, "Team A"),
            Self::LowSeed => write!(f, "Team B"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresetOption {
    pub(crate) display_name: &'static str,
    pub(crate) preset: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DraftPhase {
    Ban(Team),
    Pick(Team),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    S7,
    MultiworldS3,
    MultiworldS4,
    MultiworldS5,
    RslS7,
    TournoiFrancoS3,
    TournoiFrancoS4,
    TournoiFrancoS5,
    PickOnly {
        options: &'static [PresetOption],
        who_starts: Team,
        picks_per_player: u8,
        unique: bool,
        label: &'static str,
    },
    #[allow(dead_code)]
    BanPick {
        options: &'static [PresetOption],
        order: &'static [DraftPhase],
        label: &'static str,
    },
    BanOnly {
        options: &'static [PresetOption],
        order: &'static [DraftPhase],
        label: &'static str,
    },
}

impl Kind {
    pub(crate) fn uses_button_draft(self) -> bool {
        matches!(self, Self::PickOnly { .. } | Self::BanPick { .. } | Self::BanOnly { .. })
    }

    /// Look up the display name for a preset identifier in this draft kind's options.
    pub(crate) fn preset_display_name(&self, preset: &str) -> Option<&'static str> {
        let options: &[PresetOption] = match self {
            Self::PickOnly { options, .. } | Self::BanPick { options, .. } | Self::BanOnly { options, .. } => options,
            _ => return None,
        };
        options.iter().find(|p| p.preset == preset).map(|p| p.display_name)
    }

    fn language(&self) -> Language {
        match self {
            | Self::S7
            | Self::MultiworldS3
            | Self::MultiworldS4
            | Self::MultiworldS5
            | Self::RslS7
            | Self::TournoiFrancoS4
            | Self::TournoiFrancoS5
            | Self::PickOnly { .. }
            | Self::BanPick { .. }
            | Self::BanOnly { .. }
                => English,
            | Self::TournoiFrancoS3
                => French,
        }
    }
}

#[derive(Clone)]
pub(crate) struct BanSetting {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) default: &'static str,
    pub(crate) default_display: &'static str,
    pub(crate) description: Cow<'static, str>,
}

pub(crate) struct BanSettings(Vec<(&'static str, Vec<BanSetting>)>);

impl BanSettings {
    pub(crate) fn num_settings(&self) -> usize {
        self.0.iter().map(|(_, page)| page.len()).sum()
    }

    pub(crate) fn page(&self, idx: usize) -> Option<(&'static str, &[BanSetting])> {
        self.0.get(idx).map(|(name, settings)| (*name, &**settings))
    }

    pub(crate) fn all(self) -> impl Iterator<Item = BanSetting> {
        self.0.into_iter().flat_map(|(_, settings)| settings)
    }

    pub(crate) fn get(&self, name: &str) -> Option<BanSetting> {
        self.0.iter().flat_map(|(_, settings)| settings).find(|setting| setting.name == name).cloned()
    }
}

#[derive(Clone)]
pub(crate) struct DraftSettingChoice {
    pub(crate) name: &'static str,
    pub(crate) display: Cow<'static, str>,
}

#[derive(Clone)]
pub(crate) struct DraftSetting {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) options: Vec<DraftSettingChoice>,
    pub(crate) description: Cow<'static, str>,
}

pub(crate) struct DraftSettings(Vec<(&'static str, Vec<DraftSetting>)>);

impl DraftSettings {
    pub(crate) fn num_settings(&self) -> usize {
        self.0.iter().map(|(_, page)| page.len()).sum()
    }

    pub(crate) fn page(&self, idx: usize) -> Option<(&'static str, &[DraftSetting])> {
        self.0.get(idx).map(|(name, settings)| (*name, &**settings))
    }

    pub(crate) fn all(self) -> impl Iterator<Item = DraftSetting> {
        self.0.into_iter().flat_map(|(_, settings)| settings)
    }

    pub(crate) fn get(&self, name: &str) -> Option<DraftSetting> {
        self.0.iter().flat_map(|(_, settings)| settings).find(|setting| setting.name == name).cloned()
    }
}

pub(crate) enum StepKind {
    /// The high seed chooses whether to go first or second.
    GoFirst,
    /// The given team sets one of the available settings to its default value.
    Ban {
        team: Team,
        /// Grouped into named pages in case they exceed the button limit for Discord message components.
        available_settings: BanSettings,
        skippable: bool,
        /// In RSL, bans are called blocks, and picks are called bans.
        rsl: bool,
    },
    Pick {
        team: Team,
        /// Grouped into named pages in case they exceed the button limit for Discord message components.
        available_choices: DraftSettings,
        skippable: bool,
        /// In RSL, bans are called blocks, and picks are called bans.
        rsl: bool,
    },
    BooleanChoice {
        team: Team,
    },
    Done(seed::Settings), //TODO use ootr_utils::Settings instead?
    DoneRsl {
        preset: rsl::VersionedPreset,
        world_count: u8,
    },
    /// A single-step preset picker: clicking one button picks the preset for the given game.
    PickPreset {
        team: Team,
        available_presets: Vec<PresetOption>,
        game: u8,
    },
}

pub(crate) struct Step {
    pub(crate) kind: StepKind,
    pub(crate) message: String,
}

pub(crate) enum MessageContext<'a> {
    None,
    Discord {
        transaction: Transaction<'a, Postgres>,
        guild_id: GuildId,
        command_ids: CommandIds,
        teams: Vec<team::Team>,
        team: team::Team,
    },
    RaceTime {
        high_seed_name: &'a str,
        low_seed_name: &'a str,
        reply_to: &'a str,
    },
}

impl<'a> MessageContext<'a> {
    //HACK: convenience method to get the database transaction back out of MessageContext::Discord. Panics if called on another variant
    pub(crate) fn into_transaction(self) -> Transaction<'a, Postgres> {
        let Self::Discord { transaction, .. } = self else { panic!("called into_transaction on non-Discord draft message context") };
        transaction
    }
}

pub(crate) enum Action {
    GoFirst(bool),
    Ban {
        setting: String,
    },
    Pick {
        setting: String,
        value: String,
    },
    Skip,
    BooleanChoice(bool),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Draft {
    pub(crate) high_seed: Id<Teams>,
    pub(crate) went_first: Option<bool>,
    #[serde(default)]
    pub(crate) skipped_bans: u8,
    #[serde(flatten)]
    pub(crate) settings: Picks,
}

impl Draft {
    pub(crate) async fn for_game1(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, kind: Kind, event: &event::Data<'_>, phase: Option<&str>, [team1, team2]: [&team::Team; 2]) -> Result<Self, cal::Error> {
        let [high_seed, low_seed] = match kind {
            Kind::S7 | Kind::RslS7 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => [
                min_by_key(team1, team2, |team| team.qualifier_rank).id,
                max_by_key(team1, team2, |team| team.qualifier_rank).id,
            ],
            Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 => if phase.is_some_and(|phase| phase == "Top 8") {
                let seeding = match kind {
                    Kind::MultiworldS3 => [
                        Id::from(5834711445123920517_u64), // DAD
                        Id::from(167966029947875858_u64), // Snack Pack
                        Id::from(541730158313016345_u64), // The Highest Gorons
                        Id::from(8470597845703477673_u64), // Pirate Ship
                        Id::from(3240373008633749917_u64), // The Good, The Bad and The Shopskipper
                        Id::from(4729976791199641222_u64), // SariasObjects
                        Id::from(4543618089366873966_u64), // Anju's Secret
                        Id::from(3547322866530836817_u64), // Raid: Shadow Temple
                    ],
                    Kind::MultiworldS4 => [
                        Id::from(8429274534302278572_u64), // Anju's Secret
                        Id::from(12142947927479333421_u64), // ADD
                        Id::from(5548902498821246494_u64), // Donutdog!!! Wuff! Wuff!
                        Id::from(7622448514297787774_u64), // Snack Pack
                        Id::from(13644615382444869291_u64), // The Highest Gorons
                        Id::from(4984265622447250649_u64), // Bongo Akimbo
                        Id::from(592664405695569367_u64), // The Jhegsons
                        Id::from(14405144517033747435_u64), // Pandora's Brot
                    ],
                    Kind::MultiworldS5 => [
                        Id::from(15744003931430480727_u64), // Schulzer, Jay and Bobby are here
                        Id::from(1129367405239208119_u64), // Pandora's Brot
                        Id::from(17104756766000096702_u64), // Moeko Appreciation Society
                        Id::from(32215927528820085_u64), // Captain Levi's Fish and Chimp Shop
                        Id::from(4924230698541822293_u64), // Trois clampins et une chaussette
                        Id::from(17175023655531815569_u64), // SariasObjects
                        Id::from(5779097592179749366_u64), // Clube Athletico Paranaense
                        Id::from(3033012543915648585_u64), // DAChuck
                    ],
                    _ => unreachable!("checked by outer match"),
                };
                let mut team_ids = [team1.id, team2.id];
                team_ids.sort_unstable_by_key(|team| seeding.iter().position(|iter_team| iter_team == team));
                team_ids
            } else {
                let qualifier_kind = event.qualifier_kind(&mut *transaction, None).await?;
                let signups = teams::signups_sorted(&mut *transaction, &mut teams::Cache::new(http_client.clone()), None, event, false, qualifier_kind, None, true, false).await?;
                let SignupsTeam { members: members1, .. } = signups.iter().find(|SignupsTeam { team, .. }| team.as_ref().is_some_and(|team| team == team1)).expect("match with team that didn't sign up");
                let SignupsTeam { members: members2, .. } = signups.iter().find(|SignupsTeam { team, .. }| team.as_ref().is_some_and(|team| team == team2)).expect("match with team that didn't sign up");
                let avg1 = members1.iter().try_fold(Duration::default(), |acc, member| Some(acc + member.qualifier_time?)).map(|total| total / u32::try_from(members1.len()).expect("too many team members"));
                let avg2 = members2.iter().try_fold(Duration::default(), |acc, member| Some(acc + member.qualifier_time?)).map(|total| total / u32::try_from(members2.len()).expect("too many team members"));
                match [avg1, avg2] {
                    [Some(_), None] => [team1.id, team2.id],
                    [None, Some(_)] => [team2.id, team1.id],
                    [Some(avg1), Some(avg2)] if avg1 < avg2 => [team1.id, team2.id],
                    [Some(avg1), Some(avg2)] if avg1 > avg2 => [team2.id, team1.id],
                    _ => {
                        // tie broken by coin flip
                        let mut team_ids = [team1.id, team2.id];
                        team_ids.shuffle(&mut rng());
                        team_ids
                    }
                }
            },
            Kind::TournoiFrancoS3 | Kind::TournoiFrancoS4 | Kind::TournoiFrancoS5 => {
                let mut team_ids = [team1.id, team2.id];
                team_ids.shuffle(&mut rng());
                team_ids
            }
        };
        Ok(Self::for_next_game(transaction, kind, high_seed, low_seed).await?)
    }

    pub(crate) async fn for_next_game(transaction: &mut Transaction<'_, Postgres>, kind: Kind, loser: Id<Teams>, winner: Id<Teams>) -> sqlx::Result<Self> {
        Ok(Self {
            high_seed: loser,
            went_first: None,
            skipped_bans: 0,
            settings: match kind {
                Kind::S7 | Kind::MultiworldS3 | Kind::MultiworldS5 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => HashMap::default(),
                // accessibility accommodation for The Aussie Boiiz in mw/4 to default to CSMC
                Kind::MultiworldS4 => HashMap::from_iter(
                    (loser == Id::from(17814073240662869290_u64) || winner == Id::from(17814073240662869290_u64))
                        .then_some((Cow::Borrowed("special_csmc"), Cow::Borrowed("yes"))),
                ),
                Kind::RslS7 => {
                    let team_rows = sqlx::query!("SELECT lite_ok FROM teams WHERE id = $1 OR id = $2", loser as _, winner as _).fetch_all(&mut **transaction).await?;
                    let lite_ok = team_rows.iter().all(|row| row.lite_ok);
                    collect![as HashMap<_, _>:
                        Cow::Borrowed("lite_ok") => Cow::Borrowed(if lite_ok { "ok" } else { "no" }),
                    ]
                }
                Kind::TournoiFrancoS3 | Kind::TournoiFrancoS4 | Kind::TournoiFrancoS5 => {
                    let team_rows = sqlx::query!("SELECT hard_settings_ok, mq_ok FROM teams WHERE id = $1 OR id = $2", loser as _, winner as _).fetch_all(&mut **transaction).await?;
                    let hard_settings_ok = team_rows.iter().all(|row| row.hard_settings_ok);
                    let mq_ok = team_rows.iter().all(|row| row.mq_ok);
                    collect![as HashMap<_, _>:
                        Cow::Borrowed("hard_settings_ok") => Cow::Borrowed(if hard_settings_ok { "ok" } else { "no" }),
                        Cow::Borrowed("mq_ok") => Cow::Borrowed(if mq_ok { "ok" } else { "no" }),
                    ]
                }
            },
        })
    }

    fn pick_count(&self, kind: Kind) -> u8 {
        match kind {
            Kind::PickOnly { picks_per_player, .. } => {
                let total = picks_per_player as usize * 2;
                u8::try_from((1..=total).filter(|&n| self.settings.contains_key(&*format!("game{n}_preset"))).count()).unwrap()
            }
            Kind::BanPick { order, .. } => {
                let mut ban_count = 0usize;
                let mut pick_count = 0usize;
                let mut done = 0u8;
                for phase in order {
                    match phase {
                        DraftPhase::Ban(_) => {
                            ban_count += 1;
                            if self.settings.contains_key(&*format!("ban{ban_count}")) { done += 1; }
                        }
                        DraftPhase::Pick(_) => {
                            pick_count += 1;
                            if self.settings.contains_key(&*format!("pick{pick_count}")) { done += 1; }
                        }
                    }
                }
                done
            }
            Kind::BanOnly { order, .. } => {
                let mut ban_count = 0usize;
                let mut done = 0u8;
                for phase in order {
                    if let DraftPhase::Ban(_) = phase {
                        ban_count += 1;
                        if self.settings.contains_key(&*format!("ban{ban_count}")) { done += 1; }
                    }
                }
                done
            }
            Kind::S7 => self.skipped_bans + u8::try_from(self.settings.len()).unwrap(),
            Kind::RslS7 => self.skipped_bans
                + u8::try_from(rsl::FORCE_OFF_SETTINGS.into_iter().filter(|&rsl::ForceOffSetting { name, .. }| self.settings.contains_key(name)).count()).unwrap()
                + u8::try_from(rsl::FIFTY_FIFTY_SETTINGS.into_iter().chain(rsl::MULTI_OPTION_SETTINGS).filter(|&rsl::MultiOptionSetting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
            Kind::MultiworldS3 => self.skipped_bans + u8::try_from(mw::S3_SETTINGS.iter().copied().filter(|&mw::Setting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
            Kind::MultiworldS4 => self.skipped_bans + u8::try_from(mw::S4_SETTINGS.iter().copied().filter(|&mw::Setting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
            Kind::MultiworldS5 => self.skipped_bans + u8::try_from(mw::S5_SETTINGS.iter().copied().filter(|&mw::Setting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
            Kind::TournoiFrancoS3 => self.skipped_bans + u8::try_from(fr::S3_SETTINGS.into_iter().filter(|&fr::Setting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
            Kind::TournoiFrancoS4 => self.skipped_bans + u8::try_from(fr::S4_SETTINGS.into_iter().filter(|&fr::Setting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
            Kind::TournoiFrancoS5 => self.skipped_bans + u8::try_from(fr::S5_SETTINGS.into_iter().filter(|&fr::Setting { name, .. }| self.settings.contains_key(name)).count()).unwrap(),
        }
    }



    async fn next_step_s7(&self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let kind = Kind::S7;
        Ok({
            if let Some(went_first) = self.went_first {
                match self.pick_count(kind) {
                    prev_bans @ 0..=1 => {
                        let team = match (prev_bans, went_first) {
                            (0, true) | (1, false) => Team::HighSeed,
                            (0, false) | (1, true) => Team::LowSeed,
                            (2.., _) => unreachable!(),
                        };
                        let (major_setings, minor_settings) = s::S7_SETTINGS.into_iter().partition::<Vec<_>, _>(|&s::Setting { major, .. }| major);
                        Step {
                            kind: StepKind::Ban {
                                available_settings: BanSettings(vec![
                                    ("Major Settings", major_setings.into_iter()
                                        .filter(|&s::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|setting @ s::Setting { name, display, default_display, .. }| BanSetting {
                                            default: "default",
                                            description: Cow::Owned(setting.description()),
                                            name, display, default_display,
                                        })
                                        .collect()),
                                    ("Minor Settings", minor_settings.into_iter()
                                        .filter(|&s::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|setting @ s::Setting { name, display, default_display, .. }| BanSetting {
                                            default: "default",
                                            description: Cow::Owned(setting.description()),
                                            name, display, default_display,
                                        })
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                        .push(": lock a setting to its default using ")
                                        .mention_command(command_ids.ban.unwrap(), "ban")
                                        .push(", or use ")
                                        .mention_command(command_ids.skip.unwrap(), "skip")
                                        .push(" if you don't want to ban anything.")
                                        .build()
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => format!(
                                    "{}, lock a setting to its default using “!ban <setting>”, or use “!skip” if you don't want to ban anything.{}",
                                    team.choose(high_seed_name, low_seed_name),
                                    if prev_bans == 0 { " Use “!settings” for a list of available settings." } else { "" },
                                ),
                            },
                        }
                    }
                    n @ 2..=5 => {
                        let team = match (n, went_first) {
                            (2, true) | (3, false) | (4, false) | (5, true) => Team::HighSeed,
                            (2, false) | (3, true) | (4, true) | (5, false) => Team::LowSeed,
                            (0..=1 | 6.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Pick {
                                available_choices: DraftSettings(vec![
                                    (if n < 4 { "Major Settings" } else { "Minor Settings" }, s::S7_SETTINGS.into_iter()
                                        .filter(|&s::Setting { name, major, .. }| major == (n < 4) && !self.settings.contains_key(name))
                                        .map(|setting @ s::Setting { name, display, other, .. }| DraftSetting {
                                            options: other.iter().map(|&(name, display, _)| DraftSettingChoice { name, display: display.into() }).collect(),
                                            description: Cow::Owned(setting.description()),
                                            name, display,
                                        })
                                        .collect()),
                                ]),
                                skippable: false,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    match n {
                                        2 | 3 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a major setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        4 | 5 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a minor setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        0..=1 | 6.. => unreachable!(),
                                    }
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => match n {
                                    2 => format!("{}, pick a major setting using “!pick <setting> <value>”", team.choose(high_seed_name, low_seed_name)),
                                    3 => format!("{}, pick a major setting.", team.choose(high_seed_name, low_seed_name)),
                                    4 | 5 => format!("{}, pick a minor setting.", team.choose(high_seed_name, low_seed_name)),
                                    0..=1 | 6.. => unreachable!(),
                                },
                            },
                        }
                    }
                    6.. => Step {
                        kind: StepKind::Done(s::resolve_s7_draft_settings(&self.settings)),
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { .. } => format!("Settings draft completed. You will be playing with {}.", s::display_s7_draft_picks(&self.settings)),
                            MessageContext::RaceTime { .. } => s::display_s7_draft_picks(&self.settings),
                        },
                    },
                }
            } else {
                Step {
                    kind: StepKind::GoFirst,
                    message: match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            if game.is_some_and(|game| game > 1) {
                                builder.push(": as the loser of the previous race, please choose whether you want to go ");
                            } else {
                                builder.push(": you have the higher seed. Choose whether you want to go ");
                            }
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(" or ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            if let Some(game) = game {
                                builder.push(" in the settings draft for game ");
                                builder.push(game.to_string());
                                builder.push('.');
                            } else {
                                builder.push(" in the settings draft.");
                            }
                            builder
                                .push(" You can also wait until the race room is opened to draft your settings.")
                                .build()
                        }
                        MessageContext::RaceTime { high_seed_name, .. } => format!("{high_seed_name}, you have the higher seed. Choose whether you want to go !first or !second"),
                    },
                }
            }
        })
    }

    async fn next_step_rsl_s7(&self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let kind = Kind::RslS7;
        Ok({
            if let Some(went_first) = self.went_first {
                let is_lite = self.settings.get("preset").map(|preset| &**preset).unwrap_or("league") == "lite";
                match (is_lite, self.pick_count(kind)) {
                    (true, n @ (0 | 1)) | (false, n @ (0 | 1 | 4 | 5)) => {
                        let team = match (n, went_first) {
                            (0, true) | (1, false) | (4, true) | (5, false) => Team::HighSeed,
                            (0, false) | (1, true) | (4, false) | (5, true) => Team::LowSeed,
                            (2..=3 | 6.., _) => unreachable!(),
                        };
                        let mut multi_options_settings = Vec::default();
                        let rsl_script_path = rsl::VersionedPreset::XoparCustom { version: None, weights: rsl::Weights::default() }.script_path().await?;
                        for setting in rsl::MULTI_OPTION_SETTINGS.into_iter()
                            // Weights in blocked settings may not be banned
                            .filter(|&rsl::MultiOptionSetting { name, .. }| self.settings.get(name).is_none_or(|value| value != "blocked"))
                            // Each player may only ban one weight within each setting
                            .filter(|&rsl::MultiOptionSetting { name, .. }| self.settings.get(&*format!("{name}_banned_by")).is_none_or(|banned_by| !banned_by.split(',').any(|banned_by| banned_by == team.to_string())))
                        {
                            // A weight may not be banned if that would leave its setting with no nonzero weights
                            let mut options = Vec::default();
                            for (name, display, lite, ban) in setting.options {
                                if is_lite && !lite { continue }
                                let mut weights = rsl::resolve_s7_draft_weights(&rsl_script_path, &self.settings).await?;
                                if let Some(ban) = ban {
                                    ban(&mut weights);
                                } else {
                                    weights.weights.get_mut(setting.name).unwrap().remove(*name);
                                }
                                if weights.weights.into_values().all(|weight| weight.into_values().any(|value| value > 0)) {
                                    options.push((name, display));
                                }
                            }
                            if let Ok(options) = NEVec::try_from(options) {
                                multi_options_settings.push(DraftSetting {
                                    name: setting.name,
                                    display: setting.display,
                                    options: options.iter().map(|(name, display)| DraftSettingChoice { name, display: format!("{}: {display}", setting.display).into() }).collect(),
                                    description: Cow::Owned(format!("{}: {}", setting.name, English.join_str_with("or", options.into_nonempty_iter().map(|(name, _)| name)))),
                                });
                            }
                        }
                        Step {
                            kind: StepKind::Pick {
                                available_choices: DraftSettings(vec![
                                    ("“Force Off” Settings", rsl::FORCE_OFF_SETTINGS.into_iter()
                                        .filter(|&rsl::ForceOffSetting { name, lite, .. }| !self.settings.contains_key(name) && (!is_lite || lite))
                                        .map(|rsl::ForceOffSetting { name, display, .. }|
                                            DraftSetting {
                                                options: vec![DraftSettingChoice { name: "banned", display: display.into() }],
                                                description: Cow::Owned(format!("{name}: banned")),
                                                name, display,
                                            }
                                        )
                                        .collect()),
                                    ("“50/50” Settings", rsl::FIFTY_FIFTY_SETTINGS.into_iter()
                                        .filter(|&rsl::MultiOptionSetting { name, options, .. }| !self.settings.contains_key(name) && (!is_lite || options.iter().any(|(_, _, lite, _)| *lite)))
                                        .map(|rsl::MultiOptionSetting { name, display: setting_display, options, .. }|
                                            DraftSetting {
                                                display: setting_display,
                                                options: options.iter().filter(|(_, _, lite, _)| !is_lite || *lite).map(|(name, display, _, _)| DraftSettingChoice { name, display: format!("{setting_display}: {display}").into() }).collect(),
                                                description: Cow::Owned(format!("{name}: {}", English.join_str_with("or", options.iter().try_into_nonempty_iter().expect("has at least one option").map(|(name, _, _, _)| name)))),
                                                name,
                                            }
                                        )
                                        .collect()),
                                    ("“Multiple Options” Settings", multi_options_settings),
                                ]),
                                skippable: true,
                                rsl: true,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                        .push(": ban a setting using ")
                                        .mention_command(command_ids.pick.unwrap(), "ban")
                                        .push(", or use ")
                                        .mention_command(command_ids.skip.unwrap(), "skip")
                                        .push(" if you don't want to ban anything.")
                                        .build()
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => format!(
                                    "{}, ban a setting using “!ban <setting> <value>”, or use “!skip” if you don't want to ban anything.{}",
                                    team.choose(high_seed_name, low_seed_name),
                                    if n == 0 { " Use “!settings” for a list of available weights." } else { "" },
                                ),
                            },
                        }
                    }
                    (false, n @ (2 | 3)) => {
                        let team = match (n, went_first) {
                            (2, false) | (3, true) => Team::HighSeed,
                            (2, true) | (3, false) => Team::LowSeed,
                            (0..=1 | 4.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Ban {
                                available_settings: BanSettings(vec![
                                    ("“Force Off” Settings", rsl::FORCE_OFF_SETTINGS.into_iter()
                                        .filter(|&rsl::ForceOffSetting { name, lite, .. }| !self.settings.contains_key(name) && (!is_lite || lite))
                                        .map(|rsl::ForceOffSetting { name, display, .. }|
                                            BanSetting {
                                                default: "blocked",
                                                default_display: display,
                                                description: Cow::Owned(format!("{name}: blocked")),
                                                name, display,
                                            }
                                        )
                                        .collect()),
                                    ("“50/50” Settings", rsl::FIFTY_FIFTY_SETTINGS.into_iter()
                                        .filter(|&rsl::MultiOptionSetting { name, options, .. }| !self.settings.contains_key(name) && (!is_lite || options.iter().any(|(_, _, lite, _)| *lite)))
                                        .map(|rsl::MultiOptionSetting { name, display, .. }|
                                            BanSetting {
                                                default: "blocked",
                                                default_display: display,
                                                description: Cow::Owned(format!("{name}: blocked")),
                                                name, display,
                                            }
                                        )
                                        .collect()),
                                    ("“Multiple Options” Settings", rsl::MULTI_OPTION_SETTINGS.into_iter()
                                        .filter(|&rsl::MultiOptionSetting { name, options, .. }| !self.settings.contains_key(name) && (!is_lite || options.iter().any(|(_, _, lite, _)| *lite)))
                                        .map(|rsl::MultiOptionSetting { name, display, .. }|
                                            BanSetting {
                                                default: "blocked",
                                                default_display: display,
                                                description: Cow::Owned(format!("{name}: blocked")),
                                                name, display,
                                            }
                                        )
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: true,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                        .push(": block a weight from being modified using ")
                                        .mention_command(command_ids.ban.unwrap(), "block")
                                        .push(", or use ")
                                        .mention_command(command_ids.skip.unwrap(), "skip")
                                        .push(" if you don't want to block anything.")
                                        .build()
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => format!(
                                    "{}, block a weight from being modified using “!block <setting>”, or use “!skip” if you don't want to block anything.",
                                    team.choose(high_seed_name, low_seed_name),
                                ),
                            },
                        }
                    }
                    (true, 2..) | (false, 6..) => Step {
                        kind: StepKind::DoneRsl {
                            preset: rsl::VersionedPreset::XoparCustom {
                                version: None,
                                weights: rsl::resolve_s7_draft_weights(
                                    &rsl::VersionedPreset::XoparCustom { version: None, weights: rsl::Weights::default() }.script_path().await?,
                                    &self.settings,
                                ).await?,
                            },
                            world_count: 1,
                        },
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { .. } => format!("Weights draft completed. You will be playing with {}.", rsl::display_s7_draft_picks(&self.settings)),
                            MessageContext::RaceTime { .. } => rsl::display_s7_draft_picks(&self.settings),
                        },
                    },
                }
            } else {
                Step {
                    kind: StepKind::GoFirst,
                    message: match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            if game.is_some_and(|game| game > 1) {
                                builder.push(": as the loser of the previous race, please choose whether you want to go ");
                            } else {
                                builder.push(": you have the higher seed. Choose whether you want to go ");
                            }
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(" or ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            if let Some(game) = game {
                                builder.push(" in the settings draft for game ");
                                builder.push(game.to_string());
                                builder.push('.');
                            } else {
                                builder.push(" in the settings draft.");
                            }
                            if self.settings.get("lite_ok").map(|lite_ok| &**lite_ok).unwrap_or("no") == "ok" {
                                builder.push(" Please consult with your opponent and specify whether you would like to use RSL-Lite weights using the ");
                                builder.push_mono("lite");
                                builder.push(" parameter.");
                            }
                            builder.build()
                        }
                        MessageContext::RaceTime { high_seed_name, .. } => format!("{high_seed_name}, you have the higher seed. Choose whether you want to go !first or !second"),
                    },
                }
            }
        })
    }

    async fn next_step_multiworld_s3(&self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let kind = Kind::MultiworldS3;
        Ok({
            if let Some(went_first) = self.went_first {
                match self.pick_count(kind) {
                    prev_bans @ 0..=1 => {
                        let team = match (prev_bans, went_first) {
                            (0, true) | (1, false) => Team::HighSeed,
                            (0, false) | (1, true) => Team::LowSeed,
                            (2.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Ban {
                                available_settings: BanSettings(vec![
                                    ("All Settings", mw::S3_SETTINGS.iter().copied()
                                        .filter(|&mw::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|mw::Setting { name, display, default, default_display, description, .. }| BanSetting {
                                            description: Cow::Borrowed(description),
                                            name, display, default, default_display,
                                        })
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                        .push(": lock a setting to its default using ")
                                        .mention_command(command_ids.ban.unwrap(), "ban")
                                        .push(", or use ")
                                        .mention_command(command_ids.skip.unwrap(), "skip")
                                        .push(" if you don't want to ban anything.")
                                        .build()
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => format!(
                                    "{}, lock a setting to its default using “!ban <setting>”, or use “!skip” if you don't want to ban anything.{}",
                                    team.choose(high_seed_name, low_seed_name),
                                    if prev_bans == 0 { " Use “!settings” for a list of available settings." } else { "" },
                                ),
                            },
                        }
                    }
                    n @ 2..=5 => {
                        let team = match (n, went_first) {
                            (2, true) | (3, false) | (4, false) | (5, true) => Team::HighSeed,
                            (2, false) | (3, true) | (4, true) | (5, false) => Team::LowSeed,
                            (0..=1 | 6.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Pick {
                                available_choices: DraftSettings(vec![
                                    ("All Settings", mw::S3_SETTINGS.iter().copied()
                                        .filter(|&mw::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|mw::Setting { name, display, default, default_display, other, description }| DraftSetting {
                                            options: iter::once(DraftSettingChoice { name: default, display: default_display.into() })
                                                .chain(other.iter().map(|&(name, display)| DraftSettingChoice { name, display: display.into() }))
                                                .collect(),
                                            description: Cow::Borrowed(description),
                                            name, display,
                                        })
                                        .collect()),
                                ]),
                                skippable: n == 5,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    match n {
                                        2 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        3 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push(". You will have another pick after this.")
                                            .build(),
                                        4 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick your second setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        5 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push(". You can also use ")
                                            .mention_command(command_ids.skip.unwrap(), "skip")
                                            .push(" if you want to leave the settings as they are.")
                                            .build(),
                                        0..=1 | 6.. => unreachable!(),
                                    }
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => match n {
                                    2 => format!("{}, pick a setting using “!pick <setting> <value>”", team.choose(high_seed_name, low_seed_name)),
                                    3 => format!("{}, pick two settings.", team.choose(high_seed_name, low_seed_name)),
                                    4 => format!("And your second pick?"),
                                    5 => format!("{}, pick the final setting. You can also use “!skip” if you want to leave the settings as they are.", team.choose(high_seed_name, low_seed_name)),
                                    0..=1 | 6.. => unreachable!(),
                                },
                            },
                        }
                    }
                    6.. => Step {
                        kind: StepKind::Done(mw::resolve_s3_draft_settings(&self.settings)),
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { .. } => format!("Settings draft completed. You will be playing with {}.", mw::display_s3_draft_picks(&self.settings)),
                            MessageContext::RaceTime { .. } => mw::display_s3_draft_picks(&self.settings),
                        },
                    },
                }
            } else {
                Step {
                    kind: StepKind::GoFirst,
                    message: match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            if game.is_some_and(|game| game > 1) {
                                builder.push(": as the losers of the previous race, please choose whether you want to go ");
                            } else {
                                builder.push(": you have the higher seed. Choose whether you want to go ");
                            }
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(" or ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            if let Some(game) = game {
                                builder.push(" in the settings draft for game ");
                                builder.push(game.to_string());
                                builder.push('.');
                            } else {
                                builder.push(" in the settings draft.");
                            }
                            builder.build()
                        }
                        MessageContext::RaceTime { high_seed_name, .. } => format!("{high_seed_name}, you have the higher seed. Choose whether you want to go !first or !second"),
                    },
                }
            }
        })
    }

    async fn next_step_multiworld_s4(&self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let kind = Kind::MultiworldS4;
        Ok({
            if let Some(went_first) = self.went_first {
                match self.pick_count(kind) {
                    prev_bans @ (0..=1 | 6..=7) => {
                        let team = match (prev_bans, went_first) {
                            (0, true) | (1, false) | (6, false) | (7, true) => Team::HighSeed,
                            (0, false) | (1, true) | (6, true) | (7, false) => Team::LowSeed,
                            (2..=5 | 8.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Ban {
                                available_settings: BanSettings(vec![
                                    ("All Settings", mw::S4_SETTINGS.iter().copied()
                                        .filter(|&mw::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|mw::Setting { name, display, default, default_display, description, .. }|
                                            if name == "camc" && self.settings.get("special_csmc").map(|special_csmc| &**special_csmc).unwrap_or("no") == "yes" {
                                                BanSetting {
                                                    default: "both",
                                                    default_display: "chest size & texture match contents",
                                                    description: Cow::Borrowed("camc (Chest Appearance Matches Contents): both (default: size & texture) or off"),
                                                    name, display,
                                                }
                                            } else {
                                                BanSetting {
                                                    description: Cow::Borrowed(description),
                                                    name, display, default, default_display,
                                                }
                                            }
                                        )
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                        .push(": lock a setting to its default using ")
                                        .mention_command(command_ids.ban.unwrap(), "ban")
                                        .push(", or use ")
                                        .mention_command(command_ids.skip.unwrap(), "skip")
                                        .push(" if you don't want to ban anything.")
                                        .build()
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => format!(
                                    "{}, lock a setting to its default using “!ban <setting>”, or use “!skip” if you don't want to ban anything.{}",
                                    team.choose(high_seed_name, low_seed_name),
                                    if prev_bans == 0 { " Use “!settings” for a list of available settings." } else { "" },
                                ),
                            },
                        }
                    }
                    n @ (2..=5 | 8..=9) => {
                        let team = match (n, went_first) {
                            (2, true) | (3, false) | (4, false) | (5, true) | (8, false) | (9, true) => Team::HighSeed,
                            (2, false) | (3, true) | (4, true) | (5, false) | (8, true) | (9, false) => Team::LowSeed,
                            (0..=1 | 6..=7 | 10.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Pick {
                                available_choices: DraftSettings(vec![
                                    ("All Settings", mw::S4_SETTINGS.iter().copied()
                                        .filter(|&mw::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|mw::Setting { name, display, default, default_display, other, description }|
                                            if name == "camc" && self.settings.get("special_csmc").map(|special_csmc| &**special_csmc).unwrap_or("no") == "yes" {
                                                DraftSetting {
                                                    options: vec![
                                                        DraftSettingChoice { name: "both", display: "chest size & texture match contents".into() },
                                                        DraftSettingChoice { name: "off", display: "vanilla chest appearances".into() },
                                                    ],
                                                    description: Cow::Borrowed("camc (Chest Appearance Matches Contents): both (default: size & texture) or off"),
                                                    name, display,
                                                }
                                            } else {
                                                DraftSetting {
                                                    options: iter::once(DraftSettingChoice { name: default, display: default_display.into() })
                                                        .chain(other.iter().map(|&(name, display)| DraftSettingChoice { name, display: display.into() }))
                                                        .collect(),
                                                    description: Cow::Borrowed(description),
                                                    name, display,
                                                }
                                            }
                                        )
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    match n {
                                        2 | 5 | 8 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        3 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push(". You will have another pick after this.")
                                            .build(),
                                        4 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick your second setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        9 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push(". You can also use ")
                                            .mention_command(command_ids.skip.unwrap(), "skip")
                                            .push(" if you want to leave the settings as they are.")
                                            .build(),
                                        0..=1 | 6..=7 | 10.. => unreachable!(),
                                    }
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => match n {
                                    2 => format!("{}, pick a setting using “!pick <setting> <value>”", team.choose(high_seed_name, low_seed_name)),
                                    3 => format!("{}, pick two settings.", team.choose(high_seed_name, low_seed_name)),
                                    4 => format!("And your second pick?"),
                                    5 | 8 => format!("{}, pick a setting.", team.choose(high_seed_name, low_seed_name)),
                                    9 => format!("{}, pick the final setting. You can also use “!skip” if you want to leave the settings as they are.", team.choose(high_seed_name, low_seed_name)),
                                    0..=1 | 6..=7 | 10.. => unreachable!(),
                                },
                            },
                        }
                    }
                    10.. => Step {
                        kind: StepKind::Done(mw::resolve_s4_draft_settings(&self.settings)),
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { .. } => format!("Settings draft completed. You will be playing with {}.", mw::display_s4_draft_picks(&self.settings)),
                            MessageContext::RaceTime { .. } => mw::display_s4_draft_picks(&self.settings),
                        },
                    },
                }
            } else {
                Step {
                    kind: StepKind::GoFirst,
                    message: match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            if game.is_some_and(|game| game > 1) {
                                builder.push(": as the losers of the previous race, please choose whether you want to go ");
                            } else {
                                builder.push(": you have the higher seed. Choose whether you want to go ");
                            }
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(" or ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            if let Some(game) = game {
                                builder.push(" in the settings draft for game ");
                                builder.push(game.to_string());
                                builder.push('.');
                            } else {
                                builder.push(" in the settings draft.");
                            }
                            if self.settings.get("special_csmc").map(|special_csmc| &**special_csmc).unwrap_or("no") == "yes" {
                                builder.push_line("");
                                builder.push("Please note that for accessibility reasons, the Chest Appearance Matches Contents setting will default to Both Size and Texture for this match. It can be locked to Both Size and Texture using a ban or pick, or changed to Off using a pick. Texture Only is not available in this match.");
                            }
                            builder.build()
                        }
                        MessageContext::RaceTime { high_seed_name, .. } => format!("{high_seed_name}, you have the higher seed. Choose whether you want to go !first or !second"),
                    },
                }
            }
        })
    }

    async fn next_step_multiworld_s5(&self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let kind = Kind::MultiworldS5;
        Ok({
            if let Some(went_first) = self.went_first {
                match self.pick_count(kind) {
                    prev_bans @ (0..=1 | 6..=7) => {
                        let team = match (prev_bans, went_first) {
                            (0, true) | (1, false) | (6, false) | (7, true) => Team::HighSeed,
                            (0, false) | (1, true) | (6, true) | (7, false) => Team::LowSeed,
                            (2..=5 | 8.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Ban {
                                available_settings: BanSettings(vec![
                                    ("All Settings", mw::S5_SETTINGS.iter().copied()
                                        .filter(|&mw::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|mw::Setting { name, display, default, default_display, description, .. }| BanSetting {
                                            description: Cow::Borrowed(description),
                                            name, display, default, default_display,
                                        })
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                        .push(": lock a setting to its default using ")
                                        .mention_command(command_ids.ban.unwrap(), "ban")
                                        .push(", or use ")
                                        .mention_command(command_ids.skip.unwrap(), "skip")
                                        .push(" if you don't want to ban anything.")
                                        .build()
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => format!(
                                    "{}, lock a setting to its default using “!ban <setting>”, or use “!skip” if you don't want to ban anything.{}",
                                    team.choose(high_seed_name, low_seed_name),
                                    if prev_bans == 0 { " Use “!settings” for a list of available settings." } else { "" },
                                ),
                            },
                        }
                    }
                    n @ (2..=5 | 8..=9) => {
                        let team = match (n, went_first) {
                            (2, true) | (3, false) | (4, false) | (5, true) | (8, false) | (9, true) => Team::HighSeed,
                            (2, false) | (3, true) | (4, true) | (5, false) | (8, true) | (9, false) => Team::LowSeed,
                            (0..=1 | 6..=7 | 10.., _) => unreachable!(),
                        };
                        Step {
                            kind: StepKind::Pick {
                                available_choices: DraftSettings(vec![
                                    ("All Settings", mw::S5_SETTINGS.iter().copied()
                                        .filter(|&mw::Setting { name, .. }| !self.settings.contains_key(name))
                                        .map(|mw::Setting { name, display, default, default_display, other, description }| DraftSetting {
                                            options: iter::once(DraftSettingChoice { name: default, display: default_display.into() })
                                                .chain(other.iter().map(|&(name, display)| DraftSettingChoice { name, display: display.into() }))
                                                .collect(),
                                            description: Cow::Borrowed(description),
                                            name, display,
                                        })
                                        .collect()),
                                ]),
                                skippable: true,
                                rsl: false,
                                team,
                            },
                            message: match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                    let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                    let high_seed = high_seed.remove(0);
                                    let low_seed = low_seed.remove(0);
                                    match n {
                                        2 | 5 | 8 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        3 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push(". You will have another pick after this.")
                                            .build(),
                                        4 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick your second setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push('.')
                                            .build(),
                                        9 => MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(": pick a setting using ")
                                            .mention_command(command_ids.pick.unwrap(), "pick")
                                            .push(". You can also use ")
                                            .mention_command(command_ids.skip.unwrap(), "skip")
                                            .push(" if you want to leave the settings as they are.")
                                            .build(),
                                        0..=1 | 6..=7 | 10.. => unreachable!(),
                                    }
                                }
                                MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => match n {
                                    2 => format!("{}, pick a setting using “!pick <setting> <value>”", team.choose(high_seed_name, low_seed_name)),
                                    3 => format!("{}, pick two settings.", team.choose(high_seed_name, low_seed_name)),
                                    4 => format!("And your second pick?"),
                                    5 | 8 => format!("{}, pick a setting.", team.choose(high_seed_name, low_seed_name)),
                                    9 => format!("{}, pick the final setting. You can also use “!skip” if you want to leave the settings as they are.", team.choose(high_seed_name, low_seed_name)),
                                    0..=1 | 6..=7 | 10.. => unreachable!(),
                                },
                            },
                        }
                    }
                    10.. => Step {
                        kind: StepKind::Done(mw::resolve_s5_draft_settings(&self.settings)),
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { .. } => format!("Settings draft completed. You will be playing with {}.", mw::display_s5_draft_picks(&self.settings)),
                            MessageContext::RaceTime { .. } => mw::display_s5_draft_picks(&self.settings),
                        },
                    },
                }
            } else {
                Step {
                    kind: StepKind::GoFirst,
                    message: match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            if game.is_some_and(|game| game > 1) {
                                builder.push(": as the losers of the previous race, please choose whether you want to go ");
                            } else {
                                builder.push(": you have the higher seed. Choose whether you want to go ");
                            }
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(" or ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            if let Some(game) = game {
                                builder.push(" in the settings draft for game ");
                                builder.push(game.to_string());
                                builder.push('.');
                            } else {
                                builder.push(" in the settings draft.");
                            }
                            builder.build()
                        }
                        MessageContext::RaceTime { high_seed_name, .. } => format!("{high_seed_name}, you have the higher seed. Choose whether you want to go !first or !second"),
                    },
                }
            }
        })
    }

    async fn next_step_tournoi_franco(&self, kind: Kind, _game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let all_settings = match kind {
            Kind::TournoiFrancoS3 => &fr::S3_SETTINGS[..],
            Kind::TournoiFrancoS4 => &fr::S4_SETTINGS[..],
            Kind::TournoiFrancoS5 => &fr::S5_SETTINGS[..],
            Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 | Kind::RslS7 | Kind::S7 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => unreachable!(),
        };
        Ok({
            if let Some(went_first) = self.went_first {
                let mut pick_count = self.pick_count(kind);
                let select_mixed_dungeons = !self.settings.contains_key("mixed-dungeons") && self.settings.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off") == "on" && self.settings.get("mixed-er").map(|mixed_er| &**mixed_er).unwrap_or("off") == "on";
                if select_mixed_dungeons {
                    // chosen by the same team that chose the previous setting
                    pick_count -= 1;
                }
                let team = match (kind, pick_count, went_first) {
                    (_, 0, true) | (_, 1, false) | (_, 2, true) | (_, 3, false) | (_, 4, false) | (_, 5, true) | (_, 6, true) | (_, 7, false) | (Kind::TournoiFrancoS3, 8, true) | (Kind::TournoiFrancoS3, 9, false) => Team::HighSeed,
                    (_, 0, false) | (_, 1, true) | (_, 2, false) | (_, 3, true) | (_, 4, true) | (_, 5, false) | (_, 6, false) | (_, 7, true) | (Kind::TournoiFrancoS3, 8, false) | (Kind::TournoiFrancoS3, 9, true) => Team::LowSeed,
                    (Kind::PickOnly { .. }, 8.., _) | (Kind::BanPick { .. }, 8.., _) | (Kind::BanOnly { .. }, 8.., _) => unreachable!(),
                    (Kind::TournoiFrancoS3, 10.., _) | (Kind::TournoiFrancoS4 | Kind::TournoiFrancoS5, 8.., _) => return Ok(Step {
                        kind: StepKind::Done(match kind {
                            Kind::TournoiFrancoS3 => fr::resolve_s3_draft_settings(&self.settings),
                            Kind::TournoiFrancoS4 => fr::resolve_s4_draft_settings(&self.settings),
                            Kind::TournoiFrancoS5 => fr::resolve_s5_draft_settings(&self.settings),
                            Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 | Kind::RslS7 | Kind::S7 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => unreachable!(),
                        }),
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { .. } => if let French = kind.language() {
                                format!("Fin du draft ! Voici un récapitulatif : {}.", fr::display_draft_picks(kind.language(), all_settings, &self.settings))
                            } else {
                                format!("Settings draft completed. You will be playing with {}.", fr::display_draft_picks(kind.language(), all_settings, &self.settings))
                            },
                            MessageContext::RaceTime { .. } => fr::display_draft_picks(kind.language(), all_settings, &self.settings),
                        },
                    }),
                    (Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 | Kind::RslS7 | Kind::S7, _, _) => unreachable!(),
                };
                if select_mixed_dungeons {
                    Step {
                        kind: StepKind::BooleanChoice { team },
                        message: match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                let high_seed = high_seed.remove(0);
                                let low_seed = low_seed.remove(0);
                                MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                    .push(if let French = kind.language() {
                                        " : Est-ce que les donjons seront mixés avec les intérieurs et les grottos ? Répondez en utilisant "
                                    } else {
                                        ": Should dungeon entrances be mixed with interiors and grottos? Use "
                                    })
                                    .mention_command(command_ids.yes.unwrap(), "yes")
                                    .push(if let French = kind.language() { " ou " } else { " or " })
                                    .mention_command(command_ids.no.unwrap(), "no")
                                    .push('.')
                                    .build()
                            }
                            MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => if let French = kind.language() {
                                format!(
                                    "{}, est-ce que les donjons seront mixés avec les intérieurs et les grottos ? Répondez en utilisant !yes ou !no",
                                    team.choose(high_seed_name, low_seed_name),
                                )
                            } else {
                                format!(
                                    "{}, should dungeon entrances be mixed with interiors and grottos? Use !yes or !no",
                                    team.choose(high_seed_name, low_seed_name),
                                )
                            },
                        },
                    }
                } else {
                    match self.pick_count(kind) {
                        prev_bans @ 0..=1 => {
                            let hard_settings_ok = self.settings.get("hard_settings_ok").map(|hard_settings_ok| &**hard_settings_ok).unwrap_or("no") == "ok";
                            let (hard_settings, classic_settings) = all_settings.iter()
                                .filter(|&&fr::Setting { name, .. }| !self.settings.contains_key(name) && match name {
                                    "keysy" => self.settings.get("keysanity").is_none_or(|keysanity| keysanity == "off"),
                                    "1major" if kind == Kind::TournoiFrancoS5 => self.settings.get("th").is_none_or(|th| th == "off") && self.settings.get("souls").is_none_or(|souls| souls == "off"),
                                    "souls" if kind == Kind::TournoiFrancoS5 => self.settings.get("1major").is_none_or(|one_major| one_major == "off"),
                                    "th" if kind == Kind::TournoiFrancoS5 => self.settings.get("1major").is_none_or(|one_major| one_major == "off"),
                                    "keysanity" => self.settings.get("keysy").is_none_or(|keysy| keysy == "off"),
                                    _ => true,
                                })
                                .filter_map(|fr::Setting { name, display, default, default_display, other, description }| {
                                    let (is_hard, is_empty) = if hard_settings_ok {
                                        (other.iter().all(|&(_, hard, _)| hard), other.is_empty())
                                    } else {
                                        (false, other.iter().all(|&(_, hard, _)| hard))
                                    };
                                    (!is_empty).then(|| (is_hard, BanSetting {
                                        display: if *display == "enemy souls" && !hard_settings_ok { "boss souls" } else { display },
                                        description: Cow::Borrowed(description),
                                        name, default, default_display,
                                    }))
                                })
                                .partition::<Vec<_>, _>(|&(is_hard, _)| is_hard);
                            let mut available_settings = vec![
                                (if let French = kind.language() { "Settings classiques" } else { "Classic Settings" }, classic_settings.into_iter().map(|(_, setting)| setting).collect()),
                            ];
                            if hard_settings_ok && !hard_settings.is_empty() {
                                available_settings.push((if let French = kind.language() { "Settings difficiles" } else { "Hard Settings" }, hard_settings.into_iter().map(|(_, setting)| setting).collect()));
                            }
                            Step {
                                kind: StepKind::Ban {
                                    available_settings: BanSettings(available_settings),
                                    skippable: false,
                                    rsl: false,
                                    team,
                                },
                                message: match msg_ctx {
                                    MessageContext::None => String::default(),
                                    MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                        let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                        let high_seed = high_seed.remove(0);
                                        let low_seed = low_seed.remove(0);
                                        MessageBuilder::default()
                                            .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                            .push(if let French = kind.language() {
                                                " : Veuillez ban un setting en utilisant "
                                            } else {
                                                ": lock a setting to its default using "
                                            })
                                            .mention_command(command_ids.ban.unwrap(), "ban")
                                            .push('.')
                                            .build()
                                    }
                                    MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => if let French = kind.language() {
                                        format!(
                                            "{}, veuillez ban un setting en utilisant “!ban <setting>”.{}",
                                            team.choose(high_seed_name, low_seed_name),
                                            if prev_bans == 0 { " Utilisez “!settings” pour la liste des settings." } else { "" },
                                        )
                                    } else {
                                        format!(
                                            "{}, lock a setting to its default using “!ban <setting>”.{}",
                                            team.choose(high_seed_name, low_seed_name),
                                            if prev_bans == 0 { " Use “!settings” for a list of available settings." } else { "" },
                                        )
                                    },
                                },
                            }
                        }
                        n @ 2..=9 => {
                            let round_count = match kind {
                                Kind::TournoiFrancoS3 => 10,
                                Kind::TournoiFrancoS4 => 8,
                                Kind::TournoiFrancoS5 => 8,
                                Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 | Kind::RslS7 | Kind::S7 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => unreachable!(),
                            };
                            let hard_settings_ok = self.settings.get("hard_settings_ok").map(|hard_settings_ok| &**hard_settings_ok).unwrap_or("no") == "ok";
                            let can_ban = match kind {
                                Kind::TournoiFrancoS3 | Kind::TournoiFrancoS4 => n < round_count - 2 || self.settings.get(team.choose("high_seed_has_picked", "low_seed_has_picked")).map(|has_picked| &**has_picked).unwrap_or("no") == "yes",
                                Kind::TournoiFrancoS5 => n == 4 || n == 5,
                                Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 | Kind::RslS7 | Kind::S7 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => unreachable!(),
                            };
                            let skippable = n == round_count - 1 && can_ban;
                            let (hard_settings, classic_settings) = all_settings.iter()
                                .filter(|&&fr::Setting { name, .. }| !self.settings.contains_key(name))
                                .filter_map(|&fr::Setting { name, display, default, default_display, other, description }| {
                                    let (is_hard, other) = if hard_settings_ok {
                                        (other.iter().all(|&(_, hard, _)| hard), other.to_owned())
                                    } else {
                                        (false, other.iter().filter(|(_, hard, _)| !hard).copied().collect_vec())
                                    };
                                    (!other.is_empty()).then(|| (is_hard, DraftSetting {
                                        display: if display == "enemy souls" && !hard_settings_ok { "boss souls" } else { display },
                                        options: can_ban.then(|| DraftSettingChoice { name: default, display: default_display.into() }).into_iter()
                                            .chain(other.into_iter().map(|(name, _, display)| DraftSettingChoice { name, display: display.into() }))
                                            .collect(),
                                        description: Cow::Borrowed(description),
                                        name,
                                    }))
                                })
                                .partition::<Vec<_>, _>(|&(is_hard, _)| is_hard);
                            let mut available_choices = vec![
                                (if let French = kind.language() { "Settings classiques" } else { "Classic Settings" }, classic_settings.into_iter().map(|(_, setting)| setting).collect()),
                            ];
                            if hard_settings_ok && !hard_settings.is_empty() {
                                available_choices.push((if let French = kind.language() { "Settings difficiles" } else { "Hard Settings" }, hard_settings.into_iter().map(|(_, setting)| setting).collect()));
                            }
                            Step {
                                kind: StepKind::Pick {
                                    available_choices: DraftSettings(available_choices),
                                    rsl: false,
                                    team, skippable,
                                },
                                message: match msg_ctx {
                                    MessageContext::None => String::default(),
                                    MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => {
                                        let (mut high_seed, mut low_seed) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                                        let high_seed = high_seed.remove(0);
                                        let low_seed = low_seed.remove(0);
                                        match (kind, n) {
                                            (_, 9) | (Kind::TournoiFrancoS4, 7) => if let French = kind.language() {
                                                let mut builder = MessageBuilder::default();
                                                builder.mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?;
                                                builder.push(" : Choisissez un setting avec ");
                                                builder.mention_command(command_ids.pick.unwrap(), "pick");
                                                builder.push('.');
                                                if skippable {
                                                    builder.push(" Vous pouvez également utiliser ");
                                                    builder.mention_command(command_ids.skip.unwrap(), "skip");
                                                    builder.push(" si vous voulez laisser les settings comme ils sont.");
                                                }
                                                builder.build()
                                            } else {
                                                let mut builder = MessageBuilder::default();
                                                builder.mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?;
                                                builder.push(": pick a setting using ");
                                                builder.mention_command(command_ids.pick.unwrap(), "pick");
                                                builder.push('.');
                                                if skippable {
                                                    builder.push(" You can also use ");
                                                    builder.mention_command(command_ids.skip.unwrap(), "skip");
                                                    builder.push(" if you want to leave the settings as they are.");
                                                }
                                                builder.build()
                                            },
                                            (_, 2 | 7 | 8) => if let French = kind.language() {
                                                MessageBuilder::default()
                                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                                    .push(" : Choisissez un setting en utilisant ")
                                                    .mention_command(command_ids.pick.unwrap(), "pick")
                                                    .push('.')
                                                    .build()
                                            } else {
                                                MessageBuilder::default()
                                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                                    .push(": pick a setting using ")
                                                    .mention_command(command_ids.pick.unwrap(), "pick")
                                                    .push('.')
                                                    .build()
                                            },
                                            (_, 3 | 5) => if let French = kind.language() {
                                                MessageBuilder::default()
                                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                                    .push(" : Choisissez un setting avec ")
                                                    .mention_command(command_ids.pick.unwrap(), "pick")
                                                    .push(". Vous aurez un autre pick après celui-ci.")
                                                    .build()
                                            } else {
                                                MessageBuilder::default()
                                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                                    .push(": pick a setting using ")
                                                    .mention_command(command_ids.pick.unwrap(), "pick")
                                                    .push(". You will have another pick after this.")
                                                    .build()
                                            },
                                            (_, 4 | 6) => if let French = kind.language() {
                                                MessageBuilder::default()
                                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                                    .push(" : Choisissez votre second setting avec ")
                                                    .mention_command(command_ids.pick.unwrap(), "pick")
                                                    .push('.')
                                                    .build()
                                            } else {
                                                MessageBuilder::default()
                                                    .mention_team(transaction, Some(*guild_id), team.choose(high_seed, low_seed)).await?
                                                    .push(": pick your second setting using ")
                                                    .mention_command(command_ids.pick.unwrap(), "pick")
                                                    .push('.')
                                                    .build()
                                            },
                                            (_, 0..=1 | 10..) => unreachable!(),
                                        }
                                    }
                                    MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => match (kind, n) {
                                        (Kind::TournoiFrancoS4, 7) | (_, 9) if skippable => if let French = kind.language() {
                                            format!("{}, choisissez le dernier setting. Vous pouvez également utiliser “!skip” si vous voulez laisser les settings comme ils sont.", team.choose(high_seed_name, low_seed_name))
                                        } else {
                                            format!("{},  pick the final setting. You can also use “!skip” if you want to leave the settings as they are.", team.choose(high_seed_name, low_seed_name))
                                        },
                                        (Kind::TournoiFrancoS4, 7) | (_, 9) => if let French = kind.language() {
                                            format!("{}, choisissez votre dernier setting.", team.choose(high_seed_name, low_seed_name))
                                        } else {
                                            format!("{}, pick the final setting.", team.choose(high_seed_name, low_seed_name))
                                        },
                                        (_, 2) => if let French = kind.language() {
                                            format!("{}, choisissez un setting avec “!pick <setting> <configuration>”. <configuration> signifie la valeur du setting. Par exemple pour tokensanity, la configuration peut être {{all, dungeon, overworld}}.", team.choose(high_seed_name, low_seed_name))
                                        } else {
                                            format!("{}, pick a setting using “!pick <setting> <value>”", team.choose(high_seed_name, low_seed_name))
                                        },
                                        (_, 3 | 5) => if let French = kind.language() {
                                            format!("{}, choisissez deux settings. Quel est votre premier ?", team.choose(high_seed_name, low_seed_name))
                                        } else {
                                            format!("{}, pick two settings.", team.choose(high_seed_name, low_seed_name))
                                        },
                                        (_, 4 | 6) => if let French = kind.language() {
                                            format!("Et votre second ?")
                                        } else {
                                            format!("And your second pick?")
                                        },
                                        (_, 7 | 8) => if let French = kind.language() {
                                            format!("{}, choisissez un setting.", team.choose(high_seed_name, low_seed_name))
                                        } else {
                                            format!("{}, pick a setting.", team.choose(high_seed_name, low_seed_name))
                                        },
                                        (_, 0..=1 | 10..) => unreachable!(),
                                    },
                                },
                            }
                        }
                        10.. => unreachable!(),
                    }
                }
            } else {
                Step {
                    kind: StepKind::GoFirst,
                    message: match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { transaction, guild_id, command_ids, teams, .. } => if let French = kind.language() {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            builder.push(" : Vous avez été sélectionné pour décider qui commencera le draft en premier. Si vous voulez commencer, veuillez entrer ");
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(". Autrement, entrez ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            builder.push(".");
                            if self.settings.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" {
                                builder.push(" Veuillez choisir combien de donjons Master Quest seront présents. Vous devez vous concerter pour choisir ce nombre. Utilisez “/first” ou “/second” suivi de “mq:{nombre}”");
                            }
                            builder.build()
                        } else {
                            let (mut high_seed, _) = teams.iter().partition::<Vec<_>, _>(|team| team.id == self.high_seed);
                            let high_seed = high_seed.remove(0);
                            let mut builder = MessageBuilder::default();
                            builder.mention_team(transaction, Some(*guild_id), high_seed).await?;
                            builder.push(": you have won the coin flip. Choose whether you want to go ");
                            builder.mention_command(command_ids.first.unwrap(), "first");
                            builder.push(" or ");
                            builder.mention_command(command_ids.second.unwrap(), "second");
                            builder.push(" in the settings draft.");
                            if self.settings.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" {
                                builder.push(" Please include the number of MQ dungeons.");
                            }
                            builder.build()
                        },
                        MessageContext::RaceTime { high_seed_name, .. } => if let French = kind.language() {
                            format!("{high_seed_name}, vous avez été sélectionné pour décider qui commencera le draft en premier. Si vous voulez commencer, veuillez entrer “!first”. Autrement, entrez “!second”.")
                        } else {
                            format!("{high_seed_name}, you have won the coin flip. Choose whether you want to go !first or !second in the settings draft.")
                        },
                    },
                }
            }
        })
    }

    async fn next_step_pick_only(&self, options: &'static [PresetOption], who_starts: Team, picks_per_player: u8, unique: bool, label: &'static str, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        let total_picks = picks_per_player as usize * 2;
        let last_game_key = format!("game{total_picks}_preset");
        if self.settings.contains_key(&*last_game_key) {
            // Draft complete — return Done for the current game
            let game_num = game.unwrap_or(1) as usize;
            let preset_val = self.settings.get(&*format!("game{game_num}_preset"))
                .expect("PickOnly draft complete but game preset missing");
            let preset_display = options.iter().find(|p| p.preset == preset_val.as_ref()).map(|p| p.display_name).unwrap_or(preset_val.as_ref());
            let mut settings = seed::Settings::new();
            settings.insert(format!("preset"), json!(preset_val.as_ref()));
            Ok(Step {
                kind: StepKind::Done(settings),
                message: match msg_ctx {
                    MessageContext::None => String::default(),
                    MessageContext::Discord { .. } => if game.unwrap_or(1) == 1 {
                        let mut parts = Vec::with_capacity(total_picks + 1);
                        parts.push("Preset draft completed.".to_owned());
                        for n in 1..=total_picks {
                            let v = self.settings.get(&*format!("game{n}_preset")).map(|s| s.as_ref()).unwrap_or("?");
                            let display = options.iter().find(|p| p.preset == v).map(|p| p.display_name).unwrap_or(v);
                            parts.push(format!("Game {n}: {display}"));
                        }
                        parts.join("\n")
                    } else {
                        String::default()
                    },
                    MessageContext::RaceTime { .. } => format!("Playing {} preset", preset_display),
                },
            })
        } else {
            // Determine how many picks have been made so far
            let picks_made = (1..=total_picks).filter(|&n| self.settings.contains_key(&*format!("game{n}_preset"))).count();
            // Whose turn: who_starts picks on even counts, the other on odd
            let team = if picks_made % 2 == 0 { who_starts } else { match who_starts { Team::HighSeed => Team::LowSeed, Team::LowSeed => Team::HighSeed } };
            let next_game = (picks_made + 1) as u8;
            // Build available presets (filter already-taken if unique)
            let available_presets: Vec<PresetOption> = if unique {
                let taken: Vec<&str> = (1..=picks_made)
                    .filter_map(|n| self.settings.get(&*format!("game{n}_preset")).map(|s| s.as_ref()))
                    .collect();
                options.iter().filter(|p| !taken.contains(&p.preset)).copied().collect()
            } else {
                options.to_vec()
            };
            let preset_list = available_presets.iter().map(|p| p.display_name).collect::<Vec<_>>().join(", ");
            Ok(Step {
                kind: StepKind::PickPreset { team, available_presets, game: next_game },
                message: match msg_ctx {
                    MessageContext::None => String::default(),
                    MessageContext::Discord { transaction, guild_id, teams, .. } => {
                        let (mut high, mut low): (Vec<_>, Vec<_>) = teams.iter().partition(|t| t.id == self.high_seed);
                        let active = team.choose(high.remove(0), low.remove(0));
                        let mut builder = MessageBuilder::default();
                        if picks_made > 0 {
                            let prev_preset = self.settings.get(&*format!("game{picks_made}_preset")).map(|s| s.as_ref()).unwrap_or("?");
                            let prev_display = options.iter().find(|p| p.preset == prev_preset).map(|p| p.display_name).unwrap_or(prev_preset);
                            builder.push(format!("**{prev_display}** was picked for Game {picks_made}.\n"));
                        }
                        builder.mention_team(transaction, Some(*guild_id), active).await?;
                        builder.push(format!(": Pick the {label} for Game {next_game}."));
                        builder.build()
                    }
                    MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => {
                        let name = team.choose(*high_seed_name, *low_seed_name);
                        format!("{name}, pick the {label} for Game {next_game}. Available: {preset_list}")
                    }
                },
            })
        }
    }

    async fn next_step_ban_pick(&self, options: &'static [PresetOption], order: &'static [DraftPhase], label: &'static str, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        if self.settings.contains_key("game3_preset") {
            // Draft complete (game3 auto-assigned in apply) — return Done for current game
            let game_num = game.unwrap_or(1) as usize;
            let preset_val = self.settings.get(&*format!("game{game_num}_preset"))
                .expect("BanPick draft complete but game preset missing");
            let preset_display = options.iter().find(|p| p.preset == preset_val.as_ref()).map(|p| p.display_name).unwrap_or(preset_val.as_ref());
            let mut settings = seed::Settings::new();
            settings.insert(format!("preset"), json!(preset_val.as_ref()));
            Ok(Step {
                kind: StepKind::Done(settings),
                message: match msg_ctx {
                    MessageContext::None => String::default(),
                    MessageContext::Discord { .. } => if game.unwrap_or(1) == 1 {
                        let total_picks = order.iter().filter(|p| matches!(p, DraftPhase::Pick(_))).count();
                        let mut parts = vec!["Preset draft completed.".to_owned()];
                        for n in 1..=total_picks { let v = self.settings.get(&*format!("game{n}_preset")).map(|s| s.as_ref()).unwrap_or("?"); let d = options.iter().find(|p| p.preset == v).map(|p| p.display_name).unwrap_or(v); parts.push(format!("Game {n}: {d}")); }
                        if let Some(v) = self.settings.get("game3_preset") { let d = options.iter().find(|p| p.preset == v.as_ref()).map(|p| p.display_name).unwrap_or(v.as_ref()); parts.push(format!("Game 3: {d}")); }
                        parts.join("\n")
                    } else {
                        String::default()
                    },
                    MessageContext::RaceTime { .. } => format!("Playing {} preset", preset_display),
                },
            })
        } else {
            // Walk through order to find the first unsatisfied step
            let mut ban_count = 0usize;
            let mut pick_count = 0usize;
            let mut prev_action: Option<String> = None;
            for phase in order {
                match phase {
                    DraftPhase::Ban(team) => {
                        ban_count += 1;
                        if !self.settings.contains_key(&*format!("ban{ban_count}")) {
                            // Current step: team bans
                            let banned: Vec<&str> = (1..ban_count).filter_map(|n| self.settings.get(&*format!("ban{n}")).map(|s| s.as_ref())).collect();
                            let picked: Vec<&str> = (1..=pick_count).filter_map(|n| self.settings.get(&*format!("game{n}_preset")).map(|s| s.as_ref())).collect();
                            let available: Vec<BanSetting> = options.iter()
                                .filter(|p| !banned.contains(&p.preset) && !picked.contains(&p.preset))
                                .map(|p| BanSetting { name: p.preset, display: p.display_name, default: p.preset, default_display: p.display_name, description: Cow::Borrowed(p.display_name) })
                                .collect();
                            let avail_display = available.iter().map(|s| s.display).collect::<Vec<_>>().join(", ");
                            return Ok(Step {
                                kind: StepKind::Ban { team: *team, available_settings: BanSettings(vec![("Presets", available)]), skippable: false, rsl: false },
                                message: match msg_ctx {
                                    MessageContext::None => String::default(),
                                    MessageContext::Discord { transaction, guild_id, teams, .. } => {
                                        let (mut high, mut low): (Vec<_>, Vec<_>) = teams.iter().partition(|t| t.id == self.high_seed);
                                        let active = team.choose(high.remove(0), low.remove(0));
                                        let mut builder = MessageBuilder::default();
                                        if let Some(prev) = &prev_action {
                                            builder.push(format!("{prev}\n"));
                                        }
                                        builder.mention_team(transaction, Some(*guild_id), active).await?;
                                        builder.push(format!(": Ban a {label}."));
                                        builder.build()
                                    }
                                    MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => {
                                        let name = team.choose(*high_seed_name, *low_seed_name);
                                        format!("{name}, ban a {label} using \"!ban <{label}>\". Available: {avail_display}")
                                    }
                                },
                            });
                        }
                        let val = self.settings.get(&*format!("ban{ban_count}")).map(|s| s.as_ref()).unwrap_or("?");
                        let display = options.iter().find(|p| p.preset == val).map(|p| p.display_name).unwrap_or(val);
                        prev_action = Some(format!("**{display}** was banned."));
                    }
                    DraftPhase::Pick(team) => {
                        pick_count += 1;
                        if !self.settings.contains_key(&*format!("pick{pick_count}")) {
                            // Current step: team picks for game pick_count
                            let banned: Vec<&str> = (1..=ban_count).filter_map(|n| self.settings.get(&*format!("ban{n}")).map(|s| s.as_ref())).collect();
                            let picked: Vec<&str> = (1..pick_count).filter_map(|n| self.settings.get(&*format!("game{n}_preset")).map(|s| s.as_ref())).collect();
                            let available_presets: Vec<PresetOption> = options.iter()
                                .filter(|p| !banned.contains(&p.preset) && !picked.contains(&p.preset))
                                .copied()
                                .collect();
                            let avail_display = available_presets.iter().map(|p| p.display_name).collect::<Vec<_>>().join(", ");
                            let current_pick = pick_count as u8;
                            return Ok(Step {
                                kind: StepKind::PickPreset { team: *team, available_presets, game: current_pick },
                                message: match msg_ctx {
                                    MessageContext::None => String::default(),
                                    MessageContext::Discord { transaction, guild_id, teams, .. } => {
                                        let (mut high, mut low): (Vec<_>, Vec<_>) = teams.iter().partition(|t| t.id == self.high_seed);
                                        let active = team.choose(high.remove(0), low.remove(0));
                                        let mut builder = MessageBuilder::default();
                                        if let Some(prev) = &prev_action {
                                            builder.push(format!("{prev}\n"));
                                        }
                                        builder.mention_team(transaction, Some(*guild_id), active).await?;
                                        builder.push(format!(": Pick the {label} for Game {current_pick}."));
                                        builder.build()
                                    }
                                    MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => {
                                        let name = team.choose(*high_seed_name, *low_seed_name);
                                        format!("{name}, pick the {label} for Game {current_pick}. Available: {avail_display}")
                                    }
                                },
                            });
                        }
                        let val = self.settings.get(&*format!("game{pick_count}_preset")).map(|s| s.as_ref()).unwrap_or("?");
                        let display = options.iter().find(|p| p.preset == val).map(|p| p.display_name).unwrap_or(val);
                        prev_action = Some(format!("**{display}** was picked for Game {pick_count}."));
                    }
                }
            }
            // Should not reach here (game3_preset check at top handles Done)
            unreachable!("BanPick next_step reached end of order without Done check triggering")
        }
    }

    async fn next_step_ban_only(&self, options: &'static [PresetOption], order: &'static [DraftPhase], label: &'static str, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        if self.settings.contains_key("game3_preset") {
            // Draft complete — presets were randomly assigned after final ban
            let game_num = game.unwrap_or(1) as usize;
            let preset_val = self.settings.get(&*format!("game{game_num}_preset"))
                .expect("BanOnly draft complete but game preset missing");
            let preset_display = options.iter().find(|p| p.preset == preset_val.as_ref()).map(|p| p.display_name).unwrap_or(preset_val.as_ref());
            let mut settings = seed::Settings::new();
            settings.insert(format!("preset"), json!(preset_val.as_ref()));
            Ok(Step {
                kind: StepKind::Done(settings),
                message: match msg_ctx {
                    MessageContext::None => String::default(),
                    MessageContext::Discord { .. } => if game.unwrap_or(1) == 1 {
                        let mut parts = vec!["Preset draft completed.".to_owned()];
                        for n in 1..=3 {
                            if let Some(v) = self.settings.get(&*format!("game{n}_preset")) {
                                let d = options.iter().find(|p| p.preset == v.as_ref()).map(|p| p.display_name).unwrap_or(v.as_ref());
                                parts.push(format!("Game {n}: {d}"));
                            }
                        }
                        parts.join("\n")
                    } else {
                        String::default()
                    },
                    MessageContext::RaceTime { .. } => format!("Playing {} preset", preset_display),
                },
            })
        } else {
            // Walk through ban order to find the first unsatisfied step
            let mut ban_count = 0usize;
            let mut prev_action: Option<String> = None;
            for phase in order {
                match phase {
                    DraftPhase::Ban(team) => {
                        ban_count += 1;
                        if !self.settings.contains_key(&*format!("ban{ban_count}")) {
                            let banned: Vec<&str> = (1..ban_count).filter_map(|n| self.settings.get(&*format!("ban{n}")).map(|s| s.as_ref())).collect();
                            let available: Vec<BanSetting> = options.iter()
                                .filter(|p| !banned.contains(&p.preset))
                                .map(|p| BanSetting { name: p.preset, display: p.display_name, default: p.preset, default_display: p.display_name, description: Cow::Borrowed(p.display_name) })
                                .collect();
                            let avail_display = available.iter().map(|s| s.display).collect::<Vec<_>>().join(", ");
                            return Ok(Step {
                                kind: StepKind::Ban { team: *team, available_settings: BanSettings(vec![("Presets", available)]), skippable: false, rsl: false },
                                message: match msg_ctx {
                                    MessageContext::None => String::default(),
                                    MessageContext::Discord { transaction, guild_id, teams, .. } => {
                                        let (mut high, mut low): (Vec<_>, Vec<_>) = teams.iter().partition(|t| t.id == self.high_seed);
                                        let active = team.choose(high.remove(0), low.remove(0));
                                        let mut builder = MessageBuilder::default();
                                        if let Some(prev) = &prev_action {
                                            builder.push(format!("{prev}\n"));
                                        }
                                        builder.mention_team(transaction, Some(*guild_id), active).await?;
                                        builder.push(format!(": Ban a {label}."));
                                        builder.build()
                                    }
                                    MessageContext::RaceTime { high_seed_name, low_seed_name, .. } => {
                                        let name = team.choose(*high_seed_name, *low_seed_name);
                                        format!("{name}, ban a {label} using \"!ban <{label}>\". Available: {avail_display}")
                                    }
                                },
                            });
                        }
                        let val = self.settings.get(&*format!("ban{ban_count}")).map(|s| s.as_ref()).unwrap_or("?");
                        let display = options.iter().find(|p| p.preset == val).map(|p| p.display_name).unwrap_or(val);
                        prev_action = Some(format!("**{display}** was banned."));
                    }
                    DraftPhase::Pick(_) => unreachable!("BanOnly order should not contain Pick phases"),
                }
            }
            unreachable!("BanOnly next_step reached end of order without Done check triggering")
        }
    }

    pub(crate) async fn next_step(&self, kind: Kind, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        match kind {
            Kind::S7 => self.next_step_s7(game, msg_ctx).await,
            Kind::RslS7 => self.next_step_rsl_s7(game, msg_ctx).await,
            Kind::MultiworldS3 => self.next_step_multiworld_s3(game, msg_ctx).await,
            Kind::MultiworldS4 => self.next_step_multiworld_s4(game, msg_ctx).await,
            Kind::MultiworldS5 => self.next_step_multiworld_s5(game, msg_ctx).await,
            Kind::TournoiFrancoS3 | Kind::TournoiFrancoS4 | Kind::TournoiFrancoS5 => {
                self.next_step_tournoi_franco(kind, game, msg_ctx).await
            }
            Kind::PickOnly { options, who_starts, picks_per_player, unique, label } => {
                self.next_step_pick_only(options, who_starts, picks_per_player, unique, label, game, msg_ctx).await
            }
            Kind::BanPick { options, order, label } => {
                self.next_step_ban_pick(options, order, label, game, msg_ctx).await
            }
            Kind::BanOnly { options, order, label } => {
                self.next_step_ban_only(options, order, label, game, msg_ctx).await
            }
        }
    }
    pub(crate) async fn active_team(&self, kind: Kind, game: Option<i16>) -> Result<Option<Team>, Error> {
        Ok(match self.next_step(kind, game, &mut MessageContext::None).await?.kind {
            StepKind::GoFirst => Some(Team::HighSeed),
            StepKind::Ban { team, .. } | StepKind::Pick { team, .. } | StepKind::BooleanChoice { team } | StepKind::PickPreset { team, .. } => Some(team),
            StepKind::Done(_) | StepKind::DoneRsl { .. } => None,
        })
    }

    /// Assumes that the caller has checked that the team is part of the race in the first place.
    pub(crate) async fn is_active_team(&self, kind: Kind, game: Option<i16>, team: Id<Teams>) -> Result<bool, Error> {
        Ok(match self.active_team(kind, game).await? {
            Some(Team::HighSeed) => team == self.high_seed,
            Some(Team::LowSeed) => team != self.high_seed,
            None => false,
        })
    }



    async fn apply_s7(&mut self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let kind = Kind::S7;
        Ok({
            let resolved_action = match action {
                Action::Ban { setting } => if let Some(setting) = s::S7_SETTINGS.into_iter().find(|&s::Setting { name, .. }| *name == setting) {
                    Action::Pick { setting: setting.name.to_owned(), value: format!("default") }
                } else {
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => {
                            let mut content = MessageBuilder::default();
                            content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                            for (i, setting) in s::S7_SETTINGS.into_iter().enumerate() {
                                if i > 0 {
                                    content.push(" or ");
                                }
                                content.push_mono(setting.name);
                            }
                            content.build()
                        }
                        MessageContext::RaceTime { reply_to, .. } => format!(
                            "Sorry {reply_to}, I don't recognize that setting. Use one of the following: {}",
                            s::S7_SETTINGS.into_iter().map(|setting| setting.name).format(" or "),
                        ),
                    }))
                },
                Action::BooleanChoice(value) if matches!(self.next_step_s7(game, &mut MessageContext::None).await?.kind, StepKind::GoFirst) => Action::GoFirst(value),
                _ => action,
            };
            match resolved_action {
                Action::GoFirst(first) => match self.next_step_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => {
                        self.went_first = Some(first);
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have" } else { " has" })
                                .push(" chosen to go ")
                                .push(if first { "first" } else { "second" })
                                .push(" in the settings draft.")
                                .build(),
                        })
                    }
                    StepKind::Ban { .. } | StepKind::Pick { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, first pick has already been chosen."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick has already been chosen."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Ban { .. } => unreachable!("normalized to Action::Pick above"),
                Action::Pick { setting, value } => match self.next_step_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”"),
                    }),
                    StepKind::Ban { available_settings, skippable, .. } => if let Some(setting) = available_settings.get(&setting) {
                        if value == setting.default {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(setting.default));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have locked in " } else { " has locked in " })
                                    .push(setting.default_display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                                    .push("Sorry, bans haven't been chosen yet, use ")
                                    .mention_command(command_ids.ban.unwrap(), "ban")
                                    .build(),
                                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, bans haven't been chosen yet. Use “!ban <setting>”"),
                            })
                        }
                    } else {
                        let exists = s::S7_SETTINGS.into_iter().any(|s::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_settings.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to ban anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_settings.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to ban anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::Pick { available_choices, skippable, .. } => if let Some(setting) = available_choices.get(&setting) {
                        if let Some(option) = setting.options.iter().find(|option| option.name == value) {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(option.name));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have picked " } else { " has picked " })
                                    .push(&*option.display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { .. } => {
                                    let mut content = MessageBuilder::default();
                                    content.push("Sorry, that's not a possible value for this setting. Use one of the following: ");
                                    for (i, value) in setting.options.into_iter().enumerate() {
                                        if i > 0 {
                                            content.push(" or ");
                                        }
                                        content.push_mono(value.name);
                                    }
                                    content.build()
                                }
                                MessageContext::RaceTime { reply_to, .. } => format!(
                                    "Sorry {reply_to}, that's not a possible value for this setting. Use one of the following: {}",
                                    setting.options.into_iter().map(|value| value.name).format(" or "),
                                ),
                            })
                        }
                    } else {
                        let exists = s::S7_SETTINGS.into_iter().any(|s::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_choices.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to pick anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_choices.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to pick anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Skip => match self.next_step_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”")
                    }),
                    StepKind::Ban { skippable: true, .. } | StepKind::Pick { skippable: true, .. } => {
                        let skip_kind = match self.pick_count(kind) {
                            0 | 1 => "ban",
                            _ => "final pick",
                        };
                        self.skipped_bans += 1;
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have skipped " } else { " has skipped " })
                                .push(team.possessive_determiner(transaction).await?)
                                .push(' ')
                                .push(skip_kind)
                                .push('.')
                                .build(),
                        })
                    }
                    StepKind::Ban { skippable: false, .. } | StepKind::Pick { skippable: false, .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this part of the draft can't be skipped."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this part of the draft can't be skipped."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::BooleanChoice(_) => match self.next_step_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => unreachable!("normalized to Action::GoFirst above"),
                    StepKind::BooleanChoice { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                    _ => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, the current step is not a yes/no question."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the current step is not a yes/no question."),
                    }),
                },
            }
        })
    }

    async fn apply_multiworld_s3(&mut self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let kind = Kind::MultiworldS3;
        Ok({
            let resolved_action = match action {
                Action::Ban { setting } => if let Some(setting) = mw::S3_SETTINGS.iter().copied().find(|&mw::Setting { name, .. }| *name == setting) {
                    Action::Pick { setting: setting.name.to_owned(), value: setting.default.to_owned() }
                } else {
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => {
                            let mut content = MessageBuilder::default();
                            content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                            for (i, setting) in mw::S3_SETTINGS.iter().copied().enumerate() {
                                if i > 0 {
                                    content.push(" or ");
                                }
                                content.push_mono(setting.name);
                            }
                            content.build()
                        }
                        MessageContext::RaceTime { reply_to, .. } => format!(
                            "Sorry {reply_to}, I don't recognize that setting. Use one of the following: {}",
                            mw::S3_SETTINGS.iter().copied().map(|setting| setting.name).format(" or "),
                        ),
                    }))
                },
                Action::BooleanChoice(value) if matches!(self.next_step_multiworld_s3(game, &mut MessageContext::None).await?.kind, StepKind::GoFirst) => Action::GoFirst(value),
                _ => action,
            };
            match resolved_action {
                Action::GoFirst(first) => match self.next_step_multiworld_s3(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => {
                        self.went_first = Some(first);
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have" } else { " has" })
                                .push(" chosen to go ")
                                .push(if first { "first" } else { "second" })
                                .push(" in the settings draft.")
                                .build(),
                        })
                    }
                    StepKind::Ban { .. } | StepKind::Pick { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, first pick has already been chosen."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick has already been chosen."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Ban { .. } => unreachable!("normalized to Action::Pick above"),
                Action::Pick { setting, value } => match self.next_step_multiworld_s3(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”"),
                    }),
                    StepKind::Ban { available_settings, skippable, .. } => if let Some(setting) = available_settings.get(&setting) {
                        if value == setting.default {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(setting.default));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have locked in " } else { " has locked in " })
                                    .push(setting.default_display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                                    .push("Sorry, bans haven't been chosen yet, use ")
                                    .mention_command(command_ids.ban.unwrap(), "ban")
                                    .build(),
                                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, bans haven't been chosen yet. Use “!ban <setting>”"),
                            })
                        }
                    } else {
                        let exists = mw::S3_SETTINGS.iter().copied().any(|mw::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_settings.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to ban anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_settings.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to ban anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::Pick { available_choices, skippable, .. } => if let Some(setting) = available_choices.get(&setting) {
                        if let Some(option) = setting.options.iter().find(|option| option.name == value) {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(option.name));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have picked " } else { " has picked " })
                                    .push(&*option.display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { .. } => {
                                    let mut content = MessageBuilder::default();
                                    content.push("Sorry, that's not a possible value for this setting. Use one of the following: ");
                                    for (i, value) in setting.options.into_iter().enumerate() {
                                        if i > 0 {
                                            content.push(" or ");
                                        }
                                        content.push_mono(value.name);
                                    }
                                    content.build()
                                }
                                MessageContext::RaceTime { reply_to, .. } => format!(
                                    "Sorry {reply_to}, that's not a possible value for this setting. Use one of the following: {}",
                                    setting.options.into_iter().map(|value| value.name).format(" or "),
                                ),
                            })
                        }
                    } else {
                        let exists = mw::S3_SETTINGS.iter().copied().any(|mw::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_choices.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to pick anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_choices.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to pick anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Skip => match self.next_step_multiworld_s3(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”")
                    }),
                    StepKind::Ban { skippable: true, .. } | StepKind::Pick { skippable: true, .. } => {
                        let skip_kind = match self.pick_count(kind) {
                            0 | 1 => "ban",
                            _ => "final pick",
                        };
                        self.skipped_bans += 1;
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have skipped " } else { " has skipped " })
                                .push(team.possessive_determiner(transaction).await?)
                                .push(' ')
                                .push(skip_kind)
                                .push('.')
                                .build(),
                        })
                    }
                    StepKind::Ban { skippable: false, .. } | StepKind::Pick { skippable: false, .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this part of the draft can't be skipped."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this part of the draft can't be skipped."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::BooleanChoice(_) => match self.next_step_multiworld_s3(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => unreachable!("normalized to Action::GoFirst above"),
                    StepKind::BooleanChoice { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                    _ => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, the current step is not a yes/no question."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the current step is not a yes/no question."),
                    }),
                },
            }
        })
    }

    async fn apply_multiworld_s4(&mut self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let kind = Kind::MultiworldS4;
        Ok({
            let resolved_action = match action {
                Action::Ban { setting } => if let Some(setting) = mw::S4_SETTINGS.iter().copied().find(|&mw::Setting { name, .. }| *name == setting) {
                    Action::Pick {
                        setting: setting.name.to_owned(),
                        value: if setting.name == "camc" && self.settings.get("special_csmc").map(|special_csmc| &**special_csmc).unwrap_or("no") == "yes" {
                            format!("both")
                        } else {
                            setting.default.to_owned()
                        },
                    }
                } else {
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => {
                            let mut content = MessageBuilder::default();
                            content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                            for (i, setting) in mw::S4_SETTINGS.iter().copied().enumerate() {
                                if i > 0 {
                                    content.push(" or ");
                                }
                                content.push_mono(setting.name);
                            }
                            content.build()
                        }
                        MessageContext::RaceTime { reply_to, .. } => format!(
                            "Sorry {reply_to}, I don't recognize that setting. Use one of the following: {}",
                            mw::S4_SETTINGS.iter().copied().map(|setting| setting.name).format(" or "),
                        ),
                    }))
                },
                Action::BooleanChoice(value) if matches!(self.next_step_multiworld_s4(game, &mut MessageContext::None).await?.kind, StepKind::GoFirst) => Action::GoFirst(value),
                _ => action,
            };
            match resolved_action {
                Action::GoFirst(first) => match self.next_step_multiworld_s4(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => {
                        self.went_first = Some(first);
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have" } else { " has" })
                                .push(" chosen to go ")
                                .push(if first { "first" } else { "second" })
                                .push(" in the settings draft.")
                                .build(),
                        })
                    }
                    StepKind::Ban { .. } | StepKind::Pick { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, first pick has already been chosen."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick has already been chosen."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Ban { .. } => unreachable!("normalized to Action::Pick above"),
                Action::Pick { setting, value } => match self.next_step_multiworld_s4(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”"),
                    }),
                    StepKind::Ban { available_settings, skippable, .. } => if let Some(setting) = available_settings.get(&setting) {
                        if value == setting.default {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(setting.default));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have locked in " } else { " has locked in " })
                                    .push(setting.default_display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                                    .push("Sorry, bans haven't been chosen yet, use ")
                                    .mention_command(command_ids.ban.unwrap(), "ban")
                                    .build(),
                                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, bans haven't been chosen yet. Use “!ban <setting>”"),
                            })
                        }
                    } else {
                        let exists = mw::S4_SETTINGS.iter().copied().any(|mw::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_settings.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to ban anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_settings.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to ban anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::Pick { available_choices, skippable, .. } => if let Some(setting) = available_choices.get(&setting) {
                        if let Some(option) = setting.options.iter().find(|option| option.name == value) {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(option.name));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have picked " } else { " has picked " })
                                    .push(&*option.display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { .. } => {
                                    let mut content = MessageBuilder::default();
                                    content.push("Sorry, that's not a possible value for this setting. Use one of the following: ");
                                    for (i, value) in setting.options.into_iter().enumerate() {
                                        if i > 0 {
                                            content.push(" or ");
                                        }
                                        content.push_mono(value.name);
                                    }
                                    content.build()
                                }
                                MessageContext::RaceTime { reply_to, .. } => format!(
                                    "Sorry {reply_to}, that's not a possible value for this setting. Use one of the following: {}",
                                    setting.options.into_iter().map(|value| value.name).format(" or "),
                                ),
                            })
                        }
                    } else {
                        let exists = mw::S4_SETTINGS.iter().copied().any(|mw::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_choices.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to pick anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_choices.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to pick anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Skip => match self.next_step_multiworld_s4(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”")
                    }),
                    StepKind::Ban { skippable: true, .. } | StepKind::Pick { skippable: true, .. } => {
                        let skip_kind = match self.pick_count(kind) {
                            0 | 1 | 6 | 7 => "ban",
                            9 => "final pick",
                            _ => "pick",
                        };
                        self.skipped_bans += 1;
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have skipped " } else { " has skipped " })
                                .push(team.possessive_determiner(transaction).await?)
                                .push(' ')
                                .push(skip_kind)
                                .push('.')
                                .build(),
                        })
                    }
                    StepKind::Ban { skippable: false, .. } | StepKind::Pick { skippable: false, .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this part of the draft can't be skipped."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this part of the draft can't be skipped."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::BooleanChoice(_) => match self.next_step_multiworld_s4(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => unreachable!("normalized to Action::GoFirst above"),
                    StepKind::BooleanChoice { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                    _ => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, the current step is not a yes/no question."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the current step is not a yes/no question."),
                    }),
                },
            }
        })
    }

    async fn apply_multiworld_s5(&mut self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let kind = Kind::MultiworldS5;
        Ok({
            let resolved_action = match action {
                Action::Ban { setting } => if let Some(setting) = mw::S5_SETTINGS.iter().copied().find(|&mw::Setting { name, .. }| *name == setting) {
                    Action::Pick {
                        setting: setting.name.to_owned(),
                        value: setting.default.to_owned(),
                    }
                } else {
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => {
                            let mut content = MessageBuilder::default();
                            content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                            for (i, setting) in mw::S5_SETTINGS.iter().copied().enumerate() {
                                if i > 0 {
                                    content.push(" or ");
                                }
                                content.push_mono(setting.name);
                            }
                            content.build()
                        }
                        MessageContext::RaceTime { reply_to, .. } => format!(
                            "Sorry {reply_to}, I don't recognize that setting. Use one of the following: {}",
                            mw::S5_SETTINGS.iter().copied().map(|setting| setting.name).format(" or "),
                        ),
                    }))
                },
                Action::BooleanChoice(value) if matches!(self.next_step_multiworld_s5(game, &mut MessageContext::None).await?.kind, StepKind::GoFirst) => Action::GoFirst(value),
                _ => action,
            };
            match resolved_action {
                Action::GoFirst(first) => match self.next_step_multiworld_s5(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => {
                        self.went_first = Some(first);
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have" } else { " has" })
                                .push(" chosen to go ")
                                .push(if first { "first" } else { "second" })
                                .push(" in the settings draft.")
                                .build(),
                        })
                    }
                    StepKind::Ban { .. } | StepKind::Pick { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, first pick has already been chosen."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick has already been chosen."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Ban { .. } => unreachable!("normalized to Action::Pick above"),
                Action::Pick { setting, value } => match self.next_step_multiworld_s5(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”"),
                    }),
                    StepKind::Ban { available_settings, skippable, .. } => if let Some(setting) = available_settings.get(&setting) {
                        if value == setting.default {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(setting.default));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have locked in " } else { " has locked in " })
                                    .push(setting.default_display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                                    .push("Sorry, bans haven't been chosen yet, use ")
                                    .mention_command(command_ids.ban.unwrap(), "ban")
                                    .build(),
                                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, bans haven't been chosen yet. Use “!ban <setting>”"),
                            })
                        }
                    } else {
                        let exists = mw::S5_SETTINGS.iter().copied().any(|mw::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_settings.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to ban anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_settings.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to ban anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::Pick { available_choices, skippable, .. } => if let Some(setting) = available_choices.get(&setting) {
                        if let Some(option) = setting.options.iter().find(|option| option.name == value) {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(option.name));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have picked " } else { " has picked " })
                                    .push(&*option.display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { .. } => {
                                    let mut content = MessageBuilder::default();
                                    content.push("Sorry, that's not a possible value for this setting. Use one of the following: ");
                                    for (i, value) in setting.options.into_iter().enumerate() {
                                        if i > 0 {
                                            content.push(" or ");
                                        }
                                        content.push_mono(value.name);
                                    }
                                    content.build()
                                }
                                MessageContext::RaceTime { reply_to, .. } => format!(
                                    "Sorry {reply_to}, that's not a possible value for this setting. Use one of the following: {}",
                                    setting.options.into_iter().map(|value| value.name).format(" or "),
                                ),
                            })
                        }
                    } else {
                        let exists = mw::S5_SETTINGS.iter().copied().any(|mw::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_choices.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to pick anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_choices.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to pick anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::Skip => match self.next_step_multiworld_s5(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”")
                    }),
                    StepKind::Ban { skippable: true, .. } | StepKind::Pick { skippable: true, .. } => {
                        let skip_kind = match self.pick_count(kind) {
                            0 | 1 | 6 | 7 => "ban",
                            9 => "final pick",
                            _ => "pick",
                        };
                        self.skipped_bans += 1;
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have skipped " } else { " has skipped " })
                                .push(team.possessive_determiner(transaction).await?)
                                .push(' ')
                                .push(skip_kind)
                                .push('.')
                                .build(),
                        })
                    }
                    StepKind::Ban { skippable: false, .. } | StepKind::Pick { skippable: false, .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this part of the draft can't be skipped."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this part of the draft can't be skipped."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                },
                Action::BooleanChoice(_) => match self.next_step_multiworld_s5(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => unreachable!("normalized to Action::GoFirst above"),
                    StepKind::BooleanChoice { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this settings draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this settings draft is already completed."),
                    }),
                    _ => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, the current step is not a yes/no question."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the current step is not a yes/no question."),
                    }),
                },
            }
        })
    }

    async fn apply_rsl_s7(&mut self, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let _kind = Kind::RslS7;
        Ok({
            let resolved_action = match action {
                Action::Ban { setting } => Action::Pick {
                    setting,
                    value: format!("blocked"),
                },
                Action::BooleanChoice(value) if matches!(self.next_step_rsl_s7(game, &mut MessageContext::None).await?.kind, StepKind::GoFirst) => Action::GoFirst(value),
                _ => action,
            };
            match resolved_action {
                Action::GoFirst(first) => match self.next_step_rsl_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => {
                        self.went_first = Some(first);
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => {
                                let mut content = MessageBuilder::default();
                                content.mention_team(transaction, Some(*guild_id), team).await?;
                                content.push(if team.name_is_plural() { " have" } else { " has" });
                                content.push(" chosen to go ");
                                content.push(if first { "first" } else { "second" });
                                content.push(" in the weights draft");
                                if self.settings.get("lite_ok").map(|lite_ok| &**lite_ok).unwrap_or("no") == "ok" {
                                    content.push(" and selected ");
                                    content.push(if self.settings.get("preset").map(|preset| &**preset).unwrap_or("league") == "lite" { "RSL-Lite weights" } else { "RSL weights" });
                                }
                                content
                                    .push('.')
                                    .build()
                            }
                        })
                    }
                    StepKind::Ban { .. } | StepKind::Pick { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, first pick has already been chosen."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick has already been chosen."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::Done(_) | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::DoneRsl { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this weights draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this weights draft is already completed."),
                    }),
                },
                Action::Ban { .. } => unreachable!("normalized to Action::Pick above"),
                Action::Pick { setting, value } => match self.next_step_rsl_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”"),
                    }),
                    StepKind::Ban { available_settings, skippable, .. } => if let Some(setting) = available_settings.get(&setting) {
                        if value == setting.default {
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(setting.default));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have blocked " } else { " has blocked " })
                                    .push(setting.default_display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                                    .push("Sorry, the current step is a block, not a ban, use ")
                                    .mention_command(command_ids.ban.unwrap(), "block")
                                    .build(),
                                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the current step is a block, not a ban. Use “!block <setting>”"),
                            })
                        }
                    } else {
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                content.push("Sorry, that setting doesn't exist or can no longer be blocked. Use one of the following: ");
                                for (i, setting) in available_settings.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to block anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, that setting doesn't exist or can no longer be blocked. Use one of the following: {}{}",
                                available_settings.all().map(|setting| setting.name).format(" or "),
                                if skippable { ". Use “!skip” if you don't want to block anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::Pick { team, available_choices, skippable, .. } => if let Some(setting) = available_choices.get(&setting) {
                        if let Some(option) = setting.options.iter().find(|option| option.name == value) {
                            match self.settings.entry(Cow::Borrowed(setting.name)) {
                                hash_map::Entry::Occupied(mut entry) => {
                                    let entry = entry.get_mut();
                                    *entry = Cow::Owned(format!("{entry},{value}"));
                                }
                                hash_map::Entry::Vacant(entry) => { entry.insert(Cow::Borrowed(option.name)); }
                            }
                            match self.settings.entry(Cow::Owned(format!("{}_banned_by", setting.name))) {
                                hash_map::Entry::Occupied(mut entry) => {
                                    let entry = entry.get_mut();
                                    *entry = Cow::Owned(format!("{entry},{team}"));
                                }
                                hash_map::Entry::Vacant(entry) => { entry.insert(Cow::Owned(team.to_string())); }
                            }
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                    .mention_team(transaction, Some(*guild_id), team).await?
                                    .push(if team.name_is_plural() { " have banned " } else { " has banned " })
                                    .push(&*option.display)
                                    .push('.')
                                    .build(),
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { .. } => {
                                    let mut content = MessageBuilder::default();
                                    content.push("Sorry, that's not a possible value for this setting. Use one of the following: ");
                                    for (i, value) in setting.options.into_iter().enumerate() {
                                        if i > 0 {
                                            content.push(" or ");
                                        }
                                        content.push_mono(value.name);
                                    }
                                    content.build()
                                }
                                MessageContext::RaceTime { reply_to, .. } => format!(
                                    "Sorry {reply_to}, that's not a possible value for this setting. Use one of the following: {}",
                                    setting.options.into_iter().map(|value| value.name).format(" or "),
                                ),
                            })
                        }
                    } else {
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                content.push("Sorry, that setting doesn't exist or can no longer be banned. Use one of the following: ");
                                for (i, setting) in available_choices.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to pick anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, that setting doesn't exist or can no longer be banned. Use one of the following: {}{}",
                                available_choices.all().map(|setting| setting.name).format(" or "),
                                if skippable { ". Use “!skip” if you don't want to pick anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::BooleanChoice { .. } | StepKind::Done(_) | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::DoneRsl { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this weights draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this weights draft is already completed."),
                    }),
                },
                Action::Skip => match self.next_step_rsl_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”")
                    }),
                    kind @ (StepKind::Ban { skippable: true, .. } | StepKind::Pick { skippable: true, .. }) => {
                        let skip_kind = match kind {
                            StepKind::Ban { .. } => "block",
                            StepKind::Pick { .. } => "ban",
                            _ => unreachable!(),
                        };
                        self.skipped_bans += 1;
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have skipped " } else { " has skipped " })
                                .push(team.possessive_determiner(transaction).await?)
                                .push(' ')
                                .push(skip_kind)
                                .push('.')
                                .build(),
                        })
                    }
                    StepKind::Ban { skippable: false, .. } | StepKind::Pick { skippable: false, .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this part of the draft can't be skipped."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this part of the draft can't be skipped."),
                    }),
                    StepKind::BooleanChoice { .. } | StepKind::Done(_) | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::DoneRsl { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this weights draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this weights draft is already completed."),
                    }),
                },
                Action::BooleanChoice(_) => match self.next_step_rsl_s7(game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => unreachable!("normalized to Action::GoFirst above"),
                    StepKind::BooleanChoice { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this weights draft is already completed."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this weights draft is already completed."),
                    }),
                    _ => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, the current step is not a yes/no question."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the current step is not a yes/no question."),
                    }),
                },
            }
        })
    }

    async fn apply_tournoi_franco(&mut self, kind: Kind, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        Ok({
            let all_settings = match kind {
                Kind::TournoiFrancoS3 => &fr::S3_SETTINGS[..],
                Kind::TournoiFrancoS4 => &fr::S4_SETTINGS[..],
                Kind::TournoiFrancoS5 => &fr::S5_SETTINGS[..],
                Kind::MultiworldS3 | Kind::MultiworldS4 | Kind::MultiworldS5 | Kind::RslS7 | Kind::S7 | Kind::PickOnly { .. } | Kind::BanPick { .. } | Kind::BanOnly { .. } => unreachable!(),
            };
            let resolved_action = match action {
                Action::Ban { setting } => if let Some(setting) = all_settings.iter().find(|&&fr::Setting { name, .. }| *name == setting) {
                    Action::Pick { setting: setting.name.to_owned(), value: setting.default.to_owned() }
                } else {
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => {
                            let mut content = MessageBuilder::default();
                            content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                            for (i, setting) in all_settings.iter().enumerate() {
                                if i > 0 {
                                    content.push(" or ");
                                }
                                content.push_mono(setting.name);
                            }
                            content.build()
                        }
                        MessageContext::RaceTime { reply_to, .. } => format!(
                            "Sorry {reply_to}, I don't recognize that setting. Use one of the following: {}",
                            all_settings.iter().map(|setting| setting.name).format(" or "),
                        ),
                    }))
                },
                _ => action,
            };
            match resolved_action {
                Action::GoFirst(first) => match self.next_step_tournoi_franco(kind, game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => {
                        self.went_first = Some(first);
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => if let French = kind.language() {
                                let mut content = MessageBuilder::default();
                                content.mention_team(transaction, Some(*guild_id), team).await?;
                                content.push(" a choisi de partir ");
                                content.push(if first { "premier" } else { "second" });
                                content.push(" pour le draft");
                                if self.settings.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" {
                                    let mq_dungeons_count = self.settings.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0");
                                    content.push(" et a choisi ");
                                    content.push(mq_dungeons_count);
                                    content.push(" donjon");
                                    if mq_dungeons_count != "1" {
                                        content.push('s');
                                    }
                                    content.push(" MQ");
                                }
                                content
                                    .push('.')
                                    .build()
                            } else {
                                let mut content = MessageBuilder::default();
                                content.mention_team(transaction, Some(*guild_id), team).await?;
                                content.push(" has chosen to go ");
                                content.push(if first { "first" } else { "second" });
                                content.push(" in the settings draft");
                                if self.settings.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" {
                                    let mq_dungeons_count = self.settings.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0");
                                    content.push(" and has selected ");
                                    content.push(mq_dungeons_count);
                                    content.push(" MQ dungeon");
                                    if mq_dungeons_count != "1" {
                                        content.push('s');
                                    }
                                }
                                content
                                    .push('.')
                                    .build()
                            },
                        })
                    }
                    StepKind::Ban { .. } | StepKind::Pick { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => if let French = kind.language() {
                            format!("Désolé, le premier pick a déjà été sélectionné.")
                        } else {
                            format!("Sorry, first pick has already been chosen.")
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, le premier pick a déjà été sélectionné.")
                        } else {
                            format!("Sorry {reply_to}, first pick has already been chosen.")
                        },
                    }),
                    StepKind::BooleanChoice { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => if let French = kind.language() {
                            MessageBuilder::default()
                                .push("Désolé, avant que le draft ne puisse continuer, vous devez d'abord choisir si les donjons seront mixés ou non avec le reste. Utilisez ")
                                .mention_command(command_ids.yes.unwrap(), "yes")
                                .push(" ou ")
                                .mention_command(command_ids.no.unwrap(), "no")
                                .push('.')
                                .build()
                        } else {
                            MessageBuilder::default()
                                .push("Sorry, before the settings draft can continue, you first have to choose whether dungeons entrances should be mixed. Use ")
                                .mention_command(command_ids.yes.unwrap(), "yes")
                                .push(" or ")
                                .mention_command(command_ids.no.unwrap(), "no")
                                .push('.')
                                .build()
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, avant que le draft ne puisse continuer, vous devez d'abord choisir si les donjons seront mixés ou non avec le reste. Utilisez !yes ou !no")
                        } else {
                            format!("Sorry {reply_to}, before the settings draft can continue, you first have to choose whether dungeons entrances should be mixed. Use !yes or !no")
                        },
                    }),
                    StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => if let French = kind.language() {
                            format!("Désolé, ce draft est terminé.")
                        } else {
                            format!("Sorry, this settings draft is already completed.")
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, ce draft est terminé.")
                        } else {
                            format!("Sorry {reply_to}, this settings draft is already completed.")
                        },
                    }),
                },
                Action::Ban { .. } => unreachable!("normalized to Action::Pick above"),
                Action::Pick { setting, value } => match self.next_step_tournoi_franco(kind, game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”"),
                    }),
                    StepKind::Ban { available_settings, skippable, .. } => if let Some(setting) = available_settings.get(&setting) {
                        if value == setting.default {
                            let hard_settings_ok = self.settings.get("hard_settings_ok").map(|hard_settings_ok| &**hard_settings_ok).unwrap_or("no") == "ok";
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(setting.default));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => if let French = kind.language() {
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team).await?
                                        .push(" a banni ")
                                        .push(match setting.name { "camc" => "no CAMC", "souls" if !hard_settings_ok => "boss souls", _ => setting.display })
                                        .push('.')
                                        .build()
                                } else {
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team).await?
                                        .push(" has banned ")
                                        .push(match setting.name { "camc" => "no CAMC", "souls" if !hard_settings_ok => "boss souls", _ => setting.display })
                                        .push('.')
                                        .build()
                                },
                            })
                        } else {
                            //TODO check if this setting is disabled because it is hard
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                                    .push("Sorry, bans haven't been chosen yet, use ")
                                    .mention_command(command_ids.ban.unwrap(), "ban")
                                    .build(),
                                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, bans haven't been chosen yet. Use “!ban <setting>”"),
                            })
                        }
                    } else {
                        let exists = all_settings.iter().any(|&fr::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    //TODO check if this setting is disabled because it is hard
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_settings.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to ban anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_settings.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to ban anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::Pick { available_choices, skippable, .. } => if let Some(setting) = available_choices.get(&setting) {
                        if let Some(option) = setting.options.iter().find(|option| option.name == value) {
                            let hard_settings_ok = self.settings.get("hard_settings_ok").map(|hard_settings_ok| &**hard_settings_ok).unwrap_or("no") == "ok";
                            let is_default = value == all_settings.iter().find(|&&fr::Setting { name, .. }| setting.name == name).unwrap().default;
                            if !is_default {
                                self.settings.insert(Cow::Borrowed(self.active_team(kind, game).await?.unwrap().choose("high_seed_has_picked", "low_seed_has_picked")), Cow::Borrowed("yes"));
                            }
                            self.settings.insert(Cow::Borrowed(setting.name), Cow::Borrowed(option.name));
                            Ok(match msg_ctx {
                                MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                                MessageContext::Discord { transaction, guild_id, team, .. } => if let French = kind.language() {
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team).await?
                                        .push(if is_default { " a banni " } else { " a choisi " })
                                        .push(if is_default { match setting.name { "camc" => "no CAMC", "souls" if !hard_settings_ok => "boss souls", _ => setting.display } } else { &option.display })
                                        .push('.')
                                        .build()
                                } else {
                                    MessageBuilder::default()
                                        .mention_team(transaction, Some(*guild_id), team).await?
                                        .push(if is_default { " has banned " } else { " has picked " })
                                        .push(if is_default { match setting.name { "camc" => "no CAMC", "souls" if !hard_settings_ok => "boss souls", _ => setting.display } } else { &option.display })
                                        .push('.')
                                        .build()
                                },
                            })
                        } else {
                            Err(match msg_ctx {
                                MessageContext::None => String::default(),
                                MessageContext::Discord { .. } => {
                                    let mut content = MessageBuilder::default();
                                    content.push("Sorry, that's not a possible value for this setting. Use one of the following: ");
                                    for (i, value) in setting.options.into_iter().enumerate() {
                                        if i > 0 {
                                            content.push(" or ");
                                        }
                                        content.push_mono(value.name);
                                    }
                                    content.build()
                                }
                                MessageContext::RaceTime { reply_to, .. } => format!(
                                    "Sorry {reply_to}, that's not a possible value for this setting. Use one of the following: {}",
                                    setting.options.into_iter().map(|value| value.name).format(" or "),
                                ),
                            })
                        }
                    } else {
                        let exists = all_settings.iter().any(|&fr::Setting { name, .. }| setting == name);
                        Err(match msg_ctx {
                            MessageContext::None => String::default(),
                            MessageContext::Discord { command_ids, .. } => {
                                let mut content = MessageBuilder::default();
                                if exists {
                                    content.push("Sorry, that setting is already locked in. Use one of the following: ");
                                } else {
                                    content.push("Sorry, I don't recognize that setting. Use one of the following: ");
                                }
                                for (i, setting) in available_choices.all().enumerate() {
                                    if i > 0 {
                                        content.push(" or ");
                                    }
                                    content.push_mono(setting.name);
                                }
                                if exists && skippable {
                                    content.push(". Use ");
                                    content.mention_command(command_ids.skip.unwrap(), "skip");
                                    content.push(" if you don't want to pick anything.");
                                }
                                content.build()
                            }
                            MessageContext::RaceTime { reply_to, .. } => format!(
                                "Sorry {reply_to}, {}. Use one of the following: {}{}",
                                if exists { "that setting is already locked in" } else { "I don't recognize that setting" },
                                available_choices.all().map(|setting| setting.name).format(" or "),
                                if exists && skippable { ". Use “!skip” if you don't want to pick anything." } else { "" },
                            ),
                        })
                    },
                    StepKind::BooleanChoice { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => if let French = kind.language() {
                            MessageBuilder::default()
                                .push("Désolé, avant que le draft ne puisse continuer, vous devez d'abord choisir si les donjons seront mixés ou non avec le reste. Utilisez ")
                                .mention_command(command_ids.yes.unwrap(), "yes")
                                .push(" ou ")
                                .mention_command(command_ids.no.unwrap(), "no")
                                .push('.')
                                .build()
                        } else {
                            MessageBuilder::default()
                                .push("Sorry, before the settings draft can continue, you first have to choose whether dungeons entrances should be mixed. Use ")
                                .mention_command(command_ids.yes.unwrap(), "yes")
                                .push(" or ")
                                .mention_command(command_ids.no.unwrap(), "no")
                                .push('.')
                                .build()
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, avant que le draft ne puisse continuer, vous devez d'abord choisir si les donjons seront mixés ou non avec le reste. Utilisez !yes ou !no")
                        } else {
                            format!("Sorry {reply_to}, before the settings draft can continue, you first have to choose whether dungeons entrances should be mixed. Use !yes or !no")
                        },
                    }),
                    StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => if let French = kind.language() {
                            format!("Désolé, ce draft est terminé.")
                        } else {
                            format!("Sorry, this settings draft is already completed.")
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, ce draft est terminé.")
                        } else {
                            format!("Sorry {reply_to}, this settings draft is already completed.")
                        },
                    }),
                },
                Action::Skip => match self.next_step_tournoi_franco(kind, game, &mut MessageContext::None).await?.kind {
                    StepKind::GoFirst => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Sorry, first pick hasn't been chosen yet, use ")
                            .mention_command(command_ids.first.unwrap(), "first")
                            .push(" or ")
                            .mention_command(command_ids.second.unwrap(), "second")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, first pick hasn't been chosen yet, use “!first” or “!second”")
                    }),
                    StepKind::Ban { skippable: true, .. } | StepKind::Pick { skippable: true, .. } => {
                        let skip_kind = match self.pick_count(kind) {
                            0 | 1 => "ban",
                            _ => "final pick",
                        };
                        self.skipped_bans += 1;
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => MessageBuilder::default()
                                .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                .push(if team.name_is_plural() { " have skipped " } else { " has skipped " })
                                .push(team.possessive_determiner(transaction).await?)
                                .push(' ')
                                .push(skip_kind)
                                .push('.')
                                .build(),
                        })
                    }
                    StepKind::Ban { skippable: false, .. } | StepKind::Pick { skippable: false, .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, this part of the draft can't be skipped."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, this part of the draft can't be skipped."),
                    }),
                    StepKind::BooleanChoice { .. } => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => if let French = kind.language() {
                            MessageBuilder::default()
                                .push("Désolé, avant que le draft ne puisse continuer, vous devez d'abord choisir si les donjons seront mixés ou non avec le reste. Utilisez ")
                                .mention_command(command_ids.yes.unwrap(), "yes")
                                .push(" ou ")
                                .mention_command(command_ids.no.unwrap(), "no")
                                .push('.')
                                .build()
                        } else {
                            MessageBuilder::default()
                                .push("Sorry, before the settings draft can continue, you first have to choose whether dungeons entrances should be mixed. Use ")
                                .mention_command(command_ids.yes.unwrap(), "yes")
                                .push(" or ")
                                .mention_command(command_ids.no.unwrap(), "no")
                                .push('.')
                                .build()
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, avant que le draft ne puisse continuer, vous devez d'abord choisir si les donjons seront mixés ou non avec le reste. Utilisez !yes ou !no")
                        } else {
                            format!("Sorry {reply_to}, before the settings draft can continue, you first have to choose whether dungeons entrances should be mixed. Use !yes or !no")
                        },
                    }),
                    StepKind::DoneRsl { .. } | StepKind::PickPreset { .. } => unreachable!(),
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => if let French = kind.language() {
                            format!("Désolé, ce draft est terminé.")
                        } else {
                            format!("Sorry, this settings draft is already completed.")
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, ce draft est terminé.")
                        } else {
                            format!("Sorry {reply_to}, this settings draft is already completed.")
                        },
                    }),
                },
                Action::BooleanChoice(value) => match self.next_step_tournoi_franco(kind, game, &mut MessageContext::None).await?.kind {
                    StepKind::BooleanChoice { .. } => {
                        self.settings.insert(Cow::Borrowed("mixed-dungeons"), Cow::Borrowed(if value { "mixed" } else { "separate" }));
                        Ok(match msg_ctx {
                            MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                            MessageContext::Discord { transaction, guild_id, team, .. } => if let French = kind.language() {
                                MessageBuilder::default()
                                    .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                    .push(if value {
                                        " a choisi les trois ER mixés."
                                    } else {
                                        " a choisi de n'avoir que grottos et interior mixés."
                                    })
                                    .build()
                            } else {
                                MessageBuilder::default()
                                    .mention_team(&mut *transaction, Some(*guild_id), team).await?
                                    .push(if value {
                                        " has selected mixed dungeon entrances."
                                    } else {
                                        " has selected separate dungeon entrances."
                                    })
                                    .build()
                            },
                        })
                    }
                    StepKind::Done(_) => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => if let French = kind.language() {
                            format!("Désolé, ce draft est terminé.")
                        } else {
                            format!("Sorry, this settings draft is already completed.")
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, ce draft est terminé.")
                        } else {
                            format!("Sorry {reply_to}, this settings draft is already completed.")
                        },
                    }),
                    _ => Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => if let French = kind.language() {
                            format!("Désolé, vous n'avez pas à répondre oui ou non.")
                        } else {
                            format!("Sorry, the current step is not a yes/no question.")
                        },
                        MessageContext::RaceTime { reply_to, .. } => if let French = kind.language() {
                            format!("Désolé {reply_to}, vous n'avez pas à répondre oui ou non.")
                        } else {
                            format!("Sorry {reply_to}, the current step is not a yes/no question.")
                        },
                    }),
                },
            }
        })
    }

    async fn apply_pick_only(&mut self, options: &'static [PresetOption], who_starts: Team, picks_per_player: u8, unique: bool, label: &'static str, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let step = self.next_step_pick_only(options, who_starts, picks_per_player, unique, label, game, &mut MessageContext::None).await?;
        Ok(match step.kind {
            StepKind::Done(_) => Err(match msg_ctx {
                MessageContext::None => String::default(),
                MessageContext::Discord { .. } => format!("Sorry, the preset draft is already completed."),
                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the preset draft is already completed."),
            }),
            StepKind::PickPreset { team: _team, available_presets, game: next_game } => {
                let preset_name = match action {
                    Action::Pick { setting, .. } => setting,
                    Action::Ban { setting } => setting,
                    _ => return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Please use /pick to choose a preset."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, please use \"!pick <preset>\" to pick a preset."),
                    })),
                };
                let preset_opt = available_presets.iter().find(|p| p.preset == preset_name);
                let Some(preset_opt) = preset_opt else {
                    let avail = available_presets.iter().map(|p| p.display_name).collect::<Vec<_>>().join(", ");
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, that preset is not available. Use one of: {avail}"),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, that preset is not available. Use one of: {avail}"),
                    }));
                };
                let key = format!("game{next_game}_preset");
                self.settings.insert(Cow::Owned(key), Cow::Borrowed(preset_opt.preset));
                Ok(match msg_ctx {
                    MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                    MessageContext::Discord { transaction, guild_id, team: team_data, .. } => MessageBuilder::default()
                        .mention_team(transaction, Some(*guild_id), team_data).await?
                        .push(if team_data.name_is_plural() { " have picked " } else { " has picked " })
                        .push(preset_opt.display_name)
                        .push(format!(" for Game {next_game}."))
                        .build(),
                })
            }
            StepKind::GoFirst | StepKind::Ban { .. } | StepKind::Pick { .. } | StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } => {
                unreachable!("PickOnly draft should only produce PickPreset or Done steps")
            }
        })
    }

    async fn apply_ban_pick(&mut self, options: &'static [PresetOption], order: &'static [DraftPhase], label: &'static str, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let step = self.next_step_ban_pick(options, order, label, game, &mut MessageContext::None).await?;
        Ok(match step.kind {
            StepKind::Done(_) => Err(match msg_ctx {
                MessageContext::None => String::default(),
                MessageContext::Discord { .. } => format!("Sorry, the preset draft is already completed."),
                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the preset draft is already completed."),
            }),
            StepKind::Ban { team: _team, available_settings, .. } => {
                let preset_name = match action {
                    Action::Ban { setting } => setting,
                    Action::Pick { setting, .. } => setting,
                    _ => return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Please use ")
                            .mention_command(command_ids.ban.unwrap(), "ban")
                            .push(" to ban a preset.")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, please use \"!ban <preset>\" to ban a preset."),
                    })),
                };
                if available_settings.get(&preset_name).is_none() {
                    let avail = available_settings.all().map(|s| s.display.to_owned()).collect::<Vec<_>>().join(", ");
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, that preset is not available to ban. Available: {avail}"),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, that preset is not available to ban. Available: {avail}"),
                    }));
                }
                // Store as ban{N}
                let ban_count = (1..).find(|n| !self.settings.contains_key(&*format!("ban{n}"))).unwrap();
                let display = options.iter().find(|p| p.preset == preset_name)
                    .map(|p| p.display_name.to_owned())
                    .unwrap_or_else(|| preset_name.clone());
                self.settings.insert(Cow::Owned(format!("ban{ban_count}")), Cow::Owned(preset_name));
                Ok(match msg_ctx {
                    MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                    MessageContext::Discord { transaction, guild_id, team: team_data, .. } => MessageBuilder::default()
                        .mention_team(transaction, Some(*guild_id), team_data).await?
                        .push(if team_data.name_is_plural() { " have banned " } else { " has banned " })
                        .push(display)
                        .push('.')
                        .build(),
                })
            }
            StepKind::PickPreset { team: _team, available_presets, game: current_pick } => {
                let preset_name = match action {
                    Action::Pick { setting, .. } => setting,
                    Action::Ban { setting } => setting,
                    _ => return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Please use /pick to choose a preset."),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, please use \"!pick <preset>\" to pick a preset."),
                    })),
                };
                let preset_opt = available_presets.iter().find(|p| p.preset == preset_name);
                let Some(preset_opt) = preset_opt else {
                    let avail = available_presets.iter().map(|p| p.display_name).collect::<Vec<_>>().join(", ");
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, that preset is not available. Use one of: {avail}"),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, that preset is not available. Use one of: {avail}"),
                    }));
                };
                let pick_num = (1..).find(|n| !self.settings.contains_key(&*format!("pick{n}"))).unwrap();
                self.settings.insert(Cow::Owned(format!("pick{pick_num}")), Cow::Borrowed(preset_opt.preset));
                self.settings.insert(Cow::Owned(format!("game{current_pick}_preset")), Cow::Borrowed(preset_opt.preset));

                // Count pick phases in order
                let total_picks = order.iter().filter(|p| matches!(p, DraftPhase::Pick(_))).count();
                let response = Ok(match msg_ctx {
                    MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                    MessageContext::Discord { transaction, guild_id, team: team_data, .. } => MessageBuilder::default()
                        .mention_team(transaction, Some(*guild_id), team_data).await?
                        .push(if team_data.name_is_plural() { " have picked " } else { " has picked " })
                        .push(preset_opt.display_name)
                        .push(format!(" for Game {current_pick}."))
                        .build(),
                });

                // If this is the last pick, auto-assign remaining preset to game3
                if pick_num == total_picks {
                    let ban_count = order.iter().filter(|p| matches!(p, DraftPhase::Ban(_))).count();
                    let banned: Vec<&str> = (1..=ban_count).filter_map(|n| self.settings.get(&*format!("ban{n}")).map(|s| s.as_ref())).collect();
                    let picked: Vec<&str> = (1..=total_picks).filter_map(|n| self.settings.get(&*format!("game{n}_preset")).map(|s| s.as_ref())).collect();
                    let remaining = options.iter().find(|p| !banned.contains(&p.preset) && !picked.contains(&p.preset))
                        .expect("no remaining preset for game3");
                    self.settings.insert(Cow::Borrowed("game3_preset"), Cow::Borrowed(remaining.preset));
                }
                response
            }
            StepKind::GoFirst | StepKind::Pick { .. } | StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } => {
                unreachable!("BanPick draft should only produce Ban, PickPreset, or Done steps")
            }
        })
    }

    async fn apply_ban_only(&mut self, options: &'static [PresetOption], order: &'static [DraftPhase], label: &'static str, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        let step = self.next_step_ban_only(options, order, label, game, &mut MessageContext::None).await?;
        Ok(match step.kind {
            StepKind::Done(_) => Err(match msg_ctx {
                MessageContext::None => String::default(),
                MessageContext::Discord { .. } => format!("Sorry, the preset draft is already completed."),
                MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, the preset draft is already completed."),
            }),
            StepKind::Ban { team: _team, available_settings, .. } => {
                let preset_name = match action {
                    Action::Ban { setting } => setting,
                    Action::Pick { setting, .. } => setting,
                    _ => return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { command_ids, .. } => MessageBuilder::default()
                            .push("Please use ")
                            .mention_command(command_ids.ban.unwrap(), "ban")
                            .push(" to ban a preset.")
                            .build(),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, please use \"!ban <preset>\" to ban a preset."),
                    })),
                };
                if available_settings.get(&preset_name).is_none() {
                    let avail = available_settings.all().map(|s| s.display.to_owned()).collect::<Vec<_>>().join(", ");
                    return Ok(Err(match msg_ctx {
                        MessageContext::None => String::default(),
                        MessageContext::Discord { .. } => format!("Sorry, that preset is not available to ban. Available: {avail}"),
                        MessageContext::RaceTime { reply_to, .. } => format!("Sorry {reply_to}, that preset is not available to ban. Available: {avail}"),
                    }));
                }
                let ban_count = (1..).find(|n| !self.settings.contains_key(&*format!("ban{n}"))).unwrap();
                let display = options.iter().find(|p| p.preset == preset_name)
                    .map(|p| p.display_name.to_owned())
                    .unwrap_or_else(|| preset_name.clone());
                self.settings.insert(Cow::Owned(format!("ban{ban_count}")), Cow::Owned(preset_name));

                // If all bans are done, randomly assign remaining presets to game slots
                let total_bans = order.iter().filter(|p| matches!(p, DraftPhase::Ban(_))).count();
                if ban_count == total_bans {
                    let banned: Vec<&str> = (1..=total_bans).filter_map(|n| self.settings.get(&*format!("ban{n}")).map(|s| s.as_ref())).collect();
                    let mut remaining: Vec<PresetOption> = options.iter()
                        .filter(|p| !banned.contains(&p.preset))
                        .copied()
                        .collect();
                    remaining.shuffle(&mut rng());
                    for (i, preset) in remaining.iter().enumerate() {
                        self.settings.insert(Cow::Owned(format!("game{}_preset", i + 1)), Cow::Borrowed(preset.preset));
                    }
                }

                Ok(match msg_ctx {
                    MessageContext::None | MessageContext::RaceTime { .. } => String::default(),
                    MessageContext::Discord { transaction, guild_id, team: team_data, .. } => MessageBuilder::default()
                        .mention_team(transaction, Some(*guild_id), team_data).await?
                        .push(if team_data.name_is_plural() { " have banned " } else { " has banned " })
                        .push(display)
                        .push('.')
                        .build(),
                })
            }
            StepKind::GoFirst | StepKind::Pick { .. } | StepKind::PickPreset { .. } | StepKind::BooleanChoice { .. } | StepKind::DoneRsl { .. } => {
                unreachable!("BanOnly draft should only produce Ban or Done steps")
            }
        })
    }

    pub(crate) async fn apply(&mut self, kind: Kind, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        match kind {
            Kind::S7 => self.apply_s7(game, msg_ctx, action).await,
            Kind::RslS7 => self.apply_rsl_s7(game, msg_ctx, action).await,
            Kind::MultiworldS3 => self.apply_multiworld_s3(game, msg_ctx, action).await,
            Kind::MultiworldS4 => self.apply_multiworld_s4(game, msg_ctx, action).await,
            Kind::MultiworldS5 => self.apply_multiworld_s5(game, msg_ctx, action).await,
            Kind::TournoiFrancoS3 | Kind::TournoiFrancoS4 | Kind::TournoiFrancoS5 => {
                self.apply_tournoi_franco(kind, game, msg_ctx, action).await
            }
            Kind::PickOnly { options, who_starts, picks_per_player, unique, label } => {
                self.apply_pick_only(options, who_starts, picks_per_player, unique, label, game, msg_ctx, action).await
            }
            Kind::BanPick { options, order, label } => {
                self.apply_ban_pick(options, order, label, game, msg_ctx, action).await
            }
            Kind::BanOnly { options, order, label } => {
                self.apply_ban_only(options, order, label, game, msg_ctx, action).await
            }
        }
    }
    pub(crate) async fn complete_randomly(mut self, kind: Kind) -> Result<Picks, Error> {
        Ok(loop {
            let action = match self.next_step(kind, None, &mut MessageContext::None).await?.kind {
                StepKind::GoFirst => Action::GoFirst(rng().random()),
                StepKind::Ban { available_settings, skippable, .. } => {
                    let mut settings = available_settings.all().map(Some).collect_vec();
                    if skippable {
                        settings.push(None);
                    }
                    if let Some(setting) = settings.into_iter().choose(&mut rng()).expect("no available settings") {
                        Action::Ban { setting: setting.name.to_owned() }
                    } else {
                        Action::Skip
                    }
                }
                StepKind::Pick { available_choices, skippable, .. } => {
                    let mut settings = available_choices.all().map(Some).collect_vec();
                    if skippable {
                        settings.push(None);
                    }
                    if let Some(setting) = settings.into_iter().choose(&mut rng()).expect("no available settings") {
                        Action::Pick { setting: setting.name.to_owned(), value: setting.options.choose(&mut rng()).expect("no available values").name.to_owned() }
                    } else {
                        Action::Skip
                    }
                }
                StepKind::PickPreset { available_presets, .. } => {
                    let preset = available_presets.into_iter().choose(&mut rng()).expect("no available presets for PickPreset step");
                    Action::Pick { setting: preset.preset.to_owned(), value: preset.preset.to_owned() }
                }
                StepKind::BooleanChoice { .. } => Action::BooleanChoice(rng().random()),
                StepKind::Done(_) | StepKind::DoneRsl { .. } => break self.settings,
            };
            self.apply(kind, None, &mut MessageContext::None, action).await?.expect("random draft made illegal action");
        })
    }
}
