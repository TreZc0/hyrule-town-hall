-- Add asyncs_active and swiss_standings configuration columns to events table
ALTER TABLE public.events 
ADD COLUMN asyncs_active boolean DEFAULT true NOT NULL,
ADD COLUMN swiss_standings boolean DEFAULT false NOT NULL;
