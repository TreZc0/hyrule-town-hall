ALTER TABLE races ADD COLUMN IF NOT EXISTS speedgaming_onsite_id BIGINT;
ALTER TABLE races ADD COLUMN IF NOT EXISTS video_url_es TEXT;

ALTER TABLE events ADD COLUMN IF NOT EXISTS speedgaming_in_person_id BIGINT;
ALTER TABLE events ADD COLUMN IF NOT EXISTS emulator_settings_reminder BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE events ADD COLUMN IF NOT EXISTS prevent_late_joins BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS speedgaming_onsite_disambiguation_messages (
    speedgaming_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS racetime_maintenance (
    start TIMESTAMP WITH TIME ZONE NOT NULL,
    end_time TIMESTAMP WITH TIME ZONE NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON speedgaming_onsite_disambiguation_messages TO mido;
GRANT SELECT, INSERT, UPDATE, DELETE ON racetime_maintenance TO mido;