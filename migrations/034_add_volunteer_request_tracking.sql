-- Add volunteer request settings to events table
ALTER TABLE public.events
ADD COLUMN volunteer_requests_enabled BOOLEAN DEFAULT false NOT NULL,
ADD COLUMN volunteer_request_lead_time_hours INTEGER DEFAULT 48 NOT NULL,
ADD COLUMN volunteer_request_ping_enabled BOOLEAN DEFAULT true NOT NULL;

-- Add tracking column to races table
ALTER TABLE public.races
ADD COLUMN volunteer_request_sent BOOLEAN DEFAULT false NOT NULL;

-- Partial index for efficient querying of races needing announcements
CREATE INDEX idx_races_volunteer_request_pending
ON public.races(volunteer_request_sent)
WHERE volunteer_request_sent = false;
