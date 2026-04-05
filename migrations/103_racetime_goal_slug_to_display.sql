-- Phase 4: Migrate racetime_goal_slug from internal identifiers to real racetime.gg goal strings.
-- After this migration, racetime_goal_slug stores the exact string shown on racetime.gg.

UPDATE events SET racetime_goal_slug = 'Beat the game - Tournament (Solo)'
    WHERE racetime_goal_slug = 'door_rando';

UPDATE events SET racetime_goal_slug = 'Beat the game - Tournament (Solo)'
    WHERE racetime_goal_slug = 'avianart';
