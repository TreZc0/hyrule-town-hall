-- Add primary key to existing phase/round mappings table (needed for row-level remove UI)
ALTER TABLE public.startgg_phase_round_mappings ADD COLUMN id bigserial PRIMARY KEY;

-- New table for pool identifier → display name mappings
CREATE TABLE public.startgg_pool_name_mappings (
    series character varying(20) NOT NULL,
    event character varying(20) NOT NULL,
    original_identifier text NOT NULL,
    mapped_name text NOT NULL,
    CONSTRAINT startgg_pool_name_mappings_series_event_fkey
        FOREIGN KEY (series, event) REFERENCES public.events(series, event)
);

ALTER TABLE public.startgg_pool_name_mappings OWNER TO mido;
