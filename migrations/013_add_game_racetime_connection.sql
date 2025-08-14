-- Add game_racetime_connection table for per-game racetime bot credentials
-- This migration adds support for multiple racetime bot credentials per game

-- Create game_racetime_connection table
CREATE TABLE game_racetime_connection (
    id SERIAL PRIMARY KEY,
    game_id INTEGER REFERENCES games(id) ON DELETE CASCADE,
    category_slug VARCHAR(255) NOT NULL,
    client_id VARCHAR(255) NOT NULL,
    client_secret VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(game_id, category_slug)
);

ALTER TABLE public.game_racetime_connection OWNER TO mido;

-- Insert initial connections for existing games
-- Note: These will need to be updated with actual credentials
INSERT INTO game_racetime_connection (game_id, category_slug, client_id, client_secret) VALUES
(1, 'ootr', '', ''),  -- OOTR game
(2, 'alttpr', '', '');  -- ALttPR game 