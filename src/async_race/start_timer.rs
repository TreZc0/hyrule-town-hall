//! Restore pooled force-start timers from the already persisted READY deadline.
use super::*;

#[derive(sqlx::FromRow)]
struct Timer {
    id: i64,
    control_version: i64,
    discord_thread: i64,
    player: i64,
    revealed_at: DateTime<Utc>,
    start_due_at: DateTime<Utc>,
}

async fn pending(pool: &PgPool) -> Result<Vec<Timer>, sqlx::Error> {
    sqlx::query_as(r#"SELECT attempt.id, attempt.control_version, attempt.discord_thread,
        entrant.discord_id AS player, attempt.revealed_at, attempt.start_due_at
        FROM qualifier_attempts attempt
        JOIN LATERAL (SELECT users.discord_id FROM team_members
            JOIN users ON users.id = team_members.member
            WHERE team_members.team = attempt.team_id AND users.discord_id IS NOT NULL
            ORDER BY users.id LIMIT 1) entrant ON TRUE
        WHERE attempt.source = 'async' AND attempt.state = 'revealed'
            AND attempt.discord_thread IS NOT NULL AND attempt.revealed_at IS NOT NULL
            AND attempt.start_due_at IS NOT NULL
            AND jsonb_typeof(attempt.delivery_messages->'seed') = 'number'"#)
        .fetch_all(pool).await
}

pub(super) async fn reconcile(pool: &PgPool, http: &Arc<Http>) -> Result<(), Error> {
    static TASKS: LazyLock<std::sync::Mutex<HashMap<(i64, i64), tokio::task::JoinHandle<()>>>> =
        LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
    let timers = pending(pool).await?;
    let mut tasks = TASKS.lock().expect("force-start task registry poisoned");
    // Keep active tasks even after they claim START: they may be posting the countdown.
    tasks.retain(|_, task| !task.is_finished());
    for timer in timers {
        tasks.entry((timer.id, timer.control_version)).or_insert_with(|| {
            spawn_force_start_at(pool.clone(), Arc::clone(http),
                ChannelId::new(timer.discord_thread as u64),
                AsyncRun::PooledQualifier { attempt_id: timer.id, control_version: timer.control_version },
                UserId::new(timer.player as u64), timer.revealed_at, timer.start_due_at)
        });
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn test_recovery(pool: &PgPool, id: i64) {
    let ready = Utc::now() - chrono::Duration::minutes(15);
    let due = ready + chrono::Duration::minutes(10);
    sqlx::query("UPDATE qualifier_attempts SET state='revealed', discord_thread=1, revealed_at=$2, start_due_at=$3, delivery_messages='{}' WHERE id=$1")
        .bind(id).bind(ready).bind(due).execute(pool).await.unwrap();
    assert!(!pending(pool).await.unwrap().iter().any(|timer| timer.id == id));
    sqlx::query("UPDATE qualifier_attempts SET delivery_messages='{\"seed\":123}' WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    let timer = pending(pool).await.unwrap().into_iter().find(|timer| timer.id == id).unwrap();
    assert_eq!(timer.revealed_at.timestamp_micros(), ready.timestamp_micros());
    assert_eq!(timer.start_due_at.timestamp_micros(), due.timestamp_micros());
    for state in ["starting", "running", "void"] {
        sqlx::query("UPDATE qualifier_attempts SET state=$2 WHERE id=$1")
            .bind(id).bind(state).execute(pool).await.unwrap();
        assert!(!pending(pool).await.unwrap().iter().any(|timer| timer.id == id));
    }
    sqlx::query("UPDATE qualifier_attempts SET state='revealed', start_due_at=NULL WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    assert!(!pending(pool).await.unwrap().iter().any(|timer| timer.id == id));
}
