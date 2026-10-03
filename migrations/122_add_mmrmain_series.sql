-- Register the main MMR series; event configuration remains managed through the UI.
-- Preserve any mapping an administrator already assigned through Manage Series.
INSERT INTO game_series (game_id, series)
SELECT id, 'mmrmain'
FROM games
WHERE name = 'mmr'
  AND NOT EXISTS (SELECT 1 FROM game_series WHERE series = 'mmrmain')
ON CONFLICT (game_id, series) DO NOTHING;
