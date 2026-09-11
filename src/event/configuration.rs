//! Validation shared by event creation and editing. Runtime workflows remain
//! ordinary Rust implementations; their event-specific parameters are data.
use crate::{
    draft,
    racetime_bot::seed_gen_type::{OwrEventConfig, PracticeOption},
};
use serde_json::Value;

pub(crate) fn validate_seed(kind: Option<&str>, config: Option<&Value>) -> Result<(), String> {
    let empty = serde_json::json!({});
    let config = config.unwrap_or(&empty);
    if !config.is_object() {
        return Err("Seed configuration must be a JSON object.".into());
    }
    let Some(kind) = kind else { return Ok(()) };
    for field in ["practice_modes", "practice_choices", "practice_presets"] {
        if let Some(value) = config.get(field) {
            let options: Vec<PracticeOption> =
                serde_json::from_value(value.clone()).map_err(|e| format!("{field}: {e}"))?;
            let mut values = std::collections::HashSet::new();
            for option in options {
                if option.value.trim().is_empty()
                    || option.label.trim().is_empty()
                    || !values.insert(option.value)
                {
                    return Err(format!(
                        "{field} requires non-empty labels and unique, non-empty values."
                    ));
                }
            }
        }
    }
    match kind {
        "alttpr_dr" => match config.get("source") {
            None => Ok(()),
            Some(Value::String(source)) => match source.as_str() {
                "boothisman" => Ok(()),
                "mutual_choices" => validate_choices(config),
                "mystery_pool" => {
                    let value = required_string(config, "mystery_weights_url")?;
                    let url =
                        url::Url::parse(value).map_err(|e| format!("Invalid weights URL: {e}"))?;
                    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                        return Err("Weights URL must be an HTTP or HTTPS URL.".into());
                    }
                    Ok(())
                }
                _ => Err(format!("Unknown ALTTPR source: {source}")),
            },
            Some(_) => Err("source must be a string.".into()),
        },
        "owr" | "owr_tourney" => validate_choices(config),
        "twwr" => required_string(config, "permalink").map(|_| ()),
        "alttpr_avianart" => {
            if config.get("preset").is_some() {
                required_string(config, "preset")?;
            }
            Ok(()) // A draft may supply the preset instead of an event default.
        }
        "ootr" | "ootr_web" | "ootr_tfb" | "ootr_rsl" => Ok(()),
        "mmr" => {
            Err("MMR seed generation is not implemented for official and async events.".into())
        }
        _ => Err(format!("Unknown seed generator: {kind}")),
    }
}

fn required_string<'a>(config: &'a Value, field: &str) -> Result<&'a str, String> {
    config
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{field} must be a non-empty string."))
}

fn validate_choices(value: &Value) -> Result<(), String> {
    let config = OwrEventConfig::parse(value)?;
    if config.choices.is_null() {
        return Ok(());
    }
    let choices = config
        .choices
        .as_object()
        .ok_or("choices must be an object.")?;
    for (key, entry) in choices {
        let entry = entry
            .as_object()
            .ok_or_else(|| format!("Choice {key} must be an object."))?;
        for field in ["settings", "placements", "value_labels"] {
            if entry.get(field).is_some_and(|v| !v.is_object()) {
                return Err(format!("Choice {key}: {field} must be an object."));
            }
        }
        for field in ["start_inventory", "supercedes"] {
            if let Some(items) = entry.get(field) {
                let items: Vec<String> = serde_json::from_value(items.clone())
                    .map_err(|e| format!("Choice {key}, {field}: {e}"))?;
                if field == "supercedes"
                    && items
                        .iter()
                        .any(|item| item == key || !choices.contains_key(item))
                {
                    return Err(format!(
                        "Choice {key}: supercedes must reference other configured choices."
                    ));
                }
            }
        }
        if entry.get("priority").is_some_and(|v| v.as_i64().is_none()) {
            return Err(format!("Choice {key}: priority must be an integer."));
        }
        if entry
            .get("hidden_for_async")
            .is_some_and(|v| !v.is_boolean())
        {
            return Err(format!("Choice {key}: hidden_for_async must be a boolean."));
        }
    }
    Ok(())
}

pub(crate) fn validate_draft(
    kind: Option<&str>,
    config: Option<&Value>,
    seed_kind: Option<&str>,
    seed_config: Option<&Value>,
    game_count: Option<i16>,
) -> Result<(), String> {
    let named = if matches!(seed_kind, Some("owr" | "owr_tourney"))
        || seed_kind == Some("alttpr_dr")
            && seed_config
                .and_then(|c| c.get("source"))
                .and_then(Value::as_str)
                == Some("mutual_choices")
    {
        seed_config
            .filter(|c| c.get("baselines").is_some())
            .map(OwrEventConfig::parse)
            .transpose()?
    } else {
        None
    };
    let Some(kind) = kind else {
        if named.as_ref().is_some_and(|c| c.default_baseline.is_none()) {
            return Err("Named baselines need a preset draft or a default_baseline.".into());
        }
        return Ok(());
    };
    let draft =
        draft::Kind::from_db(Some(kind), config).ok_or("Invalid draft kind or configuration.")?;
    draft.validate()?;
    if let Some(named) = &named {
        let options = match &draft {
            draft::Kind::PickOnly { options, .. }
            | draft::Kind::BanPick { options, .. }
            | draft::Kind::BanOnly { options, .. } => options,
            _ => return Err("Named baselines require a generic preset draft.".into()),
        };
        for option in options {
            if !named
                .baselines
                .as_ref()
                .unwrap()
                .contains_key(&option.preset)
            {
                return Err(format!(
                    "Draft preset {} does not name a configured baseline.",
                    option.preset
                ));
            }
            if option.display_name.chars().count() > 74 {
                return Err(
                    "Draft mode names must fit a Discord button (at most 74 characters).".into(),
                );
            }
        }
    }
    if draft.uses_button_draft() {
        let consumes_preset = named.is_some()
            || seed_kind == Some("alttpr_avianart")
            || seed_kind == Some("alttpr_dr")
                && seed_config
                    .and_then(|c| c.get("source"))
                    .and_then(Value::as_str)
                    .is_none_or(|s| s == "boothisman");
        if !consumes_preset {
            return Err(
                "Preset drafts require named OWR/DR baselines, Avianart, or Boothisman.".into(),
            );
        }
        if game_count.is_some_and(|count| {
            count <= 0
                || draft
                    .game_count()
                    .is_some_and(|available| count as usize > available)
        }) {
            return Err("The draft must assign a preset for every game in the match.".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_start_delay(value: i32) -> Result<(), String> {
    if (0..=255).contains(&value) {
        Ok(())
    } else {
        Err("Start delay must be between 0 and 255 seconds.".into())
    }
}

pub(crate) fn validate_seed_policies(
    preroll: &str,
    spoiler: &str,
    generator: Option<&str>,
) -> Result<(), String> {
    if !matches!(preroll, "none" | "short" | "medium" | "long") {
        return Err("Unknown preroll mode.".into());
    }
    if !matches!(spoiler, "never" | "after" | "immediately") {
        return Err("Unknown spoiler release policy.".into());
    }
    if matches!(preroll, "short" | "long")
        && generator.is_some_and(|g| matches!(g, "alttpr_dr" | "alttpr_avianart" | "owr" | "owr_tourney" | "twwr"))
    {
        return Err("This seed generator supports None or Medium preroll; Short and Long are not implemented.".into());
    }
    Ok(())
}

pub(crate) async fn save_twwr_permalink(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    series: crate::series::Series,
    event: &str,
    permalink: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET settings_string = $1, seed_config = jsonb_set(COALESCE(seed_config, '{}'::jsonb), '{permalink}', to_jsonb($1::text)) WHERE series = $2 AND event = $3 AND seed_gen_type = 'twwr'")
        .bind(permalink).bind(series).bind(event).execute(&mut **transaction).await?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("HTH_TEST_DATABASE_URL").expect("set HTH_TEST_DATABASE_URL explicitly");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        database.ends_with("_test"),
        "database tests require a *_test database"
    );
    pool
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_owr_builds_require_valid_settings_and_supported_preroll() {
        for kind in ["owr", "owr_tourney"] {
            assert!(validate_seed(Some(kind), Some(&serde_json::json!({"base_settings": {}}))).is_ok());
            assert!(validate_seed(Some(kind), None).is_err());
            assert!(validate_seed(Some(kind), Some(&serde_json::json!({"base_settings": []}))).is_err());
            for preroll in ["none", "medium"] {
                assert!(validate_seed_policies(preroll, "never", Some(kind)).is_ok());
            }
            for preroll in ["short", "long"] {
                assert!(validate_seed_policies(preroll, "never", Some(kind)).is_err());
            }
        }
    }


    #[test]
    fn qualification_requires_an_explicit_supported_method() {
        for mode in ["none", "rank", "single"] {
            assert!(validate_qualification(mode, None).is_ok());
        }
        assert!(validate_qualification("score", Some("time_relative")).is_ok());
        assert!(validate_qualification("score", None).is_err());
        assert!(validate_qualification("score", Some("unknown")).is_err());
        assert!(validate_qualification("automatic", None).is_err());
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL"]
    async fn database_qualification_method_does_not_change_with_available_results() {
        use crate::event::{Data, teams::QualifierKind};
        let pool = test_pool().await;
        let mut tx = pool.begin().await.unwrap();
        let (series, slug): (String, String) = sqlx::query_as("SELECT series, event FROM events WHERE qualifier_mode = 'single' ORDER BY series, event LIMIT 1")
            .fetch_one(&mut *tx).await.unwrap();
        let series = series.parse().unwrap();
        // This event already has an async qualifier. An explicit method takes precedence.
        for (mode, expected) in [("none", 0), ("rank", 1), ("single", 2), ("score", 3)] {
            sqlx::query("UPDATE events SET qualifier_mode = $1, qualifier_score_kind = 'time_relative' WHERE series = $2 AND event = $3")
                .bind(mode).bind(series).bind(&slug).execute(&mut *tx).await.unwrap();
            let data = Data::new(&mut tx, series, &slug).await.unwrap().unwrap();
            let kind = data.qualifier_kind(&mut tx, None).await.unwrap();
            assert!(match (expected, kind) {
                (0, QualifierKind::None)
                | (1, QualifierKind::Rank)
                | (2, QualifierKind::Single { .. })
                | (3, QualifierKind::Score(_)) => true,
                _ => false,
            });
        }
        tx.rollback().await.unwrap();
    }
    use super::*;

    #[test]
    fn invalid_seed_configuration_does_not_silently_choose_another_generator() {
        for (kind, config) in [
            ("alttpr_dr", serde_json::json!({"source": "typo"})),
            ("alttpr_dr", serde_json::json!({"source": "mutual_choices"})),
            ("owr", serde_json::json!({"base_settings": []})),
            ("twwr", serde_json::json!({"permalink": " "})),
        ] {
            assert!(validate_seed(Some(kind), Some(&config)).is_err());
        }
    }

    #[test]
    fn legacy_choice_patches_and_priority_remain_valid() {
        assert!(validate_seed(Some("owr"), Some(&serde_json::json!({
            "base_settings": {},
            "choices": {
                "flute": {"flute_mode": "active"},
                "inverted": {"priority": 10, "supercedes": ["flute"], "settings": {"mode": "inverted"}}
            }
        }))).is_ok());
    }

    #[test]
    fn countdown_cannot_wrap_when_converted_to_u8() {
        assert!(validate_start_delay(-1).is_err());
        assert!(validate_start_delay(256).is_err());
        assert!(validate_start_delay(15).is_ok());
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn migrated_event_configurations_and_seed_records_load() {
        let pool = test_pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let events: Vec<(String, String, Option<String>, Option<Value>)> = sqlx::query_as(
            "SELECT series, event, seed_gen_type, seed_config FROM events ORDER BY series, event",
        )
        .fetch_all(&mut *transaction)
        .await
        .unwrap();
        assert!(!events.is_empty());
        for (series, slug, seed_kind, seed_config) in &events {
            let data =
                crate::event::Data::new(&mut transaction, series.parse().unwrap(), slug.as_str())
                    .await
                    .unwrap()
                    .unwrap();
            validate_seed(seed_kind.as_deref(), seed_config.as_ref())
                .unwrap_or_else(|e| panic!("{series}/{slug}: {e}"));
            validate_draft(
                data.draft_kind_str.as_deref(),
                data.draft_config.as_ref(),
                seed_kind.as_deref(),
                seed_config.as_ref(),
                Some(data.default_game_count),
            )
            .unwrap_or_else(|e| panic!("{series}/{slug}: {e}"));
            data.qualifier_kind(&mut transaction, None).await.unwrap();
            if seed_kind.as_deref() == Some("twwr") {
                assert_eq!(
                    data.twwr_permalink(),
                    seed_config
                        .as_ref()
                        .and_then(|c| c.get("permalink"))
                        .and_then(Value::as_str)
                );
                assert_eq!(data.twwr_permalink(), data.settings_string.as_deref());
            }
        }
        let seeds: Vec<Value> =
            sqlx::query_scalar("SELECT seed_data FROM races WHERE seed_data IS NOT NULL")
                .fetch_all(&mut *transaction)
                .await
                .unwrap();
        assert!(!seeds.is_empty());
        for data in &seeds {
            let files = crate::seed::Files::from_seed_data(data)
                .expect("migrated seed must retain a readable identity");
            assert!(crate::seed::Files::from_seed_data(&files.to_seed_data_base()).is_some());
        }
        // A newly saved goal is visible to room discovery immediately, without
        // constructing global bot state or connecting to external integrations.
        let row: (String, String, String) = sqlx::query_as("SELECT e.series, e.event, grc.category_slug FROM events e JOIN game_series gs ON gs.series = e.series JOIN game_racetime_connection grc ON grc.game_id = gs.game_id WHERE e.racetime_goal_slug IS NOT NULL LIMIT 1")
            .fetch_one(&mut *transaction).await.unwrap();
        let goal = "HTH migration regression test goal";
        assert!(
            !crate::racetime_bot::configured_goal_exists(&mut *transaction, &row.2, goal, true)
                .await
                .unwrap()
        );
        sqlx::query("UPDATE events SET racetime_goal_slug = $1, is_custom_goal = true WHERE series = $2 AND event = $3")
            .bind(goal).bind(&row.0).bind(&row.1).execute(&mut *transaction).await.unwrap();
        assert!(
            crate::racetime_bot::configured_goal_exists(&mut *transaction, &row.2, goal, true)
                .await
                .unwrap()
        );
        assert!(
            !crate::racetime_bot::configured_goal_exists(&mut *transaction, &row.2, goal, false)
                .await
                .unwrap()
        );
        assert!(
            !crate::racetime_bot::configured_goal_exists(
                &mut *transaction,
                "unconfigured-test-category",
                goal,
                true
            )
            .await
            .unwrap()
        );
        eprintln!(
            "Validated {} event configurations and {} persisted seed identities",
            events.len(),
            seeds.len()
        );
        transaction.rollback().await.unwrap();
    }
}

/// A selected scoring strategy can be retained while qualification is disabled.
/// It only takes effect when the organizer explicitly selects scored qualification.
pub(crate) fn validate_qualification(mode: &str, score_kind: Option<&str>) -> Result<(), String> {
    match mode {
        "none" | "rank" | "single" => Ok(()),
        "pooled_by_mode" if score_kind.is_none() => Ok(()),
        "pooled_by_mode" => {
            Err("Pooled-by-mode qualification uses its own cohort scoring configuration.".into())
        }
        "score"
            if score_kind.is_some_and(|kind| {
                super::teams::QualifierScoreKind::from_slug(kind).is_some()
                    || matches!(kind, "songs_of_hope" | "triforce_blitz")
            }) =>
        {
            Ok(())
        }
        "score" => Err("Choose a qualifier scoring strategy for configured scoring.".into()),
        _ => Err("Choose a qualification method.".into()),
    }
}
