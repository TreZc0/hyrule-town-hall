use {
    async_proto::Protocol,
    serde::Deserialize,
};

// Note: We need to redefine SpoilerLog here instead of using ootr_utils::spoiler::SpoilerLog
// because our database migration changed file_hash from [HashIcon; 5] to [String; 5].
// The rust-ootr-utils version still uses the old enum type.
#[derive(Debug, Deserialize, Protocol)]
pub struct SpoilerLog {
    pub file_hash: [String; 5],
    pub password: Option<[ootr_utils::spoiler::OcarinaNote; 6]>,
    pub settings: Vec<ootr_utils::spoiler::Settings>,
}
