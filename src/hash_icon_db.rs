use {
    serde::{Deserialize, Serialize},
    sqlx::{Transaction, Postgres},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HashIconData {
    pub id: i32,
    pub name: String,
    pub game_id: i32,
    pub file_name: String,
    pub racetime_emoji: String,
}

impl HashIconData {
    /// Get hash icon by name for a specific game
    pub async fn by_name(
        transaction: &mut Transaction<'_, Postgres>,
        game_id: i32,
        name: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            HashIconData,
            r#"
                SELECT id, name, game_id as "game_id!", file_name, racetime_emoji
                FROM hash_icons
                WHERE game_id = $1 AND name = $2
            "#,
            game_id,
            name
        )
        .fetch_optional(&mut **transaction)
        .await
    }
} 