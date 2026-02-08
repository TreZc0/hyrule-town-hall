use crate::prelude::*;
use crate::series::Series;

#[derive(Debug, thiserror::Error)]
pub(crate) enum GameError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub(crate) struct Game {
    pub(crate) id: i32,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) description: Option<String>,
    #[allow(dead_code)]
    pub(crate) created_at: DateTime<Utc>,
    #[allow(dead_code)]
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are used in database queries, not read back from results
pub(crate) struct GameRacetimeConnection {
    pub(crate) id: i32,
    pub(crate) game_id: i32,
    pub(crate) category_slug: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

impl Game {
    pub(crate) async fn all(transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<Self>, GameError> {
        let rows = sqlx::query!(
            r#"SELECT id, name, display_name, description, created_at, updated_at 
               FROM games ORDER BY display_name"#
        )
        .fetch_all(&mut **transaction)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Game {
                id: row.id,
                name: row.name,
                display_name: row.display_name,
                description: row.description,
                created_at: row.created_at.expect("created_at should not be null"),
                updated_at: row.updated_at.expect("updated_at should not be null"),
            })
            .collect())
    }

    pub(crate) async fn from_name(transaction: &mut Transaction<'_, Postgres>, name: &str) -> Result<Option<Self>, GameError> {
        let row = sqlx::query!(
            r#"SELECT id, name, display_name, description, created_at, updated_at 
               FROM games WHERE name = $1"#,
            name
        )
        .fetch_optional(&mut **transaction)
        .await?;

        Ok(row.map(|row| Game {
            id: row.id,
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            created_at: row.created_at.expect("created_at should not be null"),
            updated_at: row.updated_at.expect("updated_at should not be null"),
        }))
    }

    pub(crate) async fn series(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<Series>, GameError> {
        let rows = sqlx::query!(
            r#"SELECT DISTINCT series AS "series: Series" 
               FROM game_series WHERE game_id = $1 ORDER BY series"#,
            self.id
        )
        .fetch_all(&mut **transaction)
        .await?;

        Ok(rows.into_iter().map(|row| row.series).collect())
    }

    pub(crate) async fn from_series(transaction: &mut Transaction<'_, Postgres>, series: Series) -> Result<Option<Self>, GameError> {
        let row = sqlx::query!(
            r#"SELECT g.id, g.name, g.display_name, g.description, g.created_at, g.updated_at 
               FROM games g 
               JOIN game_series gs ON g.id = gs.game_id 
               WHERE gs.series = $1"#,
            series as _
        )
        .fetch_optional(&mut **transaction)
        .await?;

        Ok(row.map(|row| Game {
            id: row.id,
            name: row.name,
            display_name: row.display_name,
            description: row.description,
            created_at: row.created_at.expect("created_at should not be null"),
            updated_at: row.updated_at.expect("updated_at should not be null"),
        }))
    }

    pub(crate) async fn admins(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<User>, GameError> {
        let admin_ids = sqlx::query_scalar!(
            r#"SELECT admin_id FROM game_admins WHERE game_id = $1 ORDER BY admin_id"#,
            self.id
        )
        .fetch_all(&mut **transaction)
        .await?;

        let mut admins = Vec::new();
        for admin_id in admin_ids {
            if let Some(admin_id) = admin_id {
                if let Some(user) = User::from_id(&mut **transaction, Id::<Users>::from(admin_id as i64)).await? {
                    admins.push(user);
                }
            }
        }

        Ok(admins)
    }

    pub(crate) async fn is_admin(&self, transaction: &mut Transaction<'_, Postgres>, user: &User) -> Result<bool, GameError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM game_admins WHERE game_id = $1 AND admin_id = $2"#,
            self.id,
            i64::from(user.id)
        )
        .fetch_one(&mut **transaction)
        .await?;

        Ok(count.unwrap_or(0) > 0)
    }

    pub(crate) async fn notification_channel(&self, transaction: &mut Transaction<'_, Postgres>, language: Language) -> Result<Option<(GuildId, ChannelId)>, GameError> {
        let row = sqlx::query!(
            r#"SELECT guild_id, channel_id FROM game_notification_channels WHERE game_id = $1 AND language = $2"#,
            self.id,
            language as _
        )
        .fetch_optional(&mut **transaction)
        .await?;

        Ok(row.map(|row| (GuildId::new(row.guild_id as u64), ChannelId::new(row.channel_id as u64))))
    }

    pub(crate) async fn restreamers(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<(User, Language)>, GameError> {
        let rows = sqlx::query!(
            r#"SELECT restreamer, language AS "language: Language" FROM game_restreamers WHERE game_id = $1 ORDER BY restreamer"#,
            self.id
        )
        .fetch_all(&mut **transaction)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            if let Some(user) = User::from_id(&mut **transaction, Id::<Users>::from(row.restreamer as i64)).await? {
                result.push((user, row.language));
            }
        }
        Ok(result)
    }

    #[allow(dead_code)]
    pub(crate) async fn restreamers_for_language(&self, transaction: &mut Transaction<'_, Postgres>, language: Language) -> Result<Vec<User>, GameError> {
        let rows = sqlx::query_scalar!(
            r#"SELECT restreamer FROM game_restreamers WHERE game_id = $1 AND language = $2 ORDER BY restreamer"#,
            self.id,
            language as _
        )
        .fetch_all(&mut **transaction)
        .await?;

        let mut users = Vec::new();
        for restreamer_id in rows {
            if let Some(user) = User::from_id(&mut **transaction, Id::<Users>::from(restreamer_id as i64)).await? {
                users.push(user);
            }
        }
        Ok(users)
    }

    pub(crate) async fn is_restreamer(&self, transaction: &mut Transaction<'_, Postgres>, user: &User, language: Language) -> Result<bool, GameError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM game_restreamers WHERE game_id = $1 AND restreamer = $2 AND language = $3"#,
            self.id,
            i64::from(user.id),
            language as _
        )
        .fetch_one(&mut **transaction)
        .await?;

        Ok(count.unwrap_or(0) > 0)
    }

    pub(crate) async fn is_restreamer_any_language(&self, transaction: &mut Transaction<'_, Postgres>, user: &User) -> Result<bool, GameError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM game_restreamers WHERE game_id = $1 AND restreamer = $2"#,
            self.id,
            i64::from(user.id)
        )
        .fetch_one(&mut **transaction)
        .await?;

        Ok(count.unwrap_or(0) > 0)
    }
} 