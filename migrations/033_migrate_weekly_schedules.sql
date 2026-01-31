-- Seed existing weekly schedules from hardcoded values

-- Standard weeklies (currently on hiatus during s/9 qualifiers, so active = false)
INSERT INTO weekly_schedules (id, series, event, name, frequency_days, time_of_day, timezone, anchor_date, active) VALUES
(1, 's', 'w', 'Kokiri', 14, '18:00:00', 'America/New_York', '2025-01-04', false),
(2, 's', 'w', 'Goron', 14, '14:00:00', 'America/New_York', '2025-01-05', false),
(3, 's', 'w', 'Zora', 14, '14:00:00', 'America/New_York', '2025-01-11', false),
(4, 's', 'w', 'Gerudo', 14, '09:00:00', 'America/New_York', '2025-01-12', false);

-- TWWR weekly (active)
INSERT INTO weekly_schedules (id, series, event, name, frequency_days, time_of_day, timezone, anchor_date, active) VALUES
(5, 'twwrmain', 'w', 'Saturday', 7, '18:00:00', 'America/New_York', '2026-01-24', true);
