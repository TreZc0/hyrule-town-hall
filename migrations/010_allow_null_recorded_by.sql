-- Allow NULL values for recorded_by and recorded_at in async_times table
-- Ready records should have recorded_by and recorded_at as NULL to distinguish them from reported records
-- recorded_at should only be set when an actual result is recorded via result-async or forfeit-async
ALTER TABLE async_times 
ALTER COLUMN recorded_by DROP NOT NULL;

ALTER TABLE async_times 
ALTER COLUMN recorded_at DROP DEFAULT,
ALTER COLUMN recorded_at DROP NOT NULL;
