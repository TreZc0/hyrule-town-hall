ALTER TABLE events
    ADD COLUMN live_room_open_minutes_before SMALLINT NOT NULL DEFAULT 30
        CHECK (live_room_open_minutes_before BETWEEN 15 AND 60),
    ADD COLUMN async_room_open_minutes_before SMALLINT NOT NULL DEFAULT 30
        CHECK (async_room_open_minutes_before BETWEEN 15 AND 60);
