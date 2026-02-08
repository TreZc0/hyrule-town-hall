-- Notification channels per game per language
CREATE TABLE game_notification_channels (
    game_id    INTEGER     NOT NULL REFERENCES games(id),
    language   language    NOT NULL,
    guild_id   BIGINT      NOT NULL,
    channel_id BIGINT      NOT NULL,
    PRIMARY KEY (game_id, language)
);

ALTER TABLE public.game_notification_channels OWNER TO mido;

-- Game-level restream coordinators, scoped by language
CREATE TABLE game_restreamers (
    game_id    INTEGER  NOT NULL REFERENCES games(id),
    restreamer BIGINT   NOT NULL REFERENCES users(id),
    language   language NOT NULL,
    PRIMARY KEY (game_id, restreamer, language)
);

ALTER TABLE public.game_restreamers OWNER TO mido;