-- Make series and event columns nullable in role_bindings table
-- This allows game-level role bindings to have NULL series and event values

ALTER TABLE role_bindings 
ALTER COLUMN series DROP NOT NULL;

ALTER TABLE role_bindings 
ALTER COLUMN event DROP NOT NULL; 