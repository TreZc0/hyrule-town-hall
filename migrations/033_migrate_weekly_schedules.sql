-- Seed existing weekly schedules from hardcoded values

-- Standard weeklies (currently on hiatus during s/9 qualifiers, so active = false)
-- settings_description is "variety" (from SHORT_WEEKLY_SETTINGS constant)
-- notification_channel_id is NULL (removing the @Standard role ping)
-- room_open_minutes_before is 30 (current default)
-- TWWR weekly (active)
INSERT INTO weekly_schedules (id, series, event, name, frequency_days, time_of_day, timezone, anchor_date, active, settings_description, notification_channel_id, room_open_minutes_before) VALUES
(5, 'twwrmain', 'w', 'Saturday', 7, '18:00:00', 'America/New_York', '2026-01-24', true, 'Preliminary Miniblins S2 Settings', NULL, 30);
