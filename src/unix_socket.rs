use {
    async_proto::{
        ReadError,
        ReadErrorKind,
    },
    mhstatus::PrepareStopUpdate,
    serde_json::Value as Json,
    tokio::net::UnixListener,
    crate::{
        discord_bot::{
            Element,
            MULTIWORLD_GUILD,
        },
        prelude::*,
        racetime_bot::{
            CleanShutdownUpdate,
            Goal,
            PrerollMode,
            RollError,
            SeedCommandParseResult,
            SeedRollUpdate,
            VersionedBranch,
        },
    },
};

pub(crate) const PATH: &str = "/usr/local/share/midos-house/sock";

fn json_arg(arg: &str) -> serde_json::Result<Json> {
    serde_json::from_str(arg)
}

#[derive(clap::Subcommand, Protocol)]
pub(crate) enum ClientMessage {
    CleanupRoles {
        guild_id: GuildId,
    },
    PrepareStop {
        #[clap(long)]
        no_new_rooms: bool,
        #[clap(long)]
        async_proto: bool,
    },
    Roll {
        version: ootr_utils::Version,
        #[clap(value_parser = json_arg)]
        settings: Json,
        #[clap(short = 'l', long)]
        spoiler_log: bool,
    },
    RollRsl {
        preset: Option<String>,
        #[clap(short, long, default_value = "xopar")]
        branch: String,
        #[clap(long)]
        rsl_version: Option<ootr_utils::Version>,
        #[clap(short = 'n', long, default_value_t = 1)]
        worlds: u8,
        #[clap(short = 'l', long)]
        spoiler_log: bool,
    },
    Seed {
        #[clap(short = 'o', long = "official")]
        is_official: bool,
        #[clap(short = 'l', long, alias = "spoiler-log")]
        spoiler_seed: bool,
        #[clap(short = 'P', long)]
        no_password: bool,
        /// Disallow rolling the seed on ootrandomizer.com
        #[clap(long)]
        no_web: bool,
        goal: Goal,
        args: Vec<String>,
    },
    UpdateRegionalVc {
        user_id: Id<Users>,
        scene: u8,
    },
}

pub(crate) async fn listen(mut shutdown: rocket::Shutdown, clean_shutdown: Arc<Mutex<racetime_bot::CleanShutdown>>, global_state: Arc<racetime_bot::GlobalState>) -> wheel::Result<()> {
    fs::remove_file(PATH).await.missing_ok()?;
    let listener = UnixListener::bind(PATH).at(PATH)?;
    loop {
        select! {
            () = &mut shutdown => break,
            res = listener.accept() => {
                let (mut sock, _) = res.at_unknown()?;
                let clean_shutdown = clean_shutdown.clone();
                let global_state = global_state.clone();
                tokio::spawn(async move {
                    loop {
                        match ClientMessage::read(&mut sock).await {
                            Ok(ClientMessage::CleanupRoles { guild_id }) => {
                                let discord_ctx = global_state.discord_ctx.read().await;
                                let mut transaction = global_state.db_pool.begin().await.expect("error cleaning up Discord roles");
                                let mut roles_to_remove = sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND role IS NOT NULL"#, PgSnowflake(guild_id) as _).fetch(&mut *transaction)
                                    .map_ok(|PgSnowflake(role)| role)
                                    .try_collect::<Vec<_>>().await.expect("error cleaning up Discord roles");
                                roles_to_remove.extend(
                                    sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND racetime_team IS NOT NULL"#, PgSnowflake(guild_id) as _).fetch(&mut *transaction)
                                        .map_ok(|PgSnowflake(role)| role)
                                        .try_collect::<Vec<_>>().await.expect("error cleaning up Discord roles")
                                );
                                let mut members = pin!(guild_id.members_iter(&*discord_ctx));
                                while let Some(member) = members.try_next().await.expect("error cleaning up Discord roles") {
                                    for role in &member.roles {
                                        if roles_to_remove.contains(role) {
                                            member.remove_role(&*discord_ctx, role).await.expect("error cleaning up Discord roles");
                                        }
                                    }
                                }
                                transaction.commit().await.expect("error cleaning up Discord roles");
                                0u8.write(&mut sock).await.expect("error writing to UNIX socket");
                            }
                            Ok(ClientMessage::PrepareStop { no_new_rooms, async_proto: _ }) => {
                                Some(PrepareStopUpdate::AcquiringMutex).write(&mut sock).await.expect("error writing to UNIX socket");
                                let mut notifier = lock!(clean_shutdown = clean_shutdown; {
                                    clean_shutdown.requested = true;
                                    if no_new_rooms { clean_shutdown.block_new = true }
                                    if clean_shutdown.open_rooms.is_empty() {
                                        broadcast::channel(1).1
                                    } else {
                                        Some(PrepareStopUpdate::WaitingForRooms(clean_shutdown.open_rooms.clone())).write(&mut sock).await.expect("error writing to UNIX socket");
                                        clean_shutdown.updates.subscribe()
                                    }
                                });
                                loop {
                                    match notifier.recv().await {
                                        Ok(CleanShutdownUpdate::RoomOpened(room)) => Some(PrepareStopUpdate::RoomOpened(room)).write(&mut sock).await.expect("error writing to UNIX socket"),
                                        Ok(CleanShutdownUpdate::RoomClosed(room)) => Some(PrepareStopUpdate::RoomClosed(room)).write(&mut sock).await.expect("error writing to UNIX socket"),
                                        Ok(CleanShutdownUpdate::Empty) => break,
                                        Err(broadcast::error::RecvError::Closed) => break,
                                        Err(broadcast::error::RecvError::Lagged(_)) => notifier = lock!(clean_shutdown = clean_shutdown; {
                                            clean_shutdown.requested = true;
                                            if no_new_rooms { clean_shutdown.block_new = true }
                                            if clean_shutdown.open_rooms.is_empty() {
                                                broadcast::channel(1).1
                                            } else {
                                                Some(PrepareStopUpdate::WaitingForRooms(clean_shutdown.open_rooms.clone())).write(&mut sock).await.expect("error writing to UNIX socket");
                                                clean_shutdown.updates.subscribe()
                                            }
                                        }),
                                    }
                                }
                                None::<PrepareStopUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                break
                            }
                            Ok(ClientMessage::Roll { version, settings, spoiler_log }) => if let Json::Object(settings) = settings {
                                let mut rx = global_state.clone().roll_seed(PrerollMode::Medium, true, None, VersionedBranch::Pinned { version }, settings, if spoiler_log { UnlockSpoilerLog::Now } else { UnlockSpoilerLog::Never });
                                loop {
                                    let update = rx.recv().await;
                                    update.write(&mut sock).await.expect("error writing to UNIX socket");
                                    if update.is_none() { break }
                                }
                            } else {
                                Some(SeedRollUpdate::Error(RollError::NonObjectSettings)).write(&mut sock).await.expect("error writing to UNIX socket");
                                None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                break
                            },
                            Ok(ClientMessage::RollRsl { preset, branch, rsl_version, worlds, spoiler_log }) => {
                                let preset = if let Some(rsl_version) = rsl_version {
                                    rsl::VersionedPreset::new_versioned(rsl_version, preset.as_deref())
                                } else {
                                    rsl::VersionedPreset::new_unversioned(&branch, preset.as_deref())
                                };
                                if let Ok(preset) = preset {
                                    let mut rx = global_state.clone().roll_rsl_seed(None, preset, worlds, if spoiler_log { UnlockSpoilerLog::Now } else { UnlockSpoilerLog::Never });
                                    loop {
                                        let update = rx.recv().await;
                                        update.write(&mut sock).await.expect("error writing to UNIX socket");
                                        if update.is_none() { break }
                                    }
                                } else {
                                    Some(SeedRollUpdate::Error(RollError::RslVersion)).write(&mut sock).await.expect("error writing to UNIX socket");
                                    None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                    break
                                }
                            }
                            Ok(ClientMessage::Seed { is_official, spoiler_seed, no_password, no_web, goal, args }) => {
                                let mut transaction = match global_state.db_pool.begin().await {
                                    Ok(transaction) => transaction,
                                    Err(e) => {
                                        Some(SeedRollUpdate::Error(e.into())).write(&mut sock).await.expect("error writing to UNIX socket");
                                        None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                        break
                                    }
                                };
                                let mut rx = match goal.parse_seed_command(&mut transaction, &global_state, is_official, spoiler_seed, &args).await {
                                    Ok(SeedCommandParseResult::Regular { mut settings, unlock_spoiler_log, description, .. }) => {
                                        if no_password {
                                            settings.remove("password_lock");
                                        }
                                        Some(SeedRollUpdate::Message(description)).write(&mut sock).await.expect("error writing to UNIX socket");
                                        global_state.clone().roll_seed(goal.preroll_seeds(None /*TODO replace is_official parameter with optional series and event */), !no_web, None, goal.rando_version(None /*TODO replace is_official parameter with optional series and event */), settings, unlock_spoiler_log)
                                    }
                                    Ok(SeedCommandParseResult::Alttpr) => unimplemented!(),
                                    Ok(SeedCommandParseResult::Rsl { preset, world_count, unlock_spoiler_log, description, .. }) => {
                                        Some(SeedRollUpdate::Message(description)).write(&mut sock).await.expect("error writing to UNIX socket");
                                        global_state.clone().roll_rsl_seed(None, preset, world_count, unlock_spoiler_log)
                                    }
                                    Ok(SeedCommandParseResult::Tfb { version, unlock_spoiler_log, description, .. }) => {
                                        Some(SeedRollUpdate::Message(description)).write(&mut sock).await.expect("error writing to UNIX socket");
                                        global_state.clone().roll_tfb_seed(None, version, None, unlock_spoiler_log)
                                    }
                                    Ok(SeedCommandParseResult::TfbDev { coop, unlock_spoiler_log, description, .. }) => {
                                        Some(SeedRollUpdate::Message(description)).write(&mut sock).await.expect("error writing to UNIX socket");
                                        global_state.clone().roll_tfb_dev_seed(None, coop, None, unlock_spoiler_log)
                                    }
                                    Ok(SeedCommandParseResult::QueueExisting { data, description, .. }) => {
                                        Some(SeedRollUpdate::Message(description)).write(&mut sock).await.expect("error writing to UNIX socket");
                                        Some(SeedRollUpdate::Done { rsl_preset: None, unlock_spoiler_log: UnlockSpoilerLog::After, seed: data }).write(&mut sock).await.expect("error writing to UNIX socket");
                                        None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                        break
                                    }
                                    Ok(SeedCommandParseResult::SendPresets { msg, .. }) => {
                                        Some(SeedRollUpdate::Error(RollError::Cloned { debug: String::default(), display: msg.to_owned() })).write(&mut sock).await.expect("error writing to UNIX socket");
                                        None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                        break
                                    }
                                    Ok(SeedCommandParseResult::SendSettings { msg, .. } | SeedCommandParseResult::Error { msg, .. }) => {
                                        Some(SeedRollUpdate::Error(RollError::Cloned { debug: String::default(), display: msg.into_owned() })).write(&mut sock).await.expect("error writing to UNIX socket");
                                        None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                        break
                                    }
                                    Ok(SeedCommandParseResult::StartDraft { .. }) => unimplemented!(), //TODO
                                    Err(e) => {
                                        Some(SeedRollUpdate::Error(e.into())).write(&mut sock).await.expect("error writing to UNIX socket");
                                        None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                        break
                                    }
                                };
                                match transaction.commit().await {
                                    Ok(()) => {}
                                    Err(e) => {
                                        Some(SeedRollUpdate::Error(e.into())).write(&mut sock).await.expect("error writing to UNIX socket");
                                        None::<SeedRollUpdate>.write(&mut sock).await.expect("error writing to UNIX socket");
                                        break
                                    }
                                }
                                loop {
                                    let update = rx.recv().await;
                                    update.write(&mut sock).await.expect("error writing to UNIX socket");
                                    if update.is_none() { break }
                                }
                            }
                            Ok(ClientMessage::UpdateRegionalVc { user_id, scene }) => {
                                if let Some(user) = User::from_id(&global_state.db_pool, user_id).await.expect("error updating regional voice") {
                                    if let Some(discord_user) = user.discord {
                                        let element = match scene { //FROM https://wiki.cloudmodding.com/oot/Scene_Table/NTSC_1.0
                                            0x00 => Some(Element::Forest), // Inside the Deku Tree
                                            0x01 => Some(Element::Fire), // Dodongo's Cavern
                                            0x02 => Some(Element::Water), // Inside Jabu-Jabu's Belly
                                            0x03 => Some(Element::Forest), // Forest Temple
                                            0x04 => Some(Element::Fire), // Fire Temple
                                            0x05 => Some(Element::Water), // Water Temple
                                            0x06 => Some(Element::Spirit), // Spirit Temple
                                            0x07 => Some(Element::Shadow), // Shadow Temple
                                            0x08 => Some(Element::Shadow), // Bottom of the Well
                                            0x09 => Some(Element::Water), // Ice Cavern
                                            0x0B => Some(Element::Spirit), // Gerudo Training Ground
                                            0x0C => Some(Element::Spirit), // Thieves' Hideout
                                            0x0D => Some(Element::Light), // Inside Ganon's Castle
                                            0x1B => Some(Element::Light), // Market Entrance (Child - Day)
                                            0x1C => Some(Element::Light), // Market Entrance (Child - Night)
                                            0x1D => Some(Element::Light), // Market Entrance (Ruins)
                                            0x1E => Some(Element::Light), // Back Alley (Child - Day)
                                            0x1F => Some(Element::Light), // Back Alley (Child - Night)
                                            0x20 => Some(Element::Light), // Market (Child - Day)
                                            0x21 => Some(Element::Light), // Market (Child - Night)
                                            0x22 => Some(Element::Light), // Market (Ruins)
                                            0x23 => Some(Element::Light), // Temple of Time Exterior (Child - Day)
                                            0x24 => Some(Element::Light), // Temple of Time Exterior (Child - Night)
                                            0x25 => Some(Element::Light), // Temple of Time Exterior (Ruins)
                                            0x34 => Some(Element::Forest), // Link's House, HACK to make child spawn set region, only works if child spawn ER and special interior ER are both off
                                            0x43 => Some(Element::Light), // Temple of Time
                                            0x45 => Some(Element::Light), // Castle Hedge Maze (Day)
                                            0x46 => Some(Element::Light), // Castle Hedge Maze (Night)
                                            0x4A => Some(Element::Light), // Castle Courtyard
                                            0x51 => Some(Element::Light), // Spot 00 - Hyrule Field
                                            0x52 => Some(Element::Shadow), // Spot 01 - Kakariko Village
                                            0x53 => Some(Element::Shadow), // Spot 02 - Graveyard
                                            0x54 => Some(Element::Water), // Spot 03 - Zora's River
                                            0x55 => Some(Element::Forest), // Spot 04 - Kokiri Forest
                                            0x56 => Some(Element::Forest), // Spot 05 - Sacred Forest Meadow
                                            0x57 => Some(Element::Water), // Spot 06 - Lake Hylia
                                            0x58 => Some(Element::Water), // Spot 07 - Zora's Domain
                                            0x59 => Some(Element::Water), // Spot 08 - Zora's Fountain
                                            0x5A => Some(Element::Spirit), // Spot 09 - Gerudo Valley
                                            0x5B => Some(Element::Forest), // Spot 10 - Lost Woods
                                            0x5C => Some(Element::Spirit), // Spot 11 - Desert Colossus
                                            0x5D => Some(Element::Spirit), // Spot 12 - Gerudo's Fortress
                                            0x5E => Some(Element::Spirit), // Spot 13 - Haunted Wasteland
                                            0x5F => Some(Element::Light), // Spot 15 - Hyrule Castle
                                            0x60 => Some(Element::Fire), // Spot 16 - Death Mountain Trail
                                            0x61 => Some(Element::Fire), // Spot 17 - Death Mountain Crater
                                            0x62 => Some(Element::Fire), // Spot 18 - Goron City
                                            0x63 => Some(Element::Light), // Spot 20 - Lon Lon Ranch
                                            0x64 => Some(Element::Light), // Ganon's Castle Exterior
                                            _ => None,
                                        };
                                        if let Some(element) = element {
                                            let discord_ctx = global_state.discord_ctx.read().await;
                                            if discord_ctx.data.write().await.entry::<Element>().or_default().insert(discord_user.id, element).unwrap_or(Element::Light) != element {
                                                let _ = MULTIWORLD_GUILD.move_member(&*discord_ctx, discord_user.id, element.voice_channel()).await; // errors if the user isn't in voice in the multiworld guild
                                            }
                                        }
                                    }
                                }
                                0u8.write(&mut sock).await.expect("error writing to UNIX socket");
                                break
                            }
                            Err(ReadError { kind: ReadErrorKind::Io(e), .. }) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                            Err(e) => panic!("error reading from UNIX socket: {e} ({e:?})"),
                        }
                    }
                });
            }
        }
    }
    Ok(())
}
