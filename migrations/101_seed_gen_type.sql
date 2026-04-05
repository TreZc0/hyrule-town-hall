-- Phase 1: Add seed_gen_type + seed_config to events table
ALTER TABLE events ADD COLUMN seed_gen_type VARCHAR(20);
ALTER TABLE events ADD COLUMN seed_config JSONB;

-- Backfill existing events
UPDATE events SET
    seed_gen_type = 'alttpr_dr',
    seed_config = '{"source":"boothisman"}'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'alttprde' AND event IN ('9bracket', '9swissa', '9swissb');

UPDATE events SET
    seed_gen_type = 'alttpr_avianart',
    seed_config = '{}'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'alttprde' AND event IN ('rival26gr', 'rival26br');

UPDATE events SET
    seed_gen_type = 'alttpr_dr',
    seed_config = '{"source":"mutual_choices"}'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'xkeys' AND event = '2025';

UPDATE events SET
    seed_gen_type = 'alttpr_dr',
    seed_config = '{"source":"mystery_pool","mystery_weights_url":"https://zeldaspeedruns.com/assets/hth/miniturnier_doors.yaml"}'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'mysteryd' AND event = '20';

UPDATE events SET
    seed_gen_type = 'twwr',
    seed_config = jsonb_build_object('permalink', settings_string)
WHERE series = 'twwrmain' AND settings_string IS NOT NULL;

-- Phase 4: Unify seed_data JSONB on races table

-- Update existing TWWR seed_data to add "type" field (permalink + seed_hash, no type yet)
UPDATE races
SET seed_data = seed_data || '{"type":"twwr"}'::jsonb
WHERE seed_data IS NOT NULL
  AND seed_data ? 'permalink'
  AND NOT seed_data ? 'type';

-- Update existing Avianart seed_data to add "type" field (avianart_hash, no type yet)
UPDATE races
SET seed_data = jsonb_build_object(
    'type', 'alttpr_avianart',
    'hash', seed_data->>'avianart_hash',
    'seed_hash', seed_data->>'avianart_seed_hash'
)
WHERE seed_data IS NOT NULL
  AND seed_data ? 'avianart_hash'
  AND NOT seed_data ? 'type';

-- Migrate AlttprDoorRando seeds (xkeys_uuid → seed_data)
UPDATE races
SET seed_data = jsonb_strip_nulls(jsonb_build_object(
    'type', 'alttpr_dr',
    'uuid', xkeys_uuid::text,
    'hash1', hash1,
    'hash2', hash2,
    'hash3', hash3,
    'hash4', hash4,
    'hash5', hash5
))
WHERE xkeys_uuid IS NOT NULL AND seed_data IS NULL;

-- Migrate TriforceBlitz seeds (tfb_uuid → seed_data)
UPDATE races
SET seed_data = jsonb_build_object(
    'type', 'ootr_tfb',
    'uuid', tfb_uuid::text,
    'is_dev', is_tfb_dev
)
WHERE tfb_uuid IS NOT NULL AND seed_data IS NULL;

-- Migrate OotrWeb seeds (web_id + file_stem → seed_data)
UPDATE races
SET seed_data = jsonb_strip_nulls(jsonb_build_object(
    'type', 'ootr_web',
    'id', web_id,
    'gen_time', web_gen_time,
    'file_stem', file_stem
))
WHERE web_id IS NOT NULL AND seed_data IS NULL;

-- Migrate MidosHouse seeds (file_stem without web_id → seed_data)
UPDATE races
SET seed_data = jsonb_strip_nulls(jsonb_build_object(
    'type', 'midos_house',
    'file_stem', file_stem,
    'locked_spoiler_log_path', locked_spoiler_log_path
))
WHERE file_stem IS NOT NULL AND web_id IS NULL AND seed_data IS NULL;

-- Move hash1-5 into seed_data for alttpr_dr seeds that got them from xkeys_uuid path above
-- (already done in the xkeys_uuid migration above)

-- Drop old seed columns from races (now all data is in seed_data)
ALTER TABLE races DROP COLUMN xkeys_uuid;
ALTER TABLE races DROP COLUMN tfb_uuid;
ALTER TABLE races DROP COLUMN is_tfb_dev;
ALTER TABLE races DROP COLUMN web_id;
ALTER TABLE races DROP COLUMN web_gen_time;
ALTER TABLE races DROP COLUMN file_stem;
ALTER TABLE races DROP COLUMN locked_spoiler_log_path;
