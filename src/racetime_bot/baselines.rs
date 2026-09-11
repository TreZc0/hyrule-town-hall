//! Baseline selection is shared by live, async and practice generation.
use super::seed_gen_type::OwrEventConfig;
use crate::{draft, prelude::*};

pub(crate) fn validate_match(event: &event::Data<'_>, games: i16) -> Result<(), String> {
    use super::seed_gen_type::{AlttprDrSource, SeedGenType};
    if let Some(
        SeedGenType::Owr { config, .. }
        | SeedGenType::AlttprDoorRando {
            source: AlttprDrSource::MutualChoices { config },
            ..
        },
    ) = &event.seed_gen_type
    {
        if config.baselines.is_some() {
            if event.round_modes.is_some() && event.draft_kind_str.is_some() {
                return Err("Remove round_modes to use a named-baseline draft.".into());
            }
            if let Some(kind) = event.draft_kind() {
                if games < 1 || kind.game_count().is_none_or(|count| games as usize > count) {
                    return Err(
                        "The mode draft must assign a baseline for every game in the match.".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) async fn format_draft_step(
    event: &event::Data<'_>,
    race: &Race,
    step: &mut draft::Step,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), cal::Error> {
    use super::seed_gen_type::{AlttprDrSource, SeedGenType};
    if matches!(step.kind, draft::StepKind::Done(_)) {
        if let Some(
            SeedGenType::Owr { config, .. }
            | SeedGenType::AlttprDoorRando {
                source: AlttprDrSource::MutualChoices { config },
                ..
            },
        ) = &event.seed_gen_type
        {
            if config.baselines.is_some() {
                let games = race.game_count(transaction).await?;
                if let Some(summary) = race
                    .draft
                    .as_ref()
                    .and_then(|state| config.draft_summary(state, games))
                {
                    step.message = summary;
                }
            }
        }
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct PatchReport {
    pub(super) suppressed: std::collections::BTreeMap<String, Vec<String>>,
    pub(super) overridden: std::collections::BTreeMap<String, Vec<(String, String)>>,
}

pub(super) fn presentation(
    config: &OwrEventConfig,
    resolved: &HashMap<String, bool>,
    report: &PatchReport,
) -> Option<serde_json::Value> {
    let (key, label) = config.selected_baseline.as_ref()?;
    let name = |key: &str| super::choice_entry_label(config.choices.get(key), key).to_owned();
    let summary = |is_async| {
        let mut lines = Vec::new();
        if let Some(choices) = config.choices.as_object() {
            for (key, entry) in choices {
                let affects_seed = super::choice_entry_affects_seed(Some(entry));
                if is_async && !affects_seed && super::choice_entry_hidden_for_async(Some(entry)) {
                    continue;
                }
                let enabled = *resolved.get(key).unwrap_or(&false);
                let mut result = if affects_seed {
                    if enabled {
                        if let Some(by) = report.suppressed.get(key) {
                            format!(
                                "not applied (superseded by {})",
                                by.iter().map(|k| name(k)).join(", ")
                            )
                        } else {
                            "applied".to_owned()
                        }
                    } else {
                        "not applied; baseline unchanged".to_owned()
                    }
                } else {
                    if enabled { "allowed" } else { "not allowed" }.to_owned()
                };
                if enabled && !report.suppressed.contains_key(key) {
                    if let Some(fields) = report.overridden.get(key) {
                        result.push_str(&format!(
                            " ({} overridden)",
                            fields
                                .iter()
                                .map(|(field, by)| format!("{field} by {}", name(by)))
                                .join(", ")
                        ));
                    }
                }
                // Value labels describe rules; for patches retain the actual application status,
                // so a suppressed option can never be announced as an active setting.
                if !affects_seed {
                    if let Some(value_label) = entry
                        .get("value_labels")
                        .and_then(|v| v.get(if enabled { "always" } else { "never" }))
                        .and_then(|v| v.as_str())
                    {
                        if !value_label.is_empty() {
                            lines.push(value_label.to_owned());
                        }
                        continue;
                    }
                }
                lines.push(format!("{}: {result}", name(key)));
            }
        }
        if lines.is_empty() {
            "No optional patches applied.".to_owned()
        } else {
            lines.join("\n")
        }
    };
    Some(
        json!({"baseline_key": key, "baseline_label": label, "settings_summary": summary(false), "async_settings_summary": summary(true)}),
    )
}

/// Add metadata before any consumer persists or announces a successful seed.
pub(super) fn with_presentation(
    mut updates: mpsc::Receiver<super::SeedRollUpdate>,
    presentation: Option<serde_json::Value>,
) -> mpsc::Receiver<super::SeedRollUpdate> {
    let Some(presentation) = presentation else {
        return updates;
    };
    let (tx, rx) = mpsc::channel(128);
    tokio::spawn(async move {
        while let Some(mut update) = updates.recv().await {
            if let super::SeedRollUpdate::Done { seed, .. } = &mut update {
                if let Some(data) = &mut seed.seed_data {
                    data["seed_presentation"] = presentation.clone();
                }
            }
            if tx.send(update).await.is_err() {
                break;
            }
        }
    });
    rx
}

impl OwrEventConfig {
    pub(crate) fn parse(value: &serde_json::Value) -> Result<Self, String> {
        let config: Self = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        if let Some(baselines) = &config.baselines {
            if ["base_settings", "base_placements", "start_inventory"]
                .iter()
                .any(|key| value.get(key).is_some())
            {
                return Err("Use either baselines or a root baseline, not both.".into());
            }
            if baselines.is_empty() {
                return Err("baselines must contain at least one named baseline.".into());
            }
            for (key, baseline) in baselines {
                // Leaves room for the existing Discord draft_pick_preview_<game>_ prefix.
                if key.is_empty()
                    || key.len() > 64
                    || !key
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                {
                    return Err(
                        "Baseline keys must be 1–64 ASCII letters, digits, underscores or hyphens."
                            .into(),
                    );
                }
                if baseline.label.trim().is_empty() || baseline.label.chars().count() > 80 {
                    return Err(format!(
                        "Baseline {key}: label must contain 1–80 characters."
                    ));
                }
                Self::check_base(&baseline.base_settings, &baseline.base_placements)
                    .map_err(|e| format!("Baseline {key}: {e}"))?;
            }
            if config
                .default_baseline
                .as_ref()
                .is_some_and(|key| !baselines.contains_key(key))
            {
                return Err("default_baseline must name a configured baseline.".into());
            }
        } else {
            if value.get("baselines").is_some() || config.default_baseline.is_some() {
                return Err(
                    "baselines must be a non-empty object when using named baselines.".into(),
                );
            }
            Self::check_base(&config.base_settings, &config.base_placements)?;
        }
        Ok(config)
    }

    fn check_base(
        settings: &serde_json::Value,
        placements: &serde_json::Value,
    ) -> Result<(), String> {
        if !settings.is_object() {
            return Err("base_settings must be an object.".into());
        }
        if !placements.is_null() && !placements.is_object() {
            return Err("base_placements must be an object.".into());
        }
        Ok(())
    }

    /// Returns a fresh, single-baseline configuration. A supplied but invalid key never falls back.
    pub(crate) fn select(&self, key: Option<&str>) -> Result<Self, String> {
        let Some(baselines) = &self.baselines else {
            return Ok(self.clone());
        };
        let key = key
            .or(self.default_baseline.as_deref())
            .ok_or("Select a baseline before rolling a seed.")?;
        let baseline = baselines.get(key).ok_or_else(|| {
            format!("Unknown baseline {key}; check the event and draft configuration.")
        })?;
        Ok(Self {
            base_settings: baseline.base_settings.clone(),
            base_placements: baseline.base_placements.clone(),
            start_inventory: baseline.start_inventory.clone(),
            choices: self.choices.clone(),
            choice_resolution: self.choice_resolution,
            selected_baseline: Some((key.to_owned(), baseline.label.clone())),
            ..Self::default()
        })
    }

    pub(crate) async fn for_draft(
        &self,
        state: Option<&Draft>,
        kind: Option<&draft::Kind>,
        game: Option<i16>,
    ) -> Result<Self, String> {
        if self.baselines.is_none() {
            return Ok(self.clone());
        }
        let Some(kind) = kind else {
            if state.is_some() {
                return Err(
                    "This race has a draft but the event's draft configuration is missing.".into(),
                );
            }
            return self.select(None);
        };
        kind.validate()
            .map_err(|error| format!("Invalid mode draft: {error}"))?;
        let count = kind
            .game_count()
            .ok_or("Named baselines require a preset draft.")?;
        let game = game.unwrap_or(1);
        if game < 1 || game as usize > count {
            return Err("The draft does not assign a baseline for this game.".into());
        }
        let state = state.ok_or("The mode draft has not been started.")?;
        if (1..=count).any(|n| {
            !state
                .settings
                .contains_key(format!("game{n}_preset").as_str())
        }) {
            return Err("Finish the mode draft before rolling a seed.".into());
        }
        let step = state
            .next_step(kind, Some(game), &mut draft::MessageContext::None)
            .await
            .map_err(|e| e.to_string())?;
        let draft::StepKind::Done(settings) = step.kind else {
            return Err("Finish the mode draft before rolling a seed.".into());
        };
        let key = settings
            .get("preset")
            .and_then(|v| v.as_str())
            .ok_or("Draft did not select a baseline.")?;
        if kind.preset_display_name(key).is_none() {
            return Err(format!(
                "Baseline {key} is no longer available in this draft."
            ));
        }
        self.select(Some(key))
    }

    pub(crate) async fn for_race(
        &self,
        race: &Race,
        event: &event::Data<'_>,
    ) -> Result<Self, String> {
        if self.baselines.is_some() && event.draft_kind_str.is_some() && event.round_modes.is_some()
        {
            return Err("Remove round_modes to use a named-baseline draft.".into());
        }
        let kind = event.draft_kind();
        if self.baselines.is_some() && event.draft_kind_str.is_some() && kind.is_none() {
            return Err("The event's mode draft configuration is invalid.".into());
        }
        self.for_draft(race.draft.as_ref(), kind.as_ref(), race.game)
            .await
    }

    /// Read-only description: never resolves random choices or guesses a pending draft's mode.
    pub(crate) fn pending_baseline(&self, race: &Race, draft_required: bool) -> Option<String> {
        let baselines = self.baselines.as_ref()?;
        let key = match &race.draft {
            Some(state) => state
                .settings
                .get(format!("game{}_preset", race.game.unwrap_or(1)).as_str())
                .map(|s| s.as_ref()),
            None if !draft_required => self.default_baseline.as_deref(),
            None => None,
        };
        Some(match key.and_then(|key| baselines.get(key)) {
            Some(base) => format!("Baseline: {}", base.label),
            None => "Baseline: awaiting mode draft".into(),
        })
    }

    pub(crate) fn draft_summary(&self, state: &Draft, actual_games: i16) -> Option<String> {
        let baselines = self.baselines.as_ref()?;
        let mut lines = vec!["Mode draft completed.".to_owned()];
        for game in 1..=actual_games {
            let key = state.settings.get(format!("game{game}_preset").as_str())?;
            let baseline = baselines.get(key.as_ref())?;
            let suffix = if game == actual_games && actual_games > 2 {
                " (if needed)"
            } else {
                ""
            };
            lines.push(format!("Game {game}: {}{suffix}", baseline.label));
        }
        Some(lines.join("\n"))
    }
}

/// Presentation comes from the saved seed, never from today's event configuration.
pub(crate) fn seed_summary(data: &serde_json::Value, is_async: bool) -> Option<String> {
    if let Some(presentation) = data.get("seed_presentation") {
        let label = presentation.get("baseline_label")?.as_str()?;
        let summary = if is_async {
            presentation.get("async_settings_summary")
        } else {
            None
        }
        .or_else(|| presentation.get("settings_summary"))?
        .as_str()?;
        Some(format!("Baseline: {label}\n{summary}"))
    } else {
        data.get("resolved_randoms")
            .and_then(|v| v.as_str())
            .map(|s| format!("Final settings - {s}"))
    }
}

/// Keep individual messages below both Discord and Racetime limits, even for long custom labels.
pub(crate) fn message_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    for line in text.lines() {
        for c in line.chars().chain(iter::once('\n')) {
            if chunk.len() + c.len_utf8() > 900 {
                chunks.push(mem::take(&mut chunk));
            }
            chunk.push(c);
        }
    }
    if !chunk.is_empty() {
        chunks.push(chunk)
    }
    chunks
}

#[cfg(test)]
mod tests;
