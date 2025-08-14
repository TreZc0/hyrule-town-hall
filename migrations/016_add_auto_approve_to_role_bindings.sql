-- Add auto_approve field to role_bindings table
-- This determines if role requests are automatically approved
-- Defaults to false (manual approval required)

ALTER TABLE role_bindings 
ADD COLUMN auto_approve BOOLEAN NOT NULL DEFAULT false; 