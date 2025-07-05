-- Add hash_icon enum type and update races table columns
-- This migration creates a new enum type for ALTTP hash icons and updates the races table

ALTER TYPE hash_icon rename to hash_icon_old;

-- Create the new enum type with all HashIcon values
CREATE TYPE hash_icon AS ENUM (
    'Bomb',
    'Bombos',
    'Boomerang',
    'Bow',
    'Hookshot',
    'Mushroom',
    'Pendant',
    'Powder',
    'Rod',
    'Ether',
    'Quake',
    'Lamp',
    'Hammer',
    'Shovel',
    'Ocarina',
    'Bug Net',
    'Book',
    'Bottle',
    'Potion',
    'Cane',
    'Cape',
    'Mirror',
    'Boots',
    'Gloves',
    'Flippers',
    'Pearl',
    'Shield',
    'Tunic',
    'Heart',
    'Map',
    'Compass',
    'Key'
);

-- Alter the races table columns to use the new enum type
ALTER TABLE races 
    ALTER COLUMN hash1 TYPE hash_icon USING hash1::text::hash_icon,
    ALTER COLUMN hash2 TYPE hash_icon USING hash2::text::hash_icon,
    ALTER COLUMN hash3 TYPE hash_icon USING hash3::text::hash_icon,
    ALTER COLUMN hash4 TYPE hash_icon USING hash4::text::hash_icon,
    ALTER COLUMN hash5 TYPE hash_icon USING hash5::text::hash_icon; 

ALTER TABLE asyncs 
    ALTER COLUMN hash1 TYPE hash_icon USING hash1::text::hash_icon,
    ALTER COLUMN hash2 TYPE hash_icon USING hash2::text::hash_icon,
    ALTER COLUMN hash3 TYPE hash_icon USING hash3::text::hash_icon,
    ALTER COLUMN hash4 TYPE hash_icon USING hash4::text::hash_icon,
    ALTER COLUMN hash5 TYPE hash_icon USING hash5::text::hash_icon; 

ALTER TABLE rsl_seeds 
    ALTER COLUMN hash1 TYPE hash_icon USING hash1::text::hash_icon,
    ALTER COLUMN hash2 TYPE hash_icon USING hash2::text::hash_icon,
    ALTER COLUMN hash3 TYPE hash_icon USING hash3::text::hash_icon,
    ALTER COLUMN hash4 TYPE hash_icon USING hash4::text::hash_icon,
    ALTER COLUMN hash5 TYPE hash_icon USING hash5::text::hash_icon; 

ALTER TABLE prerolled_seeds 
    ALTER COLUMN hash1 TYPE hash_icon USING hash1::text::hash_icon,
    ALTER COLUMN hash2 TYPE hash_icon USING hash2::text::hash_icon,
    ALTER COLUMN hash3 TYPE hash_icon USING hash3::text::hash_icon,
    ALTER COLUMN hash4 TYPE hash_icon USING hash4::text::hash_icon,
    ALTER COLUMN hash5 TYPE hash_icon USING hash5::text::hash_icon; 