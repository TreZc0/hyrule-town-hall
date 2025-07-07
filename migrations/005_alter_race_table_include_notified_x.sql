-- Add notified support for all stages of an async

-- Add async_notified_1-3 columns to races table
ALTER TABLE races 
ADD COLUMN async_notified_1 BOOLEAN DEFAULT false NOT NULL;

ALTER TABLE races 
ADD COLUMN async_notified_2 BOOLEAN DEFAULT false NOT NULL;

ALTER TABLE races 
ADD COLUMN async_notified_3 BOOLEAN DEFAULT false NOT NULL;
