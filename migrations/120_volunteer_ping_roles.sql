-- NULL includes all effective roles in the workflow's language, including future bindings.
-- An array limits the workflow to those binding IDs; an empty array pings no roles.
ALTER TABLE volunteer_ping_workflows
    ADD COLUMN role_binding_ids JSONB,
    ADD CONSTRAINT ping_role_binding_ids_array CHECK (
        role_binding_ids IS NULL OR jsonb_typeof(role_binding_ids) = 'array'
    );
