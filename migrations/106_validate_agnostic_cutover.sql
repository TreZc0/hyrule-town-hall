-- This is the final gate of the fully quiesced agnostic-event cutover. Keep the
-- application stopped until this migration commits. If it fails after migration
-- 101 has run, either repair the reported invariant while still offline or
-- restore the full pre-cutover database backup together with the old binary.

DO $migration$
DECLARE
    is_production_adoption boolean := false;
BEGIN
    -- The adoption utility records every historical row with execution_time=0,
    -- which distinguishes the manually maintained production database from a
    -- normal development replay. On that path, absence of any currently active
    -- event is data loss and must abort the cutover. Development databases can
    -- still replay migrations without having production event rows seeded.
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        EXECUTE $sql$
            SELECT count(*) = 82
            FROM public._sqlx_migrations
            WHERE version BETWEEN 0 AND 81
              AND success
              AND execution_time = 0
        $sql$ INTO is_production_adoption;
    END IF;

    IF is_production_adoption AND (
        SELECT count(*)
        FROM events
        WHERE (series, event) IN (
            ('twwrmain', 's9'),
            ('casboots', '2026'),
            ('xkeys', '2026')
        )
    ) <> 3 THEN
        RAISE EXCEPTION
            'migration 106 requires exactly the three active production event rows';
    END IF;

    -- Every legacy identity archived by migration 085 must still be represented
    -- by the canonical fields consumed by the agnostic application.
    IF EXISTS (
        SELECT 1
        FROM races
        WHERE seed_data->'_legacy_seed_columns_v085' ? 'xkeys_uuid'
          AND (
              COALESCE(seed_data->>'type', '') NOT IN ('alttpr_dr', 'alttpr_owr')
              OR seed_data->>'uuid'
                  IS DISTINCT FROM seed_data->'_legacy_seed_columns_v085'->>'xkeys_uuid'
          )
    ) THEN
        RAISE EXCEPTION
            'migration 106 found an ALTTPR identity that did not survive seed canonicalization';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM races
        WHERE seed_data->'_legacy_seed_columns_v085' ? 'tfb_uuid'
          AND (
              seed_data->>'type' IS DISTINCT FROM 'ootr_tfb'
              OR seed_data->>'uuid'
                  IS DISTINCT FROM seed_data->'_legacy_seed_columns_v085'->>'tfb_uuid'
          )
    ) THEN
        RAISE EXCEPTION
            'migration 106 found a Triforce Blitz identity that did not survive seed canonicalization';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM races
        WHERE seed_data->'_legacy_seed_columns_v085' ? 'web_id'
          AND (
              (
                  (
                      seed_data->'_legacy_seed_columns_v085' ? 'web_gen_time'
                      OR start IS NOT NULL
                      OR async_start1 IS NOT NULL
                      OR async_start2 IS NOT NULL
                      OR async_start3 IS NOT NULL
                  )
                  AND (
                      seed_data->>'type' IS DISTINCT FROM 'ootr_web'
                      OR seed_data->>'id'
                          IS DISTINCT FROM seed_data->'_legacy_seed_columns_v085'->>'web_id'
                      OR NULLIF(seed_data->>'gen_time', '') IS NULL
                      OR seed_data->>'file_stem'
                          IS DISTINCT FROM seed_data->'_legacy_seed_columns_v085'->>'file_stem'
                  )
              )
              OR (
                  NOT (seed_data->'_legacy_seed_columns_v085' ? 'web_gen_time')
                  AND start IS NULL
                  AND async_start1 IS NULL
                  AND async_start2 IS NULL
                  AND async_start3 IS NULL
                  AND (
                      seed_data->>'type' IS DISTINCT FROM 'midos_house'
                      OR seed_data->>'file_stem'
                          IS DISTINCT FROM seed_data->'_legacy_seed_columns_v085'->>'file_stem'
                      OR (
                          seed_data->'_legacy_seed_columns_v085' ? 'locked_spoiler_log_path'
                          AND seed_data->>'locked_spoiler_log_path' IS DISTINCT FROM
                              seed_data->'_legacy_seed_columns_v085'->>'locked_spoiler_log_path'
                      )
                  )
              )
          )
    ) THEN
        RAISE EXCEPTION
            'migration 106 found a legacy web/local identity that did not survive seed canonicalization';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM races
        WHERE seed_data->'_legacy_seed_columns_v085' ? 'file_stem'
          AND NOT (seed_data->'_legacy_seed_columns_v085' ? 'web_id')
          AND (
              seed_data->>'type' IS DISTINCT FROM 'midos_house'
              OR seed_data->>'file_stem'
                  IS DISTINCT FROM seed_data->'_legacy_seed_columns_v085'->>'file_stem'
              OR (
                  seed_data->'_legacy_seed_columns_v085' ? 'locked_spoiler_log_path'
                  AND seed_data->>'locked_spoiler_log_path' IS DISTINCT FROM
                      seed_data->'_legacy_seed_columns_v085'->>'locked_spoiler_log_path'
              )
          )
    ) THEN
        RAISE EXCEPTION
            'migration 106 found a local seed identity that did not survive seed canonicalization';
    END IF;

    -- This is the detectable signature of the published migration 101 bug on
    -- an already-migrated database: auxiliary Crosskeys data survived, but the
    -- UUID column was dropped before being merged into seed_data.
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
            'migration 106 found orphaned Crosskeys resolved_randoms; recover UUIDs from a pre-101 backup';
    END IF;

    -- Verify the historical goal rows corrected after migrations 100 and 103.
    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'alttprde'
          AND event IN ('9bracket', '9swissa', '9swissb')
          AND (
              racetime_goal_slug IS DISTINCT FROM '9. Deutsches ALTTPR Turnier'
              OR is_custom_goal IS DISTINCT FROM true
              OR seed_gen_type IS DISTINCT FROM 'alttpr_dr'
              OR seed_config->>'source' IS DISTINCT FROM 'boothisman'
              OR seed_config->'practice_modes' IS DISTINCT FROM '[
                  {"value":"ambroz1a","label":"Ambroz1a"},
                  {"value":"crosskeys","label":"Crosskeys"},
                  {"value":"enemizer","label":"Enemizer"},
                  {"value":"inverted","label":"Inverted"},
                  {"value":"open","label":"Open"}
              ]'::jsonb
              OR jsonb_array_length(seed_config->'practice_choices') <> 21
              OR (event IN ('9swissa', '9swissb') AND round_modes IS NOT NULL
                  AND (draft_kind IS NOT NULL OR draft_config IS NOT NULL))
              OR (event = '9bracket' AND draft_kind IS DISTINCT FROM 'ban_pick')
              OR preroll_mode IS DISTINCT FROM 'none'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid ALTTPRDE season 9 goal configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'alttprde'
          AND (
              (event = 'rival26br' AND racetime_goal_slug IS DISTINCT FROM 'ALTTPRDE Rival Cup Brackets')
              OR (event = 'rival26gr' AND racetime_goal_slug IS DISTINCT FROM 'ALTTPRDE Rival Cup Groups')
              OR (event IN ('rival26br', 'rival26gr') AND is_custom_goal IS DISTINCT FROM true)
              OR (event IN ('rival26br', 'rival26gr') AND seed_gen_type IS DISTINCT FROM 'alttpr_avianart')
              OR (event IN ('rival26br', 'rival26gr') AND seed_config->'practice_presets' IS DISTINCT FROM '[
                  {"value":"tt_chaos/open","label":"Open"},
                  {"value":"tt_chaos/standard","label":"Standard"},
                  {"value":"tt_chaos/casualboots","label":"Casual Boots"},
                  {"value":"tt_chaos/mcboss","label":"MC Boss"},
                  {"value":"tt_chaos/adtournamentkeys","label":"AD Tournament Keys"}
              ]'::jsonb)
              OR (event IN ('rival26br', 'rival26gr') AND preroll_mode IS DISTINCT FROM 'none')
              OR (event IN ('rival26br', 'rival26gr') AND spoiler_unlock IS DISTINCT FROM 'never')
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid ALTTPRDE Rival Cup goal configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'xkeys'
          AND event = '2025'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'ALttP Randomizer Crosskeys 2025'
              OR is_custom_goal IS DISTINCT FROM true
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Crosskeys 2025 goal configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'mysteryd'
          AND event = '20'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Deutsches Mystery Turnier 2.0'
              OR is_custom_goal IS DISTINCT FROM true
              OR seed_gen_type IS DISTINCT FROM 'alttpr_dr'
              OR seed_config->>'source' IS DISTINCT FROM 'mystery_pool'
              OR seed_config->>'mystery_weights_url'
                  IS DISTINCT FROM 'https://assets.zsr.gg/hth/miniturnier_doors.yaml'
              OR preroll_mode IS DISTINCT FROM 'medium'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Mystery D20 goal configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'twwrmain'
          AND event IN ('w', 'miniblins26')
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Miniblins'
              OR is_custom_goal IS DISTINCT FROM false
              OR seed_gen_type IS DISTINCT FROM 'twwr'
              OR settings_string IS NULL
              OR seed_config->>'permalink' IS DISTINCT FROM settings_string
              OR preroll_mode IS DISTINCT FROM 'medium'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid TWWR weekly/Miniblins configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'cabookey'
          AND event = '2026'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Cabookey Tournament 2026'
              OR is_custom_goal IS DISTINCT FROM true
              OR seed_gen_type IS DISTINCT FROM 'owr'
              OR seed_config->'base_settings'->>'goal' IS DISTINCT FROM 'dungeons'
              OR seed_config->'base_settings'->>'mode' IS DISTINCT FROM 'standard'
              OR seed_config->'base_settings'->>'shuffle' IS DISTINCT FROM 'vanilla'
              OR seed_config->'base_settings'->>'bigkeyshuffle' IS DISTINCT FROM 'wild'
              OR seed_config->'base_settings'->>'mapshuffle' IS DISTINCT FROM 'wild'
              OR seed_config->'start_inventory' IS DISTINCT FROM '["Pegasus Boots"]'::jsonb
              OR NOT COALESCE(
                  (seed_config->'choices') ?& ARRAY[
                      'keydrop', '100pct', 'tileswap', 'mirror_scroll', 'enemizer'
                  ],
                  false
              )
              OR preroll_mode IS DISTINCT FROM 'none'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Cabookey 2026 configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'cabookey'
          AND event = '2026'
          AND (
              (SELECT count(*) FROM game_series WHERE series = 'cabookey') <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN games g ON g.id = gs.game_id
                  JOIN game_racetime_connection grc ON grc.game_id = g.id
                  WHERE gs.series = 'cabookey'
                    AND g.name = 'alttpr'
                    AND grc.category_slug = 'alttpr'
              ) <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN game_racetime_connection grc ON grc.game_id = gs.game_id
                  WHERE gs.series = 'cabookey'
              ) <> 1
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Cabookey game/category routing';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'botwmsr'
          AND event = '2026'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Master Sword'
              OR is_custom_goal IS DISTINCT FROM false
              OR seed_gen_type IS NOT NULL
              OR seed_config IS NOT NULL
              OR preroll_mode IS DISTINCT FROM 'none'
              OR spoiler_unlock IS DISTINCT FROM 'never'
              OR is_live_event IS DISTINCT FROM false
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid BotW MSR 2026 configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'wolfdash'
          AND event = '5'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Any%'
              OR is_custom_goal IS DISTINCT FROM false
              OR seed_gen_type IS NOT NULL
              OR seed_config IS NOT NULL
              OR preroll_mode IS DISTINCT FROM 'medium'
              OR spoiler_unlock IS DISTINCT FROM 'never'
              OR is_live_event IS DISTINCT FROM false
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Wolfdash 5 configuration';
    END IF;

    -- Active-event validation is conditional on row existence so a brand-new
    -- development database (where Casboots/XKeys are configured manually) can
    -- still replay the schema. Production adoption separately requires all
    -- three active rows to exist before cutover.
    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'twwrmain'
          AND event = 's9'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Standard Race'
              OR is_custom_goal IS DISTINCT FROM false
              OR qualifier_score_kind IS DISTINCT FROM 'twwr_main'
              OR seed_gen_type IS DISTINCT FROM 'twwr'
              OR settings_string IS NULL
              OR rando_version IS NULL
              OR seed_config->>'permalink' IS DISTINCT FROM settings_string
              OR preroll_mode IS DISTINCT FROM 'medium'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid TWWR Main S9 configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'twwrmain'
          AND event = 's9'
          AND (
              (SELECT count(*) FROM game_series WHERE series = 'twwrmain') <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN games g ON g.id = gs.game_id
                  JOIN game_racetime_connection grc ON grc.game_id = g.id
                  WHERE gs.series = 'twwrmain'
                    AND g.name = 'twwr'
                    AND grc.category_slug = 'twwr'
              ) <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN game_racetime_connection grc ON grc.game_id = gs.game_id
                  WHERE gs.series = 'twwrmain'
              ) <> 1
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid TWWR Main game/category routing';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'casboots'
          AND event = '2026'
          AND (
              racetime_goal_slug IS DISTINCT FROM 'Casual Boots'
              OR is_custom_goal IS DISTINCT FROM true
              OR seed_gen_type IS DISTINCT FROM 'alttpr_avianart'
              OR seed_config->>'preset' IS DISTINCT FROM 'casualboots'
              OR preroll_mode IS DISTINCT FROM 'none'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Casual Boots 2026 configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'casboots'
          AND event = '2026'
          AND (
              (SELECT count(*) FROM game_series WHERE series = 'casboots') <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN games g ON g.id = gs.game_id
                  JOIN game_racetime_connection grc ON grc.game_id = g.id
                  WHERE gs.series = 'casboots'
                    AND g.name = 'alttpr'
                    AND grc.category_slug = 'alttpr'
              ) <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN game_racetime_connection grc ON grc.game_id = gs.game_id
                  WHERE gs.series = 'casboots'
              ) <> 1
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Casual Boots game/category routing';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'xkeys'
          AND event IN ('2025', '2026')
          AND (
              racetime_goal_slug IS DISTINCT FROM CASE event
                  WHEN '2025' THEN 'ALttP Randomizer Crosskeys 2025'
                  WHEN '2026' THEN 'Beat the game - Tournament (Solo)'
              END
              OR is_custom_goal IS DISTINCT FROM (event = '2025')
              OR seed_gen_type IS DISTINCT FROM 'alttpr_dr'
              OR seed_config->>'source' IS DISTINCT FROM 'mutual_choices'
              OR NOT COALESCE(
                  (seed_config->'choices') ?& ARRAY[
                      'all_dungeons',
                      'completionist',
                      'flute',
                      'hovering',
                      'inverted',
                      'keydrop',
                      'mirror_scroll',
                      'no_delay',
                      'pseudoboots',
                      'zw'
                  ],
                  false
              )
              OR seed_config->'base_settings'->>'shuffle' IS DISTINCT FROM 'crossed'
              OR seed_config->'base_settings'->>'mode' IS DISTINCT FROM 'open'
              OR seed_config->'base_settings'->>'goal' IS DISTINCT FROM 'crystals'
              OR seed_config->'choices'->'completionist'->'priority'
                  IS DISTINCT FROM '10'::jsonb
              OR seed_config->'choices'->'completionist' ? 'supercedes'
              OR seed_config->'choices'->'no_delay'->>'hidden_for_async'
                  IS DISTINCT FROM 'true'
              OR seed_config->'practice_choices' IS DISTINCT FROM '[
                  {"value":"all_dungeons","label":"All Dungeons"},
                  {"value":"completionist","label":"Completionist"},
                  {"value":"flute","label":"Starting Flute"},
                  {"value":"inverted","label":"Inverted World State"},
                  {"value":"keydrop","label":"Enemy/Pot Keydrop"},
                  {"value":"mirror_scroll","label":"Starting Mirror Scroll"},
                  {"value":"pseudoboots","label":"Starting Pseudoboots"},
                  {"value":"zw","label":"ZW"}
              ]'::jsonb
              OR preroll_mode IS DISTINCT FROM 'none'
              OR spoiler_unlock IS DISTINCT FROM 'never'
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Crosskeys configuration';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM events
        WHERE series = 'xkeys'
          AND event = '2026'
          AND (
              (SELECT count(*) FROM game_series WHERE series = 'xkeys') <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN games g ON g.id = gs.game_id
                  JOIN game_racetime_connection grc ON grc.game_id = g.id
                  WHERE gs.series = 'xkeys'
                    AND g.name = 'alttpr'
                    AND grc.category_slug = 'alttpr'
              ) <> 1
              OR (
                  SELECT count(*)
                  FROM game_series gs
                  JOIN game_racetime_connection grc ON grc.game_id = gs.game_id
                  WHERE gs.series = 'xkeys'
              ) <> 1
          )
    ) THEN
        RAISE EXCEPTION 'migration 106 found invalid Crosskeys game/category routing';
    END IF;
END
$migration$;
