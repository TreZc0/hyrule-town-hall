CREATE TABLE event_restreamer_discord_roles (
    series VARCHAR(20) NOT NULL,
    event VARCHAR(20) NOT NULL,
    language language NOT NULL,
    discord_role_id BIGINT NOT NULL,
    PRIMARY KEY (series, event, language),
    FOREIGN KEY (series, event) REFERENCES events(series, event) ON DELETE CASCADE
);

CREATE INDEX idx_event_restreamer_discord_roles_role_id
ON event_restreamer_discord_roles(discord_role_id);
