-- Add round_modes column to events table
-- This stores a JSON object mapping round names to mode names for swiss events
-- Example: {"Round 1": "ambroz1a", "Round 2": "crosskeys", "Round 3": "enemizer"}
-- When a race's round matches a key, that mode is used instead of drafting

ALTER TABLE public.events
ADD COLUMN round_modes jsonb DEFAULT NULL;
