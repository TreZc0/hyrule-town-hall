-- Add missing enum types if they don't exist
DO $$ BEGIN
    CREATE TYPE role_request_status AS ENUM ('pending', 'approved', 'rejected');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE signup_status AS ENUM ('pending', 'confirmed', 'declined');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Update existing tables to use enum types if they're still using VARCHAR
ALTER TABLE role_requests 
ALTER COLUMN status TYPE role_request_status 
USING status::role_request_status;

ALTER TABLE signups 
ALTER COLUMN status TYPE signup_status 
USING status::signup_status; 