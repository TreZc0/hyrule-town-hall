-- Add link column to async_times table for storing recording/VoD links
ALTER TABLE async_times 
ADD COLUMN link TEXT; 