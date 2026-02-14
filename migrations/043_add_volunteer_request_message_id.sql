-- Add column to store the Discord message ID for volunteer request posts
-- This allows updating the message when signups change
ALTER TABLE races ADD COLUMN volunteer_request_message_id BIGINT;
