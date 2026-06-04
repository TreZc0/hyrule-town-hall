CREATE TABLE restream_channels (
    id SERIAL PRIMARY KEY,
    url_pattern VARCHAR(500) NOT NULL UNIQUE,
    discord_invite_url VARCHAR(500) NOT NULL,
    display_name VARCHAR(200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE restream_channels OWNER TO mido;
