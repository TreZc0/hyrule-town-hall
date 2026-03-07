-- Maps a restream channel URL to a short alias for export to Google Sheets
CREATE TABLE zsr_channel_aliases (
    channel_url TEXT PRIMARY KEY,
    alias TEXT NOT NULL
);

ALTER TABLE public.zsr_channel_aliases OWNER TO mido;