-- Expand series and event columns from VARCHAR(8) to VARCHAR(20)
-- in tables created after migration 025 that were missed

-- startgg_phase_round_mappings (from 037)
ALTER TABLE startgg_phase_round_mappings ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE startgg_phase_round_mappings ALTER COLUMN event TYPE VARCHAR(20);

-- event_blocks (from 042)
ALTER TABLE event_blocks ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE event_blocks ALTER COLUMN event TYPE VARCHAR(20);

-- event_descriptions (from 048)
ALTER TABLE event_descriptions ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE event_descriptions ALTER COLUMN event TYPE VARCHAR(20);
