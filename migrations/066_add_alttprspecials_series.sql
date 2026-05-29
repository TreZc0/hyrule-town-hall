-- Add ALTTPR Specials as a baseline ALTTPR series.

INSERT INTO game_series (game_id, series)
VALUES ((SELECT id FROM games WHERE name = 'alttpr'), 'alttprspecials')
ON CONFLICT (game_id, series) DO NOTHING;
