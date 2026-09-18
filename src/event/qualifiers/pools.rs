use super::*;

#[derive(sqlx::FromRow)]
#[cfg_attr(test, derive(Default))]
pub(super) struct SeedRow {
    pub(super) id: i64,
    pub(super) live_race_id: Option<i64>,
    pub(super) live_round: Option<String>,
    pub(super) has_attempts: bool,
    pub(super) retired_at: Option<DateTime<Utc>>,
    pub(super) mode_id: i64,
    pub(super) pool_position: Option<i16>,
    pub(super) generation_state: String,
    pub(super) physical_seed_identity: Option<String>,
    pub(super) seed_data: Option<serde_json::Value>,
    pub(super) released_at: Option<DateTime<Utc>>,
    pub(super) generation_error: Option<String>,
}
#[derive(sqlx::FromRow)]
#[cfg_attr(test, derive(Default))]
pub(super) struct AttemptRow {
    pub(super) mode_id: i64,
    pub(super) seed_id: i64,
    pub(super) par_eligible: bool,
    pub(super) superseded_by: Option<i64>,
    pub(super) discord_thread: Option<i64>,
    pub(super) id: i64,
    pub(super) team_id: Option<i64>,
    pub(super) racetime_id: String,
    pub(super) signup_eligible: bool,
    pub(super) entrant_name: String,
    pub(super) mode_name: String,
    pub(super) source: String,
    pub(super) state: String,
    pub(super) counts_for_entrant: bool,
    pub(super) official_outcome: Option<String>,
    pub(super) official_time: Option<sqlx::postgres::types::PgInterval>,
    pub(super) vod: Option<String>,
    pub(super) retry_of: Option<i64>,
    pub(super) retry_declared_at: Option<DateTime<Utc>>,
    pub(super) retry_banned_at: Option<DateTime<Utc>>,
    pub(super) control_version: i64,
    pub(super) delivery_error: Option<String>,
    pub(super) correction_history: serde_json::Value,
}

pub(super) async fn load_seeds(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
) -> Result<Vec<SeedRow>, sqlx::Error> {
    sqlx::query_as::<_, SeedRow>(
        r#"SELECT seed.id, seed.live_race_id, race.round AS live_round, seed.retired_at, seed.mode_id, seed.pool_position,
            seed.generation_state, seed.physical_seed_identity, seed.seed_data, seed.released_at, seed.generation_error,
            EXISTS(SELECT 1 FROM qualifier_attempts WHERE seed_id = seed.id) AS has_attempts
            FROM qualifier_seeds seed LEFT JOIN races race ON race.id = seed.live_race_id
            WHERE seed.series = $1 AND seed.event = $2
            ORDER BY seed.mode_id, seed.source, seed.pool_position, seed.live_race_id"#,
    )
    .bind(series)
    .bind(event)
    .fetch_all(&mut **transaction)
    .await
}

pub(super) async fn load_attempts(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
) -> Result<Vec<AttemptRow>, sqlx::Error> {
    sqlx::query_as::<_, AttemptRow>(
            r#"SELECT attempt.id, attempt.mode_id, attempt.seed_id, attempt.par_eligible, attempt.superseded_by, attempt.discord_thread, attempt.racetime_id,
                (SELECT teams.id FROM teams WHERE teams.series=attempt.series AND teams.event=attempt.event
                    AND NOT teams.resigned AND qualifier_runner_team(attempt.series, attempt.event, attempt.racetime_id, teams.id)
                    AND NOT EXISTS (SELECT 1 FROM team_members WHERE team=teams.id AND status='unconfirmed')
                    LIMIT 1) AS team_id,
                qualifier_signup_eligible(attempt.series, attempt.event, attempt.racetime_id) AS signup_eligible,
                COALESCE(user_account.discord_display_name, user_account.racetime_display_name,
                    attempt.allocation_metadata->>'racetime_name', attempt.racetime_id) AS entrant_name,
                mode.display_name AS mode_name, attempt.source, attempt.state,
                attempt.counts_for_entrant, attempt.official_outcome,
                attempt.official_time, attempt.vod, attempt.retry_of, attempt.retry_declared_at, attempt.retry_banned_at, attempt.control_version, attempt.delivery_error, attempt.correction_history
            FROM qualifier_attempts attempt
            JOIN qualifier_modes mode ON mode.id = attempt.mode_id
            LEFT JOIN users user_account ON user_account.racetime_id = attempt.racetime_id
            WHERE attempt.series = $1 AND attempt.event = $2
            ORDER BY attempt.requested_at, attempt.id"#,
        )
        .bind(series)
        .bind(event)
        .fetch_all(&mut **transaction)
        .await
}

pub(super) fn seed_label(id: i64, seeds: &[SeedRow]) -> String {
    seeds.iter().find(|seed| seed.id == id).map_or_else(
        || format!("Seed {id}"),
        |seed| match (seed.pool_position, seed.live_race_id) {
            (Some(slot), _) => format!("Async slot {slot} (seed {id})"),
            (_, Some(_)) => format!("{} (seed {id})", seed.live_round.as_deref().filter(|round| !round.trim().is_empty()).unwrap_or("Live qualifier")),
            _ => format!("Seed {id}"),
        },
    )
}

fn seed_metadata(seed: &SeedRow) -> RawHtml<String> {
    let Some(payload) = seed.seed_data.as_ref() else {
        return html! { : "—"; };
    };
    let data = seed::Data::from_seed_data_only(Some(payload.clone()), None, false);
    let hash = data.file_hash.as_ref().map(|hash| hash.join(" · ")).or_else(|| {
        match data.files() {
            Some(seed::Files::AvianartSeed { seed_hash: Some(hash), .. }) => Some(hash.join(" · ")),
            Some(seed::Files::TwwrPermalink { seed_hash, .. }) => Some(seed_hash),
            _ => None,
        }
    });
    let uuid = payload.get("uuid").and_then(serde_json::Value::as_str);
    let id = format!("pool-seed-meta-{}", seed.id);
    html! {
        button(type = "button", class = "settings-link setting-help-trigger seed-meta-trigger",
            popovertarget = &id, aria_controls = &id, aria_expanded = "false",
            data_hover_help = "true", aria_label = format!("Seed metadata for seed {}", seed.id)) : "Seed meta";
        div(id = &id, class = "setting-help seed-meta-panel", popover = "auto",
            role = "dialog", aria_labelledby = format!("{id}-title")) {
            div(class = "setting-help-header") {
                h4(id = format!("{id}-title")) : format!("Seed {} metadata", seed.id);
                button(type = "button", class = "setting-help-close", popovertarget = &id,
                    popovertargetaction = "hide", aria_label = "Close seed metadata") : "×";
            }
            div(class = "setting-help-body") {
                p {
                    strong : if uuid.is_some() { "UUID: " } else { "Seed ID: " };
                    code : uuid.or(seed.physical_seed_identity.as_deref()).unwrap_or("Not available");
                }
                p { strong : "Seed hash: "; : hash.as_deref().unwrap_or("Not available"); }
            }
        }
    }
}

impl AttemptRow {
    fn outcome(&self) -> Option<pooled_qualifiers::Outcome> {
        use pooled_qualifiers::Outcome;
        if self.state != "finalized" {
            return None;
        }
        Some(match self.official_outcome.as_deref()? {
            "finished" => Outcome::Finished(
                pooled_qualifiers::pg_interval_duration(self.official_time.as_ref()?)
                    .to_std()
                    .ok()?,
            ),
            "forfeit" => Outcome::Forfeit,
            "dq" => Outcome::Dq,
            "invalid" => Outcome::Invalid,
            _ => return None,
        })
    }
}

#[derive(Debug)]
struct Population {
    assigned: usize,
    counted: usize,
    active: usize,
    awaiting: usize,
    finalized: usize,
    forfeited: usize,
    eligible_finishes: usize,
    par: Option<f64>,
}

fn population(config: &pooled_qualifiers::Config, attempts: &[&AttemptRow]) -> Population {
    let finishes: Vec<_> = attempts
        .iter()
        .filter(|attempt| attempt.par_eligible && attempt.signup_eligible)
        .filter_map(|attempt| match attempt.outcome() {
            Some(pooled_qualifiers::Outcome::Finished(time)) => Some(time),
            _ => None,
        })
        .collect();
    Population {
        assigned: attempts
            .iter()
            .filter(|attempt| attempt.state != "void")
            .count(),
        counted: attempts
            .iter()
            .filter(|attempt| {
                attempt.state != "void" && attempt.counts_for_entrant && attempt.signup_eligible
            })
            .count(),
        active: attempts
            .iter()
            .filter(|attempt| {
                matches!(
                    attempt.state.as_str(),
                    "assigned" | "revealed" | "starting" | "running"
                )
            })
            .count(),
        awaiting: attempts
            .iter()
            .filter(|attempt| attempt.state == "awaiting_verification")
            .count(),
        finalized: attempts
            .iter()
            .filter(|attempt| attempt.state == "finalized")
            .count(),
        forfeited: attempts
            .iter()
            .filter(|attempt| {
                matches!(attempt.outcome(), Some(pooled_qualifiers::Outcome::Forfeit))
            })
            .count(),
        eligible_finishes: finishes.len(),
        par: pooled_qualifiers::seed_par(config, finishes),
    }
}

pub(super) fn overview(
    config: &pooled_qualifiers::Config,
    mode_id: i64,
    seeds: &[SeedRow],
    attempts: &[AttemptRow],
    guild: Option<u64>,
) -> RawHtml<String> {
    let groups = seeds
        .iter()
        .filter(|seed| seed.mode_id == mode_id)
        .map(|seed| {
            let assigned = attempts
                .iter()
                .filter(|attempt| attempt.seed_id == seed.id)
                .collect_vec();
            let stats = population(config, &assigned);
            (seed, assigned, stats)
        })
        .collect_vec();
    html! {
        div(class = "qualifier-pool-table-scroll") {
            table(class = "qualifier-pool-table") {
                thead { tr {
                    th : "Seed"; th : "Metadata"; th : "State"; th : "Assigned"; th : "Counted";
                    th : "Active"; th : "Awaiting verification"; th : "Finalized"; th : "Forfeits";
                    th : "Eligible finishes / par";
                } }
                tbody {
                    @for (seed, _, stats) in &groups {
                        tr {
                            td { a(href = format!("#pool-seed-{}", seed.id)) : seed_label(seed.id, seeds); }
                            td : seed_metadata(seed);
                            td {
                                : &seed.generation_state;
                                @if seed.retired_at.is_some() { : " (retired)"; }
                                @if let Some(error) = &seed.generation_error { p : error; }
                            }
                            td : stats.assigned;
                            td : stats.counted;
                            td : stats.active;
                            td : stats.awaiting;
                            td : stats.finalized;
                            td : stats.forfeited;
                            td {
                                : format!("{} eligible finishes", stats.eligible_finishes);
                                @if let Some(par) = stats.par {
                                    : format!(" · Par: {} (fastest {})", English.format_duration(Duration::from_secs_f64(par), false), config.par_finishers.min(stats.eligible_finishes as i16));
                                } else {
                                    : format!(" · {}/{} finishes — par pending", stats.eligible_finishes, config.par_finishers);
                                }
                            }
                        }
                    }
                }
            }
        }
        @for (seed, assigned, stats) in &groups {
            details(id = format!("pool-seed-{}", seed.id), style = "text-align: left; overflow-wrap: anywhere; margin: 1rem 0;") {
                summary : format!("{} — entrant history ({} attempts)", seed_label(seed.id, seeds), assigned.len());
                p : format!("{} assignments; {} currently counted; {} eligible finishes", stats.assigned, stats.counted, stats.eligible_finishes);
                @if stats.par.is_none() { p : format!("{}/{} finishes — par pending", stats.eligible_finishes, config.par_finishers); }
                p { : "Identity: "; : seed.physical_seed_identity.as_deref().unwrap_or("not generated"); }
                @if let Some(released) = seed.released_at { p : format!("Released: {}", released.to_rfc3339()); }
                @if assigned.is_empty() { p : "No entrants assigned."; }
                @for attempt in assigned {
                    section {
                        h5 { : &attempt.entrant_name; : format!(" (attempt {})", attempt.id); }
                        p {
                            : format!("{}; {}", attempt.source, attempt.state);
                            @if attempt.state == "void" { : "; void — excluded from population and scoring"; }
                            else if !attempt.signup_eligible { : "; excluded from scoring: no active signup at the required time"; }
                            else if attempt.counts_for_entrant { : "; counted for entrant"; }
                            else { : "; not counted for entrant"; }
                            @if let Some(original) = attempt.retry_of {
                                : "; replacement of "; a(href = format!("#attempt-{original}")) : format!("attempt {original}");
                            } else { : "; original attempt"; }
                            @if let Some(replacement) = attempt.superseded_by {
                                : "; replaced by "; a(href = format!("#attempt-{replacement}")) : format!("attempt {replacement}");
                            }
                        }
                        @if let Some(outcome) = attempt.outcome() {
                            p {
                                @match outcome {
                                    pooled_qualifiers::Outcome::Finished(time) => { : English.format_duration(time, false); }
                                    pooled_qualifiers::Outcome::Forfeit => { : "Forfeit"; }
                                    pooled_qualifiers::Outcome::Dq => { : "Disqualified"; }
                                    pooled_qualifiers::Outcome::Invalid => { : "Invalid result"; }
                                }
                                : " — ";
                                @if !attempt.signup_eligible { : "excluded from scoring"; }
                                else { @match pooled_qualifiers::performance_score(config, outcome, stats.par) {
                                    pooled_qualifiers::ModeScore::Pending => { : "points pending"; }
                                    pooled_qualifiers::ModeScore::Score(points) => { : format!("{points:.2} points"); }
                                }
                                }
                                @if matches!(outcome, pooled_qualifiers::Outcome::Finished(_)) && attempt.par_eligible && attempt.signup_eligible { : "; eligible for par"; }
                                else { : "; excluded from par"; }
                            }
                        }
                        p {
                            a(href = format!("#attempt-{}", attempt.id)) : "Review result / history";
                            @if let (Some(guild), Some(thread)) = (guild, attempt.discord_thread) {
                                : " · ";
                                a(href = format!("https://discord.com/channels/{guild}/{}", thread as u64)) : "Private async thread";
                            }
                            @if let Some(vod) = &attempt.vod { : " · "; a(href = vod) : "VOD"; }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finish(id: i64, minutes: i64) -> AttemptRow {
        AttemptRow {
            id,
            seed_id: 1,
            entrant_name: format!("Entrant <{id}>"),
            source: "async".into(),
            state: "finalized".into(),
            counts_for_entrant: true,
            signup_eligible: true,
            par_eligible: true,
            official_outcome: Some("finished".into()),
            official_time: Some(sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: minutes * 60_000_000,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn final_pool_summary_excludes_unsigned_finishers() {
        let mut config = pooled_qualifiers::tests::config();
        config.submissions_close_at = Some(Utc::now() - chrono::Duration::hours(1));
        let mut attempts = vec![finish(1, 30), finish(2, 50), finish(3, 60)];
        attempts[0].signup_eligible = false;
        let stats = population(&config, &attempts.iter().collect::<Vec<_>>());
        assert_eq!(stats.assigned, 3);
        assert_eq!(stats.counted, 2);
        assert_eq!(stats.eligible_finishes, 2);
        assert_eq!(stats.par, Some(55.0 * 60.0));
    }

    #[test]
    fn pool_population_and_par_preserve_replaced_finishes_and_exclude_voids_and_dqs() {
        let config = pooled_qualifiers::tests::config();
        let mut attempts: Vec<_> = [50, 55, 60, 65, 70, 100]
            .into_iter()
            .enumerate()
            .map(|(id, minutes)| finish(id as i64 + 1, minutes))
            .collect();
        attempts[0].counts_for_entrant = false;
        attempts[0].superseded_by = Some(7);
        attempts[0].discord_thread = Some(123);
        attempts.push(AttemptRow {
            id: 7,
            seed_id: 1,
            retry_of: Some(1),
            state: "running".into(),
            counts_for_entrant: true,
            signup_eligible: true,
            ..Default::default()
        });
        attempts.push(AttemptRow {
            id: 8,
            seed_id: 1,
            state: "awaiting_verification".into(),
            counts_for_entrant: true,
            signup_eligible: true,
            ..Default::default()
        });
        let mut void = finish(9, 1);
        void.state = "void".into();
        void.counts_for_entrant = false;
        attempts.push(void);
        let mut dq = finish(10, 1);
        dq.official_outcome = Some("dq".into());
        dq.par_eligible = false;
        attempts.push(dq);
        let mut forfeit = finish(11, 1);
        forfeit.official_outcome = Some("forfeit".into());
        forfeit.par_eligible = false;
        attempts.push(forfeit);
        let stats = population(&config, &attempts.iter().collect_vec());
        assert_eq!(
            (
                stats.assigned,
                stats.counted,
                stats.active,
                stats.awaiting,
                stats.finalized
            ),
            (10, 9, 1, 1, 8)
        );
        assert_eq!(stats.forfeited, 1);
        assert_eq!(stats.eligible_finishes, 6);
        assert_eq!(stats.par, Some(3600.0));
        assert_eq!(
            population(&config, &attempts[..4].iter().collect_vec()).par,
            None
        );
        let seeds = vec![
            SeedRow {
                id: 1,
                mode_id: 1,
                pool_position: Some(2),
                generation_state: "ready".into(),
                seed_data: Some(json!({
                    "type": "alttpr_owr", "uuid": "00000000-0000-0000-0000-000000000001",
                    "hash1": "Bow", "hash2": "Hookshot", "hash3": "Boots",
                    "hash4": "Hammer", "hash5": "Mirror",
                })),
                ..Default::default()
            },
            SeedRow {
                id: 2,
                mode_id: 1,
                live_race_id: Some(42),
                live_round: Some("Live qualifier <1>".into()),
                generation_state: "ready".into(),
                ..Default::default()
            },
        ];
        let html = overview(&config, 1, &seeds, &attempts, Some(456)).0;
        for text in [
            "Async slot 2",
            "Live qualifier &lt;1&gt;",
            "Entrant &lt;1&gt;",
            "105.00 points",
            "eligible for par",
            "#attempt-7",
            "Seed meta",
            "00000000-0000-0000-0000-000000000001",
            "Bow · Hookshot · Boots · Hammer · Mirror",
        ] {
            assert!(html.contains(text), "missing {text}");
        }
        assert!(html.contains("https://discord.com/channels/456/123"));
        assert!(html.contains("0/5 finishes — par pending"));
        assert!(html.contains("void — excluded"));
        assert!(
            !overview(&config, 2, &seeds, &attempts, Some(456))
                .0
                .contains("Entrant &lt;")
        );
        if let Ok(path) = std::env::var("HTH_POOL_BROWSER_FIXTURE") {
            let css = include_str!("../../../assets/static/common.css");
            let script = include_str!("../../../assets/static/setting-help.js");
            std::fs::write(path, format!("<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>{css}</style></head><body><main><article class=\"qualifier-admin\"><h1>Qualifier Seed Pools</h1>{html}</article><div id=\"attempt-7\">Review attempt 7</div></main><script>{script}</script></body></html>")).unwrap();
        }
    }
}
