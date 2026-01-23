use {
    crate::{
        cal::{Race, RaceSchedule, Entrants},
        discord_bot::PgSnowflake,
        event::Data as EventData,
        prelude::*,
    },
    chrono::TimeDelta,
    serenity::all::{
        CreateScheduledEvent,
        EditScheduledEvent,
        ScheduledEventType,
        Timestamp,
    },
    sqlx::{Transaction, Postgres},
    std::borrow::Cow,
};

pub(crate) type DiscordCtx = serenity::all::Context;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] DiscordBot(#[from] crate::discord_bot::Error),
    #[error("Event does not have Discord guild configured")]
    NoDiscordGuild,
}

/// Check if a race should have a Discord scheduled event based on configuration
pub(crate) fn should_create_discord_event(
    race: &Race,
    event_config: &EventData<'_>,
) -> bool {
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

/// Generate Discord event title from race data
async fn generate_event_title(
    race: &Race,
    _event_config: &EventData<'_>,
    transaction: &mut Transaction<'_, Postgres>,
    ctx: &DiscordCtx,
) -> Result<String, Error> {
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
            let p1_name = p1.name(transaction, ctx).await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p2_name = p2.name(transaction, ctx).await?
                .unwrap_or(Cow::Borrowed("TBD"));
            title.push_str(&format!("{} vs {}", p1_name, p2_name));
        }
        Entrants::Three([p1, p2, p3]) => {
            let p1_name = p1.name(transaction, ctx).await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p2_name = p2.name(transaction, ctx).await?
                .unwrap_or(Cow::Borrowed("TBD"));
            let p3_name = p3.name(transaction, ctx).await?
                .unwrap_or(Cow::Borrowed("TBD"));
            title.push_str(&format!("{} vs {} vs {}", p1_name, p2_name, p3_name));
        }
        Entrants::Named(name) => {
            title.push_str(name);
        }
        Entrants::Open | Entrants::Count { .. } => {
            title.push_str("Open Race");
        }
    }

    // Add game number if applicable
    if let Some(game) = race.game {
        title.push_str(&format!(" (Game {})", game));
    }

    Ok(title)
}

/// Generate Discord event description
fn generate_event_description(
    race: &Race,
    event_config: &EventData<'_>,
) -> String {
    let mut desc = String::new();

    // Add event name
    desc.push_str(&format!("**{}**\n\n", event_config.display_name));

    // Add phase/round details
    if let (Some(phase), Some(round)) = (&race.phase, &race.round) {
        desc.push_str(&format!("Phase: {}\nRound: {}\n\n", phase, round));
    }

    // Add restream links if available
    let restream_links: Vec<String> = race.video_urls.iter()
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
            let channel_info = url.path()
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(|channel| format!(" ({})", channel))
                .unwrap_or_default();

            format!("[{} Restream{}{}]({})", lang_str, platform, channel_info, url)
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

/// Create a Discord scheduled event for a race
pub(crate) async fn create_discord_scheduled_event(
    ctx: &DiscordCtx,
    transaction: &mut Transaction<'_, Postgres>,
    race: &mut Race,
    event_config: &EventData<'_>,
) -> Result<(), Error> {
    let guild_id = event_config.discord_guild.ok_or(Error::NoDiscordGuild)?;

    let RaceSchedule::Live { start, .. } = race.schedule else {
        return Ok(()); // Only create for live races
    };

    if !should_create_discord_event(race, event_config) {
        return Ok(());
    }

    // If event already exists, update it instead
    if race.discord_scheduled_event_id.is_some() {
        return update_discord_scheduled_event(ctx, transaction, race, event_config).await;
    }

    // Generate event content
    let title = generate_event_title(race, event_config, transaction, ctx).await?;
    let description = generate_event_description(race, event_config);

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
    .location(event_config.url.as_ref().map(|u| u.to_string()).unwrap_or_else(|| "https://ootrandomizer.com".to_string()));

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
) -> Result<(), Error> {
    let guild_id = event_config.discord_guild.ok_or(Error::NoDiscordGuild)?;
    let event_id = match &race.discord_scheduled_event_id {
        Some(PgSnowflake(id)) => *id,
        None => return Ok(()), // No event to update
    };

    let RaceSchedule::Live { start, .. } = race.schedule else {
        // Schedule changed from live to async/unscheduled, delete the event
        return delete_discord_scheduled_event(ctx, transaction, &mut race.clone(), event_config).await;
    };

    if !should_create_discord_event(race, event_config) {
        // No longer meets criteria, delete the event
        return delete_discord_scheduled_event(ctx, transaction, &mut race.clone(), event_config).await;
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
        // Event has started, completed, been cancelled, or doesn't exist - delete and recreate
        // Delete the old event (if it still exists)
        let _ = guild_id.delete_scheduled_event(&ctx.http, event_id).await;

        // Clear our stored ID
        sqlx::query!("UPDATE races SET discord_scheduled_event_id = NULL WHERE id = $1", race.id as _)
            .execute(&mut **transaction)
            .await?;

        // Create a new event (inline to avoid recursion)
        let title = generate_event_title(race, event_config, transaction, ctx).await?;
        let description = generate_event_description(race, event_config);
        let end_time = start + TimeDelta::hours(3);

        let builder = CreateScheduledEvent::new(
            ScheduledEventType::External,
            title,
            Timestamp::from_unix_timestamp(start.timestamp()).expect("valid timestamp"),
        )
        .description(description)
        .end_time(Timestamp::from_unix_timestamp(end_time.timestamp()).expect("valid timestamp"))
        .location(event_config.url.as_ref().map(|u| u.to_string()).unwrap_or_else(|| "https://ootrandomizer.com".to_string()));

        let scheduled_event = guild_id.create_scheduled_event(&ctx.http, builder).await?;

        // Store the new event ID in database
        sqlx::query!("UPDATE races SET discord_scheduled_event_id = $1 WHERE id = $2",
            PgSnowflake(scheduled_event.id) as _, race.id as _)
            .execute(&mut **transaction)
            .await?;

        return Ok(());
    }

    // Generate updated content
    let title = generate_event_title(race, event_config, transaction, ctx).await?;
    let description = generate_event_description(race, event_config);

    let end_time = start + TimeDelta::hours(3);

    // Update the event
    let builder = EditScheduledEvent::new()
        .name(title)
        .description(description)
        .start_time(Timestamp::from_unix_timestamp(start.timestamp()).expect("valid timestamp"))
        .end_time(Timestamp::from_unix_timestamp(end_time.timestamp()).expect("valid timestamp"));

    guild_id.edit_scheduled_event(&ctx.http, event_id, builder).await?;

    Ok(())
}

/// Delete a Discord scheduled event
pub(crate) async fn delete_discord_scheduled_event(
    ctx: &DiscordCtx,
    _transaction: &mut Transaction<'_, Postgres>,
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

    Ok(())
}
