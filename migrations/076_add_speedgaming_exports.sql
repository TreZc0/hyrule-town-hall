CREATE TYPE public.speedgaming_export_trigger AS ENUM (
    'when_scheduled',
    'when_restream_channel_set',
    'when_volunteer_signed_up'
);

CREATE TYPE public.speedgaming_delivery_state AS ENUM (
    'pending',
    'in_progress',
    'succeeded',
    'failed',
    'ambiguous'
);

CREATE TABLE public.speedgaming_exports (
    id SERIAL PRIMARY KEY,
    series VARCHAR(24) NOT NULL,
    event VARCHAR(24) NOT NULL,
    language public.language NOT NULL,
    slug TEXT NOT NULL,
    trigger_condition public.speedgaming_export_trigger NOT NULL,
    delay_minutes INTEGER NOT NULL DEFAULT 0 CHECK (delay_minutes >= 0),
    export_volunteers BOOLEAN NOT NULL DEFAULT false,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(series, event, language),
    FOREIGN KEY (series, event) REFERENCES public.events(series, event) ON DELETE CASCADE,
    CHECK (slug <> '' AND slug !~ '[/?#[:space:]]')
);

ALTER TABLE public.speedgaming_exports OWNER TO mido;

CREATE TABLE public.speedgaming_race_exports (
    race_id BIGINT NOT NULL REFERENCES public.races(id) ON DELETE CASCADE,
    export_id INTEGER NOT NULL REFERENCES public.speedgaming_exports(id) ON DELETE CASCADE,
    episode_id BIGINT,
    state public.speedgaming_delivery_state NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempt_at TIMESTAMPTZ,
    exported_at TIMESTAMPTZ,
    last_polled_at TIMESTAMPTZ,

    PRIMARY KEY (race_id, export_id),
    CHECK ((state = 'succeeded') = (episode_id IS NOT NULL))
);

ALTER TABLE public.speedgaming_race_exports OWNER TO mido;

CREATE UNIQUE INDEX speedgaming_race_exports_episode
    ON public.speedgaming_race_exports(export_id, episode_id)
    WHERE episode_id IS NOT NULL;

CREATE INDEX speedgaming_race_exports_export
    ON public.speedgaming_race_exports(export_id);

CREATE TABLE public.speedgaming_volunteer_exports (
    signup_id INTEGER NOT NULL REFERENCES public.signups(id) ON DELETE CASCADE,
    export_id INTEGER NOT NULL REFERENCES public.speedgaming_exports(id) ON DELETE CASCADE,
    state public.speedgaming_delivery_state NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempt_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ,

    PRIMARY KEY (signup_id, export_id),
    CHECK ((state = 'succeeded') = (submitted_at IS NOT NULL))
);

ALTER TABLE public.speedgaming_volunteer_exports OWNER TO mido;

CREATE INDEX speedgaming_volunteer_exports_export
    ON public.speedgaming_volunteer_exports(export_id);
