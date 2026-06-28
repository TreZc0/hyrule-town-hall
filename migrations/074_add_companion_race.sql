ALTER TABLE races
ADD COLUMN companion_race_id bigint REFERENCES races(id);

ALTER TABLE races
ADD CONSTRAINT races_companion_race_id_unique UNIQUE (companion_race_id);
