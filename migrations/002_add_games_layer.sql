-- Add games data layer
-- This migration adds support for games as containers for series and events

-- Create games table
CREATE TABLE games (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

ALTER TABLE public.games OWNER TO mido;

-- Create game_series table to link games to series
CREATE TABLE game_series (
    id SERIAL PRIMARY KEY,
    game_id INTEGER REFERENCES games(id) ON DELETE CASCADE,
    series VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(game_id, series)
);

ALTER TABLE public.game_series OWNER TO mido;

-- Create game_admins table to manage game administrators
CREATE TABLE game_admins (
    id SERIAL PRIMARY KEY,
    game_id INTEGER REFERENCES games(id) ON DELETE CASCADE,
    admin_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(game_id, admin_id)
);

ALTER TABLE public.game_admins OWNER TO mido;

-- Add force_custom_role_binding column to event table
ALTER TABLE events
ADD COLUMN force_custom_role_binding BOOLEAN DEFAULT TRUE;


-- Add game_id and event_id columns to role_bindings table
ALTER TABLE role_bindings 
ADD COLUMN game_id INTEGER REFERENCES games(id) ON DELETE CASCADE;

-- Add custom_pool column to role_bindings table to define when volunteers can't come from the game pool
ALTER TABLE role_bindings 
ADD COLUMN custom_pool BOOLEAN DEFAULT FALSE;

-- Add constraint to ensure custom_pool is only false if game_id is not null
ALTER TABLE role_bindings 
ADD CONSTRAINT check_custom_pool 
CHECK (NOT custom_pool OR game_id IS NOT NULL);

-- Add constraint to ensure role_bindings reference either a game or an event, not both
ALTER TABLE role_bindings 
ADD CONSTRAINT check_game_or_event 
CHECK ((game_id IS NOT NULL AND event IS NULL) OR (game_id IS NULL AND event IS NOT NULL));

-- Insert initial games
INSERT INTO games (name, display_name, description) VALUES
('ootr', 'Ocarina of Time Randomizer', 'Ocarina of Time Randomizer tournaments and events'),
('alttpr', 'A Link to the Past Randomizer', 'A Link to the Past Randomizer tournaments and events');

-- Insert initial game_series mappings
INSERT INTO game_series (game_id, series) VALUES
(1, 's'),      -- Standard series for OOTR
(1, 'league'), -- League series for OOTR
(1, 'rsl'),    -- RSL series for OOTR
(1, 'mw'),     -- Multiworld series for OOTR
(1, 'mp'),     -- Mixed Pools series for OOTR
(1, 'br'),     -- Battle Royale series for OOTR
(1, 'fr'),     -- French Tournament series for OOTR
(1, 'tfb'),    -- Triforce Blitz series for OOTR
(1, 'ohko'),   -- One Hit KO series for OOTR
(1, 'mq'),     -- MQ series for OOTR
(1, 'scrubs'), -- Scrubs series for OOTR
(1, 'sgl'),    -- SpeedGaming series for OOTR
(1, 'soh'),    -- Songs of Hope series for OOTR
(1, 'wttbb'),  -- We Try To Be Better series for OOTR
(1, 'coop'),   -- CoOp series for OOTR
(1, 'pic'),    -- Pictionary series for OOTR
(1, 'ndos'),   -- Nine Days of Saws series for OOTR
(1, 'xkeys');  -- Crosskeys series for OOTR
