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
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Sql(_) => false,
            Self::Wheel(e) => e.is_network_error(),
        }
    }
}

pub(crate) type Picks = HashMap<Cow<'static, str>, Cow<'static, str>>;

#[derive(Clone, Copy)]
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
pub(crate) enum Kind {
    // when defining a new variant, make sure to add it to event::Data::draft_kind and racetime_bot::Goal::draft_kind
    S7,
}

impl Kind {
    fn language(&self) -> Language {
        match self {
            Self::S7 => English,
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
            Kind::S7 => [
                min_by_key(team1, team2, |team| team.qualifier_rank).id,
                max_by_key(team1, team2, |team| team.qualifier_rank).id,
            ],
        };
        Ok(Self::for_next_game(transaction, kind, high_seed, low_seed).await?)
    }

    pub(crate) async fn for_next_game(transaction: &mut Transaction<'_, Postgres>, kind: Kind, loser: Id<Teams>, winner: Id<Teams>) -> sqlx::Result<Self> {
        Ok(Self {
            high_seed: loser,
            went_first: None,
            skipped_bans: 0,
            settings: match kind {
                Kind::S7 => HashMap::default(),
            },
        })
    }

    fn pick_count(&self, kind: Kind) -> u8 {
        match kind {
            Kind::S7 => self.skipped_bans + u8::try_from(self.settings.len()).unwrap(),
        }
    }

    pub(crate) async fn next_step(&self, kind: Kind, game: Option<i16>, msg_ctx: &mut MessageContext<'_>) -> Result<Step, Error> {
        Ok(match kind {
            Kind::S7 => {
                // Simplified S7 draft logic - removed series modules
                return Ok(Step {
                    kind: StepKind::Done(seed::Settings::default()),
                    message: "Draft completed.".to_string(),
                });
            }
        })
    }

    pub(crate) async fn active_team(&self, kind: Kind, game: Option<i16>) -> Result<Option<Team>, Error> {
        Ok(match self.next_step(kind, game, &mut MessageContext::None).await?.kind {
            StepKind::GoFirst => Some(Team::HighSeed),
            StepKind::Ban { team, .. } | StepKind::Pick { team, .. } | StepKind::BooleanChoice { team } => Some(team),
            StepKind::Done(_) => None,
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

    pub(crate) async fn apply(&mut self, kind: Kind, game: Option<i16>, msg_ctx: &mut MessageContext<'_>, action: Action) -> Result<Result<String, String>, Error> {
        Ok(match kind {
            Kind::S7 => {
                // Simplified S7 draft logic - removed series modules
                return Ok(Ok("Draft completed.".to_string()));
            }
        })
    }
}
