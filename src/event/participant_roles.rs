use crate::prelude::*;

/// Keep a shared role while another non-resigned team still needs it.
pub(super) async fn still_needed(
    transaction: &mut Transaction<'_, Postgres>,
    guild: GuildId,
    role: RoleId,
    discord_user: UserId,
    removed_team: Id<Teams>,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
            FROM discord_roles dr
            JOIN teams t ON t.series = dr.series AND t.event = dr.event
            JOIN team_members tm ON tm.team = t.id
            JOIN users u ON u.id = tm.member
            WHERE dr.guild = $1 AND dr.id = $2 AND u.discord_id = $3
                AND dr.role IS NULL AND dr.racetime_team IS NULL
                AND t.id <> $4 AND NOT t.resigned
                AND tm.status IN ('created', 'confirmed')
        )",
    )
    .bind(guild.get() as i64)
    .bind(role.get() as i64)
    .bind(discord_user.get() as i64)
    .bind(i64::from(removed_team))
    .fetch_one(&mut **transaction)
    .await
}
