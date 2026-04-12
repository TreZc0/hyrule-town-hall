-- Add append_mode field to ZSR export configs
-- This controls whether the draft mode should be appended to the title in brackets

ALTER TABLE public.zsr_restream_exports
ADD COLUMN append_mode BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN public.zsr_restream_exports.append_mode
IS 'If true, appends the draft mode (if available) to the race title in square brackets.';
