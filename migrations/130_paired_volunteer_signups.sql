ALTER TABLE role_bindings
    ADD COLUMN allow_signup_notes boolean NOT NULL DEFAULT true,
    ADD COLUMN allow_paired_signups boolean NOT NULL DEFAULT false;

ALTER TABLE event_role_binding_overrides
    ADD COLUMN allow_signup_notes boolean,
    ADD COLUMN allow_paired_signups boolean,
    DROP CONSTRAINT event_role_binding_overrides_has_at_least_one,
    ADD CONSTRAINT event_role_binding_overrides_has_at_least_one CHECK (
        discord_role_id IS NOT NULL OR min_count IS NOT NULL OR max_count IS NOT NULL
        OR allow_signup_notes IS NOT NULL OR allow_paired_signups IS NOT NULL
    );

CREATE TABLE volunteer_signup_pairs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    race_id bigint NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    role_binding_id integer NOT NULL REFERENCES role_bindings(id) ON DELETE CASCADE,
    inviter_user_id bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invitee_user_id bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    state text NOT NULL DEFAULT 'awaiting_partner'
        CHECK (state IN ('awaiting_partner', 'pending', 'confirmed', 'declined', 'cancelled', 'expired')),
    inviter_notes varchar(60),
    accepted_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (inviter_user_id <> invitee_user_id)
);

CREATE UNIQUE INDEX volunteer_signup_pairs_outgoing ON volunteer_signup_pairs
    (race_id, role_binding_id, inviter_user_id) WHERE state = 'awaiting_partner';
CREATE UNIQUE INDEX volunteer_signup_pairs_members ON volunteer_signup_pairs
    (race_id, role_binding_id, LEAST(inviter_user_id, invitee_user_id), GREATEST(inviter_user_id, invitee_user_id))
    WHERE state IN ('awaiting_partner', 'pending', 'confirmed');
CREATE INDEX volunteer_signup_pairs_incoming ON volunteer_signup_pairs(invitee_user_id)
    WHERE state = 'awaiting_partner';

ALTER TABLE signups ADD COLUMN pair_id bigint REFERENCES volunteer_signup_pairs(id) ON DELETE CASCADE;
CREATE UNIQUE INDEX signup_pair_member ON signups(pair_id, user_id) WHERE pair_id IS NOT NULL;

-- Existing solo duplicates are not silently deleted by this migration. The
-- service serializes new allocations, including solo applications, instead.
CREATE INDEX signup_pair ON signups(pair_id) WHERE pair_id IS NOT NULL;

-- Deferred validation permits a transaction to change the group and both
-- members while forbidding a committed, partially confirmed pair.
CREATE FUNCTION validate_volunteer_signup_pair() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    pair_key bigint;
    pair_row volunteer_signup_pairs%ROWTYPE;
    expected_status volunteer_signup_status;
    member_count integer;
BEGIN
    IF TG_TABLE_NAME = 'signups' THEN
        IF TG_OP = 'UPDATE' AND OLD.pair_id IS NOT NULL AND NEW.pair_id IS DISTINCT FROM OLD.pair_id THEN
            RAISE EXCEPTION 'A paired signup cannot be detached';
        END IF;
        pair_key := CASE WHEN TG_OP = 'DELETE' THEN OLD.pair_id ELSE NEW.pair_id END;
    ELSE
        pair_key := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
    END IF;
    SELECT * INTO pair_row FROM volunteer_signup_pairs WHERE id = pair_key;
    IF NOT FOUND THEN RETURN NULL; END IF;
    SELECT count(*) INTO member_count FROM signups WHERE pair_id = pair_key;
    IF pair_row.accepted_at IS NULL THEN
        IF member_count <> 0 OR pair_row.state IN ('pending', 'confirmed') THEN
            RAISE EXCEPTION 'An unaccepted pair cannot have volunteer signups';
        END IF;
        RETURN NULL;
    END IF;
    expected_status := CASE pair_row.state
        WHEN 'pending' THEN 'pending'::volunteer_signup_status
        WHEN 'confirmed' THEN 'confirmed'::volunteer_signup_status
        WHEN 'declined' THEN 'declined'::volunteer_signup_status
        WHEN 'cancelled' THEN 'aborted'::volunteer_signup_status
        WHEN 'expired' THEN 'aborted'::volunteer_signup_status
        ELSE NULL END;
    IF member_count <> 2 OR expected_status IS NULL OR EXISTS (
        SELECT 1 FROM signups s WHERE s.pair_id = pair_key AND (
            s.race_id <> pair_row.race_id OR s.role_binding_id <> pair_row.role_binding_id
            OR s.user_id NOT IN (pair_row.inviter_user_id, pair_row.invitee_user_id)
            OR s.status <> expected_status
        )
    ) THEN
        RAISE EXCEPTION 'Both consenting volunteers must have the same pair status';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER check_volunteer_pair_members
    AFTER INSERT OR UPDATE OR DELETE ON signups DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION validate_volunteer_signup_pair();
CREATE CONSTRAINT TRIGGER check_volunteer_pair_state
    AFTER INSERT OR UPDATE ON volunteer_signup_pairs DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION validate_volunteer_signup_pair();
