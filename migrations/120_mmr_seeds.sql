-- Register MMR; seeds use the existing seed_config and seed_data columns.
INSERT INTO games (name, display_name, description)
VALUES ('mmr', 'Majora''s Mask Randomizer', 'Majora''s Mask Randomizer tournaments and events')
ON CONFLICT (name) DO NOTHING;
