ALTER TABLE events RENAME COLUMN goal_slug TO racetime_goal_slug;

-- New boolean event flags to replace hardcoded series/event checks

ALTER TABLE events ADD COLUMN is_live_event BOOL NOT NULL DEFAULT false;

-- Backfill existing events
UPDATE events SET is_live_event = true WHERE series = 'sgl' AND event LIKE '%live';

-- SongsOfHope qualifier kind via qualifier_score_kind column
UPDATE events SET qualifier_score_kind = 'songs_of_hope' WHERE series = 'soh' AND event = '1';

-- TriforceBlitz "Pieces Found" qualifier display via qualifier_score_kind column
UPDATE events SET qualifier_score_kind = 'triforce_blitz' WHERE series = 'tfb';
