-- Add discord_async_channel column to events table
ALTER TABLE events 
ADD COLUMN discord_async_channel BIGINT; 

-- Add fields for async thread management
ALTER TABLE races 
ADD COLUMN async_thread1 BIGINT,
ADD COLUMN async_thread2 BIGINT,
ADD COLUMN async_thread3 BIGINT,
ADD COLUMN async_seed1 BOOLEAN DEFAULT false NOT NULL,
ADD COLUMN async_seed2 BOOLEAN DEFAULT false NOT NULL,
ADD COLUMN async_seed3 BOOLEAN DEFAULT false NOT NULL,
ADD COLUMN async_ready1 BOOLEAN DEFAULT false NOT NULL,
ADD COLUMN async_ready2 BOOLEAN DEFAULT false NOT NULL,
ADD COLUMN async_ready3 BOOLEAN DEFAULT false NOT NULL; 

-- Create table for storing async race times
CREATE TABLE async_times (
    id BIGSERIAL PRIMARY KEY,
    race_id BIGINT NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    async_part INTEGER NOT NULL CHECK (async_part IN (1, 2, 3)),
    finish_time INTERVAL NOT NULL,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    recorded_by BIGINT NOT NULL REFERENCES users(id),
    start_time TIMESTAMP WITH TIME ZONE,
    UNIQUE(race_id, async_part)
); 

ALTER TABLE public.async_times OWNER TO mido;