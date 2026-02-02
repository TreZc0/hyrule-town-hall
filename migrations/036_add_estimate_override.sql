-- Add estimate_override field to ZSR export configs
-- This allows overriding the auto-calculated estimate from series default duration

ALTER TABLE public.zsr_restream_exports
ADD COLUMN estimate_override VARCHAR(10);

COMMENT ON COLUMN public.zsr_restream_exports.estimate_override
IS 'Optional override for the estimate time (HH:MM:SS format). If null, uses series default_race_duration.';
