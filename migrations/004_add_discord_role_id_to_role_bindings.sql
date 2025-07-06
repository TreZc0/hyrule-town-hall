-- Add Discord role ID support to role bindings
-- This migration adds support for Discord role assignment when role requests are approved

-- Add discord_role_id column to role_bindings table
ALTER TABLE role_bindings 
ADD COLUMN discord_role_id BIGINT;

-- Add constraint to ensure discord_role_id is only set for event role bindings (not game role bindings)
ALTER TABLE role_bindings 
ADD CONSTRAINT check_discord_role_event_only 
CHECK (discord_role_id IS NULL OR game_id IS NULL);

-- Add index for efficient Discord role lookups
CREATE INDEX idx_role_bindings_discord_role_id ON role_bindings(discord_role_id) WHERE discord_role_id IS NOT NULL;

-- Add pending_discord_invites table to track users who need Discord invites
CREATE TABLE pending_discord_invites (
    id SERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_request_id INTEGER NOT NULL REFERENCES role_requests(id) ON DELETE CASCADE,
    discord_guild_id BIGINT NOT NULL,
    discord_role_id BIGINT NOT NULL,
    invite_url TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE DEFAULT (NOW() + INTERVAL '7 days'),
    UNIQUE(user_id, role_request_id)
);

ALTER TABLE public.pending_discord_invites OWNER TO mido;

-- Add index for pending invites cleanup
CREATE INDEX idx_pending_discord_invites_expires_at ON pending_discord_invites(expires_at);
CREATE INDEX idx_pending_discord_invites_user_id ON pending_discord_invites(user_id);
