-- Migration script to move crosskeys/2025 tournament data from old boolean columns to custom_choices JSONB
-- This script is idempotent and can be run multiple times safely
-- Follows the pattern in teams.rs where only 'yes' values are stored in custom_choices

UPDATE teams
SET custom_choices = custom_choices
    || CASE WHEN all_dungeons_ok THEN jsonb_build_object('all_dungeons', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN flute_ok THEN jsonb_build_object('flute', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN hover_ok THEN jsonb_build_object('hovering', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN inverted_ok THEN jsonb_build_object('inverted', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN keydrop_ok THEN jsonb_build_object('keydrop', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN mirror_scroll_ok THEN jsonb_build_object('mirror_scroll', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN no_delay_ok THEN jsonb_build_object('no_delay', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN pb_ok THEN jsonb_build_object('pseudoboots', 'yes') ELSE '{}'::jsonb END
    || CASE WHEN zw_ok THEN jsonb_build_object('zw', 'yes') ELSE '{}'::jsonb END
WHERE series = 'xkeys'
  AND event = '2025';

-- Reset the old boolean columns to FALSE for crosskeys/2025 teams
-- (Keeping the columns in the schema for backwards compatibility with other events)
UPDATE teams
SET
    all_dungeons_ok = FALSE,
    flute_ok = FALSE,
    hover_ok = FALSE,
    inverted_ok = FALSE,
    keydrop_ok = FALSE,
    mirror_scroll_ok = FALSE,
    no_delay_ok = FALSE,
    pb_ok = FALSE,
    zw_ok = FALSE
WHERE series = 'xkeys'
  AND event = '2025';

-- Update the enter_flow to use booleanChoice instead of dedicated requirement types
UPDATE events
SET enter_flow = '{
  "requirements": [
    {"type": "raceTime"},
    {"type": "twitch"},
    {"type": "discord"},
    {"name": "Crosskeys 202 BC", "type": "discordGuild"},
    {"type": "startGG", "optional": true},
    {"type": "rules", "document": "https://www.youtube.com/watch?v=dQw4w9WgXcQ"},
    {"type": "booleanChoice", "key": "all_dungeons", "label": "All Dungeons OK?"},
    {"type": "booleanChoice", "key": "hovering", "label": "Hovering/Moldorm Bouncing OK?"},
    {"type": "booleanChoice", "key": "inverted", "label": "Inverted OK?"},
    {"type": "booleanChoice", "key": "flute", "label": "Flute OK?"},
    {"type": "booleanChoice", "key": "keydrop", "label": "Keydrop OK?"},
    {"type": "booleanChoice", "key": "mirror_scroll", "label": "Mirror Scroll OK?"},
    {"type": "booleanChoice", "key": "no_delay", "label": "No Delay OK?"},
    {"type": "booleanChoice", "key": "pseudoboots", "label": "Pseudoboots OK?"},
    {"type": "booleanChoice", "key": "zw", "label": "Zelda''s Wishes OK?"},
    {"type": "restreamConsent", "optional": true}
  ]
}'::jsonb
WHERE series = 'xkeys' AND event = '2025';
