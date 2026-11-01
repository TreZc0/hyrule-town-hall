-- Give each async its own identity; kind describes its purpose.
ALTER TABLE asyncs ADD COLUMN id BIGSERIAL, ADD COLUMN name TEXT;
UPDATE asyncs SET name = CASE kind::text
    WHEN 'qualifier' THEN 'Async 1' WHEN 'qualifier2' THEN 'Async 2'
    WHEN 'qualifier3' THEN 'Async 3' WHEN 'seeding' THEN 'Seeding'
    WHEN 'tiebreaker1' THEN 'Tiebreaker 1' ELSE 'Tiebreaker 2' END;

ALTER TABLE async_teams ADD COLUMN async_id BIGINT;
ALTER TABLE async_players ADD COLUMN async_id BIGINT;
UPDATE async_teams entry SET async_id = a.id FROM asyncs a, teams t
    WHERE entry.team = t.id AND a.series = t.series AND a.event = t.event AND a.kind = entry.kind;
UPDATE async_players entry SET async_id = a.id FROM asyncs a
    WHERE a.series = entry.series AND a.event = entry.event AND a.kind = entry.kind;

ALTER TABLE asyncs DROP CONSTRAINT asyncs_pkey;
ALTER TABLE asyncs
    ADD PRIMARY KEY (id),
    ADD UNIQUE (id, series, event),
    ALTER COLUMN name SET NOT NULL,
    ADD CHECK (length(trim(name)) BETWEEN 1 AND 100);
CREATE INDEX asyncs_event_order ON asyncs (series, event, id);

ALTER TABLE async_teams DROP CONSTRAINT async_teams_pkey;
ALTER TABLE async_teams DROP COLUMN kind,
    ALTER COLUMN async_id SET NOT NULL,
    ADD PRIMARY KEY (team, async_id),
    ADD FOREIGN KEY (async_id) REFERENCES asyncs(id);
ALTER TABLE async_players DROP CONSTRAINT async_players_pkey;
ALTER TABLE async_players DROP COLUMN kind,
    ALTER COLUMN async_id SET NOT NULL,
    ADD PRIMARY KEY (series, event, player, async_id),
    ADD FOREIGN KEY (async_id, series, event) REFERENCES asyncs(id, series, event);

UPDATE asyncs SET kind = 'qualifier' WHERE kind IN ('qualifier2', 'qualifier3');

