-- Add support for event-specific Discord role ID overrides
-- This allows events to use game role bindings but override Discord role IDs

-- Add event_discord_role_overrides table to store event-specific Discord role ID overrides
CREATE TABLE event_discord_role_overrides (
    id SERIAL PRIMARY KEY,
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    role_type_id INTEGER NOT NULL REFERENCES role_types(id) ON DELETE CASCADE,
    discord_role_id BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(series, event, role_type_id)
);

ALTER TABLE public.event_discord_role_overrides OWNER TO mido;

-- Add index for efficient lookups
CREATE INDEX idx_event_discord_role_overrides_series_event ON event_discord_role_overrides(series, event);

-- Add comment explaining the purpose
COMMENT ON TABLE event_discord_role_overrides IS 'Stores event-specific Discord role ID overrides when events use game role bindings'; 