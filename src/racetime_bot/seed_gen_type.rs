use {
    std::collections::HashMap,
    itertools::Itertools as _,
    sqlx::PgPool,
    crate::cal::Race,
};

/// Which seed generator an event uses, stored in `events.seed_gen_type`.
#[derive(Debug, Clone)]
pub(crate) enum SeedGenType {
    AlttprDoorRando {
        source: AlttprDrSource,
    },
    AlttprAvianart,
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
pub(crate) enum AlttprDrSource {
    /// Fetch a preset YAML from the boothisman.de API.
    Boothisman,
    /// Read settings from `teams.custom_choices`; both teams must agree on each option.
    MutualChoices,
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
                    Some("mutual_choices") => AlttprDrSource::MutualChoices,
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
                Some(Self::AlttprDoorRando { source })
            }
            "alttpr_avianart" => Some(Self::AlttprAvianart),
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
    pub(crate) async fn scheduling_thread_str(
        &self,
        db_pool: &PgPool,
        race: &Race,
        round_modes: Option<&HashMap<String, String>>,
    ) -> Option<String> {
        match self {
            Self::AlttprDoorRando { source: AlttprDrSource::Boothisman } => {
                let opts = super::AlttprDeRaceOptions::for_race(db_pool, race, round_modes).await;
                opts.mode_display().map(|mode| format!("This race will be played in {} mode.", mode))
            }
            Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices } => {
                let opts = super::CrosskeysRaceOptions::for_race(db_pool, race).await;
                Some(format!(
                    "This race will be played with {} as settings.\n\nThis race will be played with {}.",
                    opts.as_seed_options_str(),
                    opts.as_race_options_str(),
                ))
            }
            _ => None,
        }
    }

    /// Whether this seed gen type has per-race player-chosen settings that should
    /// be shown as a column in the race table.
    pub(crate) fn has_display_settings(&self) -> bool {
        matches!(self, Self::AlttprDoorRando { source: AlttprDrSource::MutualChoices })
    }

    /// For `MutualChoices` events: query the DB for each team's `custom_choices`
    /// and return a comma-separated string of the human-readable labels for every
    /// setting that **all** teams chose "yes".
    ///
    /// Returns `None` if this seed gen type is not `MutualChoices`, if `labels`
    /// is empty, or if there are fewer than 2 teams.
    ///
    /// `labels` should come from `event::Data::boolean_choice_requirements()`,
    /// which maps DB keys to human-readable display names.
    ///
    /// Accepts any sqlx executor: `&PgPool`, `&mut Transaction<'_, Postgres>`, etc.
    pub(crate) async fn agreed_settings_str<'e, E>(
        &self,
        executor: E,
        race: &Race,
        labels: &[(&str, String)],
    ) -> Option<String>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let source = match self {
            Self::AlttprDoorRando { source } => source,
            _ => return None,
        };
        if !matches!(source, AlttprDrSource::MutualChoices) {
            return None;
        }
        if labels.is_empty() {
            return None;
        }
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

        // A setting is "agreed" when every team chose "yes" for that key.
        let agreed = labels
            .iter()
            .filter(|(key, _)| {
                rows.iter().all(|row| {
                    row.custom_choices
                        .get(*key)
                        .is_some_and(|v| v == "yes")
                })
            })
            .map(|(_, label)| label.as_str())
            .collect_vec();

        if agreed.is_empty() {
            Some("base settings".to_owned())
        } else {
            Some(agreed.join(", "))
        }
    }
}
