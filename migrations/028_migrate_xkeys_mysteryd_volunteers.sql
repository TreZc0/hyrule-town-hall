-- ============================================================================
-- Migrate existing volunteers to alttpr game-level bindings
-- xkeys volunteers → alttpr English game volunteers
-- mysteryd volunteers → alttpr German game volunteers
-- ============================================================================
-- This migration should be run during a maintenance window to avoid race
-- conditions with concurrent role requests.

-- First, create game role bindings for alttpr for each role type that exists in xkeys or mysteryd
-- These will be the target bindings for migrated volunteers

-- Create English game bindings for alttpr from xkeys role types
-- Using DO NOTHING with partial index - if binding already exists, skip it
INSERT INTO role_bindings (game_id, series, event, role_type_id, min_count, max_count, language)
SELECT DISTINCT ON (rb.role_type_id)
    2 AS game_id,  -- alttpr game_id
    NULL AS series,
    NULL AS event,
    rb.role_type_id,
    COALESCE(rb.min_count, 1) AS min_count,
    COALESCE(rb.max_count, 2) AS max_count,
    'en' AS language
FROM role_bindings rb
WHERE rb.series = 'xkeys'
  AND rb.game_id IS NULL  -- Only event-level bindings
ORDER BY rb.role_type_id, rb.created_at DESC
ON CONFLICT DO NOTHING;

-- Create German game bindings for alttpr from mysteryd role types
INSERT INTO role_bindings (game_id, series, event, role_type_id, min_count, max_count, language)
SELECT DISTINCT ON (rb.role_type_id)
    2 AS game_id,  -- alttpr game_id
    NULL AS series,
    NULL AS event,
    rb.role_type_id,
    COALESCE(rb.min_count, 1) AS min_count,
    COALESCE(rb.max_count, 2) AS max_count,
    'de' AS language
FROM role_bindings rb
WHERE rb.series = 'mysteryd'
  AND rb.game_id IS NULL  -- Only event-level bindings
ORDER BY rb.role_type_id, rb.created_at DESC
ON CONFLICT DO NOTHING;

-- Now migrate role_requests from xkeys event bindings to alttpr English game bindings
-- We need to update role_binding_id to point to the new game binding with matching role_type_id
UPDATE role_requests rr
SET role_binding_id = game_binding.id
FROM role_bindings event_binding
JOIN role_bindings game_binding ON game_binding.role_type_id = event_binding.role_type_id
WHERE rr.role_binding_id = event_binding.id
  AND event_binding.series = 'xkeys'
  AND event_binding.game_id IS NULL  -- Source is event binding
  AND game_binding.game_id = 2       -- Target is alttpr game binding
  AND game_binding.language = 'en';  -- English

-- Migrate role_requests from mysteryd event bindings to alttpr German game bindings
UPDATE role_requests rr
SET role_binding_id = game_binding.id
FROM role_bindings event_binding
JOIN role_bindings game_binding ON game_binding.role_type_id = event_binding.role_type_id
WHERE rr.role_binding_id = event_binding.id
  AND event_binding.series = 'mysteryd'
  AND event_binding.game_id IS NULL  -- Source is event binding
  AND game_binding.game_id = 2       -- Target is alttpr game binding
  AND game_binding.language = 'de';  -- German

-- Add xkeys and mysteryd to alttpr game_series if not already there
INSERT INTO game_series (game_id, series)
VALUES (2, 'xkeys')
ON CONFLICT (game_id, series) DO NOTHING;

INSERT INTO game_series (game_id, series)
VALUES (2, 'mysteryd')
ON CONFLICT (game_id, series) DO NOTHING;

-- Set force_custom_role_binding to FALSE for xkeys and mysteryd events
-- so they use the game-level bindings
UPDATE events
SET force_custom_role_binding = FALSE
WHERE series IN ('xkeys', 'mysteryd');
