use {
    chrono::{DateTime, Utc},
    serenity::all::{
        ChannelId, CreateThread, MessageBuilder, CreateMessage,
        ChannelType, AutoArchiveDuration, CreateActionRow, CreateButton, ButtonStyle,
        Http, ComponentInteraction,
        CreateInteractionResponse, CreateInteractionResponseMessage,
        EditInteractionResponse,
        ActionRowComponent, ButtonKind,
    },
    sqlx::{PgPool, Transaction, Postgres},
    tokio::time::{sleep, Duration},

    crate::{
        cal::{Race, RaceSchedule},
        event::{self, AsyncKind, Data as EventData},
        prelude::*,
        team::Team,
        user::User,
        seed,
    },
};

pub(crate) struct AsyncRaceManager;

impl AsyncRaceManager {
    /// Creates async threads 30 minutes before the scheduled start time
    pub(crate) async fn create_async_threads(
        pool: &PgPool,
        discord_ctx: &DiscordCtx,
        _http_client: &reqwest::Client,
    ) -> Result<(), Error> {
        let mut transaction = pool.begin().await?;
        
        // Find races that need async threads created (bracket races)
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
                        // - The start time is in the future and less than 30 minutes away
                        let thread_exists = match async_part {
                            1 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_thread1 IS NOT NULL) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                            2 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_thread2 IS NOT NULL) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                            3 => sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1 AND async_thread3 IS NOT NULL) AS "exists!""#, race.id as _).fetch_one(&mut *transaction).await?,
                            _ => false,
                        };
                        if !thread_exists && time_until_start > chrono::Duration::zero() && time_until_start <= chrono::Duration::minutes(30) {
                            Self::create_async_thread(
                                &mut transaction,
                                discord_ctx,
                                &event,
                                &race,
                                async_part,
                                start_time,
                                async_channel,
                                pool,
                            ).await?;
                        }
                    }
                }
            }
        }
        
        // Also create threads for qualifier asyncs (automated_asyncs = true)
        Self::create_qualifier_threads(&mut transaction, discord_ctx, pool).await?;
        
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
        db_pool: &PgPool,
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
        let display_order = Self::get_display_order(race, async_part);
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
            format!("Async {}: {} ({})", round_str.trim(), player_name, if display_order == 1 { "1st" } else if display_order == 2 { "2nd" } else { "3rd" })
        } else {
            format!("Async {}: {} ({})", matchup, player_name, if display_order == 1 { "1st" } else if display_order == 2 { "2nd" } else { "3rd" })
        };
        
        let mut content = Self::build_async_thread_content(
            transaction,
            event,
            race,
            async_part,
            start_time,
            &player,
            is_first_half,
            db_pool,
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
        
        // Create the READY button
        let ready_button = CreateActionRow::Buttons(vec![
            CreateButton::new("async_ready")
                .label("READY!")
                .style(ButtonStyle::Primary)
        ]);

        // Send the initial message with the READY button
        thread.send_message(discord_ctx, CreateMessage::new()
            .content(content.build())
            .components(vec![ready_button])
        ).await?;
        
        // Add organizers and player to thread (but exclude organizers who are opponents)
        let organizers = event.organizers(transaction).await.map_err(Error::Event)?;
        let current_team = Self::get_team_for_async_part(race, async_part)?;
        
        // Track which Discord users we've already added to avoid duplicates
        let mut added_users = HashSet::new();
        
        // First, add the current player
        if let Some(discord) = &player.discord {
            if let Ok(member) = thread.guild_id.member(discord_ctx, discord.id).await {
                let _ = thread.id.add_thread_member(discord_ctx, member.user.id).await;
                added_users.insert(discord.id);
            }
        }
        
        // Then add organizers who are not opponents
        for organizer in organizers {
            if let Some(discord) = &organizer.discord {
                // Skip if we already added this user (e.g., if they're the current player)
                if added_users.contains(&discord.id) {
                    continue;
                }
                
                // Check if this organizer is part of any opponent team (not the current team)
                let mut is_opponent = false;
                for team in race.teams() {
                    if team.id == current_team.id {
                        continue; // Skip current team
                    } else {
                        // Check if organizer is a member of this opponent team
                        if let Ok(members) = team.members(transaction).await {
                            if members.iter().any(|member| {
                                member.discord.as_ref().map(|d| d.id) == Some(discord.id)
                            }) {
                                is_opponent = true;
                                break;
                            }
                        }
                    }
                }
                
                // Only add organizers who are NOT opponents
                if !is_opponent {
                    if let Ok(member) = thread.guild_id.member(discord_ctx, discord.id).await {
                        let _ = thread.id.add_thread_member(discord_ctx, member.user.id).await;
                        added_users.insert(discord.id);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Builds the content for the async thread
    async fn build_async_thread_content(
        transaction: &mut Transaction<'_, Postgres>,
        event: &EventData<'_>,
        race: &Race,
        async_part: u8,
        _start_time: DateTime<Utc>,
        player: &User,
        _is_first_half: bool,
        db_pool: &PgPool,
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
                content.push_safe(team.name(transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()));
            }
            if i < teams.len() - 1 {
                content.push(" vs. ");
            }
        }
        content.push(")");
        
        content.push_line("");
        content.push("You are considered Player ");
        let display_order = Self::get_display_order(race, async_part);
        content.push(display_order.to_string());
        content.push(" of this round.");

        // Add settings information for Crosskeys2025 events only
        let goal = racetime_bot::Goal::for_event(race.series, &race.event).expect("Goal not found for event");
        if matches!(goal, racetime_bot::Goal::Crosskeys2025) {
            let crosskeys_options = racetime_bot::CrosskeysRaceOptions::for_race(db_pool, race).await;

            content.push_line("");
            content.push_line("");
            content.push("---");
            content.push_line("");
            content.push(format!("**Seed Settings:** {}", crosskeys_options.as_seed_options_str()));
            content.push_line("");

            // Use the new method that excludes delay setting
            let race_options_str = crosskeys_options.as_race_options_str_no_delay();

            content.push(format!("**Race Rules:** {}", race_options_str));
            content.push_line("");
            content.push("---");
        }

        // Add round mode and settings information for AlttprDe events (9swissX and 9bracket)
        if race.series == Series::AlttprDe {
            let alttprde_options = racetime_bot::AlttprDeRaceOptions::for_race(db_pool, race, event.round_modes.as_ref()).await;

            content.push_line("");
            content.push_line("");
            content.push("---");
            content.push_line("");

            // Display round mode for swiss events (9swissX)
            if let Some(ref round) = race.round {
                if event.round_modes.is_some() {
                    // This is a swiss event with fixed round modes
                    if let Some(mode_display) = alttprde_options.mode_display() {
                        content.push(format!("**Round Mode:** {} - {}", round, mode_display));
                    } else {
                        content.push(format!("**Round Mode:** {} - not yet set", round));
                    }
                    content.push_line("");
                }
            }

            // Display mode and custom settings for bracket events (9bracket)
            if event.event == "9bracket" {
                if let Some(mode_display) = alttprde_options.mode_display() {
                    content.push(format!("**Mode:** {}", mode_display));
                    content.push_line("");
                }

                // Display custom settings picked by both runners
                if !alttprde_options.custom_choices.is_empty() {
                    content.push("**Settings chosen by both runners:** ");
                    let choices = alttprde_options.custom_choices_labels();
                    content.push(choices.join(", "));
                    content.push_line("");
                }
            }

            content.push("---");
        }
        
        content.push_line("");
        content.push("Click the **READY!** button below when you are ready to receive your seed. Once you click ready:");
        content.push_line("");
        content.push("• The seed will be posted immediately");
        content.push_line("");
        content.push("• Organizers will be notified that you are starting");
        content.push_line("");
        content.push("• You will see a button to start a 5-second countdown for your seed.");
        content.push_line("");
        content.push("• After the countdown, start to play. Once done, press the Finish button.");
        content.push_line("");
        content.push("To maintain fairness, the final match results will only be shared after both players have completed the seed and organizers have confirmed the results.");
        
        // Instructions based on display order
        let is_alttpr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'alttpr'
            ) AS "is_alttpr!""#,
            event.series as _
        ).fetch_one(&mut **transaction).await.unwrap_or(false);
        let is_twwr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'twwr'
            ) AS "is_twwr!""#,
            event.series as _
        ).fetch_one(&mut **transaction).await.unwrap_or(false);
        let display_order = Self::get_display_order(race, async_part);
        if display_order == 1 {
            content.push_line("");
            content.push("**First Player Instructions:**");
            content.push_line("");
            content.push("• Local record from OBS and upload to YouTube as unlisted.");
            content.push_line("");
            if is_alttpr {
                content.push("• When finished, inform us immediately with your finish time and a screenshot of the collection rate end scene.");
            } else if is_twwr {
                content.push("• When finished, inform us immediately with a screenshot showing your finish time together with the sword in Ganondorf's head.");
            } else {
                content.push("• When finished, inform us immediately and provide a screenshot showing your final time and an indicator of seed completion.");
            }
            content.push_line("");
            content.push("• We suggest using MKV format for recording (more crash-resistant than MP4).");
        } else {
            content.push_line("");
            content.push("**Second Player Instructions:**");
            content.push_line("");
            content.push("• You can stream to Twitch/YouTube OR local record and upload to YouTube as unlisted.");
            content.push_line("");
            if is_alttpr {
                content.push("• When finished, inform us immediately with your finish time and a screenshot of the collection rate end scene.");
            } else if is_twwr {
                content.push("• When finished, inform us immediately with your finish time and a screenshot while showing the sword in Ganondorf's head.");
            } else {
                content.push("• When finished, inform us immediately and provide a screenshot showing your final time and an indicator of seed completion.");
            }
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
        
        // Get the player for this async part
        let team = Self::get_team_for_async_part(race, async_part)?;
        let player = team.members(transaction).await?.into_iter().next()
            .ok_or(Error::NoTeamMembers)?;
        
        let mut content = MessageBuilder::default();
        content.push("Hey ");
        content.mention_user(&player);
        content.push(", ");
        content.push_line("");
        match &race.seed.files {
            Some(seed::Files::TwwrPermalink { permalink, seed_hash }) => {
                content.push(format!("Your seed is ready! Permalink: {permalink}"));
                if !seed_hash.is_empty() {
                    content.push_line("");
                    content.push(format!("Seed Hash: {seed_hash}"));
                }
            }
            _ => {
                content.push("Your seed is ready! Please use this URL: ");
                content.push(&seed_url);
                if let Some(file_hash) = race.seed.file_hash.as_ref() {
                    content.push_line("");
                    content.push("The hash for this seed is: ");
                    content.push(format!("{}, {}, {}, {}, {}",
                        file_hash[0], file_hash[1], file_hash[2], file_hash[3], file_hash[4]));
                }
            }
        }
        
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

    /// Gets the team for a specific async part (database mapping - team order)
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

    /// Gets the display order (1st, 2nd, 3rd) for an async part based on scheduled start times
    fn get_display_order(race: &Race, async_part: u8) -> u8 {
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
                    (position + 1) as u8 // Convert to 1-based display order
                } else {
                    // Fallback to async_part number if not found
                    async_part
                }
            }
            _ => async_part, // Fallback
        }
    }

    /// Gets the seed URL for a race
    fn get_seed_url(race: &Race) -> Result<String, Error> {
        // Check if race has a seed
        if let Some(seed_files) = &race.seed.files {
            match seed_files {
                seed::Files::AlttprDoorRando { uuid } => {
                    let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html")?;
                    patcher_url.query_pairs_mut().append_pair("patch", &format!("{}/seed/DR_{uuid}.bps", base_uri()));
                    Ok(patcher_url.to_string())
                }
                seed::Files::TwwrPermalink { permalink, .. } => {
                    Ok(format!("Permalink: {permalink}"))
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
                (r.async_start1 IS NOT NULL AND r.async_thread1 IS NULL AND r.async_start1 <= NOW() + INTERVAL '30 minutes' AND r.async_start1 > NOW() + INTERVAL '29 minutes') OR
                (r.async_start2 IS NOT NULL AND r.async_thread2 IS NULL AND r.async_start2 <= NOW() + INTERVAL '30 minutes' AND r.async_start2 > NOW() + INTERVAL '29 minutes') OR
                (r.async_start3 IS NOT NULL AND r.async_thread3 IS NULL AND r.async_start3 <= NOW() + INTERVAL '30 minutes' AND r.async_start3 > NOW() + INTERVAL '29 minutes') OR
                (r.async_start1 IS NOT NULL AND r.async_thread1 IS NULL AND r.async_start1 <= NOW() + INTERVAL '15 minutes' AND r.async_start1 > NOW()) OR
                (r.async_start2 IS NOT NULL AND r.async_thread2 IS NULL AND r.async_start2 <= NOW() + INTERVAL '15 minutes' AND r.async_start2 > NOW()) OR
                (r.async_start3 IS NOT NULL AND r.async_thread3 IS NULL AND r.async_start3 <= NOW() + INTERVAL '15 minutes' AND r.async_start3 > NOW())
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

    /// Creates threads for qualifier teams that have requested but not yet received a thread
    async fn create_qualifier_threads(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        _pool: &PgPool,
    ) -> Result<(), Error> {
        // Query teams that need qualifier threads:
        // - requested IS NOT NULL (team has requested the qualifier)
        // - submitted IS NULL (team hasn't submitted yet)
        // - discord_thread IS NULL (no thread created yet)
        // - event has automated_asyncs = true
        // - event has discord_async_channel configured
        // - async has a seed available (web_id, tfb_uuid, xkeys_uuid, file_stem, or seed_data)
        let teams_needing_threads = sqlx::query!(
            r#"
            SELECT
                at.team AS "team_id: Id<Teams>",
                at.kind AS "async_kind: event::AsyncKind",
                t.series AS "series: Series",
                t.event
            FROM async_teams at
            JOIN teams t ON at.team = t.id
            JOIN events e ON t.series = e.series AND t.event = e.event
            JOIN asyncs a ON t.series = a.series AND t.event = a.event AND at.kind = a.kind
            WHERE at.requested IS NOT NULL
              AND at.submitted IS NULL
              AND at.discord_thread IS NULL
              AND e.automated_asyncs = true
              AND e.discord_async_channel IS NOT NULL
              AND (a.web_id IS NOT NULL OR a.tfb_uuid IS NOT NULL OR a.xkeys_uuid IS NOT NULL OR a.file_stem IS NOT NULL OR a.seed_data IS NOT NULL)
            "#
        ).fetch_all(&mut **transaction).await?;

        for row in teams_needing_threads {
            // Load event data
            let event = EventData::new(transaction, row.series, &row.event)
                .await
                .map_err(|e| Error::Event(event::Error::Data(e)))?
                .ok_or(Error::EventNotFound)?;

            // Load team
            let team = Team::from_id(transaction, row.team_id).await?
                .ok_or(Error::NoTeamFound)?;

            if let Some(async_channel) = event.discord_async_channel {
                if let Err(e) = Self::create_qualifier_thread(
                    transaction,
                    discord_ctx,
                    &event,
                    &team,
                    row.async_kind,
                    async_channel,
                ).await {
                    // Log error but continue processing other teams
                    log::error!("Failed to create qualifier thread for team {}: {:?}", row.team_id, e);
                }
            }
        }

        Ok(())
    }

    /// Creates a private thread for a qualifier async
    async fn create_qualifier_thread(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        event: &EventData<'_>,
        team: &Team,
        async_kind: AsyncKind,
        async_channel: ChannelId,
    ) -> Result<(), Error> {
        // Get team name
        let team_name = team.name(transaction).await?
            .unwrap_or_else(|| "Unknown Team".to_string().into());

        // Build thread name
        let kind_str = match async_kind {
            AsyncKind::Qualifier1 => "Qualifier",
            AsyncKind::Qualifier2 => "Qualifier 2",
            AsyncKind::Qualifier3 => "Qualifier 3",
            AsyncKind::Seeding => "Seeding",
            AsyncKind::Tiebreaker1 => "Tiebreaker",
            AsyncKind::Tiebreaker2 => "Tiebreaker 2",
        };
        let thread_name = format!("{}: {}", kind_str, team_name);

        // Build thread content
        let mut content = Self::build_qualifier_thread_content(
            transaction,
            event,
            team,
            async_kind,
        ).await?;

        // Create the thread
        let thread = async_channel.create_thread(discord_ctx, CreateThread::new(&thread_name)
            .kind(ChannelType::PrivateThread)
            .auto_archive_duration(AutoArchiveDuration::OneWeek)
        ).await?;

        // Store thread ID in async_teams
        let thread_id = thread.id.get() as i64;
        sqlx::query!(
            "UPDATE async_teams SET discord_thread = $1 WHERE team = $2 AND kind = $3",
            thread_id,
            team.id as _,
            async_kind as _
        ).execute(&mut **transaction).await?;

        // Create the READY button
        let ready_button = CreateActionRow::Buttons(vec![
            CreateButton::new(format!("async_ready_qualifier_{}_{}", team.id, async_kind as i32))
                .label("READY!")
                .style(ButtonStyle::Primary)
        ]);

        // Send the initial message with the READY button
        thread.send_message(discord_ctx, CreateMessage::new()
            .content(content.build())
            .components(vec![ready_button])
        ).await?;

        // Add team members to thread
        let members = team.members(transaction).await?;
        for member in &members {
            if let Some(discord) = &member.discord {
                if let Ok(guild_member) = thread.guild_id.member(discord_ctx, discord.id).await {
                    let _ = thread.id.add_thread_member(discord_ctx, guild_member.user.id).await;
                }
            }
        }

        // Add organizers to thread
        let organizers = event.organizers(transaction).await.map_err(Error::Event)?;
        for organizer in organizers {
            if let Some(discord) = &organizer.discord {
                if let Ok(guild_member) = thread.guild_id.member(discord_ctx, discord.id).await {
                    let _ = thread.id.add_thread_member(discord_ctx, guild_member.user.id).await;
                }
            }
        }

        Ok(())
    }

    /// Builds the content for a qualifier async thread
    async fn build_qualifier_thread_content(
        transaction: &mut Transaction<'_, Postgres>,
        event: &EventData<'_>,
        team: &Team,
        async_kind: AsyncKind,
    ) -> Result<MessageBuilder, Error> {
        let mut content = MessageBuilder::default();

        // Get team members for mention
        let members = team.members(transaction).await?;

        // Header
        content.push("Welcome ");
        for (i, member) in members.iter().enumerate() {
            content.mention_user(member);
            if i < members.len() - 1 {
                content.push(", ");
            }
        }
        content.push("!");
        content.push_line("");
        content.push_line("");

        content.push("This thread is for your **");
        content.push(match async_kind {
            AsyncKind::Qualifier1 => "1st qualifier",
            AsyncKind::Qualifier2 => "2nd qualifier",
            AsyncKind::Qualifier3 => "3rd qualifier",
            AsyncKind::Seeding => "Seeding",
            AsyncKind::Tiebreaker1 => "Tiebreaker",
            AsyncKind::Tiebreaker2 => "Tiebreaker 2",
        });
        content.push("** async request for ");
        content.push_safe(event.display_name.clone());
        content.push(".");
        content.push_line("");
        content.push_line("");

        content.push("**Instructions:**");
        content.push_line("");
        content.push("1. When you're ready to receive the seed, click the **READY!** button below.");
        content.push_line("");
        content.push("2. The seed will be posted immediately after clicking READY.");
        content.push_line("");
        content.push("3. Click **START COUNTDOWN** when ready to begin your run.");
        content.push_line("");
        content.push("4. After the countdown, your timer begins.");
        content.push_line("");
        content.push("5. Click **FINISH** when you complete the seed.");
        content.push_line("");
        content.push("6. Post your VOD/recording link and any required screenshots.");
        content.push_line("");
        content.push("7. Staff will verify and record your official time using `/result-async`.");
        content.push_line("");
        content.push_line("");

        content.push("**Recording Requirements:**");
        content.push_line("");
        content.push("• Upload your recording to YouTube (unlisted is fine).");
        content.push_line("");
        let is_alttpr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'alttpr'
            ) AS "is_alttpr!""#,
            event.series as _
        ).fetch_one(&mut **transaction).await.unwrap_or(false);
        let is_twwr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'twwr'
            ) AS "is_twwr!""#,
            event.series as _
        ).fetch_one(&mut **transaction).await.unwrap_or(false);
        if is_alttpr {
            content.push("• Provide a screenshot of your final time and collection rate.");
        } else if is_twwr {
            content.push("• Provide a screenshot showing your final time together with the sword in Ganondorf's head.");
        } else {
            content.push("• Provide a screenshot showing your final time and an indicator of seed completion.");
        }
        content.push_line("");

        Ok(content)
    }

    /// Handles the READY button click for async races
    pub(crate) async fn handle_ready_button(
        pool: &PgPool,
        discord_ctx: &DiscordCtx,
        race_id: i64,
        async_part: u8,
        user_id: UserId,
    ) -> Result<(), Error> {
        let mut transaction = pool.begin().await?;
        
        // Load the race
        let race = Race::from_id(&mut transaction, &reqwest::Client::new(), Id::from(race_id as u64)).await?;
        
        // Verify the user is a member of the team for this async part
        let team = Self::get_team_for_async_part(&race, async_part)?;
        let members = team.members(&mut transaction).await?;
        if !members.iter().any(|m| m.discord.as_ref().map(|d| d.id) == Some(user_id)) {
            return Err(Error::UnauthorizedUser);
        }
        let player = members.into_iter().next().ok_or(Error::NoTeamMembers)?;
        
        // Check if already ready
        let already_ready = match async_part {
            1 => sqlx::query_scalar!("SELECT async_ready1 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
            2 => sqlx::query_scalar!("SELECT async_ready2 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
            3 => sqlx::query_scalar!("SELECT async_ready3 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
            _ => return Err(Error::InvalidAsyncPart),
        };
        
        if already_ready {
            return Err(Error::AlreadyReady);
        }
        
        // Mark as ready
        match async_part {
            1 => sqlx::query!("UPDATE races SET async_ready1 = TRUE WHERE id = $1", race_id).execute(&mut *transaction).await?,
            2 => sqlx::query!("UPDATE races SET async_ready2 = TRUE WHERE id = $1", race_id).execute(&mut *transaction).await?,
            3 => sqlx::query!("UPDATE races SET async_ready3 = TRUE WHERE id = $1", race_id).execute(&mut *transaction).await?,
            _ => return Ok(()),
        };
        
        // Create initial async_times record (without start_time)
        // Ready records should have recorded_by and recorded_at as NULL to distinguish them from reported records
        sqlx::query!(
            "INSERT INTO async_times (race_id, async_part, recorded_by, recorded_at) VALUES ($1, $2, NULL, NULL) ON CONFLICT (race_id, async_part) DO NOTHING",
            race_id,
            async_part as i32,
        ).execute(&mut *transaction).await?;
        
        // Load event data
        let event = EventData::new(&mut transaction, race.series, &race.event)
            .await
            .map_err(|e| Error::Event(event::Error::Data(e)))?
            .ok_or(Error::EventNotFound)?;
        
        // Distribute seed and notify organizers
        Self::distribute_seed_to_thread(
            &mut transaction,
            discord_ctx,
            &event,
            &race,
            async_part,
        ).await?;
        
        // Notify in the async thread
        let thread_id = match async_part {
            1 => sqlx::query_scalar!("SELECT async_thread1 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
            2 => sqlx::query_scalar!("SELECT async_thread2 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
            3 => sqlx::query_scalar!("SELECT async_thread3 FROM races WHERE id = $1", race_id).fetch_one(&mut *transaction).await?,
            _ => return Err(Error::InvalidAsyncPart),
        };
        
        if let Some(thread_id) = thread_id {
            let thread = ChannelId::new(thread_id as u64);
            
            let mut content = MessageBuilder::default();
            content.push("@here **This part of the async is ready to start!**");
            content.push_line("");
            content.push("Player ");
            content.mention_user(&player);
            content.push(" is ready for their portion of the async ");
            
            content.push("(");
            let teams: Vec<_> = race.teams().collect();
            for (i, team) in teams.iter().enumerate() {
                content.push_safe(team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()));
                if i < teams.len() - 1 {
                    content.push(" vs. ");
                }
            }

            if let Some(round) = &race.round {
                content.push(", ");
                content.push_safe(round.clone());
            }
            content.push(")");
            
            content.push_line("");
            content.push_line("");
            content.push("Click the START COUNTDOWN button when you're ready to begin your run.");

            // Check for force-start delay
            let async_start_delay = event.async_start_delay;
            if let Some(delay) = async_start_delay {
                if delay > 0 {
                    content.push_line("");
                    content.push(format!("You have **{} minutes** to click START COUNTDOWN before the seed is automatically started.", delay));
                }
            }

            // Create the START COUNTDOWN button
            let start_countdown_button = CreateActionRow::Buttons(vec![
                CreateButton::new("async_start_countdown")
                    .label("START COUNTDOWN")
                    .style(ButtonStyle::Success)
            ]);

            thread.send_message(discord_ctx, CreateMessage::new()
                .content(content.build())
                .components(vec![start_countdown_button])
            ).await?;

            // Spawn force-start task if configured
            if let Some(delay) = async_start_delay {
                let run = AsyncRun::BracketRace { race_id, async_part };
                let player_id = player.discord.as_ref().map(|d| d.id).ok_or(Error::NoTeamMembers)?;
                spawn_force_start_task(
                    pool.clone(),
                    Arc::clone(&discord_ctx.http),
                    thread,
                    run,
                    player_id,
                    delay,
                );
            }
        }

        transaction.commit().await?;
        Ok(())
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
    #[error("unauthorized user")]
    UnauthorizedUser,
    #[error("already ready")]
    AlreadyReady,
    #[error("already started")]
    AlreadyStarted,
    #[error("not started")]
    NotStarted,
    #[error("already finished")]
    AlreadyFinished,
}

/// Unified abstraction for both bracket async races and qualifier asyncs.
#[derive(Debug, Clone)]
pub(crate) enum AsyncRun {
    BracketRace { race_id: i64, async_part: u8 },
    Qualifier { team_id: i64, async_kind: AsyncKind },
}

impl AsyncRun {
    /// Parse a unified button custom_id.
    /// Returns (action, run, optional nonce).
    pub(crate) fn parse_button(custom_id: &str) -> Option<(String, AsyncRun, Option<i64>)> {
        let rest = custom_id.strip_prefix("async:")?;
        let mut parts = rest.splitn(2, ':');
        let action = parts.next()?;
        let remainder = parts.next()?;

        if let Some(bracket_params) = remainder.strip_prefix("bracket:") {
            let mut params = bracket_params.splitn(3, ':');
            let race_id = params.next()?.parse().ok()?;
            let async_part = params.next()?.parse().ok()?;
            let nonce = params.next().and_then(|n| n.parse().ok());
            Some((action.to_owned(), AsyncRun::BracketRace { race_id, async_part }, nonce))
        } else if let Some(qual_params) = remainder.strip_prefix("qual:") {
            let mut params = qual_params.splitn(3, ':');
            let team_id = params.next()?.parse().ok()?;
            let kind_int: i32 = params.next()?.parse().ok()?;
            let async_kind = AsyncKind::from_i32(kind_int)?;
            let nonce = params.next().and_then(|n| n.parse().ok());
            Some((action.to_owned(), AsyncRun::Qualifier { team_id, async_kind }, nonce))
        } else {
            None
        }
    }

    /// Generate a button custom_id for a given action.
    pub(crate) fn button_id(&self, action: &str) -> String {
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                format!("async:{}:bracket:{}:{}", action, race_id, async_part)
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                format!("async:{}:qual:{}:{}", action, team_id, *async_kind as i32)
            }
        }
    }

    /// Generate a button custom_id with a nonce (for revert buttons).
    pub(crate) fn button_id_with_nonce(&self, action: &str, nonce: i64) -> String {
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                format!("async:{}:bracket:{}:{}:{}", action, race_id, async_part, nonce)
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                format!("async:{}:qual:{}:{}:{}", action, team_id, *async_kind as i32, nonce)
            }
        }
    }

    /// Verify that the Discord user is authorized to click buttons for this run.
    /// For both bracket and qualifier: checks team membership.
    pub(crate) async fn verify_user(&self, pool: &PgPool, user_id: UserId) -> Result<(), Error> {
        let is_member = match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                let mut transaction = pool.begin().await?;
                let race = Race::from_id(&mut transaction, &reqwest::Client::new(), Id::from(*race_id as u64)).await?;
                let team = AsyncRaceManager::get_team_for_async_part(&race, *async_part)?;
                let members = team.members(&mut transaction).await?;
                members.iter().any(|m| m.discord.as_ref().map(|d| d.id) == Some(user_id))
            }
            AsyncRun::Qualifier { team_id, .. } => {
                sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM team_members tm
                        JOIN users u ON tm.member = u.id
                        WHERE tm.team = $1 AND u.discord_id = $2
                    ) AS "exists!""#,
                    *team_id,
                    user_id.get() as i64
                ).fetch_one(pool).await?
            }
        };
        if is_member { Ok(()) } else { Err(Error::UnauthorizedUser) }
    }

    /// Check whether the countdown has already been started.
    pub(crate) async fn is_started(&self, pool: &PgPool) -> Result<bool, Error> {
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                let started = sqlx::query_scalar!(
                    r#"SELECT start_time IS NOT NULL AS "started!" FROM async_times WHERE race_id = $1 AND async_part = $2"#,
                    *race_id,
                    *async_part as i32
                ).fetch_optional(pool).await?.unwrap_or(false);
                Ok(started)
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                let started = sqlx::query_scalar!(
                    r#"SELECT start_time IS NOT NULL AS "started!" FROM async_teams WHERE team = $1 AND kind = $2"#,
                    *team_id,
                    *async_kind as _
                ).fetch_optional(pool).await?.unwrap_or(false);
                Ok(started)
            }
        }
    }

    /// Record start_time = NOW().
    pub(crate) async fn record_start_time(&self, pool: &PgPool) -> Result<(), Error> {
        let now = Utc::now();
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                sqlx::query!(
                    "UPDATE async_times SET start_time = $1 WHERE race_id = $2 AND async_part = $3",
                    now, *race_id, *async_part as i32,
                ).execute(pool).await?;
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                sqlx::query!(
                    "UPDATE async_teams SET start_time = $1 WHERE team = $2 AND kind = $3",
                    now, *team_id, *async_kind as _,
                ).execute(pool).await?;
            }
        }
        Ok(())
    }

    /// Check that finish is allowed (started and not already finished).
    pub(crate) async fn check_finish_allowed(&self, pool: &PgPool) -> Result<(), Error> {
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                let record = sqlx::query!(
                    "SELECT start_time, finish_time, player_finished_at FROM async_times WHERE race_id = $1 AND async_part = $2",
                    *race_id, *async_part as i32
                ).fetch_optional(pool).await?;
                match record {
                    Some(r) if r.start_time.is_none() => Err(Error::NotStarted),
                    Some(r) if r.player_finished_at.is_some() => Err(Error::AlreadyFinished),
                    Some(r) => {
                        if let Some(ft) = r.finish_time {
                            if ft.microseconds != 0 || ft.days != 0 || ft.months != 0 {
                                return Err(Error::AlreadyFinished);
                            }
                        }
                        Ok(())
                    }
                    None => Err(Error::NotStarted),
                }
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                let record = sqlx::query!(
                    r#"SELECT start_time, finish_time, player_finished_at FROM async_teams WHERE team = $1 AND kind = $2"#,
                    *team_id, *async_kind as _
                ).fetch_optional(pool).await?;
                match record {
                    Some(r) if r.start_time.is_none() => Err(Error::NotStarted),
                    Some(r) if r.finish_time.is_some() || r.player_finished_at.is_some() => Err(Error::AlreadyFinished),
                    Some(_) => Ok(()),
                    None => Err(Error::NotStarted),
                }
            }
        }
    }

    /// Calculate the elapsed time since start and return formatted HH:MM:SS.
    pub(crate) async fn calculate_finish_time(&self, pool: &PgPool) -> Result<String, Error> {
        let start_time = match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                sqlx::query_scalar!(
                    "SELECT start_time FROM async_times WHERE race_id = $1 AND async_part = $2",
                    *race_id, *async_part as i32
                ).fetch_one(pool).await?
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                sqlx::query_scalar!(
                    "SELECT start_time FROM async_teams WHERE team = $1 AND kind = $2",
                    *team_id, *async_kind as _
                ).fetch_one(pool).await?
            }
        };
        let start_time = start_time.ok_or(Error::NotStarted)?;
        let duration = Utc::now().signed_duration_since(start_time);
        let total_seconds = duration.num_seconds();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        Ok(format!("{:02}:{:02}:{:02}", hours, minutes, seconds))
    }

    /// Set player_finished_at = NOW().
    pub(crate) async fn set_player_finished_at(&self, pool: &PgPool) -> Result<(), Error> {
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                sqlx::query!(
                    "UPDATE async_times SET player_finished_at = NOW() WHERE race_id = $1 AND async_part = $2 AND player_finished_at IS NULL",
                    *race_id, *async_part as i32
                ).execute(pool).await?;
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                sqlx::query!(
                    "UPDATE async_teams SET player_finished_at = NOW() WHERE team = $1 AND kind = $2 AND player_finished_at IS NULL",
                    *team_id, *async_kind as _
                ).execute(pool).await?;
            }
        }
        Ok(())
    }

    /// Query whether the event's game matches a given game name.
    pub(crate) async fn is_game(&self, pool: &PgPool, game_name: &str) -> Result<bool, Error> {
        match self {
            AsyncRun::BracketRace { race_id, .. } => {
                Ok(sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM races r
                        JOIN game_series gs ON gs.series = r.series
                        JOIN games g ON g.id = gs.game_id
                        WHERE r.id = $1 AND g.name = $2
                    ) AS "exists!""#,
                    *race_id, game_name
                ).fetch_one(pool).await?)
            }
            AsyncRun::Qualifier { team_id, .. } => {
                Ok(sqlx::query_scalar!(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM teams t
                        JOIN game_series gs ON gs.series = t.series
                        JOIN games g ON g.id = gs.game_id
                        WHERE t.id = $1 AND g.name = $2
                    ) AS "exists!""#,
                    *team_id, game_name
                ).fetch_one(pool).await?)
            }
        }
    }

    /// Get the Discord thread/channel ID for this run.
    #[allow(dead_code)]
    pub(crate) async fn get_channel_id(&self, pool: &PgPool) -> Result<Option<ChannelId>, Error> {
        match self {
            AsyncRun::BracketRace { race_id, async_part } => {
                let thread_id = match async_part {
                    1 => sqlx::query_scalar!("SELECT async_thread1 FROM races WHERE id = $1", *race_id).fetch_one(pool).await?,
                    2 => sqlx::query_scalar!("SELECT async_thread2 FROM races WHERE id = $1", *race_id).fetch_one(pool).await?,
                    3 => sqlx::query_scalar!("SELECT async_thread3 FROM races WHERE id = $1", *race_id).fetch_one(pool).await?,
                    _ => return Err(Error::InvalidAsyncPart),
                };
                Ok(thread_id.map(|id| ChannelId::new(id as u64)))
            }
            AsyncRun::Qualifier { team_id, async_kind } => {
                let thread_id = sqlx::query_scalar!(
                    "SELECT discord_thread FROM async_teams WHERE team = $1 AND kind = $2",
                    *team_id, *async_kind as _
                ).fetch_optional(pool).await?.flatten();
                Ok(thread_id.map(|id| ChannelId::new(id as u64)))
            }
        }
    }

    /// Get the event's configured async_start_delay.
    #[allow(dead_code)]
    pub(crate) async fn get_async_start_delay(&self, pool: &PgPool) -> Result<Option<i32>, Error> {
        match self {
            AsyncRun::BracketRace { race_id, .. } => {
                Ok(sqlx::query_scalar!(
                    "SELECT e.async_start_delay FROM events e JOIN races r ON r.series = e.series AND r.event = e.event WHERE r.id = $1",
                    *race_id
                ).fetch_optional(pool).await?.flatten())
            }
            AsyncRun::Qualifier { team_id, .. } => {
                Ok(sqlx::query_scalar!(
                    "SELECT e.async_start_delay FROM events e JOIN teams t ON t.series = e.series AND t.event = e.event WHERE t.id = $1",
                    *team_id
                ).fetch_optional(pool).await?.flatten())
            }
        }
    }
}

/// Create FINISH and FORFEIT buttons for a run.
pub(crate) fn create_finish_forfeit_buttons(run: &AsyncRun) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new(run.button_id("finish"))
            .label("FINISH")
            .style(ButtonStyle::Danger),
        CreateButton::new(run.button_id("forfeit"))
            .label("Forfeit this async")
            .style(ButtonStyle::Secondary),
    ])
}

/// Run the 5-4-3-2-1-GO countdown and record start_time.
/// Returns false if already started.
pub(crate) async fn run_countdown(
    pool: &PgPool,
    http: &Arc<Http>,
    channel_id: ChannelId,
    run: &AsyncRun,
) -> Result<bool, Error> {
    if run.is_started(pool).await? {
        return Ok(false);
    }

    channel_id.say(http, "**Your async is about to start!**").await?;
    sleep(Duration::from_secs(1)).await;

    for i in (1..=5).rev() {
        channel_id.say(http, format!("**{}**", i)).await?;
        sleep(Duration::from_secs(1)).await;
    }

    channel_id.say(http, "**GO!** \u{1F3C3}\u{200D}\u{2642}\u{FE0F}").await?;

    run.record_start_time(pool).await?;

    // Send FINISH / Forfeit buttons
    let race_buttons = create_finish_forfeit_buttons(run);
    channel_id.send_message(http, CreateMessage::new()
        .content("**Good luck!** Click the FINISH button once you have completed your run.\nIf you need to forfeit, click the Forfeit button.")
        .components(vec![race_buttons])
    ).await?;

    Ok(true)
}

/// Send the game-specific completion message after finish is finalized.
pub(crate) async fn send_completion_message(
    pool: &PgPool,
    http: &Arc<Http>,
    channel_id: ChannelId,
    run: &AsyncRun,
    formatted_time: &str,
) -> Result<(), Error> {
    let is_alttpr = run.is_game(pool, "alttpr").await.unwrap_or(false);
    let is_twwr = run.is_game(pool, "twwr").await.unwrap_or(false);

    let mut msg = MessageBuilder::default();
    match run {
        AsyncRun::BracketRace { .. } => {
            msg.push("@here - **This part of the async race is complete!**\n\n");
            msg.push(format!("**Estimated finish time:** {}\n\n", formatted_time));
            msg.push("Please provide:\n");
            msg.push("• A link to your VOD/recording\n");
        }
        AsyncRun::Qualifier { .. } => {
            msg.push("@here - **Qualifier run complete!**\n\n");
            msg.push(format!("**Estimated finish time:** {}\n\n", formatted_time));
            msg.push("Please provide:\n");
            msg.push("• A link to your VOD/recording\n");
        }
    }

    if is_alttpr {
        msg.push("• A screenshot of your final time & collection rate\n\n");
    } else if is_twwr {
        msg.push("• A screenshot showing your final time together with the sword in Ganondorf's head\n\n");
    } else {
        msg.push("• A screenshot of your final time and indication of seed completion\n\n");
    }
    msg.push("Staff will verify and record your official time using `/result-async`.");

    channel_id.say(http, msg.build()).await?;
    Ok(())
}

/// Spawn the force-start timer task.
/// After the seed is distributed, warns the player and auto-starts if they don't click START COUNTDOWN.
pub(crate) fn spawn_force_start_task(
    pool: PgPool,
    http: Arc<Http>,
    channel_id: ChannelId,
    run: AsyncRun,
    player_id: UserId,
    delay_minutes: i32,
) {
    tokio::spawn(async move {
        if delay_minutes <= 0 {
            // Immediate force-start
            if !run.is_started(&pool).await.unwrap_or(true) {
                let _ = channel_id.say(&http, format!("<@{}> **The seed is being automatically started!**", player_id.get())).await;
                let _ = run_countdown(&pool, &http, channel_id, &run).await;
            }
            return;
        }

        let total_secs = delay_minutes as u64 * 60;

        // Calculate warning times
        let warning_2min = if delay_minutes > 2 { Some((total_secs - 120) as u64) } else { None };
        let warning_30s = if total_secs > 30 { Some((total_secs - 30) as u64) } else { None };

        // Wait and send warnings
        let mut elapsed: u64 = 0;

        if let Some(t) = warning_2min {
            if t > elapsed {
                sleep(Duration::from_secs(t - elapsed)).await;
                elapsed = t;
                if run.is_started(&pool).await.unwrap_or(true) { return; }
                let _ = channel_id.say(&http, format!(
                    "<@{}> **2 minutes remaining** before the seed is automatically started!",
                    player_id.get()
                )).await;
            }
        }

        if let Some(t) = warning_30s {
            if t > elapsed {
                sleep(Duration::from_secs(t - elapsed)).await;
                elapsed = t;
                if run.is_started(&pool).await.unwrap_or(true) { return; }
                let _ = channel_id.say(&http, format!(
                    "<@{}> **30 seconds remaining** before the seed is automatically started!",
                    player_id.get()
                )).await;
            }
        }

        // Wait for the final stretch
        if total_secs > elapsed {
            sleep(Duration::from_secs(total_secs - elapsed)).await;
        }

        // Force start
        if run.is_started(&pool).await.unwrap_or(true) { return; }
        let _ = channel_id.say(&http, format!(
            "<@{}> **The seed is being automatically started!**",
            player_id.get()
        )).await;
        let _ = run_countdown(&pool, &http, channel_id, &run).await;
    });
}

/// Shared handler for START COUNTDOWN button click.
/// Performs auth check, countdown, records start time, sends FINISH/FORFEIT buttons.
/// Removes the START COUNTDOWN button on success.
pub(crate) async fn handle_start_countdown(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    // Auth
    run.verify_user(pool, interaction.user.id).await?;

    // Already started?
    if run.is_started(pool).await? {
        return Err(Error::AlreadyStarted);
    }

    // Defer so we can send multiple messages during countdown
    interaction.defer(&ctx.http).await?;

    // Countdown + record start + send FINISH/FORFEIT
    run_countdown(pool, &ctx.http, interaction.channel_id, run).await?;

    // Remove the START COUNTDOWN button
    interaction.edit_response(ctx, EditInteractionResponse::new()
        .components(vec![])
    ).await?;

    Ok(())
}

/// Shared handler for FINISH button click.
/// Auth check, calculate time, replace buttons with REVERT, spawn 30s finalize task.
pub(crate) async fn handle_finish(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: AsyncRun,
) -> Result<(), Error> {
    // Auth
    run.verify_user(pool, interaction.user.id).await?;

    // Check finish is allowed
    run.check_finish_allowed(pool).await?;

    // Calculate time
    let formatted_time = run.calculate_finish_time(pool).await?;

    // Show REVERT button with nonce
    let revert_nonce = Utc::now().timestamp_millis();
    let revert_button_id = run.button_id_with_nonce("revert", revert_nonce);
    let revert_button = CreateActionRow::Buttons(vec![
        CreateButton::new(&revert_button_id)
            .label("REVERT")
            .style(ButtonStyle::Secondary)
    ]);

    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .content(format!("\u{2705} **Finished in {}**\nYou have 30 seconds to revert if needed.", formatted_time))
            .components(vec![revert_button])
    )).await?;

    // Spawn 30s finalize task
    let ctx_clone = ctx.clone();
    let pool_clone = pool.clone();
    let channel_id = interaction.channel_id;
    let message_id = interaction.message.id;
    let run_clone = run.clone();

    tokio::spawn(async move {
        sleep(Duration::from_secs(30)).await;

        // Check if REVERT button still present (matching nonce)
        if let Ok(message) = channel_id.message(&ctx_clone, message_id).await {
            let has_revert = message.components.first()
                .map_or(false, |row| row.components.iter().any(|c| {
                    matches!(c, ActionRowComponent::Button(b)
                        if matches!(&b.data, ButtonKind::NonLink { custom_id, .. } if custom_id == &revert_button_id))
                }));

            if has_revert {
                // Remove REVERT button
                let _ = channel_id.edit_message(&ctx_clone, message_id, serenity::all::EditMessage::new()
                    .components(vec![])
                ).await;

                // Finalize: set player_finished_at + send completion message
                let _ = run_clone.set_player_finished_at(&pool_clone).await;
                let _ = send_completion_message(&pool_clone, &ctx_clone.http, channel_id, &run_clone, &formatted_time).await;
            }
        }
    });

    Ok(())
}

/// Shared handler for REVERT button click.
/// Restores FINISH/FORFEIT buttons.
pub(crate) async fn handle_revert(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    run: &AsyncRun,
) -> Result<(), Error> {
    let race_buttons = create_finish_forfeit_buttons(run);
    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .content("**Finish reverted!** Continue playing and click the FINISH button once you have completed your run.\nIf you need to forfeit, click the Forfeit button.")
            .components(vec![race_buttons])
    )).await?;
    Ok(())
}

/// Shared handler for FORFEIT button click.
/// Shows ephemeral confirmation dialog.
pub(crate) async fn handle_forfeit(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    run: &AsyncRun,
) -> Result<(), Error> {
    let confirm_button = CreateActionRow::Buttons(vec![
        CreateButton::new(run.button_id("forfeit_confirm"))
            .label("Yes, forfeit")
            .style(ButtonStyle::Danger),
        CreateButton::new(run.button_id("forfeit_cancel"))
            .label("Cancel")
            .style(ButtonStyle::Secondary),
    ]);

    interaction.create_response(ctx, CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .ephemeral(true)
            .content("\u{26A0}\u{FE0F} **Are you sure you want to forfeit?**\n\nThis will notify the organizers that you are forfeiting this async.")
            .components(vec![confirm_button])
    )).await?;
    Ok(())
}

/// Shared handler for confirmed FORFEIT.
/// Sends @here ping, removes FINISH/FORFEIT buttons.
pub(crate) async fn handle_forfeit_confirm(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    // Auth
    run.verify_user(pool, interaction.user.id).await?;

    // Send @here forfeit message
    let mut msg = MessageBuilder::default();
    msg.push("@here - ");
    msg.mention(&interaction.user);
    msg.push(" has indicated they want to **forfeit** this async.\n\n");
    msg.push("**Organizers:** To confirm this forfeit, use the `/forfeit-async` command in this thread.");

    interaction.channel_id.say(ctx, msg.build()).await?;

    // Find and remove FINISH/FORFEIT buttons from recent messages
    let messages = interaction.channel_id.messages(ctx, serenity::all::GetMessages::new().limit(50)).await?;
    for mut message in messages {
        if !message.components.is_empty() && message.content.contains("Good luck!") {
            let _ = message.edit(ctx, serenity::all::EditMessage::new().components(vec![])).await;
            break;
        }
    }

    // Update the ephemeral
    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .content("Forfeit request sent. An organizer will confirm it shortly.")
            .components(vec![])
    )).await?;
    Ok(())
}

/// Shared handler for FORFEIT cancel.
pub(crate) async fn handle_forfeit_cancel(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .content("Forfeit cancelled.")
            .components(vec![])
    )).await?;
    Ok(())
}

/// Resolve an `AsyncRun::BracketRace` from a thread ID.
pub(crate) async fn bracket_run_from_thread(pool: &PgPool, thread_id: i64) -> Result<Option<AsyncRun>, Error> {
    let race_info = sqlx::query!(
        r#"
        SELECT id,
               CASE
                   WHEN async_thread1 = $1 THEN 1
                   WHEN async_thread2 = $1 THEN 2
                   WHEN async_thread3 = $1 THEN 3
                   ELSE NULL
               END as async_part
        FROM races
        WHERE async_thread1 = $1 OR async_thread2 = $1 OR async_thread3 = $1
        "#,
        thread_id
    ).fetch_optional(pool).await?;

    Ok(race_info.and_then(|r| r.async_part.map(|p| AsyncRun::BracketRace { race_id: r.id, async_part: p as u8 })))
}
