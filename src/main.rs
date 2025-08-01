#![recursion_limit = "512"]

//#[global_allocator]
//static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc; // fails to compile on Ubuntu

use {
    std::env,
    rocket::Rocket,
    serde_json_inner as _, // `preserve_order` feature required to correctly render progression spoilers
    sqlx::{
        ConnectOptions as _,
        postgres::{
            PgConnectOptions,
            PgPoolOptions,
        },
    },
    crate::prelude::*,
};
#[cfg(unix)] use {
    mhstatus::PrepareStopUpdate,
    openssl as _, // `vendored` feature required to fix release build
    tokio::{
        io::{
            AsyncWriteExt as _,
            stdout,
        },
        net::UnixStream,
    },
    crate::{
        racetime_bot::SeedRollUpdate,
        unix_socket::ClientMessage as Subcommand,
    },
};

mod admin;
mod api;
mod async_race;
mod auth;
mod cal;
mod challonge;
mod config;
mod discord_bot;
mod discord_role_manager;
mod draft;
mod event;
mod favicon;
mod form;
mod game;
mod games;
mod hash_icon;
mod hash_icon_db;
#[macro_use] mod http;
mod id;
mod lang;
mod legal;
#[macro_use] mod macros;

mod notification;
mod ootr_web;
mod prelude;
mod racetime_bot;
mod seed;
mod series;
mod sheets;
mod startgg;
mod team;
mod time;
#[cfg(unix)] mod unix_socket;
mod user;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[allow(unused)] // variants only constructed under conditional compilation
#[derive(Default, Clone, Copy)]
enum Environment {
    #[cfg_attr(any(feature = "production", not(any(feature = "dev", feature = "local", debug_assertions))), default)]
    Production,
    #[cfg_attr(any(feature = "dev", all(debug_assertions, not(feature = "production"), not(feature = "local"))), default)]
    Dev,
    #[cfg_attr(feature = "local", default)]
    Local,
}

impl Environment {
    fn is_dev(&self) -> bool {
        match self {
            Self::Production => false,
            Self::Dev => true,
            Self::Local => true,
        }
    }

    fn night_path(&self) -> &'static str {
        if self.is_dev() { "/var/log/midoshouse/dev" } else { "/var/log/midoshouse/prod" }
    }

    fn racetime_host(&self) -> &'static str {
        if self.is_dev() { "rtdev.zeldaspeedruns.com" } else { "racetime.gg" }
    }

    fn base_uri(&self) -> rocket::http::uri::Absolute<'static> {
        match self {
            Self::Production => uri!("https://hth.zeldaspeedruns.com"),
            Self::Dev => uri!("https://hthdev.zeldaspeedruns.com"),
            Self::Local => uri!("http://localhost:24814"),
        }
    }
}

fn night_path() -> &'static str {
    Environment::default().night_path()
}

fn racetime_host() -> &'static str {
    Environment::default().racetime_host()
}

fn base_uri() -> rocket::http::uri::Absolute<'static> {
    Environment::default().base_uri()
}

fn parse_port(arg: &str) -> Result<u16, std::num::ParseIntError> {
    match arg {
        "production" => Ok(24812),
        "dev" => Ok(24814),
        _ => arg.parse(),
    }
}

#[cfg(not(unix))]
#[derive(clap::Subcommand)]
enum Subcommand {}

#[derive(clap::Parser)]
#[clap(version = CLAP_VERSION)]
struct Args {
    #[clap(long, value_parser = parse_port)]
    port: Option<u16>,
    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)] Any(#[from] anyhow::Error),
    #[error(transparent)] Base64(#[from] base64::DecodeError),
    #[error(transparent)] Event(#[from] event::Error),
    #[cfg(unix)] #[error(transparent)] Io(#[from] io::Error),
    #[error(transparent)] Racetime(#[from] racetime_bot::MainError),
    #[cfg(unix)] #[error(transparent)] Read(#[from] async_proto::ReadError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Rocket(#[from] rocket::Error),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Task(#[from] tokio::task::JoinError),
    #[cfg(unix)] #[error(transparent)] Wheel(#[from] wheel::Error),
    #[cfg(unix)] #[error(transparent)] Write(#[from] async_proto::WriteError),
}

#[wheel::main(rocket)]
async fn main(Args { port, subcommand }: Args) -> Result<(), Error> {
    if let Some(subcommand) = subcommand {
        #[cfg(unix)] let mut sock = UnixStream::connect(unix_socket::PATH).await?;
        #[cfg(unix)] subcommand.write(&mut sock).await?;
        match subcommand {
            #[cfg(unix)] Subcommand::CleanupRoles { .. } => {
                u8::read(&mut sock).await?;
            }
            #[cfg(unix)] Subcommand::PrepareStop { async_proto: false, .. } => {
                while let Some(update) = Option::<PrepareStopUpdate>::read(&mut sock).await? {
                    println!("{} preparing to stop HTH: {update}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
                }
                println!("{} preparing to stop HTH: done", Utc::now().format("%Y-%m-%d %H:%M:%S"));
            }
            #[cfg(unix)] Subcommand::PrepareStop { async_proto: true, .. } => {
                let mut stdout = stdout();
                while let Some(update) = Option::<PrepareStopUpdate>::read(&mut sock).await? {
                    update.write(&mut stdout).await?;
                    stdout.flush().await?;
                }
            }
            #[cfg(unix)] Subcommand::Roll { .. } | Subcommand::RollRsl { .. } | Subcommand::Seed { .. } => while let Some(update) = Option::<SeedRollUpdate>::read(&mut sock).await? {
                println!("{} {update:#?}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
            },
            #[cfg(unix)] Subcommand::UpdateRegionalVc { .. } => {
                println!("{} HTH: updating regional voice chat", Utc::now().format("%Y-%m-%d %H:%M:%S"));
                u8::read(&mut sock).await?;
                println!("{} HTH: done updating regional voice chat", Utc::now().format("%Y-%m-%d %H:%M:%S"));
            }
        }
    } else {
        let default_panic_hook = std::panic::take_hook();
        if let Environment::Production = Environment::default() {
            std::panic::set_hook(Box::new(move |info| {
                let _ = wheel::night_report_sync(&format!("{}/error", night_path()), Some("thread panic"));
                default_panic_hook(info)
            }));
        }
        let config = Config::load().await?;
        let http_client = reqwest::Client::builder()
            .user_agent(concat!("MidosHouse/", env!("CARGO_PKG_VERSION"), " (https://github.com/midoshouse/midos.house)"))
            .timeout(Duration::from_secs(30))
            .use_rustls_tls()
            .hickory_dns(true)
            .https_only(true)
            .build()?;
        let insecure_http_client = reqwest::Client::builder()
            .user_agent(concat!("MidosHouse/", env!("CARGO_PKG_VERSION"), " (https://github.com/midoshouse/midos.house)"))
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true) // https://discord.com/channels/274180765816848384/1012773802201071736/1372836620822122526
            .use_rustls_tls()
            .hickory_dns(true)
            .build()?;
        let discord_config = if Environment::default().is_dev() { &config.discord_dev } else { &config.discord_production };
        let discord_builder = serenity_utils::builder(discord_config.bot_token.clone()).await?;
        let db_pool = PgPoolOptions::default()
            .max_connections(16)
            .connect_with(PgConnectOptions::default()
                .username("mido")
                .database(if Environment::default().is_dev() { "fados_house" } else { "midos_house" })
                .application_name("midos-house")
                .log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(10))
            ).await?;
        let seed_metadata = Arc::default();
        let ootr_api_client = Arc::new(ootr_web::ApiClient::new(http_client.clone(), config.ootr_api_key.clone(), config.ootr_api_key_encryption.clone()));
        let rocket = http::rocket(
            db_pool.clone(),
            discord_builder.ctx_fut.clone(),
            http_client.clone(),
            config.clone(),
            port.unwrap_or_else(|| if Environment::default().is_dev() { 24814 } else { 24812 }),
            Arc::clone(&seed_metadata),
            ootr_api_client.clone(),
        ).await?;
        let new_room_lock = Arc::default();
        let clean_shutdown = Arc::default();
        // Create a default racetime config for fallback (will be overridden by database-driven credentials)
        let racetime_config = config::ConfigRaceTime {
            client_id: String::new(),
            client_secret: String::new(),
        };
        let startgg_token = if Environment::default().is_dev() { &config.startgg_dev } else { &config.startgg_production };
        let (seed_cache_tx, seed_cache_rx) = watch::channel(());
        let global_state = Arc::new(racetime_bot::GlobalState::new(
            Arc::clone(&new_room_lock),
            racetime_config,
            db_pool.clone(),
            http_client.clone(),
            insecure_http_client,
            config.league_api_key.clone(),
            startgg_token.clone(),
            ootr_api_client,
            discord_builder.ctx_fut.clone(),
            Arc::clone(&clean_shutdown),
            seed_cache_tx,
            seed_metadata,
        ).await);
        let discord_builder = discord_bot::configure_builder(discord_builder, global_state.clone(), db_pool.clone(), http_client.clone(), config.clone(), Arc::clone(&new_room_lock), Arc::clone(&clean_shutdown), rocket.shutdown());
        #[cfg(unix)] let unix_listener = unix_socket::listen(rocket.shutdown(), clean_shutdown, global_state.clone());
        let racetime_task = tokio::spawn(racetime_bot::main(config.clone(), rocket.shutdown(), global_state, seed_cache_rx)).map(|res| match res {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(Error::from(e)),
            Err(e) => Err(Error::from(e)),
        });
        let import_task = tokio::spawn(cal::auto_import_races(db_pool.clone(), http_client.clone(), config, rocket.shutdown(), discord_builder.ctx_fut.clone(), new_room_lock)).map(|res| match res {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(Error::from(e)),
            Err(e) => Err(Error::Task(e)),
        });
        let async_race_task = tokio::spawn(async_race_manager(db_pool, discord_builder.ctx_fut.clone(), http_client, rocket.shutdown())).map(|res| match res {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(Error::Task(e)),
        });
        let rocket_task = tokio::spawn(rocket.launch()).map(|res| match res {
            Ok(Ok(Rocket { .. })) => Ok(()),
            Ok(Err(e)) => Err(Error::from(e)),
            Err(e) => Err(Error::from(e)),
        });
        let discord_task = tokio::spawn(discord_builder.run()).map(|res| match res {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(Error::from(e)),
            Err(e) => Err(Error::from(e)),
        });
        #[cfg(unix)] let unix_socket_task = tokio::spawn(unix_listener).map(|res| match res {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(Error::from(e)),
            Err(e) => Err(Error::from(e)),
        });
        #[cfg(not(unix))] let unix_socket_task = future::ok(());
        let ((), (), (), (), (), ()) = tokio::try_join!(discord_task, import_task, racetime_task, async_race_task, rocket_task, unix_socket_task)?;
    }
    Ok(())
}

/// Background task for managing async races
async fn async_race_manager(
    db_pool: PgPool,
    discord_ctx: RwFuture<DiscordCtx>,
    http_client: reqwest::Client,
    shutdown: rocket::Shutdown,
) -> Result<(), Error> {
    let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
    
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let discord_ctx = discord_ctx.read().await;
                if let Ok(()) = async_race::AsyncRaceManager::create_async_threads(&db_pool, &discord_ctx, &http_client).await {
                    // Threads created successfully
                }
            }
            _ = shutdown.clone() => break,
        }
    }
    
    Ok(())
}
