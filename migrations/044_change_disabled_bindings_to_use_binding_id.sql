-- Change event_disabled_role_bindings to reference specific role_binding_id
-- instead of role_type_id, so we can disable specific language variants

-- Drop the old unique constraint and index
ALTER TABLE event_disabled_role_bindings
DROP CONSTRAINT IF EXISTS event_disabled_role_bindings_series_event_role_type_id_key;

DROP INDEX IF EXISTS idx_event_disabled_role_bindings_lookup;

-- Add role_binding_id column
ALTER TABLE event_disabled_role_bindings
ADD COLUMN role_binding_id BIGINT REFERENCES role_bindings(id) ON DELETE CASCADE;

-- Migrate existing data: for each disabled role_type, find the corresponding game role bindings
-- and create disabled entries for each language variant
-- Note: This assumes the disabled bindings were meant to disable all language variants
-- If there's existing data, we'll need to expand it to cover all languages
DO $$
DECLARE
    rec RECORD;
    binding RECORD;
BEGIN
    FOR rec IN SELECT * FROM event_disabled_role_bindings WHERE role_binding_id IS NULL LOOP
        -- Find all game role bindings for this role_type that apply to this event
        FOR binding IN
            SELECT rb.id
            FROM role_bindings rb
            WHERE rb.role_type_id = rec.role_type_id
              AND rb.game_id IS NOT NULL
              AND rb.series IS NULL
              AND rb.event IS NULL
        LOOP
            -- Update or insert with the specific binding_id
            -- Use the first one we find for the existing row, insert new ones for others
            IF rec.role_binding_id IS NULL THEN
                UPDATE event_disabled_role_bindings
                SET role_binding_id = binding.id
                WHERE id = rec.id;
            ELSE
                -- Insert additional rows for other language variants
                INSERT INTO event_disabled_role_bindings (series, event, role_type_id, role_binding_id, created_at)
                VALUES (rec.series, rec.event, rec.role_type_id, binding.id, rec.created_at)
                ON CONFLICT DO NOTHING;
            END IF;
        END LOOP;
    END LOOP;
END $$;

-- Make role_binding_id NOT NULL
ALTER TABLE event_disabled_role_bindings
ALTER COLUMN role_binding_id SET NOT NULL;

-- Drop role_type_id column (we can still get it via the role_binding)
ALTER TABLE event_disabled_role_bindings
DROP COLUMN role_type_id;

-- Add unique constraint on series, event, role_binding_id
CREATE UNIQUE INDEX event_disabled_role_bindings_unique
ON event_disabled_role_bindings (series, event, role_binding_id);

-- Add index for efficient lookups
CREATE INDEX idx_event_disabled_role_bindings_lookup
ON event_disabled_role_bindings(series, event);

-- Add comment
COMMENT ON COLUMN event_disabled_role_bindings.role_binding_id IS 'Specific role binding to disable (language-specific)';
