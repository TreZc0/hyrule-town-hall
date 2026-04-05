use serde::Deserialize;

/// Pagination links returned by Challonge list endpoints
#[derive(Debug, Deserialize)]
pub(crate) struct PaginationLinks {
    pub(crate) next: Option<url::Url>,
}

// === Matches ===

#[derive(Debug, Deserialize)]
pub(crate) struct MatchesResponse {
    pub(crate) data: Vec<Match>,
    pub(crate) links: PaginationLinks,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Match {
    pub(crate) id: String,
    pub(crate) attributes: Option<MatchAttributes>,
    pub(crate) relationships: MatchRelationships,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MatchAttributes {
    /// "pending", "open", "complete"
    pub(crate) state: String,
    /// Round number (1, 2, 3...; negative for losers bracket)
    pub(crate) round: i32,
    /// Match identifier letter (A, B, C...)
    pub(crate) identifier: String,
    pub(crate) suggested_play_order: Option<i32>,
    /// Winner's participant ID (set when state=complete)
    pub(crate) winner_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MatchRelationships {
    pub(crate) player1: Option<PlayerRelation>,
    pub(crate) player2: Option<PlayerRelation>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PlayerRelation {
    pub(crate) data: PlayerData,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PlayerData {
    pub(crate) id: String,
}

// === Participants ===

#[derive(Debug, Deserialize)]
pub(crate) struct ParticipantsResponse {
    pub(crate) data: Vec<Participant>,
    pub(crate) links: PaginationLinks,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Participant {
    pub(crate) id: String,
    pub(crate) attributes: ParticipantAttributes,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ParticipantAttributes {
    pub(crate) name: String,
    pub(crate) seed: Option<i32>,
    pub(crate) group_id: Option<i32>,
    pub(crate) tournament_id: i64,
    /// Challonge username if the participant is linked to an account
    pub(crate) username: Option<String>,
    pub(crate) final_rank: Option<i32>,
}
