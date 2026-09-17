use super::*;

pub(super) async fn load(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    config: &pooled_qualifiers::Config,
    modes: &[pooled_qualifiers::Mode],
    attempts: &[pools::AttemptRow],
) -> Result<RawHtml<String>, sqlx::Error> {
    let entrants: Vec<(i64, String)> = sqlx::query_as(
        r#"SELECT team.id, COALESCE(team.name, account.discord_display_name,
            account.racetime_display_name, 'Team ' || team.id::TEXT)
        FROM teams team
        LEFT JOIN team_members member ON member.team = team.id
        LEFT JOIN users account ON account.id = member.member
        WHERE team.series = $1 AND team.event = $2 AND NOT team.resigned
            AND NOT EXISTS (SELECT 1 FROM team_members unconfirmed
                WHERE unconfirmed.team = team.id AND unconfirmed.status = 'unconfirmed')"#,
    )
    .bind(series)
    .bind(event)
    .fetch_all(&mut **transaction)
    .await?;
    let scores = pooled_qualifiers::standings(transaction, config).await?;
    Ok(render(entrants, scores, modes, attempts))
}

fn render(
    mut entrants: Vec<(i64, String)>,
    scores: Vec<pooled_qualifiers::Standing>,
    modes: &[pooled_qualifiers::Mode],
    attempts: &[pools::AttemptRow],
) -> RawHtml<String> {
    let scores: HashMap<_, _> = scores
        .into_iter()
        .map(|score| (score.team_id, score))
        .collect();
    let average = |team| scores.get(&team).and_then(|score| score.average);
    entrants.sort_by(|(left, left_name), (right, right_name)| {
        match (average(*left), average(*right)) {
            (Some(left), Some(right)) => right.total_cmp(&left),
            (Some(_), None) => Less,
            (None, Some(_)) => Greater,
            (None, None) => Equal,
        }
        .then_with(|| left_name.cmp(right_name))
        .then_with(|| left.cmp(right))
    });
    html! {
        section(id = "pooled-standings", class = "qualifier-section") {
            h2 : "Qualifier standings";
            p(class = "qualifier-intro") : "Organizer view of reviewed results, including scores hidden from entrants. Scores remain pending until a seed has enough finishes to establish par. The average is available after every required mode has a score.";
            @if entrants.is_empty() {
                p : "No confirmed entrants yet.";
            } else {
                table {
                    thead { tr {
                        th : "Rank";
                        th : "Entrant";
                        @for mode in modes.iter().filter(|mode| mode.enabled) { th : &mode.display_name; }
                        th : "Forfeits";
                        th : "Average";
                    } }
                    tbody {
                        @for (team_id, name) in &entrants {
                            tr {
                                td : average(*team_id).map(|score| (1 + entrants.iter().take_while(|(other, _)| average(*other) != Some(score)).count()).to_string()).unwrap_or_else(|| "—".into());
                                td : name;
                                @for mode in modes.iter().filter(|mode| mode.enabled) {
                                    @let outcome = attempts.iter().find(|attempt| attempt.team_id == *team_id && attempt.mode_id == mode.id && attempt.counts_for_entrant && attempt.state == "finalized").and_then(|attempt| attempt.official_outcome.as_deref());
                                    td : match outcome {
                                        Some("forfeit") => "Forfeit (0.00)".into(),
                                        Some("dq") => "Disqualified (0.00)".into(),
                                        Some("invalid") => "Invalid (0.00)".into(),
                                        _ => match scores.get(team_id).and_then(|standing| standing.mode_scores.iter().find(|(position, _, _)| *position == mode.position)).map(|(_, score, _)| score) {
                                            Some(pooled_qualifiers::ModeScore::Score(score)) => format!("{score:.2}"),
                                            _ => "Pending".into(),
                                        },
                                    };
                                }
                                td : attempts.iter().filter(|attempt| attempt.team_id == *team_id && attempt.counts_for_entrant && attempt.state == "finalized" && attempt.official_outcome.as_deref() == Some("forfeit")).count();
                                td : average(*team_id).map(|score| format!("{score:.2}")).unwrap_or_else(|| "Pending".into());
                            }
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
    use kuchiki::traits::TendrilSink as _;

    #[test]
    fn forfeits_are_distinct_from_zero_scores_and_only_count_current_results() {
        let mode = pooled_qualifiers::Mode {
            id: 1,
            position: 1,
            display_name: "Open".into(),
            seed_gen_type: "owr".into(),
            seed_config: json!({}),
            generator_profile: "default".into(),
            settings_fingerprint: String::new(),
            enabled: true,
        };
        let mut attempts = Vec::new();
        for (team_id, outcome, state, counted) in [
            (1, "forfeit", "finalized", true),
            (1, "forfeit", "finalized", false),
            (1, "forfeit", "void", true),
            (1, "forfeit", "awaiting_verification", true),
            (2, "dq", "finalized", true),
            (3, "finished", "finalized", true),
            (4, "invalid", "finalized", true),
        ] {
            attempts.push(pools::AttemptRow {
                team_id,
                mode_id: 1,
                official_outcome: Some(outcome.into()),
                state: state.into(),
                counts_for_entrant: counted,
                ..Default::default()
            });
        }
        let scores = (1..=4)
            .map(|team_id| pooled_qualifiers::Standing {
                team_id,
                average: Some(0.0),
                entered: 1,
                finished: 1,
                forfeited: 0,
                mode_scores: vec![(1, pooled_qualifiers::ModeScore::Score(0.0), true)],
            })
            .collect();
        let html = render(
            (1..=4)
                .map(|team| (team, format!("Entrant {team}")))
                .collect(),
            scores,
            &[mode],
            &attempts,
        );
        let document = kuchiki::parse_html().one(html.0);
        let rows: Vec<_> = document
            .select("tbody tr")
            .unwrap()
            .map(|row| {
                row.as_node()
                    .select("td")
                    .unwrap()
                    .map(|cell| cell.text_contents())
                    .collect::<Vec<_>>()
            })
            .collect();
        for (row, result, count) in [
            (&rows[0], "Forfeit (0.00)", "1"),
            (&rows[1], "Disqualified (0.00)", "0"),
            (&rows[2], "0.00", "0"),
            (&rows[3], "Invalid (0.00)", "0"),
        ] {
            assert_eq!(row[2], result);
            assert_eq!(row[3], count);
        }
    }

    #[test]
    fn standings_rank_ties_and_keep_pending_entrants_unranked() {
        let score = |team_id, average| pooled_qualifiers::Standing {
            team_id,
            average,
            entered: 1,
            finished: 1,
            forfeited: 0,
            mode_scores: Vec::new(),
        };
        let html = render(
            vec![
                (1, "Pending".into()),
                (2, "Zero".into()),
                (3, "Winner B".into()),
                (4, "Winner A".into()),
            ],
            vec![
                score(2, Some(0.0)),
                score(3, Some(100.0)),
                score(4, Some(100.0)),
            ],
            &[],
            &[],
        );
        let document = kuchiki::parse_html().one(html.0);
        let rows: Vec<Vec<String>> = document
            .select("tbody tr")
            .unwrap()
            .map(|row| {
                row.as_node()
                    .select("td")
                    .unwrap()
                    .map(|cell| cell.text_contents())
                    .collect()
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                vec!["1", "Winner A", "0", "100.00"],
                vec!["1", "Winner B", "0", "100.00"],
                vec!["3", "Zero", "0", "0.00"],
                vec!["—", "Pending", "0", "Pending"],
            ]
        );
    }
}
