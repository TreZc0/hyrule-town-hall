//! Recovery of the pooled Discord workflow. Database transactions never span Discord calls.
use super::*;
use serenity::all::{GetMessages, Message, MessageId};

#[derive(sqlx::FromRow)]
struct Delivery {
    id: i64,
    control_version: i64,
    state: String,
    discord_thread: Option<i64>,
    channel: i64,
    mode_name: String,
    seed_data: serde_json::Value,
    start_due_at: Option<DateTime<Utc>>,
    starts_close_at: Option<DateTime<Utc>>,
    submissions_close_at: Option<DateTime<Utc>>,
    undo_until: Option<DateTime<Utc>>,
    participant_outcome: Option<String>,
    official_outcome: Option<String>,
    delivery_messages: serde_json::Value,
}

fn recovery_error(message: &str) -> Error {
    pooled_qualifiers::Error::InvalidTransition(message.into()).into()
}

pub(super) async fn sweep(pool: &PgPool, http: &Arc<Http>) -> Result<(), Error> {
    let ids: Vec<i64> = sqlx::query_scalar(r#"SELECT id FROM qualifier_attempts WHERE source = 'async' AND
        (delivery_claim_until IS NULL OR delivery_claim_until < NOW()) AND (
            (state = 'assigned' AND NOT delivery_messages ? 'ready') OR
            (state = 'revealed' AND (NOT delivery_messages ? 'seed' OR start_due_at <= NOW())) OR
            state = 'starting' OR
            (state = 'running' AND NOT delivery_messages ? ('run-' || control_version)) OR
            (state = 'awaiting_verification' AND (undo_until IS NULL OR undo_until <= NOW()) AND NOT delivery_messages ? ('staff-' || control_version)) OR
            (state = 'finalized' AND NOT delivery_messages ? ('final-' || control_version))
        ) ORDER BY id"#)
        .fetch_all(pool).await?;
    stream::iter(ids)
        .map(|id| async move {
            let _ = reconcile(pool, http, id).await;
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
    Ok(())
}

pub(super) async fn reconcile(pool: &PgPool, http: &Arc<Http>, id: i64) -> Result<(), Error> {
    let claim = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;
    pooled_qualifiers::lock_attempt(&mut tx, id).await?;
    let claimed = sqlx::query("UPDATE qualifier_attempts SET delivery_claim = $2, delivery_claim_until = NOW() + INTERVAL '2 minutes' WHERE id = $1 AND (delivery_claim_until IS NULL OR delivery_claim_until < NOW())")
        .bind(id).bind(&claim).execute(&mut *tx).await?.rows_affected() == 1;
    tx.commit().await?;
    if !claimed {
        return Ok(());
    }
    let result = tokio::time::timeout(Duration::from_secs(90), deliver(pool, http, id, &claim))
        .await
        .unwrap_or_else(|_| {
            Err(recovery_error(
                "Discord delivery timed out; recovery will reconcile the pending operation",
            ))
        });
    let mut tx = pool.begin().await?;
    pooled_qualifiers::lock_attempt(&mut tx, id).await?;
    sqlx::query("UPDATE qualifier_attempts SET delivery_claim = NULL, delivery_claim_until = CASE WHEN $3::TEXT IS NULL THEN NULL ELSE NOW() + INTERVAL '1 minute' END, delivery_error = $3 WHERE id = $1 AND delivery_claim = $2")
        .bind(id).bind(&claim).bind(result.as_ref().err().map(ToString::to_string)).execute(&mut *tx).await?;
    tx.commit().await?;
    result
}

async fn remember(
    pool: &PgPool,
    row: &Delivery,
    claim: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    pooled_qualifiers::lock_attempt(&mut tx, row.id).await?;
    let changed = sqlx::query("UPDATE qualifier_attempts SET delivery_messages = jsonb_set(delivery_messages, ARRAY[$3], $4) WHERE id = $1 AND delivery_claim = $2 AND delivery_claim_until > NOW() AND control_version = $5 AND state = $6")
        .bind(row.id).bind(claim).bind(key).bind(value).bind(row.control_version).bind(&row.state).execute(&mut *tx).await?.rows_affected() == 1;
    if !changed {
        return Err(recovery_error("delivery lease expired"));
    }
    tx.commit().await?;
    Ok(())
}

/// Store intent before sending. On an uncertain send, adopt the bot's correlated
/// message. If it cannot be found, stop for staff recovery rather than reset a clock.
#[async_trait]
trait Transport: Send + Sync {
    async fn threads(&self, _parent: ChannelId) -> Result<Vec<PrivateThread>, Error> {
        Err(recovery_error("thread lookup unavailable"))
    }
    async fn create_thread(
        &self,
        _parent: ChannelId,
        _name: String,
    ) -> Result<PrivateThread, Error> {
        Err(recovery_error("thread creation unavailable"))
    }
    async fn get(&self, channel: ChannelId, id: MessageId) -> Result<Message, Error>;
    async fn find(&self, channel: ChannelId, marker: &str) -> Result<Option<Message>, Error>;
    async fn send(
        &self,
        channel: ChannelId,
        content: String,
        components: Vec<CreateActionRow>,
    ) -> Result<Message, Error>;
}

struct DiscordTransport<'a>(&'a Arc<Http>);

#[async_trait]
impl Transport for DiscordTransport<'_> {
    async fn threads(&self, parent: ChannelId) -> Result<Vec<PrivateThread>, Error> {
        let channel = parent
            .to_channel(self.0)
            .await?
            .guild()
            .ok_or(Error::EventNotFound)?;
        let mut threads = channel.guild_id.get_active_threads(self.0).await?.threads;
        threads.extend(
            parent
                .get_archived_private_threads(self.0, None, Some(100))
                .await?
                .threads,
        );
        let bot = self.0.get_current_user().await?.id;
        Ok(threads
            .into_iter()
            .map(|thread| PrivateThread {
                id: thread.id,
                parent: thread.parent_id,
                name: thread.name,
                private: thread.kind == ChannelType::PrivateThread,
                owned_by_bot: thread.owner_id == Some(bot),
            })
            .collect())
    }
    async fn create_thread(&self, parent: ChannelId, name: String) -> Result<PrivateThread, Error> {
        let thread = parent
            .create_thread(
                self.0,
                CreateThread::new(&name)
                    .kind(ChannelType::PrivateThread)
                    .invitable(false)
                    .auto_archive_duration(AutoArchiveDuration::OneWeek),
            )
            .await?;
        Ok(PrivateThread {
            id: thread.id,
            parent: Some(parent),
            name,
            private: true,
            owned_by_bot: true,
        })
    }
    async fn get(&self, channel: ChannelId, id: MessageId) -> Result<Message, Error> {
        Ok(channel.message(self.0, id).await?)
    }
    async fn find(&self, channel: ChannelId, marker: &str) -> Result<Option<Message>, Error> {
        let bot = self.0.get_current_user().await?.id;
        let mut before = None;
        loop {
            let mut query = GetMessages::new().limit(100);
            if let Some(id) = before {
                query = query.before(id);
            }
            let messages = channel.messages(self.0, query).await?;
            if let Some(message) = messages
                .iter()
                .find(|message| message.author.id == bot && message.content.ends_with(marker))
            {
                return Ok(Some(message.clone()));
            }
            if messages.len() < 100 {
                return Ok(None);
            }
            before = messages.last().map(|message| message.id);
        }
    }
    async fn send(
        &self,
        channel: ChannelId,
        content: String,
        components: Vec<CreateActionRow>,
    ) -> Result<Message, Error> {
        Ok(channel
            .send_message(
                self.0,
                CreateMessage::new().content(content).components(components),
            )
            .await?)
    }
}

async fn message(
    pool: &PgPool,
    http: &Arc<Http>,
    row: &Delivery,
    claim: &str,
    channel: ChannelId,
    key: &str,
    content: String,
    components: Vec<CreateActionRow>,
) -> Result<Message, Error> {
    deliver_message(
        pool,
        &DiscordTransport(http),
        row,
        claim,
        channel,
        key,
        content,
        components,
    )
    .await
}

async fn deliver_message(
    pool: &PgPool,
    transport: &impl Transport,
    row: &Delivery,
    claim: &str,
    channel: ChannelId,
    key: &str,
    content: String,
    components: Vec<CreateActionRow>,
) -> Result<Message, Error> {
    if let Some(id) = row
        .delivery_messages
        .get(key)
        .and_then(serde_json::Value::as_u64)
    {
        return transport.get(channel, MessageId::new(id)).await;
    }
    let marker = format!("[qualifier:{}:{key}]", row.id);
    let sent = if row.delivery_messages.get(key).is_some() {
        transport.find(channel, &marker).await?.ok_or_else(|| recovery_error("uncertain Discord send could not be recovered; staff must review the delivery operation"))?
    } else {
        remember(pool, row, claim, key, serde_json::json!("pending")).await?;
        transport
            .send(channel, format!("{content}\n-# {marker}"), components)
            .await?
    };
    remember(pool, row, claim, key, serde_json::json!(sent.id.get())).await?;
    Ok(sent)
}

#[derive(Clone)]
struct PrivateThread {
    id: ChannelId,
    parent: Option<ChannelId>,
    name: String,
    private: bool,
    owned_by_bot: bool,
}

async fn ensure_thread(
    pool: &PgPool,
    transport: &impl Transport,
    row: &Delivery,
    claim: &str,
) -> Result<ChannelId, Error> {
    if let Some(thread) = row.discord_thread {
        return Ok(ChannelId::new(thread as u64));
    }
    let parent = ChannelId::new(row.channel as u64);
    let name = format!("qualifier-{}", row.id);
    let thread = if row.delivery_messages.get("thread").is_some() {
        let candidates = transport.threads(parent).await?;
        let mut matches = candidates.into_iter().filter(|thread| {
            thread.parent == Some(parent)
                && thread.name == name
                && thread.private
                && thread.owned_by_bot
        });
        let found = matches.next().ok_or_else(|| recovery_error("uncertain private-thread creation; staff must locate and connect the original thread"))?;
        if matches.next().is_some() {
            return Err(recovery_error(
                "multiple matching private threads; staff recovery required",
            ));
        }
        found
    } else {
        remember(pool, row, claim, "thread", serde_json::json!("pending")).await?;
        transport.create_thread(parent, name).await?
    };
    let mut tx = pool.begin().await?;
    pooled_qualifiers::lock_attempt(&mut tx, row.id).await?;
    let changed = sqlx::query("UPDATE qualifier_attempts SET discord_thread = $3 WHERE id = $1 AND delivery_claim = $2 AND delivery_claim_until > NOW() AND discord_thread IS NULL AND control_version=$4 AND state=$5")
        .bind(row.id).bind(claim).bind(thread.id.get() as i64).bind(row.control_version).bind(&row.state).execute(&mut *tx).await?.rows_affected() == 1;
    tx.commit().await?;
    if !changed {
        return Err(recovery_error(
            "thread delivery lease or attempt state changed",
        ));
    }
    Ok(thread.id)
}

async fn deliver(pool: &PgPool, http: &Arc<Http>, id: i64, claim: &str) -> Result<(), Error> {
    let row: Delivery = sqlx::query_as(r#"SELECT attempt.id, attempt.control_version, attempt.state, attempt.discord_thread,
        event.discord_async_channel AS channel, mode.display_name AS mode_name, seed.seed_data,
        attempt.start_due_at, config.starts_close_at, config.submissions_close_at,
        attempt.undo_until, attempt.participant_outcome, attempt.official_outcome, attempt.delivery_messages
        FROM qualifier_attempts attempt JOIN events event USING (series, event)
        JOIN pooled_qualifier_configs config USING (series, event)
        JOIN qualifier_modes mode ON mode.id = attempt.mode_id JOIN qualifier_seeds seed ON seed.id = attempt.seed_id
        WHERE attempt.id = $1 AND attempt.delivery_claim = $2"#)
        .bind(id).bind(claim).fetch_one(pool).await?;
    if row.state == "void" {
        return Ok(());
    }
    if matches!(row.state.as_str(), "assigned" | "revealed")
        && (row.starts_close_at.is_none_or(|end| Utc::now() >= end)
            || row.submissions_close_at.is_none_or(|end| Utc::now() >= end))
    {
        // An unstarted run is not silently treated as a participant DNF.
        let mut tx = pool.begin().await?;
        pooled_qualifiers::lock_attempt(&mut tx, id).await?;
        sqlx::query("UPDATE qualifier_attempts SET state = 'awaiting_verification', player_finished_at = NOW(), participant_outcome = 'forfeit', control_version = control_version + 1 WHERE id = $1 AND control_version = $2 AND state IN ('assigned', 'revealed')")
            .bind(id).bind(row.control_version).execute(&mut *tx).await?;
        tx.commit().await?;
        return Err(recovery_error(
            "last GO passed before the attempt started; staff must review participant versus service failure",
        ));
    }
    let channel = ensure_thread(pool, &DiscordTransport(http), &row, claim).await?;
    // Retrying membership delivery is safe; never swallow failure and strand an entrant.
    let users: Vec<i64> = sqlx::query_scalar(r#"SELECT DISTINCT users.discord_id FROM users WHERE users.discord_id IS NOT NULL AND users.id IN (
        SELECT member FROM team_members WHERE team = (SELECT team_id FROM qualifier_attempts WHERE id = $1)
        UNION SELECT organizer FROM organizers WHERE (series, event) = (SELECT series, event FROM qualifier_attempts WHERE id = $1))"#)
        .bind(id).fetch_all(pool).await?;
    for user in users {
        channel
            .add_thread_member(http, UserId::new(user as u64))
            .await?;
    }
    let run = AsyncRun::PooledQualifier {
        attempt_id: id,
        control_version: row.control_version,
    };
    let current_control = match row.state.as_str() {
        "assigned" => "ready".to_owned(),
        "revealed" => "seed".to_owned(),
        "running" => format!("run-{}", row.control_version),
        "awaiting_verification" if row.undo_until.is_some_and(|end| Utc::now() < end) => {
            format!("undo-{}", row.control_version)
        }
        "awaiting_verification" => format!("staff-{}", row.control_version),
        _ => String::new(),
    };
    if let Some(messages) = row.delivery_messages.as_object() {
        for (key, value) in messages {
            if key != &current_control
                && (key == "ready"
                    || key == "seed"
                    || key.starts_with("run-")
                    || key.starts_with("undo-")
                    || key.starts_with("staff-"))
            {
                if let Some(id) = value.as_u64() {
                    let _ = channel
                        .edit_message(
                            http,
                            MessageId::new(id),
                            EditMessage::new().components(vec![]),
                        )
                        .await;
                }
            }
        }
    }
    match row.state.as_str() {
        "assigned" => {
            message(pool, http, &row, claim, channel, "ready", format!("Welcome to your **{}** qualifier. Click READY! when you are ready to receive the seed.", row.mode_name), vec![CreateActionRow::Buttons(vec![CreateButton::new(run.button_id("ready")).label("READY!").style(ButtonStyle::Primary)])]).await?;
        }
        "revealed" => {
            let mut content = pooled_seed_message(&row.seed_data)?.build();
            if let Some(due) = row.start_due_at {
                content.push_str(&format!(
                    "\nAutomatic countdown: <t:{}:R>.",
                    due.timestamp()
                ));
            }
            message(
                pool,
                http,
                &row,
                claim,
                channel,
                "seed",
                content,
                vec![CreateActionRow::Buttons(vec![
                    CreateButton::new(run.button_id("start_countdown"))
                        .label("START COUNTDOWN")
                        .style(ButtonStyle::Success),
                ])],
            )
            .await?;
            if row.start_due_at.is_some_and(|due| due <= Utc::now()) {
                if pooled_qualifiers::request_start(pool, id, row.control_version).await? {
                    Box::pin(deliver(pool, http, id, claim)).await?;
                }
            }
        }
        "starting" => {
            let key = format!("go-{}", row.control_version);
            // An existing or uncertain GO is reconciled even after the last-GO boundary.
            if row.delivery_messages.get(&key).is_none() {
                message(
                    pool,
                    http,
                    &row,
                    claim,
                    channel,
                    &format!("countdown-{}", row.control_version),
                    "**Your async is about to start! GO follows in six seconds.**".into(),
                    vec![],
                )
                .await?;
                let due = row
                    .start_due_at
                    .ok_or_else(|| recovery_error("countdown has no due time"))?;
                if due > Utc::now() {
                    sleep((due - Utc::now()).to_std().unwrap_or_default()).await;
                }
                if row.starts_close_at.is_none_or(|end| Utc::now() >= end)
                    || row.submissions_close_at.is_none_or(|end| Utc::now() >= end)
                {
                    return Err(recovery_error(
                        "countdown passed the last GO; staff recovery required",
                    ));
                }
            }
            let go = message(
                pool,
                http,
                &row,
                claim,
                channel,
                &key,
                "**GO!** 🏃".into(),
                vec![],
            )
            .await?;
            pooled_qualifiers::record_go(pool, id, row.control_version, *go.timestamp, Some(claim))
                .await?;
            Box::pin(deliver(pool, http, id, claim)).await?;
        }
        "running" => {
            message(
                pool,
                http,
                &row,
                claim,
                channel,
                &format!("run-{}", row.control_version),
                "**Good luck!** Click FINISH when done, or Forfeit this async.".into(),
                vec![create_finish_forfeit_buttons(&run)],
            )
            .await?;
        }
        "awaiting_verification" => {
            if row.undo_until.is_some_and(|end| Utc::now() < end) {
                message(
                    pool,
                    http,
                    &row,
                    claim,
                    channel,
                    &format!("undo-{}", row.control_version),
                    "FINISH accepted. You may revert until the original 30-second undo deadline."
                        .into(),
                    vec![CreateActionRow::Buttons(vec![
                        CreateButton::new(run.button_id("revert"))
                            .label("REVERT")
                            .style(ButtonStyle::Secondary),
                    ])],
                )
                .await?;
            } else {
                let forfeit = row.participant_outcome.as_deref() == Some("forfeit");
                message(pool, http, &row, claim, channel, &format!("staff-{}", row.control_version), if forfeit { "@here — Forfeit reported. Organizers: confirm the forfeit after review." } else { "@here — Qualifier complete. Please provide your VOD/recording and final-time screenshot. Staff: record the official result." }.into(), vec![CreateActionRow::Buttons(vec![CreateButton::new(run.button_id(if forfeit { "org_forfeit" } else { "org_result" })).label(if forfeit { "Confirm Forfeit" } else { "Confirm Result" }).style(if forfeit { ButtonStyle::Danger } else { ButtonStyle::Primary })])]).await?;
            }
        }
        "finalized" => {
            message(pool, http, &row, claim, channel, &format!("final-{}", row.control_version),
                format!("This qualifier has been recorded as **{}**. Contact an organizer if the result needs review.", row.official_outcome.as_deref().unwrap_or("final")), vec![]).await?;
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn test_delivery_failures(pool: &PgPool, id: i64) {
    struct Fake {
        messages: std::sync::Mutex<Vec<Message>>,
        threads: std::sync::Mutex<Vec<PrivateThread>>,
        fail_before: bool,
        fail_after: bool,
    }
    #[async_trait]
    impl Transport for Fake {
        async fn threads(&self, _: ChannelId) -> Result<Vec<PrivateThread>, Error> {
            Ok(self.threads.lock().unwrap().clone())
        }
        async fn create_thread(
            &self,
            parent: ChannelId,
            name: String,
        ) -> Result<PrivateThread, Error> {
            if self.fail_before {
                return Err(recovery_error("failure before thread creation"));
            }
            let thread = PrivateThread {
                id: ChannelId::new(12345678),
                parent: Some(parent),
                name,
                private: true,
                owned_by_bot: true,
            };
            self.threads.lock().unwrap().push(thread.clone());
            if self.fail_after {
                Err(recovery_error("lost thread creation response"))
            } else {
                Ok(thread)
            }
        }
        async fn get(&self, _: ChannelId, id: MessageId) -> Result<Message, Error> {
            self.messages
                .lock()
                .unwrap()
                .iter()
                .find(|message| message.id == id)
                .cloned()
                .ok_or_else(|| recovery_error("missing message"))
        }
        async fn find(&self, _: ChannelId, marker: &str) -> Result<Option<Message>, Error> {
            Ok(self
                .messages
                .lock()
                .unwrap()
                .iter()
                .find(|message| message.content.ends_with(marker))
                .cloned())
        }
        async fn send(
            &self,
            channel: ChannelId,
            content: String,
            _: Vec<CreateActionRow>,
        ) -> Result<Message, Error> {
            if self.fail_before {
                return Err(recovery_error("failure before send"));
            }
            let message: Message = serde_json::from_value(serde_json::json!({
                "id": "123456", "channel_id": channel.to_string(),
                "author": {"id":"1234", "username":"fake", "discriminator":"0001", "avatar":null, "bot":true},
                "content":content, "timestamp":"2026-09-10T10:00:00Z", "edited_timestamp":null,
                "tts":false, "mention_everyone":false, "mentions":[], "mention_roles":[], "attachments":[], "embeds":[], "pinned":false, "type":0
            })).unwrap();
            self.messages.lock().unwrap().push(message.clone());
            if self.fail_after {
                Err(recovery_error("send succeeded but response was lost"))
            } else {
                Ok(message)
            }
        }
    }
    let mut row = Delivery {
        id,
        control_version: 1,
        state: "assigned".into(),
        discord_thread: Some(1),
        channel: 1,
        mode_name: "test".into(),
        seed_data: serde_json::json!({}),
        start_due_at: None,
        starts_close_at: None,
        submissions_close_at: None,
        undo_until: None,
        participant_outcome: None,
        official_outcome: None,
        delivery_messages: serde_json::json!({}),
    };
    for (key, before, after) in [
        ("test-before", true, false),
        ("test-after", false, true),
        ("test-ok", false, false),
    ] {
        sqlx::query("UPDATE qualifier_attempts SET delivery_claim='test', delivery_claim_until=NOW()+INTERVAL '1 minute' WHERE id=$1")
            .bind(id).execute(pool).await.unwrap();
        let transport = Fake {
            messages: std::sync::Mutex::new(Vec::new()),
            threads: std::sync::Mutex::new(Vec::new()),
            fail_before: before,
            fail_after: after,
        };
        let initial = deliver_message(
            pool,
            &transport,
            &row,
            "test",
            ChannelId::new(1),
            key,
            "GO".into(),
            vec![],
        )
        .await;
        assert_eq!(initial.is_err(), before || after);
        row.delivery_messages =
            sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
        let recovered = deliver_message(
            pool,
            &transport,
            &row,
            "test",
            ChannelId::new(1),
            key,
            "GO".into(),
            vec![],
        )
        .await;
        assert_eq!(recovered.is_err(), before);
        assert_eq!(
            transport.messages.lock().unwrap().len(),
            usize::from(!before)
        );
        if let Ok(message) = recovered {
            assert_eq!(
                message.timestamp.unix_timestamp(),
                DateTime::parse_from_rfc3339("2026-09-10T10:00:00Z")
                    .unwrap()
                    .timestamp()
            );
        }
    }
    let transport = Fake {
        messages: std::sync::Mutex::new(Vec::new()),
        threads: std::sync::Mutex::new(Vec::new()),
        fail_before: false,
        fail_after: false,
    };
    assert!(
        deliver_message(
            pool,
            &transport,
            &row,
            "expired-claim",
            ChannelId::new(1),
            "test-stale",
            "GO".into(),
            vec![]
        )
        .await
        .is_err()
    );
    assert!(transport.messages.lock().unwrap().is_empty());
    for (before, after) in [(true, false), (false, true), (false, false)] {
        sqlx::query("UPDATE qualifier_attempts SET discord_thread=NULL, delivery_claim='test', delivery_claim_until=NOW()+INTERVAL '1 minute', delivery_messages='{}' WHERE id=$1").bind(id).execute(pool).await.unwrap();
        row.discord_thread = None;
        row.delivery_messages = serde_json::json!({});
        let transport = Fake {
            messages: std::sync::Mutex::new(Vec::new()),
            threads: std::sync::Mutex::new(Vec::new()),
            fail_before: before,
            fail_after: after,
        };
        assert_eq!(
            ensure_thread(pool, &transport, &row, "test").await.is_err(),
            before || after
        );
        row.delivery_messages =
            sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
        row.discord_thread =
            sqlx::query_scalar("SELECT discord_thread FROM qualifier_attempts WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(
            ensure_thread(pool, &transport, &row, "test").await.is_err(),
            before
        );
        assert_eq!(
            transport.threads.lock().unwrap().len(),
            usize::from(!before)
        );
    }
    sqlx::query("UPDATE qualifier_attempts SET discord_thread=NULL WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE qualifier_attempts SET delivery_claim=NULL, delivery_claim_until=NULL, delivery_messages='{}' WHERE id=$1").bind(id).execute(pool).await.unwrap();
}
