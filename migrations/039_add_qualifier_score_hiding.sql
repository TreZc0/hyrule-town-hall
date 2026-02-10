CREATE TYPE qualifier_score_hiding AS ENUM ('none', 'async_only', 'full_points', 'full_points_counts', 'full_complete');
ALTER TABLE events ADD COLUMN qualifier_score_hiding qualifier_score_hiding DEFAULT 'none' NOT NULL;
