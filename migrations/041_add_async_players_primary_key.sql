-- Add primary key to async_players table
-- This is required for the ON CONFLICT clause in /result-async command
ALTER TABLE async_players ADD PRIMARY KEY (series, event, player, kind);
