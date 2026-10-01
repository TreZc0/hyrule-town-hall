-- Existing schedules remain in UTC and retain the event's request window.
ALTER TABLE volunteer_ping_workflows
    ADD COLUMN schedule_timezone TEXT NOT NULL DEFAULT 'UTC',
    ADD COLUMN cutoff_hours INTEGER,
    ADD CONSTRAINT ping_cutoff_hours_positive CHECK (cutoff_hours IS NULL OR cutoff_hours BETWEEN 1 AND 168);
