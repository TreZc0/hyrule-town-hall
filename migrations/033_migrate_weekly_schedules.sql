-- Seed existing weekly schedules from hardcoded values

-- Standard weeklies (currently on hiatus during s/9 qualifiers, so active = false)
-- settings_description is "variety" (from SHORT_WEEKLY_SETTINGS constant)
-- notification_channel_id is NULL (removing the @Standard role ping)
-- room_open_minutes_before is 30 (current default)
INSERT INTO weekly_schedules (id, series, event, name, frequency_days, time_of_day, timezone, anchor_date, active, settings_description, notification_channel_id, room_open_minutes_before) VALUES
(1, 's', 'w', 'Kokiri', 14, '18:00:00', 'America/New_York', '2025-01-04', false, 'variety', NULL, 30),
(2, 's', 'w', 'Goron', 14, '14:00:00', 'America/New_York', '2025-01-05', false, 'variety', NULL, 30),
(3, 's', 'w', 'Zora', 14, '14:00:00', 'America/New_York', '2025-01-11', false, 'variety', NULL, 30),
(4, 's', 'w', 'Gerudo', 14, '09:00:00', 'America/New_York', '2025-01-12', false, 'variety', NULL, 30);

-- TWWR weekly (active)
INSERT INTO weekly_schedules (id, series, event, name, frequency_days, time_of_day, timezone, anchor_date, active, settings_description, notification_channel_id, room_open_minutes_before) VALUES
(5, 'twwrmain', 'w', 'Saturday', 7, '18:00:00', 'America/New_York', '2026-01-24', true, 'Preliminary Miniblins S2 Settings', NULL, 30);
