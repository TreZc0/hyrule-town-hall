-- Racetime is the sporting identity; a team is only required for async delivery.
CREATE FUNCTION qualifier_team_racetime(team BIGINT) RETURNS TEXT
LANGUAGE SQL STABLE AS $$
    SELECT users.racetime_id FROM team_members JOIN users ON users.id = team_members.member
    WHERE team_members.team = $1 AND users.racetime_id IS NOT NULL
$$;

CREATE FUNCTION qualifier_runner_team(s TEXT, e TEXT, runner TEXT, team BIGINT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
    SELECT EXISTS(SELECT 1 FROM teams WHERE id = team AND series = s AND event = e
        AND qualifier_team_racetime(team) = runner)
$$;

ALTER TABLE qualifier_attempts ADD COLUMN racetime_id TEXT;
UPDATE qualifier_attempts attempt SET racetime_id = COALESCE(
    (SELECT racetime_entrant_id FROM qualifier_live_entries WHERE attempt_id = attempt.id),
    qualifier_team_racetime(attempt.team_id));
-- Fail explicitly on an unresolvable legacy identity; never invent a runner.
ALTER TABLE qualifier_attempts ALTER COLUMN racetime_id SET NOT NULL,
    ALTER COLUMN team_id DROP NOT NULL,
    ADD CONSTRAINT qualifier_async_registration CHECK (source <> 'async' OR team_id IS NOT NULL),
    ADD CONSTRAINT qualifier_racetime_not_empty CHECK (btrim(racetime_id) <> ''),
    ADD UNIQUE (id, racetime_id, mode_id, series, event),
    ADD UNIQUE (id, racetime_id, seed_id, series, event),
    ADD UNIQUE (racetime_id, mode_id, attempt_sequence);

-- Replace the registration-based retry foreign keys and reservation check.
DO $$ DECLARE item RECORD; BEGIN
    FOR item IN SELECT conrelid::regclass AS tbl, conname FROM pg_constraint
        WHERE conrelid IN ('qualifier_attempts'::regclass, 'qualifier_live_entries'::regclass)
          AND ((contype = 'f' AND pg_get_constraintdef(oid) LIKE '%team_id%'
                AND confrelid = 'qualifier_attempts'::regclass)
            OR (contype = 'c' AND pg_get_constraintdef(oid) LIKE '%retry_reserved_at%team_id%'))
    LOOP EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', item.tbl, item.conname); END LOOP;
END $$;
ALTER TABLE qualifier_attempts ADD FOREIGN KEY (retry_of, racetime_id, mode_id, series, event)
    REFERENCES qualifier_attempts(id, racetime_id, mode_id, series, event);
ALTER TABLE qualifier_live_entries
    ADD FOREIGN KEY (retry_original_attempt_id, racetime_entrant_id, mode_id, series, event)
        REFERENCES qualifier_attempts(id, racetime_id, mode_id, series, event),
    ADD FOREIGN KEY (attempt_id, racetime_entrant_id, seed_id, series, event)
        REFERENCES qualifier_attempts(id, racetime_id, seed_id, series, event),
    ADD CHECK (retry_reserved_at IS NULL OR retry_original_attempt_id IS NOT NULL);

DROP INDEX qualifier_attempts_counted_per_mode;
DROP INDEX qualifier_attempts_live_team;
DROP INDEX qualifier_attempts_one_retry;
DROP INDEX qualifier_attempts_one_pending_retry_declaration;
DROP INDEX qualifier_live_entries_one_retry_reservation;
CREATE UNIQUE INDEX qualifier_attempts_counted_per_mode ON qualifier_attempts(racetime_id, mode_id)
    WHERE counts_for_entrant AND state <> 'void';
CREATE UNIQUE INDEX qualifier_attempts_live_runner ON qualifier_attempts(seed_id, racetime_id) WHERE source = 'live';
CREATE UNIQUE INDEX qualifier_attempts_one_retry ON qualifier_attempts(series, event, racetime_id)
    WHERE retry_of IS NOT NULL AND state <> 'void';
CREATE UNIQUE INDEX qualifier_attempts_one_pending_retry_declaration ON qualifier_attempts(series, event, racetime_id)
    WHERE retry_declared_at IS NOT NULL AND counts_for_entrant AND state <> 'void';
CREATE UNIQUE INDEX qualifier_live_entries_one_retry_reservation ON qualifier_live_entries(series, event, racetime_entrant_id)
    WHERE retry_reserved_at IS NOT NULL AND retry_committed_at IS NULL AND retry_released_at IS NULL;

CREATE FUNCTION qualifier_attempt_identity() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.racetime_id := COALESCE(NEW.racetime_id, qualifier_team_racetime(NEW.team_id));
        IF NEW.team_id IS NOT NULL AND NEW.racetime_id IS DISTINCT FROM qualifier_team_racetime(NEW.team_id) THEN
            RAISE EXCEPTION 'qualifier attempt registration does not match racetime identity';
        END IF;
    ELSIF NEW.racetime_id IS DISTINCT FROM OLD.racetime_id THEN
        RAISE EXCEPTION 'qualifier attempt racetime identity is immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER qualifier_attempt_identity BEFORE INSERT OR UPDATE ON qualifier_attempts
    FOR EACH ROW EXECUTE FUNCTION qualifier_attempt_identity();

CREATE FUNCTION qualifier_signup_eligible(s TEXT, e TEXT, runner TEXT) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
    SELECT CASE WHEN submissions_close_at <= statement_timestamp() THEN EXISTS (
        SELECT 1 FROM teams JOIN team_members ON team_members.team=teams.id JOIN users ON users.id=team_members.member
        WHERE teams.series=s AND teams.event=e AND users.racetime_id=runner AND NOT teams.resigned
          AND NOT EXISTS (SELECT 1 FROM team_members pending WHERE pending.team=teams.id AND pending.status='unconfirmed')
    ) ELSE NOT EXISTS (
        SELECT 1 FROM teams JOIN team_members ON team_members.team=teams.id JOIN users ON users.id=team_members.member
        WHERE teams.series=s AND teams.event=e AND users.racetime_id=runner AND teams.resigned
    ) OR EXISTS (
        SELECT 1 FROM teams JOIN team_members ON team_members.team=teams.id JOIN users ON users.id=team_members.member
        WHERE teams.series=s AND teams.event=e AND users.racetime_id=runner AND NOT teams.resigned
    ) END FROM pooled_qualifier_configs WHERE series=s AND event=e
$$;
