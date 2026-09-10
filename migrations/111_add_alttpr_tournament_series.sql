-- Make the new tournament series available under ALTTPR.
INSERT INTO game_series (game_id, series)
SELECT games.id, series.slug
FROM games
CROSS JOIN (VALUES ('alttprmain'), ('alttprenemizer')) AS series(slug)
WHERE games.name = 'alttpr'
ON CONFLICT (game_id, series) DO NOTHING;
