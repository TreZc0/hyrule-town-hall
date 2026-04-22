-- Expand series and event columns from VARCHAR(8) to VARCHAR(20)
-- in event_round_configs (from 061, missed by 055)

ALTER TABLE event_round_configs ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE event_round_configs ALTER COLUMN event TYPE VARCHAR(20);
