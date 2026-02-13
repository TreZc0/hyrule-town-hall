-- Add primary key to asyncs table
-- This is required for the ON CONFLICT clause in async upserts
ALTER TABLE asyncs ADD PRIMARY KEY (series, event, kind);
