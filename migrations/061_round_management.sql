CREATE TABLE public.event_phase_configs (
    id                         bigserial PRIMARY KEY,
    series                     varchar(8) NOT NULL,
    event                      varchar(8) NOT NULL,
    phase                      text NOT NULL,
    restream_consent_required  bool NOT NULL DEFAULT false,
    scheduling_deadline        timestamptz,
    FOREIGN KEY (series, event) REFERENCES public.events(series, event),
    UNIQUE (series, event, phase)
);

ALTER TABLE public.races ADD COLUMN scheduling_deadline          timestamptz;
ALTER TABLE public.races ADD COLUMN deadline_reminded_3d         bool NOT NULL DEFAULT false;
ALTER TABLE public.races ADD COLUMN deadline_reminded_24h        bool NOT NULL DEFAULT false;
ALTER TABLE public.races ADD COLUMN deadline_organizer_notified  bool NOT NULL DEFAULT false;

ALTER TABLE public.event_phase_configs OWNER TO mido;
