//! Race choices are independent of seeds. Persist before publishing or generating anything.
use super::{
    ChoiceValue,
    seed_gen_type::{AlttprDrSource, OwrEventConfig, SeedGenType},
};
use crate::prelude::*;
use sqlx::types::Json;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(unix, derive(async_proto::Protocol))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Timing {
    RaceCreation,
    RoomOpening,
    #[default]
    SeedRolling,
}

impl Timing {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::RaceCreation => "race creation/import",
            Self::RoomOpening => "room opening",
            Self::SeedRolling => "seed rolling",
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
}

impl Snapshot {
    fn validate(&self, teams: &[i64], config: &OwrEventConfig) -> sqlx::Result<()> {
        if self.teams != teams || self.definitions != config.choices {
            return Err(sqlx::Error::Protocol("Participants or choice definitions changed after this race's choices were resolved. Restore them or create a replacement race; saved outcomes cannot be rerolled.".into()));
        }
        Ok(())
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
        let config = OwrEventConfig {
            choices: self.definitions.clone(),
            selected_baseline: Some((String::new(), String::new())),
            ..OwrEventConfig::default()
        };
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
        format!(
            "Choices resolved at {}:\n{}",
            self.timing.label(),
            presentation[key].as_str().unwrap_or_default()
        )
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
    if let Some(snapshot) = read(&mut **transaction, race.id).await? {
        snapshot.validate(&teams, config)?;
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
    let snapshot = Snapshot {
        teams,
        definitions: config.choices.clone(),
        resolved: super::resolve_all_choices(&preferences, config),
        preferences,
        timing: stage,
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

/// Durable pending announcements are retried after commit, including races without a room.
pub(crate) async fn announce_pending(pool: &PgPool, ctx: &DiscordCtx) -> sqlx::Result<()> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, (i64, Json<Snapshot>, i64, Option<i16>, bool)>(
        "SELECT r.id, r.resolved_settings, r.scheduling_thread, r.game, (e.automated_asyncs OR r.async_start1 IS NOT NULL) FROM races r JOIN events e ON e.series = r.series AND e.event = r.event WHERE r.resolved_settings IS NOT NULL AND NOT COALESCE((r.resolved_settings->>'announced')::boolean, FALSE) AND (r.resolved_settings->>'timing' != 'seed_rolling' OR r.seed_data IS NOT NULL) AND r.scheduling_thread IS NOT NULL AND NOT r.ignored FOR UPDATE OF r SKIP LOCKED"
    ).fetch_all(&mut *transaction).await?;
    for (race, Json(snapshot), thread, game, is_async) in rows {
        let prefix = game
            .map(|game| format!("Game {game}\n"))
            .unwrap_or_default();
        let message = format!("{prefix}{}", snapshot.display(is_async));
        let mut delivered = true;
        for chunk in crate::discord_bot::split_discord_message(message) {
            if let Err(error) = ChannelId::new(thread as u64).say(ctx, chunk).await {
                log::warn!("Could not announce choices for race {race}: {error}");
                delivered = false;
                break;
            }
        }
        if !delivered {
            continue;
        }
        sqlx::query("UPDATE races SET resolved_settings = jsonb_set(resolved_settings, '{announced}', 'true') WHERE id = $1")
            .bind(race).execute(&mut *transaction).await?;
    }
    transaction.commit().await
}

#[cfg(test)]
mod tests;
