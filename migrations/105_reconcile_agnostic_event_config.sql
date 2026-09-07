-- CUTOVER BOUNDARY: this migration belongs to the same fully quiesced cutover
-- as migration 085 and the published migrations 100-104. Do not start the
-- agnostic application until migration 106 has validated these values. A
-- rollback across migration 101 requires restoring the pre-cutover DB backup.

-- Migrations 100 and 103 stored internal placeholders (and, for two ALTTPRDE
-- families, replaced them with the wrong shared goal). Restore the exact
-- racetime.gg goal strings and custom-goal behavior from the old application.
UPDATE events
SET racetime_goal_slug = '9. Deutsches ALTTPR Turnier',
    is_custom_goal = true
WHERE series = 'alttprde'
  AND event IN ('9bracket', '9swissa', '9swissb');

UPDATE events
SET racetime_goal_slug = CASE event
        WHEN 'rival26br' THEN 'ALTTPRDE Rival Cup Brackets'
        WHEN 'rival26gr' THEN 'ALTTPRDE Rival Cup Groups'
    END,
    is_custom_goal = true
WHERE series = 'alttprde'
  AND event IN ('rival26br', 'rival26gr');

UPDATE events
SET racetime_goal_slug = CASE event
        WHEN '2025' THEN 'ALttP Randomizer Crosskeys 2025'
        WHEN '2026' THEN 'Beat the game - Tournament (Solo)'
    END,
    is_custom_goal = (event = '2025')
WHERE series = 'xkeys'
  AND event IN ('2025', '2026');

UPDATE events
SET racetime_goal_slug = 'Deutsches Mystery Turnier 2.0',
    is_custom_goal = true,
    seed_gen_type = 'alttpr_dr',
    seed_config = COALESCE(seed_config, '{}'::jsonb) || '{
        "source": "mystery_pool",
        "mystery_weights_url": "https://assets.zsr.gg/hth/miniturnier_doors.yaml"
    }'::jsonb,
    preroll_mode = 'medium',
    spoiler_unlock = 'never'
WHERE series = 'mysteryd'
  AND event = '20';

UPDATE events
SET racetime_goal_slug = 'Miniblins',
    is_custom_goal = false,
    seed_gen_type = 'twwr',
    seed_config = COALESCE(seed_config, '{}'::jsonb)
        || jsonb_build_object('permalink', settings_string),
    preroll_mode = 'medium',
    spoiler_unlock = 'never'
WHERE series = 'twwrmain'
  AND event IN ('w', 'miniblins26');

-- The Swiss events use a fixed mode per round. Keeping the generic button
-- draft written by migration 100 would change their pre-race flow even though
-- Data::draft_kind currently happens to suppress it when round_modes is set.
UPDATE events
SET draft_kind = NULL,
    draft_config = NULL
WHERE series = 'alttprde'
  AND event IN ('9swissa', '9swissb')
  AND round_modes IS NOT NULL;

-- Restore practice-generation options which used to live in Rust match arms.
UPDATE events
SET seed_config = COALESCE(seed_config, '{}'::jsonb) || '{
    "practice_modes": [
        {"value":"ambroz1a","label":"Ambroz1a"},
        {"value":"crosskeys","label":"Crosskeys"},
        {"value":"enemizer","label":"Enemizer"},
        {"value":"inverted","label":"Inverted"},
        {"value":"open","label":"Open"}
    ],
    "practice_choices": [
        {"value":"pool_hard","label":"Hard Item Pool"},
        {"value":"pool_expert","label":"Expert Item Pool"},
        {"value":"pots","label":"Pottery Shuffle"},
        {"value":"all_dungeons","label":"All Dungeons"},
        {"value":"flute","label":"Flute"},
        {"value":"hovering","label":"Hovering"},
        {"value":"inverted","label":"Inverted"},
        {"value":"keydrop","label":"Keydrop Shuffle"},
        {"value":"mirror_scroll","label":"Mirror Scroll"},
        {"value":"no_delay","label":"No Delay"},
        {"value":"pseudoboots","label":"Pseudoboots"},
        {"value":"boots","label":"Boots"},
        {"value":"zw","label":"ZW"},
        {"value":"boss","label":"Boss Shuffle"},
        {"value":"retro","label":"Retro"},
        {"value":"bones","label":"Bonk Rocks"},
        {"value":"shop","label":"Shop Shuffle"},
        {"value":"keys","label":"Key Shuffle"},
        {"value":"dmg","label":"Damage Shuffle"},
        {"value":"bag","label":"Progressive Bag"},
        {"value":"door","label":"Door Shuffle"}
    ]
}'::jsonb
WHERE series = 'alttprde'
  AND event IN ('9bracket', '9swissa', '9swissb');

UPDATE events
SET seed_config = COALESCE(seed_config, '{}'::jsonb) || '{
    "practice_presets": [
        {"value":"tt_chaos/open","label":"Open"},
        {"value":"tt_chaos/standard","label":"Standard"},
        {"value":"tt_chaos/casualboots","label":"Casual Boots"},
        {"value":"tt_chaos/mcboss","label":"MC Boss"},
        {"value":"tt_chaos/adtournamentkeys","label":"AD Tournament Keys"}
    ]
}'::jsonb
WHERE series = 'alttprde'
  AND event IN ('rival26br', 'rival26gr');

-- TWWR Main Season 9 exactly matches Goal::TwwrMainS9: an existing racetime
-- goal, the event's TWWR permalink, medium preroll, locked spoilers, and the
-- TWWR Main qualifier scoring implementation.
UPDATE events
SET racetime_goal_slug = 'Standard Race',
    is_custom_goal = false,
    qualifier_score_kind = 'twwr_main',
    seed_gen_type = 'twwr',
    seed_config = COALESCE(seed_config, '{}'::jsonb)
        || jsonb_build_object('permalink', settings_string),
    preroll_mode = 'medium',
    spoiler_unlock = 'never'
WHERE series = 'twwrmain'
  AND event = 's9';

-- Cabookey 2026 exactly matches the former OWR_CONFIG, including every base
-- setting and optional patch. Its practice form is derived from these choices.
UPDATE events
SET racetime_goal_slug = 'Cabookey Tournament 2026',
    is_custom_goal = true,
    seed_gen_type = 'owr',
    seed_config = COALESCE(seed_config, '{}'::jsonb) || '{
        "base_settings": {
            "aga_randomness": false,
            "accessibility": "locations",
            "bigkeyshuffle": "wild",
            "boss_shuffle": "none",
            "compassshuffle": "wild",
            "crystals_ganon": "7",
            "crystals_gt": "7",
            "dropshuffle": "none",
            "enemy_shuffle": "none",
            "flute_mode": "normal",
            "goal": "dungeons",
            "item_functionality": "normal",
            "key_logic_algorithm": "partial",
            "keyshuffle": "wild",
            "linked_drops": "unset",
            "mapshuffle": "wild",
            "mirrorscroll": 0,
            "mode": "standard",
            "money_balance": 0,
            "ow_mixed": 0,
            "pottery": "none",
            "pseudoboots": 0,
            "shuffle": "vanilla",
            "shuffletavern": 1,
            "skullwoods": "original",
            "swords": "assured"
        },
        "start_inventory": ["Pegasus Boots"],
        "choices": {
            "keydrop": {
                "label": "Key Drop Shuffle",
                "settings": {"dropshuffle":"keys","pottery":"keys"}
            },
            "100pct": {
                "label": "100% Completionist",
                "settings": {"goal":"completionist"}
            },
            "tileswap": {
                "label": "Tile Swap (OW Mixed)",
                "settings": {"ow_mixed":1}
            },
            "mirror_scroll": {
                "label": "Mirror Scroll",
                "settings": {"mirrorscroll":1}
            },
            "enemizer": {
                "label": "Enemizer + Boss Shuffle",
                "settings": {"enemy_shuffle":"shuffled","boss_shuffle":"random"}
            }
        }
    }'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'cabookey'
  AND event = '2026';

-- Casual Boots 2026 exactly matches Goal::Casboots2026. Migration 104 already
-- writes this on a fresh cutover; repeating the scoped assignment also repairs
-- manually migrated post-104 databases whose row drifted afterward.
UPDATE events
SET racetime_goal_slug = 'Casual Boots',
    is_custom_goal = true,
    seed_gen_type = 'alttpr_avianart',
    seed_config = COALESCE(seed_config, '{}'::jsonb)
        || '{"preset":"casualboots"}'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'casboots'
  AND event = '2026';

-- Both Crosskeys events used the same CrosskeysRaceOptions generator. The goal
-- registration differs, but their seed and practice behavior is identical.
-- Existing JSON is on the left so unrelated top-level extension keys survive;
-- the complete required config is on the right so stale required keys cannot
-- override tournament behavior.
UPDATE events
SET racetime_goal_slug = CASE event
        WHEN '2025' THEN 'ALttP Randomizer Crosskeys 2025'
        WHEN '2026' THEN 'Beat the game - Tournament (Solo)'
    END,
    is_custom_goal = (event = '2025'),
    seed_gen_type = 'alttpr_dr',
    seed_config = COALESCE(seed_config, '{}'::jsonb) || '{
        "source": "mutual_choices",
        "base_settings": {
            "accessibility": "locations",
            "bigkeyshuffle": true,
            "compassshuffle": true,
            "crystals_ganon": "7",
            "crystals_gt": "7",
            "dropshuffle": "none",
            "flute_mode": "normal",
            "goal": "crystals",
            "item_functionality": "normal",
            "key_logic_algorithm": "partial",
            "keyshuffle": "wild",
            "linked_drops": "unset",
            "mapshuffle": true,
            "mirrorscroll": 0,
            "mode": "open",
            "pottery": "none",
            "pseudoboots": 0,
            "shuffle": "crossed",
            "shuffletavern": 0,
            "skullwoods": "original"
        },
        "base_placements": {
            "Skull Woods - Pinball Room": "Small Key (Skull Woods)"
        },
        "start_inventory": [],
        "choices": {
            "all_dungeons": {
                "label": "All Dungeons",
                "settings": {
                    "aga_randomness": false,
                    "goal": "dungeons"
                }
            },
            "completionist": {
                "label": "Completionist",
                "priority": 10,
                "settings": {
                    "goal": "completionist"
                }
            },
            "flute": {
                "label": "Starting Flute",
                "settings": {
                    "flute_mode": "active"
                },
                "start_inventory": ["Ocarina (Activated)"]
            },
            "hovering": {
                "label": "Hovering/Moldorm Bouncing",
                "value_labels": {
                    "always": "hovering and moldorm bouncing ALLOWED",
                    "random": "hovering and moldorm bouncing: random",
                    "never": "hovering and moldorm bouncing BANNED"
                }
            },
            "inverted": {
                "label": "Inverted",
                "settings": {
                    "mode": "inverted"
                }
            },
            "keydrop": {
                "label": "Keydrop",
                "settings": {
                    "dropshuffle": "keys",
                    "pottery": "keys"
                }
            },
            "mirror_scroll": {
                "label": "Mirror Scroll",
                "settings": {
                    "mirrorscroll": 1
                }
            },
            "no_delay": {
                "label": "No Delay",
                "hidden_for_async": true,
                "value_labels": {
                    "always": "no stream delay",
                    "random": "stream delay: random",
                    "never": "stream delay(10m)"
                }
            },
            "pseudoboots": {
                "label": "Pseudoboots",
                "settings": {
                    "pseudoboots": 1
                }
            },
            "zw": {
                "label": "ZW",
                "settings": {
                    "skullwoods": "followlinked"
                },
                "placements": {
                    "Skull Woods - Pinball Room": null
                }
            }
        },
        "practice_choices": [
            {"value":"all_dungeons","label":"All Dungeons"},
            {"value":"completionist","label":"Completionist"},
            {"value":"flute","label":"Starting Flute"},
            {"value":"inverted","label":"Inverted World State"},
            {"value":"keydrop","label":"Enemy/Pot Keydrop"},
            {"value":"mirror_scroll","label":"Starting Mirror Scroll"},
            {"value":"pseudoboots","label":"Starting Pseudoboots"},
            {"value":"zw","label":"ZW"}
        ]
    }'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'xkeys'
  AND event IN ('2025', '2026');

-- These series were added manually after the game layer was introduced. Map
-- them to the already-provisioned ALTTPR racetime connection so room creation
-- and known-goal discovery do not silently skip them.
INSERT INTO game_series (game_id, series)
SELECT games.id, series_name
FROM games
CROSS JOIN (VALUES ('cabookey'), ('casboots')) AS series_rows(series_name)
WHERE games.name = 'alttpr'
ON CONFLICT (game_id, series) DO NOTHING;

-- Preserve the behavior of recently completed non-randomizer events. Their
-- game/category reference data still needs to be provisioned before they can
-- create future rooms; the application now fails closed if it is absent.
UPDATE events
SET racetime_goal_slug = 'Master Sword',
    is_custom_goal = false,
    seed_gen_type = NULL,
    seed_config = NULL,
    preroll_mode = 'none',
    spoiler_unlock = 'never',
    is_live_event = false
WHERE series = 'botwmsr'
  AND event = '2026';

UPDATE events
SET racetime_goal_slug = 'Any%',
    is_custom_goal = false,
    seed_gen_type = NULL,
    seed_config = NULL,
    preroll_mode = 'medium',
    spoiler_unlock = 'never',
    is_live_event = false
WHERE series = 'wolfdash'
  AND event = '5';

-- Keep the event copy data-driven. This is idempotent with migration 104.
INSERT INTO event_descriptions (series, event, content)
SELECT
    'cabookey',
    '2026',
    '<p>Welcome to Cabookey 2026! The tournament is organised by {{organizers}}.</p>'
WHERE EXISTS (
    SELECT 1
    FROM events
    WHERE series = 'cabookey' AND event = '2026'
)
ON CONFLICT (series, event) DO NOTHING;

INSERT INTO event_descriptions (series, event, content)
SELECT
    'casboots',
    '2026',
    '<p>Welcome to Casboots 2026! The tournament is organised by {{organizers}}.</p>'
WHERE EXISTS (
    SELECT 1
    FROM events
    WHERE series = 'casboots' AND event = '2026'
)
ON CONFLICT (series, event) DO NOTHING;
