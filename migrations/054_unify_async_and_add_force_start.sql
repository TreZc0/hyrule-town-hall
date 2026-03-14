-- Add player_finished_at to async_times (same concept as finished_at in async_teams)
ALTER TABLE async_times ADD COLUMN player_finished_at TIMESTAMPTZ;

-- Rename finished_at to player_finished_at in async_teams for consistency
ALTER TABLE async_teams RENAME COLUMN finished_at TO player_finished_at;

-- Add configurable force-start delay (minutes) to events
ALTER TABLE events ADD COLUMN async_start_delay INT;
