use crate::prelude::*;

/// Handle Discord role assignment for users who join the server
pub(crate) async fn handle_member_join(
    discord_ctx: &DiscordCtx,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get the database pool from the Discord context
    let db_pool = discord_ctx.data.read().await.get::<crate::discord_bot::DbPool>()
        .expect("database connection pool missing from Discord context")
        .clone();
    
    let mut transaction = db_pool.begin().await?;
    
    // Check if this user has any pending Discord invites
    let pending_invites = sqlx::query!(
        r#"SELECT role_request_id, discord_role_id, invite_url 
           FROM pending_discord_invites 
           WHERE user_id = (SELECT id FROM users WHERE discord_id = $1) 
           AND discord_guild_id = $2"#,
        PgSnowflake(user_id) as _,
        guild_id.get() as i64
    )
    .fetch_all(&mut *transaction)
    .await?;
    
    for invite in pending_invites {
        // Try to assign the Discord role
        if let Err(e) = guild_id.member(discord_ctx, user_id).await?.add_role(discord_ctx, RoleId::new(invite.discord_role_id.try_into().unwrap())).await {
            eprintln!("Failed to assign Discord role {} to user {}: {}", invite.discord_role_id, user_id, e);
            continue;
        }
        
        // Remove the pending invite since the role was successfully assigned
        sqlx::query!(
            r#"DELETE FROM pending_discord_invites 
               WHERE role_request_id = $1 AND discord_role_id = $2"#,
            invite.role_request_id,
            invite.discord_role_id
        )
        .execute(&mut *transaction)
        .await?;
        
        eprintln!("Successfully assigned Discord role {} to user {} for role request {}", 
                 invite.discord_role_id, user_id, invite.role_request_id);
    }
    
    transaction.commit().await?;
    Ok(())
}

/// Clean up expired pending invites
pub(crate) async fn cleanup_expired_invites(
    db_pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut transaction = db_pool.begin().await?;
    
    // Delete expired pending invites
    let deleted_count = sqlx::query!(
        r#"DELETE FROM pending_discord_invites 
           WHERE expires_at < NOW()"#
    )
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    
    if deleted_count > 0 {
        eprintln!("Cleaned up {} expired Discord invites", deleted_count);
    }
    
    transaction.commit().await?;
    Ok(())
} 
