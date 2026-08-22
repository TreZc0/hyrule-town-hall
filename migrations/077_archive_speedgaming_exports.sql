ALTER TABLE public.speedgaming_exports
    ADD COLUMN archived_at TIMESTAMPTZ;

CREATE TABLE public.speedgaming_export_languages (
    export_id INTEGER NOT NULL REFERENCES public.speedgaming_exports(id) ON DELETE CASCADE,
    language public.language NOT NULL,

    PRIMARY KEY (export_id, language)
);

ALTER TABLE public.speedgaming_export_languages OWNER TO mido;

INSERT INTO public.speedgaming_export_languages (export_id, language)
SELECT id, language
FROM public.speedgaming_exports;

ALTER TABLE public.speedgaming_exports
    DROP CONSTRAINT speedgaming_exports_series_event_language_key;

ALTER TABLE public.speedgaming_exports
    DROP COLUMN language;

CREATE UNIQUE INDEX speedgaming_exports_active_event
    ON public.speedgaming_exports(series, event)
    WHERE archived_at IS NULL;

CREATE INDEX speedgaming_exports_archived_event_slug
    ON public.speedgaming_exports(series, event, slug, archived_at DESC)
    WHERE archived_at IS NOT NULL;
