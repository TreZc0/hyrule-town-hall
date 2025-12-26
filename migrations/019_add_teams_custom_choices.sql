ALTER TABLE teams ADD COLUMN custom_choices jsonb DEFAULT '{}'::jsonb NOT NULL;
