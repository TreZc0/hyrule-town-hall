CREATE TABLE public.race_entrants (
    race bigint NOT NULL REFERENCES public.races(id) ON DELETE CASCADE,
    team bigint NOT NULL REFERENCES public.teams(id),
    position smallint NOT NULL CHECK (position > 0),
    PRIMARY KEY (race, position),
    UNIQUE (race, team)
);

ALTER TABLE race_entrants OWNER TO mido;
