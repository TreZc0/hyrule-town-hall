-- Generated mode IDs include the series, event, and mode name.
ALTER TABLE qualifier_modes ALTER COLUMN slug TYPE TEXT;

-- Pooled OWR always used the tournament installation before the build selector
-- became explicit. Preserve that behavior for existing modes and seed material.
UPDATE qualifier_seeds seed
SET settings_fingerprint = 'owr_tourney:' || substr(mode.settings_fingerprint, 5)
FROM qualifier_modes mode
WHERE seed.mode_id = mode.id AND mode.seed_gen_type = 'owr'
    AND mode.settings_fingerprint LIKE 'owr:%'
    AND seed.settings_fingerprint = mode.settings_fingerprint;

UPDATE qualifier_modes
SET seed_gen_type = 'owr_tourney',
    settings_fingerprint = CASE WHEN settings_fingerprint LIKE 'owr:%'
        THEN 'owr_tourney:' || substr(settings_fingerprint, 5)
        ELSE settings_fingerprint END
WHERE seed_gen_type = 'owr';
