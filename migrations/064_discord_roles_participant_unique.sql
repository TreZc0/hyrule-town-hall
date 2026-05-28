CREATE UNIQUE INDEX discord_roles_participant_unique
    ON discord_roles (series, event)
    WHERE role IS NULL AND racetime_team IS NULL;
