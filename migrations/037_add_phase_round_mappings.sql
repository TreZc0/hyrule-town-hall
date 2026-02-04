CREATE TABLE public.startgg_phase_round_mappings (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    original_phase text,
    original_round text,
    mapped_phase text,
    mapped_round text
);


ALTER TABLE public.startgg_phase_round_mappings OWNER TO mido;

ALTER TABLE ONLY public.startgg_phase_round_mappings
    ADD CONSTRAINT startgg_phase_round_mappings_series_event_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);