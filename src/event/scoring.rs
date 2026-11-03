use serde::{Deserialize, Serialize};
use std::{collections::HashMap, hash::Hash, time::Duration};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Rounding {
    #[default]
    None,
    Floor,
    Nearest,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Aggregation {
    #[default]
    Sum,
    Average,
    AverageCounted,
}

/// Parameters of the existing time-relative-to-par qualifier workflow.
/// DNF remains 0 and a completed run awaiting sufficient finishers remains -1.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ParScoreConfig {
    pub(crate) par_finishers: u8,
    pub(crate) required_finishes: usize,
    pub(crate) counted_attempts: usize,
    pub(crate) best_results: usize,
    pub(crate) scale: f64,
    pub(crate) offset: f64,
    pub(crate) minimum: f64,
    pub(crate) maximum: Option<f64>,
    pub(crate) rounding: Rounding,
    pub(crate) aggregation: Aggregation,
    pub(crate) extrapolation_score: f64,
    pub(crate) ineligible_players_count_towards_average: bool,
    pub(crate) best_result_is_scrapped: bool,
}

impl Default for ParScoreConfig {
    fn default() -> Self {
        Self {
            par_finishers: 3,
            required_finishes: 2,
            counted_attempts: 4,
            best_results: 2,
            scale: 1000.0,
            offset: 0.0,
            minimum: 100.0,
            maximum: None,
            rounding: Rounding::None,
            aggregation: Aggregation::Sum,
            extrapolation_score: 2000.0,
            ineligible_players_count_towards_average: true,
            best_result_is_scrapped: false,
        }
    }
}

impl ParScoreConfig {
    /// Defaults offered by the guided editor for newly selected time-relative scoring.
    pub(crate) fn guided_defaults() -> Self {
        Self {
            best_results: 4,
            maximum: Some(1100.0),
            aggregation: Aggregation::AverageCounted,
            extrapolation_score: 1100.0,
            ineligible_players_count_towards_average: false,
            ..Self::default()
        }
    }

    pub(crate) fn for_kind(
        kind: &str,
        config: Option<&serde_json::Value>,
    ) -> Result<Option<Self>, String> {
        let defaults = match kind {
            "twwr_main" | "time_relative" => Self::default(),
            "twwr_miniblins26" => Self {
                offset: 2000.0,
                rounding: Rounding::Floor,
                extrapolation_score: 5000.0,
                ..Self::default()
            },
            _ => {
                if config.is_some_and(|v| v.as_object().is_none_or(|o| !o.is_empty())) {
                    return Err(
                        "Scoring parameters require a time-relative scoring strategy.".into(),
                    );
                }
                return Ok(None);
            }
        };
        let mut merged = serde_json::to_value(defaults).map_err(|e| e.to_string())?;
        if let Some(config) = config {
            let overrides = config
                .as_object()
                .ok_or("Scoring configuration must be an object.")?;
            merged
                .as_object_mut()
                .expect("score config serializes as an object")
                .extend(overrides.clone());
        }
        let parsed: Self = serde_json::from_value(merged).map_err(|e| e.to_string())?;
        parsed.validate()?;
        Ok(Some(parsed))
    }

    pub(crate) fn validate(self) -> Result<(), String> {
        if self.par_finishers == 0
            || self.required_finishes == 0
            || self.counted_attempts == 0
            || self.required_finishes > self.counted_attempts
            || self.best_results == 0
            || self.best_results > self.counted_attempts
            || (self.best_result_is_scrapped && self.required_finishes < 2)
        {
            return Err("Scoring counts must be positive; min count and best results cannot exceed max count. Scrapping the best result requires a min count of at least 2.".into());
        }
        if ![
            self.scale,
            self.offset,
            self.minimum,
            self.extrapolation_score,
        ]
        .iter()
        .all(|n| n.is_finite())
            || self.scale <= 0.0
            || self.minimum <= 0.0
            || (self.maximum.is_none() && self.extrapolation_score < self.minimum)
            || self
                .maximum
                .is_some_and(|n| !n.is_finite() || n < self.minimum)
        {
            return Err(
                "Scoring values must be finite, with a positive scale/minimum and valid bounds."
                    .into(),
            );
        }
        Ok(())
    }

    pub(crate) fn points(self, finish: Duration, par: Duration) -> Option<f64> {
        if par.is_zero() {
            return None;
        }
        // Preserve operation and rounding order of the original TWWR formulas.
        let relative =
            (1.0 - (finish.as_secs_f64() - par.as_secs_f64()) / par.as_secs_f64()) * self.scale;
        let rounded = match self.rounding {
            Rounding::None => relative,
            Rounding::Floor => relative.floor(),
            Rounding::Nearest => relative.round(),
        };
        let score = (self.offset + rounded).max(self.minimum);
        Some(self.maximum.map_or(score, |limit| score.min(limit)))
    }

    pub(crate) fn aggregate(self, chronological_scores: &[f64]) -> f64 {
        let mut scores: Vec<_> = chronological_scores
            .iter()
            .take(self.counted_attempts)
            .copied()
            .filter(|s| match self.aggregation {
                Aggregation::AverageCounted => *s >= 0.0,
                Aggregation::Sum | Aggregation::Average => *s > 0.0,
            })
            .collect();
        scores.sort_by(|a, b| b.total_cmp(a));
        if self.best_result_is_scrapped && !scores.is_empty() {
            scores.remove(0);
        }
        scores.truncate(self.best_results);
        let sum: f64 = scores.iter().sum();
        match self.aggregation {
            Aggregation::Sum => sum,
            Aggregation::Average => sum / self.best_results as f64,
            Aggregation::AverageCounted => sum / scores.len().max(1) as f64,
        }
    }

    /// Establish eligibility across live and async attempts together, before selecting
    /// the fastest finishers in each round. The max-count attempt itself is eligible.
    pub(crate) fn eligible_par_times<P: Eq + Hash, T: Ord, R: Copy + Eq + Hash + Ord>(
        self,
        results: &[ParResult<P, T, R>],
    ) -> HashMap<R, Vec<Duration>> {
        let mut chronological: Vec<_> = results.iter().collect();
        chronological.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.round.cmp(&b.round)));
        let mut attempts = HashMap::<&P, usize>::new();
        let mut times = HashMap::<R, Vec<Duration>>::new();
        for result in chronological {
            let count = attempts.entry(&result.player).or_default();
            let eligible =
                self.ineligible_players_count_towards_average || *count < self.counted_attempts;
            *count += 1;
            let round_times = times.entry(result.round).or_default();
            if eligible && let Some(time) = result.finish {
                round_times.push(time);
            }
        }
        for times in times.values_mut() {
            times.sort_unstable();
        }
        times
    }
}

pub(crate) struct ParResult<P, T, R> {
    pub(crate) player: P,
    pub(crate) started_at: T,
    pub(crate) round: R,
    pub(crate) finish: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guided_defaults_and_legacy_settings_are_distinct() {
        let config = ParScoreConfig::guided_defaults();
        config.validate().unwrap();
        assert!(!config.ineligible_players_count_towards_average);
        assert!(!config.best_result_is_scrapped);
        assert_eq!(config.minimum, 100.0);
        assert_eq!(config.maximum, Some(1100.0));
        assert_eq!(config.aggregate(&[1000.0, 800.0, 600.0]), 800.0);

        let legacy = ParScoreConfig::for_kind("time_relative", Some(&serde_json::json!({})))
            .unwrap()
            .unwrap();
        assert!(legacy.ineligible_players_count_towards_average);
        assert_eq!(legacy.aggregate(&[1000.0, 800.0, 600.0]), 1800.0);
        let serialized = serde_json::to_value(config).unwrap();
        let reloaded = ParScoreConfig::for_kind("time_relative", Some(&serialized))
            .unwrap()
            .unwrap();
        assert_eq!(reloaded.aggregate(&[1000.0, 800.0, 600.0]), 800.0);
        assert!(!reloaded.ineligible_players_count_towards_average);
    }

    #[test]
    fn averages_best_counted_results_after_scrapping_one_best() {
        let config = ParScoreConfig {
            best_results: 2,
            ..ParScoreConfig::guided_defaults()
        };
        assert_eq!(config.aggregate(&[600.0, 1000.0, 800.0]), 900.0);
        let config = ParScoreConfig {
            best_result_is_scrapped: true,
            ..config
        };
        assert_eq!(config.aggregate(&[600.0, 1000.0, 800.0]), 700.0);
        // Tied best results discard exactly one result.
        assert_eq!(config.aggregate(&[1000.0, 1000.0, 600.0]), 800.0);
        let config = ParScoreConfig {
            required_finishes: 4,
            best_results: 4,
            ..config
        };
        assert_eq!(config.aggregate(&[600.0, 1000.0, 800.0, 700.0]), 700.0);
    }

    #[test]
    fn average_uses_actual_results_and_chronological_attempt_limit() {
        let config = ParScoreConfig::guided_defaults();
        assert_eq!(config.aggregate(&[800.0, 0.0, -1.0, 1000.0, 1100.0]), 600.0);
        assert_eq!(config.aggregate(&[800.0]), 800.0);
        assert_eq!(config.aggregate(&[]), 0.0);
        let config = ParScoreConfig {
            best_result_is_scrapped: true,
            ..config
        };
        assert_eq!(config.aggregate(&[800.0, 0.0, -1.0, 1000.0, 1100.0]), 400.0);
        assert_eq!(config.aggregate(&[800.0]), 0.0);
        assert_eq!(config.aggregate(&[-1.0, 0.0]), 0.0);
    }

    #[test]
    fn par_eligibility_combines_live_and_async_attempts_chronologically() {
        let result = |player, started_at, round, finish: Option<u64>| ParResult {
            player,
            started_at,
            round,
            finish: finish.map(Duration::from_secs),
        };
        // Intentionally shuffled: the second attempt is eligible, the third is not.
        // A forfeit uses an attempt, but does not supply a time for par.
        let results = [
            result("a", 30, (false, 2), Some(10)),
            result("b", 30, (false, 2), Some(60)),
            result("a", 10, (true, 1), None),
            result("a", 20, (false, 1), Some(20)),
            result("c", 30, (false, 2), Some(40)),
        ];
        let config = ParScoreConfig {
            counted_attempts: 2,
            best_results: 2,
            ..ParScoreConfig::guided_defaults()
        };
        let times = config.eligible_par_times(&results);
        assert_eq!(times[&(false, 1)], vec![Duration::from_secs(20)]);
        assert_eq!(
            times[&(false, 2)],
            vec![Duration::from_secs(40), Duration::from_secs(60)]
        );
        assert!(times[&(true, 1)].is_empty());
        let config = ParScoreConfig {
            ineligible_players_count_towards_average: true,
            ..config
        };
        assert_eq!(
            config.eligible_par_times(&results)[&(false, 2)],
            vec![
                Duration::from_secs(10),
                Duration::from_secs(40),
                Duration::from_secs(60)
            ]
        );
    }

    #[test]
    fn guided_formula_bounds_and_invalid_settings() {
        let config = ParScoreConfig::guided_defaults();
        let par = Duration::from_secs(100);
        assert_eq!(config.points(par, par), Some(1000.0));
        assert_eq!(config.points(Duration::from_secs(1), par), Some(1100.0));
        assert_eq!(config.points(Duration::from_secs(300), par), Some(100.0));
        let config = ParScoreConfig {
            minimum: 200.0,
            maximum: Some(1200.0),
            ..config
        };
        ParScoreConfig {
            minimum: 1200.0,
            maximum: Some(1500.0),
            ..config
        }
        .validate()
        .unwrap();
        assert_eq!(config.points(Duration::from_secs(1), par), Some(1200.0));
        assert_eq!(config.points(Duration::from_secs(300), par), Some(200.0));
        for invalid in [
            ParScoreConfig {
                required_finishes: 1,
                best_result_is_scrapped: true,
                ..config
            },
            ParScoreConfig {
                required_finishes: 5,
                ..config
            },
            ParScoreConfig {
                best_results: 0,
                ..config
            },
            ParScoreConfig {
                best_results: 5,
                ..config
            },
            ParScoreConfig {
                maximum: Some(50.0),
                ..config
            },
            ParScoreConfig {
                maximum: Some(f64::INFINITY),
                ..config
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn legacy_formulas_and_rounding_are_preserved_exactly() {
        for time in [1.0, 1200.25, 3600.0, 5000.123, 20000.0] {
            let finish = Duration::from_secs_f64(time);
            let par = Duration::from_secs(3600);
            let relative = (1.0 - (time - 3600.0) / 3600.0) * 1000.0;
            assert_eq!(
                ParScoreConfig::for_kind("twwr_main", None)
                    .unwrap()
                    .unwrap()
                    .points(finish, par),
                Some(relative.max(100.0))
            );
            assert_eq!(
                ParScoreConfig::for_kind("twwr_miniblins26", None)
                    .unwrap()
                    .unwrap()
                    .points(finish, par),
                Some((2000.0 + relative.floor()).max(100.0))
            );
        }
    }

    #[test]
    fn legacy_first_four_best_two_excludes_pending_and_dnf() {
        assert_eq!(
            ParScoreConfig::default().aggregate(&[800.0, 0.0, -1.0, 900.0, 2000.0]),
            1700.0
        );
    }

    #[test]
    fn new_event_can_use_best_three_of_six_without_new_strategy() {
        let config = ParScoreConfig::for_kind(
            "time_relative",
            Some(&serde_json::json!({
                "par_finishers": 4, "required_finishes": 3, "counted_attempts": 6, "best_results": 3
            })),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            config.aggregate(&[500.0, 800.0, 900.0, 0.0, -1.0, 700.0, 2000.0]),
            2400.0
        );
    }

    #[test]
    fn invalid_counts_and_zero_par_are_handled() {
        assert!(
            ParScoreConfig::for_kind(
                "time_relative",
                Some(&serde_json::json!({"par_finishers": 0}))
            )
            .is_err()
        );
        assert!(
            ParScoreConfig::for_kind(
                "time_relative",
                Some(&serde_json::json!({"best_results": 5}))
            )
            .is_err()
        );
        assert_eq!(
            ParScoreConfig::default().points(Duration::from_secs(10), Duration::ZERO),
            None
        );
    }
}
