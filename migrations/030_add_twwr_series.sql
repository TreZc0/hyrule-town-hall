-- Add settings_string column to events table for base permalinks/settings
ALTER TABLE events ADD COLUMN IF NOT EXISTS settings_string TEXT;

-- Add generic seed_data JSONB column to races table
-- This stores game-specific seed info (permalink, seed_hash, patch_url, hash_icons, etc.)
ALTER TABLE races ADD COLUMN IF NOT EXISTS seed_data JSONB;

-- Add twwr game
INSERT INTO games (name, display_name, description) VALUES
('twwr', 'The Wind Waker Randomizer', 'The Wind Waker Randomizer');

-- Add series mapping
INSERT INTO game_series (game_id, series) VALUES
((SELECT id FROM games WHERE name = 'twwr'), 'twwrmain');

-- Add racetime connection for twwr category
INSERT INTO game_racetime_connection (game_id, category_slug, client_id, client_secret) VALUES
((SELECT id FROM games WHERE name = 'twwr'), 'twwr', '', '');

-- Add events (settings_string to be populated with actual permalinks)
INSERT INTO events (
    series, event, display_name, short_name, listed,
    team_config, language, default_game_count, settings_string, force_custom_role_binding
) VALUES
('twwrmain', 'w', 'TWWR Weeklies', 'TWWR Weekly', true,
 'solo', 'en', 1, 'MS4xMC4wAEEAFwMiAHPowgMMsACCcQ8AAMkHAAAA', false),
('twwrmain', 'miniblins26', 'Miniblins 2026', 'Miniblins 2026', true,
 'solo', 'en', 1, 'MS4xMC4wAEEAFwMiAHPowgMMsACCcQ8AAMkHAAAA', false);
 
 UPDATE events
    SET rando_version = '{"type": "tww", "identifier": "wwrando", "githubUrl": "https://github.com/LagoLunatic/wwrando"}'::jsonb
    WHERE series = 'twwrmain';
