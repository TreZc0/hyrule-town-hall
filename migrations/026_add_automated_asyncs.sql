-- Migration: Add automated_asyncs support for qualifier system integration
-- This enables the Discord thread-based async workflow for qualifier asyncs

-- Enable automated Discord workflow for qualifiers
ALTER TABLE events
ADD COLUMN automated_asyncs BOOLEAN DEFAULT false NOT NULL;

-- Store thread and timing data directly on async_teams
-- discord_thread: Discord thread ID for automated qualifier workflow
-- start_time: When the player started (after countdown)
-- finish_time: Final verified time as INTERVAL (set by /result-async command)
ALTER TABLE async_teams
ADD COLUMN discord_thread BIGINT,
ADD COLUMN start_time TIMESTAMP WITH TIME ZONE,
ADD COLUMN finish_time INTERVAL;

-- Index for efficient thread lookups when handling button interactions
CREATE INDEX idx_async_teams_discord_thread ON async_teams(discord_thread) WHERE discord_thread IS NOT NULL;
