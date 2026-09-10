-- Pooled qualification is opt-in. Existing qualification methods and async tables
-- remain unchanged and continue to serve all existing events.
ALTER TABLE events DROP CONSTRAINT qualifier_mode_known;
ALTER TABLE events ADD CONSTRAINT qualifier_mode_known
    CHECK (qualifier_mode IN ('none', 'rank', 'single', 'score', 'pooled_by_mode'));

-- Composite keys let child tables prove that teams, races, modes, and seeds all
-- belong to the same event without relying on application checks alone.
ALTER TABLE teams ADD CONSTRAINT teams_id_series_event_key UNIQUE (id, series, event);
ALTER TABLE races ADD CONSTRAINT races_id_series_event_key UNIQUE (id, series, event);

CREATE TABLE pooled_qualifier_configs (
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    required_mode_count SMALLINT NOT NULL DEFAULT 3 CHECK (required_mode_count > 0),
    pool_seed_count SMALLINT NOT NULL DEFAULT 3 CHECK (pool_seed_count > 0),
    live_races_per_mode SMALLINT NOT NULL DEFAULT 3 CHECK (live_races_per_mode >= 0),
    requests_open_at TIMESTAMPTZ,
    requests_close_at TIMESTAMPTZ,
    starts_close_at TIMESTAMPTZ,
    submissions_close_at TIMESTAMPTZ,
    retries_close_at TIMESTAMPTZ,
    results_release_at TIMESTAMPTZ,
    async_run_limit INTERVAL NOT NULL DEFAULT INTERVAL '12 hours'
        CHECK (async_run_limit > INTERVAL '0'),
    live_entry_close_lead INTERVAL NOT NULL DEFAULT INTERVAL '10 minutes'
        CHECK (live_entry_close_lead >= INTERVAL '0'),
    retry_limit SMALLINT NOT NULL DEFAULT 1 CHECK (retry_limit BETWEEN 0 AND 1),
    allocation_spread SMALLINT NOT NULL DEFAULT 2 CHECK (allocation_spread >= 1),
    par_finishers SMALLINT NOT NULL DEFAULT 5 CHECK (par_finishers > 0),
    score_scale DOUBLE PRECISION NOT NULL DEFAULT 100 CHECK (score_scale > 0),
    score_offset DOUBLE PRECISION NOT NULL DEFAULT 2,
    score_minimum DOUBLE PRECISION NOT NULL DEFAULT 0,
    score_maximum DOUBLE PRECISION NOT NULL DEFAULT 105,
    requests_paused BOOLEAN NOT NULL DEFAULT FALSE,
    settings_locked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (series, event),
    FOREIGN KEY (series, event) REFERENCES events(series, event) ON DELETE CASCADE,
    CHECK (score_maximum >= score_minimum),
    CHECK (requests_close_at IS NULL OR requests_open_at IS NULL OR requests_close_at > requests_open_at),
    CHECK (starts_close_at IS NULL OR requests_close_at IS NULL OR starts_close_at >= requests_close_at),
    CHECK (submissions_close_at IS NULL OR starts_close_at IS NULL OR submissions_close_at >= starts_close_at)
);

CREATE TABLE qualifier_modes (
    id BIGSERIAL PRIMARY KEY,
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    position SMALLINT NOT NULL CHECK (position > 0),
    slug VARCHAR(40) NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9_-]*$'),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    seed_gen_type VARCHAR(20) NOT NULL,
    seed_config JSONB NOT NULL CHECK (jsonb_typeof(seed_config) = 'object'),
    generator_profile VARCHAR(80) NOT NULL CHECK (btrim(generator_profile) <> ''),
    settings_fingerprint TEXT NOT NULL CHECK (btrim(settings_fingerprint) <> ''),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (series, event) REFERENCES pooled_qualifier_configs(series, event) ON DELETE CASCADE,
    UNIQUE (series, event, position),
    UNIQUE (series, event, slug),
    UNIQUE (id, series, event)
);

CREATE TABLE qualifier_seeds (
    id BIGSERIAL PRIMARY KEY,
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    mode_id BIGINT NOT NULL,
    source VARCHAR(12) NOT NULL CHECK (source IN ('async_pool', 'live')),
    pool_position SMALLINT CHECK (pool_position > 0),
    live_race_id BIGINT,
    generation_state VARCHAR(12) NOT NULL DEFAULT 'pending'
        CHECK (generation_state IN ('pending', 'generating', 'ready', 'failed')),
    seed_data JSONB CHECK (seed_data IS NULL OR jsonb_typeof(seed_data) = 'object'),
    generator_profile VARCHAR(80) NOT NULL,
    physical_seed_identity TEXT,
    settings_fingerprint TEXT NOT NULL,
    generated_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    entry_closed_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (mode_id, series, event) REFERENCES qualifier_modes(id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (live_race_id, series, event) REFERENCES races(id, series, event) ON DELETE RESTRICT,
    CHECK ((source = 'async_pool' AND pool_position IS NOT NULL AND live_race_id IS NULL)
        OR (source = 'live' AND pool_position IS NULL AND live_race_id IS NOT NULL)),
    CHECK (generation_state <> 'ready' OR (seed_data IS NOT NULL AND physical_seed_identity IS NOT NULL)),
    UNIQUE (mode_id, pool_position),
    UNIQUE (live_race_id),
    UNIQUE (series, event, physical_seed_identity),
    UNIQUE (id, mode_id, series, event)
);

CREATE TABLE qualifier_attempts (
    id BIGSERIAL PRIMARY KEY,
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    mode_id BIGINT NOT NULL,
    seed_id BIGINT NOT NULL,
    team_id BIGINT NOT NULL,
    attempt_sequence SMALLINT NOT NULL CHECK (attempt_sequence > 0),
    source VARCHAR(8) NOT NULL CHECK (source IN ('async', 'live')),
    state VARCHAR(24) NOT NULL DEFAULT 'assigned' CHECK (state IN (
        'assigned', 'revealed', 'starting', 'running', 'awaiting_verification', 'finalized', 'void'
    )),
    counts_for_entrant BOOLEAN NOT NULL DEFAULT TRUE,
    par_eligible BOOLEAN NOT NULL DEFAULT TRUE,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revealed_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    deadline_at TIMESTAMPTZ,
    player_finished_at TIMESTAMPTZ,
    official_outcome VARCHAR(10) CHECK (official_outcome IN ('finished', 'forfeit', 'dq', 'invalid')),
    official_time INTERVAL,
    vod TEXT,
    verified_at TIMESTAMPTZ,
    verified_by BIGINT REFERENCES users(id),
    discord_thread BIGINT,
    control_version BIGINT NOT NULL DEFAULT 1 CHECK (control_version > 0),
    retry_of BIGINT,
    superseded_by BIGINT,
    retry_banned_at TIMESTAMPTZ,
    retry_banned_by BIGINT REFERENCES users(id),
    retry_ban_reason TEXT,
    voided_at TIMESTAMPTZ,
    voided_by BIGINT REFERENCES users(id),
    void_reason TEXT,
    created_by BIGINT REFERENCES users(id),
    FOREIGN KEY (mode_id, series, event) REFERENCES qualifier_modes(id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (seed_id, mode_id, series, event) REFERENCES qualifier_seeds(id, mode_id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (team_id, series, event) REFERENCES teams(id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (retry_of, team_id, mode_id, series, event)
        REFERENCES qualifier_attempts(id, team_id, mode_id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (superseded_by, series, event)
        REFERENCES qualifier_attempts(id, series, event) ON DELETE RESTRICT,
    UNIQUE (team_id, mode_id, attempt_sequence),
    UNIQUE (id, team_id, mode_id, series, event),
    UNIQUE (id, series, event),
    UNIQUE (discord_thread),
    CHECK ((official_outcome = 'finished') = (official_time IS NOT NULL)
        OR (official_outcome IS NULL AND official_time IS NULL)),
    CHECK ((state = 'finalized') = (official_outcome IS NOT NULL)),
    CHECK ((retry_banned_at IS NULL) = (retry_ban_reason IS NULL)),
    CHECK ((voided_at IS NULL) = (void_reason IS NULL))
);

CREATE UNIQUE INDEX qualifier_attempts_counted_per_mode
    ON qualifier_attempts(team_id, mode_id) WHERE counts_for_entrant AND state <> 'void';
CREATE UNIQUE INDEX qualifier_attempts_one_active_async
    ON qualifier_attempts(team_id) WHERE source = 'async' AND state IN ('assigned', 'revealed', 'starting', 'running');
CREATE UNIQUE INDEX qualifier_attempts_live_team
    ON qualifier_attempts(seed_id, team_id) WHERE source = 'live';
CREATE UNIQUE INDEX qualifier_attempts_one_retry
    ON qualifier_attempts(team_id) WHERE retry_of IS NOT NULL AND state <> 'void';
CREATE INDEX qualifier_attempts_seed_results ON qualifier_attempts(seed_id, state, par_eligible);

CREATE TABLE qualifier_live_entries (
    id BIGSERIAL PRIMARY KEY,
    seed_id BIGINT NOT NULL,
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    mode_id BIGINT NOT NULL,
    racetime_entrant_id TEXT NOT NULL,
    user_id BIGINT REFERENCES users(id),
    team_id BIGINT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    departed_at TIMESTAMPTZ,
    eligibility_frozen_at TIMESTAMPTZ,
    eligible BOOLEAN,
    exclusion_reason TEXT,
    present_at_go BOOLEAN,
    retry_original_attempt_id BIGINT,
    retry_reserved_at TIMESTAMPTZ,
    retry_committed_at TIMESTAMPTZ,
    retry_released_at TIMESTAMPTZ,
    retry_declared_by BIGINT REFERENCES users(id),
    retry_reason TEXT,
    attempt_id BIGINT REFERENCES qualifier_attempts(id) ON DELETE RESTRICT,
    FOREIGN KEY (seed_id, mode_id, series, event) REFERENCES qualifier_seeds(id, mode_id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (team_id, series, event) REFERENCES teams(id, series, event) ON DELETE RESTRICT,
    FOREIGN KEY (retry_original_attempt_id, team_id, mode_id, series, event)
        REFERENCES qualifier_attempts(id, team_id, mode_id, series, event) ON DELETE RESTRICT,
    UNIQUE (seed_id, racetime_entrant_id),
    CHECK ((eligible IS NULL AND eligibility_frozen_at IS NULL)
        OR (eligible IS NOT NULL AND eligibility_frozen_at IS NOT NULL)),
    CHECK (eligible IS DISTINCT FROM FALSE OR exclusion_reason IS NOT NULL),
    CHECK (retry_reserved_at IS NULL OR (retry_original_attempt_id IS NOT NULL AND team_id IS NOT NULL)),
    CHECK (retry_committed_at IS NULL OR retry_reserved_at IS NOT NULL),
    CHECK (retry_released_at IS NULL OR (retry_reserved_at IS NOT NULL AND retry_committed_at IS NULL))
);
CREATE UNIQUE INDEX qualifier_live_entries_one_retry_reservation
    ON qualifier_live_entries(team_id)
    WHERE retry_reserved_at IS NOT NULL AND retry_committed_at IS NULL AND retry_released_at IS NULL;

ALTER TABLE pooled_qualifier_configs OWNER TO mido;
ALTER TABLE restream_channels OWNER TO mido;
ALTER TABLE qualifier_modes OWNER TO mido;
ALTER TABLE qualifier_attempts OWNER TO mido;
ALTER TABLE qualifier_live_entries OWNER TO mido;

-- Durable delivery and generation stay on the existing workflow rows.
ALTER TABLE pooled_qualifier_configs ALTER COLUMN requests_paused SET DEFAULT TRUE;
ALTER TABLE qualifier_attempts
    ADD COLUMN allocation_metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    ADD COLUMN correction_history JSONB NOT NULL DEFAULT '[]'::JSONB,
    ADD COLUMN participant_outcome TEXT CHECK (participant_outcome IN ('finished', 'forfeit')),
    ADD COLUMN undo_until TIMESTAMPTZ,
    ADD COLUMN start_due_at TIMESTAMPTZ,
    ADD COLUMN delivery_claim TEXT,
    ADD COLUMN delivery_claim_until TIMESTAMPTZ,
    ADD COLUMN delivery_error TEXT,
    ADD COLUMN delivery_messages JSONB NOT NULL DEFAULT '{}'::JSONB;
ALTER TABLE qualifier_seeds
    ADD COLUMN generation_claim TEXT,
    ADD COLUMN generation_claim_until TIMESTAMPTZ,
    ADD COLUMN generation_error TEXT,
    ADD COLUMN settings_attested_by BIGINT REFERENCES users (id),
    ADD COLUMN settings_attested_at TIMESTAMPTZ;
