use serde::{Deserialize, Serialize};
use std::time::Duration;

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
        }
    }
}

impl ParScoreConfig {
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
            || self.best_results == 0
            || self.required_finishes > self.counted_attempts
            || self.best_results > self.counted_attempts
        {
            return Err("Scoring counts must be positive; required finishes and best results cannot exceed counted attempts.".into());
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
            || self.extrapolation_score < self.minimum
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
            .filter(|s| *s > 0.0)
            .collect();
        scores.sort_by(|a, b| b.total_cmp(a));
        scores.truncate(self.best_results);
        let sum: f64 = scores.iter().sum();
        match self.aggregation {
            Aggregation::Sum => sum,
            Aggregation::Average => sum / self.best_results as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
