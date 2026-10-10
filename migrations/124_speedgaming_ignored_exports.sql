ALTER TYPE speedgaming_delivery_state ADD VALUE 'ignored';

-- Ignore is terminal, including when a later race edit queues an update/delete.
-- Keep the record and remote IDs so the export cannot be submitted again.
CREATE FUNCTION speedgaming_preserve_ignored() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.state::text = 'ignored' THEN
        NEW.state = OLD.state;
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER speedgaming_race_preserve_ignored BEFORE UPDATE ON speedgaming_race_exports
    FOR EACH ROW EXECUTE FUNCTION speedgaming_preserve_ignored();
CREATE TRIGGER speedgaming_volunteer_preserve_ignored BEFORE UPDATE ON speedgaming_volunteer_exports
    FOR EACH ROW EXECUTE FUNCTION speedgaming_preserve_ignored();
