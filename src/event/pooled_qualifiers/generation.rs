use super::*;
use racetime_bot::{
    GlobalState, SeedRollUpdate, UnlockSpoilerLog,
    seed_gen_type::{AlttprDrSource, SeedGenType},
};

pub(crate) fn supported(kind: &SeedGenType) -> bool {
    match kind {
        SeedGenType::Owr { config, .. }
        | SeedGenType::AlttprDoorRando {
            source: AlttprDrSource::MutualChoices { config },
            ..
        } => config.baselines.is_none() && config.base_settings.is_object(),
        SeedGenType::AlttprDoorRando {
            source: AlttprDrSource::MysteryPool { weights_url },
            ..
        } => Url::parse(weights_url).is_ok(),
        SeedGenType::AlttprAvianart { default_preset, .. } => default_preset
            .as_ref()
            .is_some_and(|preset| !preset.trim().is_empty()),
        SeedGenType::TWWR { permalink } => !permalink.trim().is_empty(),
        _ => false,
    }
}

/// The same baseline-only resolver serves private slots and linked live qualifiers.
/// Entrant choices and race drafts are intentionally absent from this interface.
pub(crate) fn roll(
    state: Arc<GlobalState>,
    kind: &SeedGenType,
) -> Result<mpsc::Receiver<SeedRollUpdate>, Error> {
    if !supported(kind) {
        return Err(Error::ModeUnavailable);
    }
    Ok(match kind {
        SeedGenType::Owr { config, .. } => state.roll_pooled_owr_seed(config.clone()),
        SeedGenType::AlttprDoorRando {
            source: AlttprDrSource::MutualChoices { config },
            ..
        } => state.roll_mutual_choices_dr_seed(config.clone(), HashMap::new(), None),
        SeedGenType::AlttprDoorRando {
            source: AlttprDrSource::MysteryPool { weights_url },
            ..
        } => state.roll_mystery_pool_seed(weights_url.clone()),
        SeedGenType::AlttprAvianart {
            default_preset: Some(preset),
            ..
        } => state.roll_avianart_seed(preset.clone()),
        SeedGenType::TWWR { permalink } => {
            state.roll_twwr_seed(None, permalink.clone(), UnlockSpoilerLog::Never)
        }
        _ => return Err(Error::ModeUnavailable),
    })
}

pub(crate) async fn validate_payload(
    kind: &SeedGenType,
    data: &serde_json::Value,
) -> Result<(), Error> {
    let files = seed::Files::from_seed_data(data).ok_or(Error::NoSeed)?;
    match (kind, files) {
        (SeedGenType::Owr { .. }, seed::Files::AlttprDoorRando { uuid, is_owr: true }) => {
            validate_patch(data, uuid, "OR_").await
        }
        (
            SeedGenType::AlttprDoorRando { .. },
            seed::Files::AlttprDoorRando {
                uuid,
                is_owr: false,
            },
        ) => validate_patch(data, uuid, "DR_").await,
        (
            SeedGenType::AlttprAvianart { .. },
            seed::Files::AvianartSeed {
                hash,
                seed_hash: Some(seed_hash),
            },
        ) if !hash.is_empty() && seed_hash.iter().all(|icon| !icon.is_empty()) => Ok(()),
        (
            SeedGenType::TWWR { .. },
            seed::Files::TwwrPermalink {
                permalink,
                seed_hash,
            },
        ) if !permalink.is_empty() && !seed_hash.is_empty() => Ok(()),
        _ => Err(Error::InvalidTransition(
            "seed payload does not match the mode generator or lacks delivery data".into(),
        )),
    }
}

async fn validate_patch(data: &serde_json::Value, uuid: Uuid, prefix: &str) -> Result<(), Error> {
    let hash = seed::Data::from_seed_data_only(Some(data.clone()), None, false).file_hash;
    if hash.is_none_or(|hash| hash.iter().any(|icon| icon.is_empty())) {
        return Err(Error::InvalidTransition(
            "seed is missing its five hash icons".into(),
        ));
    }
    let path = Path::new(seed::DIR).join(format!("{prefix}{uuid}.bps"));
    if tokio::fs::metadata(path)
        .await
        .map_or(true, |metadata| !metadata.is_file() || metadata.len() == 0)
    {
        return Err(Error::InvalidTransition(
            "generated patch is missing or empty".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    mode_id: i64,
    position: Option<i16>,
    retry: bool,
) -> Result<(), Error> {
    lock_event(tx, series, event).await?;
    let config = Config::load(tx, series, event)
        .await?
        .ok_or(Error::NotConfigured)?;
    let mode = Mode::for_event(tx, series, event)
        .await?
        .into_iter()
        .find(|mode| mode.id == mode_id && mode.enabled)
        .ok_or(Error::ModeUnavailable)?;
    let kind = SeedGenType::from_db(Some(&mode.seed_gen_type), Some(&mode.seed_config))
        .ok_or(Error::ModeUnavailable)?;
    if mode.generator_profile != "default" || !supported(&kind) {
        return Err(Error::ModeUnavailable);
    }
    if position.is_some_and(|position| position < 1 || position > config.pool_seed_count) {
        return Err(Error::NoSeed);
    }
    for slot in 1..=config.pool_seed_count {
        if position.is_some_and(|position| position != slot) {
            continue;
        }
        sqlx::query(r#"INSERT INTO qualifier_seeds(series, event, mode_id, source, pool_position, generator_profile, settings_fingerprint)
            VALUES ($1,$2,$3,'async_pool',$4,$5,$6) ON CONFLICT (mode_id, pool_position) DO UPDATE SET generation_state = 'pending', generation_claim = NULL,
                generation_claim_until = NULL, generation_error = NULL
            WHERE $7 AND qualifier_seeds.generation_state = 'failed' AND qualifier_seeds.released_at IS NULL
              AND NOT EXISTS(SELECT 1 FROM qualifier_attempts WHERE seed_id = qualifier_seeds.id)"#)
            .bind(series).bind(event).bind(mode.id).bind(slot).bind(&mode.generator_profile).bind(&mode.settings_fingerprint).bind(retry)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

pub(crate) async fn sweep(state: Arc<GlobalState>) -> Result<(), Error> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM qualifier_seeds WHERE source = 'async_pool' AND (generation_state = 'pending' OR (generation_state = 'generating' AND generation_claim_until < NOW())) ORDER BY id")
        .fetch_all(&state.db_pool).await?;
    for id in ids {
        let mut tx = state.db_pool.begin().await?;
        sqlx::query("SELECT 1 FROM pooled_qualifier_configs config WHERE (series, event) = (SELECT series, event FROM qualifier_seeds WHERE id = $1) FOR UPDATE OF config")
            .bind(id).fetch_optional(&mut *tx).await?;
        // Unknown generator output stays unpublished. Explicit retry is required.
        sqlx::query("UPDATE qualifier_seeds SET generation_state = 'failed', generation_error = 'Generation worker expired; review unpublished output, then retry this slot.' WHERE id = $1 AND generation_state = 'generating' AND generation_claim_until < NOW()")
            .bind(id).execute(&mut *tx).await?;
        let claim = Uuid::new_v4().to_string();
        let input: Option<(String, serde_json::Value)> = sqlx::query_as(r#"UPDATE qualifier_seeds seed SET generation_state = 'generating', generation_claim = $2, generation_claim_until = NOW() + INTERVAL '2 hours'
            FROM qualifier_modes mode WHERE seed.id = $1 AND mode.id = seed.mode_id AND seed.generation_state = 'pending'
              AND mode.enabled AND mode.generator_profile = 'default' AND seed.retired_at IS NULL
              AND seed.settings_fingerprint = mode.settings_fingerprint
            RETURNING mode.seed_gen_type, mode.seed_config"#)
            .bind(id).bind(&claim).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        if let Some((kind, settings)) = input {
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    Duration::from_secs(5400),
                    generate(Arc::clone(&state), &kind, &settings),
                )
                .await
                .unwrap_or_else(|_| Err(Error::InvalidTransition("generator timed out".into())));
                let _ = complete(&state.db_pool, id, &claim, result).await;
            });
        }
    }
    Ok(())
}

async fn generate(
    state: Arc<GlobalState>,
    name: &str,
    config: &serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let kind = SeedGenType::from_db(Some(name), Some(config)).ok_or(Error::ModeUnavailable)?;
    let updates = roll(state, &kind)?;
    consume(&kind, updates).await
}

async fn consume(
    kind: &SeedGenType,
    mut updates: mpsc::Receiver<SeedRollUpdate>,
) -> Result<serde_json::Value, Error> {
    while let Some(update) = updates.recv().await {
        match update {
            SeedRollUpdate::Done {
                seed,
                resolved_randoms,
                ..
            } => {
                let mut data = seed.to_seed_data().ok_or(Error::NoSeed)?;
                if let Some(resolved) = resolved_randoms {
                    data["resolved_randoms"] = serde_json::json!(resolved);
                }
                validate_payload(kind, &data).await?;
                return Ok(data);
            }
            SeedRollUpdate::Error(error) => {
                return Err(Error::InvalidTransition(format!(
                    "generator failed: {error}"
                )));
            }
            _ => (),
        }
    }
    Err(Error::InvalidTransition(
        "generator ended without a result".into(),
    ))
}

pub(super) async fn complete(
    pool: &PgPool,
    id: i64,
    claim: &str,
    result: Result<serde_json::Value, Error>,
) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM pooled_qualifier_configs config WHERE (series, event) = (SELECT series, event FROM qualifier_seeds WHERE id = $1) FOR UPDATE OF config")
        .bind(id).fetch_optional(&mut *tx).await?;
    match result {
        Ok(data) => {
            let identity = physical_seed_identity(&data).ok_or(Error::NoSeed)?;
            let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qualifier_seeds WHERE physical_seed_identity = $2 AND (series, event) = (SELECT series, event FROM qualifier_seeds WHERE id = $1) AND id <> $1)")
                .bind(id).bind(&identity).fetch_one(&mut *tx).await?;
            if duplicate {
                sqlx::query("UPDATE qualifier_seeds SET generation_state = 'failed', generation_error = 'Generator returned a physical seed already used in this event' WHERE id = $1 AND generation_claim = $2 AND generation_state = 'generating'")
                    .bind(id).bind(claim).execute(&mut *tx).await?;
            } else {
                sqlx::query("UPDATE qualifier_seeds SET generation_state = 'ready', seed_data = $3, physical_seed_identity = $4, generated_at = NOW(), generation_error = NULL WHERE id = $1 AND generation_claim = $2 AND generation_state = 'generating' AND generation_claim_until > NOW() AND released_at IS NULL")
                    .bind(id).bind(claim).bind(data).bind(identity).execute(&mut *tx).await?;
            }
        }
        Err(error) => {
            sqlx::query("UPDATE qualifier_seeds SET generation_state = 'failed', generation_error = $3 WHERE id = $1 AND generation_claim = $2 AND generation_state = 'generating'")
                .bind(id).bind(claim).bind(error.to_string()).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generator_result_requires_matching_delivery_data() {
        let kind = SeedGenType::TWWR {
            permalink: "fixture-settings".into(),
        };
        let (sender, receiver) = mpsc::channel(2);
        let data = seed::Files::TwwrPermalink {
            permalink: "fixture-seed".into(),
            seed_hash: "fixture-hash".into(),
        }
        .to_seed_data_base();
        sender
            .send(SeedRollUpdate::Done {
                seed: seed::Data::from_seed_data_only(Some(data.clone()), None, false),
                rsl_preset: None,
                version: None,
                unlock_spoiler_log: UnlockSpoilerLog::Never,
                resolved_randoms: None,
            })
            .await
            .unwrap();
        drop(sender);
        assert_eq!(consume(&kind, receiver).await.unwrap(), data);
        assert!(validate_payload(&kind, &serde_json::json!({"type":"alttpr_owr","uuid":"00000000-0000-0000-0000-000000000001"})).await.is_err());
        let (sender, receiver) = mpsc::channel(1);
        drop(sender);
        assert!(consume(&kind, receiver).await.is_err());
    }

    #[tokio::test]
    async fn owr_rejects_missing_hash_icons() {
        let kind = SeedGenType::Owr {
            build: racetime_bot::seed_gen_type::OwrBuild::Tournament,
            config: serde_json::from_value(serde_json::json!({"base_settings":{}})).unwrap(),
        };
        assert!(validate_payload(&kind, &serde_json::json!({"type":"alttpr_owr","uuid":"00000000-0000-0000-0000-000000000001"})).await.is_err());
    }
}
