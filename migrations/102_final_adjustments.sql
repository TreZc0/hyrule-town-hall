ALTER TABLE events RENAME COLUMN goal_slug TO racetime_goal_slug;

-- New boolean event flags to replace hardcoded series/event checks

ALTER TABLE events ADD COLUMN is_live_event BOOL NOT NULL DEFAULT false;

