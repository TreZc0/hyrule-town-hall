-- Add table to track disabled game role bindings per event
CREATE TABLE event_disabled_role_bindings (
    id SERIAL PRIMARY KEY,
    series TEXT NOT NULL,
    event TEXT NOT NULL,
    role_type_id INTEGER NOT NULL REFERENCES role_types(id),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(series, event, role_type_id)
);

-- Add index for efficient lookups
CREATE INDEX idx_event_disabled_role_bindings_lookup ON event_disabled_role_bindings(series, event, role_type_id); 

ALTER TABLE public.event_disabled_role_bindings OWNER TO mido;