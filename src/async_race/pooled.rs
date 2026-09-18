//! Pool allocation/delivery persistence around the shared async workflow.
//! Countdown, force-start timing, controls, and messages live in the parent module.
//! Database transactions never span Discord calls.
use super::*;
use serenity::all::{EditThread, GetMessages, Message, MessageId, Nonce};

#[derive(sqlx::FromRow)]
struct Delivery {
    id: i64,
    control_version: i64,
    state: String,
    source: String,
    series: String,
    event: String,
    discord_thread: Option<i64>,
    channel: Option<i64>,
    mode_name: String,
    seed_data: serde_json::Value,
    start_due_at: Option<DateTime<Utc>>,
    starts_close_at: Option<DateTime<Utc>>,
    submissions_close_at: Option<DateTime<Utc>>,
    undo_until: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    player_finished_at: Option<DateTime<Utc>>,
    participant_outcome: Option<String>,
    official_outcome: Option<String>,
    delivery_messages: serde_json::Value,
}

fn recovery_error(message: &str) -> Error {
    pooled_qualifiers::Error::InvalidTransition(message.into()).into()
}

pub(super) async fn sweep(pool: &PgPool, http: &Arc<Http>) -> Result<(), Error> {
    stream::iter(pending_delivery_ids(pool).await?)
        .map(|id| async move {
            let _ = reconcile(pool, http, id).await;
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await;
    Ok(())
}

async fn pending_delivery_ids(pool: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT id FROM qualifier_attempts WHERE
        (delivery_claim_until IS NULL OR delivery_claim_until < NOW()) AND ((source = 'async' AND (
            (state = 'assigned' AND jsonb_typeof(delivery_messages->'ready') IS DISTINCT FROM 'number') OR
            (state = 'revealed' AND jsonb_typeof(delivery_messages->'seed') IS DISTINCT FROM 'number') OR
            (state = 'starting' AND (delivery_messages ? ('go-' || control_version)
                OR NOT delivery_messages ? ('countdown-' || control_version))) OR
            (state = 'running' AND jsonb_typeof(delivery_messages->('run-' || control_version)) IS DISTINCT FROM 'number') OR
            (state = 'awaiting_verification' AND (undo_until IS NULL OR undo_until <= NOW()) AND jsonb_typeof(delivery_messages->('staff-' || control_version)) IS DISTINCT FROM 'number') OR
            (state = 'finalized' AND jsonb_typeof(delivery_messages->('final-' || control_version)) IS DISTINCT FROM 'number')
        )) OR (retry_declared_at IS NOT NULL
            AND jsonb_typeof(delivery_messages->'retry-declared') IS DISTINCT FROM 'number'
            AND EXISTS(SELECT 1 FROM events WHERE events.series=qualifier_attempts.series
                AND events.event=qualifier_attempts.event AND discord_organizer_channel IS NOT NULL))) ORDER BY id"#)
        .fetch_all(pool).await
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
    if let Err(error) = &result {
        if let Err(alert_error) = notify_setup_failure(pool, http, id, &error.to_string()).await {
            log::error!("could not report pooled async setup failure for attempt {id}: {alert_error}");
        }
    }
    result
}

async fn notify_setup_failure(pool: &PgPool, http: &Arc<Http>, id: i64, reason: &str) -> Result<(), Error> {
    let row: Option<(i64, i64, String, Option<i64>, Option<i64>)> = sqlx::query_as(r#"SELECT attempt.team_id, attempt.control_version, mode.display_name,
        event.discord_organizer_channel, event.discord_async_channel FROM qualifier_attempts attempt
        JOIN events event USING(series,event) JOIN qualifier_modes mode ON mode.id=attempt.mode_id
        WHERE attempt.id=$1 AND attempt.source='async' AND attempt.state='assigned'"#)
        .bind(id).fetch_optional(pool).await?;
    if let Some((team_id, control_version, mode, organizer, Some(parent))) = row {
        let mut tx = pool.begin().await?;
        let team = Team::from_id(&mut tx, Id::from(team_id as u64)).await?.ok_or(Error::NoTeamFound)?;
        let label = format!("{} — {mode} pool async", team.name(&mut tx).await?.unwrap_or_else(|| "Unknown entrant".into()));
        tx.commit().await?;
        setup::notify_failure(http, organizer.map(|id| ChannelId::new(id as u64)), ChannelId::new(parent as u64),
            &AsyncRun::PooledQualifier { attempt_id: id, control_version }, &label, reason).await;
    }
    Ok(())
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
    async fn edit(&self, channel: ChannelId, id: MessageId, content: String, components: Vec<CreateActionRow>) -> Result<Message, Error>;
    async fn find(&self, channel: ChannelId, attempt: i64, key: &str, nonce: Option<&str>) -> Result<Option<Message>, Error>;
    async fn send(
        &self,
        channel: ChannelId,
        content: String,
        components: Vec<CreateActionRow>,
        nonce: String,
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
    async fn edit(&self, channel: ChannelId, id: MessageId, content: String, components: Vec<CreateActionRow>) -> Result<Message, Error> {
        Ok(channel.edit_message(self.0, id, EditMessage::new().content(content).components(components)).await?)
    }
    async fn find(&self, channel: ChannelId, attempt: i64, key: &str, nonce: Option<&str>) -> Result<Option<Message>, Error> {
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
                .find(|message| message.author.id == bot && matches_delivery(message, attempt, key, nonce))
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
        nonce: String,
    ) -> Result<Message, Error> {
        Ok(channel
            .send_message(
                self.0,
                CreateMessage::new().content(content).components(components).nonce(Nonce::String(nonce)).enforce_nonce(true),
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

    let sent = if row.delivery_messages.get(key).is_some() {
        transport.find(channel, row.id, key, row.delivery_messages.get(format!("nonce:{key}")).and_then(serde_json::Value::as_str)).await?.ok_or_else(|| recovery_error("uncertain Discord send could not be recovered; staff must review the delivery operation"))?
    } else {
        // Keep delivery identity in Discord metadata, outside the visible text.
        let nonce = Uuid::new_v4().simple().to_string()[..25].to_owned();
        remember(pool, row, claim, &format!("nonce:{key}"), serde_json::json!(nonce)).await?;
        remember(pool, row, claim, key, serde_json::json!("pending")).await?;
        transport
            .send(channel, with_delivery_link(row, key, &content), components, nonce)
            .await?
    };
    remember(pool, row, claim, key, serde_json::json!(sent.id.get())).await?;
    Ok(sent)
}

pub(crate) fn matches_delivery(message: &Message, _attempt: i64, _key: &str, nonce: Option<&str>) -> bool {
    matches!((&message.nonce, nonce), (Some(Nonce::String(actual)), Some(expected)) if actual == expected)
}

fn with_delivery_link(row: &Delivery, key: &str, content: &str) -> String {
    let (label, page) = match key {
        "ready" => ("Check your current qualifier status on HTH", "status"),
        "retry-declared" => ("Qualifier management", "qualifiers"),
        _ => return content.to_owned(),
    };
    format!("{content}\n-# [{label}]({}/event/{}/{}/{page}#qualifier:{}:{key})", base_uri(), row.series, row.event, row.id)
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
    let parent = ChannelId::new(row.channel.ok_or_else(|| recovery_error("async channel is not configured"))? as u64);
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

async fn thread_details(pool: &PgPool, row: &Delivery) -> Result<(String, String), Error> {
    let mut tx = pool.begin().await?;
    let (team_id, retry): (i64, bool) = sqlx::query_as("SELECT team_id, retry_of IS NOT NULL FROM qualifier_attempts WHERE id=$1")
        .bind(row.id).fetch_one(&mut *tx).await?;
    let team = Team::from_id(&mut tx, Id::from(team_id as u64)).await?.ok_or(Error::NoTeamFound)?;
    let event = EventData::new(&mut tx, team.series, &team.event).await
        .map_err(|error| Error::Event(event::Error::Data(error)))?.ok_or(Error::EventNotFound)?;
    let name = team.name(&mut tx).await?.map(Cow::into_owned).unwrap_or_else(|| "Unknown entrant".into());
    let short = |text: &str| text.split_whitespace().join(" ").chars().take(38).collect::<String>();
    let thread_name = format!("Qualifier: {} — {}{}", short(&name), short(&row.mode_name), if retry { " (retry)" } else { "" });
    let config = pooled_qualifiers::Config::load(&mut tx, team.series, &team.event).await?
        .ok_or(pooled_qualifiers::Error::NotConfigured)?;
    let members = team.members(&mut tx).await?;
    let mut content = MessageBuilder::default();
    content.push("Welcome ");
    for (index, member) in members.iter().enumerate() {
        if index > 0 { content.push(", "); }
        content.mention_user(member);
    }
    content.push("!\n\nThis thread is for your ").push_bold_safe(&row.mode_name)
        .push(if retry { " qualifier re-attempt in " } else { " qualifier async in " })
        .push_safe(&event.display_name).push(".\n\n");
    append_async_instructions(&mut content, event.async_start_delay);
    AsyncRaceManager::append_qualifier_recording_requirements(&mut tx, &event, &mut content).await?;
    content.push("\n**Timing:**\nRun limit: ")
        .push(English.format_duration(config.run_limit().to_std().unwrap_or_default(), false))
        .push(" from GO, or the submission deadline below, whichever comes first.\n");
    if let Some(end) = row.starts_close_at {
        content.push(format!("Latest GO: <t:{}:F>.\n", end.timestamp()));
    }
    if let Some(end) = row.submissions_close_at {
        content.push(format!("Submission deadline: <t:{}:F>.\n", end.timestamp()));
    }
    content.push("\nIf you need help, ask the organizers in this thread.");
    tx.commit().await?;
    Ok((thread_name, content.build()))
}

async fn deliver(pool: &PgPool, http: &Arc<Http>, id: i64, claim: &str) -> Result<(), Error> {
    let row: Delivery = sqlx::query_as(r#"SELECT attempt.id, attempt.control_version, attempt.state, attempt.source, attempt.series, attempt.event, attempt.discord_thread,
        event.discord_async_channel AS channel, mode.display_name AS mode_name, seed.seed_data,
        attempt.start_due_at, config.starts_close_at, config.submissions_close_at,
        attempt.undo_until, attempt.started_at, attempt.player_finished_at, attempt.participant_outcome, attempt.official_outcome, attempt.delivery_messages
        FROM qualifier_attempts attempt JOIN events event USING (series, event)
        JOIN pooled_qualifier_configs config USING (series, event)
        JOIN qualifier_modes mode ON mode.id = attempt.mode_id JOIN qualifier_seeds seed ON seed.id = attempt.seed_id
        WHERE attempt.id = $1 AND attempt.delivery_claim = $2"#)
        .bind(id).bind(claim).fetch_one(pool).await?;
    let retry_notification = deliver_retry_declaration(pool, &DiscordTransport(http), &row, claim).await;
    if row.source != "async" || row.state == "void" {
        return retry_notification;
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
    let run = AsyncRun::PooledQualifier {
        attempt_id: id,
        control_version: row.control_version,
    };
    let current_control = match row.state.as_str() {
        "assigned" => "ready".to_owned(),
        "revealed" => "seed".to_owned(),
        "running" => format!("run-{}", row.control_version),
        "awaiting_verification" if row.undo_until.is_some_and(|end| Utc::now() < end) => {
            format!("run-{}", row.control_version - 1)
        }
        "awaiting_verification" => format!("staff-{}", row.control_version),
        _ => String::new(),
    };
    let current_message = row.delivery_messages.get(&current_control).or_else(|| {
        (row.state == "running").then(|| row.delivery_messages.get(format!("restore-run-{}", row.control_version))).flatten()
    });
    if let Some(messages) = row.delivery_messages.as_object() {
        for (key, value) in messages {
            if key != &current_control
                && current_message != Some(value)
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
            let (thread_name, welcome) = thread_details(pool, &row).await?;
            if row.delivery_messages.get("thread-name").and_then(serde_json::Value::as_str) != Some(&thread_name) {
                channel.edit_thread(http, EditThread::new().name(&thread_name)).await?;
                remember(pool, &row, claim, "thread-name", serde_json::json!(thread_name)).await?;
            }
            if row.delivery_messages.get("ready").is_none() {
                let entrants: Vec<Option<i64>> = sqlx::query_scalar(r#"SELECT DISTINCT users.discord_id FROM users
                    JOIN team_members ON team_members.member=users.id
                    WHERE team_members.team=(SELECT team_id FROM qualifier_attempts WHERE id=$1)"#).bind(id).fetch_all(pool).await?;
                let entrants: Vec<i64> = entrants.into_iter().map(|user| user.ok_or(Error::NoTeamMembers)).collect::<Result<_, _>>()?;
                if entrants.is_empty() { return Err(Error::NoTeamMembers); }
                let thread = channel.to_channel(http).await?.guild().ok_or(Error::EventNotFound)?;
                let entrant_ids: Vec<_> = entrants.iter().map(|user| UserId::new(*user as u64)).collect();
                add_async_thread_members(http, &thread, entrant_ids).await?;
                let organizers: Vec<i64> = sqlx::query_scalar(r#"SELECT DISTINCT users.discord_id FROM users
                    JOIN organizers ON organizers.organizer=users.id
                    WHERE (organizers.series,organizers.event)=(SELECT series,event FROM qualifier_attempts WHERE id=$1)
                        AND users.discord_id IS NOT NULL"#).bind(id).fetch_all(pool).await?;
                for organizer in organizers {
                    if entrants.contains(&organizer) { continue; }
                    let _ = add_async_thread_members(http, &thread, [UserId::new(organizer as u64)]).await;
                }
            }

            message(pool, http, &row, claim, channel, "ready", welcome, vec![CreateActionRow::Buttons(vec![CreateButton::new(run.button_id("ready")).label("READY!").style(ButtonStyle::Primary)])]).await?;
            setup::clear_alert(&run);
        }
        "revealed" => {
            if row.delivery_messages.get("seed").is_none() {
                if let Some(summary) = racetime_bot::baselines::seed_summary(&row.seed_data, true) {
                    for chunk in racetime_bot::baselines::message_chunks(&summary) {
                        channel.send_message(http, CreateMessage::new().content(chunk).allowed_mentions(serenity::all::CreateAllowedMentions::default())).await?;
                    }
                }
            }
            let mut content = seed_message(&row.seed_data)?.build();
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
                vec![start_button(&run)],
            )
            .await?;
        }

        "starting" => {
            let go = deliver_countdown(pool, &DiscordTransport(http), &row, claim, channel).await?;
            pooled_qualifiers::record_go(pool, id, row.control_version, *go.timestamp, Some(claim))
                .await?;
            Box::pin(deliver(pool, http, id, claim)).await?;
        }
        "running" => {
            deliver_running_controls(pool, &DiscordTransport(http), &row, claim, channel).await?;
        }
        "awaiting_verification" => {
            // FINISH already edits the running controls with the shared REVERT
            // button. Do not clear or duplicate that message during its grace period.
            if row.undo_until.is_some_and(|end| Utc::now() < end) { return retry_notification; }
            let (content, components) = if row.participant_outcome.as_deref() == Some("forfeit") {
                forfeit_message(&run, &participant_mentions(pool, id).await?)
            } else {
                let finished = row.player_finished_at.ok_or(Error::NotStarted)?;
                let started = row.started_at.ok_or(Error::NotStarted)?;
                completion_message(pool, &run, &format_finish_time(started, finished)).await?
            };
            message(pool, http, &row, claim, channel, &format!("staff-{}", row.control_version), content, components).await?;
        }

        "finalized" => {
            message(pool, http, &row, claim, channel, &format!("final-{}", row.control_version),
                format!("This qualifier has been recorded as **{}**. Contact an organizer if the result needs review.", row.official_outcome.as_deref().unwrap_or("final")), vec![]).await?;
        }
        _ => (),
    }
    retry_notification
}

async fn deliver_running_controls(pool: &PgPool, transport: &impl Transport, row: &Delivery, claim: &str, channel: ChannelId) -> Result<Message, Error> {
    let key = format!("run-{}", row.control_version);
    let run = AsyncRun::PooledQualifier { attempt_id: row.id, control_version: row.control_version };
    if row.delivery_messages.get(&key).and_then(serde_json::Value::as_u64).is_none() {
        if let Some(id) = row.delivery_messages.get(format!("restore-run-{}", row.control_version)).and_then(serde_json::Value::as_u64) {
            let message = transport.edit(channel, MessageId::new(id), REVERTED_MESSAGE.into(), vec![create_finish_forfeit_buttons(&run)]).await?;
            // Retrying the same edit is safe even if Discord's response was lost.
            remember(pool, row, claim, &key, serde_json::json!(id)).await?;
            return Ok(message);
        }
    }
    deliver_message(pool, transport, row, claim, channel, &key, RUNNING_MESSAGE.into(), vec![create_finish_forfeit_buttons(&run)]).await
}

async fn participant_mentions(pool: &PgPool, id: i64) -> Result<String, Error> {
    let players: Vec<i64> = sqlx::query_scalar(r#"SELECT users.discord_id FROM users
        JOIN team_members member ON member.member=users.id
        JOIN qualifier_attempts attempt ON attempt.team_id=member.team
        WHERE attempt.id=$1 AND users.discord_id IS NOT NULL ORDER BY users.id"#)
        .bind(id).fetch_all(pool).await?;
    Ok(players.iter().map(|id| format!("<@{id}>")).join(" "))
}

async fn deliver_countdown(
    pool: &PgPool,
    transport: &impl Transport,
    row: &Delivery,
    claim: &str,
    channel: ChannelId,
) -> Result<Message, Error> {
    let go_key = format!("go-{}", row.control_version);
    // Recover a sent or uncertain GO directly, even after the last-GO boundary.
    if row.delivery_messages.get(&go_key).is_some() {
        return deliver_message(pool, transport, row, claim, channel, &go_key, CountdownStep::Go.content(), vec![]).await;
    }
    if row.delivery_messages.get(format!("countdown-{}", row.control_version)).is_some() {
        return Err(recovery_error("countdown was interrupted before GO; organizer review required"));
    }
    if row.delivery_messages.get("forced-start").and_then(serde_json::Value::as_bool) == Some(true) {
        deliver_message(pool, transport, row, claim, channel,
            &format!("forced-start-{}", row.control_version), FORCE_START_MESSAGE.into(), vec![]).await?;
    }
    send_countdown(|step| async move {
        let key = match step {
            CountdownStep::Announcement => format!("countdown-{}", row.control_version),
            CountdownStep::Number(_) => {
                // Ordinary chat, just like normal asyncs: never edit, persist or
                // replay individual numbers after an interrupted countdown.
                return transport.send(channel, step.content(), vec![], Uuid::new_v4().simple().to_string()[..25].to_owned()).await;
            }
            CountdownStep::Go => {
                if row.starts_close_at.is_none_or(|end| Utc::now() >= end)
                    || row.submissions_close_at.is_none_or(|end| Utc::now() >= end)
                {
                    return Err(recovery_error("countdown passed the last GO; staff recovery required"));
                }
                format!("go-{}", row.control_version)
            }
        };
        deliver_message(pool, transport, row, claim, channel, &key, step.content(), vec![]).await
    }).await
}

async fn deliver_retry_declaration(
    pool: &PgPool,
    transport: &impl Transport,
    row: &Delivery,
    claim: &str,
) -> Result<(), Error> {
    if row.delivery_messages.get("retry-declared").is_some_and(serde_json::Value::is_number) {
        return Ok(());
    }
    let declaration: Option<(String, String, String, i64, i64, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT attempt.series, attempt.event, event.display_name,
            event.discord_organizer_channel, attempt.retry_declared_by, attempt.retry_declared_at
        FROM qualifier_attempts attempt JOIN events event USING(series,event)
        WHERE attempt.id=$1 AND attempt.retry_declared_at IS NOT NULL
          AND event.discord_organizer_channel IS NOT NULL"#,
    ).bind(row.id).fetch_optional(pool).await?;
    let Some((series, event, event_name, channel, declared_by, declared_at)) = declaration else {
        return Ok(());
    };
    let user = User::from_id(pool, Id::from(declared_by as u64)).await?
        .ok_or_else(|| recovery_error("retry declarant no longer exists"))?;
    let mut content = MessageBuilder::default();
    content.mention_user(&user)
        .push(" declared a qualifier re-attempt for ").push_bold_safe(&row.mode_name)
        .push(" in ").push_safe(event_name).push(".\nOriginal result: ")
        .push_safe(&row.source).push(" attempt ").push(row.id.to_string())
        .push(format!(". Declared <t:{}:F>.\n", declared_at.timestamp()))
        .push("The declaration applies to the next eligible attempt in this pool. Leaving a live race before GO keeps it pending.\n")
        .push(format!("<{}/event/{series}/{event}/qualifiers#attempt-{}>", base_uri(), row.id));
    deliver_message(pool, transport, row, claim, ChannelId::new(channel as u64),
        "retry-declared", content.build(), vec![]).await?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn test_delivery_failures(pool: &PgPool, id: i64) {
    struct Fake {
        messages: std::sync::Mutex<Vec<Message>>,
        threads: std::sync::Mutex<Vec<PrivateThread>>,
        fail_before: bool,
        fail_after: bool,
        fail_countdown_at: Option<u8>,
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
        async fn edit(&self, _: ChannelId, id: MessageId, content: String, components: Vec<CreateActionRow>) -> Result<Message, Error> {
            if self.fail_before { return Err(recovery_error("failure before edit")); }
            let mut messages = self.messages.lock().unwrap();
            let message = messages.iter_mut().find(|message| message.id == id).ok_or_else(|| recovery_error("missing message"))?;
            message.content = content;
            message.components = serde_json::from_value(serde_json::to_value(components).unwrap()).unwrap();
            if self.fail_after { return Err(recovery_error("lost edit response")); }
            Ok(message.clone())
        }
        async fn find(&self, _: ChannelId, attempt: i64, key: &str, nonce: Option<&str>) -> Result<Option<Message>, Error> {
            Ok(self
                .messages
                .lock()
                .unwrap()
                .iter()
                .find(|message| matches_delivery(message, attempt, key, nonce))
                .cloned())
        }
        async fn send(
            &self,
            channel: ChannelId,
            content: String,
            components: Vec<CreateActionRow>,
            nonce: String,
        ) -> Result<Message, Error> {
            if self.fail_before {
                return Err(recovery_error("failure before send"));
            }
            let fail_countdown = self.fail_countdown_at.is_some_and(|number| content == format!("**{number}**"));
            let message: Message = serde_json::from_value(serde_json::json!({
                "id": (123456 + self.messages.lock().unwrap().len() as u64).to_string(), "channel_id": channel.to_string(),
                "author": {"id":"1234", "username":"fake", "discriminator":"0001", "avatar":null, "bot":true},
                "content":content, "timestamp":"2026-09-10T10:00:00Z", "edited_timestamp":null,
                "tts":false, "mention_everyone":false, "mentions":[], "mention_roles":[], "attachments":[], "embeds":[], "pinned":false, "type":0,
                "components": components, "nonce": nonce
            })).unwrap();
            self.messages.lock().unwrap().push(message.clone());
            if self.fail_after || fail_countdown {
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
        source: "async".into(),
        series: "casboots".into(),
        event: "pqtest".into(),
        discord_thread: Some(1),
        channel: Some(1),
        mode_name: "test".into(),
        seed_data: serde_json::json!({}),
        start_due_at: None,
        starts_close_at: None,
        submissions_close_at: None,
        undo_until: None,
        started_at: None,
        player_finished_at: None,
        participant_outcome: None,
        official_outcome: None,
        delivery_messages: serde_json::json!({}),
    };
    let (name, welcome) = thread_details(pool, &row).await.unwrap();
    assert!(name.starts_with("Qualifier: "));
    assert!(name.ends_with(" — test"));
    assert!(!name.contains("Unknown entrant"));
    assert!(name.chars().count() <= 100);
    for instruction in ["Welcome", "**test**", "**READY!**", "**START COUNTDOWN**", "six-second", "**FINISH**", "**FORFEIT**", "30 seconds", "YouTube", "collection rate", "Organizers", "Run limit:"] {
        assert!(welcome.contains(instruction), "missing qualifier instruction: {instruction}");
    }
    assert!(with_delivery_link(&row, "ready", &welcome).chars().count() <= 2000);
    assert_eq!(with_delivery_link(&row, "ready", &welcome).matches("/status#").count(), 1);
    for key in ["seed", "countdown-3", "go-3", "run-3", "undo-4", "staff-4", "final-5", "warn-2min", "warn-30s"] {
        assert_eq!(with_delivery_link(&row, key, "Step text"), "Step text");
    }
    assert!(welcome.contains("No automatic force-start is configured"));
    for delay in [Some(10_i32), Some(0), Some(-1), None] {
        sqlx::query("UPDATE events SET async_start_delay=$2 WHERE (series,event)=(SELECT series,event FROM qualifier_attempts WHERE id=$1)")
            .bind(id).bind(delay).execute(pool).await.unwrap();
        let (_, instructions) = thread_details(pool, &row).await.unwrap();
        if delay == Some(10) {
            assert!(instructions.contains("**10 minutes**"));
        } else {
            assert!(instructions.contains("No automatic force-start is configured"));
        }
        assert!(!instructions.contains("automatically immediately"));
        assert!(with_delivery_link(&row, "ready", &instructions).chars().count() <= 2000);
    }
    for (state, key) in [("running", "run-1"), ("awaiting_verification", "staff-1"), ("finalized", "final-1")] {
        sqlx::query("UPDATE qualifier_attempts SET state=$2, official_outcome=CASE WHEN $2='finalized' THEN 'forfeit' ELSE NULL END, delivery_messages=jsonb_build_object($3::TEXT,'pending') WHERE id=$1")
            .bind(id).bind(state).bind(key).execute(pool).await.unwrap();
        assert!(pending_delivery_ids(pool).await.unwrap().contains(&id), "recover uncertain {key} delivery");
        sqlx::query("UPDATE qualifier_attempts SET delivery_messages=jsonb_build_object($2::TEXT,123) WHERE id=$1")
            .bind(id).bind(key).execute(pool).await.unwrap();
        assert!(!pending_delivery_ids(pool).await.unwrap().contains(&id), "do not resend confirmed {key}");
    }
    // No organizer notification or accepted submission during the REVERT window.
    sqlx::query("UPDATE qualifier_attempts SET state='awaiting_verification', official_outcome=NULL, undo_until=NOW()+INTERVAL '30 seconds', delivery_messages='{}' WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    assert!(!pending_delivery_ids(pool).await.unwrap().contains(&id));
    sqlx::query("UPDATE qualifier_attempts SET undo_until=NOW()-INTERVAL '1 second' WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    assert!(pending_delivery_ids(pool).await.unwrap().contains(&id));
    sqlx::query("UPDATE qualifier_attempts SET undo_until=NULL WHERE id=$1").bind(id).execute(pool).await.unwrap();
    start_timer::test_recovery(pool, id).await;
    sqlx::query("UPDATE qualifier_attempts SET state='assigned', official_outcome=NULL, revealed_at=NULL, start_due_at=NULL, delivery_messages='{}' WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
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
            fail_countdown_at: None,
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
            assert!(!message.content.contains("[qualifier:"));
            assert_eq!(message.content, "GO");
            let nonce = row.delivery_messages.get(format!("nonce:{key}")).and_then(serde_json::Value::as_str);
            assert!(matches_delivery(&message, id, key, nonce));
            assert!(!matches_delivery(&message, id, key, Some("wrong-nonce")));
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
        fail_countdown_at: None,
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
    // Failed REVERT edits remain pending, including a lost successful response.
    for fail_after in [false, true] {
        let mut transport = Fake {
            messages: std::sync::Mutex::new(Vec::new()),
            threads: std::sync::Mutex::new(Vec::new()),
            fail_before: false, fail_after: false, fail_countdown_at: None,
        };
        let original = transport.send(ChannelId::new(1), "Finish pending".into(), vec![], "revert-test".into()).await.unwrap();
        row.delivery_messages = serde_json::json!({"restore-run-1": original.id.get()});
        sqlx::query("UPDATE qualifier_attempts SET delivery_messages=$2 WHERE id=$1")
            .bind(id).bind(&row.delivery_messages).execute(pool).await.unwrap();
        transport.fail_before = !fail_after;
        transport.fail_after = fail_after;
        assert!(deliver_running_controls(pool, &transport, &row, "test", ChannelId::new(1)).await.is_err());
        row.delivery_messages = sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
            .bind(id).fetch_one(pool).await.unwrap();
        assert!(row.delivery_messages.get("run-1").is_none());
        transport.fail_before = false;
        transport.fail_after = false;
        let restored = deliver_running_controls(pool, &transport, &row, "test", ChannelId::new(1)).await.unwrap();
        assert_eq!(restored.id, original.id);
        assert_eq!(restored.content, REVERTED_MESSAGE);
        assert!(serde_json::to_string(&restored.components).unwrap().contains(&format!("async:finish:pool:{id}:1")));
        assert_eq!(transport.messages.lock().unwrap().len(), 1);
        let saved: i64 = sqlx::query_scalar("SELECT (delivery_messages->>'run-1')::BIGINT FROM qualifier_attempts WHERE id=$1")
            .bind(id).fetch_one(pool).await.unwrap();
        assert_eq!(saved as u64, original.id.get());
    }
    // Countdown numbers are ordinary chat. An interrupted countdown stops for
    // review rather than replaying numbers. Only an existing GO is recovered.
    let mut countdown_transport = Fake {
        messages: std::sync::Mutex::new(Vec::new()),
        threads: std::sync::Mutex::new(Vec::new()),
        fail_before: false,
        fail_after: false,
        fail_countdown_at: Some(3),
    };
    row.starts_close_at = Some(Utc::now() + chrono::Duration::minutes(5));
    row.submissions_close_at = row.starts_close_at;
    assert!(deliver_countdown(pool, &countdown_transport, &row, "test", ChannelId::new(1)).await.is_err());
    row.delivery_messages = sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap();
    countdown_transport.fail_countdown_at = None;
    assert!(deliver_countdown(pool, &countdown_transport, &row, "test", ChannelId::new(1)).await.is_err());
    assert_eq!(countdown_transport.messages.lock().unwrap().iter().map(|message| message.content.clone()).collect::<Vec<_>>(),
        ["**Your async is about to start!**", "**5**", "**4**", "**3**"]);
    assert!(!row.delivery_messages.as_object().unwrap().keys().any(|key| key.starts_with("countdown-1-")));
    // A fresh attempt sends exactly the same sequence as the shared normal flow.
    sqlx::query("UPDATE qualifier_attempts SET delivery_messages='{}' WHERE id=$1").bind(id).execute(pool).await.unwrap();
    row.delivery_messages = serde_json::json!({});
    countdown_transport.messages.lock().unwrap().clear();
    let go = deliver_countdown(pool, &countdown_transport, &row, "test", ChannelId::new(1)).await.unwrap();
    assert_eq!(countdown_transport.messages.lock().unwrap().iter().map(|message| message.content.clone()).collect::<Vec<_>>(),
        ["**Your async is about to start!**", "**5**", "**4**", "**3**", "**2**", "**1**", "**GO!** 🏃‍♂️"]);
    row.delivery_messages = sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap();
    row.starts_close_at = Some(Utc::now() - chrono::Duration::seconds(1));
    let recovered_go = deliver_countdown(pool, &countdown_transport, &row, "test", ChannelId::new(1)).await.unwrap();
    assert_eq!(recovered_go.id, go.id);
    assert_eq!(recovered_go.timestamp, go.timestamp);
    assert_eq!(countdown_transport.messages.lock().unwrap().len(), 7);
    for (before, after) in [(true, false), (false, true), (false, false)] {
        sqlx::query("UPDATE qualifier_attempts SET discord_thread=NULL, delivery_claim='test', delivery_claim_until=NOW()+INTERVAL '1 minute', delivery_messages='{}' WHERE id=$1").bind(id).execute(pool).await.unwrap();
        row.discord_thread = None;
        row.delivery_messages = serde_json::json!({});
        let transport = Fake {
            messages: std::sync::Mutex::new(Vec::new()),
            threads: std::sync::Mutex::new(Vec::new()),
            fail_before: before,
            fail_after: after,
            fail_countdown_at: None,
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
    // Retry declarations from live results must be delivered without an async
    // thread. Recover a lost response without sending a duplicate announcement.
    sqlx::query("UPDATE qualifier_attempts SET source='live', retry_declared_at=NOW(), retry_declared_by=created_by, delivery_messages='{}', delivery_claim=NULL, delivery_claim_until=NULL WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    row.source = "live".into();
    row.channel = None;
    row.delivery_messages = serde_json::json!({});
    let transport = Fake {
        messages: std::sync::Mutex::new(Vec::new()),
        threads: std::sync::Mutex::new(Vec::new()),
        fail_before: false,
        fail_after: true,
        fail_countdown_at: None,
    };
    assert!(!pending_delivery_ids(pool).await.unwrap().contains(&id));
    deliver_retry_declaration(pool, &transport, &row, "test").await.unwrap();
    assert!(transport.messages.lock().unwrap().is_empty());
    sqlx::query("UPDATE events SET discord_organizer_channel=2 WHERE (series,event)=(SELECT series,event FROM qualifier_attempts WHERE id=$1)")
        .bind(id).execute(pool).await.unwrap();
    assert!(pending_delivery_ids(pool).await.unwrap().contains(&id));
    sqlx::query("UPDATE qualifier_attempts SET delivery_claim='test', delivery_claim_until=NOW()+INTERVAL '1 minute' WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    assert!(deliver_retry_declaration(pool, &transport, &row, "test").await.is_err());
    row.delivery_messages = sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap();
    deliver_retry_declaration(pool, &transport, &row, "test").await.unwrap();
    row.delivery_messages = sqlx::query_scalar("SELECT delivery_messages FROM qualifier_attempts WHERE id=$1")
        .bind(id).fetch_one(pool).await.unwrap();
    deliver_retry_declaration(pool, &transport, &row, "test").await.unwrap();
    {
        let messages = transport.messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel_id, ChannelId::new(2));
        assert!(messages[0].content.contains("declared a qualifier re-attempt"));
        assert!(messages[0].content.contains(&format!("/event/casboots/pqtest/qualifiers#attempt-{id}")));
        assert!(messages[0].content.contains("**test**"));
    }
    assert!(transport.threads.lock().unwrap().is_empty());
    sqlx::query("UPDATE qualifier_attempts SET delivery_claim=NULL, delivery_claim_until=NULL WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    assert!(!pending_delivery_ids(pool).await.unwrap().contains(&id));
    sqlx::query("UPDATE events SET discord_organizer_channel=NULL WHERE (series,event)=(SELECT series,event FROM qualifier_attempts WHERE id=$1)")
        .bind(id).execute(pool).await.unwrap();
    sqlx::query("UPDATE qualifier_attempts SET source='async', retry_declared_at=NULL, retry_declared_by=NULL WHERE id=$1")
        .bind(id).execute(pool).await.unwrap();
    sqlx::query("UPDATE qualifier_attempts SET discord_thread=NULL WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE qualifier_attempts SET delivery_claim=NULL, delivery_claim_until=NULL, delivery_messages='{}' WHERE id=$1").bind(id).execute(pool).await.unwrap();
}
