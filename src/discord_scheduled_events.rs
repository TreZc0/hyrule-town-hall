use {
    crate::{
        cal::{Entrants, Race, RaceSchedule},
        discord_bot::PgSnowflake,
        event::Data as EventData,
        prelude::*,
    },
    chrono::TimeDelta,
    serenity::all::{CreateScheduledEvent, EditScheduledEvent, ScheduledEventType, Timestamp},
    sqlx::{Postgres, Transaction},
    std::borrow::Cow,
};

pub(crate) type DiscordCtx = serenity::all::Context;

const MAX_DISCORD_EVENT_TITLE_CHARS: usize = 100;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Serenity(#[from] serenity::Error),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Cal(#[from] cal::Error),
    #[error(transparent)]
    DiscordBot(#[from] crate::discord_bot::Error),
    #[error(transparent)]
    Event(#[from] event::DataError),
    #[error("Event does not have Discord guild configured")]
    NoDiscordGuild,
}

/// Check if a race should have a Discord scheduled event based on configuration
pub(crate) fn should_create_discord_event(race: &Race, event_config: &EventData<'_>) -> bool {
    // Feature must be enabled
    if !event_config.discord_events_enabled {
        return false;
    }

    // Must have Discord guild
    if event_config.discord_guild.is_none() {
        return false;
    }

    // Must be a live scheduled race (not async or unscheduled)
    if !matches!(race.schedule, RaceSchedule::Live { .. }) {
        return false;
    }

    // Check restream requirement
    if event_config.discord_events_require_restream {
        // Any restream URL counts (any language)
        !race.video_urls.is_empty()
    } else {
        true
    }
}

/// Resolve the location URL for a Discord event, falling back through:
/// restream URL → multistream URL → event URL
async fn resolve_event_location(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    race: &Race,
    event_config: &EventData<'_>,
) -> Result<String, Error> {
    let url = if !race.video_urls.is_empty() {
        race.video_urls
            .get(&event_config.language)
            .or_else(|| race.video_urls.values().next())
            .map(|u| u.to_string())
    } else if matches!(race.schedule, RaceSchedule::Live { room: Some(_), .. }) {
        race.multistream_url(transaction, http_client, event_config)
            .await?
            .map(|u| u.to_string())
    } else {
        race.multistream_url_prerace(transaction, http_client, event_config)
            .await?
            .map(|u| u.to_string())
    };

    let url = url.unwrap_or_else(|| {
        event_config
            .url
            .as_ref()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "https://ootrandomizer.com".to_string())
    });

    Ok(if url.len() > 100 {
        if url.contains("start.gg") {
            if let Some(event_pos) = url.find("/event/") {
                let base_url = &url[..event_pos];
                if base_url.len() <= 100 {
                    base_url.to_string()
                } else {
                    url.chars().take(97).collect::<String>() + "..."
                }
            } else {
                url.chars().take(97).collect::<String>() + "..."
            }
        } else {
            url.chars().take(97).collect::<String>() + "..."
        }
    } else {
        url
    })
}

fn discord_event_title(title: String) -> String {
    if title.chars().count() <= MAX_DISCORD_EVENT_TITLE_CHARS {
        title
    } else {
        format!(
            "{}...",
            title
                .chars()
                .take(MAX_DISCORD_EVENT_TITLE_CHARS - 3)
                .collect::<String>()
        )
    }
}

fn compact_title_label(race: &Race) -> String {
    let phase = race.phase.as_deref().map(|phase| {
        phase
            .replace("Winners", "W")
            .replace("Winner", "W")
            .replace("Losers", "L")
            .replace("Loser", "L")
            .replace("Lower", "L")
            .replace("Bracket", "")
            .replace("Quarterfinal", "QF")
            .replace("Semifinal", "SF")
            .replace("Final", "F")
            .split_whitespace()
            .join(" ")
    });
    let round = race.round.as_deref().map(|round| {
        round
            .replace("Round ", "R")
            .replace("Quarterfinal", "QF")
            .replace("Semifinal", "SF")
            .replace("Final", "F")
    });
    let game = race.game.map(|game| format!("G{game}"));
    [phase, round, game].into_iter().flatten().join(" ")
}

async fn compact_title_matchup(
    race: &Race,
    transaction: &mut Transaction<'_, Postgres>,
    ctx: &DiscordCtx,
) -> Result<String, Error> {
    Ok(match &race.entrants {
        Entrants::Two([p1, p2]) => {
            let p1_name = p1
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p2_name = p2
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            format!("{p1_name}/{p2_name}")
        }
        Entrants::Three([p1, p2, p3]) => {
            let p1_name = p1
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p2_name = p2
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p3_name = p3
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            format!("{p1_name}/{p2_name}/{p3_name}")
        }
        Entrants::Named(name) => name.clone(),
        Entrants::Open | Entrants::Count { .. } => "Open Race".to_owned(),
        Entrants::Many(teams) => format!("{} entrants", teams.len()),
    })
}

async fn compact_combined_event_title(
    race: &Race,
    companion: &Race,
    transaction: &mut Transaction<'_, Postgres>,
    ctx: &DiscordCtx,
) -> Result<String, Error> {
    let primary_label = compact_title_label(race);
    let companion_label = compact_title_label(companion);
    let primary_matchup = compact_title_matchup(race, transaction, ctx).await?;
    let companion_matchup = compact_title_matchup(companion, transaction, ctx).await?;
    Ok(if primary_label == companion_label {
        if primary_label.is_empty() {
            format!("{primary_matchup} & {companion_matchup}")
        } else {
            format!("{primary_label} - {primary_matchup} & {companion_matchup}")
        }
    } else {
        let primary = if primary_label.is_empty() {
            primary_matchup
        } else {
            format!("{primary_label} - {primary_matchup}")
        };
        let companion = if companion_label.is_empty() {
            companion_matchup
        } else {
            format!("{companion_label} - {companion_matchup}")
        };
        format!("{primary} & {companion}")
    })
}

/// Generate Discord event title from race data
async fn generate_event_title(
    race: &Race,
    companion: Option<&Race>,
    event_config: &EventData<'_>,
    transaction: &mut Transaction<'_, Postgres>,
    ctx: &DiscordCtx,
) -> Result<String, Error> {
    if let Some(custom_title) = &race.custom_title {
        let mut title = format!("{}: {}", event_config.display_name, custom_title);
        if let Some(companion) = companion {
            title.push_str(" / ");
            title.push_str(&companion.matchup_label(transaction, ctx).await?);
        }
        return Ok(discord_event_title(title));
    }

    if let Some(companion) = companion {
        return Ok(discord_event_title(
            compact_combined_event_title(race, companion, transaction, ctx).await?,
        ));
    }

    let mut title = String::new();

    // Add phase and round info if available
    if let (Some(phase), Some(round)) = (&race.phase, &race.round) {
        title.push_str(&format!("{} - {}", phase, round));
    } else if let Some(phase) = &race.phase {
        title.push_str(phase);
    } else if let Some(round) = &race.round {
        title.push_str(round);
    } else {
        title.push_str("Race");
    }

    // Add matchup info
    title.push_str(": ");
    match &race.entrants {
        Entrants::Two([p1, p2]) => {
            let p1_name = p1
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p2_name = p2
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            title.push_str(&format!("{} vs {}", p1_name, p2_name));
        }
        Entrants::Three([p1, p2, p3]) => {
            let p1_name = p1
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p2_name = p2
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p3_name = p3
                .name(transaction, ctx)
                .await?
                .unwrap_or(Cow::Borrowed("TBD"));
            title.push_str(&format!("{} vs {} vs {}", p1_name, p2_name, p3_name));
        }
        Entrants::Named(name) => {
            title.push_str(name);
        }
        Entrants::Open | Entrants::Count { .. } => {
            title.push_str("Open Race");
        }
        Entrants::Many(teams) => {
            title.push_str(&format!("{} entrants", teams.len()));
        }
    }

    // Add game number if applicable
    if let Some(game) = race.game {
        title.push_str(&format!(" (Game {})", game));
    }
    if let Some(companion) = companion {
        title.push_str(" / ");
        title.push_str(&companion.matchup_label(transaction, ctx).await?);
    }

    Ok(discord_event_title(title))
}

/// Generate Discord event description
async fn generate_event_description(
    race: &Race,
    companion: Option<&Race>,
    event_config: &EventData<'_>,
    transaction: &mut Transaction<'_, Postgres>,
    ctx: &DiscordCtx,
) -> Result<String, Error> {
    let mut desc = event_description(race, event_config);
    if let Some(companion) = companion {
        desc.push_str("\n**Shared race room:**\n");
        desc.push_str(&format!(
            "This scheduled event covers this race and {} for restream purposes.\n",
            companion.matchup_label(transaction, ctx).await?
        ));
        desc.push_str("Only each runner's originally assigned matchup result counts for tournament progression.\n");
    }
    Ok(desc)
}

/// The same saved mode is rendered on initial creation and every later update.
pub(crate) fn event_description(race: &Race, event_config: &EventData<'_>) -> String {
    // Keep race details together immediately below the name for Discord's preview.
    let details = [
        race.completed_mode(event_config),
        race.phase.as_ref().map(|phase| format!("Phase: {phase}")),
        race.round.as_ref().map(|round| format!("Round: {round}")),
    ]
    .into_iter()
    .flatten()
    .join(" · ");
    let mut desc = format!("**{}**\n", event_config.display_name);
    if !details.is_empty() {
        desc.push_str(&MessageBuilder::default().push_safe(details).build());
        desc.push('\n');
    }
    desc.push('\n');

    // Add restream links if available
    let restream_links: Vec<String> = race
        .video_urls
        .iter()
        .map(|(lang, url)| {
            let lang_str = match lang {
                English => "EN",
                French => "FR",
                German => "DE",
                Portuguese => "PT",
            };

            // Detect platform from URL
            let platform = if let Some(host) = url.host_str() {
                if host.contains("twitch.tv") {
                    " on Twitch"
                } else if host.contains("youtube.com") || host.contains("youtu.be") {
                    " on YouTube"
                } else {
                    ""
                }
            } else {
                ""
            };

            // Extract channel name from URL
            let channel_info = url
                .path()
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(|channel| format!(" ({})", channel))
                .unwrap_or_default();

            format!(
                "[{} Restream{}{}]({})",
                lang_str, platform, channel_info, url
            )
        })
        .collect();

    if !restream_links.is_empty() {
        desc.push_str("**Restreams:**\n");
        for link in restream_links {
            desc.push_str(&format!("- {}\n", link));
        }
    }

    desc
}

async fn load_companion_race(
    transaction: &mut Transaction<'_, Postgres>,
    http_client: &reqwest::Client,
    race: &Race,
) -> Result<Option<Race>, Error> {
    Ok(if let Some(companion_race_id) = race.companion_race_id {
        Some(Race::from_id(transaction, http_client, companion_race_id).await?)
    } else {
        None
    })
}

/// Refresh metadata without changing the schedule or recreating an active event.
pub(crate) async fn refresh_mode_description(
    pool: &PgPool,
    ctx: &DiscordCtx,
    http_client: &reqwest::Client,
    race_id: Id<Races>,
) -> Result<(), Error> {
    let mut transaction = pool.begin().await?;
    let race = Race::from_id(&mut transaction, http_client, race_id).await?;
    let Some(event) = EventData::new(&mut transaction, race.series, &race.event).await? else {
        return Ok(());
    };
    if race.ignored
        || !should_create_discord_event(&race, &event)
        || !matches!(race.schedule, RaceSchedule::Live { start, .. } if start > Utc::now())
        || race.companion_primary_id(&mut transaction).await?.is_some()
    {
        return Ok(());
    }
    let (Some(guild), Some(PgSnowflake(event_id))) =
        (event.discord_guild, race.discord_scheduled_event_id)
    else {
        return Ok(());
    };
    let companion = load_companion_race(&mut transaction, http_client, &race).await?;
    let description =
        generate_event_description(&race, companion.as_ref(), &event, &mut transaction, ctx)
            .await?;
    transaction.commit().await?;

    let current = guild.scheduled_event(&ctx.http, event_id, false).await?;
    if current.status == ScheduledEventStatus::Scheduled
        && current.description.as_deref() != Some(description.as_str())
    {
        guild
            .edit_scheduled_event(
                &ctx.http,
                event_id,
                EditScheduledEvent::new().description(description),
            )
            .await?;
    }
    Ok(())
}

/// Scan a bounded batch each tick, including events whose drafts predate deployment.
/// Failed updates are retried on the next pass; unchanged descriptions need no edit.
pub(crate) async fn reconcile_mode_descriptions(
    pool: &PgPool,
    ctx: &DiscordCtx,
    http_client: &reqwest::Client,
    after: &mut i64,
) -> Result<(), Error> {
    let ids = sqlx::query_scalar::<_, i64>(
        "SELECT r.id FROM races r JOIN events e ON r.series = e.series AND r.event = e.event
         WHERE r.id > $1 AND NOT r.ignored AND r.start > NOW()
           AND r.discord_scheduled_event_id IS NOT NULL AND r.draft_state IS NOT NULL
           AND e.discord_events_enabled AND e.discord_guild IS NOT NULL
           AND e.draft_kind IN ('pick_only', 'ban_pick', 'ban_only')
         ORDER BY r.id LIMIT 100",
    )
    .bind(*after)
    .fetch_all(pool)
    .await?;
    *after = if ids.len() == 100 {
        *ids.last().unwrap()
    } else {
        i64::MIN
    };
    for id in ids {
        if let Err(error) = refresh_mode_description(pool, ctx, http_client, id.into()).await {
            log::warn!("Could not refresh mode for Discord event of race {id}: {error}");
        }
    }
    Ok(())
}

/// Create a Discord scheduled event for a race
pub(crate) async fn create_discord_scheduled_event(
    ctx: &DiscordCtx,
    transaction: &mut Transaction<'_, Postgres>,
    race: &mut Race,
    event_config: &EventData<'_>,
    http_client: &reqwest::Client,
) -> Result<(), Error> {
    let guild_id = event_config.discord_guild.ok_or(Error::NoDiscordGuild)?;

    let RaceSchedule::Live { start, .. } = race.schedule else {
        return Ok(()); // Only create for live races
    };

    if race.companion_primary_id(transaction).await?.is_some() {
        if race.discord_scheduled_event_id.is_some() {
            delete_discord_scheduled_event(ctx, transaction, race, event_config).await?;
        }
        return Ok(());
    }

    if !should_create_discord_event(race, event_config) {
        return Ok(());
    }

    // Don't create events that start in less than 5 minutes
    // Discord will immediately transition them to ACTIVE, causing issues
    if start < Utc::now() + TimeDelta::minutes(5) {
        return Ok(());
    }

    // If event already exists, update it instead
    if race.discord_scheduled_event_id.is_some() {
        return update_discord_scheduled_event(ctx, transaction, race, event_config, http_client)
            .await;
    }

    // Generate event content
    let companion = load_companion_race(transaction, http_client, race).await?;
    let title =
        generate_event_title(race, companion.as_ref(), event_config, transaction, ctx).await?;
    let description =
        generate_event_description(race, companion.as_ref(), event_config, transaction, ctx)
            .await?;
    let location = resolve_event_location(transaction, http_client, race, event_config).await?;

    // Calculate end time (start + 3 hours default)
    let end_time = start + TimeDelta::hours(3);

    // Create the Discord scheduled event
    let builder = CreateScheduledEvent::new(
        ScheduledEventType::External,
        title,
        Timestamp::from_unix_timestamp(start.timestamp()).expect("valid timestamp"),
    )
    .description(description)
    .end_time(Timestamp::from_unix_timestamp(end_time.timestamp()).expect("valid timestamp"))
    .location(location);

    let scheduled_event = guild_id.create_scheduled_event(&ctx.http, builder).await?;

    // Store the event ID
    race.discord_scheduled_event_id = Some(PgSnowflake(scheduled_event.id));

    Ok(())
}

/// Update an existing Discord scheduled event
pub(crate) async fn update_discord_scheduled_event(
    ctx: &DiscordCtx,
    transaction: &mut Transaction<'_, Postgres>,
    race: &Race,
    event_config: &EventData<'_>,
    http_client: &reqwest::Client,
) -> Result<(), Error> {
    let guild_id = event_config.discord_guild.ok_or(Error::NoDiscordGuild)?;
    let event_id = match &race.discord_scheduled_event_id {
        Some(PgSnowflake(id)) => *id,
        None => return Ok(()), // No event to update
    };

    let RaceSchedule::Live { start, .. } = race.schedule else {
        // Schedule changed from live to async/unscheduled, delete the event
        return delete_discord_scheduled_event(ctx, transaction, &mut race.clone(), event_config)
            .await;
    };

    if race.companion_primary_id(transaction).await?.is_some() {
        return delete_discord_scheduled_event(ctx, transaction, &mut race.clone(), event_config)
            .await;
    }

    if !should_create_discord_event(race, event_config) {
        // No longer meets criteria, delete the event
        return delete_discord_scheduled_event(ctx, transaction, &mut race.clone(), event_config)
            .await;
    }

    // Try to fetch the current event to check its state
    let current_event = guild_id.scheduled_event(&ctx.http, event_id, false).await;

    // Check if event exists and is in a state where we can update it
    // SCHEDULED = 1, ACTIVE = 2, COMPLETED = 3, CANCELLED = 4
    // We can only update SCHEDULED events
    use serenity::all::ScheduledEventStatus;
    let needs_recreate = match current_event {
        Ok(event) => event.status != ScheduledEventStatus::Scheduled,
        Err(_) => true, // Event doesn't exist anymore
    };

    if needs_recreate {
        // Event has started, completed, been cancelled, or doesn't exist
        // Delete the old event (if it still exists)
        let _ = guild_id.delete_scheduled_event(&ctx.http, event_id).await;

        // Clear our stored ID from database
        sqlx::query!(
            "UPDATE races SET discord_scheduled_event_id = NULL WHERE id = $1",
            race.id as _
        )
        .execute(&mut **transaction)
        .await?;

        // Only recreate if the new start time is at least 5 minutes in the future
        // Otherwise Discord will immediately transition it to ACTIVE, causing the same issue
        if start < Utc::now() + TimeDelta::minutes(5) {
            return Ok(());
        }

        // Create a new event
        let companion = load_companion_race(transaction, http_client, race).await?;
        let title =
            generate_event_title(race, companion.as_ref(), event_config, transaction, ctx).await?;
        let description =
            generate_event_description(race, companion.as_ref(), event_config, transaction, ctx)
                .await?;
        let location = resolve_event_location(transaction, http_client, race, event_config).await?;
        let end_time = start + TimeDelta::hours(3);

        let builder = CreateScheduledEvent::new(
            ScheduledEventType::External,
            title,
            Timestamp::from_unix_timestamp(start.timestamp()).expect("valid timestamp"),
        )
        .description(description)
        .end_time(Timestamp::from_unix_timestamp(end_time.timestamp()).expect("valid timestamp"))
        .location(location);

        let scheduled_event = guild_id.create_scheduled_event(&ctx.http, builder).await?;

        // Store the new event ID in database
        sqlx::query!(
            "UPDATE races SET discord_scheduled_event_id = $1 WHERE id = $2",
            PgSnowflake(scheduled_event.id) as _,
            race.id as _
        )
        .execute(&mut **transaction)
        .await?;

        return Ok(());
    }

    // Generate updated content
    let companion = load_companion_race(transaction, http_client, race).await?;
    let title =
        generate_event_title(race, companion.as_ref(), event_config, transaction, ctx).await?;
    let description =
        generate_event_description(race, companion.as_ref(), event_config, transaction, ctx)
            .await?;
    let location = resolve_event_location(transaction, http_client, race, event_config).await?;

    let end_time = start + TimeDelta::hours(3);

    let builder = EditScheduledEvent::new()
        .name(title)
        .description(description)
        .start_time(Timestamp::from_unix_timestamp(start.timestamp()).expect("valid timestamp"))
        .end_time(Timestamp::from_unix_timestamp(end_time.timestamp()).expect("valid timestamp"))
        .location(location);

    guild_id
        .edit_scheduled_event(&ctx.http, event_id, builder)
        .await?;

    Ok(())
}

/// Delete a Discord scheduled event
pub(crate) async fn delete_discord_scheduled_event(
    ctx: &DiscordCtx,
    transaction: &mut Transaction<'_, Postgres>,
    race: &mut Race,
    event_config: &EventData<'_>,
) -> Result<(), Error> {
    let guild_id = event_config.discord_guild.ok_or(Error::NoDiscordGuild)?;
    let event_id = match &race.discord_scheduled_event_id {
        Some(PgSnowflake(id)) => *id,
        None => return Ok(()), // No event to delete
    };

    // Delete from Discord (ignore errors if already deleted)
    let _ = guild_id.delete_scheduled_event(&ctx.http, event_id).await;

    // Clear the stored ID
    race.discord_scheduled_event_id = None;
    sqlx::query!(
        "UPDATE races SET discord_scheduled_event_id = NULL WHERE id = $1",
        race.id as _
    )
    .execute(&mut **transaction)
    .await?;

    Ok(())
}
