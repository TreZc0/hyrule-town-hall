-- ZSR Restreaming Export Feature
-- This migration adds tables for exporting race data to ZSR restreaming backend sheets

-- Trigger condition enum
CREATE TYPE public.zsr_export_trigger AS ENUM (
    'when_scheduled',
    'when_restream_channel_set',
    'when_volunteer_signed_up'
);

-- ZSR Restreaming Backends table
-- Stores configuration for each restreaming backend (ZSR, ZSRDE, ZSRFR, etc.)
CREATE TABLE public.zsr_restreaming_backends (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    google_sheet_id VARCHAR(255) NOT NULL,
    language public.language NOT NULL,

    -- Column letters for dynamic positioning (e.g., "R", "P", "Q", "I", "S")
    hth_export_id_col VARCHAR(5) NOT NULL,
    commentators_col VARCHAR(5) NOT NULL,
    trackers_col VARCHAR(5) NOT NULL,
    restream_channel_col VARCHAR(5) NOT NULL,
    notes_col VARCHAR(5) NOT NULL,

    -- DST calculation formulas (Google Sheets formula strings)
    -- Standard time formula (e.g., "=IF(A{row}=\"\",\"\",A{row}-Sheet2!$A$1)")
    dst_formula_standard TEXT NOT NULL,
    -- Daylight saving time formula (e.g., "=IF(A{row}=\"\",\"\",A{row}-Sheet2!$A$2)")
    dst_formula_dst TEXT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE public.zsr_restreaming_backends OWNER TO mido;

-- ZSR Restream Export Configurations (per series/event/backend)
-- Each event can have multiple export configurations, one per backend
CREATE TABLE public.zsr_restream_exports (
    id SERIAL PRIMARY KEY,
    series VARCHAR(24) NOT NULL,
    event VARCHAR(24) NOT NULL,
    backend_id INTEGER NOT NULL REFERENCES public.zsr_restreaming_backends(id) ON DELETE CASCADE,

    -- Optional override title (if null, uses event display_name)
    title VARCHAR(255),
    -- Description text for the export (shown in sheet)
    description TEXT,

    -- Export settings
    delay_minutes INTEGER NOT NULL DEFAULT 0,
    -- NodeCG package key for integration
    nodecg_pk INTEGER,
    -- When to trigger the export
    trigger_condition public.zsr_export_trigger NOT NULL,

    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One export per backend per event
    UNIQUE(series, event, backend_id),
    FOREIGN KEY (series, event) REFERENCES public.events(series, event) ON DELETE CASCADE
);

ALTER TABLE public.zsr_restream_exports OWNER TO mido;

-- Race export tracking table
-- Tracks which races have been exported to which backends
CREATE TABLE public.zsr_race_exports (
    race_id BIGINT NOT NULL REFERENCES public.races(id) ON DELETE CASCADE,
    export_id INTEGER NOT NULL REFERENCES public.zsr_restream_exports(id) ON DELETE CASCADE,
    -- The HTH Export ID written to the sheet (used to find/update the row)
    sheet_row_id VARCHAR(100) NOT NULL,
    -- When the race was first exported
    exported_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- When the race was last synced/updated
    last_synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (race_id, export_id)
);

ALTER TABLE public.zsr_race_exports OWNER TO mido;

-- Indexes for efficient querying
CREATE INDEX idx_zsr_restream_exports_series_event ON public.zsr_restream_exports(series, event);
CREATE INDEX idx_zsr_restream_exports_enabled ON public.zsr_restream_exports(enabled) WHERE enabled = true;
CREATE INDEX idx_zsr_race_exports_export_id ON public.zsr_race_exports(export_id);
