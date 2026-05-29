ALTER TABLE races
ADD COLUMN custom_title text,
ADD COLUMN custom_create_room boolean DEFAULT true NOT NULL;
