-- Add language column to role_bindings table
-- This enables per-language role bindings (e.g., "English Tracker", "French Tracker")

-- Add language column with default 'en'
ALTER TABLE role_bindings
ADD COLUMN language language NOT NULL DEFAULT 'en';

-- Drop old unique constraint that didn't include language
ALTER TABLE role_bindings
DROP CONSTRAINT IF EXISTS role_bindings_series_event_role_type_id_key;

-- Use partial unique indexes instead of constraints because NULL values in PostgreSQL
-- are treated as distinct in unique constraints, which would allow duplicates.

-- For event-level bindings (where game_id IS NULL)
CREATE UNIQUE INDEX role_bindings_event_role_type_language_unique
ON role_bindings (series, event, role_type_id, language)
WHERE game_id IS NULL;

-- For game-level bindings (where series IS NULL AND event IS NULL)
CREATE UNIQUE INDEX role_bindings_game_role_type_language_unique
ON role_bindings (game_id, role_type_id, language)
WHERE series IS NULL AND event IS NULL;

-- Add index for efficient language filtering
CREATE INDEX idx_role_bindings_language ON role_bindings(language);

-- Set language='de' for German series events (mysteryd and alttprde)
UPDATE role_bindings
SET language = 'de'
WHERE series IN ('mysteryd', 'alttprde');

-- Add default volunteer language setting to events table
-- This determines which language tab is shown by default
ALTER TABLE events
ADD COLUMN default_volunteer_language language NOT NULL DEFAULT 'en';

-- Set default language for German series events
UPDATE events
SET default_volunteer_language = 'de'
WHERE series IN ('mysteryd', 'alttprde');

-- Add comment explaining the column
COMMENT ON COLUMN role_bindings.language IS 'Language for this role binding (e.g., English Tracker vs French Tracker)';
COMMENT ON COLUMN events.default_volunteer_language IS 'Default language tab shown on volunteer pages';
