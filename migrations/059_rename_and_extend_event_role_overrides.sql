ALTER TABLE event_discord_role_overrides
    RENAME TO event_role_binding_overrides;

ALTER TABLE event_role_binding_overrides
    ALTER COLUMN discord_role_id DROP NOT NULL,
    ADD COLUMN min_count INTEGER,
    ADD COLUMN max_count INTEGER,
    ADD CONSTRAINT event_role_binding_overrides_has_at_least_one
        CHECK (discord_role_id IS NOT NULL OR min_count IS NOT NULL OR max_count IS NOT NULL);
