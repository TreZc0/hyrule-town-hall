-- A Discord participant role can be shared by multiple events.
ALTER TABLE discord_roles DROP CONSTRAINT discord_roles_pkey;
CREATE INDEX discord_roles_id_idx ON discord_roles (id);

-- discord_roles_participant_unique still enforces one participant role per event.
-- Other role mappings retain their existing role-ID uniqueness.
CREATE UNIQUE INDEX discord_roles_non_participant_unique ON discord_roles (id)
    WHERE role IS NOT NULL OR racetime_team IS NOT NULL;
