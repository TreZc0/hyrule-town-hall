-- Change event_discord_role_overrides to reference role_binding_id instead of role_type_id

ALTER TABLE event_discord_role_overrides DROP CONSTRAINT event_discord_role_overrides_series_event_role_type_id_key;
ALTER TABLE event_discord_role_overrides DROP COLUMN role_type_id;
ALTER TABLE event_discord_role_overrides ADD COLUMN role_binding_id INTEGER NOT NULL REFERENCES role_bindings(id) ON DELETE CASCADE;
ALTER TABLE event_discord_role_overrides ADD CONSTRAINT event_discord_role_overrides_series_event_role_binding_id_key UNIQUE(series, event, role_binding_id);
