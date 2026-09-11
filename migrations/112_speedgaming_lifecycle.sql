-- Extend the existing delivery records; preserve them for remote cleanup.
ALTER TABLE speedgaming_race_exports DROP CONSTRAINT speedgaming_race_exports_race_id_fkey;
ALTER TABLE speedgaming_volunteer_exports DROP CONSTRAINT speedgaming_volunteer_exports_signup_id_fkey;

ALTER TABLE speedgaming_race_exports
    ADD COLUMN match_id BIGINT,
    ADD COLUMN synced_start TIMESTAMPTZ,
    ADD COLUMN operation TEXT NOT NULL DEFAULT 'create'
        CHECK (operation IN ('create', 'update', 'delete'));

-- A failed update/delete must retain its already-created episode ID.
ALTER TABLE speedgaming_race_exports DROP CONSTRAINT speedgaming_race_exports_check;
ALTER TABLE speedgaming_race_exports ADD CONSTRAINT speedgaming_race_exports_check
    CHECK (state <> 'succeeded' OR episode_id IS NOT NULL OR operation = 'delete');

ALTER TABLE speedgaming_volunteer_exports
    ADD COLUMN episode_id BIGINT,
    ADD COLUMN remote_id BIGINT,
    ADD COLUMN role TEXT;

UPDATE speedgaming_race_exports re
SET synced_start = date_trunc('minute', r.start + make_interval(mins => e.delay_minutes))
FROM races r, speedgaming_exports e WHERE r.id = re.race_id AND e.id = re.export_id;

UPDATE speedgaming_volunteer_exports v SET episode_id=re.episode_id,role=rt.name
FROM signups s, role_bindings rb, role_types rt, speedgaming_race_exports re
WHERE v.signup_id=s.id AND s.role_binding_id=rb.id AND rb.role_type_id=rt.id
    AND re.race_id=s.race_id AND re.export_id=v.export_id;

-- Notifications are delivered on commit. Periodic reconciliation also reads
-- these same records, so missed notifications cannot lose deletion work.
CREATE FUNCTION speedgaming_notify_change() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'races' AND TG_OP = 'UPDATE' THEN
        IF (OLD.start,OLD.ignored,OLD.phase,OLD.round,OLD.video_url,OLD.video_url_fr,OLD.video_url_de,OLD.video_url_pt)
            IS NOT DISTINCT FROM
            (NEW.start,NEW.ignored,NEW.phase,NEW.round,NEW.video_url,NEW.video_url_fr,NEW.video_url_de,NEW.video_url_pt) THEN
            RETURN NULL;
        END IF;
        IF NEW.start IS NULL OR (NEW.ignored AND NOT OLD.ignored AND OLD.start>NOW()) THEN
            UPDATE speedgaming_race_exports SET operation='delete',
                state=CASE WHEN state='in_progress' THEN state ELSE 'pending' END,
                last_error=NULL,last_attempt_at=NULL
            WHERE race_id=NEW.id AND episode_id IS NOT NULL;
        ELSIF NOT NEW.ignored AND (OLD.start IS DISTINCT FROM NEW.start OR OLD.ignored) THEN
            UPDATE speedgaming_race_exports SET operation='update',
                state=CASE WHEN state='in_progress' THEN state ELSE 'pending' END,
                last_error=NULL,last_attempt_at=NULL
            WHERE race_id=NEW.id AND episode_id IS NOT NULL;
        END IF;
    ELSIF TG_TABLE_NAME = 'races' AND TG_OP = 'DELETE' THEN
        UPDATE speedgaming_race_exports SET operation='delete',
            state=CASE WHEN state='in_progress' THEN state ELSE 'pending' END,
            last_error=NULL,last_attempt_at=NULL
        WHERE race_id=OLD.id AND episode_id IS NOT NULL;
    END IF;
    PERFORM pg_notify('speedgaming_sync', '');
    RETURN NULL;
END $$;
CREATE TRIGGER speedgaming_race_change AFTER INSERT OR UPDATE OR DELETE ON races
    FOR EACH ROW EXECUTE FUNCTION speedgaming_notify_change();
CREATE TRIGGER speedgaming_signup_change AFTER INSERT OR UPDATE OR DELETE ON signups
    FOR EACH ROW EXECUTE FUNCTION speedgaming_notify_change();
CREATE TRIGGER speedgaming_config_change AFTER INSERT OR UPDATE OR DELETE ON speedgaming_exports
    FOR EACH ROW EXECUTE FUNCTION speedgaming_notify_change();
CREATE TRIGGER speedgaming_team_change AFTER UPDATE OF restream_consent ON teams
    FOR EACH ROW EXECUTE FUNCTION speedgaming_notify_change();
CREATE TRIGGER speedgaming_round_change AFTER INSERT OR UPDATE OR DELETE ON event_round_configs
    FOR EACH ROW EXECUTE FUNCTION speedgaming_notify_change();
