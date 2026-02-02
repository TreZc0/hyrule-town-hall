# Hyrule Town Hall (HTH)

Tournament/event management platform for Zelda randomizer speedrunning. Fork of Mido's House. Rust/Rocket web app with PostgreSQL, Discord bot (serenity), and racetime.gg integration.

## Architecture

```
src/
  main.rs           # Entry point, spawns all background tasks
  http.rs           # Rocket web routes
  discord_bot.rs    # Discord slash commands, buttons, event handlers
  racetime_bot/     # racetime.gg bot (room creation, chat commands, results)
    mod.rs          # Goals, seed commands, race handling
    report.rs       # Result reporting to Discord and start.gg
  cal.rs            # Race/calendar data model (Race, RaceSchedule, Entrants)
  event/            # Event configuration and management
    mod.rs          # Event data model, team configs
    asyncs.rs       # Async/qualifier management UI
    roles.rs        # Volunteer role system
    enter.rs        # Registration flows
    teams.rs        # Team management
  series/           # Tournament-specific logic per series
    mod.rs          # Series enum (21 series: Standard, RSL, Multiworld, etc.)
    s.rs            # Standard tournaments (S7 settings, drafts)
    mw.rs           # Multiworld tournaments
    rsl.rs          # Random Settings League
    tfb.rs          # Triforce Blitz
    ...             # Each series has settings, draft logic, presets
  startgg.rs        # start.gg GraphQL integration
  challonge.rs      # Challonge REST API integration
  async_race.rs     # Async race thread management
  volunteer_requests.rs  # Volunteer request announcements
  seed.rs           # Seed file storage and serving
  draft.rs          # Settings draft system
  ootr_web.rs       # ootrandomizer.com API client
```

## External Integrations

### racetime.gg
- **Location**: `src/racetime_bot/`
- **Connection**: Uses `racetime` crate (v0.32), WebSocket-based
- **Functionality**:
  - Creates race rooms (official/practice) via `new_room_lock`
  - Handles chat commands: `!seed`, `!draft`, `!pick`, `!ban`, `!fpa`, `!break`
  - Monitors race state (entrants, finishes, forfeits)
  - Reports results to Discord when races end
  - Per-goal configuration (Goal enum with ~30 tournament goals)
- **Seed Preroll**: Seeds can be pre-generated based on `PrerollMode` (None/Short/Medium/Long)
- **Room Invites**: Invites entrants based on team config and racetime IDs

### start.gg (GraphQL)
- **Location**: `src/startgg.rs`, `assets/graphql/startgg-*.graphql`
- **Rate Limit**: 80 requests/60s with caching (30min TTL)
- **Queries**:
  - `EventSetsQuery`: Import matches/sets from tournaments
  - `EntrantsQuery`: Fetch participant data and Swiss standings
  - `SetQuery`: Get current set state for result reporting
  - `UserSlugQuery`: User lookup
- **Mutations**:
  - `ReportOneGameResultMutation`: Report single game results
  - `ReportBracketSetMutation`: Report full set with game-by-game data
- **Features**:
  - Auto-import matches from start.gg events
  - Swiss standings calculation with bye detection
  - Multi-game match support (best-of-N)
  - Result reporting back to start.gg after races

### Challonge (REST API v2)
- **Location**: `src/challonge.rs`
- **Functionality**:
  - Import matches from Challonge brackets
  - Supports community tournaments (`/v2/communities/{community}/tournaments/{id}/matches.json`)
- **Limitations vs start.gg**:
  - No result reporting back to Challonge
  - No phase/round/game metadata (must be filled manually)
  - No Swiss/standings support
  - No GraphQL caching
  - Basic match import only

### Discord (serenity)
- **Location**: `src/discord_bot.rs`, `src/discord_scheduled_events.rs`, `src/discord_role_manager.rs`
- **Features**:
  - Scheduling threads for matches
  - Slash commands: `/schedule`, `/race`, `/restream`, `/result-async`, `/result-sync`, etc.
  - Button interactions (volunteer signup, async READY/COUNTDOWN/FINISH)
  - Role management for volunteers
  - Scheduled events creation for restreamed matches
  - Race results posting to configured channels

## Async/Qualifier System
- **Location**: `src/async_race.rs`, `src/event/asyncs.rs`
- **Types** (`AsyncKind`): Qualifier1/2/3, Seeding, Tiebreaker1/2
- **Bracket Asyncs** (`RaceSchedule::Async`):
  - Up to 3 async starts per race (start1/start2/start3)
  - Private Discord threads created 30min before start
  - READY button reveals seed, START COUNTDOWN begins timer
  - Organizers verify times via `/result-async`
- **Qualifier Asyncs** (`automated_asyncs=true`):
  - Teams request qualifier via web UI
  - System creates private thread when seed is ready
  - Same READY/COUNTDOWN/FINISH flow
- **Database**: `async_teams` (requests/submissions), `async_times` (start/finish times), `asyncs` (seed data)

## Seed Generation
- **Location**: `src/seed.rs`, `src/ootr_web.rs`, `src/racetime_bot/mod.rs`
- **Sources**:
  - `ootrandomizer.com` API: Most OoTR seeds, version/branch selection
  - Local generation: Python randomizer for specific branches
  - `triforceblitz.com`: TFB seeds
  - `alttprpatch.synack.live`: ALTTP door rando (Crosskeys)
  - `seedbot.twwrando.com` TWWR permalinks: Wind Waker randomizer
- **Storage**: `seed::Files` enum, files in `/var/www/midos.house/seed/` (Unix) or `G:/source/hth-seeds` (Windows)
- **Prerolled Seeds**: `prerolled_seeds` table for events with long generation times
- **Hash/Password**: Extracted from spoiler log, displayed on race pages

## Volunteer Management
- **Location**: `src/volunteer_requests.rs`, `src/event/roles.rs`
- **Roles**: Defined per-game or per-event (restreamers, commentators, trackers, etc.)
- **Role Bindings**: `role_bindings` table with min/max counts per role
- **Signup Flow**:
  1. User requests role via web UI or Discord button
  2. Auto-approve or pending organizer approval
  3. Assigned to specific races via `signups` table
- **Announcements**: Background task checks every 30min, posts to `discord_volunteer_info_channel`
- **Lead Time**: Configurable `volunteer_request_lead_time_hours`

## Settings Draft System
- **Location**: `src/draft.rs`
- **Draft Kinds**: S7, MultiworldS3/S4/S5, RSL, TournoiFranco, AlttprDe9
- **Flow**:
  1. High seed chooses to go first/second
  2. Alternating bans/picks based on draft kind rules
  3. Settings resolved into randomizer plando
- **Commands**: `!draft`, `!pick <setting> <value>`, `!ban <setting>`, `!skip`
- **Random Draft**: `!seed random` simulates draft with random picks

## Key Data Models

### Race (`cal.rs`)
```rust
struct Race {
    id, series, event, source,
    entrants: Entrants,           // Two/Three team matchups
    schedule: RaceSchedule,       // Unscheduled/Live/Async
    draft: Option<Draft>,
    seed: seed::Data,
    phase, round, game,           // Match identification
    scheduling_thread,
    video_urls, restreamers,
}
```

### Event (`event/mod.rs`)
```rust
struct Data {
    series, event, display_name,
    team_config: TeamConfig,      // Solo/CoOp/Pictionary/Multiworld
    discord_*_channel,            // Various Discord channels
    rando_version, settings_string,
    asyncs_active, automated_asyncs,
    volunteer_requests_enabled,
    swiss_standings,
}
```

### Series (`series/mod.rs`)
21 tournament series: Standard, League, RSL, Multiworld, SpeedGaming, TriforceBlitz, Crosskeys, AlttprDe, TournoiFrancophone, Pictionary, CoOp, etc.

## Configuration
- **Location**: `cfg/` directory, loaded in `src/config.rs`
- **Keys**: Discord bot token, start.gg token, Challonge API key, OOTR API key, League API key
- **Database**: PostgreSQL with sqlx (compile-time checked queries in `.sqlx/`)

## Patterns & Conventions

### Prelude (`src/prelude.rs`)
All modules import `use crate::prelude::*;` which provides:
- Standard library: `Cow`, `HashMap`, `HashSet`, `Duration`, `Arc`, `Mutex`
- Chrono: `DateTime`, `Utc`, `NaiveDate`, `NaiveTime`, timezone types
- Rocket: `Form`, `Context`, `Status`, `Redirect`, `State`, `Origin`
- SQLx: `PgPool`, `Transaction`, `Postgres`
- Serenity: `DiscordCtx`, `MessageBuilder`, model types
- Tokio: `mpsc`, `watch`, `broadcast`, `sleep`, `Instant`
- Custom: `User`, `Team`, `Race`, `Series`, form helpers, ID types

### CSRF Protection
Forms use `rocket_csrf` with the `CsrfForm` derive macro:
```rust
#[derive(FromForm, CsrfForm)]
pub(crate) struct MyForm {
    #[field(default = String::new())]
    csrf: String,
    // other fields...
}

// In handler:
form.verify(&csrf);  // Must call before processing
```
- Pass `csrf: Option<&CsrfToken>` to form rendering functions
- Use `full_form()` or `button_form()` from `src/form.rs` which handle CSRF automatically

### Error Handling
- Each module defines its own `Error` enum with `#[derive(thiserror::Error, rocket_util::Error)]`
- Implement `IsNetworkError` trait for retry logic
- Use `StatusOrError<E>` for web handlers that can return HTTP status or error
- racetime errors use `.to_racetime()?` extension method for conversion

### Authorization
```rust
// Global admin check
if !me.is_global_admin() { ... }

// Event organizer check
if !event.organizers(&mut transaction).await?.contains(&me) { ... }

// Combined checks common pattern
if !me.is_global_admin() && !event.organizers(&mut transaction).await?.contains(&me) {
    return Err(StatusOrError::Status(Status::Forbidden));
}
```
- `User::GLOBAL_ADMIN_USER_IDS` contains admin user IDs
- `event.organizers()` returns list of organizers for an event
- `event.restreamers()` returns list of restreamers

### Typed IDs (`src/id.rs`)
Uses phantom-typed IDs for type safety:
```rust
pub(crate) struct Id<T: Table> { inner: u64, _table: PhantomData<T> }

// Table marker types
pub(crate) enum Users {}
pub(crate) enum Teams {}
pub(crate) enum Races {}
pub(crate) enum Notifications {}
// etc.

// Usage
fn get_user(id: Id<Users>) -> User { ... }
Id::<Teams>::new(&mut transaction).await?  // Generate new random ID
```

### Database Queries
- SQLx compile-time checked queries with type annotations
- Always use transactions: `pool.begin().await?`
- Type annotations for custom types: `AS "field: CustomType"`
```rust
sqlx::query!(r#"SELECT id AS "id: Id<Users>", series AS "series: Series" FROM ..."#, ...)
```

### Form Helpers (`src/form.rs`)
- `EmptyForm`: CSRF-only form for simple buttons
- `form_field()`: Wrap field with error display
- `full_form()`: Complete form with CSRF, content, submit button
- `button_form()`: Simple POST button with CSRF

### HTML Rendering
Uses `rocket_util::html!` macro (horrorshow-based):
```rust
html! {
    div(class = "container") {
        h1 : "Title";
        @if condition { p : "Conditional"; }
        @for item in items { li : item; }
        : some_raw_html;  // Include RawHtml
    }
}
```

### Discord Snowflakes
`PgSnowflake<T>` wrapper for storing Discord IDs in PostgreSQL as BIGINT:
```rust
discord_guild AS "discord_guild: PgSnowflake<GuildId>"
```

### Language Support (`src/lang.rs`)
`Language` enum: English, French, German, Portuguese
- `language.format_duration()` for localized time formatting
- `language.join_html()` for localized list joining ("and" vs "et" vs "und")

### Match Sources (`cal.rs`)
```rust
enum Source {
    Manual,
    Challonge { id: String },
    League { id: i32 },
    Sheet { timestamp: NaiveDateTime },
    StartGG { event: String, set: startgg::ID },
    SpeedGaming { id: i64 },
}
```

## Additional Integrations

### Google Sheets (`src/sheets.rs`)
- Rate limited (1 req/sec), cached reads
- Used for events that publish schedules as spreadsheets
- Service account auth via `assets/google-client-secret.json`

### SpeedGaming
- Series for SGL (SpeedGaming Live) events
- Imports matches with SpeedGaming IDs
- Integrates with restream scheduling

### GraphQL API (`src/api.rs`)
- External API using `async_graphql`
- Scoped API keys: `entrants_read`, `user_search`, `write`
- Available at `/api/v1/graphql`

### Games Layer (`src/game.rs`)
- Games table for multi-game support (OoTR, ALTTPR, TWWR, etc.)
- `game_series` links games to tournament series
- `game_admins` for per-game admin permissions
- `game_racetime_connections` for racetime category credentials

### Weekly Schedules (`src/weekly.rs`)
- Database-driven recurring race schedules
- Configurable frequency, time, timezone, anchor date
- Room opens X minutes before scheduled time

### Notifications (`src/notification.rs`)
- In-app notification system
- Types: Accept, Decline, Resign (team invites)
- Displayed on user's profile/dashboard

## Development

### Build
```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo build --features dev     # Dev environment
cargo build --features local   # Local environment
```

### Database
- PostgreSQL with compile-time query checking
- Migrations in `migrations/` directory
- Query cache in `.sqlx/` (commit this)

### Environments
| Environment | Port | racetime host |
|------------|------|---------------|
| Production | 24812 | racetime.gg |
| Dev | 24814 | rtdev.zeldaspeedruns.com |
| Local | 24814 | localhost |

### Adding New Series
1. Add variant to `Series` enum in `src/series/mod.rs`
2. Implement `slug()` and `display_name()` for the variant
3. Create series module in `src/series/` if needed
4. Add Goal variants in `src/racetime_bot/mod.rs` if needed
5. Add draft kind in `src/draft.rs` if drafting is used
