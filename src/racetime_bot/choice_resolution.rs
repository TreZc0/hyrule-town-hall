//! Race choices are independent of seeds. Persist before publishing or generating anything.
use super::{
    ChoiceValue,
    seed_gen_type::{AlttprDrSource, OwrEventConfig, SeedGenType},
};
use crate::prelude::*;
use serenity::all::{EditMessage, GetMessages};
use sqlx::types::Json;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(unix, derive(async_proto::Protocol))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Timing {
    RaceCreation,
    RoomOpening,
    // Keep the stored key/protocol variant compatible. Generation fixes the
    // choices privately; each participant sees them only with their seed.
    #[default]
    SeedRolling,
}

impl Timing {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::RaceCreation => "race creation/import",
            Self::RoomOpening => "room opening",
            Self::SeedRolling => "seed reveal",
        }
    }
}

pub(crate) fn config(kind: &SeedGenType) -> Option<&OwrEventConfig> {
    match kind {
        SeedGenType::Owr { config, .. }
        | SeedGenType::AlttprDoorRando {
            source: AlttprDrSource::MutualChoices { config },
            ..
        } => Some(config),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    teams: Vec<i64>,
    definitions: serde_json::Value,
    preferences: HashMap<String, ChoiceValue>,
    pub(crate) resolved: HashMap<String, bool>,
    timing: Timing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_baseline: Option<(String, String)>,
}

impl Snapshot {
    fn validate(&self, teams: &[i64], config: &OwrEventConfig) -> sqlx::Result<()> {
        if self.teams != teams || &self.definitions != config.choice_definitions() {
            return Err(sqlx::Error::Protocol("Participants or choice definitions changed after this race's choices were resolved. Restore them or create a replacement race; saved outcomes cannot be rerolled.".into()));
        }
        if let (Some((saved, _)), Some((selected, _))) = (&self.selected_baseline, &config.selected_baseline) {
            if saved != selected {
                return Err(sqlx::Error::Protocol("The baseline changed after this race's seed settings were selected. Restore the draft or create a replacement race.".into()));
            }
        }
        Ok(())
    }

    /// A saved seed can exist long before either async participant sees it.
    pub(crate) fn visible_at(&self, stage: Timing) -> bool {
        self.timing <= stage
    }

    pub(crate) fn seed_presentation(&self, config: &OwrEventConfig) -> serde_json::Value {
        json!({
            "baseline_key": config.selected_baseline.as_ref().map(|(key, _)| key),
            "baseline_label": config.selected_baseline.as_ref().map(|(_, label)| label),
            "settings_summary": self.display_for_config(false, config),
            "async_settings_summary": self.display_for_config(true, config),
            "includes_choice_heading": true,
        })
    }

    pub(crate) fn values(&self) -> HashMap<String, ChoiceValue> {
        self.resolved
            .iter()
            .map(|(key, enabled)| {
                (
                    key.clone(),
                    if *enabled {
                        ChoiceValue::Always
                    } else {
                        ChoiceValue::Never
                    },
                )
            })
            .collect()
    }

    pub(crate) fn display(&self, is_async: bool) -> String {
        if let Some((key, label)) = &self.selected_baseline {
            return format!("Choices resolved at {}:\nBaseline: {label}\n{}", self.timing.label(), self.baseline_summary(is_async, Some(key)));
        }
        let scopes: std::collections::BTreeSet<_> = self.definitions.as_object().into_iter()
            .flat_map(|choices| choices.values())
            .filter_map(|entry| entry.get("baselines").and_then(serde_json::Value::as_array))
            .flatten().filter_map(serde_json::Value::as_str).collect();
        if !scopes.is_empty() {
            // Decisions can be published before the draft. Describe their conditional scope,
            // never claim that patches for every mode apply to one seed.
            let mut parts = vec![format!("Choices resolved at {} (mode selection pending):", self.timing.label())];
            for key in scopes {
                parts.push(format!("If {key} is selected:\n{}", self.baseline_summary(is_async, Some(key))));
            }
            return parts.join("\n");
        }
        format!("Choices resolved at {}:\n{}", self.timing.label(), self.baseline_summary(is_async, None))
    }

    pub(crate) fn display_for_config(&self, is_async: bool, config: &OwrEventConfig) -> String {
        let mut snapshot = self.clone();
        if snapshot.selected_baseline.is_none() {
            snapshot.selected_baseline = config.selected_baseline.clone();
        }
        let display = snapshot.display(is_async);
        if snapshot.selected_baseline.is_none() && config.baselines.is_some() {
            format!("Baseline: awaiting mode draft\n{display}")
        } else {
            display
        }
    }

    fn baseline_summary(&self, is_async: bool, baseline: Option<&str>) -> String {
        let mut config = OwrEventConfig {
            choices: baseline.map_or_else(|| self.definitions.clone(), |key| OwrEventConfig::choices_for_baseline(&self.definitions, key)),
            selected_baseline: Some((baseline.unwrap_or_default().to_owned(), String::new())),
            ..OwrEventConfig::default()
        };
        // Disabled optional patches were never in play unless the players randomized them.
        // Keep rule-only outcomes (including bans), and preserve supersession diagnostics.
        if let Some(choices) = config.choices.as_object_mut() {
            choices.retain(|key, entry| {
                !super::choice_entry_affects_seed(Some(entry))
                    || self.resolved.get(key) == Some(&true)
                    || self.preferences.get(key) == Some(&ChoiceValue::Random)
            });
        }
        let report = super::apply_patches_with_supercedes(
            &self.resolved,
            &config.choices,
            &mut serde_json::Map::new(),
            &mut serde_json::Map::new(),
            &mut Vec::new(),
        );
        let presentation = super::baselines::presentation(&config, &self.resolved, &report)
            .expect("display baseline provided");
        let key = if is_async {
            "async_settings_summary"
        } else {
            "settings_summary"
        };
        presentation[key].as_str().unwrap_or_default().to_owned()
    }

    pub(crate) fn reveal(
        &self,
        config: &OwrEventConfig,
        labels: &[(String, String)],
    ) -> Option<String> {
        super::reveal_resolved_randoms_str(&self.preferences, &self.resolved, config, labels)
    }
}

pub(crate) async fn read<'e, E>(executor: E, race: Id<Races>) -> sqlx::Result<Option<Snapshot>>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, Json<Snapshot>>(
        "SELECT resolved_settings FROM races WHERE id = $1 AND resolved_settings IS NOT NULL",
    )
    .bind(i64::from(race))
    .fetch_optional(executor)
    .await
    .map(|value| value.map(|Json(value)| value))
}

/// The row lock serializes room and seed handlers. Repeated calls return the original decision.
pub(crate) async fn ensure(
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    config: &OwrEventConfig,
    stage: Timing,
) -> sqlx::Result<Option<Snapshot>> {
    let (team1, team2, team3) = sqlx::query_as::<_, (Option<i64>, Option<i64>, Option<i64>)>(
        "SELECT team1, team2, team3 FROM races WHERE id = $1 FOR UPDATE",
    )
    .bind(i64::from(race.id))
    .fetch_one(&mut **transaction)
    .await?;
    let mut stored_teams = [team1, team2, team3].into_iter().flatten().collect_vec();
    stored_teams.extend(
        sqlx::query_scalar::<_, i64>("SELECT team FROM race_entrants WHERE race = $1")
            .bind(i64::from(race.id))
            .fetch_all(&mut **transaction)
            .await?,
    );
    stored_teams.sort_unstable();
    let mut teams = race.teams().map(|team| i64::from(team.id)).collect_vec();
    teams.sort_unstable();
    if stored_teams != teams {
        return Err(sqlx::Error::Protocol(
            "Participants changed while preparing the race. Reload the race before retrying."
                .into(),
        ));
    }
    if let Some(mut snapshot) = read(&mut **transaction, race.id).await? {
        snapshot.validate(&teams, config)?;
        if snapshot.selected_baseline.is_none() && config.selected_baseline.is_some() {
            snapshot.selected_baseline = config.selected_baseline.clone();
            sqlx::query("UPDATE races SET resolved_settings = jsonb_set(resolved_settings, '{selected_baseline}', $2) WHERE id = $1")
                .bind(i64::from(race.id)).bind(Json(&snapshot.selected_baseline))
                .execute(&mut **transaction).await?;
        }
        return Ok(Some(snapshot));
    }
    if stage < config.choice_resolution
        || config
            .choices
            .as_object()
            .is_none_or(|choices| choices.is_empty())
    {
        return Ok(None);
    }
    // Do not invent results for seeds generated before this feature was enabled.
    if race.seed.files().is_some() && stage != Timing::SeedRolling {
        return Ok(None);
    }
    if race.teams_opt().is_none() || teams.len() < 2 {
        if stage == Timing::SeedRolling && !teams.is_empty() {
            return Err(sqlx::Error::Protocol(
                "All participants must be registered before resolving race choices.".into(),
            ));
        }
        return Ok(None);
    }
    let rows = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT custom_choices FROM teams WHERE id = ANY($1)",
    )
    .bind(&teams)
    .fetch_all(&mut **transaction)
    .await?;
    if rows.len() != teams.len() {
        return Err(sqlx::Error::Protocol(
            "A participant is missing while resolving race choices.".into(),
        ));
    }
    let preferences = super::resolve_choice_values(&rows);
    let all_choices = OwrEventConfig {
        choices: config.choice_definitions().clone(),
        ..OwrEventConfig::default()
    };
    let snapshot = Snapshot {
        teams,
        definitions: all_choices.choices.clone(),
        resolved: super::resolve_all_choices(&preferences, &all_choices),
        preferences,
        timing: config.choice_resolution,
        selected_baseline: config.selected_baseline.clone(),
    };
    sqlx::query("UPDATE races SET resolved_settings = $2 WHERE id = $1")
        .bind(i64::from(race.id))
        .bind(Json(&snapshot))
        .execute(&mut **transaction)
        .await?;
    Ok(Some(snapshot))
}

pub(crate) async fn for_seed(
    pool: &PgPool,
    race: &Race,
    config: &OwrEventConfig,
) -> sqlx::Result<Option<Snapshot>> {
    let mut transaction = pool.begin().await?;
    let snapshot = ensure(&mut transaction, race, config, Timing::SeedRolling).await?;
    transaction.commit().await?;
    Ok(snapshot)
}

const PENDING_ANNOUNCEMENTS: &str = "SELECT r.id, r.resolved_settings, r.scheduling_thread, r.game, (e.automated_asyncs OR r.async_start1 IS NOT NULL) FROM races r JOIN events e ON e.series = r.series AND e.event = r.event WHERE r.resolved_settings IS NOT NULL AND NOT COALESCE((r.resolved_settings->>'announced')::boolean, FALSE) AND r.resolved_settings->>'timing' = 'race_creation' AND r.scheduling_thread IS NOT NULL AND NOT r.ignored FOR UPDATE OF r SKIP LOCKED";

/// Only creation-time choices belong in the shared scheduling thread.
pub(crate) async fn announce_pending(pool: &PgPool, ctx: &DiscordCtx) -> sqlx::Result<()> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, (i64, Json<Snapshot>, i64, Option<i16>, bool)>(
        PENDING_ANNOUNCEMENTS
    ).fetch_all(&mut *transaction).await?;
    for (race, Json(snapshot), thread, game, is_async) in rows {
        let prefix = game
            .map(|game| format!("Game {game}\n"))
            .unwrap_or_default();
        let message = format!("{prefix}{}", snapshot.display(is_async));
        let thread = ChannelId::new(thread as u64);
        // Imports can fill in participants after the scheduling thread was created.
        // Put these late creation-time decisions after its welcome paragraph too.
        let delivered: serenity::Result<()> = async {
            let messages = thread.messages(ctx, GetMessages::new().after(MessageId::new(1)).limit(1)).await?;
            let Some(mut opening) = messages.into_iter().next() else {
                return Err(serenity::Error::Other("Scheduling thread has no opening message"));
            };
            if !opening.content.contains(&message) {
                let mut chunks = crate::discord_bot::split_discord_message(crate::discord_bot::insert_scheduling_settings(&opening.content, &message)).into_iter();
                opening.edit(ctx, EditMessage::new().content(chunks.next().unwrap_or_default())).await?;
                for chunk in chunks {
                    thread.say(ctx, chunk).await?;
                }
            }
            Ok(())
        }.await;
        if let Err(error) = delivered {
            log::warn!("Could not announce choices for race {race}: {error}");
            continue;
        }
        sqlx::query("UPDATE races SET resolved_settings = jsonb_set(resolved_settings, '{announced}', 'true') WHERE id = $1")
            .bind(race).execute(&mut *transaction).await?;
    }
    transaction.commit().await
}

#[cfg(test)]
mod tests;
