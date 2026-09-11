-- Run after migration 112 inside BEGIN ... ROLLBACK. No HTTP calls.
DO $$
DECLARE
    test_series TEXT;
    race BIGINT := 9000000000000000112;
    export INTEGER := -112;
    old_start TIMESTAMPTZ := NOW() + INTERVAL '1 day';
BEGIN
    INSERT INTO events (series,event,display_name,team_config)
        SELECT series,'hthsgtest','HTH sync test',team_config FROM events LIMIT 1
        RETURNING series INTO STRICT test_series;
    INSERT INTO races (id,series,event,start,p1,p2)
        VALUES (race,test_series,'hthsgtest',old_start,'HTH Test A','HTH Test B');
    INSERT INTO speedgaming_exports (id,series,event,slug,trigger_condition)
        VALUES (export,test_series,'hthsgtest','test','when_scheduled');
    INSERT INTO speedgaming_race_exports (race_id,export_id,state,synced_start,episode_id)
        VALUES (race,export,'succeeded',old_start,112);
    UPDATE races SET start=start+INTERVAL '2 hours' WHERE id=race;
    IF NOT EXISTS (SELECT 1 FROM speedgaming_race_exports WHERE race_id=race
        AND synced_start=old_start AND operation='update' AND state='pending') THEN
        RAISE EXCEPTION 'Time changes must preserve last sent time and queue update';
    END IF;
    UPDATE races SET start=NULL WHERE id=race;
    IF (SELECT operation FROM speedgaming_race_exports WHERE race_id=race)<>'delete' THEN
        RAISE EXCEPTION 'Unscheduling did not queue deletion';
    END IF;
    UPDATE races SET start=old_start WHERE id=race;
    IF (SELECT operation FROM speedgaming_race_exports WHERE race_id=race)<>'update' THEN
        RAISE EXCEPTION 'Rescheduling must supersede a not-yet-sent deletion';
    END IF;
    UPDATE races SET ignored=TRUE WHERE id=race;
    IF (SELECT operation FROM speedgaming_race_exports WHERE race_id=race)<>'delete' THEN
        RAISE EXCEPTION 'Upcoming cancellation did not queue deletion';
    END IF;
    UPDATE races SET ignored=FALSE,start=NOW()-INTERVAL '9 hours' WHERE id=race;
    UPDATE races SET ignored=TRUE,end_time=start+INTERVAL '8 hours' WHERE id=race;
    IF (SELECT operation FROM speedgaming_race_exports WHERE race_id=race)='delete' THEN
        RAISE EXCEPTION 'Historical housekeeping must not remove the remote match';
    END IF;
    INSERT INTO speedgaming_volunteer_exports
        (signup_id,export_id,state,submitted_at,episode_id,remote_id,role)
        VALUES (-112,export,'succeeded',NOW(),112,113,'Commentary');
    DELETE FROM races WHERE id=race;
    IF NOT EXISTS (SELECT 1 FROM speedgaming_race_exports WHERE race_id=race
        AND episode_id=112 AND operation='delete') THEN
        RAISE EXCEPTION 'Hard deletion lost the export/cleanup record';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM speedgaming_volunteer_exports WHERE signup_id=-112) THEN
        RAISE EXCEPTION 'Hard deletion lost volunteer cleanup identity';
    END IF;
    UPDATE speedgaming_race_exports SET state='failed',last_error='HTH test error' WHERE race_id=race;
    IF NOT EXISTS (SELECT 1 FROM speedgaming_race_exports WHERE race_id=race
        AND state='failed' AND episode_id=112) THEN
        RAISE EXCEPTION 'Failed deletion must preserve remote identity';
    END IF;
    UPDATE speedgaming_race_exports SET last_attempt_at=NULL WHERE race_id=race;
    IF (SELECT COALESCE(state='failed' AND last_attempt_at>NOW()-INTERVAL '5 minutes',FALSE)
        FROM speedgaming_race_exports WHERE race_id=race) THEN
        RAISE EXCEPTION 'Manual retry must clear the failed-operation cooldown';
    END IF;
    -- Verified remote deletion can finish without losing the existing record.
    DELETE FROM speedgaming_volunteer_exports WHERE export_id=export AND episode_id=112;
    UPDATE speedgaming_race_exports SET operation='delete',state='succeeded',
        episode_id=NULL,match_id=NULL,synced_start=NULL WHERE race_id=race;
    -- Reuse the same record for a subsequent creation; no generation column.
    UPDATE speedgaming_race_exports SET operation='create',state='in_progress' WHERE race_id=race;
    -- A creation response arriving after local deletion remains available for cleanup.
    UPDATE speedgaming_race_exports SET state='succeeded',episode_id=114,synced_start=old_start WHERE race_id=race;
    IF NOT EXISTS (SELECT 1 FROM speedgaming_race_exports WHERE race_id=race AND episode_id=114) THEN
        RAISE EXCEPTION 'In-flight creation response could not be persisted after local deletion';
    END IF;
    INSERT INTO races (id,series,event,start,p1,p2)
        VALUES (race+1,test_series,'hthsgtest',old_start,'HTH Test C','HTH Test D');
    INSERT INTO speedgaming_race_exports (race_id,export_id,state,synced_start)
        VALUES (race+1,export,'in_progress',old_start);
    UPDATE races SET start=NULL WHERE id=race+1;
    UPDATE speedgaming_race_exports SET state='succeeded',episode_id=115 WHERE race_id=race+1;
    IF NOT EXISTS (SELECT 1 FROM speedgaming_race_exports re JOIN races r ON r.id=re.race_id
        WHERE re.race_id=race+1 AND re.episode_id=115 AND re.synced_start=old_start AND r.start IS NULL) THEN
        RAISE EXCEPTION 'Unscheduling during creation must retain both desired state and the cleanup target';
    END IF;
    RAISE NOTICE 'SpeedGaming lifecycle database assertions passed';
END $$;
