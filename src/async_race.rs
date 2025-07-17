use {
    chrono::{DateTime, Utc},
    serenity::all::{
        ChannelId, CreateThread, MessageBuilder,
        ChannelType, AutoArchiveDuration,
    },
    sqlx::{PgPool, Transaction, Postgres},

    crate::{
        cal::{Race, RaceSchedule},
        event::Data as EventData,
        prelude::*,
        team::Team,
        user::User,
        seed,
    },
};

pub(crate) struct AsyncRaceManager;

impl AsyncRaceManager {
    /// Creates async threads 45 minutes before the scheduled start time
    pub(crate) async fn create_async_threads(
        pool: &PgPool,
        discord_ctx: &DiscordCtx,
        _http_client: &reqwest::Client,
    ) -> Result<(), Error> {
        let mut transaction = pool.begin().await?;
        
        // Find races that need async threads created
        let races = Self::get_races_needing_threads(&mut transaction).await?;
        
        for race in races {
            let event = EventData::new(&mut transaction, race.series, &race.event)
                .await
                .map_err(|e| Error::Event(event::Error::Data(e)))?
                .ok_or(Error::EventNotFound)?;
            
            if let Some(async_channel) = event.discord_async_channel {
                for (async_part, start_time) in Self::get_async_parts(&race) {
                    if let Some(start_time) = start_time {
                        let time_until_start = start_time - Utc::now();
                        // Only create the thread for this part if:
                        // - The thread does not exist
                        // - The start time is in the future and less than 45 minutes away
                        let thread_exists = match async_part {
                            1 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_thread1 IS NOT NULL) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                            2 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_thread2 IS NOT NULL) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                            3 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_thread3 IS NOT NULL) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                            _ => false,
                        };
                        if !thread_exists && time_until_start > chrono::Duration::zero() && time_until_start <= chrono::Duration::minutes(45) {
                            Self::create_async_thread(
                                &mut transaction,
                                discord_ctx,
                                &event,
                                &race,
                                async_part,
                                start_time,
                                async_channel,
                            ).await?;
                        }
                    }
                }
            }
        }
        
        transaction.commit().await?;
        Ok(())
    }

    /// Distributes seeds 10 minutes before the scheduled start time
    pub(crate) async fn distribute_seeds(
        pool: &PgPool,
        discord_ctx: &DiscordCtx,
        _http_client: &reqwest::Client,
    ) -> Result<(), Error> {
        let mut transaction = pool.begin().await?;
        
        // Find races that need seeds distributed
        let races = Self::get_races_needing_seeds(&mut transaction).await?;
        
        for race in races {
            let event = EventData::new(&mut transaction, race.series, &race.event)
                .await
                .map_err(|e| Error::Event(event::Error::Data(e)))?
                .ok_or(Error::EventNotFound)?;
            
            for (async_part, start_time) in Self::get_async_parts(&race) {
                if let Some(start_time) = start_time {
                    let time_until_start = start_time - Utc::now();
                    // Only distribute the seed for this part if:
                    // - The seed has not been distributed
                    // - The start time is in the future and less than 10 minutes away
                    let seed_distributed = match async_part {
                        1 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_seed1 = TRUE) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                        2 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_seed2 = TRUE) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                        3 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_seed3 = TRUE) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                        _ => false,
                    };
                    if !seed_distributed && time_until_start > chrono::Duration::zero() && time_until_start <= chrono::Duration::minutes(10) {
                        Self::distribute_seed_to_thread(
                            &mut transaction,
                            discord_ctx,
                            &event,
                            &race,
                            async_part,
                        ).await?;
                    }
                }
            }
        }
        
        transaction.commit().await?;
        Ok(())
    }

    /// Creates a private thread for an async race part
    async fn create_async_thread(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        event: &EventData<'_>,
        race: &Race,
        async_part: u8,
        start_time: DateTime<Utc>,
        async_channel: ChannelId,
    ) -> Result<(), Error> {
        let team = Self::get_team_for_async_part(race, async_part)?;
        let player = team.members(transaction).await?.into_iter().next()
            .ok_or(Error::NoTeamMembers)?;
        
        // Determine if this is first or second half based on scheduled start times
        let is_first_half = Self::is_first_half(race, async_part, start_time);
        
        // Build matchup string
        let teams: Vec<_> = race.teams().collect();
        let mut matchup = String::new();
        for (i, _team) in teams.iter().enumerate() {
            if i > 0 {
                matchup.push_str("v");
            }
            matchup.push_str(&format!("P{}", i + 1));
        }
        
        // Get player name
        let player_name = player.display_name();
        
        // Build thread name: Async <Round>: <player> (<1st/2nd>) if round/phase exists, else Async <Matchup>: <player> (<1st/2nd>)
        let thread_name = if race.phase.is_some() || race.round.is_some() {
            let round_str = if let Some(phase) = &race.phase {
                if let Some(round) = &race.round {
                    format!("{} {}", phase, round)
                } else {
                    phase.clone()
                }
            } else if let Some(round) = &race.round {
                round.clone()
            } else {
                String::new()
            };
            format!("Async {}: {} ({})", round_str.trim(), player_name, if is_first_half { "1st" } else { "2nd" })
        } else {
            format!("Async {}: {} ({})", matchup, player_name, if is_first_half { "1st" } else { "2nd" })
        };
        
        let mut content = Self::build_async_thread_content(
            transaction,
            event,
            race,
            async_part,
            start_time,
            &player,
            is_first_half,
        ).await?;
        
        let thread = async_channel.create_thread(discord_ctx, CreateThread::new(&thread_name)
            .kind(ChannelType::PrivateThread)
            .auto_archive_duration(AutoArchiveDuration::OneWeek)
        ).await?;
        
        // Store thread ID in database
        let thread_id = thread.id.get() as i64;
        match async_part {
            1 => sqlx::query!("UPDATE races SET async_thread1 = $1 WHERE id = $2", thread_id, race.id as _).execute(&mut **transaction).await?,
            2 => sqlx::query!("UPDATE races SET async_thread2 = $1 WHERE id = $2", thread_id, race.id as _).execute(&mut **transaction).await?,
            3 => sqlx::query!("UPDATE races SET async_thread3 = $1 WHERE id = $2", thread_id, race.id as _).execute(&mut **transaction).await?,
            _ => return Ok(()),
        };
        
        thread.say(discord_ctx, content.build()).await?;
        
        // Add organizers and player to thread
        let organizers = event.organizers(transaction).await.map_err(Error::Event)?;
        for organizer in organizers {
            if let Some(discord) = &organizer.discord {
                let _ = thread.guild_id.member(discord_ctx, discord.id).await;
            }
        }
        
        if let Some(discord) = &player.discord {
            let _ = thread.guild_id.member(discord_ctx, discord.id).await;
        }
        
        Ok(())
    }

    /// Builds the content for the async thread
    async fn build_async_thread_content(
        transaction: &mut Transaction<'_, Postgres>,
        event: &EventData<'_>,
        race: &Race,
        _async_part: u8,
        start_time: DateTime<Utc>,
        player: &User,
        is_first_half: bool,
    ) -> Result<MessageBuilder, Error> {
        let mut content = MessageBuilder::default();
        
        // Header with player mention and race info
        content.push("Hey ");
        content.mention_user(player);
        content.push(", this thread will be used to handle your part of the async for this race: ");
        
        if let Some(phase) = &race.phase {
            content.push_safe(phase.clone());
            content.push(' ');
        }
        if let Some(round) = &race.round {
            content.push_safe(round.clone());
            content.push(' ');
        }
        
        content.push("(");
        let teams: Vec<_> = race.teams().collect();
        for (i, team) in teams.iter().enumerate() {
            if team.id == Self::get_team_for_async_part(race, async_part)?.id {
                // Mention (ping) the current player's team
                content.mention_team(transaction, event.discord_guild, team).await?;
            } else {
                // Just show the opponent's team name without pinging
                content.push_safe(team.name(transaction).await?.unwrap_or_else(|| "Unknown Team".to_string()));
            }
            if i < teams.len() - 1 {
                content.push(" vs. ");
            }
        }
        content.push(")");
        
        content.push_line("");
        content.push("You are considered Player ");
        content.push(if is_first_half { "1" } else { "2" });
        content.push(" of this round.");
        
        content.push_line("");
        content.push("The bot will hand out your seed 10 minutes before the designated start of <t:");
        content.push(start_time.timestamp().to_string());
        content.push(":F>.");
        
        content.push_line("");
        content.push("Please note in the interest of keeping a level playing field, even if running second, you will not be given the results of the match until after both players have run the seed.");
        
        // Instructions based on whether player goes first or second
        if is_first_half {
            content.push_line("");
            content.push("**First Player Instructions:**");
            content.push_line("");
            content.push("• Local record from OBS and upload to YouTube as unlisted.");
            content.push_line("");
            content.push("• When finished, inform us immediately with your finish time and a screenshot of the collection rate end scene.");
            content.push_line("");
            content.push("• We suggest using MKV format for recording (more crash-resistant than MP4).");
        } else {
            content.push_line("");
            content.push("**Second Player Instructions:**");
            content.push_line("");
            content.push("• You can stream to Twitch/YouTube OR local record and upload to YouTube as unlisted.");
            content.push_line("");
            content.push("• When finished, inform us immediately with your finish time and a screenshot of the collection rate end scene.");
            content.push_line("");
            content.push("• If streaming to Twitch, ensure VoDs are published for access for the organizers.");
        }
        
        Ok(content)
    }

    /// Distributes seed to a specific async thread
    async fn distribute_seed_to_thread(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        _event: &EventData<'_>,
        race: &Race,
        async_part: u8,
    ) -> Result<(), Error> {
        let seed_url = Self::get_seed_url(race)?;
        
        let mut content = MessageBuilder::default();
        content.push("**Seed Distribution**");
        content.push_line("");
        content.push("Your seed is ready! Please use this URL: ");
        content.push(seed_url);
        content.push_line("");
        content.push("Good luck with your run!");
        
        // Get thread ID from database
        let thread_id = match async_part {
            1 => sqlx::query_scalar!("SELECT async_thread1 FROM races WHERE id = $1", race.id as _).fetch_one(&mut **transaction).await?,
            2 => sqlx::query_scalar!("SELECT async_thread2 FROM races WHERE id = $1", race.id as _).fetch_one(&mut **transaction).await?,
            3 => sqlx::query_scalar!("SELECT async_thread3 FROM races WHERE id = $1", race.id as _).fetch_one(&mut **transaction).await?,
            _ => return Err(Error::InvalidAsyncPart),
        };
        
        if let Some(thread_id) = thread_id {
            let thread = ChannelId::new(thread_id as u64);
            thread.say(discord_ctx, content.build()).await?;
            
            // Mark seed as distributed
            match async_part {
                1 => sqlx::query!("UPDATE races SET async_seed1 = TRUE WHERE id = $1", race.id as _).execute(&mut **transaction).await?,
                2 => sqlx::query!("UPDATE races SET async_seed2 = TRUE WHERE id = $1", race.id as _).execute(&mut **transaction).await?,
                3 => sqlx::query!("UPDATE races SET async_seed3 = TRUE WHERE id = $1", race.id as _).execute(&mut **transaction).await?,
                _ => return Ok(()),
            };
        }
        
        Ok(())
    }



    /// Gets async parts and their start times
    fn get_async_parts(race: &Race) -> Vec<(u8, Option<DateTime<Utc>>)> {
        match &race.schedule {
            RaceSchedule::Async { start1, start2, start3, .. } => {
                vec![
                    (1, *start1),
                    (2, *start2),
                    (3, *start3),
                ]
            }
            _ => vec![],
        }
    }

    /// Gets the team for a specific async part
    fn get_team_for_async_part(race: &Race, async_part: u8) -> Result<&Team, Error> {
        let teams: Vec<_> = race.teams().collect();
        match async_part {
            1 => teams.get(0).copied().ok_or(Error::NoTeamFound),
            2 => teams.get(1).copied().ok_or(Error::NoTeamFound),
            3 => teams.get(2).copied().ok_or(Error::NoTeamFound),
            _ => Err(Error::InvalidAsyncPart),
        }
    }

    /// Determines if this async part is the first half based on scheduled start times
    fn is_first_half(race: &Race, async_part: u8, _start_time: DateTime<Utc>) -> bool {
        match &race.schedule {
            RaceSchedule::Async { start1, start2, start3, .. } => {
                // Get all scheduled start times that are not None
                let mut scheduled_times = Vec::new();
                if let Some(time) = start1 { scheduled_times.push((1, *time)); }
                if let Some(time) = start2 { scheduled_times.push((2, *time)); }
                if let Some(time) = start3 { scheduled_times.push((3, *time)); }
                
                // Sort by start time
                scheduled_times.sort_by_key(|&(_, time)| time);
                
                // Find the position of this async part in the sorted list
                if let Some(position) = scheduled_times.iter().position(|&(part, _)| part == async_part) {
                    position == 0 // First position (earliest time) = first half
                } else {
                    // Fallback to async_part number if not found
                    async_part == 1
                }
            }
            _ => async_part == 1, // Fallback
        }
    }

    /// Gets the seed URL for a race
    fn get_seed_url(race: &Race) -> Result<String, Error> {
        // Check if race has a seed
        if let Some(seed_files) = &race.seed.files {
            match seed_files {
                seed::Files::AlttprDoorRando { uuid } => {
                    let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html")?;
                    patcher_url.query_pairs_mut().append_pair("patch", &format!("https://hth.zeldaspeedruns.com/seed/DR_{uuid}.bps"));
                    Ok(patcher_url.to_string())
                }
                _ => Err(Error::UnsupportedSeedType),
            }
        } else {
            Err(Error::NoSeedAvailable)
        }
    }

    /// Gets races that need async threads created
    async fn get_races_needing_threads(
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<Race>, Error> {
        let race_rows = sqlx::query!(
            r#"
            SELECT r.id, r.series, r.event
            FROM races r
            JOIN events e ON r.series = e.series AND r.event = e.event
            WHERE e.discord_async_channel IS NOT NULL
            AND (r.async_start1 IS NOT NULL OR r.async_start2 IS NOT NULL OR r.async_start3 IS NOT NULL)
            AND (r.async_thread1 IS NULL OR r.async_thread2 IS NULL OR r.async_thread3 IS NULL)
            AND (
                (r.async_start1 IS NOT NULL AND r.async_thread1 IS NULL AND r.async_start1 <= NOW() + INTERVAL '45 minutes' AND r.async_start1 > NOW() + INTERVAL '44 minutes') OR
                (r.async_start2 IS NOT NULL AND r.async_thread2 IS NULL AND r.async_start2 <= NOW() + INTERVAL '45 minutes' AND r.async_start2 > NOW() + INTERVAL '44 minutes') OR
                (r.async_start3 IS NOT NULL AND r.async_thread3 IS NULL AND r.async_start3 <= NOW() + INTERVAL '45 minutes' AND r.async_start3 > NOW() + INTERVAL '44 minutes') OR
                (r.async_start1 IS NOT NULL AND r.async_thread1 IS NULL AND r.async_start1 <= NOW() + INTERVAL '30 minutes' AND r.async_start1 > NOW()) OR
                (r.async_start2 IS NOT NULL AND r.async_thread2 IS NULL AND r.async_start2 <= NOW() + INTERVAL '30 minutes' AND r.async_start2 > NOW()) OR
                (r.async_start3 IS NOT NULL AND r.async_thread3 IS NULL AND r.async_start3 <= NOW() + INTERVAL '30 minutes' AND r.async_start3 > NOW())
            )
            "#
        ).fetch_all(&mut **transaction).await?;
        let mut races = Vec::new();
        for race_row in race_rows {
            let race = Race::from_id(transaction, &reqwest::Client::new(), Id::from(race_row.id)).await?;
            races.push(race);
        }
        Ok(races)
    }

    /// Gets races that need seeds distributed
    async fn get_races_needing_seeds(
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<Race>, Error> {
        let race_rows = sqlx::query!(
            r#"
            SELECT r.id, r.series, r.event
            FROM races r
            JOIN events e ON r.series = e.series AND r.event = e.event
            WHERE e.discord_async_channel IS NOT NULL
            AND (r.async_start1 IS NOT NULL OR r.async_start2 IS NOT NULL OR r.async_start3 IS NOT NULL)
            AND (
                (r.async_start1 IS NOT NULL AND r.async_seed1 = FALSE AND r.async_start1 <= NOW() + INTERVAL '10 minutes' AND r.async_start1 > NOW() + INTERVAL '9 minutes') OR
                (r.async_start2 IS NOT NULL AND r.async_seed2 = FALSE AND r.async_start2 <= NOW() + INTERVAL '10 minutes' AND r.async_start2 > NOW() + INTERVAL '9 minutes') OR
                (r.async_start3 IS NOT NULL AND r.async_seed3 = FALSE AND r.async_start3 <= NOW() + INTERVAL '10 minutes' AND r.async_start3 > NOW() + INTERVAL '9 minutes') OR
                (r.async_start1 IS NOT NULL AND r.async_seed1 = FALSE AND r.async_start1 <= NOW() + INTERVAL '5 minutes' AND r.async_start1 > NOW()) OR
                (r.async_start2 IS NOT NULL AND r.async_seed2 = FALSE AND r.async_start2 <= NOW() + INTERVAL '5 minutes' AND r.async_start2 > NOW()) OR
                (r.async_start3 IS NOT NULL AND r.async_seed3 = FALSE AND r.async_start3 <= NOW() + INTERVAL '5 minutes' AND r.async_start3 > NOW())
            )
            "#
        ).fetch_all(&mut **transaction).await?;
        let mut races = Vec::new();
        for race_row in race_rows {
            let race = Race::from_id(transaction, &reqwest::Client::new(), Id::from(race_row.id)).await?;
            races.push(race);
        }
        Ok(races)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Discord(#[from] serenity::Error),
    #[error(transparent)] UrlParse(#[from] url::ParseError),
    #[error("event error: {0}")]
    Event(event::Error),
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error("event not found")]
    EventNotFound,
    #[error("no team found")]
    NoTeamFound,
    #[error("no team members")]
    NoTeamMembers,
    #[error("invalid async part")]
    InvalidAsyncPart,
    #[error("no seed available")]
    NoSeedAvailable,
    #[error("unsupported seed type")]
    UnsupportedSeedType,
} 