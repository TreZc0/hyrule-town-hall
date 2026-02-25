CREATE TYPE ping_interval AS ENUM ('daily', 'weekly');
CREATE TYPE ping_workflow_type AS ENUM ('scheduled', 'per_race');

CREATE TABLE volunteer_ping_workflows (
    id SERIAL PRIMARY KEY,
    -- Either game_id OR (series + event), same pattern as role_bindings
    game_id INTEGER REFERENCES games(id) ON DELETE CASCADE,
    series VARCHAR(50),
    event VARCHAR(50),
    language language NOT NULL DEFAULT 'en',
    discord_ping_channel BIGINT,          -- NULL = fall back to discord_volunteer_info_channel
    delete_after_race BOOLEAN NOT NULL DEFAULT false,
    workflow_type ping_workflow_type NOT NULL,
    -- For 'scheduled' type:
    ping_interval ping_interval,          -- 'daily' or 'weekly'
    schedule_time TIME,                   -- UTC time of day
    schedule_day_of_week SMALLINT,        -- 0=Mon..6=Sun, only for weekly
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT game_or_event CHECK (
        (game_id IS NOT NULL AND series IS NULL AND event IS NULL) OR
        (game_id IS NULL AND series IS NOT NULL AND event IS NOT NULL)
    ),
    CONSTRAINT scheduled_fields CHECK (
        workflow_type != 'scheduled' OR (ping_interval IS NOT NULL AND schedule_time IS NOT NULL)
    ),
    CONSTRAINT weekly_day CHECK (
        ping_interval IS NULL OR ping_interval != 'weekly' OR schedule_day_of_week IS NOT NULL
    )
);

-- Multiple lead times per per_race workflow
CREATE TABLE volunteer_ping_lead_times (
    id SERIAL PRIMARY KEY,
    workflow_id INTEGER NOT NULL REFERENCES volunteer_ping_workflows(id) ON DELETE CASCADE,
    lead_time_hours INTEGER NOT NULL,
    UNIQUE (workflow_id, lead_time_hours)
);

-- Tracks every sent ping message (for dedup and deletion)
CREATE TABLE volunteer_ping_messages (
    id SERIAL PRIMARY KEY,
    workflow_id INTEGER NOT NULL REFERENCES volunteer_ping_workflows(id) ON DELETE CASCADE,
    race_id BIGINT REFERENCES races(id) ON DELETE SET NULL,  -- NULL for scheduled pings
    lead_time_hours INTEGER,                                   -- NULL for scheduled pings
    message_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

-- Prevent re-sending the same per-race ping
CREATE UNIQUE INDEX idx_volunteer_ping_messages_race_dedup
    ON volunteer_ping_messages (workflow_id, race_id, lead_time_hours)
    WHERE race_id IS NOT NULL AND lead_time_hours IS NOT NULL;

ALTER TABLE public.volunteer_ping_workflows OWNER TO mido;
ALTER TABLE public.volunteer_ping_lead_times OWNER TO mido;
ALTER TABLE public.volunteer_ping_messages OWNER TO mido;
