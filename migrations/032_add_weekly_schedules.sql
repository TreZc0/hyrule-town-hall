-- Add weekly_schedules table for configurable recurring race schedules
CREATE TABLE public.weekly_schedules (
    id BIGINT NOT NULL PRIMARY KEY,
    series VARCHAR(20) NOT NULL,
    event VARCHAR(20) NOT NULL,
    name TEXT NOT NULL,
    frequency_days SMALLINT NOT NULL,
    time_of_day TIME NOT NULL,
    timezone TEXT NOT NULL,
    anchor_date DATE NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE (series, event, name),
    FOREIGN KEY (series, event) REFERENCES public.events(series, event)
);

ALTER TABLE public.weekly_schedules OWNER TO mido;

CREATE INDEX idx_weekly_schedules_series_event ON public.weekly_schedules (series, event);
