-- Change hash icon columns from enum type to text
-- This migration converts the hash icon columns to use text strings instead of the enum type

-- Alter the races table columns to use text instead of hash_icon enum
ALTER TABLE races 
    ALTER COLUMN hash1 TYPE text USING hash1::text,
    ALTER COLUMN hash2 TYPE text USING hash2::text,
    ALTER COLUMN hash3 TYPE text USING hash3::text,
    ALTER COLUMN hash4 TYPE text USING hash4::text,
    ALTER COLUMN hash5 TYPE text USING hash5::text; 

ALTER TABLE asyncs 
    ALTER COLUMN hash1 TYPE text USING hash1::text,
    ALTER COLUMN hash2 TYPE text USING hash2::text,
    ALTER COLUMN hash3 TYPE text USING hash3::text,
    ALTER COLUMN hash4 TYPE text USING hash4::text,
    ALTER COLUMN hash5 TYPE text USING hash5::text; 

ALTER TABLE rsl_seeds 
    ALTER COLUMN hash1 TYPE text USING hash1::text,
    ALTER COLUMN hash2 TYPE text USING hash2::text,
    ALTER COLUMN hash3 TYPE text USING hash3::text,
    ALTER COLUMN hash4 TYPE text USING hash4::text,
    ALTER COLUMN hash5 TYPE text USING hash5::text; 

ALTER TABLE prerolled_seeds 
    ALTER COLUMN hash1 TYPE text USING hash1::text,
    ALTER COLUMN hash2 TYPE text USING hash2::text,
    ALTER COLUMN hash3 TYPE text USING hash3::text,
    ALTER COLUMN hash4 TYPE text USING hash4::text,
    ALTER COLUMN hash5 TYPE text USING hash5::text; 

-- Drop the hash_icon enum type since it's no longer needed
DROP TYPE hash_icon; 