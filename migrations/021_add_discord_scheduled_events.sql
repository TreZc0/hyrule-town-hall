-- Add configuration columns to events table for Discord scheduled events
ALTER TABLE public.events
ADD COLUMN discord_events_enabled boolean DEFAULT false NOT NULL,
ADD COLUMN discord_events_require_restream boolean DEFAULT false NOT NULL;

-- Add Discord scheduled event ID tracking to races table
ALTER TABLE public.races
ADD COLUMN discord_scheduled_event_id bigint;

-- Add index for querying races by Discord event ID
CREATE INDEX idx_races_discord_scheduled_event_id
ON public.races(discord_scheduled_event_id)
WHERE discord_scheduled_event_id IS NOT NULL;
