CREATE TABLE event_volunteer_managers (
    series VARCHAR(255) NOT NULL,
    event VARCHAR(255) NOT NULL,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (series, event, user_id),
    FOREIGN KEY (series, event) REFERENCES events(series, event) ON DELETE CASCADE
);

ALTER TABLE event_volunteer_managers OWNER TO mido;

ALTER TABLE events ADD COLUMN volunteer_managers_can_manage_signups BOOLEAN NOT NULL DEFAULT FALSE;
