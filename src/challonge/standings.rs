use std::collections::HashMap;
use crate::prelude::*;
use super::client::Error;
use super::import::{fetch_participants, fetch_matches};

/// Calculate Swiss standings from Challonge tournament data.
pub(crate) async fn swiss_standings(
    http_client: &reqwest::Client,
    config: &Config,
    community: Option<&str>,
    tournament: &str,
) -> Result<Vec<startgg::SwissStanding>, Error> {
    let participants = fetch_participants(http_client, config, community, tournament).await?;
    let matches = fetch_matches(http_client, config, community, tournament, Some("complete")).await?;

    // Build participant ID -> name map and win/loss records
    let mut records: HashMap<&str, (usize, usize)> = participants.iter()
        .map(|p| (p.id.as_str(), (0, 0)))
        .collect();

    for m in &matches {
        let Some(winner_id) = m.attributes.as_ref().and_then(|a| a.winner_id) else { continue };
        let winner_id_str = winner_id.to_string();
        let p1 = m.relationships.player1.as_ref().map(|p| p.data.id.as_str());
        let p2 = m.relationships.player2.as_ref().map(|p| p.data.id.as_str());
        match (p1, p2) {
            (Some(p1), Some(p2)) => {
                // Normal match: credit win and loss
                let loser_id = if p1 == winner_id_str { p2 } else { p1 };
                if let Some(r) = records.get_mut(winner_id_str.as_str()) { r.0 += 1; }
                if let Some(r) = records.get_mut(loser_id) { r.1 += 1; }
            }
            _ => {
                // Bye: one player is null, credit the win only
                if let Some(r) = records.get_mut(winner_id_str.as_str()) { r.0 += 1; }
            }
        }
    }

    let mut standings: Vec<startgg::SwissStanding> = participants.iter()
        .map(|p| {
            let (wins, losses) = records.get(p.id.as_str()).copied().unwrap_or((0, 0));
            startgg::SwissStanding {
                placement: 0,
                name: p.attributes.name.clone(),
                wins,
                losses,
            }
        })
        .collect();

    // Sort: wins desc, losses asc, name asc
    standings.sort_by(|a, b| {
        b.wins.cmp(&a.wins)
            .then(a.losses.cmp(&b.losses))
            .then(a.name.cmp(&b.name))
    });

    // Assign placements with ties
    let mut last_record: Option<(usize, usize)> = None;
    let mut placement = 1;
    for (i, s) in standings.iter_mut().enumerate() {
        let current = (s.wins, s.losses);
        if last_record != Some(current) {
            placement = i + 1;
            last_record = Some(current);
        }
        s.placement = placement;
    }

    Ok(standings)
}
