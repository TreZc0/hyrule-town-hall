use {
    crate::{
        cal::{Race, RaceSchedule},
        event::{self, AsyncKind, Data as EventData, pooled_qualifiers},
        prelude::*,
        racetime_bot::VersionedBranch,
        seed,
        team::Team,
        user::User,
    },
    chrono::{DateTime, Utc},
    serenity::all::{
        ActionRowComponent, AutoArchiveDuration, ButtonKind, ButtonStyle, CacheHttp, ChannelId,
        ChannelType, ComponentInteraction, CreateActionRow, CreateButton, CreateInputText,
        CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, CreateModal,
        CreateThread, EditInteractionResponse, EditMessage, GuildChannel, Http, InputTextStyle, MessageBuilder,
    },
    sqlx::{PgPool, Postgres, Transaction, postgres::types::PgInterval},
    tokio::time::{Duration, sleep},
};

pub(crate) mod pooled;
mod start_timer;
mod setup;

pub(crate) struct AsyncRaceManager;

impl AsyncRaceManager {
    pub(crate) async fn reconcile_pooled(pool: &PgPool, ctx: &DiscordCtx) -> Result<(), Error> {
        start_timer::reconcile(pool, &ctx.http).await?;
        pooled_qualifiers::expire_overdue(pool).await?;
        let state = ctx
            .data
            .read()
            .await
            .get::<racetime_bot::GlobalState>()
            .cloned();
        if let Some(state) = state {
            pooled_qualifiers::generation::sweep(state).await?;
        }
        pooled::sweep(pool, &ctx.http).await
    }

    pub(crate) async fn create_async_threads(
        pool: &PgPool,
        discord_ctx: &DiscordCtx,
        _http_client: &reqwest::Client,
    ) -> Result<(), Error> {
        let mut transaction = pool.begin().await?;

        let races = Self::get_races_needing_threads(&mut transaction).await?;

        for race in races {
            let event = EventData::new(&mut transaction, race.series, &race.event)
                .await
                .map_err(|e| Error::Event(event::Error::Data(e)))?
                .ok_or(Error::EventNotFound)?;

            if let Some(async_channel) = event.discord_async_channel {
                let mut choices_prepared = false;
                for (async_part, start_time) in Self::get_async_parts(&race) {
                    if let Some(start_time) = start_time {
                        let time_until_start = start_time - Utc::now();
                        if time_until_start > chrono::Duration::zero()
                            && time_until_start <= chrono::Duration::minutes(30)
                        {
                            if !choices_prepared {
                                if let Some(config) = event.seed_gen_type.as_ref().and_then(racetime_bot::choice_resolution::config) {
                                    let mut choice_transaction = pool.begin().await?;
                                    racetime_bot::choice_resolution::ensure(&mut choice_transaction, &race, config, racetime_bot::choice_resolution::Timing::RoomOpening).await?;
                                    choice_transaction.commit().await?;
                                }
                                choices_prepared = true;
                            }
                            if let Err(error) = Self::create_async_thread(
                                &mut transaction,
                                discord_ctx,
                                &event,
                                &race,
                                async_part,
                                start_time,
                                async_channel,
                                pool,
                            )
                            .await {
                                let run = AsyncRun::BracketRace { race_id: race.id.into(), async_part };
                                let team = Self::get_team_for_async_part(&race, async_part)?;
                                let label = format!("{} — async part {async_part}", team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown entrant".into()));
                                setup::notify_failure(&discord_ctx.http, event.discord_organizer_channel, async_channel,
                                    &run, &label, &error.to_string()).await;
                            }
                        }
                    }
                }
            }
        }

        Self::create_qualifier_threads(&mut transaction, discord_ctx, pool).await?;

        transaction.commit().await?;
        Ok(())
    }

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
        let player = team
            .members(transaction)
            .await?
            .into_iter()
            .next()
            .ok_or(Error::NoTeamMembers)?;

        let is_first_half = Self::is_first_half(race, async_part, start_time);

        let teams: Vec<_> = race.teams().collect();
        let mut matchup = String::new();
        for (i, _team) in teams.iter().enumerate() {
            if i > 0 {
                matchup.push_str("v");
            }
            matchup.push_str(&format!("P{}", i + 1));
        }

        let player_name = player.display_name();
        let display_order = Self::get_display_order(race, async_part);
        let game_suffix = race
            .game
            .map(|game| format!(" (G{game})"))
            .unwrap_or_default();
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
            format!(
                "Async {}{}: {} ({})",
                round_str.trim(),
                game_suffix,
                player_name,
                if display_order == 1 {
                    "1st"
                } else if display_order == 2 {
                    "2nd"
                } else {
                    "3rd"
                }
            )
        } else {
            format!(
                "Async {}{}: {} ({})",
                matchup,
                game_suffix,
                player_name,
                if display_order == 1 {
                    "1st"
                } else if display_order == 2 {
                    "2nd"
                } else {
                    "3rd"
                }
            )
        };

        let run = AsyncRun::BracketRace { race_id: race.id.into(), async_part };
        let Some(thread) = setup::thread(db_pool, &discord_ctx.http, async_channel, &run, &thread_name).await? else { return Ok(()) };
        let mut content = Self::build_async_thread_content(
            transaction,
            event,
            race,
            async_part,
            start_time,
            &player,
            is_first_half,
            db_pool,
        )
        .await?;

        let entrant = player.discord.as_ref().ok_or(Error::NoTeamMembers)?.id;
        add_async_thread_members(&discord_ctx.http, &thread, [entrant]).await?;

        let ready_button = CreateActionRow::Buttons(vec![
            CreateButton::new(run.button_id("ready"))
                .label("READY!")
                .style(ButtonStyle::Primary),
        ]);

        setup::send_ready(&discord_ctx.http, &thread, &run, content.build(), ready_button).await?;

        let organizers = event.organizers(transaction).await.map_err(Error::Event)?;
        let current_team = Self::get_team_for_async_part(race, async_part)?;

        let mut added_users = HashSet::new();

        if let Some(discord) = &player.discord { added_users.insert(discord.id); }

        for organizer in organizers {
            if let Some(discord) = &organizer.discord {
                if added_users.contains(&discord.id) {
                    continue;
                }

                let mut is_opponent = false;
                for team in race.teams() {
                    if team.id == current_team.id {
                        continue;
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

                if !is_opponent {
                    let _ = add_async_thread_members(&discord_ctx.http, &thread, [discord.id]).await;
                    added_users.insert(discord.id);
                }
            }
        }

        setup::complete(&discord_ctx.http, &thread, &run, &thread_name).await?;
        Ok(())
    }

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

        content.push("Hey ");
        content.mention_user(player);
        content.push(", this thread will be used to handle your part of the async for this race");
        if let Some(game) = race.game {
            content.push(format!(" (G{game})"));
        }
        content.push(": ");

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
                content
                    .mention_team(transaction, event.discord_guild, team)
                    .await?;
            } else {
                content.push_safe(
                    team.name(transaction)
                        .await?
                        .unwrap_or_else(|| "Unknown Team".to_string().into()),
                );
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

        if let Some(config) = event.seed_gen_type.as_ref().and_then(racetime_bot::choice_resolution::config) {
            if let Some(snapshot) = racetime_bot::choice_resolution::read(db_pool, race.id).await?
                .filter(|snapshot| snapshot.visible_at(racetime_bot::choice_resolution::Timing::RoomOpening))
            {
                content.push_line("");
                content.push(snapshot.display_for_config(true, &config.for_display(race, event.draft_kind_str.is_some())));
                content.push_line("");
            }
        }

        if let Some(racetime_bot::seed_gen_type::SeedGenType::AlttprDoorRando {
            source: racetime_bot::seed_gen_type::AlttprDrSource::Boothisman,
            ..
        }) = event.seed_gen_type.as_ref()
        {
            let alttprde_options = racetime_bot::AlttprDeRaceOptions::for_race(
                db_pool,
                race,
                event.round_modes.as_ref(),
            )
            .await;

            let mut details = MessageBuilder::default();
            if let Some(ref round) = race.round {
                if event.round_modes.is_some() {
                    if let Some(mode_display) = alttprde_options.mode_display() {
                        details.push(format!("**Round Mode:** {} - {}", round, mode_display));
                    } else {
                        details.push(format!("**Round Mode:** {} - not yet set", round));
                    }
                    details.push_line("");
                }
            }

            if event.event == "9bracket" {
                if let Some(mode_display) = alttprde_options.mode_display() {
                    details.push(format!("**Mode:** {}", mode_display));
                    details.push_line("");
                }

                if alttprde_options.has_custom_choices() {
                    details.push("**Settings chosen by both runners:** ");
                    let choices = alttprde_options.custom_choices_labels();
                    details.push(choices.join(", "));
                    details.push_line("");
                }
            }

            let details_str = details.build();
            if !details_str.is_empty() {
                content.push_line("");
                content.push_line("");
                content.push("---");
                content.push_line("");
                content.push(details_str);
                content.push("---");
            }
        }

        content.push_line("");
        append_async_instructions(&mut content, event.async_start_delay);
        content.push("To maintain fairness, the final match results will only be shared after both players have completed the seed and organizers have confirmed the results.");

        let is_alttpr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'alttpr'
            ) AS "is_alttpr!""#,
            event.series as _
        )
        .fetch_one(&mut **transaction)
        .await
        .unwrap_or(false);
        let is_twwr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'twwr'
            ) AS "is_twwr!""#,
            event.series as _
        )
        .fetch_one(&mut **transaction)
        .await
        .unwrap_or(false);
        let display_order = Self::get_display_order(race, async_part);
        if display_order == 1 {
            content.push_line("");
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
            content.push(
                "• We suggest using MKV format for recording (more crash-resistant than MP4).",
            );
        } else {
            content.push_line("");
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

    async fn distribute_seed_to_thread(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        event: &EventData<'_>,
        race: &Race,
        async_part: u8,
    ) -> Result<(), Error> {
        let seed_data = race.seed.to_seed_data().ok_or(Error::NoSeedAvailable)?;

        let team = Self::get_team_for_async_part(race, async_part)?;
        let player = team
            .members(transaction)
            .await?
            .into_iter()
            .next()
            .ok_or(Error::NoTeamMembers)?;

        let mut content = MessageBuilder::default();
        content.push("Hey ");
        content.mention_user(&player);
        content.push(", ");
        content.push_line("");
        content.push(seed_message(&seed_data)?.build());
        if matches!(race.seed.files(), Some(seed::Files::TwwrPermalink { .. })) {
            if let (Some(VersionedBranch::Tww { tracker_link: Some(link), .. }), Some(settings)) = (&event.rando_version, &event.settings_string) {
                content.push(format!("\nTracker: https://{link}/#/tracker/new/{}", urlencoding::encode(settings)));
            }
        }

        // Read saved choices here as well, so seeds rolled before this fix get the same
        // complete, filtered summary beside the seed in each participant's private thread.
        let settings_summary = if let Some(snapshot) = racetime_bot::choice_resolution::read(&mut **transaction, race.id).await? {
            Some(snapshot.display(true))
        } else {
            race.seed.seed_data.as_ref().and_then(|data| racetime_bot::baselines::seed_summary(data, true))
        };

        let thread_id = match async_part {
            1 => {
                sqlx::query_scalar!(
                    "SELECT async_thread1 FROM races WHERE id = $1",
                    race.id as _
                )
                .fetch_one(&mut **transaction)
                .await?
            }
            2 => {
                sqlx::query_scalar!(
                    "SELECT async_thread2 FROM races WHERE id = $1",
                    race.id as _
                )
                .fetch_one(&mut **transaction)
                .await?
            }
            3 => {
                sqlx::query_scalar!(
                    "SELECT async_thread3 FROM races WHERE id = $1",
                    race.id as _
                )
                .fetch_one(&mut **transaction)
                .await?
            }
            _ => return Err(Error::InvalidAsyncPart),
        };

        if let Some(thread_id) = thread_id {
            let thread = ChannelId::new(thread_id as u64);
            thread.say(discord_ctx, content.build()).await?;
            if let Some(summary) = settings_summary {
                for chunk in racetime_bot::baselines::message_chunks(&summary) {
                    thread.send_message(discord_ctx, CreateMessage::new().content(chunk).allowed_mentions(serenity::all::CreateAllowedMentions::default())).await?;
                }
            }

            match async_part {
                1 => {
                    sqlx::query!(
                        "UPDATE races SET async_seed1 = TRUE WHERE id = $1",
                        race.id as _
                    )
                    .execute(&mut **transaction)
                    .await?
                }
                2 => {
                    sqlx::query!(
                        "UPDATE races SET async_seed2 = TRUE WHERE id = $1",
                        race.id as _
                    )
                    .execute(&mut **transaction)
                    .await?
                }
                3 => {
                    sqlx::query!(
                        "UPDATE races SET async_seed3 = TRUE WHERE id = $1",
                        race.id as _
                    )
                    .execute(&mut **transaction)
                    .await?
                }
                _ => return Ok(()),
            };
        }

        Ok(())
    }

    fn get_async_parts(race: &Race) -> Vec<(u8, Option<DateTime<Utc>>)> {
        match &race.schedule {
            RaceSchedule::Async {
                start1,
                start2,
                start3,
                ..
            } => {
                vec![(1, *start1), (2, *start2), (3, *start3)]
            }
            _ => vec![],
        }
    }

    fn get_team_for_async_part(race: &Race, async_part: u8) -> Result<&Team, Error> {
        let teams: Vec<_> = race.teams().collect();
        match async_part {
            1 => teams.get(0).copied().ok_or(Error::NoTeamFound),
            2 => teams.get(1).copied().ok_or(Error::NoTeamFound),
            3 => teams.get(2).copied().ok_or(Error::NoTeamFound),
            _ => Err(Error::InvalidAsyncPart),
        }
    }

    fn is_first_half(race: &Race, async_part: u8, _start_time: DateTime<Utc>) -> bool {
        match &race.schedule {
            RaceSchedule::Async {
                start1,
                start2,
                start3,
                ..
            } => {
                let mut scheduled_times = Vec::new();
                if let Some(time) = start1 {
                    scheduled_times.push((1, *time));
                }
                if let Some(time) = start2 {
                    scheduled_times.push((2, *time));
                }
                if let Some(time) = start3 {
                    scheduled_times.push((3, *time));
                }

                scheduled_times.sort_by_key(|&(_, time)| time);

                if let Some(position) = scheduled_times
                    .iter()
                    .position(|&(part, _)| part == async_part)
                {
                    position == 0
                } else {
                    async_part == 1
                }
            }
            _ => async_part == 1,
        }
    }

    fn get_display_order(race: &Race, async_part: u8) -> u8 {
        match &race.schedule {
            RaceSchedule::Async {
                start1,
                start2,
                start3,
                ..
            } => {
                let mut scheduled_times = Vec::new();
                if let Some(time) = start1 {
                    scheduled_times.push((1, *time));
                }
                if let Some(time) = start2 {
                    scheduled_times.push((2, *time));
                }
                if let Some(time) = start3 {
                    scheduled_times.push((3, *time));
                }

                scheduled_times.sort_by_key(|&(_, time)| time);

                if let Some(position) = scheduled_times
                    .iter()
                    .position(|&(part, _)| part == async_part)
                {
                    (position + 1) as u8
                } else {
                    async_part
                }
            }
            _ => async_part,
        }
    }

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
            AND (
                (NOT r.async_ready1 AND r.async_start1 <= NOW() + INTERVAL '30 minutes' AND r.async_start1 > NOW()) OR
                (NOT r.async_ready2 AND r.async_start2 <= NOW() + INTERVAL '30 minutes' AND r.async_start2 > NOW()) OR
                (NOT r.async_ready3 AND r.async_start3 <= NOW() + INTERVAL '30 minutes' AND r.async_start3 > NOW())
            )
            "#).fetch_all(&mut **transaction).await?;

        let mut races = Vec::new();
        for race_row in race_rows {
            let race =
                Race::from_id(transaction, &reqwest::Client::new(), Id::from(race_row.id)).await?;
            races.push(race);
        }
        Ok(races)
    }

    /// Returns `false` if the event has a participant role configured and any team member does
    /// not (yet) hold it in the Discord guild — used to defer READY so the bot doesn't
    /// invite players to start in a role-gated channel they can't actually see.
    async fn team_has_participant_role(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        event: &EventData<'_>,
        team: &Team,
    ) -> Result<bool, Error> {
        let Some(discord_guild) = event.discord_guild else {
            return Ok(true);
        };
        let Some(PgSnowflake(participant_role)) = sqlx::query_scalar!(
            r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND series = $2 AND event = $3"#,
            PgSnowflake(discord_guild) as _, event.series as _, &*event.event,
        ).fetch_optional(&mut **transaction).await? else { return Ok(true) };

        for member in &team.members(transaction).await? {
            let Some(discord) = &member.discord else {
                return Ok(false);
            };
            match discord_guild.member(discord_ctx, discord.id).await {
                Ok(guild_member) => {
                    if !guild_member.roles.contains(&participant_role) {
                        return Ok(false);
                    }
                }
                Err(_) => return Ok(false),
            }
        }
        Ok(true)
    }

    async fn create_qualifier_threads(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        pool: &PgPool,
    ) -> Result<(), Error> {
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
              AND at.start_time IS NULL
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
            let team = Team::from_id(transaction, row.team_id)
                .await?
                .ok_or(Error::NoTeamFound)?;

            if let Some(async_channel) = event.discord_async_channel {
                if let Err(e) = Self::create_qualifier_thread(
                    transaction,
                    discord_ctx,
                    &event,
                    &team,
                    row.async_kind,
                    async_channel,
                    pool,
                )
                .await
                {
                    let run = AsyncRun::Qualifier { team_id: team.id.into(), async_kind: row.async_kind };
                    let label = format!("{} — qualifier async", team.name(transaction).await?.unwrap_or_else(|| "Unknown entrant".into()));
                    setup::notify_failure(&discord_ctx.http, event.discord_organizer_channel, async_channel,
                        &run, &label, &e.to_string()).await;
                }
            }
        }

        Ok(())
    }

    async fn create_qualifier_thread(
        transaction: &mut Transaction<'_, Postgres>,
        discord_ctx: &DiscordCtx,
        event: &EventData<'_>,
        team: &Team,
        async_kind: AsyncKind,
        async_channel: ChannelId,
        pool: &PgPool,
    ) -> Result<(), Error> {
        let team_name = team
            .name(transaction)
            .await?
            .unwrap_or_else(|| "Unknown Team".to_string().into());

        let kind_str = match async_kind {
            AsyncKind::Qualifier1 => "Qualifier",
            AsyncKind::Qualifier2 => "Qualifier 2",
            AsyncKind::Qualifier3 => "Qualifier 3",
            AsyncKind::Seeding => "Seeding",
            AsyncKind::Tiebreaker1 => "Tiebreaker",
            AsyncKind::Tiebreaker2 => "Tiebreaker 2",
        };
        let thread_name = format!("{}: {}", kind_str, team_name);

        let run = AsyncRun::Qualifier { team_id: team.id.into(), async_kind };
        let Some(thread) = setup::thread(pool, &discord_ctx.http, async_channel, &run, &thread_name).await? else { return Ok(()) };
        let mut content =
            Self::build_qualifier_thread_content(transaction, event, team, async_kind).await?;

        if !Self::team_has_participant_role(transaction, discord_ctx, event, team).await? {
            return Err(sqlx::Error::Protocol("the entrant is missing the event participant role or Discord server membership".into()).into());
        }
        let entrants: Vec<_> = team.members(transaction).await?.into_iter()
            .map(|member| member.discord.map(|discord| discord.id).ok_or(Error::NoTeamMembers))
            .collect::<Result<_, _>>()?;
        if entrants.is_empty() { return Err(Error::NoTeamMembers); }
        add_async_thread_members(&discord_ctx.http, &thread, entrants).await?;

        let ready_button = CreateActionRow::Buttons(vec![
            CreateButton::new(run.button_id("ready"))
                .label("READY!")
                .style(ButtonStyle::Primary),
        ]);

        setup::send_ready(&discord_ctx.http, &thread, &run, content.build(), ready_button).await?;

        for organizer in event.organizers(transaction).await.map_err(Error::Event)? {
            if let Some(discord) = organizer.discord {
                let _ = add_async_thread_members(&discord_ctx.http, &thread, [discord.id]).await;
            }
        }

        setup::complete(&discord_ctx.http, &thread, &run, &thread_name).await?;
        Ok(())
    }

    async fn build_qualifier_thread_content(
        transaction: &mut Transaction<'_, Postgres>,
        event: &EventData<'_>,
        team: &Team,
        async_kind: AsyncKind,
    ) -> Result<MessageBuilder, Error> {
        let mut content = MessageBuilder::default();

        let members = team.members(transaction).await?;

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

        append_async_instructions(&mut content, event.async_start_delay);

        Self::append_qualifier_recording_requirements(transaction, event, &mut content).await?;
        Ok(content)
    }

    async fn append_qualifier_recording_requirements(
        transaction: &mut Transaction<'_, Postgres>,
        event: &EventData<'_>,
        content: &mut MessageBuilder,
    ) -> Result<(), Error> {
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
        )
        .fetch_one(&mut **transaction)
        .await
        .unwrap_or(false);
        let is_twwr = sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM game_series gs
                JOIN games g ON g.id = gs.game_id
                WHERE gs.series = $1 AND g.name = 'twwr'
            ) AS "is_twwr!""#,
            event.series as _
        )
        .fetch_one(&mut **transaction)
        .await
        .unwrap_or(false);
        if is_alttpr {
            content.push("• Provide a screenshot of your final time and collection rate.");
        } else if is_twwr {
            content.push("• Provide a screenshot showing your final time together with the sword in Ganondorf's head.");
        } else {
            content.push("• Provide a screenshot showing your final time and an indicator of seed completion.");
        }
        content.push_line("");

        Ok(())
    }

    pub(crate) async fn handle_ready_button(
        pool: &PgPool,
        discord_ctx: &DiscordCtx,
        race_id: i64,
        async_part: u8,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Result<(), Error> {
        let mut transaction = pool.begin().await?;

        let race = Race::from_id(
            &mut transaction,
            &reqwest::Client::new(),
            Id::from(race_id as u64),
        )
        .await?;

        let team = Self::get_team_for_async_part(&race, async_part)?;
        let members = team.members(&mut transaction).await?;
        if !members
            .iter()
            .any(|m| m.discord.as_ref().map(|d| d.id) == Some(user_id))
        {
            return Err(Error::UnauthorizedUser);
        }
        let current_thread = match async_part {
            1 => {
                sqlx::query_scalar::<_, Option<i64>>(
                    "SELECT async_thread1 FROM races WHERE id = $1 FOR UPDATE",
                )
                .bind(race_id)
                .fetch_one(&mut *transaction)
                .await?
            }
            2 => {
                sqlx::query_scalar::<_, Option<i64>>(
                    "SELECT async_thread2 FROM races WHERE id = $1 FOR UPDATE",
                )
                .bind(race_id)
                .fetch_one(&mut *transaction)
                .await?
            }
            3 => {
                sqlx::query_scalar::<_, Option<i64>>(
                    "SELECT async_thread3 FROM races WHERE id = $1 FOR UPDATE",
                )
                .bind(race_id)
                .fetch_one(&mut *transaction)
                .await?
            }
            _ => return Err(Error::InvalidAsyncPart),
        };
        if current_thread != Some(channel_id.get() as i64) {
            return Err(Error::ResetAsyncPart);
        }
        members.into_iter().next().ok_or(Error::NoTeamMembers)?;

        let already_ready = match async_part {
            1 => {
                sqlx::query_scalar!("SELECT async_ready1 FROM races WHERE id = $1", race_id)
                    .fetch_one(&mut *transaction)
                    .await?
            }
            2 => {
                sqlx::query_scalar!("SELECT async_ready2 FROM races WHERE id = $1", race_id)
                    .fetch_one(&mut *transaction)
                    .await?
            }
            3 => {
                sqlx::query_scalar!("SELECT async_ready3 FROM races WHERE id = $1", race_id)
                    .fetch_one(&mut *transaction)
                    .await?
            }
            _ => return Err(Error::InvalidAsyncPart),
        };

        if already_ready {
            return Err(Error::AlreadyReady);
        }

        match async_part {
            1 => {
                sqlx::query!(
                    "UPDATE races SET async_ready1 = TRUE WHERE id = $1",
                    race_id
                )
                .execute(&mut *transaction)
                .await?
            }
            2 => {
                sqlx::query!(
                    "UPDATE races SET async_ready2 = TRUE WHERE id = $1",
                    race_id
                )
                .execute(&mut *transaction)
                .await?
            }
            3 => {
                sqlx::query!(
                    "UPDATE races SET async_ready3 = TRUE WHERE id = $1",
                    race_id
                )
                .execute(&mut *transaction)
                .await?
            }
            _ => return Ok(()),
        };

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

        Self::distribute_seed_to_thread(&mut transaction, discord_ctx, &event, &race, async_part)
            .await?;

        let thread_id = match async_part {
            1 => {
                sqlx::query_scalar!("SELECT async_thread1 FROM races WHERE id = $1", race_id)
                    .fetch_one(&mut *transaction)
                    .await?
            }
            2 => {
                sqlx::query_scalar!("SELECT async_thread2 FROM races WHERE id = $1", race_id)
                    .fetch_one(&mut *transaction)
                    .await?
            }
            3 => {
                sqlx::query_scalar!("SELECT async_thread3 FROM races WHERE id = $1", race_id)
                    .fetch_one(&mut *transaction)
                    .await?
            }
            _ => return Err(Error::InvalidAsyncPart),
        };

        if let Some(thread_id) = thread_id {
            let thread = ChannelId::new(thread_id as u64);

            let mut content = MessageBuilder::default();
            content.push("**This part of the async is ready to start!**");
            content.push_line("");
            content.push_line("");

            let teams: Vec<_> = race.teams().collect();
            content.push("**Matchup:** ");
            for (i, team) in teams.iter().enumerate() {
                content.push_safe(
                    team.name(&mut transaction)
                        .await?
                        .unwrap_or_else(|| "Unknown Team".to_string().into()),
                );
                if i < teams.len() - 1 {
                    content.push(" vs. ");
                }
            }
            content.push_line("");

            if let Some(round) = &race.round {
                content.push("**Round:** ");
                content.push_safe(round.clone());
                content.push_line("");
            }

            let display_order = Self::get_display_order(&race, async_part);
            let part_label = match display_order {
                1 => "First",
                2 => "Second",
                _ => "Third",
            };
            content.push(format!("**Part:** {} part of async", part_label));

            content.push_line("");
            content.push_line("");
            content.push("Click the 'Start Countdown' button when you're ready to begin your run.");

            let async_start_delay = event.async_start_delay;
            if let Some(delay) = async_start_delay {
                if delay > 0 {
                    content.push_line("");
                    content.push(format!("You have **{} minutes** to click the start button before the seed is force started.", delay));
                }
            }

            let run = AsyncRun::BracketRace {
                race_id,
                async_part,
            };
            let start_countdown_button = start_button(&run);

            thread
                .send_message(
                    discord_ctx,
                    CreateMessage::new()
                        .content(content.build())
                        .components(vec![start_countdown_button]),
                )
                .await?;

            if let Some(delay) = async_start_delay {
                spawn_force_start_task(pool.clone(), Arc::clone(&discord_ctx.http), thread, run, user_id, delay);
            }

        }

        transaction.commit().await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    DiscordBot(#[from] crate::discord_bot::Error),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Discord(#[from] serenity::Error),
    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
    #[error("event error: {0}")]
    Event(event::Error),
    #[error(transparent)]
    Cal(#[from] cal::Error),
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
    #[error("this async part has been reset")]
    ResetAsyncPart,
    #[error(transparent)]
    Pooled(#[from] pooled_qualifiers::Error),
}

#[derive(Debug, Clone)]
pub(crate) enum AsyncRun {
    BracketRace {
        race_id: i64,
        async_part: u8,
    },
    Qualifier {
        team_id: i64,
        async_kind: AsyncKind,
    },
    PooledQualifier {
        attempt_id: i64,
        control_version: i64,
    },
}

impl AsyncRun {
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
            Some((
                action.to_owned(),
                AsyncRun::BracketRace {
                    race_id,
                    async_part,
                },
                nonce,
            ))
        } else if let Some(qual_params) = remainder.strip_prefix("qual:") {
            let mut params = qual_params.splitn(3, ':');
            let team_id = params.next()?.parse().ok()?;
            let kind_int: i32 = params.next()?.parse().ok()?;
            let async_kind = AsyncKind::from_i32(kind_int)?;
            let nonce = params.next().and_then(|n| n.parse().ok());
            Some((
                action.to_owned(),
                AsyncRun::Qualifier {
                    team_id,
                    async_kind,
                },
                nonce,
            ))
        } else if let Some(pool_params) = remainder.strip_prefix("pool:") {
            let mut params = pool_params.splitn(3, ':');
            let attempt_id = params.next()?.parse().ok()?;
            let control_version = params.next()?.parse().ok()?;
            let nonce = params.next().and_then(|n| n.parse().ok());
            Some((
                action.to_owned(),
                AsyncRun::PooledQualifier {
                    attempt_id,
                    control_version,
                },
                nonce,
            ))
        } else {
            None
        }
    }

    pub(crate) fn button_id(&self, action: &str) -> String {
        match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
                format!("async:{}:bracket:{}:{}", action, race_id, async_part)
            }
            AsyncRun::Qualifier {
                team_id,
                async_kind,
            } => {
                format!("async:{}:qual:{}:{}", action, team_id, *async_kind as i32)
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => {
                format!("async:{action}:pool:{attempt_id}:{control_version}")
            }
        }
    }

    pub(crate) fn button_id_with_nonce(&self, action: &str, nonce: i64) -> String {
        match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
                format!(
                    "async:{}:bracket:{}:{}:{}",
                    action, race_id, async_part, nonce
                )
            }
            AsyncRun::Qualifier {
                team_id,
                async_kind,
            } => {
                format!(
                    "async:{}:qual:{}:{}:{}",
                    action, team_id, *async_kind as i32, nonce
                )
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => {
                format!("async:{action}:pool:{attempt_id}:{control_version}:{nonce}")
            }
        }
    }

    fn next_control_version(&self) -> Self {
        match *self {
            Self::PooledQualifier {
                attempt_id,
                control_version,
            } => Self::PooledQualifier {
                attempt_id,
                control_version: control_version + 1,
            },
            ref run => run.clone(),
        }
    }

    async fn thread_id(&self, pool: &PgPool) -> Result<Option<i64>, Error> {
        let expected: Option<i64> = match self {
            Self::BracketRace { race_id, async_part } => sqlx::query_scalar(
                "SELECT CASE $2 WHEN 1 THEN async_thread1 WHEN 2 THEN async_thread2 WHEN 3 THEN async_thread3 END FROM races WHERE id=$1",
            ).bind(race_id).bind(i32::from(*async_part)).fetch_optional(pool).await?.flatten(),
            Self::Qualifier { team_id, async_kind } => sqlx::query_scalar(
                "SELECT discord_thread FROM async_teams WHERE team=$1 AND kind=$2",
            ).bind(team_id).bind(*async_kind).fetch_optional(pool).await?.flatten(),
            Self::PooledQualifier { attempt_id, .. } => sqlx::query_scalar(
                "SELECT discord_thread FROM qualifier_attempts WHERE id=$1",
            ).bind(attempt_id).fetch_optional(pool).await?.flatten(),
        };
        Ok(expected)
    }

    async fn verify_thread(&self, pool: &PgPool, channel_id: ChannelId) -> Result<(), Error> {
        let expected = self.thread_id(pool).await?;
        if expected != Some(channel_id.get() as i64) { return Err(Error::ResetAsyncPart); }
        Ok(())
    }

    pub(crate) async fn verify_user(&self, pool: &PgPool, user_id: UserId) -> Result<(), Error> {
        let is_member = match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
                let mut transaction = pool.begin().await?;
                let race = Race::from_id(
                    &mut transaction,
                    &reqwest::Client::new(),
                    Id::from(*race_id as u64),
                )
                .await?;
                let team = AsyncRaceManager::get_team_for_async_part(&race, *async_part)?;
                let members = team.members(&mut transaction).await?;
                members
                    .iter()
                    .any(|m| m.discord.as_ref().map(|d| d.id) == Some(user_id))
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
                )
                .fetch_one(pool)
                .await?
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => {
                sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (
                        SELECT 1 FROM qualifier_attempts attempt
                        JOIN team_members member ON member.team = attempt.team_id
                        JOIN users ON users.id = member.member
                        WHERE attempt.id = $1 AND attempt.control_version = $2
                          AND users.discord_id = $3
                    )"#,
                )
                .bind(attempt_id)
                .bind(control_version)
                .bind(user_id.get() as i64)
                .fetch_one(pool)
                .await?
            }
        };
        if is_member {
            Ok(())
        } else {
            Err(Error::UnauthorizedUser)
        }
    }

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
            AsyncRun::PooledQualifier { attempt_id, .. } => Ok(sqlx::query_scalar::<_, bool>(
                "SELECT state IN ('starting', 'running', 'awaiting_verification', 'finalized', 'void') FROM qualifier_attempts WHERE id = $1",
            ).bind(attempt_id).fetch_optional(pool).await?.unwrap_or(false)),
        }
    }

    pub(crate) async fn record_start_time(&self, pool: &PgPool) -> Result<(), Error> {
        let now = Utc::now();
        match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
                sqlx::query!(
                    "UPDATE async_times SET start_time = $1 WHERE race_id = $2 AND async_part = $3",
                    now,
                    *race_id,
                    *async_part as i32,
                )
                .execute(pool)
                .await?;
            }
            AsyncRun::Qualifier {
                team_id,
                async_kind,
            } => {
                sqlx::query!(
                    "UPDATE async_teams SET start_time = $1 WHERE team = $2 AND kind = $3",
                    now,
                    *team_id,
                    *async_kind as _,
                )
                .execute(pool)
                .await?;
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => {
                pooled_qualifiers::record_go(pool, *attempt_id, *control_version, now, None)
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn check_finish_allowed(&self, pool: &PgPool) -> Result<(), Error> {
        match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
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
            AsyncRun::Qualifier {
                team_id,
                async_kind,
            } => {
                let record = sqlx::query!(
                    r#"SELECT start_time, finish_time, player_finished_at FROM async_teams WHERE team = $1 AND kind = $2"#,
                    *team_id, *async_kind as _
                ).fetch_optional(pool).await?;
                match record {
                    Some(r) if r.start_time.is_none() => Err(Error::NotStarted),
                    Some(r) if r.finish_time.is_some() || r.player_finished_at.is_some() => {
                        Err(Error::AlreadyFinished)
                    }
                    Some(_) => Ok(()),
                    None => Err(Error::NotStarted),
                }
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => {
                let row: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
                    "SELECT state, deadline_at FROM qualifier_attempts WHERE id = $1 AND control_version = $2",
                ).bind(attempt_id).bind(control_version).fetch_optional(pool).await?;
                match row {
                    Some((state, _)) if state != "running" => Err(
                        if state == "awaiting_verification" || state == "finalized" {
                            Error::AlreadyFinished
                        } else {
                            Error::NotStarted
                        },
                    ),
                    Some((_, Some(deadline))) if Utc::now() > deadline => {
                        Err(Error::AlreadyFinished)
                    }
                    Some(_) => Ok(()),
                    None => Err(Error::ResetAsyncPart),
                }
            }
        }
    }

    pub(crate) async fn calculate_finish_time(&self, pool: &PgPool, finished_at: DateTime<Utc>) -> Result<String, Error> {
        let start_time = match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
                sqlx::query_scalar!(
                    "SELECT start_time FROM async_times WHERE race_id = $1 AND async_part = $2",
                    *race_id,
                    *async_part as i32
                )
                .fetch_one(pool)
                .await?
            }
            AsyncRun::Qualifier {
                team_id,
                async_kind,
            } => {
                sqlx::query_scalar!(
                    "SELECT start_time FROM async_teams WHERE team = $1 AND kind = $2",
                    *team_id,
                    *async_kind as _
                )
                .fetch_one(pool)
                .await?
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                "SELECT started_at FROM qualifier_attempts WHERE id = $1 AND control_version = $2",
            )
            .bind(attempt_id)
            .bind(control_version)
            .fetch_one(pool)
            .await?,
        };
        let start_time = start_time.ok_or(Error::NotStarted)?;
        Ok(format_finish_time(start_time, finished_at))
    }

    pub(crate) async fn set_player_finished_at(&self, pool: &PgPool, at: DateTime<Utc>) -> Result<(), Error> {
        match self {
            AsyncRun::BracketRace {
                race_id,
                async_part,
            } => {
                sqlx::query!(
                    "UPDATE async_times SET player_finished_at = $3 WHERE race_id = $1 AND async_part = $2 AND player_finished_at IS NULL",
                    *race_id, *async_part as i32, at
                ).execute(pool).await?;
            }
            AsyncRun::Qualifier {
                team_id,
                async_kind,
            } => {
                sqlx::query!(
                    "UPDATE async_teams SET player_finished_at = $3 WHERE team = $1 AND kind = $2 AND player_finished_at IS NULL",
                    *team_id, *async_kind as _, at
                ).execute(pool).await?;
            }
            AsyncRun::PooledQualifier {
                attempt_id,
                control_version,
            } => {
                pooled_qualifiers::participant_finish(
                    pool,
                    *attempt_id,
                    *control_version,
                    at,
                    false,
                )
                .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn is_game(&self, pool: &PgPool, game_name: &str) -> Result<bool, Error> {
        match self {
            AsyncRun::BracketRace { race_id, .. } => Ok(sqlx::query_scalar!(
                r#"SELECT EXISTS (
                        SELECT 1 FROM races r
                        JOIN game_series gs ON gs.series = r.series
                        JOIN games g ON g.id = gs.game_id
                        WHERE r.id = $1 AND g.name = $2
                    ) AS "exists!""#,
                *race_id,
                game_name
            )
            .fetch_one(pool)
            .await?),
            AsyncRun::Qualifier { team_id, .. } => Ok(sqlx::query_scalar!(
                r#"SELECT EXISTS (
                        SELECT 1 FROM teams t
                        JOIN game_series gs ON gs.series = t.series
                        JOIN games g ON g.id = gs.game_id
                        WHERE t.id = $1 AND g.name = $2
                    ) AS "exists!""#,
                *team_id,
                game_name
            )
            .fetch_one(pool)
            .await?),
            AsyncRun::PooledQualifier { attempt_id, .. } => Ok(sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                    SELECT 1 FROM qualifier_attempts attempt
                    JOIN game_series gs ON gs.series = attempt.series
                    JOIN games g ON g.id = gs.game_id
                    WHERE attempt.id = $1 AND g.name = $2
                )"#,
            )
            .bind(attempt_id)
            .bind(game_name)
            .fetch_one(pool)
            .await?),
        }
    }
}

pub(crate) async fn clear_message_with_button(
    http: impl CacheHttp,
    channel_id: ChannelId,
    button_id: &str,
) {
    if let Ok(messages) = channel_id
        .messages(&http, serenity::all::GetMessages::new().limit(20))
        .await
    {
        for message in messages {
            let has_button = message.components.iter().any(|row| row.components.iter().any(|c| {
                matches!(c, ActionRowComponent::Button(b)
                    if matches!(&b.data, ButtonKind::NonLink { custom_id, .. } if custom_id == button_id))
            }));
            if has_button {
                let _ = channel_id
                    .edit_message(&http, message.id, EditMessage::new().components(vec![]))
                    .await;
                break;
            }
        }
    }
}

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

async fn add_async_thread_members(http: &Http, thread: &GuildChannel, users: impl IntoIterator<Item = UserId>) -> Result<(), Error> {
    add_thread_members(users, |user| async move {
        thread.id.add_thread_member(http, user).await?;
        Ok(())
    }).await
}

async fn add_thread_members<F, Fut>(users: impl IntoIterator<Item = UserId>, mut add: F) -> Result<(), Error>
where
    F: FnMut(UserId) -> Fut,
    Fut: Future<Output = Result<(), Error>>,
{
    let mut added = HashSet::new();
    for user in users {
        if added.insert(user) { add(user).await?; }
    }
    Ok(())

}

fn format_finish_time(started_at: DateTime<Utc>, finished_at: DateTime<Utc>) -> String {
    let seconds = (finished_at - started_at).num_seconds().max(0);
    format!("{:02}:{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60, seconds % 60)
}

fn forfeit_message(run: &AsyncRun, mentions: &str) -> (String, Vec<CreateActionRow>) {
    (format!("@here - {mentions} has indicated they want to **forfeit** this async.\n\n**Organizers:** Click the button below to confirm this forfeit."),
        vec![CreateActionRow::Buttons(vec![CreateButton::new(run.button_id("org_forfeit"))
            .label("Confirm Forfeit").style(ButtonStyle::Danger)])])
}

fn append_async_instructions(content: &mut MessageBuilder, force_start_delay: Option<i32>) {
    content.push("**Instructions:**\n")
        .push("1. Have your recording setup ready, then click **READY!** to receive your seed.\n")
        .push("2. Download and prepare the seed. ");
    match force_start_delay.filter(|delay| *delay > 0) {
        Some(delay) => { content.push(format!("After READY, you have **{delay} minutes** to prepare before the countdown starts automatically.\n")); }
        None => { content.push("No automatic force-start is configured; click **START COUNTDOWN** when ready.\n"); }
    }
    content.push("3. Click **START COUNTDOWN** when ready. Your timer starts at **GO**, after the six-second countdown.\n")
        .push("4. Click **FINISH** when you finish, or **FORFEIT** if you stop. An accidental FINISH can be reverted for 30 seconds.\n")
        .push("5. Post your VOD/recording link and the required screenshot in this thread.\n")
        .push("6. Organizers will review your VOD and confirm your result afterwards.\n\n");
}

fn start_button(run: &AsyncRun) -> CreateActionRow {
    CreateActionRow::Buttons(vec![CreateButton::new(run.button_id("start_countdown"))
        .label("START COUNTDOWN").style(ButtonStyle::Success)])
}

const REVERTED_MESSAGE: &str = "**Finish reverted!** Continue playing and click the FINISH button once you have completed your run.\nIf you need to forfeit, click the Forfeit button.";

const RUNNING_MESSAGE: &str = "**Good luck!** Click FINISH when done, or FORFEIT to indicate a DNF.";

// Shared by bracket, normal qualifier, and pooled qualifier timers.
const FORCE_START_WARNINGS: [(i64, &str); 2] = [
    (120, "**2 minutes remaining** before the seed is force started!"),
    (30, "**30 seconds remaining** before the seed is force started!"),
];

const FORCE_START_MESSAGE: &str = "@here **The seed is being force started right now!**";

#[derive(Clone, Copy)]
enum CountdownStep {
    Announcement,
    Number(u8),
    Go,
}

impl CountdownStep {
    fn content(self) -> String {
        match self {
            Self::Announcement => "**Your async is about to start!**".into(),
            Self::Number(number) => format!("**{number}**"),
            Self::Go => "**GO!** \u{1F3C3}\u{200D}\u{2642}\u{FE0F}".into(),
        }
    }
}

/// All asyncs use the same separate messages and pacing. The caller supplies
/// delivery/persistence for the start marker and GO. Numbers are ordinary chat.
async fn send_countdown<F, Fut>(mut send: F) -> Result<Message, Error>
where
    F: FnMut(CountdownStep) -> Fut,
    Fut: Future<Output = Result<Message, Error>>,
{
    send(CountdownStep::Announcement).await?;
    sleep(Duration::from_secs(1)).await;
    for number in (1..=5).rev() {
        send(CountdownStep::Number(number)).await?;
        sleep(Duration::from_secs(1)).await;
    }
    send(CountdownStep::Go).await
}

pub(crate) async fn run_countdown(
    pool: &PgPool,
    http: &Arc<Http>,
    channel_id: ChannelId,
    run: &AsyncRun,
) -> Result<bool, Error> {
    run_countdown_inner(pool, http, channel_id, run, false).await
}

async fn run_countdown_inner(
    pool: &PgPool,
    http: &Arc<Http>,
    channel_id: ChannelId,
    run: &AsyncRun,
    forced: bool,
) -> Result<bool, Error> {
    if let AsyncRun::PooledQualifier {
        attempt_id,
        control_version,
    } = *run
    {
        let claimed = pooled_qualifiers::request_start(pool, attempt_id, control_version, forced).await?;
        if claimed {
            pooled::reconcile(pool, http, attempt_id).await?;
        }
        return Ok(claimed);
    }
    run.verify_thread(pool, channel_id).await?;
    if run.is_started(pool).await? { return Ok(false); }

    if forced { channel_id.say(http, FORCE_START_MESSAGE).await?; }
    send_countdown(|step| async move {
        Ok(channel_id.say(http, step.content()).await?)
    }).await?;

    run.record_start_time(pool).await?;

    let controls_run = run.next_control_version();
    let race_buttons = create_finish_forfeit_buttons(&controls_run);
    channel_id.send_message(http, CreateMessage::new()
        .content(RUNNING_MESSAGE)
        .components(vec![race_buttons])
    ).await?;

    Ok(true)
}

pub(crate) async fn send_completion_message(
    pool: &PgPool,
    http: &Arc<Http>,
    channel_id: ChannelId,
    run: &AsyncRun,
    formatted_time: &str,
) -> Result<(), Error> {
    let (content, components) = completion_message(pool, run, formatted_time).await?;
    channel_id
        .send_message(
            http,
            CreateMessage::new()
                .content(content)
                .components(components),
        )
        .await?;
    Ok(())
}

async fn completion_message(
    pool: &PgPool,
    run: &AsyncRun,
    formatted_time: &str,
) -> Result<(String, Vec<CreateActionRow>), Error> {
    let is_alttpr = run.is_game(pool, "alttpr").await.unwrap_or(false);
    let is_twwr = run.is_game(pool, "twwr").await.unwrap_or(false);

    Ok(completion_message_content(run, formatted_time, is_alttpr, is_twwr))
}

fn completion_message_content(run: &AsyncRun, formatted_time: &str, is_alttpr: bool, is_twwr: bool) -> (String, Vec<CreateActionRow>) {
    let mut msg = MessageBuilder::default();
    match run {
        AsyncRun::BracketRace { .. } => {
            msg.push("**This part of the async race is complete!**\n\n");
            msg.push(format!("**Estimated finish time:** {}\n\n", formatted_time));
            msg.push("Please provide:\n");
            msg.push("• A link to your VOD/recording\n");
        }
        AsyncRun::Qualifier { .. } | AsyncRun::PooledQualifier { .. } => {
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

    msg.push("Staff: click the **Confirm Result** button below to record the official time.");
    let confirm_button = CreateActionRow::Buttons(vec![
        CreateButton::new(run.button_id("org_result"))
            .label("Confirm Result")
            .style(ButtonStyle::Primary),
    ]);
    (msg.build(), vec![confirm_button])
}

async fn remove_start_button(http: &Http, channel_id: ChannelId, run: &AsyncRun) {
    let button_id = run.button_id("start_countdown");
    if let Ok(messages) = channel_id
        .messages(http, serenity::all::GetMessages::new().limit(20))
        .await
    {
        for message in messages {
            let has_button = message.components.iter().any(|row| row.components.iter().any(|c| {
                matches!(c, ActionRowComponent::Button(b)
                    if matches!(&b.data, ButtonKind::NonLink { custom_id, .. } if custom_id == &button_id))
            }));
            if has_button {
                let _ = channel_id
                    .edit_message(&http, message.id, EditMessage::new().components(vec![]))
                    .await;
                break;
            }
        }
    }
}

pub(crate) fn spawn_force_start_task(
    pool: PgPool,
    http: Arc<Http>,
    channel_id: ChannelId,
    run: AsyncRun,
    player_id: UserId,
    delay_minutes: i32,
) {
    if delay_minutes <= 0 { return; }
    let prepared_at = Utc::now();
    spawn_force_start_at(pool, http, channel_id, run, player_id, prepared_at,
        prepared_at + chrono::Duration::minutes(i64::from(delay_minutes)));
}

fn force_start_warnings(prepared_at: DateTime<Utc>, due: DateTime<Utc>) -> impl DoubleEndedIterator<Item = (DateTime<Utc>, &'static str)> {
    FORCE_START_WARNINGS.into_iter().filter_map(move |(seconds, text)| {
        let at = due - chrono::Duration::seconds(seconds);
        (at > prepared_at).then_some((at, text))
    })
}

fn spawn_force_start_at(
    pool: PgPool,
    http: Arc<Http>,
    channel_id: ChannelId,
    run: AsyncRun,
    player_id: UserId,
    prepared_at: DateTime<Utc>,
    due: DateTime<Utc>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        for (at, text) in force_start_warnings(prepared_at, due) {
            if let Ok(delay) = (at - Utc::now()).to_std() {
                sleep(delay).await;
                if run.is_started(&pool).await.unwrap_or(true) { return; }
                let _ = channel_id.say(&http, format!("<@{}> {text}", player_id.get())).await;
            }
        }
        if let Ok(delay) = (due - Utc::now()).to_std() { sleep(delay).await; }
        if run.is_started(&pool).await.unwrap_or(true) { return; }
        if let Err(error) = run_countdown_inner(&pool, &http, channel_id, &run, true).await {
            log::error!("async force-start failed in {channel_id}: {error}");
            return;
        }
        remove_start_button(&http, channel_id, &run).await;
    })
}

pub(crate) async fn handle_ready_bracket(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    let AsyncRun::BracketRace {
        race_id,
        async_part,
    } = run
    else {
        return Err(Error::InvalidAsyncPart);
    };
    AsyncRaceManager::handle_ready_button(
        pool,
        ctx,
        *race_id,
        *async_part,
        interaction.channel_id,
        interaction.user.id,
    )
    .await?;
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().components(vec![]),
            ),
        )
        .await?;
    Ok(())
}

pub(crate) async fn handle_ready_qualifier(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    let AsyncRun::Qualifier {
        team_id,
        async_kind,
    } = run
    else {
        return Err(Error::InvalidAsyncPart);
    };

    run.verify_user(pool, interaction.user.id).await?;

    let mut transaction = pool.begin().await?;

    let seed_info = sqlx::query!(
        r#"
        SELECT a.web_id, a.tfb_uuid, a.xkeys_uuid, a.file_stem,
               a.hash1, a.hash2, a.hash3, a.hash4, a.hash5, a.seed_password,
               a.seed_data,
               t.series AS "series: crate::series::Series", t.event
        FROM async_teams at
        JOIN teams t ON at.team = t.id
        JOIN asyncs a ON t.series = a.series AND t.event = a.event AND at.kind = a.kind
        WHERE at.team = $1 AND at.kind = $2
        "#,
        *team_id,
        *async_kind as _
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(seed) = seed_info else {
        return Err(Error::NoSeedAvailable);
    };

    let mut seed_msg = MessageBuilder::default();
    seed_msg.push("**Your seed is ready!**\n\n");

    if let Some(web_id) = seed.web_id {
        seed_msg.push(format!(
            "Seed URL: https://ootrandomizer.com/seed/get?id={}\n",
            web_id
        ));
    }
    if let Some(tfb_uuid) = seed.tfb_uuid {
        seed_msg.push(format!(
            "Triforce Blitz Seed: https://tfb.midos.house/seed/{}\n",
            tfb_uuid
        ));
    }
    if let Some(xkeys_uuid) = seed.xkeys_uuid {
        let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").unwrap();
        patcher_url
            .query_pairs_mut()
            .append_pair("patch", &format!("{}/seed/DR_{xkeys_uuid}.bps", base_uri()));
        seed_msg.push(format!("Door Rando Seed: {}\n", patcher_url));
    }
    if let Some(file_stem) = &seed.file_stem {
        seed_msg.push(format!(
            "Seed file: {}/seed/{}.zpfz\n",
            base_uri(),
            file_stem
        ));
    }

    if seed.hash1.is_some() {
        seed_msg.push("\nHash: ");
        if let (Some(h1), Some(h2), Some(h3), Some(h4), Some(h5)) = (
            &seed.hash1,
            &seed.hash2,
            &seed.hash3,
            &seed.hash4,
            &seed.hash5,
        ) {
            seed_msg.push(format!("{}, {}, {}, {}, {}\n", h1, h2, h3, h4, h5));
        }
    }

    if let Some(ref seed_data) = seed.seed_data {
        if seed::Files::from_seed_data(seed_data).is_some() {
            append_seed_details(&mut seed_msg, seed_data)?;
        } else {
            // Preserve historical normal qualifier payloads without a type tag.
            if let Some(hash) = seed_data.get("avianart_hash").and_then(|v| v.as_str()).filter(|value| !value.is_empty()) {
                seed_msg.push(format!("Seed URL: https://avianart.games/perm/{hash}\n"));
                if let Some(hash) = seed_data.get("avianart_seed_hash").and_then(|v| v.as_str()) {
                    seed_msg.push(format!("**Seed Hash:** {hash}\n"));
                }
            }
            if let Some(permalink) = seed_data.get("permalink").and_then(|v| v.as_str()).filter(|value| !value.is_empty()) {
                seed_msg.push(format!("**Permalink:** `{permalink}`\n"));
                if let Some(hash) = seed_data.get("seed_hash").and_then(|v| v.as_str()) {
                    seed_msg.push(format!("**Seed Hash:** {hash}\n"));
                }
            }
        }
    }

    if let Some(password) = &seed.seed_password {
        seed_msg.push(format!("\nPassword: {}\n", password));
    }

    let async_start_delay = sqlx::query_scalar!(
        "SELECT e.async_start_delay FROM events e JOIN teams t ON t.series = e.series AND t.event = e.event WHERE t.id = $1",
        *team_id
    ).fetch_optional(&mut *transaction).await?.flatten();

    if let Some(delay) = async_start_delay {
        if delay > 0 {
            seed_msg.push(format!("\nYou have **{} minutes** to click START COUNTDOWN before the seed is automatically started.", delay));
        }
    }

    let run = AsyncRun::Qualifier {
        team_id: *team_id,
        async_kind: *async_kind,
    };
    let start_button = start_button(&run);

    if let Some(summary) = seed.seed_data.as_ref().and_then(|data| racetime_bot::baselines::seed_summary(data, true)) {
        for chunk in racetime_bot::baselines::message_chunks(&summary) {
            interaction.channel_id.send_message(ctx, CreateMessage::new().content(chunk).allowed_mentions(serenity::all::CreateAllowedMentions::default())).await?;
        }
    }

    interaction
        .channel_id
        .send_message(
            ctx,
            CreateMessage::new()
                .content(seed_msg.build())
                .components(vec![start_button]),
        )
        .await?;

    transaction.commit().await?;
    if let Some(delay) = async_start_delay {
        spawn_force_start_task(pool.clone(), Arc::clone(&ctx.http), interaction.channel_id,
            run.clone(), interaction.user.id, delay);
    }

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().components(vec![]),
            ),
        )
        .await?;

    Ok(())
}

fn seed_message(seed_data: &serde_json::Value) -> Result<MessageBuilder, Error> {
    let mut message = MessageBuilder::default();
    message.push("**Your seed is ready!**\n\n");
    append_seed_details(&mut message, seed_data)?;
    Ok(message)
}

fn append_seed_details(message: &mut MessageBuilder, seed_data: &serde_json::Value) -> Result<(), Error> {
    match seed::Files::from_seed_data(seed_data).ok_or(Error::NoSeedAvailable)? {
        seed::Files::AlttprDoorRando { uuid, is_owr } => {
            let prefix = if is_owr { "OR_" } else { "DR_" };
            let mut patcher = Url::parse("https://alttprpatch.synack.live/patcher.html")?;
            patcher
                .query_pairs_mut()
                .append_pair("patch", &format!("{}/seed/{prefix}{uuid}.bps", base_uri()));
            message.push(format!("Seed URL: {patcher}\n"));
            if let Some(hash) = seed::Data::from_seed_data_only(Some(seed_data.clone()), None, false).file_hash {
                message.push(format!("Seed Hash: {}\n", hash.join(", ")));
            }
        }
        seed::Files::AvianartSeed { hash, seed_hash } => {
            message.push(format!("Seed URL: https://avianart.games/perm/{hash}\n"));
            if let Some(hash) = seed_hash {
                message.push(format!("Seed Hash: {}\n", hash.join(", ")));
            }
        }
        seed::Files::OotrWeb { id, .. } => {
            message.push(format!(
                "Seed URL: https://ootrandomizer.com/seed/get?id={id}\n"
            ));
        }
        seed::Files::TriforceBlitz { uuid, .. } => {
            message.push(format!("Seed URL: https://tfb.midos.house/seed/{uuid}\n"));
        }
        seed::Files::MidosHouse { file_stem, .. } => {
            message.push(format!("Seed file: {}/seed/{file_stem}.zpfz\n", base_uri()));
        }
        seed::Files::TwwrPermalink {
            permalink,
            seed_hash,
        } => {
            message.push(format!(
                "Permalink: `{permalink}`\nSeed Hash: {seed_hash}\n"
            ));
        }
        seed::Files::TfbSotd { date, ordinal } => {
            message.push(format!("Seed of the Day: {date}, seed {ordinal}\n"));
        }
    }
    Ok(())
}

pub(crate) async fn handle_ready_pooled(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    let AsyncRun::PooledQualifier {
        attempt_id,
        control_version,
    } = *run
    else {
        return Err(Error::InvalidAsyncPart);
    };
    let recovery_run = AsyncRun::PooledQualifier {
        attempt_id,
        control_version: control_version + 1,
    };
    if run.verify_user(pool, interaction.user.id).await.is_err() {
        recovery_run.verify_user(pool, interaction.user.id).await?;
    }
    run.verify_thread(pool, interaction.channel_id).await?;
    let revealed = pooled_qualifiers::reveal(
        pool,
        attempt_id,
        control_version,
        interaction.channel_id.get() as i64,
    )
    .await?;
    seed_message(&revealed.data)?;
    debug_assert_eq!(revealed.next_control_version, control_version + 1);
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(vec![]),
            ),
        )
        .await?;
    pooled::reconcile(pool, &ctx.http, attempt_id).await?;
    Ok(())
}

pub(crate) async fn handle_start_countdown(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    run.verify_user(pool, interaction.user.id).await?;
    run.verify_thread(pool, interaction.channel_id).await?;

    if run.is_started(pool).await? {
        return Err(Error::AlreadyStarted);
    }

    interaction.defer(&ctx.http).await?;

    run_countdown(pool, &ctx.http, interaction.channel_id, run).await?;

    interaction
        .edit_response(ctx, EditInteractionResponse::new().components(vec![]))
        .await?;

    Ok(())
}

pub(crate) async fn handle_finish(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: AsyncRun,
) -> Result<(), Error> {
    run.verify_user(pool, interaction.user.id).await?;
    run.verify_thread(pool, interaction.channel_id).await?;

    run.check_finish_allowed(pool).await?;

    let finished_at = *interaction.id.created_at();
    let formatted_time = run.calculate_finish_time(pool, finished_at).await?;

    let is_pooled = matches!(run, AsyncRun::PooledQualifier { .. });
    if is_pooled {
        // Store the interaction time before the grace period so an on-time click
        // cannot be turned into a timeout by the background expiry sweep.
        if let AsyncRun::PooledQualifier {
            attempt_id,
            control_version,
        } = run
        {
            pooled_qualifiers::participant_finish(
                pool,
                attempt_id,
                control_version,
                *interaction.id.created_at(),
                false,
            )
            .await?;
        }
    }
    let pending_run = if is_pooled {
        run.next_control_version()
    } else {
        run.clone()
    };
    let revert_nonce = Utc::now().timestamp_millis();
    let revert_button_id = pending_run.button_id_with_nonce("revert", revert_nonce);
    let revert_button = CreateActionRow::Buttons(vec![
        CreateButton::new(&revert_button_id)
            .label("REVERT")
            .style(ButtonStyle::Secondary),
    ]);

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(format!(
                        "\u{2705} **Finished in {}**\nYou have 30 seconds to revert if needed.",
                        formatted_time
                    ))
                    .components(vec![revert_button]),
            ),
        )
        .await?;

    let ctx_clone = ctx.clone();
    let pool_clone = pool.clone();
    let channel_id = interaction.channel_id;
    let message_id = interaction.message.id;
    let run_clone = pending_run;

    tokio::spawn(async move {
        sleep(Duration::from_secs(30)).await;

        if let Ok(message) = channel_id.message(&ctx_clone, message_id).await {
            let has_revert = message.components.first()
                .map_or(false, |row| row.components.iter().any(|c| {
                    matches!(c, ActionRowComponent::Button(b)
                        if matches!(&b.data, ButtonKind::NonLink { custom_id, .. } if custom_id == &revert_button_id))
                }));

            if has_revert {
                let _ = channel_id
                    .edit_message(
                        &ctx_clone,
                        message_id,
                        EditMessage::new().components(vec![]),
                    )
                    .await;

                if let AsyncRun::PooledQualifier { attempt_id, .. } = run_clone {
                    // The pooled adapter persists pending finishes and enforces its
                    // deadline/version rules; it uses the same completion message.
                    let _ = pooled::reconcile(&pool_clone, &ctx_clone.http, attempt_id).await;
                    return;
                }
                if run_clone.set_player_finished_at(&pool_clone, finished_at).await.is_err() { return; }
                let _ = send_completion_message(
                    &pool_clone,
                    &ctx_clone.http,
                    channel_id,
                    &run_clone,
                    &formatted_time,
                )
                .await;
            }
        }
    });

    Ok(())
}

pub(crate) async fn handle_revert(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    run.verify_user(pool, interaction.user.id).await?;
    run.verify_thread(pool, interaction.channel_id).await?;
    if let AsyncRun::PooledQualifier { attempt_id, control_version } = *run {
        interaction.defer(&ctx.http).await?;
        pooled_qualifiers::revert_finish(pool, attempt_id, control_version).await?;
        pooled::reconcile(pool, &ctx.http, attempt_id).await?;
        return Ok(());
    }
    let controls_run = run;
    let race_buttons = create_finish_forfeit_buttons(&controls_run);
    interaction.create_response(ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .content(REVERTED_MESSAGE)
            .components(vec![race_buttons])
    )).await?;
    Ok(())
}

pub(crate) async fn handle_forfeit(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    run.verify_user(pool, interaction.user.id).await?;

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

pub(crate) async fn handle_forfeit_confirm(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    run.verify_user(pool, interaction.user.id).await?;

    run.verify_thread(pool, interaction.channel_id).await?;
    let staff_run = if let AsyncRun::PooledQualifier {
        attempt_id,
        control_version,
    } = *run
    {
        AsyncRun::PooledQualifier {
            attempt_id,
            control_version: pooled_qualifiers::participant_finish(
                pool,
                attempt_id,
                control_version,
                *interaction.id.created_at(),
                true,
            )
            .await?,
        }
    } else {
        run.clone()
    };
    if let AsyncRun::PooledQualifier { attempt_id, .. } = staff_run {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .content("Forfeit accepted. Awaiting organizer confirmation.")
                        .components(vec![]),
                ),
            )
            .await?;
        pooled::reconcile(pool, &ctx.http, attempt_id).await?;
        return Ok(());
    }
    let (content, components) = forfeit_message(&staff_run, &format!("<@{}>", interaction.user.id));

    interaction
        .channel_id
        .send_message(
            ctx,
            CreateMessage::new()
                .content(content)
                .components(components),
        )
        .await?;

    clear_message_with_button(ctx, interaction.channel_id, &run.button_id("finish")).await;

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content("Forfeit request sent. An organizer will confirm it shortly.")
                    .components(vec![]),
            ),
        )
        .await?;
    Ok(())
}

pub(crate) async fn handle_forfeit_cancel(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content("Forfeit cancelled.")
                    .components(vec![]),
            ),
        )
        .await?;
    Ok(())
}

/// Looks for a twitch.tv/youtube.com/youtu.be link posted by the async player in the last 20
/// thread messages, so the org_result modal can be pre-filled with it. Any lookup failure
/// (DB error, Discord API error, no linked Discord account, no link found) just yields `None`
/// rather than blocking the modal from opening.
async fn find_recent_vod_link(
    ctx: &DiscordCtx,
    pool: &PgPool,
    interaction: &ComponentInteraction,
    run: &AsyncRun,
) -> Option<String> {
    let mut transaction = pool.begin().await.ok()?;
    let player_ids: Vec<UserId> = match run {
        AsyncRun::Qualifier { team_id, .. } => {
            let team = Team::from_id(&mut transaction, Id::from(*team_id as u64))
                .await
                .ok()??;
            team.members(&mut transaction).await.ok()?
        }
        AsyncRun::BracketRace {
            race_id,
            async_part,
        } => {
            let race = Race::from_id(
                &mut transaction,
                &reqwest::Client::new(),
                Id::from(*race_id as u64),
            )
            .await
            .ok()?;
            let team = AsyncRaceManager::get_team_for_async_part(&race, *async_part).ok()?;
            team.members(&mut transaction).await.ok()?
        }
        AsyncRun::PooledQualifier { attempt_id, .. } => {
            let team_id = sqlx::query_scalar::<_, i64>(
                "SELECT team_id FROM qualifier_attempts WHERE id = $1",
            )
            .bind(attempt_id)
            .fetch_one(&mut *transaction)
            .await
            .ok()?;
            let team = Team::from_id(&mut transaction, Id::from(team_id as u64))
                .await
                .ok()??;
            team.members(&mut transaction).await.ok()?
        }
    }
    .into_iter()
    .filter_map(|u| u.discord.map(|d| d.id))
    .collect();

    if player_ids.is_empty() {
        return None;
    }

    let messages = interaction
        .channel_id
        .messages(ctx, serenity::all::GetMessages::new().limit(20))
        .await
        .ok()?;
    messages
        .into_iter()
        .filter(|message| player_ids.contains(&message.author.id))
        .find_map(|message| {
            message.content.split_whitespace().find_map(|token| {
                let url = Url::parse(token).ok()?;
                let host = url.host_str()?;
                (host.contains("twitch.tv")
                    || host.contains("youtube.com")
                    || host.contains("youtu.be"))
                .then(|| token.to_string())
            })
        })
}

async fn is_run_organizer(
    pool: &PgPool,
    discord_id: UserId,
    run: &AsyncRun,
) -> Result<bool, Error> {
    let mut transaction = pool.begin().await?;
    let Some(user) = User::from_discord(&mut *transaction, discord_id).await? else {
        return Ok(false);
    };
    if user.is_global_admin() {
        return Ok(true);
    }
    let allowed = match run {
        AsyncRun::BracketRace { race_id, .. } => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
            SELECT 1 FROM races race JOIN organizers organizer
              ON organizer.series = race.series AND organizer.event = race.event
            WHERE race.id = $1 AND organizer.organizer = $2
        )"#,
            )
            .bind(race_id)
            .bind(user.id)
            .fetch_one(&mut *transaction)
            .await?
        }
        AsyncRun::Qualifier { team_id, .. } => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
            SELECT 1 FROM teams team JOIN organizers organizer
              ON organizer.series = team.series AND organizer.event = team.event
            WHERE team.id = $1 AND organizer.organizer = $2
        )"#,
            )
            .bind(team_id)
            .bind(user.id)
            .fetch_one(&mut *transaction)
            .await?
        }
        AsyncRun::PooledQualifier { attempt_id, .. } => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(
            SELECT 1 FROM qualifier_attempts attempt JOIN organizers organizer
              ON organizer.series = attempt.series AND organizer.event = attempt.event
            WHERE attempt.id = $1 AND organizer.organizer = $2
        )"#,
            )
            .bind(attempt_id)
            .bind(user.id)
            .fetch_one(&mut *transaction)
            .await?
        }
    };
    Ok(allowed)
}

pub(crate) async fn handle_org_result(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    let is_organizer = is_run_organizer(pool, interaction.user.id, run).await?;

    if !is_organizer {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .ephemeral(true)
                        .content("You must be an event organizer to use this."),
                ),
            )
            .await?;
        return Ok(());
    }

    let modal_id = match run {
        AsyncRun::Qualifier {
            team_id,
            async_kind,
        } => {
            format!("async_result_modal_qual_{}_{}", team_id, *async_kind as i32)
        }
        AsyncRun::BracketRace {
            race_id,
            async_part,
        } => {
            format!("async_result_modal_bracket_{}_{}", race_id, async_part)
        }
        AsyncRun::PooledQualifier {
            attempt_id,
            control_version,
        } => {
            format!("async_result_modal_pool_{attempt_id}_{control_version}")
        }
    };

    let pooled = matches!(run, AsyncRun::PooledQualifier { .. });
    let mut link_input = CreateInputText::new(
        InputTextStyle::Short,
        if pooled {
            "VOD link"
        } else {
            "VOD link (optional)"
        },
        "link",
    )
    .placeholder("https://...")
    .required(pooled);
    // A modal must be the initial response to its button interaction. Keep the optional
    // Discord history lookup from consuming Discord's three-second response window.
    if let Some(vod) = tokio::time::timeout(
        Duration::from_secs(2),
        find_recent_vod_link(ctx, pool, interaction, run),
    )
    .await
    .ok()
    .flatten()
    {
        link_input = link_input.value(vod);
    }

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Modal(
                CreateModal::new(modal_id, "Confirm Result").components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(InputTextStyle::Short, "Finish time", "time")
                            .placeholder("H:MM:SS or HH:MM:SS")
                            .required(true),
                    ),
                    CreateActionRow::InputText(link_input),
                ]),
            ),
        )
        .await?;
    Ok(())
}

pub(crate) async fn handle_org_forfeit(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    let is_organizer = is_run_organizer(pool, interaction.user.id, run).await?;

    if !is_organizer {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .ephemeral(true)
                        .content("You must be an event organizer to use this."),
                ),
            )
            .await?;
        return Ok(());
    }

    let confirm_row = CreateActionRow::Buttons(vec![
        CreateButton::new(run.button_id("org_forfeit_yes"))
            .label("Yes, confirm forfeit")
            .style(ButtonStyle::Danger),
        CreateButton::new(run.button_id("org_forfeit_cancel"))
            .label("Cancel")
            .style(ButtonStyle::Secondary),
    ]);

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content("\u{26A0}\u{FE0F} Are you sure you want to confirm this forfeit?")
                    .components(vec![confirm_row]),
            ),
        )
        .await?;
    Ok(())
}

/// Store an official bracket result; a missing duration records a forfeit.
/// Ready rows have neither recorded_by nor recorded_at, and must become official
/// on both the insert and update paths (recorded_at has no database default).
pub(crate) async fn record_bracket_result(
    transaction: &mut Transaction<'_, Postgres>,
    race_id: i64,
    async_part: i32,
    finish_time: Option<PgInterval>,
    recorded_by: Id<Users>,
    link: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO async_times (race_id, async_part, finish_time, recorded_by, link, recorded_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (race_id, async_part) DO UPDATE SET
            finish_time = EXCLUDED.finish_time,
            recorded_at = NOW(),
            recorded_by = EXCLUDED.recorded_by,
            link = EXCLUDED.link
        "#,
        race_id,
        async_part,
        finish_time,
        recorded_by as _,
        link,
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(test)]
mod result_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn bracket_results_and_forfeits_become_official_from_ready_or_missing_rows() {
        let pool = event::configuration::test_pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let race_id: i64 = sqlx::query_scalar("SELECT id FROM races ORDER BY id LIMIT 1")
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users ORDER BY id LIMIT 1")
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let finish_time = PgInterval {
            months: 0,
            days: 0,
            microseconds: 3_723_000_000,
        };
        let vod = "https://example.com/test-vod";
        let finished_at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();

        for ready_row_exists in [false, true] {
            sqlx::query("DELETE FROM async_times WHERE race_id = $1")
                .bind(race_id)
                .execute(&mut *transaction)
                .await
                .unwrap();
            if ready_row_exists {
                for part in [1, 2] {
                    sqlx::query("INSERT INTO async_times (race_id, async_part, player_finished_at) VALUES ($1, $2, $3)")
                        .bind(race_id).bind(part).bind(finished_at)
                        .execute(&mut *transaction).await.unwrap();
                }
            }

            record_bracket_result(
                &mut transaction,
                race_id,
                1,
                Some(finish_time),
                user_id.into(),
                Some(vod),
            )
            .await
            .unwrap();
            record_bracket_result(&mut transaction, race_id, 2, None, user_id.into(), None)
                .await
                .unwrap();
            let rows: Vec<(i32, Option<PgInterval>, i64, bool, Option<String>, Option<DateTime<Utc>>)> = sqlx::query_as(
                "SELECT async_part, finish_time, recorded_by, recorded_at IS NOT NULL, link, player_finished_at FROM async_times WHERE race_id = $1 AND recorded_by IS NOT NULL ORDER BY async_part",
            ).bind(race_id).fetch_all(&mut *transaction).await.unwrap();
            assert_eq!(
                rows,
                vec![
                    (
                        1,
                        Some(finish_time),
                        user_id,
                        true,
                        Some(vod.into()),
                        ready_row_exists.then_some(finished_at)
                    ),
                    (
                        2,
                        None,
                        user_id,
                        true,
                        None,
                        ready_row_exists.then_some(finished_at)
                    ),
                ]
            );

            // Main also permits an organizer to confirm a forfeit over a previous
            // result. It must clear the duration/VOD without creating a third part.
            record_bracket_result(&mut transaction, race_id, 1, None, user_id.into(), None)
                .await
                .unwrap();
            let recorded_forfeits: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM async_times WHERE race_id = $1 AND recorded_by IS NOT NULL AND recorded_at IS NOT NULL AND finish_time IS NULL AND link IS NULL",
            ).bind(race_id).fetch_one(&mut *transaction).await.unwrap();
            assert_eq!(recorded_forfeits, 2);
        }
        transaction.rollback().await.unwrap();
    }
}

pub(crate) async fn handle_org_forfeit_yes(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    run: &AsyncRun,
) -> Result<(), Error> {
    let is_organizer = is_run_organizer(pool, interaction.user.id, run).await?;

    if !is_organizer {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .content("You must be an event organizer.")
                        .components(vec![]),
                ),
            )
            .await?;
        return Ok(());
    }

    match run {
        AsyncRun::Qualifier {
            team_id,
            async_kind,
        } => {
            interaction
                .create_response(
                    ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .content("Recording forfeit...")
                            .components(vec![]),
                    ),
                )
                .await?;

            let mut transaction = pool.begin().await?;
            let team = Team::from_id(&mut transaction, Id::from(*team_id))
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            let team_name = team
                .name(&mut transaction)
                .await?
                .unwrap_or_else(|| "Unknown Team".to_string().into());

            sqlx::query!(
                "UPDATE async_teams SET submitted = NOW(), finish_time = NULL WHERE team = $1 AND kind = $2",
                *team_id as _,
                *async_kind as _
            ).execute(&mut *transaction).await?;

            let members = team.members(&mut transaction).await?;
            for member in members {
                sqlx::query!(
                    "INSERT INTO async_players (series, event, player, kind, time, vod) VALUES ($1, $2, $3, $4, NULL, NULL) ON CONFLICT (series, event, player, kind) DO UPDATE SET time = EXCLUDED.time, vod = COALESCE(EXCLUDED.vod, async_players.vod)",
                    team.series as _,
                    team.event,
                    member.id as _,
                    *async_kind as _
                ).execute(&mut *transaction).await?;
            }

            transaction.commit().await?;

            clear_message_with_button(ctx, interaction.channel_id, &run.button_id("org_forfeit"))
                .await;
            interaction
                .channel_id
                .say(ctx, format!("Forfeit confirmed for {}.", team_name))
                .await?;
        }
        AsyncRun::BracketRace {
            race_id,
            async_part,
        } => {
            interaction
                .create_response(
                    ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .content("Recording forfeit...")
                            .components(vec![]),
                    ),
                )
                .await?;

            let mut transaction = pool.begin().await?;
            let race = Race::from_id(
                &mut transaction,
                &reqwest::Client::new(),
                Id::from(*race_id as u64),
            )
            .await?;
            let user = User::from_discord(&mut *transaction, interaction.user.id)
                .await?
                .ok_or(Error::UnauthorizedUser)?;

            record_bracket_result(
                &mut transaction,
                *race_id,
                i32::from(*async_part),
                None,
                user.id,
                None,
            )
            .await?;

            let ignored_race_ids = crate::discord_bot::finalize_async_if_complete(
                ctx, &mut transaction, *race_id, &race,
            )
            .await?;

            transaction.commit().await?;
            crate::discord_bot::refresh_ignored_async_games(ctx, ignored_race_ids).await;
            clear_message_with_button(ctx, interaction.channel_id, &run.button_id("org_forfeit"))
                .await;
            interaction
                .channel_id
                .say(ctx, "Forfeit confirmed.")
                .await?;
        }
        AsyncRun::PooledQualifier {
            attempt_id,
            control_version,
        } => {
            interaction
                .create_response(
                    ctx,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .content("Recording forfeit...")
                            .components(vec![]),
                    ),
                )
                .await?;
            let mut transaction = pool.begin().await?;
            let user = User::from_discord(&mut *transaction, interaction.user.id)
                .await?
                .ok_or(Error::UnauthorizedUser)?;
            pooled_qualifiers::finalize(
                &mut transaction,
                *attempt_id,
                *control_version,
                pooled_qualifiers::Outcome::Forfeit,
                None,
                Some(user.id.into()),
            )
            .await?;
            transaction.commit().await?;
            clear_message_with_button(ctx, interaction.channel_id, &run.button_id("org_forfeit"))
                .await;
            interaction
                .channel_id
                .say(ctx, "Forfeit confirmed.")
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn handle_org_forfeit_cancel(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content("Cancelled.")
                    .components(vec![]),
            ),
        )
        .await?;
    Ok(())
}

/// Unified button dispatcher. Returns true if the button was handled.
/// Handles `async:{action}:{type}:{params}` button formats.
pub(crate) async fn dispatch_button(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    pool: &PgPool,
    custom_id: &str,
) -> Result<bool, Error> {
    let Some((action, run, _nonce)) = AsyncRun::parse_button(custom_id) else {
        return Ok(false);
    };

    match action.as_str() {
        "ready" => match &run {
            AsyncRun::BracketRace { .. } => {
                handle_ready_bracket(ctx, interaction, pool, &run).await?
            }
            AsyncRun::Qualifier { .. } => {
                handle_ready_qualifier(ctx, interaction, pool, &run).await?
            }
            AsyncRun::PooledQualifier { .. } => {
                handle_ready_pooled(ctx, interaction, pool, &run).await?
            }
        },
        "start_countdown" => {
            handle_start_countdown(ctx, interaction, pool, &run).await?;
        }
        "finish" => {
            handle_finish(ctx, interaction, pool, run).await?;
        }
        "revert" => {
            handle_revert(ctx, interaction, pool, &run).await?;
        }
        "forfeit" => {
            handle_forfeit(ctx, interaction, pool, &run).await?;
        }
        "forfeit_confirm" => {
            handle_forfeit_confirm(ctx, interaction, pool, &run).await?;
        }
        "forfeit_cancel" => {
            handle_forfeit_cancel(ctx, interaction).await?;
        }
        "org_result" => {
            handle_org_result(ctx, interaction, pool, &run).await?;
        }
        "org_forfeit" => {
            handle_org_forfeit(ctx, interaction, pool, &run).await?;
        }
        "org_forfeit_yes" => {
            handle_org_forfeit_yes(ctx, interaction, pool, &run).await?;
        }
        "org_forfeit_cancel" => {
            handle_org_forfeit_cancel(ctx, interaction).await?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod workflow_tests;
