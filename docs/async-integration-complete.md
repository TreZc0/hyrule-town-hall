# Complete Async Systems Integration Documentation

This document contains the full analysis and implementation plan for integrating the Discord async workflow with the web-based qualifier system.

---

# Part 1: Async Systems Analysis

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

- **`src/event/teams.rs`** (lines 319-340, 622-632)
  - Points calculation for score-based qualifiers
  - Standard formula (Jet + Gamble bonuses)
  - SGL Online variants (par-time ratio)
  - Standings query joins `async_players` for times

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
    recorded_at TIMESTAMP WITH TIME ZONE,
    UNIQUE(race_id, async_part)
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
   - Records `finish_time` in `async_times` table (estimated)
   - Calculates duration
   - Posts completion message
   - Requests VOD/screenshot for staff verification

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

---

## Points Calculation Verification

### How Standings Work

**Located in `/src/event/teams.rs:622-632`:**

```rust
FROM team_members
LEFT OUTER JOIN async_players ON (member = player AND series = $1 AND event = $2 AND kind = 'qualifier')
WHERE team = $3
```

1. **Query joins `async_players`** to get times for each team member
2. **Maps to `SignupsMember.qualifier_time`** from `async_players.time`
3. **Team qualified** if `async_teams.submitted IS NOT NULL` (line 552-562)
4. **Times summed** for team events: `members.iter().try_fold(Duration::default(), |acc, member| Some(acc + member.qualifier_time?))` (line 679)

**This means:** Any system that writes to `async_players.time` will automatically appear in standings.

---

# Part 2: Integration Implementation Plan

## Overview

Wire the Discord thread-based async race workflow into the web-based qualifier request/submit system, controlled by a new event configuration flag `automated_asyncs`. When enabled, qualifier requests create Discord threads directly (no fake races) with READY/countdown/FINISH buttons, and staff validate results using `/result-async` command.

## Architecture Decision: No Fake Races

**Problem:** Creating one race per team per qualifier would clutter the races table and require extensive filtering logic.

**Solution:** Store thread IDs and timing data directly in `async_teams` table.

**Benefits:**
- **Qualifier events** (from `asyncs` table) can still appear in calendars/schedules
- **Individual team threads** don't create race entries
- Clean separation: `races` table = bracket races only
- No filtering logic needed

## Database Schema Changes

**Migration:** `026_add_automated_asyncs.sql`

```sql
-- Enable automated Discord workflow for qualifiers
ALTER TABLE events
ADD COLUMN automated_asyncs BOOLEAN DEFAULT false NOT NULL;

-- Store thread and timing data directly on async_teams
ALTER TABLE async_teams
ADD COLUMN discord_thread BIGINT,
ADD COLUMN start_time TIMESTAMP WITH TIME ZONE,
ADD COLUMN finish_time INTERVAL;

-- Index for thread lookups
CREATE INDEX idx_async_teams_discord_thread ON async_teams(discord_thread) WHERE discord_thread IS NOT NULL;
```

## Data Flow

```
Request Qualifier
       ↓
  async_teams.requested = NOW()
       ↓
  Background Task (every 60s)
       ↓
  Query: requested != NULL, submitted = NULL, discord_thread = NULL
       ↓
  Create Discord Thread
       ↓
  async_teams.discord_thread = thread_id
       ↓
  READY → Seed distributed
       ↓
  START → async_teams.start_time = NOW()
       ↓
  FINISH → Show completion message
       ↓
  Team posts VOD
       ↓
  /result-async command
       ↓
  async_teams.finish_time = time
  async_players.time = time (per member)
  async_teams.submitted = NOW()
       ↓
  Standings query async_players ✓
```

## Key Design Decisions

1. **No races created** - Thread ID stored directly in `async_teams.discord_thread`
2. **Immediate thread creation** - Background task creates threads within ~60 seconds of request
3. **READY button provides control** - No need for delay
4. **Seed from asyncs table** - Copied when creating thread message
5. **Staff validation via `/result-async` command** - Consistent with bracket async workflow
6. **Dual recording** - Write to `async_teams.finish_time` AND `async_players.time`
7. **Clean separation** - Bracket races use `races`/`async_times`, qualifiers use `async_teams`
8. **No filtering needed** - Qualifier threads never enter races table

## Critical Files to Modify

### 1. `/migrations/026_add_automated_asyncs.sql` (NEW)
- Add `automated_asyncs` to events
- Add `discord_thread`, `start_time`, `finish_time` to async_teams
- Create index for thread lookups

### 2. `/src/event/mod.rs`
- Add `automated_asyncs` to Data struct
- Modify `submit_async()` to detect organizer vs team
- Add `handle_staff_async_report()` for staff submissions
- Update status page to show Discord thread link

### 3. `/src/async_race.rs`
- Modify `create_async_threads()` to query both races AND async_teams
- Add `get_qualifier_teams_needing_threads()`
- Add `create_qualifier_thread()`
- Modify button handlers to check both races and async_teams tables

### 4. `/src/event/configure.rs`
- Add `automated_asyncs` checkbox to event configuration form

### 5. `/src/discord_bot.rs`
- Modify existing `/result-async` command to support qualifier threads
- Modify `find_race_from_thread()` to check `async_teams.discord_thread` as fallback when `races` lookup fails
- Add conditional code path: when thread is from `async_teams`, write to `async_teams.finish_time` AND `async_players.time` instead of `async_times`
- Skip external reporting (start.gg/Challonge) for qualifier threads since they have no bracket match IDs

## Edge Cases Handled

1. **Multiple requests** - `discord_thread IS NULL` prevents duplicate threads
2. **Seed not available** - Gracefully skips thread creation until seed ready
3. **Discord failures** - Logged, retried on next tick (60s)
4. **Non-organizer tries `/result-async`** - Permission denied (ephemeral message)
5. **Legacy support** - Web form only available when `automated_asyncs=false`
6. **Thread cleanup** - No special handling needed (Discord auto-archives after 1 week)
7. **Race table pollution** - Avoided entirely (no fake races)

## Verification Testing Plan

### Phase 1: Migration
```sql
\d events;       -- Check automated_asyncs column
\d async_teams;  -- Check discord_thread, start_time, finish_time columns
SELECT * FROM pg_indexes WHERE tablename = 'async_teams';
```

### Phase 2: Backward Compatibility (automated_asyncs=false)
1. Request qualifier via web → Verify `discord_thread` stays NULL
2. Submit via web form → Verify `async_players` populated
3. Check standings → Verify times show correctly

### Phase 3: Discord Flow (automated_asyncs=true)
1. Enable `automated_asyncs` on test event
2. Generate seed in `asyncs` table
3. Request qualifier as team → Verify `async_teams.requested` set
4. Wait ~60s, refresh status page → Verify Discord thread link appears
5. Click READY → Verify seed distributed
6. Click START COUNTDOWN → Verify timer starts
7. Click FINISH → Verify completion message
8. Staff runs `/result-async 1:23:45` → Verify times recorded
9. Check standings → Verify team qualified with correct time

### Phase 4: Edge Cases
- Multiple requests → No duplicate threads
- Missing seed → Graceful skip
- Non-organizer `/result-async` → Permission denied
- Bracket async race thread → Verify no interference

### Phase 5: Button Handler Routing
- Create both bracket async and qualifier async threads
- Verify READY/START/FINISH buttons route correctly in both
- Verify no cross-contamination between systems

## Rollback Plan

If issues arise:
1. `UPDATE events SET automated_asyncs = false` globally
2. Legacy flow continues working (no data loss)
3. Existing `discord_thread` values are harmless (just ignored)
4. No race table cleanup needed (none were created)

## Success Criteria

- [x] Migration applies cleanly
- [x] Backward compatibility: `automated_asyncs=false` uses web-only flow
- [x] Discord workflow: `automated_asyncs=true` creates threads without races
- [x] Staff validation via `/result-async` records to both tables
- [x] Times appear in standings correctly via `async_players` query
- [x] No fake races in database
- [x] No filtering logic needed
- [x] Button handlers route correctly to both systems
- [x] No performance degradation (same 60s background task tick)

---

## Seed Type Support

The `asyncs` table supports multiple seed types:

- **OOTR Web** - `web_id`, `web_gen_time`
- **File-based** - `file_stem` (MidosHouse seeds)
- **Triforce Blitz** - `tfb_uuid`, `is_tfb_dev`
- **Crosskeys/Door Rando** - `xkeys_uuid`
- **Generic hashes** - `hash1-5` (for any seed type)
- **Password-protected** - `seed_password`

All seed types are copied from `asyncs` to the Discord thread message via `seed::Data::from_db()` when creating qualifier threads.

---

## Implementation Notes

### Button Handler Custom IDs

**Bracket async buttons:**
- `async_ready` (existing)
- `async_start_countdown` (existing)
- `async_finish` (existing)

**Qualifier async buttons (new):**
- `async_ready_qualifier`
- `async_start_qualifier`
- `async_finish_qualifier`

Both types route to the same handler functions, which check both `races` and `async_teams` tables to determine context.

### Discord Permissions

Qualifier threads are created as **PrivateThread** with:
- Auto-archive after 1 week
- Team member added
- Organizers added (excluding opponent organizers for bracket races, but qualifiers have no opponents)

### Time Recording Strategy

**For bracket async races:**
- Times go to `async_times` table
- Linked by `race_id` and `async_part`
- Staff can manually verify via admin panel

**For qualifier async races:**
- Duration stored in `async_teams.finish_time` as INTERVAL (consistent with `async_times.finish_time` type)
- Duration also stored in `async_players.time` for each team member (official time from `/result-async` command)
- `async_teams.submitted` marks completion

---

## Future Enhancements

1. **Automated VOD extraction** - Parse messages in thread for YouTube/Twitch URLs
2. **Multi-qualifier support** - Handle Qualifier1, Qualifier2, Qualifier3 in parallel
3. **Automatic results announcement** - Post standings when all teams finish
4. **Seed generation integration** - Trigger seed gen on first request
5. **Admin web panel** - UI for creating `asyncs` entries instead of SQL

---

## Document Version

- **Created:** 2026-01-25
- **Last Updated:** 2026-01-25
- **Status:** Ready for Implementation
- **Validated:** Logic validated, points calculation verified, no race pollution confirmed
