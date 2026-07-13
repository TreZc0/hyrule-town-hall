use {
    std::collections::HashMap,
    itertools::Itertools as _,
    serde::{Deserialize, Serialize},
    sqlx::PgPool,
    crate::cal::Race,
};
#[cfg(unix)] use async_proto::Protocol;

/// A (value, label) pair for a practice seed form dropdown or checkbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) struct PracticeOption {
    pub(crate) value: String,
    pub(crate) label: String,
}

/// Fully owned, JSON-configurable OWR event configuration stored in `events.seed_config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) struct OwrEventConfig {
    pub(crate) base_settings: serde_json::Value,
    #[serde(default)]
    pub(crate) base_placements: serde_json::Value,
    #[serde(default)]
    pub(crate) start_inventory: Vec<String>,
    /// Per-choice config object. Each key maps to an entry with optional fields:
    /// - `label`: human-readable label for the practice seed form and display.
    /// - `value_labels`: optional display labels for `never`, `random`, and `always`.
    ///   Entries without seed patches are displayed as player rules instead of seed settings.
    /// - `settings`, `placements`, `start_inventory`: patch applied when the choice is enabled.
    ///   Legacy flat patches (bare key→value object, no section keys) are also accepted.
    /// - `supercedes`: list of choice keys whose patches are suppressed when this choice is enabled.
    /// - `hidden_for_async`: if `true`, this choice is omitted from the scheduling thread display
    ///   for async races (e.g. a rule that only makes sense for a live, streamed race).
    #[serde(default)]
    pub(crate) choices: serde_json::Value,
}

/// Which seed generator an event uses, stored in `events.seed_gen_type`.
#[derive(Debug, Clone)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) enum SeedGenType {
    AlttprDoorRando {
        source: AlttprDrSource,
        /// Modes to display in the practice seed mode dropdown (from seed_config).
        practice_modes: Vec<PracticeOption>,
        /// Optional extra choices shown as checkboxes on the practice seed form (from seed_config).
        practice_choices: Vec<PracticeOption>,
    },
    AlttprAvianart {
        /// Presets to display in the practice seed preset dropdown (from seed_config).
        practice_presets: Vec<PracticeOption>,
    },
    /// OWR (Open World Randomizer) — player choices read from `teams.custom_choices`.
    Owr {
        /// Full event config from `events.seed_config`.
        config: OwrEventConfig,
    },
    OoTR,
    TWWR {
        /// Default permalink for the event (used for generic preroll; races may override via draft).
        #[allow(dead_code)]
        permalink: String,
    },
    OotrTriforceBlitz,
    OotrRsl,
    Mmr,
}

/// Source/method used to produce ALTTPR Door Rando settings.
#[derive(Debug, Clone)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) enum AlttprDrSource {
    /// Fetch a preset YAML from the boothisman.de API.
    Boothisman,
    /// Read settings from `teams.custom_choices`; both teams must agree on each option.
    /// The `config` field drives YAML construction and display (base settings + choice patches).
    MutualChoices { config: OwrEventConfig },
    /// Download a mystery weights YAML from a URL and run Mystery.py.
    MysteryPool {
        #[allow(dead_code)]
        weights_url: String,
    },
}

impl SeedGenType {
    /// Parse from DB columns `seed_gen_type VARCHAR(20)` and `seed_config JSONB`.
    ///
    /// Returns `None` if `seed_gen_type` is NULL or contains an unrecognised value.
    pub(crate) fn from_db(
        seed_gen_type: Option<&str>,
        seed_config: Option<&serde_json::Value>,
    ) -> Option<Self> {
        match seed_gen_type? {
            "alttpr_dr" => {
                let source_str = seed_config
                    .and_then(|c| c.get("source"))
                    .and_then(|v| v.as_str());
                let source = match source_str {
                    Some("boothisman") | None => AlttprDrSource::Boothisman,
                    Some("mutual_choices") => {
                        let config = serde_json::from_value(seed_config.cloned().unwrap_or_default())
                            .unwrap_or_else(|_| {
                                eprintln!("alttpr_dr/mutual_choices: missing or invalid seed_config");
                                OwrEventConfig {
                                    base_settings: serde_json::json!({}),
                                    base_placements: serde_json::Value::Null,
                                    start_inventory: vec![],
                                    choices: serde_json::Value::Null,
                                }
                            });
                        AlttprDrSource::MutualChoices { config }
                    }
                    Some("mystery_pool") => {
                        let weights_url = seed_config
                            .and_then(|c| c.get("mystery_weights_url"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_owned();
                        AlttprDrSource::MysteryPool { weights_url }
                    }
                    Some(other) => {
                        eprintln!("unknown alttpr_dr source in seed_config: {other}");
                        AlttprDrSource::Boothisman
                    }
                };
                let practice_modes = seed_config
                    .and_then(|c| c.get("practice_modes"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let practice_choices = seed_config
                    .and_then(|c| c.get("practice_choices"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                Some(Self::AlttprDoorRando { source, practice_modes, practice_choices })
            }
            "alttpr_avianart" => {
                let practice_presets = seed_config
                    .and_then(|c| c.get("practice_presets"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                Some(Self::AlttprAvianart { practice_presets })
            }
            "owr" => {
                let config = seed_config.and_then(|c| serde_json::from_value(c.clone()).ok());
                if let Some(config) = config {
                    Some(Self::Owr { config })
                } else {
                    eprintln!("owr event missing or invalid seed_config — skipping");
                    None
                }
            }
            "ootr" | "ootr_web" => Some(Self::OoTR),
            "twwr" => {
                let permalink = seed_config
                    .and_then(|c| c.get("permalink"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                Some(Self::TWWR { permalink })
            }
            "ootr_tfb" => Some(Self::OotrTriforceBlitz),
            "ootr_rsl" => Some(Self::OotrRsl),
            "mmr" => Some(Self::Mmr),
            other => {
                eprintln!("unknown seed_gen_type: {other}");
                None
            }
        }
    }

    /// Returns the display string to append to a scheduling thread for this seed gen type,
    /// or `None` if no extra information needs to be shown.
    ///
    /// `is_async` hides choices configured with `hidden_for_async` (e.g. rules that only make
    /// sense for a live, streamed race).
    pub(crate) async fn scheduling_thread_str(
        &self,
        db_pool: &PgPool,
        race: &Race,
        round_modes: Option<&HashMap<String, String>>,
        is_async: bool,
    ) -> Option<String> {
        match self {
            Self::AlttprDoorRando { source: AlttprDrSource::Boothisman, .. } => {
                let opts = super::AlttprDeRaceOptions::for_race(db_pool, race, round_modes).await;
                opts.mode_display().map(|mode| format!("This race will be played in {} mode.", mode))
            }
            Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices { config }, .. } => {
                let mut choices = super::owr_choices_for_race(db_pool, race).await;
                if is_async {
                    choices.retain(|key, _| !super::choice_entry_hidden_for_async(super::choice_entry(config, key)));
                }
                let seed_settings = super::owr_choices_description(&choices, config);
                if let Some(player_rules) = super::alttpr_dr_player_rules_str(&choices, config) {
                    Some(format!(
                        "This race will be played with {} as settings.\n\nThis race will be played with {}.",
                        seed_settings,
                        player_rules,
                    ))
                } else {
                    Some(format!("This race will be played with {} as settings.", seed_settings))
                }
            }
            Self::Owr { config } => {
                let mut choices = super::owr_choices_for_race(db_pool, race).await;
                if is_async {
                    choices.retain(|key, _| !super::choice_entry_hidden_for_async(super::choice_entry(config, key)));
                }
                Some(format!(
                    "This race will be played with {} as settings.",
                    super::owr_choices_description(&choices, config),
                ))
            }
            _ => None,
        }
    }

    /// Returns (key, label) pairs for all choice keys defined in the seed config,
    /// sorted alphabetically. Used to suggest radioChoice entries on the enter-flow page.
    pub(crate) fn radio_choice_suggestions(&self) -> Vec<(String, String)> {
        let config = match self {
            Self::Owr { config } => config,
            Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices { config }, .. } => config,
            _ => return vec![],
        };
        let Some(obj) = config.choices.as_object() else { return vec![]; };
        let mut pairs: Vec<(String, String)> = obj.iter()
            .map(|(k, v)| {
                let label = v.get("label").and_then(|l| l.as_str()).unwrap_or(k);
                (k.clone(), label.to_owned())
            })
            .collect();
        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
        pairs
    }

    /// Whether this seed gen type has per-race player-chosen settings that should
    /// be shown as a column in the race table.
    pub(crate) fn has_display_settings(&self) -> bool {
        matches!(self, Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices { .. }, .. } | Self::Owr { .. })
    }

    pub(crate) async fn settings_display_str<'e, E>(
        &self,
        executor: E,
        race: &Race,
        labels: &[(&str, String)],
    ) -> Option<String>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let config = match self {
            Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices { config }, .. } | Self::Owr { config } => config,
            _ => return None,
        };
        let team_ids = race.teams().map(|t| t.id).collect_vec();
        if team_ids.len() < 2 {
            return None;
        }
        let rows = sqlx::query!(
            "SELECT custom_choices FROM teams WHERE id = ANY($1)",
            &team_ids as _
        )
        .fetch_all(executor)
        .await
        .ok()?;

        let resolved = super::resolve_choice_values(rows.iter().map(|row| &row.custom_choices));
        let seed_settings = super::owr_choices_description_with_labels(&resolved, config, labels);
        match self {
            Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices { .. }, .. } => {
                if let Some(player_rules) = super::alttpr_dr_player_rules_str(&resolved, config) {
                    Some(format!("Seed Settings: {seed_settings}\nRace Rules: {player_rules}"))
                } else {
                    Some(seed_settings)
                }
            }
            Self::Owr { .. } => Some(seed_settings),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct UnknownSeedGenType;

impl std::fmt::Display for UnknownSeedGenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown seed_gen_type slug; valid values: owr, ootr_rsl, ootr_tfb, alttpr_avianart, ootr, mmr")
    }
}

impl std::error::Error for UnknownSeedGenType {}

impl std::str::FromStr for SeedGenType {
    type Err = UnknownSeedGenType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ootr_rsl"        => Ok(Self::OotrRsl),
            "ootr_tfb"        => Ok(Self::OotrTriforceBlitz),
            "alttpr_avianart" => Ok(Self::AlttprAvianart { practice_presets: vec![] }),
            "ootr"            => Ok(Self::OoTR),
            "mmr"             => Ok(Self::Mmr),
            _ => Err(UnknownSeedGenType),
        }
    }
}
