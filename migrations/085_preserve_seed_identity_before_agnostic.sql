-- CUTOVER BOUNDARY: apply this migration and migrations 100 through 106 only
-- while every application process and worker is stopped. Migration 101 removes
-- the legacy race seed columns. After that point, rolling the application back
-- also requires restoring the full pre-cutover database backup.
--
-- Version 085 is intentionally lower than the already-published 100-series
-- migrations. SQLx applies missing versions in numeric order on production, so
-- this runs before migration 101 there. On a database that already applied
-- 100-104, SQLx still runs this missing version; the no-legacy-column branch
-- below audits for the known Crosskeys loss mode instead of pretending it can
-- reconstruct an UUID which has already been dropped.

-- These two seed families already lived entirely in seed_data. Add their
-- canonical discriminator/keys without replacing any auxiliary JSON.
UPDATE races
SET seed_data = seed_data || '{"type":"twwr"}'::jsonb
WHERE seed_data IS NOT NULL
  AND seed_data ? 'permalink'
  AND seed_data->>'type' IS NULL;

UPDATE races
SET seed_data = seed_data || jsonb_strip_nulls(jsonb_build_object(
    'type', 'alttpr_avianart',
    'hash', seed_data->>'avianart_hash',
    'seed_hash', seed_data->'avianart_seed_hash'
))
WHERE seed_data IS NOT NULL
  AND seed_data ? 'avianart_hash'
  AND seed_data->>'type' IS NULL;

DO $migration$
DECLARE
    legacy_column_count integer;
    has_conflict boolean;
BEGIN
    SELECT count(*)::integer
    INTO legacy_column_count
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'races'
      AND column_name = ANY (ARRAY[
          'xkeys_uuid',
          'tfb_uuid',
          'is_tfb_dev',
          'web_id',
          'web_gen_time',
          'file_stem',
          'locked_spoiler_log_path'
      ]);

    -- A failed/hand-edited migration 101 is not a state we can classify safely.
    IF legacy_column_count NOT IN (0, 7) THEN
        RAISE EXCEPTION
            'migration 085 expected all seven legacy race seed columns or none; found %',
            legacy_column_count;
    END IF;

    IF legacy_column_count = 7 THEN
        -- This marker belongs exclusively to this migration. Refuse to replace
        -- one that was added manually or by an earlier partial cutover.
        IF EXISTS (
            SELECT 1
            FROM races
            WHERE seed_data ? '_legacy_seed_columns_v085'
        ) THEN
            RAISE EXCEPTION
                'migration 085 found a pre-existing _legacy_seed_columns_v085 archive';
        END IF;

        -- Refuse ambiguous rows rather than silently choosing one identity and
        -- discarding another when migration 101 drops the source columns.
        EXECUTE $sql$
            SELECT EXISTS (
                SELECT 1
                FROM races
                WHERE (
                    (xkeys_uuid IS NOT NULL)::integer
                    + (tfb_uuid IS NOT NULL)::integer
                    + (web_id IS NOT NULL)::integer
                    + (file_stem IS NOT NULL AND web_id IS NULL)::integer
                ) > 1
                   OR (web_id IS NOT NULL AND file_stem IS NULL)
                   OR (locked_spoiler_log_path IS NOT NULL AND file_stem IS NULL)
                   OR (is_tfb_dev AND tfb_uuid IS NULL)
            )
        $sql$ INTO has_conflict;

        IF has_conflict THEN
            RAISE EXCEPTION
                'migration 085 found an ambiguous legacy race seed identity; restore/fix the row before cutover';
        END IF;

        -- If a canonical identity is already present, it must agree with the
        -- legacy source of truth. Missing types (for example seed_data holding
        -- only Crosskeys resolved_randoms) are expected and are repaired below.
        EXECUTE $sql$
            SELECT EXISTS (
                SELECT 1
                FROM races
                WHERE xkeys_uuid IS NOT NULL
                  AND (
                      COALESCE(seed_data->>'type', '') NOT IN ('', 'alttpr_dr', 'alttpr_owr')
                      OR (
                          NULLIF(seed_data->>'uuid', '') IS NOT NULL
                          AND seed_data->>'uuid' <> xkeys_uuid::text
                      )
                  )
            )
        $sql$ INTO has_conflict;

        IF has_conflict THEN
            RAISE EXCEPTION
                'migration 085 found conflicting legacy and JSON ALTTPR seed identities';
        END IF;

        EXECUTE $sql$
            SELECT EXISTS (
                SELECT 1
                FROM races
                WHERE tfb_uuid IS NOT NULL
                  AND (
                      COALESCE(seed_data->>'type', '') NOT IN ('', 'ootr_tfb')
                      OR (
                          NULLIF(seed_data->>'uuid', '') IS NOT NULL
                          AND seed_data->>'uuid' <> tfb_uuid::text
                      )
                  )
            )
        $sql$ INTO has_conflict;

        IF has_conflict THEN
            RAISE EXCEPTION
                'migration 085 found conflicting legacy and JSON Triforce Blitz seed identities';
        END IF;

        EXECUTE $sql$
            SELECT EXISTS (
                SELECT 1
                FROM races
                WHERE web_id IS NOT NULL
                  AND (
                      web_gen_time IS NOT NULL
                      OR start IS NOT NULL
                      OR async_start1 IS NOT NULL
                      OR async_start2 IS NOT NULL
                      OR async_start3 IS NOT NULL
                  )
                  AND (
                      COALESCE(seed_data->>'type', '') NOT IN ('', 'ootr_web')
                      OR (
                          seed_data ? 'id'
                          AND seed_data->>'id' <> web_id::text
                      )
                  )
            )
        $sql$ INTO has_conflict;

        IF has_conflict THEN
            RAISE EXCEPTION
                'migration 085 found conflicting legacy and JSON OoTR Web seed identities';
        END IF;

        EXECUTE $sql$
            SELECT EXISTS (
                SELECT 1
                FROM races
                WHERE file_stem IS NOT NULL
                  AND (
                      web_id IS NULL
                      OR (
                          web_gen_time IS NULL
                          AND start IS NULL
                          AND async_start1 IS NULL
                          AND async_start2 IS NULL
                          AND async_start3 IS NULL
                      )
                  )
                  AND (
                      COALESCE(seed_data->>'type', '') NOT IN ('', 'midos_house')
                      OR (
                          NULLIF(seed_data->>'file_stem', '') IS NOT NULL
                          AND seed_data->>'file_stem' <> file_stem
                      )
                  )
            )
        $sql$ INTO has_conflict;

        IF has_conflict THEN
            RAISE EXCEPTION
                'migration 085 found conflicting legacy and JSON local seed identities';
        END IF;

        -- Keep a lossless copy of every populated legacy field inside the JSON
        -- before migration 101 drops the columns. The agnostic reader ignores
        -- this namespaced object, but it remains available for forensic repair.
        EXECUTE $sql$
            UPDATE races
            SET seed_data = COALESCE(seed_data, '{}'::jsonb)
                || jsonb_build_object(
                    '_legacy_seed_columns_v085',
                    jsonb_strip_nulls(jsonb_build_object(
                        'xkeys_uuid', xkeys_uuid::text,
                        'tfb_uuid', tfb_uuid::text,
                        'is_tfb_dev', CASE WHEN tfb_uuid IS NOT NULL THEN is_tfb_dev END,
                        'web_id', web_id,
                        'web_gen_time', web_gen_time,
                        'file_stem', file_stem,
                        'locked_spoiler_log_path', locked_spoiler_log_path
                    ))
                )
            WHERE xkeys_uuid IS NOT NULL
               OR tfb_uuid IS NOT NULL
               OR web_id IS NOT NULL
               OR file_stem IS NOT NULL
               OR locked_spoiler_log_path IS NOT NULL
        $sql$;

        -- Crosskeys is the critical case: resolved_randoms already made
        -- seed_data non-NULL, so the published migration 101 skipped the UUID.
        -- Merge canonical identity/hash fields on the right while retaining all
        -- unrelated keys on the left.
        EXECUTE $sql$
            UPDATE races
            SET seed_data = seed_data || jsonb_strip_nulls(jsonb_build_object(
                'type', COALESCE(NULLIF(seed_data->>'type', ''), 'alttpr_dr'),
                'uuid', xkeys_uuid::text,
                'hash1', hash1,
                'hash2', hash2,
                'hash3', hash3,
                'hash4', hash4,
                'hash5', hash5
            ))
            WHERE xkeys_uuid IS NOT NULL
        $sql$;

        EXECUTE $sql$
            UPDATE races
            SET seed_data = seed_data || jsonb_build_object(
                'type', 'ootr_tfb',
                'uuid', tfb_uuid::text,
                'is_dev', is_tfb_dev
            )
            WHERE tfb_uuid IS NOT NULL
        $sql$;

        -- Match the legacy reader exactly: a row with web_id + file_stem but
        -- no generation/start timestamp was treated as a local MidosHouse
        -- seed. Its now-unused web_id remains in the forensic archive above.
        EXECUTE $sql$
            UPDATE races
            SET seed_data = seed_data || jsonb_build_object(
                'type', 'ootr_web',
                'id', web_id,
                'gen_time', COALESCE(
                    web_gen_time,
                    (
                        SELECT min(candidate) - INTERVAL '1 day'
                        FROM unnest(ARRAY[
                            start,
                            async_start1,
                            async_start2,
                            async_start3
                        ]) AS starts(candidate)
                    )
                ),
                'file_stem', file_stem
            )
            WHERE web_id IS NOT NULL
              AND (
                  web_gen_time IS NOT NULL
                  OR start IS NOT NULL
                  OR async_start1 IS NOT NULL
                  OR async_start2 IS NOT NULL
                  OR async_start3 IS NOT NULL
              )
        $sql$;

        EXECUTE $sql$
            UPDATE races
            SET seed_data = seed_data || jsonb_strip_nulls(jsonb_build_object(
                'type', 'midos_house',
                'file_stem', file_stem,
                'locked_spoiler_log_path', locked_spoiler_log_path
            ))
            WHERE file_stem IS NOT NULL
              AND (
                  web_id IS NULL
                  OR (
                      web_gen_time IS NULL
                      AND start IS NULL
                      AND async_start1 IS NULL
                      AND async_start2 IS NULL
                      AND async_start3 IS NULL
                  )
              )
        $sql$;
    ELSE
        -- A post-104 staging database has already lost the legacy columns. We
        -- can accept it only when the known skipped-XKeys signature is absent.
        -- If this fires, recover the UUIDs from a pre-101 dump before proceeding.
        IF EXISTS (
            SELECT 1
            FROM races
            WHERE series = 'xkeys'
              AND seed_data ? 'resolved_randoms'
              AND (
                  COALESCE(seed_data->>'type', '') NOT IN ('alttpr_dr', 'alttpr_owr')
                  OR COALESCE(seed_data->>'uuid', '') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
              )
        ) THEN
            RAISE EXCEPTION
                'migration 085 detected Crosskeys resolved_randoms with no recoverable seed UUID; restore from a pre-101 backup';
        END IF;
    END IF;
END
$migration$;
