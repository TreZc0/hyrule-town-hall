-- Add generic seed_data JSONB column to asyncs table
ALTER TABLE asyncs ADD COLUMN IF NOT EXISTS seed_data JSONB;
ALTER TABLE prerolled_seeds ADD COLUMN IF NOT EXISTS seed_data JSONB;
