# Async Systems Analysis

## Executive Summary

This application has **two completely separate async race systems** that were developed independently and are not currently integrated:

1. **Web-based Qualifier System** - Pre-tournament qualifiers with manual request/submit workflow
2. **Discord Thread-based Async Race Manager** - In-tournament bracket races with automated Discord bot workflow

This document analyzes both systems and their current state.

---

## System 1: Web-based Qualifier System

### Purpose
Pre-tournament qualifiers where teams request a seed, play independently, and manually submit results via web forms.

### Database Tables

#### `asyncs` table
Defines qualifier periods for events.

```sql
CREATE TABLE public.asyncs (
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    kind async_kind NOT NULL,        -- 'qualifier', 'qualifier2', 'qualifier3', 'seeding', 'tiebreaker1', 'tiebreaker2'
    start TIMESTAMP WITH TIME ZONE,  -- When qualifier becomes available
    end_time TIMESTAMP WITH TIME ZONE, -- When qualifier closes

    -- Seed information
    web_id BIGINT,                   -- OOTR web seed ID
    web_gen_time TIMESTAMP,
    file_stem TEXT,                  -- For file-based seeds
    hash1-5 hash_icon,               -- Seed hash icons
    tfb_uuid UUID,                   -- Triforce Blitz seed
    xkeys_uuid UUID,                 -- Crosskeys seed
    seed_password CHAR(6),           -- Seed password (A/v/>/</^)
    is_tfb_dev BOOLEAN,

    -- Discord settings
    discord_role BIGINT,             -- Role to grant on submission
    discord_channel BIGINT,          -- Channel for announcements

    -- Other
    max_delay INTERVAL DEFAULT '00:00:00'  -- Deadline extension for odd teams
);
```

#### `async_teams` table
Tracks which teams have requested and submitted qualifiers.

```sql
CREATE TABLE public.async_teams (
    team BIGINT NOT NULL,
    kind async_kind NOT NULL,
    requested TIMESTAMP WITH TIME ZONE,  -- When team requested seed
    submitted TIMESTAMP WITH TIME ZONE,  -- When team submitted results
    fpa TEXT,                            -- Fair Play Agreement notes
    pieces SMALLINT                      -- Triforce Blitz pieces found
);
```

#### `async_players` table
Stores individual player times and VODs.

```sql
CREATE TABLE public.async_players (
    series VARCHAR(8),
    event VARCHAR(8),
    player BIGINT,
    kind async_kind,
    time INTERVAL,      -- Finish time (NULL = DNF)
    vod TEXT            -- VOD URL
);
```

### Code Files

- **`src/event/mod.rs`** (lines 550-565, 2088-2337)
  - `active_async()` - Determines which async is currently active
  - `request_async()` - Handles team requesting qualifier
  - `submit_async()` - Handles team submitting results
  - Status page rendering with seed display and forms

- **`src/event/teams.rs`** (lines 319-340)
  - Points calculation for score-based qualifiers
  - Standard formula (Jet + Gamble bonuses)
  - SGL Online variants (par-time ratio)

### User Flow

1. **Setup** (Manual by organizer)
   - Organizer generates seed
   - Organizer runs SQL INSERT into `asyncs` table
   - No admin UI exists

2. **Request** (`/event/<series>/<event>/status`)
   - Team sees "Request Now" button if async is active
   - Team confirms they're ready
   - System updates `async_teams.requested = NOW()`
   - Redirects to status page

3. **Seed Display**
   - After requesting, seed information appears
   - Shows download link, hash icons, password
   - Displays submit form

4. **Play**
   - Team downloads seed independently
   - Team plays on their own time
   - Team records VOD (usually unlisted YouTube)

5. **Submit** (`/event/<series>/<event>/submit-async`)
   - Team fills out form with times and VOD links
   - System validates and stores in `async_teams` and `async_players`
   - System posts Discord notification
   - System grants discord_role to team members
   - Redirects to status page

### Limitations

- **No Discord thread creation** - Teams just get a seed link
- **No automated time tracking** - Teams self-report times
- **No seed distribution workflow** - Seed is immediately visible
- **No countdown or READY button** - Just a confirmation checkbox
- **Manual setup required** - SQL INSERT for each qualifier

---

## System 2: Discord Thread-based Async Race Manager

### Purpose
In-tournament bracket races that run asynchronously with automated Discord bot workflow for seed distribution, countdown, and time tracking.

### Database Tables

#### `races` table (relevant fields)
```sql
CREATE TABLE public.races (
    id BIGINT NOT NULL,
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,

    -- Async schedule fields
    async_start1 TIMESTAMP WITH TIME ZONE,
    async_start2 TIMESTAMP WITH TIME ZONE,
    async_start3 TIMESTAMP WITH TIME ZONE,
    async_end1 TIMESTAMP WITH TIME ZONE,
    async_end2 TIMESTAMP WITH TIME ZONE,
    async_end3 TIMESTAMP WITH TIME ZONE,
    async_room1 TEXT,
    async_room2 TEXT,
    async_room3 TEXT,

    -- Discord thread IDs
    async_thread1 BIGINT,
    async_thread2 BIGINT,
    async_thread3 BIGINT,

    -- Constraint: Either live (start) OR async, never both
    CONSTRAINT async_exclusion CHECK (
        ((start IS NULL) OR
         ((async_start1 IS NULL) AND (async_start2 IS NULL) AND (async_start3 IS NULL)))
    )
);
```

#### `async_times` table
Tracks start/finish times for each part of async races.

```sql
CREATE TABLE public.async_times (
    race_id BIGINT NOT NULL,
    async_part SMALLINT NOT NULL,    -- 1, 2, or 3
    start_time TIMESTAMP WITH TIME ZONE,
    finish_time TIMESTAMP WITH TIME ZONE,
    recorded_by BIGINT,
    recorded_at TIMESTAMP WITH TIME ZONE
);
```

### Code Files

- **`src/async_race.rs`** (entire file, ~800 lines)
  - `create_async_threads()` - Creates Discord threads 30 min before start
  - `create_async_thread()` - Creates individual private thread
  - `handle_ready_button()` - Distributes seed when player clicks READY
  - `handle_start_countdown_button()` - 5-second countdown
  - `handle_finish_button()` - Records completion time
  - Button interaction handlers for Discord

- **`src/main.rs`** (line 325)
  - Background task calls `create_async_threads()` every minute

### User Flow

1. **Setup**
   - Race created with `async_start1/2/3` times (via bracket generation or manual)
   - Event has `discord_async_channel` configured

2. **Thread Creation** (Automated, 30 min before start)
   - Bot creates private Discord thread
   - Thread name: "Async {Round}: {Player} (1st/2nd/3rd)"
   - Adds player and organizers (excluding opponents)
   - Posts welcome message with READY button

3. **Ready Workflow**
   - Player clicks "READY!" button
   - Bot distributes seed immediately
   - Button changes to "START COUNTDOWN"

4. **Countdown Workflow**
   - Player clicks "START COUNTDOWN"
   - Bot counts down 5...4...3...2...1...GO!
   - Records `start_time` in `async_times` table
   - Button changes to "FINISH"

5. **Finish Workflow**
   - Player clicks "FINISH"
   - Records `finish_time` in `async_times` table
   - Calculates duration
   - Posts completion message

### Limitations

- **No integration with `asyncs` table** - Works only with `races` table
- **No integration with `async_teams` table** - Doesn't track request/submit
- **Only for bracket races** - Not designed for qualifiers
- **Requires scheduled times** - Can't work on-demand

---

## The Disconnect

### Zero Integration

Looking at `src/async_race.rs`:
- **Zero references** to `async_teams` table
- **Zero references** to `request_async` or `submit_async` endpoints
- **Zero references** to `asyncs` table
- Works entirely with `races` table

Looking at `src/event/mod.rs` (qualifier system):
- **Zero references** to `async_race` module
- **Zero references** to Discord threads
- **Zero references** to `async_times` table
- Works entirely with `asyncs` and `async_teams` tables

### What Actually Happens

**If you request a qualifier via web:**
- Updates `async_teams.requested = NOW()`
- Shows seed information on status page
- No Discord thread created
- No automated workflow
- Team manually plays and submits

**If you have an async bracket race:**
- Bot creates Discord thread at scheduled time
- Automated READY/countdown/finish workflow
- Records times to `async_times` table
- Never touches `asyncs` or `async_teams` tables
- Likely created via bracket generation, not qualifier submission

### Database Evidence

The `asyncs` table has a `discord_channel` field, but it's only used for:
- Posting a simple text notification when teams submit results
- NOT for creating threads or managing workflow

The `races` table has `async_thread1/2/3` fields, but:
- Only populated by `async_race.rs` module
- Never referenced by qualifier system

---

## Qualifier Types

The system supports multiple qualifier implementations:

### QualifierKind Enum (`src/event/teams.rs:23-31`)

```rust
pub(crate) enum QualifierKind {
    None,                            // No qualifier
    Rank,                            // Predetermined rank (qualifier_rank field on teams)
    Single { show_times: bool },     // Single async qualifier (uses asyncs table)
    Score(QualifierScoreKind),       // Score-based qualifier with points formula
    SongsOfHope,                     // Special case for Songs of Hope event
}

pub(crate) enum QualifierScoreKind {
    Standard,                        // Standard scoring formula (Jet + Gamble)
    Sgl2023Online,                   // SGL 2023 online specific
    Sgl2024Online,                   // SGL 2024 online specific
    Sgl2025Online,                   // SGL 2025 online specific
}
```

### Qualifier Detection (`src/event/mod.rs:432-457`)

```rust
pub(crate) async fn qualifier_kind(&self) -> QualifierKind {
    match (self.series, &*self.event) {
        (Series::SongsOfHope, "1") => QualifierKind::SongsOfHope,

        (Series::SpeedGaming, "2023onl" | "2024onl" | "2025onl") | (Series::Standard, "8") => {
            QualifierKind::Score(/* variant based on event */)
        }

        _ => {
            // Check if teams have qualifier_rank
            if exists(teams WHERE qualifier_rank IS NOT NULL) {
                QualifierKind::Rank
            }
            // Check if asyncs table has qualifier entry
            else if exists(asyncs WHERE kind='qualifier') {
                QualifierKind::Single { show_times: ... }
            }
            else {
                QualifierKind::None
            }
        }
    }
}
```

---

## Points Calculation

### Standard Scoring Formula (`src/event/teams.rs:319-340`)

Used by Standard Series and SGL events.

```
1. Get top 7 finishers (PAR cutoff) or fewer if < 7 entrants
2. Calculate t_average = average time of PAR group
3. Calculate Jet (time bonus) using:
   t_j_h = 8 min × clamp(0 to 1) of (2.5h - t_avg) / 50min
   t_jet = min(8 min, t_j_h × 0.35 × (finish - t_avg)/8min)
4. Calculate Gamble bonus using standard deviation:
   t_g_h = sqrt(sum((finish - t_avg)²) / (par_cutoff - 1))
   sigma = t_g_h / t_avg
   t_gamble = min(5 min, t_g_h × 0.3 × max(0, (finish - t_avg)/t_g_h × (sigma/0.035 - 1)))
5. Final score:
   (1 - (finish - t_avg - min(10 min, t_jet + t_gamble)) / t_avg) × 1000
   Clamped to [100, 1100]
```

Reference: [Google Docs formula](https://docs.google.com/document/d/e/2PACX-1vRrVPO_GvlRZfUwZFrp0ehnRHNzTWF-2PDSyNDPsJv5GcFXXvh5Ye_DcZrqvvCGVCfk5g42wB9EaRGU/pub)

### SGL Online Scoring

```
1. Get top 3-4 finishers based on entrant count (< 20: 3; >= 20: 4)
2. Calculate par_time = average of PAR group
3. Score = (100 × (2.0 - finish_time / par_time))
4. Clamped to [10, 110]
```

---

## Active Async Logic (`src/event/mod.rs:550-565`)

Determines which async (if any) is currently active for a team.

```rust
pub(crate) async fn active_async(&self, team_id: Option<Id<Teams>>) -> Option<AsyncKind> {
    // Get all asyncs in time window
    for kind in SELECT kind FROM asyncs
                WHERE series = $1 AND event = $2
                AND (start IS NULL OR start <= NOW())
                AND (end_time IS NULL OR end_time > NOW()) {

        match kind {
            // Qualifiers: Only active before event starts
            AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 => {
                if !self.is_started() {
                    return Some(kind)
                }
            }

            // Seeding: Always active if in time window
            AsyncKind::Seeding => return Some(kind),

            // Tiebreakers: Only for specific teams that have an async_teams entry
            AsyncKind::Tiebreaker1 | AsyncKind::Tiebreaker2 => {
                if let Some(team_id) = team_id {
                    if exists(async_teams WHERE team = team_id AND kind = kind) {
                        return Some(kind)
                    }
                }
            }
        }
    }
    Ok(None)
}
```

---

## Event Configuration

### Relevant Event Table Columns

- `asyncs_active` (BOOLEAN, default: true) - Enable/disable async races
- `discord_async_channel` (BIGINT) - Channel for Discord thread-based async system
- `show_qualifier_times` (BOOLEAN, default: true) - Show qualifier times
- `discord_organizer_channel` (BIGINT) - Fallback for notifications
- `discord_guild` (BIGINT) - Guild ID for Discord features

### Configure UI

Located at `/event/<series>/<event>/configure` (file: `src/event/configure.rs:131-135`)

```rust
form_field("asyncs_active", &mut errors, html! {
    input(type = "checkbox", id = "asyncs_active", name = "asyncs_active",
          checked? = event.asyncs_active);
    label(for = "asyncs_active") : "Allow async races";
    label(class = "help") : "(If disabled, Discord scheduling threads will not mention
                              the /schedule-async command and async races will not be possible)";
});
```

**NOTE:** This only controls whether async races are *allowed*, not which system is used.

---

## Summary

### Web-based Qualifier System
- **Tables:** `asyncs`, `async_teams`, `async_players`
- **Flow:** Request → View Seed → Play → Submit
- **Setup:** Manual SQL INSERT
- **Discord:** Simple text notification on submit
- **Time Tracking:** Self-reported by teams

### Discord Thread-based Async Race Manager
- **Tables:** `races` (async_start/thread fields), `async_times`
- **Flow:** Thread Creation → READY → Countdown → FINISH
- **Setup:** Bracket generation or manual race creation
- **Discord:** Full bot workflow with buttons
- **Time Tracking:** Automated via button clicks

### Key Insight
These are **two completely independent systems** that happen to both handle "async" races. They share no code, no database tables, and no workflows. The web-based system is for pre-tournament qualifiers; the Discord system is for in-tournament bracket matches.
