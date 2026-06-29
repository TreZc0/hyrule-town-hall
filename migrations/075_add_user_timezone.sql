ALTER TABLE users
ADD COLUMN timezone TEXT;

ALTER TABLE users
ADD CONSTRAINT users_timezone_not_empty
CHECK (timezone IS NULL OR timezone <> '');
