CREATE TABLE event_descriptions (
    series character varying(8) NOT NULL,
    event  character varying(8) NOT NULL,
    content text NOT NULL,
    PRIMARY KEY (series, event)
);

ALTER TABLE public.event_descriptions OWNER TO mido;
