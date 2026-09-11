-- Match the 20-character series/event identifiers used by events, teams and
-- races. Migration 110 reintroduced the old 8-character limit, which prevents
-- pooled qualification for series such as alttprmain and alttprenemizer.
ALTER TABLE pooled_qualifier_configs
    ALTER COLUMN series TYPE VARCHAR(20),
    ALTER COLUMN event TYPE VARCHAR(20);
ALTER TABLE qualifier_modes
    ALTER COLUMN series TYPE VARCHAR(20),
    ALTER COLUMN event TYPE VARCHAR(20);
ALTER TABLE qualifier_seeds
    ALTER COLUMN series TYPE VARCHAR(20),
    ALTER COLUMN event TYPE VARCHAR(20);
ALTER TABLE qualifier_attempts
    ALTER COLUMN series TYPE VARCHAR(20),
    ALTER COLUMN event TYPE VARCHAR(20);
ALTER TABLE qualifier_live_entries
    ALTER COLUMN series TYPE VARCHAR(20),
    ALTER COLUMN event TYPE VARCHAR(20);

-- Defaults only: preserve the policies explicitly stored on existing events.
ALTER TABLE events
    ALTER COLUMN preroll_mode SET DEFAULT 'none',
    ALTER COLUMN spoiler_unlock SET DEFAULT 'never';
