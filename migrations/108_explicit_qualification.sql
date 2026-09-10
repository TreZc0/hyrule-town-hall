-- Display names no longer decide which races feed qualification or qualifier controls.
ALTER TABLE races ADD COLUMN is_qualifier BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE races ADD COLUMN qualifier_number BIGINT;
ALTER TABLE races ADD CONSTRAINT qualifier_number_nonnegative CHECK (qualifier_number >= 0);

UPDATE races SET is_qualifier = TRUE WHERE phase = 'Qualifier';
-- Match the old final-word parsing, including its fallback to attempt 1.
WITH numbered AS (
    SELECT id, substring(trim(round) from '[^[:space:]]+$') AS token
    FROM races WHERE is_qualifier
)
UPDATE races SET qualifier_number = CASE
    WHEN token ~ '^\+?[0-9]+$' AND length(token) <= 19 THEN
        CASE WHEN token::numeric <= 9223372036854775807 THEN token::bigint ELSE 1 END
    ELSE 1 END
FROM numbered WHERE races.id = numbered.id;

-- Freeze the existing choice using the same precedence as the application.
ALTER TABLE events ADD COLUMN qualifier_mode TEXT NOT NULL DEFAULT 'none';
ALTER TABLE events ADD CONSTRAINT qualifier_mode_known CHECK (qualifier_mode IN ('none', 'rank', 'single', 'score'));
UPDATE events e SET qualifier_mode = CASE
    WHEN qualifier_score_kind IN ('songs_of_hope', 'triforce_blitz', 'standard',
        'sgl_2023_online', 'sgl_2024_online', 'sgl_2025_online',
        'twwr_miniblins26', 'twwr_main', 'time_relative') THEN 'score'
    WHEN EXISTS (SELECT 1 FROM teams t WHERE t.series = e.series AND t.event = e.event AND t.qualifier_rank IS NOT NULL) THEN 'rank'
    WHEN EXISTS (SELECT 1 FROM asyncs a WHERE a.series = e.series AND a.event = e.event AND a.kind = 'qualifier') THEN 'single'
    ELSE 'none' END;
