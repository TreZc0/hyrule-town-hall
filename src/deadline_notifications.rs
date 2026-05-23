use {
    serenity::all::CreateMessage,
    serenity::model::id::ChannelId,
    sqlx::PgPool,
    crate::{
        prelude::*,
        series::Series,
    },
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
}

pub(crate) async fn deadline_notification_manager(
    db_pool: PgPool,
    discord_ctx: RwFuture<DiscordCtx>,
    shutdown: rocket::Shutdown,
) -> Result<(), Error> {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let discord_ctx = discord_ctx.read().await;
                if let Err(e) = run_passes(&db_pool, &discord_ctx).await {
                    eprintln!("Error in deadline notification manager: {e}");
                }
            }
            _ = shutdown.clone() => break,
        }
    }

    Ok(())
}

async fn run_passes(db_pool: &PgPool, discord_ctx: &DiscordCtx) -> Result<(), Error> {
    // Pass 1 — 3-day player reminder
    let rows = sqlx::query!(r#"
        SELECT id, scheduling_thread, scheduling_deadline AS "scheduling_deadline!"
        FROM races
        WHERE NOT ignored
          AND scheduling_deadline IS NOT NULL
          AND scheduling_deadline > NOW()
          AND scheduling_deadline <= NOW() + INTERVAL '3 days'
          AND NOT deadline_reminded_3d
          AND NOT schedule_locked
          AND end_time IS NULL
          AND start IS NULL
          AND async_start1 IS NULL
          AND async_start2 IS NULL
          AND async_start3 IS NULL
    "#).fetch_all(db_pool).await?;

    for row in rows {
        if let Some(thread_id) = row.scheduling_thread {
            let channel = ChannelId::new(thread_id as u64);
            let ts = row.scheduling_deadline.timestamp();
            let _ = channel.send_message(discord_ctx, CreateMessage::new()
                .content(format!("Reminder: you have 3 days left to schedule this race (deadline: <t:{ts}:F>)."))).await;
        }
        sqlx::query!("UPDATE races SET deadline_reminded_3d = true WHERE id = $1", row.id)
            .execute(db_pool).await?;
    }

    // Pass 2 — 24-hour player reminder
    let rows = sqlx::query!(r#"
        SELECT id, scheduling_thread, scheduling_deadline AS "scheduling_deadline!"
        FROM races
        WHERE NOT ignored
          AND scheduling_deadline IS NOT NULL
          AND scheduling_deadline > NOW()
          AND scheduling_deadline <= NOW() + INTERVAL '1 day'
          AND NOT deadline_reminded_24h
          AND NOT schedule_locked
          AND end_time IS NULL
          AND start IS NULL
          AND async_start1 IS NULL
          AND async_start2 IS NULL
          AND async_start3 IS NULL
    "#).fetch_all(db_pool).await?;

    for row in rows {
        if let Some(thread_id) = row.scheduling_thread {
            let channel = ChannelId::new(thread_id as u64);
            let ts = row.scheduling_deadline.timestamp();
            let _ = channel.send_message(discord_ctx, CreateMessage::new()
                .content(format!("Final reminder: 24 hours remaining to schedule this race (deadline: <t:{ts}:F>)."))).await;
        }
        sqlx::query!("UPDATE races SET deadline_reminded_24h = true WHERE id = $1", row.id)
            .execute(db_pool).await?;
    }

    // Pass 3 — organizer notification on missed deadline
    let rows = sqlx::query!(r#"
        SELECT
            r.id,
            r.series AS "series: Series",
            r.event,
            r.phase,
            r.round,
            e.discord_organizer_channel,
            COALESCE(t1.name, r.p1) AS team1_name,
            COALESCE(t2.name, r.p2) AS team2_name
        FROM races r
        JOIN events e ON e.series = r.series AND e.event = r.event
        LEFT JOIN teams t1 ON t1.id = r.team1
        LEFT JOIN teams t2 ON t2.id = r.team2
        WHERE NOT r.ignored
          AND r.scheduling_deadline IS NOT NULL
          AND r.scheduling_deadline <= NOW()
          AND NOT r.deadline_organizer_notified
          AND NOT r.schedule_locked
          AND r.end_time IS NULL
          AND r.start IS NULL
          AND r.async_start1 IS NULL
          AND r.async_start2 IS NULL
          AND r.async_start3 IS NULL
    "#).fetch_all(db_pool).await?;

    for row in rows {
        if let Some(channel_id) = row.discord_organizer_channel {
            let channel = ChannelId::new(channel_id as u64);
            let location = match (&row.phase, &row.round) {
                (Some(phase), Some(round)) => format!("{phase} {round}"),
                (Some(phase), None) => phase.clone(),
                (None, Some(round)) => round.clone(),
                (None, None) => format!("race #{}", row.id),
            };
            let matchup = match (&row.team1_name, &row.team2_name) {
                (Some(t1), Some(t2)) => format!("{t1} vs {t2}"),
                (Some(t1), None) => t1.clone(),
                (None, Some(t2)) => t2.clone(),
                (None, None) => format!("{} / {}", row.series, row.event),
            };
            let _ = channel.send_message(discord_ctx, CreateMessage::new()
                .content(format!("Scheduling deadline passed for {matchup} ({location}). No schedule has been set."))).await;
        }
        sqlx::query!("UPDATE races SET deadline_organizer_notified = true WHERE id = $1", row.id)
            .execute(db_pool).await?;
    }

    Ok(())
}
