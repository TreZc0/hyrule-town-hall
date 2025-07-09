-- Add notes field to signups table
ALTER TABLE signups ADD COLUMN notes VARCHAR(60);

-- Update existing signups to have NULL notes
UPDATE signups SET notes = NULL WHERE notes IS NULL; 