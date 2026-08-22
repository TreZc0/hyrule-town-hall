-- Preserve the Casboots 2026 behavior added on main without teaching the application about
-- a specific event. The generic Avianart seed path now reads its default preset from seed_config.
UPDATE events SET
    racetime_goal_slug = 'Casual Boots',
    is_custom_goal = true,
    seed_gen_type = 'alttpr_avianart',
    seed_config = COALESCE(seed_config, '{}'::jsonb) || '{"preset":"casualboots"}'::jsonb,
    preroll_mode = 'none',
    spoiler_unlock = 'never'
WHERE series = 'casboots' AND event = '2026';

-- Event copy belongs in data, not in a series/event match arm.
INSERT INTO event_descriptions (series, event, content)
SELECT 'casboots', '2026', '<p>Welcome to Casboots 2026! The tournament is organised by {{organizers}}.</p>'
WHERE EXISTS (SELECT 1 FROM events WHERE series = 'casboots' AND event = '2026')
ON CONFLICT (series, event) DO NOTHING;
