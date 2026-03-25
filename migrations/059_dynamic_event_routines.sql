ALTER TABLE events ADD COLUMN goal_slug VARCHAR(40);
ALTER TABLE events ADD COLUMN draft_kind VARCHAR(40);
ALTER TABLE events ADD COLUMN draft_config JSONB;
ALTER TABLE events ADD COLUMN qualifier_score_kind VARCHAR(40);
ALTER TABLE events ADD COLUMN is_single_race BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE events ADD COLUMN hide_entrants BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE events ADD COLUMN start_delay INTEGER NOT NULL DEFAULT 15;
ALTER TABLE events ADD COLUMN start_delay_open INTEGER;
ALTER TABLE events ADD COLUMN restrict_chat_in_qualifiers BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE events ADD COLUMN is_custom_goal BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE events ADD COLUMN preroll_mode VARCHAR(10) NOT NULL DEFAULT 'medium';
ALTER TABLE events ADD COLUMN spoiler_unlock VARCHAR(15) NOT NULL DEFAULT 'after';

-- Backfill goal_slug from current Goal::matches_event logic
UPDATE events SET goal_slug = 'door_rando' WHERE series = 'alttprde' AND event IN ('9bracket', '9swissa', '9swissb');
UPDATE events SET goal_slug = 'avianart' WHERE series = 'alttprde' AND event IN ('rival26br', 'rival26gr');
UPDATE events SET goal_slug = 'crosskeys_2025' WHERE series = 'xkeys' AND event = '2025';
UPDATE events SET goal_slug = 'mystery_d20' WHERE series = 'mysteryd' AND event = '20';
UPDATE events SET goal_slug = 'twwr_permalink' WHERE series = 'twwrmain' AND event IN ('w', 'miniblins26');



-- Generic button drafts: mode + config
-- AlttprDe Season 9 (BanPick): ban/pick sequence with DE9 presets (all three S9 events share the same draft)
UPDATE events SET draft_kind = 'ban_pick', draft_config = '{
    "options": [
        {"display_name": "Ambroz1a", "preset": "ambroz1a"},
        {"display_name": "Crosskeys", "preset": "crosskeys"},
        {"display_name": "Enemizer", "preset": "enemizer"},
        {"display_name": "Inverted", "preset": "inverted"},
        {"display_name": "Open", "preset": "open"}
    ],
    "order": [
        {"phase": "ban", "team": "low_seed"},
        {"phase": "ban", "team": "high_seed"},
        {"phase": "pick", "team": "high_seed"},
        {"phase": "pick", "team": "low_seed"}
    ],
    "label": "mode"
}'::jsonb WHERE series = 'alttprde' AND event IN ('9bracket', '9swissa', '9swissb');

-- RivalsCup Groups (PickOnly): each player picks 1 unique preset
UPDATE events SET draft_kind = 'pick_only', draft_config = '{
    "options": [
        {"display_name": "Open", "preset": "tt_chaos/open"},
        {"display_name": "Standard", "preset": "tt_chaos/standard"},
        {"display_name": "Casual Boots", "preset": "tt_chaos/casualboots"},
        {"display_name": "MC Boss", "preset": "tt_chaos/mcboss"},
        {"display_name": "AD Tournament Keys", "preset": "tt_chaos/adtournamentkeys"}
    ],
    "who_starts": "high_seed",
    "picks_per_player": 1,
    "unique": true,
    "label": "preset"
}'::jsonb WHERE series = 'alttprde' AND event = 'rival26gr';

-- RivalsCup Brackets (BanOnly): 2 bans, remaining randomly assigned
UPDATE events SET draft_kind = 'ban_only', draft_config = '{
    "options": [
        {"display_name": "Open", "preset": "tt_chaos/open"},
        {"display_name": "Standard", "preset": "tt_chaos/standard"},
        {"display_name": "Casual Boots", "preset": "tt_chaos/casualboots"},
        {"display_name": "MC Boss", "preset": "tt_chaos/mcboss"},
        {"display_name": "AD Tournament Keys", "preset": "tt_chaos/adtournamentkeys"}
    ],
    "order": [
        {"phase": "ban", "team": "high_seed"},
        {"phase": "ban", "team": "low_seed"}
    ],
    "label": "preset"
}'::jsonb WHERE series = 'alttprde' AND event = 'rival26br';

-- Backfill qualifier_score_kind
UPDATE events SET qualifier_score_kind = 'twwr_miniblins26' WHERE series = 'twwrmain' AND event = 'miniblins26';
