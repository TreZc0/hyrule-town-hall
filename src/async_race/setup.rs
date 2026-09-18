//! Shared setup of normal async threads and organizer alerts for all asyncs.
use super::*;
use serenity::all::{EditThread, GetMessages, Message, Nonce};

const PENDING_PREFIX: &str = "Setting up: ";
static ALERTED: LazyLock<std::sync::Mutex<HashSet<String>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));
static COMPLETED: LazyLock<std::sync::Mutex<HashSet<i64>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));

// The existing thread ID is saved before invitations. The temporary Discord
// name marks incomplete setup, including across restarts, without a new DB field.
pub(super) async fn thread(pool: &PgPool, http: &Arc<Http>, parent: ChannelId, run: &AsyncRun, name: &str) -> Result<Option<GuildChannel>, Error> {
    if let Some(id) = run.thread_id(pool).await? {
        if COMPLETED.lock().expect("async setup cache poisoned").contains(&id) { return Ok(None); }
        let thread = ChannelId::new(id as u64).to_channel(http).await?.guild().ok_or(Error::EventNotFound)?;
        if !thread.name.starts_with(PENDING_PREFIX) {
            COMPLETED.lock().expect("async setup cache poisoned").insert(id);
            clear_alert(run);
            return Ok(None);
        }
        return Ok(Some(thread));
    }
    let thread = parent.create_thread(http, CreateThread::new(format!("{PENDING_PREFIX}{name}").chars().take(100).collect::<String>())
        .kind(ChannelType::PrivateThread).auto_archive_duration(AutoArchiveDuration::OneWeek)).await?;
    save_thread(pool, run, thread.id).await?;
    Ok(Some(thread))
}

async fn save_thread(pool: &PgPool, run: &AsyncRun, thread: ChannelId) -> Result<(), Error> {
    let id = thread.get() as i64;
    match run {
        AsyncRun::BracketRace { race_id, async_part } => {
            let query = match async_part {
                1 => "UPDATE races SET async_thread1=$2 WHERE id=$1 AND async_thread1 IS NULL",
                2 => "UPDATE races SET async_thread2=$2 WHERE id=$1 AND async_thread2 IS NULL",
                3 => "UPDATE races SET async_thread3=$2 WHERE id=$1 AND async_thread3 IS NULL",
                _ => return Err(Error::InvalidAsyncPart),
            };
            if sqlx::query(query).bind(race_id).bind(id).execute(pool).await?.rows_affected() != 1 {
                return Err(Error::ResetAsyncPart);
            }
        }
        AsyncRun::Qualifier { team_id, async_kind } => {
            if sqlx::query("UPDATE async_teams SET discord_thread=$3 WHERE team=$1 AND kind=$2 AND discord_thread IS NULL")
                .bind(team_id).bind(*async_kind).bind(id).execute(pool).await?.rows_affected() != 1 {
                return Err(Error::ResetAsyncPart);
            }
        }
        AsyncRun::PooledQualifier { .. } => return Err(Error::InvalidAsyncPart),
    }
    Ok(())
}

fn ready_nonce(thread: ChannelId) -> String {
    format!("r:{}", thread.get())
}

fn is_ready_message(message: &Message, bot: UserId, run: &AsyncRun) -> bool {
    message.author.id == bot && (matches!(&message.nonce, Some(Nonce::String(nonce)) if nonce == &ready_nonce(message.channel_id))
        || message.components.iter().any(|row| row.components.iter().any(|component| {
        matches!(component, ActionRowComponent::Button(button)
            if matches!(&button.data, ButtonKind::NonLink { custom_id, .. } if custom_id == &run.button_id("ready")))
    })))
}

pub(super) async fn send_ready(http: &Arc<Http>, thread: &GuildChannel, run: &AsyncRun, content: String, button: CreateActionRow) -> Result<(), Error> {
    let bot = http.get_current_user().await?.id;
    // A failed send may have reached Discord. Reuse its READY instead of posting twice.
    let mut before = None;
    loop {
        let mut query = GetMessages::new().limit(100);
        if let Some(id) = before { query = query.before(id); }
        let messages = thread.id.messages(http, query).await?;
        if messages.iter().any(|message| is_ready_message(message, bot, run)) { return Ok(()); }
        if messages.len() < 100 { break; }
        before = messages.last().map(|message| message.id);
    }
    thread.send_message(http, CreateMessage::new().content(content).components(vec![button])
        .nonce(Nonce::String(ready_nonce(thread.id))).enforce_nonce(true)).await?;
    Ok(())
}

pub(super) async fn complete(http: &Arc<Http>, thread: &GuildChannel, run: &AsyncRun, name: &str) -> Result<(), Error> {
    thread.id.edit_thread(http, EditThread::new().name(name)).await?;
    COMPLETED.lock().expect("async setup cache poisoned").insert(thread.id.get() as i64);
    clear_alert(run);
    Ok(())
}

pub(super) fn clear_alert(run: &AsyncRun) {
    ALERTED.lock().expect("async setup alert registry poisoned").remove(&run.button_id("ready"));
}

fn failure_message(parent: ChannelId, label: &str, reason: &str) -> String {
    let mut message = MessageBuilder::new();
    message.push("**Async thread setup needs attention**\n")
        .push_safe(label).push("\n\n")
        .push("Please check that every player has access to the parent async channel ")
        .push(format!("<#{}>", parent.get()))
        .push(" (including the required server membership and roles). Inviting someone to a private thread does not grant access to its parent channel. Also check the bot's permissions to create threads and add members.\n\n")
        .push("Setup will retry automatically and reuse any saved thread while the async remains eligible. READY is only posted after the players have been added.\n\nError: ")
        .push_safe(reason.chars().take(600).collect::<String>());
    message.build()
}

pub(super) async fn notify_failure(http: &Arc<Http>, organizer: Option<ChannelId>, parent: ChannelId, run: &AsyncRun, label: &str, reason: &str) {
    log::error!("async thread setup failed for {} ({label}): {reason}", run.button_id("ready"));
    let Some(organizer) = organizer else {
        log::error!("cannot notify organizers: no organizer channel configured");
        return;
    };
    let key = run.button_id("ready");
    if ALERTED.lock().expect("async setup alert registry poisoned").contains(&key) { return; }
    match organizer.send_message(http, CreateMessage::new().content(failure_message(parent, label, reason))
        .allowed_mentions(serenity::all::CreateAllowedMentions::default())).await {
        Ok(_) => { ALERTED.lock().expect("async setup alert registry poisoned").insert(key); }
        Err(error) => log::error!("could not notify async organizers: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_recovery_accepts_edited_bot_messages_but_not_other_users_or_runs() {
        let run = AsyncRun::Qualifier { team_id: 42, async_kind: AsyncKind::Qualifier1 };
        let bot = UserId::new(1234);
        let mut message: Message = serde_json::from_value(serde_json::json!({
            "id":"123456", "channel_id":"5678",
            "author":{"id":"1234","username":"fake","discriminator":"0001","avatar":null,"bot":true},
            "content":"Welcome", "timestamp":"2026-09-10T10:00:00Z", "edited_timestamp":null,
            "tts":false, "mention_everyone":false, "mentions":[], "mention_roles":[], "attachments":[], "embeds":[], "pinned":false, "type":0,
            "components":[CreateActionRow::Buttons(vec![CreateButton::new(run.button_id("ready")).label("READY!")])]
        })).unwrap();
        assert!(is_ready_message(&message, bot, &run));
        assert!(!is_ready_message(&message, UserId::new(999), &run));
        assert!(!is_ready_message(&message, bot, &AsyncRun::Qualifier { team_id: 43, async_kind: AsyncKind::Qualifier1 }));
        message.components.clear();
        assert!(!is_ready_message(&message, bot, &run));
        message.nonce = Some(Nonce::String(ready_nonce(message.channel_id)));
        assert!(is_ready_message(&message, bot, &run), "READY may already have been clicked after an uncertain send");
        assert!(ready_nonce(ChannelId::new(u64::MAX)).len() <= 25);
        let alert = failure_message(ChannelId::new(5678), "Player — Mode", "Missing permissions");
        assert!(alert.contains("Player — Mode"));
        assert!(alert.contains("<#5678>"));
        assert!(alert.contains("parent channel"));
        assert!(alert.contains("retry automatically"));
        assert!(alert.contains("Missing permissions"));
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn normal_async_thread_ids_survive_failed_setup_and_are_not_replaced() {
        let pool = event::configuration::test_pool().await;
        let (race_id, old_thread): (i64, Option<i64>) = sqlx::query_as("SELECT id,async_thread1 FROM races ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        let (team_id, kind, old_qualifier_thread): (i64, AsyncKind, Option<i64>) = sqlx::query_as("SELECT team,kind,discord_thread FROM async_teams ORDER BY team DESC,kind LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        let result = std::panic::AssertUnwindSafe(async {
            sqlx::query("UPDATE races SET async_thread1=NULL WHERE id=$1").bind(race_id).execute(&pool).await.unwrap();
            sqlx::query("UPDATE async_teams SET discord_thread=NULL WHERE team=$1 AND kind=$2").bind(team_id).bind(kind).execute(&pool).await.unwrap();
            for (run, id) in [
                (AsyncRun::BracketRace { race_id, async_part: 1 }, 5678),
                (AsyncRun::Qualifier { team_id, async_kind: kind }, 5679),
            ] {
                let setup_transaction = pool.begin().await.unwrap();
                assert!(run.thread_id(&pool).await.unwrap().is_none());
                save_thread(&pool, &run, ChannelId::new(id)).await.unwrap();
                // Discord invitation failed; rolling back setup must not forget its thread.
                setup_transaction.rollback().await.unwrap();
                assert_eq!(run.thread_id(&pool).await.unwrap(), Some(id as i64));
                assert!(save_thread(&pool, &run, ChannelId::new(9999)).await.is_err());
                assert_eq!(run.thread_id(&pool).await.unwrap(), Some(id as i64));
            }
        }).catch_unwind().await;
        sqlx::query("UPDATE races SET async_thread1=$2 WHERE id=$1").bind(race_id).bind(old_thread).execute(&pool).await.unwrap();
        sqlx::query("UPDATE async_teams SET discord_thread=$3 WHERE team=$1 AND kind=$2").bind(team_id).bind(kind).bind(old_qualifier_thread).execute(&pool).await.unwrap();
        if let Err(panic) = result { std::panic::resume_unwind(panic); }
    }
}
