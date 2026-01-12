-- Expand series and event columns from VARCHAR(8) to VARCHAR(20)
-- Also standardize role_bindings from VARCHAR(50) to VARCHAR(20)

-- async_players
ALTER TABLE async_players ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE async_players ALTER COLUMN event TYPE VARCHAR(20);

-- asyncs
ALTER TABLE asyncs ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE asyncs ALTER COLUMN event TYPE VARCHAR(20);

-- discord_roles
ALTER TABLE discord_roles ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE discord_roles ALTER COLUMN event TYPE VARCHAR(20);

-- events (primary table)
ALTER TABLE events ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE events ALTER COLUMN event TYPE VARCHAR(20);

-- looking_for_team
ALTER TABLE looking_for_team ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE looking_for_team ALTER COLUMN event TYPE VARCHAR(20);

-- notifications
ALTER TABLE notifications ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE notifications ALTER COLUMN event TYPE VARCHAR(20);

-- notify_on_delete
ALTER TABLE notify_on_delete ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE notify_on_delete ALTER COLUMN event TYPE VARCHAR(20);

-- opt_outs
ALTER TABLE opt_outs ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE opt_outs ALTER COLUMN event TYPE VARCHAR(20);

-- organizers
ALTER TABLE organizers ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE organizers ALTER COLUMN event TYPE VARCHAR(20);

-- phase_round_options
ALTER TABLE phase_round_options ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE phase_round_options ALTER COLUMN event TYPE VARCHAR(20);

-- races
ALTER TABLE races ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE races ALTER COLUMN event TYPE VARCHAR(20);

-- restreamers
ALTER TABLE restreamers ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE restreamers ALTER COLUMN event TYPE VARCHAR(20);

-- teams
ALTER TABLE teams ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE teams ALTER COLUMN event TYPE VARCHAR(20);

-- event_discord_role_overrides
ALTER TABLE event_discord_role_overrides ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE event_discord_role_overrides ALTER COLUMN event TYPE VARCHAR(20);

-- role_bindings (reduce from VARCHAR(50) to VARCHAR(20) for consistency)
ALTER TABLE role_bindings ALTER COLUMN series TYPE VARCHAR(20);
ALTER TABLE role_bindings ALTER COLUMN event TYPE VARCHAR(20);
