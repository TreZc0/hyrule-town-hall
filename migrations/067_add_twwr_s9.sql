INSERT INTO events (
    series, event, display_name, short_name, listed,
    start, end_time,
    discord_guild, discord_race_room_channel, discord_race_results_channel,
    discord_organizer_channel, discord_scheduling_channel,
    discord_volunteer_info_channel, discord_async_channel,
    discord_invite_url,
    hide_teams_tab, show_qualifier_times, hide_races_tab,
    enter_flow, team_config, language, default_game_count,
    min_schedule_notice, show_opt_out, retime_window, auto_import,
    open_stream_delay, invitational_stream_delay,
    manual_reporting_with_breaks,
    asyncs_active, swiss_standings, force_custom_role_binding,
    emulator_settings_reminder, prevent_late_joins,
    discord_events_enabled, discord_events_require_restream,
    automated_asyncs, default_volunteer_language,
    settings_string,
    volunteer_requests_enabled, volunteer_request_lead_time_hours,
    qualifier_score_hiding, qualifier_notification_role_id,
    async_start_delay, startgg_double_rr, fpa_enabled
) VALUES (
    'twwrmain', 's9', 'Season 9 Tournament', 'Season 9', true,
    '2026-06-29 02:00:00+02', '2026-11-03 02:00:00+01',
    453718509600374794, 1473006745952583824, 1473006767930736712,
    1473006935463952568, 1473006899598459098,
    1473006868434649100, 1473006810880409765,
    'https://discord.gg/PnmpvafQYb',
    false, true, false,
    '{"requirements": [{"type": "raceTime"}, {"type": "twitch"}, {"type": "discord"}, {"type": "startGG", "optional": true}, {"type": "restreamConsent", "optional": true}]}'::jsonb,
    'solo', 'en', 1,
    '00:30:00', true, '00:00:03', false,
    '00:00:00', '00:00:00',
    false,
    true, false, false,
    false, false,
    true, false,
    true, 'en',
    'eJxLSS2LL0nMy8o31jPUMzTQM9czjk8xNkgxMk9mcGS4//1Tc+mHA/HnJOY/kNiv9INfkeHfX/4/wgYMDA5PT/3n/K/J/ceehUGg4Ub9f3v/nE32d0rsjf//L663/v+fg5UBBBy8liYysDEJKDRdnfGzJYFHgIGJkYkFIsUgzODAWMCgxMMBAMVjLfw=',
    true, 72,
    'none', 453759646407327754,
    15, false, false
);

UPDATE events
    SET rando_version = '{"type": "tww", "githubUrl": "https://github.com/tanjo3/wwrando/releases/tag/dev_tanjo3.1.10.7.3", "identifier": "wwrando-dev-tanjo3", "trackerLink": "wooferzfg.me/tww-rando-tracker/wwrando-dev-tanjo3"}'::jsonb
    WHERE series = 'twwrmain' AND event = 's9';
