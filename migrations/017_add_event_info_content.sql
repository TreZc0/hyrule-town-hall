-- Add event_info_content table to store HTML content for event info pages
CREATE TABLE public.event_info_content (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    content text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT event_info_content_pkey PRIMARY KEY (series, event),
    CONSTRAINT event_info_content_series_event_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event) ON DELETE CASCADE
);

-- Add index for efficient lookups
CREATE INDEX idx_event_info_content_series_event ON public.event_info_content USING btree (series, event);

-- Add trigger to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_event_info_content_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER event_info_content_updated_at_trigger
    BEFORE UPDATE ON public.event_info_content
    FOR EACH ROW
    EXECUTE FUNCTION update_event_info_content_updated_at(); 

ALTER TABLE public.event_info_content OWNER TO mido;