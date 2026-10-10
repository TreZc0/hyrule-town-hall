use rocket::response::content::RawText;
use {
    crate::{
        discord_bot::ADMIN_USER,
        event::{Data, Tab, enter},
        game::Game,
        prelude::*,
        racetime_bot::VersionedBranch,
        user::DisplaySource,
    },
    serenity::model::id::RoleId,
};

async fn can_manage_game(
    transaction: &mut Transaction<'_, Postgres>,
    user: &User,
    game: &Game,
) -> Result<bool, event::Error> {
    Ok(user.is_global_admin() || game.is_admin(transaction, user).await?)
}

async fn can_manage_series(
    transaction: &mut Transaction<'_, Postgres>,
    user: &User,
    series: Series,
) -> Result<bool, event::Error> {
    if user.is_global_admin() {
        return Ok(true);
    }
    let Some(game) = Game::from_series(transaction, series).await? else {
        return Ok(false);
    };
    Ok(game.is_admin(transaction, user).await?)
}

fn preroll_help() -> RawHtml<String> {
    html! {
        p(class = "help") : "Preroll controls advance seed generation, not when entrants receive the seed. For ALTTPR Door Rando, Avianart, both OWR builds and TWWR, None and Medium use the generator's normal timing; Short and Long are not supported. Pooled qualifier seeds are prepared with Generate on the Qualifiers page, independently of this setting.";
        details {
            summary : "What each preroll mode means";
            ul {
                li : "None: no additional advance-generation policy. This still allows automatic seed generation.";
                li : "Short: legacy web generation begins at a random time during the five minutes before the seed deadline (normally 20–15 minutes before the race).";
                li : "Medium: legacy web generation begins between room opening and 15 minutes before the seed deadline (normally 30 minutes before the race), or immediately if that window has passed.";
                li : "Long: legacy reserve-seed workflow prepares seeds in advance and replenishes the reserve. Requires fixed settings; not supported for ALTTPR, OWR or TWWR.";
            }
        }
    }
}

fn seed_config_help() -> RawHtml<String> {
    html! {
        details {
            summary : "Seed configuration examples";
            p : "Copy one JSON object into Seed Config JSON and select the matching generator above. These are examples of the configuration structure; replace settings, presets, URLs and permalinks with those approved for your event.";
            @for (title, description, config) in [
                ("OWR — baseline settings (regular or tournament)", "Select owr or owr_tourney. The generator selection chooses the installation; the JSON supplies its settings. For a pooled mode, use this structure in that mode's configuration.", json!({
                    "base_settings": {"shuffle": "crossed"}, "base_placements": {}, "start_inventory": [], "choices": {}
                })),
                ("OWR — player choices (regular or tournament)", "Choices patch the baseline when enabled. The choice key must match the signup booleanChoice or radioChoice field; practice seeds expose these choices too. See Player choices and their effect on seeds above for mutual agreement, random decisions and patch rules. Pooled qualifiers use baseline settings only. Optional priority orders patches, and supercedes suppresses other choices.", json!({
                    "base_settings": {"shuffle": "crossed"}, "base_placements": {}, "start_inventory": [],
                    "choices": {"flute": {"label": "Starting activated flute", "settings": {"flute_mode": "active"}, "start_inventory": ["Ocarina (Activated)"]}}
                })),
                ("OWR — named baselines with shared choices", "Structure example, not approved tournament settings: replace all three baselines with your intended full configurations. Select owr or owr_tourney and use matching preset keys in a generic draft. For either Door Rando build add source: mutual_choices. Without a draft, add default_baseline naming one entry.", json!({
                    "baselines": {
                        "mode_a": {"label": "Mode A", "base_settings": {"goal": "crystals"}, "base_placements": {}, "start_inventory": []},
                        "mode_b": {"label": "Mode B", "base_settings": {"goal": "dungeons"}, "base_placements": {}, "start_inventory": []},
                        "mode_c": {"label": "Mode C", "base_settings": {"goal": "completionist"}, "base_placements": {}, "start_inventory": []}
                    },
                    "choices": {"flute": {"label": "Starting activated flute", "settings": {"flute_mode": "active"}, "start_inventory": ["Ocarina (Activated)"]}}
                })),
                ("ALTTPR Door Rando — boothisman presets", "Select alttpr_dr (stable) or alttpr_dr_latest (latest). Presets come from a race draft or round mode; practice_modes defines the practice dropdown. This source is not supported for pooled qualifiers.", json!({
                    "source": "boothisman", "practice_modes": [{"value": "open", "label": "Open"}, {"value": "crosskeys", "label": "Crosskeys"}],
                    "practice_choices": [{"value": "pots", "label": "Pottery Shuffle"}]
                })),
                ("ALTTPR Door Rando — mutual choices", "Select alttpr_dr (stable) or alttpr_dr_latest (latest). base_settings is required. Uses the same choices/placements/inventory structure as OWR, but runs the Door Rando generator.", json!({
                    "source": "mutual_choices", "base_settings": {"shuffle": "crossed"}, "base_placements": {}, "start_inventory": [], "choices": {}
                })),
                ("ALTTPR Door Rando — mystery weights", "Select alttpr_dr (stable) or alttpr_dr_latest (latest). Replace the URL with your event's accessible mystery weights YAML.", json!({
                    "source": "mystery_pool", "mystery_weights_url": "https://example.com/event-weights.yaml"
                })),
                ("ALTTPR Avianart — default and practice presets", "Select alttpr_avianart. preset is the default when no draft supplies one. practice_presets optionally lists available practice options; otherwise practice uses preset. Pooled modes require a default preset.", json!({
                    "preset": "casualboots", "practice_presets": [{"value": "casualboots", "label": "Casual Boots"}, {"value": "open", "label": "Open"}]
                })),
                ("TWWR — settings permalink", "Select twwr and paste the settings permalink exported by your randomizer. Replace the placeholder before saving.", json!({
                    "permalink": "PASTE_YOUR_SETTINGS_PERMALINK_HERE"
                })),
                ("MMR — pinned version and settings", "Select mmr. Paste the full flat settings map exported by MMR and a version available on the selected branch. All competition seeds are encrypted; practice seeds are not. The application always generates a locked spoiler log.", json!({
                    "branch": "master", "version": "2.0.0-0", "settings": {"GameplaySettings.DrawHash": true, "OutputSettings.GenerateSpoilerLog": true}
                })),
                ("Manual / external seeds", "Select None. Leave Seed Config JSON empty or use an empty object.", json!({})),
            ] {
                details {
                    summary : title;
                    p : description;
                    pre { : serde_json::to_string_pretty(&config).expect("example JSON serializes"); }
                }
            }
            p : "MMR uses a pinned version and a flat settings export. All seeds have locked spoilers; competition seeds are encrypted. Multiworld and settings drafts are not supported.";
        }
    }
}

pub(crate) mod guides;
mod help;

async fn setup_form(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    event: Data<'_>,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    let participant_role_id: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM discord_roles WHERE series = $1 AND event = $2 AND role IS NULL AND racetime_team IS NULL",
        event.series as _, &*event.event
    ).fetch_optional(&mut *transaction).await?;
    let header = event
        .header(&mut transaction, me.as_ref(), Tab::Setup, false)
        .await?;

    // Load enter_flow, rando_version, seed_gen_type and seed_config as raw values for display in form
    let (enter_flow_json, rando_version_json, seed_gen_type_str, seed_config_json) = sqlx::query!(
        r#"
        SELECT enter_flow AS "enter_flow: serde_json::Value",
               rando_version AS "rando_version: serde_json::Value",
               seed_gen_type,
               seed_config AS "seed_config: serde_json::Value"
        FROM events WHERE series = $1 AND event = $2
    "#,
        event.series as _,
        &*event.event
    )
    .fetch_one(&mut *transaction)
    .await
    .map(|row| {
        (
            row.enter_flow,
            row.rando_version,
            row.seed_gen_type,
            row.seed_config,
        )
    })?;

    // Format enter_flow JSON for display
    let enter_flow_string = match &enter_flow_json {
        Some(json) => serde_json::to_string_pretty(json).unwrap_or_default(),
        None => String::new(),
    };

    let seed_config_string = match &seed_config_json {
        Some(json) => serde_json::to_string_pretty(json).unwrap_or_default(),
        None => String::new(),
    };

    // Format draft_config JSON for display
    let draft_config_string = match &event.draft_config {
        Some(json) => serde_json::to_string_pretty(json).unwrap_or_default(),
        None => String::new(),
    };

    let rando_version_string = match &rando_version_json {
        Some(json) => serde_json::to_string_pretty(json).unwrap_or_default(),
        None => String::new(),
    };

    let can_manage = if let Some(ref me) = me {
        can_manage_series(&mut transaction, me, event.series).await?
    } else {
        false
    };
    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if me.is_some() {
        if can_manage {
            let mut errors = ctx.errors().collect_vec();
            let all_events = sqlx::query!(
                r#"SELECT series AS "series: crate::series::Series", event, display_name FROM events ORDER BY series, event"#
            ).fetch_all(&mut *transaction).await.unwrap_or_default();
            html! {
                article {
                    h2 : "Event Setup";

                    : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                        h3 : "Basic Event Information";

                        : form_field("display_name", &mut errors, html! {
                            label(for = "display_name") : "Display Name";
                            input(type = "text", id = "display_name", name = "display_name", value = ctx.field_value("display_name").unwrap_or(&event.display_name), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("start", &mut errors, html! {
                            : help::label("start", "Start Time");
                            input(type = "datetime-local", id = "start", name = "start", value = ctx.field_value("start").unwrap_or(
                                &event.start(&mut transaction).await?.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("end", &mut errors, html! {
                            : help::label("end", "End Time");
                            input(type = "datetime-local", id = "end", name = "end", value = ctx.field_value("end").unwrap_or(
                                &event.end.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("url", &mut errors, html! {
                            : help::label("url", "Event URL (start.gg/Challonge)");
                            input(type = "url", id = "url", name = "url", value = ctx.field_value("url").unwrap_or(
                                &event.url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("video_url", &mut errors, html! {
                            label(for = "video_url") : "Video URL";
                            input(type = "url", id = "video_url", name = "video_url", value = ctx.field_value("video_url").unwrap_or(
                                &event.video_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_invite_url", &mut errors, html! {
                            : help::label("discord_invite_url", "Discord Invite URL");
                            input(type = "url", id = "discord_invite_url", name = "discord_invite_url", value = ctx.field_value("discord_invite_url").unwrap_or(
                                &event.discord_invite_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_guild", &mut errors, html! {
                            : help::label("discord_guild", "Discord Guild ID");
                            input(type = "text", id = "discord_guild", name = "discord_guild", value = ctx.field_value("discord_guild").unwrap_or(
                                &event.discord_guild.map(|g| g.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_race_room_channel", &mut errors, html! {
                            : help::label("discord_race_room_channel", "Discord Race Room Channel ID");
                            input(type = "text", id = "discord_race_room_channel", name = "discord_race_room_channel", value = ctx.field_value("discord_race_room_channel").unwrap_or(
                                &event.discord_race_room_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_race_results_channel", &mut errors, html! {
                            : help::label("discord_race_results_channel", "Discord Race Results Channel ID");
                            input(type = "text", id = "discord_race_results_channel", name = "discord_race_results_channel", value = ctx.field_value("discord_race_results_channel").unwrap_or(
                                &event.discord_race_results_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_volunteer_info_channel", &mut errors, html! {
                            : help::label("discord_volunteer_info_channel", "Discord Volunteer Info Channel ID");
                            input(type = "text", id = "discord_volunteer_info_channel", name = "discord_volunteer_info_channel", value = ctx.field_value("discord_volunteer_info_channel").unwrap_or(
                                &event.discord_volunteer_info_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_organizer_channel", &mut errors, html! {
                            : help::label("discord_organizer_channel", "Discord Organizer Channel ID");
                            input(type = "text", id = "discord_organizer_channel", name = "discord_organizer_channel", value = ctx.field_value("discord_organizer_channel").unwrap_or(
                                &event.discord_organizer_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_scheduling_channel", &mut errors, html! {
                            : help::label("discord_scheduling_channel", "Discord Scheduling Channel ID");
                            input(type = "text", id = "discord_scheduling_channel", name = "discord_scheduling_channel", value = ctx.field_value("discord_scheduling_channel").unwrap_or(
                                &event.discord_scheduling_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_async_channel", &mut errors, html! {
                            : help::label("discord_async_channel", "Discord Async Channel ID");
                            input(type = "text", id = "discord_async_channel", name = "discord_async_channel", value = ctx.field_value("discord_async_channel").unwrap_or(
                                &event.discord_async_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_participant_role", &mut errors, html! {
                            : help::label("discord_participant_role", "Discord Participant Role ID");
                            input(type = "text", id = "discord_participant_role", name = "discord_participant_role", value = ctx.field_value("discord_participant_role").unwrap_or(
                                &participant_role_id.map(|id| id.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : "(Role assigned to players when they enter this event)";
                        });

                        : form_field("short_name", &mut errors, html! {
                            label(for = "short_name") : "Short Name";
                            input(type = "text", id = "short_name", name = "short_name", value = ctx.field_value("short_name").unwrap_or(
                                &event.short_name.clone().unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Used in compact displays)";
                        });

                        : form_field("listed", &mut errors, html! {
                            input(type = "checkbox", id = "listed", name = "listed", checked? = ctx.field_value("listed").map_or(event.listed, |value| value == "on"));
                            : help::label("listed", "Listed");
                            label(class = "help") : " (Show this event on the main page)";
                        });

                        : form_field("emulator_settings_reminder", &mut errors, html! {
                            input(type = "checkbox", id = "emulator_settings_reminder", name = "emulator_settings_reminder", checked? = ctx.field_value("emulator_settings_reminder").map_or(event.emulator_settings_reminder, |value| value == "on"));
                            : help::label("emulator_settings_reminder", "Emulator Settings Reminder");
                        });

                        : form_field("prevent_late_joins", &mut errors, html! {
                            input(type = "checkbox", id = "prevent_late_joins", name = "prevent_late_joins", checked? = ctx.field_value("prevent_late_joins").map_or(event.prevent_late_joins, |value| value == "on"));
                            : help::label("prevent_late_joins", "Prevent Late Joins");
                            label(class = "help") : " (Block joining races after they start)";
                        });

                        : form_field("fpa_enabled", &mut errors, html! {
                            input(type = "checkbox", id = "fpa_enabled", name = "fpa_enabled", checked? = ctx.field_value("fpa_enabled").map_or(event.fpa_enabled, |value| value == "on"));
                            : help::label("fpa_enabled", "FPA Enabled");
                            label(class = "help") : " (Announce fair play agreement when official race rooms open)";
                        });

                        : form_field("auto_start_with_restream", &mut errors, html! {
                            input(type = "checkbox", id = "auto_start_with_restream", name = "auto_start_with_restream", checked? = ctx.field_value("auto_start_with_restream").map_or(event.auto_start_with_restream, |value| value == "on"));
                            : help::label("auto_start_with_restream", "Auto-start races with restreams");
                            label(class = "help") : " (Restreamers can use !restream when they arrive to disable auto-start until the restream is ready)";
                        });

                        h3 : "Additional Settings";

                        : form_field("enter_url", &mut errors, html! {
                            : help::label("enter_url", "Enter URL");
                            input(type = "url", id = "enter_url", name = "enter_url", value = ctx.field_value("enter_url").unwrap_or(
                                &event.enter_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (URL for signing up/entering the event)";
                        });

                        : form_field("teams_url", &mut errors, html! {
                            : help::label("teams_url", "Teams URL");
                            input(type = "url", id = "teams_url", name = "teams_url", value = ctx.field_value("teams_url").unwrap_or(
                                &event.teams_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (External URL for teams information)";
                        });

                        : form_field("challonge_community", &mut errors, html! {
                            : help::label("challonge_community", "Challonge Community");
                            input(type = "text", id = "challonge_community", name = "challonge_community", value = ctx.field_value("challonge_community").unwrap_or(
                                &event.challonge_community.clone().unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("team_config", &mut errors, html! {
                            : help::label("team_config", "Team Configuration");
                            select(id = "team_config", name = "team_config", style = "width: 100%; max-width: 600px;") {
                                option(value = "solo", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::Solo), |v| v == "solo")) : "Solo";
                                option(value = "coop", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::CoOp), |v| v == "coop")) : "Co-op";
                                option(value = "tfbcoop", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::TfbCoOp), |v| v == "tfbcoop")) : "TFB Co-op";
                                option(value = "pictionary", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::Pictionary), |v| v == "pictionary")) : "Pictionary";
                                option(value = "multiworld", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::Multiworld), |v| v == "multiworld")) : "Multiworld";
                            }
                        });

                        : form_field("language", &mut errors, html! {
                            : help::label("language", "Language");
                            select(id = "language", name = "language", style = "width: 100%; max-width: 600px;") {
                                option(value = "en", selected? = ctx.field_value("language").map_or(event.language == English, |v| v == "en")) : "English";
                                option(value = "fr", selected? = ctx.field_value("language").map_or(event.language == French, |v| v == "fr")) : "French";
                                option(value = "de", selected? = ctx.field_value("language").map_or(event.language == German, |v| v == "de")) : "German";
                                option(value = "pt", selected? = ctx.field_value("language").map_or(event.language == Portuguese, |v| v == "pt")) : "Portuguese";
                            }
                        });

                        : form_field("default_game_count", &mut errors, html! {
                            : help::label("default_game_count", "Default Game Count");
                            input(type = "number", id = "default_game_count", name = "default_game_count", min = "1", value = ctx.field_value("default_game_count").unwrap_or(&event.default_game_count.to_string()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("open_stream_delay", &mut errors, html! {
                            : help::label("open_stream_delay", "Open Stream Delay");
                            input(type = "text", id = "open_stream_delay", name = "open_stream_delay", value = ctx.field_value("open_stream_delay").unwrap_or(&unparse_duration(event.open_stream_delay)), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Format: '15s')";
                        });
                        : form_field("live_room_open_minutes_before", &mut errors, html! {
                            : help::label("live_room_open_minutes_before", "Open live rooms (minutes before start)");
                            input(type = "number", id = "live_room_open_minutes_before", name = "live_room_open_minutes_before", min = "15", max = "60", required, value = ctx.field_value("live_room_open_minutes_before").unwrap_or(&event.live_room_open_minutes_before.to_string()));
                        });
                        : form_field("async_room_open_minutes_before", &mut errors, html! {
                            : help::label("async_room_open_minutes_before", "Open async rooms (minutes before start)");
                            input(type = "number", id = "async_room_open_minutes_before", name = "async_room_open_minutes_before", min = "15", max = "60", required, value = ctx.field_value("async_room_open_minutes_before").unwrap_or(&event.async_room_open_minutes_before.to_string()));
                        });

                        : form_field("invitational_stream_delay", &mut errors, html! {
                            : help::label("invitational_stream_delay", "Invitational Stream Delay");
                            input(type = "text", id = "invitational_stream_delay", name = "invitational_stream_delay", value = ctx.field_value("invitational_stream_delay").unwrap_or(&unparse_duration(event.invitational_stream_delay)), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Format: '30s')";
                        });

                        : form_field("hide_teams_tab", &mut errors, html! {
                            input(type = "checkbox", id = "hide_teams_tab", name = "hide_teams_tab", checked? = ctx.field_value("hide_teams_tab").map_or(event.hide_teams_tab, |value| value == "on"));
                            : help::label("hide_teams_tab", "Hide Teams Tab");
                        });

                        : form_field("hide_races_tab", &mut errors, html! {
                            input(type = "checkbox", id = "hide_races_tab", name = "hide_races_tab", checked? = ctx.field_value("hide_races_tab").map_or(event.hide_races_tab, |value| value == "on"));
                            : help::label("hide_races_tab", "Hide Races Tab");
                        });

                        : form_field("show_qualifier_times", &mut errors, html! {
                            input(type = "checkbox", id = "show_qualifier_times", name = "show_qualifier_times", checked? = ctx.field_value("show_qualifier_times").map_or(event.show_qualifier_times, |value| value == "on"));
                            : help::label("show_qualifier_times", "Show Qualifier Times");
                        });

                        : form_field("swiss_standings", &mut errors, html! {
                            input(type = "checkbox", id = "swiss_standings", name = "swiss_standings", checked? = ctx.field_value("swiss_standings").map_or(event.swiss_standings, |value| value == "on"));
                            : help::label("swiss_standings", "Show Swiss Standings Tab");
                        });

                        : form_field("automated_asyncs", &mut errors, html! {
                            input(type = "checkbox", id = "automated_asyncs", name = "automated_asyncs", checked? = ctx.field_value("automated_asyncs").map_or(event.automated_asyncs, |value| value == "on"));
                            : help::label("automated_asyncs", "Use automated Discord threads for qualifier asyncs");
                            label(class = "help") : " (When enabled, qualifier requests create private Discord threads with READY/countdown/FINISH buttons)";
                        });

                        : form_field("async_start_delay", &mut errors, html! {
                            : help::label("async_start_delay", "Force-Start Delay (minutes)");
                            input(type = "number", id = "async_start_delay", name = "async_start_delay", min = "0", value = ctx.field_value("async_start_delay").unwrap_or(
                                &event.async_start_delay.map(|d| d.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 200px;");
                            label(class = "help") : " (After seed is distributed, auto-start after this many minutes. Leave empty or set 0 to disable.)";
                        });

                        : form_field("show_opt_out", &mut errors, html! {
                            input(type = "checkbox", id = "show_opt_out", name = "show_opt_out", checked? = ctx.field_value("show_opt_out").map_or(event.show_opt_out, |value| value == "on"));
                            : help::label("show_opt_out", "Show Opt-Out");
                        });

                        : form_field("force_custom_role_binding", &mut errors, html! {
                            input(type = "checkbox", id = "force_custom_role_binding", name = "force_custom_role_binding", checked? = ctx.field_value("force_custom_role_binding").map_or(event.force_custom_role_binding, |value| value == "on"));
                            : help::label("force_custom_role_binding", "Use event-specific volunteer roles");
                            label(class = "help") : " (When enabled, uses event-specific role bindings. When disabled, uses game-level volunteer roles.)";
                        });

                        h3 : "Racetime Bot Configuration";

                        : form_field("racetime_goal_slug", &mut errors, html! {
                            : help::label("racetime_goal_slug", "Goal Slug");
                            input(type = "text", id = "racetime_goal_slug", name = "racetime_goal_slug", value = ctx.field_value("racetime_goal_slug").unwrap_or_else(|| event.racetime_goal_slug.as_deref().unwrap_or("")), style = "width: 100%; max-width: 600px;", placeholder = "Exact goal string on racetime.gg (empty = no goal)");
                        });

                        : form_field("is_custom_goal", &mut errors, html! {
                            input(type = "checkbox", id = "is_custom_goal", name = "is_custom_goal", checked? = ctx.field_value("is_custom_goal").map_or(event.is_custom_goal, |value| value == "on"));
                            : help::label("is_custom_goal", "Is Custom Goal");
                            label(class = "help") : " (When enabled, the racetime.gg goal is a custom goal rather than a standard one.)";
                        });

                        : form_field("draft_kind", &mut errors, html! {
                            : help::label("draft_kind", "Draft Kind");
                            select(id = "draft_kind", name = "draft_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("draft_kind").map_or(event.draft_kind_str.is_none(), |v| v.is_empty())) : "None";
                                @for (slug, label) in &[
                                    ("s7", "S7"),
                                    ("multiworld_s3", "Multiworld S3"),
                                    ("multiworld_s4", "Multiworld S4"),
                                    ("multiworld_s5", "Multiworld S5"),
                                    ("rsl_s7", "RSL S7"),
                                    ("tournoifranco_s3", "Tournoi Franco S3"),
                                    ("tournoifranco_s4", "Tournoi Franco S4"),
                                    ("tournoifranco_s5", "Tournoi Franco S5"),
                                    ("ban_pick", "Ban/Pick (generic, needs config)"),
                                    ("ban_only", "Ban Only (generic, needs config)"),
                                    ("pick_only", "Pick Only (generic, needs config)"),
                                ] {
                                    option(value = slug, selected? = ctx.field_value("draft_kind").map_or(event.draft_kind_str.as_deref() == Some(slug), |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("draft_config", &mut errors, html! {
                            : help::label("draft_config", "Draft Config JSON");
                            textarea(id = "draft_config", name = "draft_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("draft_config").unwrap_or(&draft_config_string);
                            }
                            label(class = "help") : " (JSON configuration for generic draft modes. Leave empty if not applicable.)";
                        });

                        : form_field("qualifier_mode", &mut errors, html! {
                            : help::label("qualifier_mode", "Qualification method");
                            select(id = "qualifier_mode", name = "qualifier_mode") {
                                @for (slug, label) in [("none", "No qualification"), ("rank", "Stored qualifier ranks"), ("single", "Single async qualifier"), ("score", "Configured scoring"), ("pooled_by_mode", "Pooled by mode")] {
                                    option(value = slug, selected? = ctx.field_value("qualifier_mode").unwrap_or(&event.qualifier_mode) == slug) : label;
                                }
                            }
                            label(class = "help") : "Choose how entrants qualify. For Stored qualifier ranks, assign entrant ranks on the Qualifiers page after creating the event. Ranks and async submissions only affect qualification when their method is selected. Configured scoring combines live qualifier races and qualifier async results.";
                        });

                        : form_field("qualifier_score_kind", &mut errors, html! {
                            : help::label("qualifier_score_kind", "Qualifier Score Kind");
                            select(id = "qualifier_score_kind", name = "qualifier_score_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("qualifier_score_kind").map_or(event.qualifier_score_kind_str.is_none(), |v| v.is_empty())) : "None";
                                @for (slug, label) in &[
                                    ("time_relative", "Time relative to par (configurable)"),
                                    ("standard", "Standard"),
                                    ("sgl_2023_online", "SGL 2023 Online"),
                                    ("sgl_2024_online", "SGL 2024 Online"),
                                    ("sgl_2025_online", "SGL 2025 Online"),
                                    ("twwr_miniblins26", "TWWR Miniblins 26"),
                                    ("twwr_main", "TWWR Main"),
                                ] {
                                    option(value = slug, selected? = ctx.field_value("qualifier_score_kind").map_or(event.qualifier_score_kind_str.as_deref() == Some(slug), |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("qualifier_score_config", &mut errors, html! {
                            : help::label("qualifier_score_config", "Qualifier scoring parameters (JSON)");
                            textarea(id = "qualifier_score_config", name = "qualifier_score_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("qualifier_score_config").map(str::to_owned).unwrap_or_else(|| event.qualifier_score_config.as_ref().map(|v| serde_json::to_string_pretty(v).unwrap_or_default()).unwrap_or_default());
                            }
                            label(class = "help") : "Leave empty to preserve the selected scoring strategy's defaults. Example: {\"par_finishers\":4,\"required_finishes\":3,\"counted_attempts\":6,\"best_results\":3}. Optional formula fields: scale, offset, minimum, maximum, rounding (none/floor/nearest), aggregation (sum/average). Average divides by best_results, counting missing results as zero.";
                        });

                        : form_field("is_single_race", &mut errors, html! {
                            input(type = "checkbox", id = "is_single_race", name = "is_single_race", checked? = ctx.field_value("is_single_race").map_or(event.is_single_race, |value| value == "on"));
                            : help::label("is_single_race", "Single Race Event");
                        });

                        : form_field("hide_entrants", &mut errors, html! {
                            input(type = "checkbox", id = "hide_entrants", name = "hide_entrants", checked? = ctx.field_value("hide_entrants").map_or(event.hide_entrants, |value| value == "on"));
                            : help::label("hide_entrants", "Hide Entrants");
                        });

                        : form_field("start_delay", &mut errors, html! {
                            : help::label("start_delay", "Start Delay (seconds)");
                            input(type = "number", id = "start_delay", name = "start_delay", value = ctx.field_value("start_delay").unwrap_or(&event.start_delay.to_string()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("start_delay_open", &mut errors, html! {
                            : help::label("start_delay_open", "Start Delay Open (seconds)");
                            input(type = "text", id = "start_delay_open", name = "start_delay_open", value = ctx.field_value("start_delay_open").unwrap_or(
                                &event.start_delay_open.map(|d| d.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Leave empty to use same as Start Delay)";
                        });

                        : form_field("restrict_chat_in_qualifiers", &mut errors, html! {
                            input(type = "checkbox", id = "restrict_chat_in_qualifiers", name = "restrict_chat_in_qualifiers", checked? = ctx.field_value("restrict_chat_in_qualifiers").map_or(event.restrict_chat_in_qualifiers, |value| value == "on"));
                            : help::label("restrict_chat_in_qualifiers", "Restrict Chat in Qualifiers");
                        });

                        : form_field("preroll_mode", &mut errors, html! {
                            : help::label("preroll_mode", "Preroll Mode");
                            select(id = "preroll_mode", name = "preroll_mode", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("none", "None"), ("short", "Short"), ("medium", "Medium"), ("long", "Long")] {
                                    option(value = val, selected? = ctx.field_value("preroll_mode").map_or(&*event.preroll_mode == *val, |v| v == *val)) : *label;
                                }
                            }
                        });

                        : form_field("spoiler_unlock", &mut errors, html! {
                            : help::label("spoiler_unlock", "Spoiler Log Unlock");
                            select(id = "spoiler_unlock", name = "spoiler_unlock", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("never", "Never"), ("after", "After race"), ("immediately", "Immediately")] {
                                    option(value = val, selected? = ctx.field_value("spoiler_unlock").map_or(&*event.spoiler_unlock == *val, |v| v == *val)) : *label;
                                }
                            }
                        });



                        : form_field("startgg_double_rr", &mut errors, html! {
                            input(type = "checkbox", id = "startgg_double_rr", name = "startgg_double_rr", checked? = ctx.field_value("startgg_double_rr").map_or(event.startgg_double_rr, |value| value == "on"));
                            : help::label("startgg_double_rr", "start.gg double round-robin mode");
                            label(class = "help") : " (When enabled with a start.gg round-robin best-of-1 bracket, HTH schedules 2 games per set and force-closes the start.gg set after both are played.)";
                        });

                        : form_field("is_live_event", &mut errors, html! {
                            input(type = "checkbox", id = "is_live_event", name = "is_live_event", checked? = ctx.field_value("is_live_event").map_or(event.is_live_event, |value| value == "on"));
                            : help::label("is_live_event", "Is Live Event");
                            label(class = "help") : " (In-person event: scheduled races after the event starts send notifications instead of creating racetime.gg rooms.)";
                        });

                        h3 : "Seed Generation";

                        : form_field("rando_version_json", &mut errors, html! {
                            : help::label("rando_version_json", "Randomizer Version (JSON)");
                            textarea(id = "rando_version_json", name = "rando_version_json", rows = "8", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("rando_version_json").unwrap_or(&rando_version_string);
                            }
                            p(class = "help") : "Randomizer version config as JSON. Leave empty to clear.";
                            details {
                                summary : "Examples";
                                pre(style = "font-size: 13px; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                    : "// TWW randomizer build:\n{\n  \"type\": \"tww\",\n  \"identifier\": \"dev_tanjo3.1.10.5\",\n  \"githubUrl\": \"https://github.com/tanjo3/wwrando/releases/tag/dev_tanjo3.1.10.5\",\n  \"trackerLink\": \"wooferzfg.me/tww-rando-tracker/miniblins\"\n}\n\n// OoTR pinned version:\n{ \"type\": \"pinned\", \"version\": \"8.3.16 f.1\" }\n\n// OoTR latest branch:\n{ \"type\": \"latest\", \"branch\": \"dev\" }";
                                }
                            }
                        });

                        : form_field("seed_gen_type", &mut errors, html! {
                            : help::label("seed_gen_type", "Seed Gen Type");
                            select(id = "seed_gen_type", name = "seed_gen_type", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("seed_gen_type").map_or(seed_gen_type_str.is_none(), |v| v.is_empty())) : "None (manual / external)";
                                @for (val, label) in &[
                                    ("alttpr_dr", "ALTTPR Door Rando (stable)"),
                                    ("alttpr_dr_latest", "ALTTPR Door Rando (latest)"),
                                    ("alttpr_avianart", "ALTTPR Avianart"),
                                    ("owr", "ALTTPR OWR (regular build)"),
                                    ("owr_tourney", "ALTTPR OWR (tournament build)"),
                                    ("ootr", "OoTR"),
                                    ("ootr_tfb", "OoTR Triforce Blitz"),
                                    ("ootr_rsl", "OoTR RSL"),
                                    ("twwr", "The Wind Waker Randomizer"),
                                    ("mmr", "MMR"),
                                ] {
                                    option(value = val, selected? = ctx.field_value("seed_gen_type").map_or(seed_gen_type_str.as_deref() == Some(val), |v| v == *val)) : *label;
                                }
                            }
                            label(class = "help") : "Choose OWR (regular build) for /opt/owr or OWR (tournament build) for /opt/owr_tourney. The selection applies to this event's live, async and practice seeds. Both accept the same JSON structure; use settings supported by the installed build. Door Rando (stable) keeps the existing installation; Door Rando (latest) uses /opt/alttpr_latest with the same source and settings options.";
                            p(class = "help") : "For pooled qualifiers, configure each mode's generator and baseline settings on the Qualifiers page after creating the event. Existing pooled OWR modes also use the tournament build. A branch name in Seed Config JSON does not switch installations.";
                        });

                        : form_field("choice_resolution", &mut errors, html! {
                            label(for = "choice_resolution") : "Resolve random player choices";
                            select(id = "choice_resolution", name = "choice_resolution") {
                                @for (value, label) in [("race_creation", "On race creation / import"), ("room_opening", "On room opening"), ("seed_rolling", "On seed reveal")] {
                                    option(value = value, selected? = ctx.field_value("choice_resolution").unwrap_or(seed_config_json.as_ref().and_then(|config| config.get("choice_resolution")).and_then(|value| value.as_str()).unwrap_or("seed_rolling")) == value) : label;
                                }
                            }
                            p(class = "help") : "For OWR and Door Rando mutual choices. Each game's result is saved and reused for all rooms and seed rerolls. Scheduling threads always show agreed settings and rules; only creation/import also reveals random results there. Room opening posts in each race room or private async thread; without a room it falls back to seed reveal. Seed reveal posts settings beside the seed, separately for each async participant. This selector sets choice_resolution in Seed Config JSON; changing it affects only unresolved races.";
                        });

                        : guides::seed_config();
                        : form_field("seed_config", &mut errors, html! {
                            : help::label("seed_config", "Seed Config JSON");
                            textarea(id = "seed_config", name = "seed_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("seed_config").unwrap_or(&seed_config_string);
                            }
                            label(class = "help") : " (JSON config for the seed gen type. Leave empty if not applicable.)";
                        });
                    }, errors.clone(), "Save Basic Info");

                    h3 : "Enter Flow Configuration";
                    : guides::enter_flow();

                    : full_form(uri!(update_enter_flow(event.series, &*event.event)), csrf, html! {
                        : form_field("enter_flow_json", &mut errors, html! {
                            : help::label("enter_flow_json", "Enter Flow JSON");
                            textarea(id = "enter_flow_json", name = "enter_flow_json", rows = "10", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("enter_flow_json").unwrap_or(&enter_flow_string);
                            }
                            p(class = "help") : "Configure the signup requirements as JSON. Leave empty for no requirements.";

                        });
                    }, errors.clone(), "Save Enter Flow");

                    h3 : "Organizer Management";

                    : full_form(uri!(add_organizer(event.series, &*event.event)), csrf, html! {
                        : form_field("organizer", &mut errors, html! {
                            : help::label("organizer", "Add Organizer");
                            div(class = "autocomplete-container", style = "width: 100%; max-width: 600px;") {
                                input(type = "text", id = "organizer", name = "organizer", autocomplete = "off", style = "width: 100%;");
                                div(id = "organizer-suggestions", class = "suggestions", style = "display: none;") {}
                            }
                        });
                    }, errors.clone(), "Add Organizer");

                    h3 : "Current Organizers";
                    @if let Ok(organizers) = event.organizers(&mut transaction).await {
                        @if organizers.is_empty() {
                            p : "No organizers assigned.";
                        } else {
                            ul {
                                @for organizer in organizers {
                                    li {
                                        : organizer;
                                        : " ";
                                        form(method = "post", action = uri!(remove_organizer(event.series, &*event.event, organizer.id))) {
                                            input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|t| t.authenticity_token().to_string()).unwrap_or_default());
                                            button(type = "submit", onclick = "return confirm('Remove this organizer?')") : "Remove";
                                        }
                                    }
                                }
                            }
                        }
                    }

                    h3 : "Copy organizers from another event";
                    : full_form(uri!(copy_organizers(event.series, &*event.event)), csrf, html! {
                        : form_field("source_event", &mut errors, html! {
                            : help::label("copy_source_event", "Copy organizers from:");
                            select(id = "copy_source_event", name = "source_event") {
                                option(value = "") : "-- Select event --";
                                @for ev in &all_events {
                                    @if !(ev.series == event.series && ev.event == *event.event) {
                                        option(value = format!("{}/{}", ev.series.slug(), ev.event)) {
                                            : format!("{} / {}", ev.series.display_name(), ev.display_name);
                                        }
                                    }
                                }
                            }
                            label(class = "help") : "All organizers from the selected event will be added to this event. Existing organizers are kept.";
                        });
                    }, errors.clone(), "Copy Organizers");
                }
            }
        } else {
            html! {
                article {
                    p : "You must be a global admin or an admin for this event's game to access this page.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(get(event.series, &*event.event)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to access this page.";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        &uri,
        PageStyle {
            chests: event.chests().await?,
            ..PageStyle::default()
        },
        &format!("Setup — {}", event.display_name),
        html! {
            : header;
            : content;
            script(src = static_url!("user-search.js")) {}
            script(src = static_url!("setting-help.js")) {}
        },
    )
    .await?)
}

#[rocket::get("/event/<series>/<event>/setup")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(setup_form(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        event_data,
        Context::default(),
    )
    .await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct SetupForm {
    #[field(default = String::new())]
    csrf: String,
    display_name: String,
    short_name: Option<String>,
    start: Option<String>,
    end: Option<String>,
    url: Option<String>,
    video_url: Option<String>,
    discord_invite_url: Option<String>,
    discord_guild: Option<String>,
    discord_race_room_channel: Option<String>,
    discord_race_results_channel: Option<String>,
    discord_volunteer_info_channel: Option<String>,
    discord_organizer_channel: Option<String>,
    discord_scheduling_channel: Option<String>,
    discord_async_channel: Option<String>,
    discord_participant_role: Option<String>,
    listed: bool,
    emulator_settings_reminder: bool,
    prevent_late_joins: bool,
    fpa_enabled: bool,
    auto_start_with_restream: bool,
    rando_version_json: Option<String>,
    enter_url: Option<String>,
    teams_url: Option<String>,
    challonge_community: Option<String>,
    team_config: String,
    language: String,
    default_game_count: i16,
    open_stream_delay: String,
    #[field(default = 30, validate = range(15..=60))]
    live_room_open_minutes_before: i16,
    #[field(default = 30, validate = range(15..=60))]
    async_room_open_minutes_before: i16,
    invitational_stream_delay: String,
    hide_teams_tab: bool,
    hide_races_tab: bool,
    show_qualifier_times: bool,
    swiss_standings: bool,
    automated_asyncs: bool,
    async_start_delay: Option<i32>,
    show_opt_out: bool,
    force_custom_role_binding: bool,
    racetime_goal_slug: Option<String>,
    draft_kind: Option<String>,
    draft_config: Option<String>,
    qualifier_mode: String,
    qualifier_score_kind: Option<String>,
    qualifier_score_config: Option<String>,
    is_single_race: bool,
    hide_entrants: bool,
    #[field(default = 15)]
    start_delay: i32,
    start_delay_open: Option<String>,
    restrict_chat_in_qualifiers: bool,
    #[field(default = String::from("medium"))]
    preroll_mode: String,
    #[field(default = String::from("after"))]
    spoiler_unlock: String,
    is_custom_goal: bool,
    startgg_double_rr: bool,
    is_live_event: bool,
    seed_gen_type: Option<String>,
    seed_config: Option<String>,
    choice_resolution: Option<String>,
}

#[rocket::post("/event/<series>/<event>/setup", data = "<form>")]
pub(crate) async fn post(
    pool: &State<PgPool>,
    discord_ctx: &State<RwFuture<DiscordCtx>>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, SetupForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !can_manage_series(&mut transaction, &me, event_data.series).await? {
            form.context.push_error(form::Error::validation(
                "You must be a global admin or an admin for this event's game to configure this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                setup_form(
                    transaction,
                    Some(me),
                    uri,
                    csrf.as_ref(),
                    event_data,
                    form.context,
                )
                .await?,
            )
        } else {
            // Parse start time (datetime-local sends YYYY-MM-DDTHH:MM format)
            let start = if let Some(start_str) = &value.start {
                if !start_str.is_empty() {
                    match NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => {
                            Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc))
                        }
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid start time format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Parse end time (datetime-local sends YYYY-MM-DDTHH:MM format)
            let end = if let Some(end_str) = &value.end {
                if !end_str.is_empty() {
                    match NaiveDateTime::parse_from_str(end_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => {
                            Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc))
                        }
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid end time format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Parse URLs
            let url = if let Some(url_str) = &value.url {
                if !url_str.is_empty() {
                    match url_str.parse::<Url>() {
                        Ok(u) => Some(u),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid URL format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let video_url = if let Some(video_url_str) = &value.video_url {
                if !video_url_str.is_empty() {
                    match video_url_str.parse::<Url>() {
                        Ok(u) => Some(u),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid video URL format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_invite_url = if let Some(discord_invite_url_str) = &value.discord_invite_url
            {
                if !discord_invite_url_str.is_empty() {
                    match discord_invite_url_str.parse::<Url>() {
                        Ok(u) => Some(u),
                        Err(_) => {
                            form.context.push_error(form::Error::validation(
                                "Invalid Discord invite URL format",
                            ));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Parse Discord IDs
            let discord_guild = if let Some(guild_str) = &value.discord_guild {
                if !guild_str.is_empty() {
                    match guild_str.parse::<u64>() {
                        Ok(id) => Some(GuildId::new(id)),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid Discord guild ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_race_room_channel = if let Some(channel_str) =
                &value.discord_race_room_channel
            {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid Discord channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_race_results_channel = if let Some(channel_str) =
                &value.discord_race_results_channel
            {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid Discord channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_volunteer_info_channel = if let Some(channel_str) =
                &value.discord_volunteer_info_channel
            {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid Discord channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_organizer_channel =
                if let Some(channel_str) = &value.discord_organizer_channel {
                    if !channel_str.is_empty() {
                        match channel_str.parse::<u64>() {
                            Ok(id) => Some(ChannelId::new(id)),
                            Err(_) => {
                                form.context.push_error(form::Error::validation(
                                    "Invalid Discord organizer channel ID",
                                ));
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

            let discord_scheduling_channel =
                if let Some(channel_str) = &value.discord_scheduling_channel {
                    if !channel_str.is_empty() {
                        match channel_str.parse::<u64>() {
                            Ok(id) => Some(ChannelId::new(id)),
                            Err(_) => {
                                form.context.push_error(form::Error::validation(
                                    "Invalid Discord scheduling channel ID",
                                ));
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

            let discord_async_channel = if let Some(channel_str) = &value.discord_async_channel {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation(
                                "Invalid Discord async channel ID",
                            ));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_participant_role = if let Some(role_str) = &value.discord_participant_role {
                if !role_str.is_empty() {
                    match role_str.parse::<u64>() {
                        Ok(id) => Some(RoleId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation(
                                "Invalid Discord participant role ID",
                            ));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Handle optional string fields (empty string -> None)
            let short_name = value
                .short_name
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let challonge_community = value
                .challonge_community
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

            // Parse new racetime bot config fields
            let racetime_goal_slug = value
                .racetime_goal_slug
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let draft_kind = value
                .draft_kind
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let qualifier_score_kind = value
                .qualifier_score_kind
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let seed_gen_type = value
                .seed_gen_type
                .as_ref()
                .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

            let draft_config_json: Option<serde_json::Value> = if let Some(ref dc_str) =
                value.draft_config
            {
                if dc_str.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str(dc_str) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            form.context.push_error(
                                form::Error::validation(format!("Invalid draft config JSON: {e}"))
                                    .with_name("draft_config"),
                            );
                            None
                        }
                    }
                }
            } else {
                None
            };

            let mut seed_config_json: Option<serde_json::Value> = if let Some(ref sc_str) =
                value.seed_config
            {
                if sc_str.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str(sc_str) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            form.context.push_error(
                                form::Error::validation(format!("Invalid seed config JSON: {e}"))
                                    .with_name("seed_config"),
                            );
                            None
                        }
                    }
                }
            } else {
                None
            };

            if let Some(timing) = value.choice_resolution.as_deref() {
                if let Some(config) = seed_config_json.as_mut().and_then(serde_json::Value::as_object_mut) {
                    config.insert("choice_resolution".into(), json!(timing));
                }
            }

            let start_delay_open: Option<i32> = if let Some(ref sdo_str) = value.start_delay_open {
                if sdo_str.trim().is_empty() {
                    None
                } else {
                    match sdo_str.trim().parse::<i32>() {
                        Ok(v) => Some(v),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid start delay open value")
                                    .with_name("start_delay_open"),
                            );
                            None
                        }
                    }
                }
            } else {
                None
            };

            // Parse additional URLs
            let enter_url = if let Some(enter_url_str) = &value.enter_url {
                if !enter_url_str.is_empty() {
                    match enter_url_str.parse::<Url>() {
                        Ok(u) => Some(u),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid enter URL format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let teams_url = if let Some(teams_url_str) = &value.teams_url {
                if !teams_url_str.is_empty() {
                    match teams_url_str.parse::<Url>() {
                        Ok(u) => Some(u),
                        Err(_) => {
                            form.context
                                .push_error(form::Error::validation("Invalid teams URL format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Parse team_config enum
            let team_config = match value.team_config.as_str() {
                "solo" => TeamConfig::Solo,
                "coop" => TeamConfig::CoOp,
                "tfbcoop" => TeamConfig::TfbCoOp,
                "pictionary" => TeamConfig::Pictionary,
                "multiworld" => TeamConfig::Multiworld,
                _ => {
                    form.context
                        .push_error(form::Error::validation("Invalid team configuration"));
                    TeamConfig::Solo // default fallback
                }
            };

            // Parse language enum
            let language = match value.language.as_str() {
                "en" => English,
                "fr" => French,
                "de" => German,
                "pt" => Portuguese,
                _ => {
                    form.context
                        .push_error(form::Error::validation("Invalid language"));
                    English // default fallback
                }
            };

            // Parse durations
            let open_stream_delay =
                if let Some(time) = parse_duration(&value.open_stream_delay, None) {
                    Some(time)
                } else {
                    form.context.push_error(
                        form::Error::validation(
                            "Duration must be formatted like '1:23:45' or '1h 23m 45s'.",
                        )
                        .with_name("open_stream_delay"),
                    );
                    None
                };

            let invitational_stream_delay =
                if let Some(time) = parse_duration(&value.invitational_stream_delay, None) {
                    Some(time)
                } else {
                    form.context.push_error(
                        form::Error::validation(
                            "Duration must be formatted like '1:23:45' or '1h 23m 45s'.",
                        )
                        .with_name("invitational_stream_delay"),
                    );
                    None
                };

            let rando_version = match value.rando_version_json.as_deref() {
                None | Some("") => Ok(None),
                Some(s) => match serde_json::from_str::<VersionedBranch>(s) {
                    Ok(_) => Ok(Some(
                        serde_json::from_str::<serde_json::Value>(s)
                            .expect("validated randomizer version is valid JSON"),
                    )),
                    Err(error) => {
                        form.context.push_error(
                            form::Error::validation(format!("Invalid randomizer version: {error}"))
                                .with_name("rando_version_json"),
                        );
                        Err(())
                    }
                },
            };

            if let Err(error) = event::configuration::validate_qualification(
                &value.qualifier_mode,
                qualifier_score_kind.as_deref(),
            ) {
                form.context
                    .push_error(form::Error::validation(error).with_name("qualifier_mode"));
            }
            let qualifier_score_config = parse_score_config(
                value.qualifier_score_config.as_deref(),
                qualifier_score_kind.as_deref(),
                &mut form.context,
            );
            if event_data.round_modes.is_some() && draft_kind.is_some()
                && seed_config_json.as_ref().is_some_and(|c| c.get("baselines").is_some())
            {
                form.context.push_error(form::Error::validation("Remove round_modes to use a named-baseline draft.").with_name("draft_config"));
            }
            for (field, result) in [
                (
                    "seed_config",
                    event::configuration::validate_seed(
                        seed_gen_type.as_deref(),
                        seed_config_json.as_ref(),
                    ),
                ),
                (
                    "preroll_mode",
                    event::configuration::validate_seed_policies(
                        &value.preroll_mode,
                        &value.spoiler_unlock,
                        seed_gen_type.as_deref(),
                    ),
                ),
                (
                    "draft_config",
                    event::configuration::validate_draft(
                        draft_kind.as_deref(),
                        draft_config_json.as_ref(),
                        seed_gen_type.as_deref(),
                        seed_config_json.as_ref(),
                        Some(value.default_game_count),
                    ),
                ),
                (
                    "start_delay",
                    event::configuration::validate_start_delay(value.start_delay),
                ),
                (
                    "start_delay_open",
                    start_delay_open
                        .map(event::configuration::validate_start_delay)
                        .unwrap_or(Ok(())),
                ),
            ] {
                if let Err(error) = result {
                    form.context
                        .push_error(form::Error::validation(error).with_name(field));
                }
            }

            if form.context.errors().next().is_some() {
                return Ok(RedirectOrContent::Content(
                    setup_form(
                        transaction,
                        Some(me),
                        uri,
                        csrf.as_ref(),
                        event_data,
                        form.context,
                    )
                    .await?,
                ));
            }

            // Update database
            sqlx::query!(r#"
                UPDATE events
                SET display_name = $1, start = $2, end_time = $3, url = $4, video_url = $5,
                    discord_invite_url = $6, discord_guild = $7, discord_race_room_channel = $8,
                    discord_race_results_channel = $9, discord_volunteer_info_channel = $10,
                    discord_organizer_channel = $11, discord_scheduling_channel = $12,
                    discord_async_channel = $13, short_name = $14,
                    listed = $15, emulator_settings_reminder = $16,
                    prevent_late_joins = $17, enter_url = $18, teams_url = $19,
                    challonge_community = $20, team_config = $21, language = $22,
                    default_game_count = $23, open_stream_delay = $24, invitational_stream_delay = $25,
                    hide_teams_tab = $26, hide_races_tab = $27, show_qualifier_times = $28,
                    automated_asyncs = $29, show_opt_out = $30, force_custom_role_binding = $31,
                    racetime_goal_slug = $34, draft_kind = $35, draft_config = $36,
                    qualifier_score_kind = $37, is_single_race = $38, hide_entrants = $39,
                    start_delay = $40, start_delay_open = $41, restrict_chat_in_qualifiers = $42,
                    async_start_delay = $43, startgg_double_rr = $44,
                    preroll_mode = $45, spoiler_unlock = $46, is_custom_goal = $47,
                    fpa_enabled = $48, swiss_standings = $49, rando_version = $50,
                    is_live_event = $51, seed_gen_type = $52, seed_config = $53
                WHERE series = $32 AND event = $33
            "#,
                value.display_name,
                start,
                end,
                url.map(|u| u.to_string()),
                video_url.map(|u| u.to_string()),
                discord_invite_url.map(|u| u.to_string()),
                discord_guild.map(|g| g.get() as i64),
                discord_race_room_channel.map(|c| c.get() as i64),
                discord_race_results_channel.map(|c| c.get() as i64),
                discord_volunteer_info_channel.map(|c| c.get() as i64),
                discord_organizer_channel.map(|c| c.get() as i64),
                discord_scheduling_channel.map(|c| c.get() as i64),
                discord_async_channel.map(|c| c.get() as i64),
                short_name,
                value.listed,
                value.emulator_settings_reminder,
                value.prevent_late_joins,
                enter_url.map(|u| u.to_string()),
                teams_url.map(|u| u.to_string()),
                challonge_community,
                team_config as _,
                language as _,
                value.default_game_count,
                open_stream_delay.unwrap() as _,
                invitational_stream_delay.unwrap() as _,
                value.hide_teams_tab,
                value.hide_races_tab,
                value.show_qualifier_times,
                value.automated_asyncs,
                value.show_opt_out,
                value.force_custom_role_binding,
                event_data.series as _,
                &event_data.event,
                racetime_goal_slug,
                draft_kind,
                draft_config_json as _,
                qualifier_score_kind,
                value.is_single_race,
                value.hide_entrants,
                value.start_delay,
                start_delay_open,
                value.restrict_chat_in_qualifiers,
                value.async_start_delay,
                value.startgg_double_rr,
                &value.preroll_mode,
                &value.spoiler_unlock,
                value.is_custom_goal,
                value.fpa_enabled,
                value.swiss_standings,
                rando_version.unwrap(),
                value.is_live_event,
                seed_gen_type,
                seed_config_json as _,
            ).execute(&mut *transaction).await?;

            mirror_twwr_permalink(&mut transaction, event_data.series, &event_data.event).await?;
            save_score_config(
                &mut transaction,
                event_data.series,
                &event_data.event,
                qualifier_score_config,
            )
            .await?;
            save_qualifier_mode(
                &mut transaction,
                event_data.series,
                &event_data.event,
                &value.qualifier_mode,
            )
            .await?;

            sqlx::query!(
                "UPDATE events SET auto_start_with_restream = $1 WHERE series = $2 AND event = $3",
                value.auto_start_with_restream,
                event_data.series as _,
                &event_data.event,
            )
            .execute(&mut *transaction)
            .await?;

            sqlx::query!(
                "UPDATE events SET live_room_open_minutes_before = $1, async_room_open_minutes_before = $2 WHERE series = $3 AND event = $4",
                value.live_room_open_minutes_before,
                value.async_room_open_minutes_before,
                event_data.series as _,
                &event_data.event,
            )
            .execute(&mut *transaction)
            .await?;

            let old_participant_role_id: Option<i64> = sqlx::query_scalar!(
                "SELECT id FROM discord_roles WHERE series = $1 AND event = $2 AND role IS NULL AND racetime_team IS NULL",
                event_data.series as _, &event_data.event
            ).fetch_optional(&mut *transaction).await?;
            let new_participant_role_id = discord_participant_role.map(|r| r.get() as i64);
            let participant_role_changed = new_participant_role_id != old_participant_role_id;

            sqlx::query!(
                "DELETE FROM discord_roles WHERE series = $1 AND event = $2 AND role IS NULL AND racetime_team IS NULL",
                event_data.series as _, &event_data.event
            ).execute(&mut *transaction).await?;
            if let (Some(role_id), Some(guild)) = (discord_participant_role, discord_guild) {
                sqlx::query!(
                    "INSERT INTO discord_roles (id, guild, series, event) VALUES ($1, $2, $3, $4)",
                    role_id.get() as i64,
                    guild.get() as i64,
                    event_data.series as _,
                    &event_data.event
                )
                .execute(&mut *transaction)
                .await?;
                if participant_role_changed {
                    let entrant_discord_ids = sqlx::query_scalar!(
                        r#"SELECT DISTINCT u.discord_id AS "discord_id!: PgSnowflake<UserId>"
                    FROM teams t
                    JOIN team_members tm ON tm.team = t.id
                    JOIN users u ON u.id = tm.member
                    WHERE t.series = $1 AND t.event = $2
                    AND tm.status IN ('created', 'confirmed')
                    AND u.discord_id IS NOT NULL"#,
                        event_data.series as _,
                        &event_data.event
                    )
                    .fetch_all(&mut *transaction)
                    .await?;
                    let discord_ctx_for_spawn = discord_ctx.inner().clone();
                    let display_name = event_data.display_name.clone();
                    transaction.commit().await?;
                    tokio::spawn(async move {
                        let discord_ctx = discord_ctx_for_spawn.read().await;
                        let mut failed_role_assignments = Vec::new();
                        for PgSnowflake(discord_id) in entrant_discord_ids {
                            if let Ok(member) = guild.member(&*discord_ctx, discord_id).await {
                                if let Err(e) = member.add_role(&*discord_ctx, role_id).await {
                                    failed_role_assignments.push((discord_id, e));
                                }
                            }
                        }
                        if !failed_role_assignments.is_empty() {
                            let mut msg = MessageBuilder::default();
                            msg.push("Failed to assign participant role ");
                            msg.mention(&role_id);
                            msg.push(" for ");
                            msg.push_safe(&display_name);
                            msg.push(" to:");
                            for (discord_id, e) in &failed_role_assignments {
                                msg.push("\n- ");
                                msg.mention(discord_id);
                                msg.push(": ");
                                msg.push_safe(e.to_string());
                            }
                            let msg = msg.build();
                            if let Ok(ch) = ADMIN_USER.create_dm_channel(&*discord_ctx).await {
                                let _ = ch.say(&*discord_ctx, msg).await;
                            }
                        }
                    });
                    return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(
                        series, event
                    )))));
                }
            }

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            setup_form(
                transaction,
                Some(me),
                uri,
                csrf.as_ref(),
                event_data,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddOrganizerForm {
    #[field(default = String::new())]
    csrf: String,
    organizer: String,
}

#[rocket::post("/event/<series>/<event>/setup/add-organizer", data = "<form>")]
pub(crate) async fn add_organizer(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, AddOrganizerForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !can_manage_series(&mut transaction, &me, event_data.series).await? {
            form.context.push_error(form::Error::validation(
                "You must be a global admin or an admin for this event's game to configure this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                setup_form(
                    transaction,
                    Some(me),
                    uri,
                    csrf.as_ref(),
                    event_data,
                    form.context,
                )
                .await?,
            )
        } else {
            // Find user by ID
            let organizer_id: Id<Users> = value
                .organizer
                .parse()
                .map_err(|_| StatusOrError::Status(Status::NotFound))?;
            let user = sqlx::query!(
                r#"
                SELECT id
                FROM users
                WHERE id = $1
            "#,
                organizer_id as _
            )
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

            // Add organizer
            sqlx::query!(
                r#"
                INSERT INTO organizers (series, event, organizer) 
                VALUES ($1, $2, $3) 
                ON CONFLICT DO NOTHING
            "#,
                event_data.series as _,
                &event_data.event,
                user.id
            )
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            setup_form(
                transaction,
                Some(me),
                uri,
                csrf.as_ref(),
                event_data,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveOrganizerForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post(
    "/event/<series>/<event>/setup/remove-organizer/<organizer>",
    data = "<form>"
)]
pub(crate) async fn remove_organizer(
    pool: &State<PgPool>,
    me: User,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    organizer: Id<Users>,
    form: Form<Contextual<'_, RemoveOrganizerForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if form.value.is_some() {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !can_manage_series(&mut transaction, &me, event_data.series).await? {
            form.context.push_error(form::Error::validation(
                "You must be a global admin or an admin for this event's game to configure this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                setup_form(
                    transaction,
                    Some(me),
                    _uri,
                    csrf.as_ref(),
                    event_data,
                    form.context,
                )
                .await?,
            )
        } else {
            // Remove organizer
            sqlx::query!(
                r#"
                DELETE FROM organizers 
                WHERE series = $1 AND event = $2 AND organizer = $3
            "#,
                event_data.series as _,
                &event_data.event,
                i64::from(organizer)
            )
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(
            setup_form(
                transaction,
                Some(me),
                _uri,
                csrf.as_ref(),
                event_data,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct CopyOrganizersForm {
    #[field(default = String::new())]
    csrf: String,
    source_event: String,
}

#[rocket::post("/event/<series>/<event>/setup/copy-organizers", data = "<form>")]
pub(crate) async fn copy_organizers(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, CopyOrganizersForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured.",
            ));
        }
        if !can_manage_series(&mut transaction, &me, event_data.series).await? {
            form.context.push_error(form::Error::validation(
                "You must be a global admin or an admin for this event's game to configure this event.",
            ));
        }
        let (source_series, source_event_slug) =
            match value.source_event.splitn(2, '/').collect::<Vec<_>>()[..] {
                [s, e] if !s.is_empty() && !e.is_empty() => {
                    let source_series = s
                        .parse::<Series>()
                        .map_err(|()| StatusOrError::Status(Status::BadRequest))?;
                    (source_series, e.to_owned())
                }
                _ => {
                    form.context.push_error(
                        form::Error::validation("Please select a source event.")
                            .with_name("source_event"),
                    );
                    return Ok(RedirectOrContent::Content(
                        setup_form(
                            transaction,
                            Some(me),
                            uri,
                            csrf.as_ref(),
                            event_data,
                            form.context,
                        )
                        .await?,
                    ));
                }
            };
        if form.context.errors().next().is_some() {
            return Ok(RedirectOrContent::Content(
                setup_form(
                    transaction,
                    Some(me),
                    uri,
                    csrf.as_ref(),
                    event_data,
                    form.context,
                )
                .await?,
            ));
        }
        sqlx::query!(
            "INSERT INTO organizers (series, event, organizer) SELECT $1, $2, organizer FROM organizers WHERE series = $3 AND event = $4 ON CONFLICT DO NOTHING",
            event_data.series as _, &event_data.event, source_series as _, &source_event_slug
        ).execute(&mut *transaction).await?;
        transaction.commit().await?;
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
    } else {
        RedirectOrContent::Content(
            setup_form(
                transaction,
                Some(me),
                uri,
                csrf.as_ref(),
                event_data,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct UpdateEnterFlowForm {
    #[field(default = String::new())]
    csrf: String,
    enter_flow_json: String,
}

#[rocket::post("/event/<series>/<event>/setup/update-enter-flow", data = "<form>")]
pub(crate) async fn update_enter_flow(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, UpdateEnterFlowForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation(
                "This event has ended and can no longer be configured",
            ));
        }
        if !can_manage_series(&mut transaction, &me, event_data.series).await? {
            form.context.push_error(form::Error::validation(
                "You must be a global admin or an admin for this event's game to configure this event.",
            ));
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(
                setup_form(
                    transaction,
                    Some(me),
                    uri,
                    csrf.as_ref(),
                    event_data,
                    form.context,
                )
                .await?,
            )
        } else {
            // Parse enter_flow JSON and validate against Flow struct
            let enter_flow_json = if !value.enter_flow_json.trim().is_empty() {
                match serde_json::from_str::<enter::Flow>(&value.enter_flow_json) {
                    Ok(_) => Some(
                        serde_json::from_str::<serde_json::Value>(&value.enter_flow_json)
                            .expect("already validated as JSON"),
                    ),
                    Err(e) => {
                        form.context.push_error(
                            form::Error::validation(format!("Invalid enter flow: {e}"))
                                .with_name("enter_flow_json"),
                        );
                        None
                    }
                }
            } else {
                None
            };

            // Check for validation errors before updating database
            if form.context.errors().next().is_some() {
                RedirectOrContent::Content(
                    setup_form(
                        transaction,
                        Some(me),
                        uri,
                        csrf.as_ref(),
                        event_data,
                        form.context,
                    )
                    .await?,
                )
            } else {
                // Update database
                sqlx::query!(
                    r#"
                    UPDATE events
                    SET enter_flow = $1
                    WHERE series = $2 AND event = $3
                "#,
                    enter_flow_json,
                    event_data.series as _,
                    &event_data.event
                )
                .execute(&mut *transaction)
                .await?;

                transaction.commit().await?;
                RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
            }
        }
    } else {
        RedirectOrContent::Content(
            setup_form(
                transaction,
                Some(me),
                uri,
                csrf.as_ref(),
                event_data,
                form.context,
            )
            .await?,
        )
    })
}

#[rocket::get("/event/setup/search-users?<query>")]
pub(crate) async fn search_users(
    pool: &State<PgPool>,
    query: Option<&str>,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let results = search_users_internal(&mut transaction, query).await?;
    transaction.commit().await?;
    Ok(RawText(serde_json::to_string(&results)?))
}

async fn search_users_internal(
    transaction: &mut Transaction<'_, Postgres>,
    query: Option<&str>,
) -> Result<Vec<UserSearchResult>, event::Error> {
    let query = query.unwrap_or("");
    if query.len() < 2 {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as!(UserSearchRow, r#"
        SELECT id, display_source AS "display_source: DisplaySource", racetime_display_name, racetime_id, discord_display_name, discord_username
        FROM users 
        WHERE (racetime_display_name ILIKE $1 OR discord_display_name ILIKE $1 OR racetime_id ILIKE $1 OR discord_username ILIKE $1)
        ORDER BY 
            CASE WHEN racetime_display_name ILIKE $1 THEN 0 ELSE 1 END,
            CASE WHEN discord_display_name ILIKE $1 THEN 0 ELSE 1 END,
            CASE WHEN racetime_id ILIKE $1 THEN 0 ELSE 1 END,
            CASE WHEN discord_username ILIKE $1 THEN 0 ELSE 1 END,
            racetime_display_name, discord_display_name
        LIMIT 10
    "#, format!("%{}%", query))
    .fetch_all(&mut **transaction).await?;

    Ok(rows
        .into_iter()
        .map(|row| UserSearchResult {
            id: row.id,
            display_name: match row.display_source {
                DisplaySource::RaceTime => row.racetime_display_name.unwrap_or_default(),
                DisplaySource::Discord => row.discord_display_name.unwrap_or_default(),
            },
            racetime_id: row.racetime_id,
            discord_username: row.discord_username,
        })
        .collect())
}

#[derive(serde::Serialize)]
struct UserSearchResult {
    #[serde(serialize_with = "serialize_user_id")]
    id: Id<Users>,
    display_name: String,
    racetime_id: Option<String>,
    discord_username: Option<String>,
}

fn serialize_user_id<S>(id: &Id<Users>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&u64::from(*id).to_string())
}

struct UserSearchRow {
    id: Id<Users>,
    display_source: DisplaySource,
    racetime_display_name: Option<String>,
    racetime_id: Option<String>,
    discord_display_name: Option<String>,
    discord_username: Option<String>,
}
fn create_form_content(
    me: &Option<User>,
    csrf: Option<&CsrfToken>,
    ctx: Context<'_>,
    game: &Game,
    series: &[Series],
    sources: &[(String, String, String)],
    selected_source: Option<&str>,
    source_defaults: Option<&HashMap<String, String>>,
    submitted: bool,
    can_manage: bool,
) -> RawHtml<String> {
    if me.is_some() {
        if can_manage {
            let mut errors = ctx.errors().collect_vec();
            let field_value = |name: &str| {
                ctx.field_value(name).or_else(|| {
                    if submitted {
                        matches!(name, "listed" | "is_custom_goal" | "is_single_race" |
                            "hide_entrants" | "restrict_chat_in_qualifiers" | "is_live_event")
                            .then_some("")
                    } else {
                        source_defaults.and_then(|defaults| defaults.get(name).map(String::as_str))
                    }
                })
            };
            html! {
                article {
                    h2 : format!("Create New Event — {}", game.display_name);

                    form(method = "get", action = uri!(create_get(&game.name, _))) {
                        fieldset {
                            label(for = "copy_from") : "Copy information from another event";
                            select(id = "copy_from", name = "copy_from") {
                                option(value = "") : "Start from scratch";
                                @for (source_series, source_event, source_name) in sources {
                                    @let source_id = format!("{source_series}/{source_event}");
                                    option(value = &source_id, selected? = selected_source == Some(source_id.as_str())) : format!("{source_name} ({source_id})");
                                }
                            }
                            input(type = "submit", value = "Load event");
                            p(class = "help") : "Selecting an event reloads this form with its setup fields. Creating the event also copies its organizers, volunteer managers, restream coordinators, and other configuration. Event dates and tournament-specific links are cleared, weekly schedules start disabled, and the Discord participant role must be assigned separately.";
                        }
                    }

                    : full_form(uri!(create_post(&game.name)), csrf, html! {
                        @if let Some(source) = selected_source {
                            input(type = "hidden", name = "copy_from", value = source);
                        }

                        : form_field("series", &mut errors, html! {
                            : help::label("series", "Series");
                            select(id = "series", name = "series", style = "width: 100%; max-width: 600px;") {
                                @for series in series {
                                    option(value = series.slug(), selected? = field_value("series").map_or(false, |v| v == series.slug())) : series.display_name();
                                }
                            }
                        });

                        : form_field("event", &mut errors, html! {
                            : help::label("event", "Event Slug");
                            input(type = "text", id = "event", name = "event", value = field_value("event").unwrap_or(&String::new()), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (e.g. \"2025\", \"s1\")";
                        });

                        : form_field("display_name", &mut errors, html! {
                            label(for = "display_name") : "Display Name";
                            input(type = "text", id = "display_name", name = "display_name", value = field_value("display_name").unwrap_or(&String::new()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("team_config", &mut errors, html! {
                            : help::label("team_config", "Team Configuration");
                            select(id = "team_config", name = "team_config", style = "width: 100%; max-width: 600px;") {
                                option(value = "solo", selected? = field_value("team_config").map_or(true, |v| v == "solo")) : "Solo";
                                option(value = "coop", selected? = field_value("team_config").map_or(false, |v| v == "coop")) : "Co-op";
                                option(value = "tfbcoop", selected? = field_value("team_config").map_or(false, |v| v == "tfbcoop")) : "TFB Co-op";
                                option(value = "pictionary", selected? = field_value("team_config").map_or(false, |v| v == "pictionary")) : "Pictionary";
                                option(value = "multiworld", selected? = field_value("team_config").map_or(false, |v| v == "multiworld")) : "Multiworld";
                            }
                        });

                        : form_field("language", &mut errors, html! {
                            : help::label("language", "Language");
                            select(id = "language", name = "language", style = "width: 100%; max-width: 600px;") {
                                option(value = "en", selected? = field_value("language").map_or(true, |v| v == "en")) : "English";
                                option(value = "fr", selected? = field_value("language").map_or(false, |v| v == "fr")) : "French";
                                option(value = "de", selected? = field_value("language").map_or(false, |v| v == "de")) : "German";
                                option(value = "pt", selected? = field_value("language").map_or(false, |v| v == "pt")) : "Portuguese";
                            }
                        });

                        : form_field("listed", &mut errors, html! {
                            input(type = "checkbox", id = "listed", name = "listed", checked? = field_value("listed").map_or(false, |value| value == "on"));
                            : help::label("listed", "Listed");
                            label(class = "help") : " (Show this event on the main page)";
                        });

                        h3 : "Racetime Bot Configuration";

                        : form_field("racetime_goal_slug", &mut errors, html! {
                            : help::label("racetime_goal_slug", "Goal Slug");
                            input(type = "text", id = "racetime_goal_slug", name = "racetime_goal_slug", value = field_value("racetime_goal_slug").unwrap_or(""), style = "width: 100%; max-width: 600px;", placeholder = "Exact goal string on racetime.gg (empty = no goal)");
                        });

                        : form_field("is_custom_goal", &mut errors, html! {
                            input(type = "checkbox", id = "is_custom_goal", name = "is_custom_goal", checked? = field_value("is_custom_goal").map_or(true, |value| value == "on"));
                            : help::label("is_custom_goal", "Is Custom Goal");
                            label(class = "help") : " (When enabled, the racetime.gg goal is a custom goal rather than a standard one.)";
                        });

                        : form_field("draft_kind", &mut errors, html! {
                            : help::label("draft_kind", "Draft Kind");
                            select(id = "draft_kind", name = "draft_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = field_value("draft_kind").map_or(true, |v| v.is_empty())) : "None";
                                @for (slug, label) in &[
                                    ("s7", "S7"),
                                    ("multiworld_s3", "Multiworld S3"),
                                    ("multiworld_s4", "Multiworld S4"),
                                    ("multiworld_s5", "Multiworld S5"),
                                    ("rsl_s7", "RSL S7"),
                                    ("tournoifranco_s3", "Tournoi Franco S3"),
                                    ("tournoifranco_s4", "Tournoi Franco S4"),
                                    ("tournoifranco_s5", "Tournoi Franco S5"),
                                    ("ban_pick", "Ban/Pick (generic, needs config)"),
                                    ("ban_only", "Ban Only (generic, needs config)"),
                                    ("pick_only", "Pick Only (generic, needs config)"),
                                ] {
                                    option(value = slug, selected? = field_value("draft_kind").map_or(false, |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("draft_config", &mut errors, html! {
                            : help::label("draft_config", "Draft Config JSON");
                            textarea(id = "draft_config", name = "draft_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : field_value("draft_config").unwrap_or(&String::new());
                            }
                            label(class = "help") : " (JSON configuration for generic draft modes. Leave empty if not applicable.)";
                        });

                        : form_field("qualifier_mode", &mut errors, html! {
                            : help::label("qualifier_mode", "Qualification method");
                            select(id = "qualifier_mode", name = "qualifier_mode") {
                                @for (slug, label) in [("none", "No qualification"), ("rank", "Stored qualifier ranks"), ("single", "Single async qualifier"), ("score", "Configured scoring"), ("pooled_by_mode", "Pooled by mode")] {
                                    option(value = slug, selected? = field_value("qualifier_mode").unwrap_or("none") == slug) : label;
                                }
                            }
                            label(class = "help") : "Choose how entrants qualify. For Stored qualifier ranks, assign entrant ranks on the Qualifiers page after creating the event. Ranks and async submissions only affect qualification when their method is selected. Configured scoring combines live qualifier races and qualifier async results.";
                        });

                        : form_field("qualifier_score_kind", &mut errors, html! {
                            : help::label("qualifier_score_kind", "Qualifier Score Kind");
                            select(id = "qualifier_score_kind", name = "qualifier_score_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = field_value("qualifier_score_kind").map_or(true, |v| v.is_empty())) : "None";
                                @for (slug, label) in &[
                                    ("time_relative", "Time relative to par (configurable)"),
                                    ("standard", "Standard"),
                                    ("sgl_2023_online", "SGL 2023 Online"),
                                    ("sgl_2024_online", "SGL 2024 Online"),
                                    ("sgl_2025_online", "SGL 2025 Online"),
                                    ("twwr_miniblins26", "TWWR Miniblins 26"),
                                    ("twwr_main", "TWWR Main"),
                                ] {
                                    @let defaults = super::scoring::ParScoreConfig::for_kind(slug, None)
                                        .expect("built-in scoring defaults are valid")
                                        .map(|config| serde_json::to_string_pretty(&config).expect("scoring defaults serialize"))
                                        .unwrap_or_else(|| "{}".into());
                                    option(value = slug, data_score_defaults = defaults, selected? = field_value("qualifier_score_kind").map_or(false, |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("qualifier_score_config", &mut errors, html! {
                            : help::label("qualifier_score_config", "Qualifier scoring parameters (JSON)");
                            textarea(id = "qualifier_score_config", name = "qualifier_score_config", rows = "5", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : field_value("qualifier_score_config").unwrap_or("");
                            }
                            label(class = "help") : "Selecting a scoring kind fills in its default parameters. Edit these values to customize scoring. An empty object means this kind has no configurable parameters.";
                        });

                        : form_field("is_single_race", &mut errors, html! {
                            input(type = "checkbox", id = "is_single_race", name = "is_single_race", checked? = field_value("is_single_race").map_or(false, |value| value == "on"));
                            : help::label("is_single_race", "Single Race Event");
                        });

                        : form_field("hide_entrants", &mut errors, html! {
                            input(type = "checkbox", id = "hide_entrants", name = "hide_entrants", checked? = field_value("hide_entrants").map_or(false, |value| value == "on"));
                            : help::label("hide_entrants", "Hide Entrants");
                        });

                        : form_field("start_delay", &mut errors, html! {
                            : help::label("start_delay", "Start Delay (seconds)");
                            input(type = "number", id = "start_delay", name = "start_delay", value = field_value("start_delay").unwrap_or(&"15".to_owned()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("start_delay_open", &mut errors, html! {
                            : help::label("start_delay_open", "Start Delay Open (seconds)");
                            input(type = "text", id = "start_delay_open", name = "start_delay_open", value = field_value("start_delay_open").unwrap_or(&String::new()), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Leave empty to use same as Start Delay)";
                        });

                        : form_field("restrict_chat_in_qualifiers", &mut errors, html! {
                            input(type = "checkbox", id = "restrict_chat_in_qualifiers", name = "restrict_chat_in_qualifiers", checked? = field_value("restrict_chat_in_qualifiers").map_or(false, |value| value == "on"));
                            : help::label("restrict_chat_in_qualifiers", "Restrict Chat in Qualifiers");
                        });

                        : form_field("preroll_mode", &mut errors, html! {
                            : help::label("preroll_mode", "Preroll Mode");
                            select(id = "preroll_mode", name = "preroll_mode", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("none", "None"), ("short", "Short"), ("medium", "Medium"), ("long", "Long")] {
                                    option(value = val, selected? = field_value("preroll_mode").map_or(*val == "none", |v| v == *val)) : *label;
                                }
                            }
                        });

                        : form_field("spoiler_unlock", &mut errors, html! {
                            : help::label("spoiler_unlock", "Spoiler Log Unlock");
                            select(id = "spoiler_unlock", name = "spoiler_unlock", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("never", "Never"), ("after", "After race"), ("immediately", "Immediately")] {
                                    option(value = val, selected? = field_value("spoiler_unlock").map_or(*val == "never", |v| v == *val)) : *label;
                                }
                            }
                        });



                        : form_field("is_live_event", &mut errors, html! {
                            input(type = "checkbox", id = "is_live_event", name = "is_live_event", checked? = field_value("is_live_event").map_or(false, |value| value == "on"));
                            : help::label("is_live_event", "Is Live Event");
                            label(class = "help") : " (In-person event: scheduled races after the event starts send notifications instead of creating racetime.gg rooms.)";
                        });

                        h3 : "Seed Generation";

                        : form_field("seed_gen_type", &mut errors, html! {
                            : help::label("seed_gen_type", "Seed Gen Type");
                            select(id = "seed_gen_type", name = "seed_gen_type", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = field_value("seed_gen_type").map_or(true, |v| v.is_empty())) : "None (manual / external)";
                                @for (val, label) in &[
                                    ("alttpr_dr", "ALTTPR Door Rando (stable)"),
                                    ("alttpr_dr_latest", "ALTTPR Door Rando (latest)"),
                                    ("alttpr_avianart", "ALTTPR Avianart"),
                                    ("owr", "ALTTPR OWR (regular build)"),
                                    ("owr_tourney", "ALTTPR OWR (tournament build)"),
                                    ("ootr", "OoTR"),
                                    ("ootr_web", "OoTR Web"),
                                    ("ootr_tfb", "OoTR Triforce Blitz"),
                                    ("ootr_rsl", "OoTR RSL"),
                                    ("twwr", "The Wind Waker Randomizer"),
                                    ("mmr", "MMR"),
                                ] {
                                    option(value = val, selected? = field_value("seed_gen_type").map_or(false, |v| v == *val)) : *label;
                                }
                            }
                            label(class = "help") : "Choose OWR (regular build) for /opt/owr or OWR (tournament build) for /opt/owr_tourney. The selection applies to this event's live, async and practice seeds. Both accept the same JSON structure; use settings supported by the installed build. Door Rando (stable) keeps the existing installation; Door Rando (latest) uses /opt/alttpr_latest with the same source and settings options.";
                            p(class = "help") : "For pooled qualifiers, configure each mode's generator and baseline settings on the Qualifiers page after creating the event. Existing pooled OWR modes also use the tournament build. A branch name in Seed Config JSON does not switch installations.";
                        });

                        : form_field("choice_resolution", &mut errors, html! {
                            label(for = "choice_resolution") : "Resolve random player choices";
                            select(id = "choice_resolution", name = "choice_resolution") {
                                @for (value, label) in [("race_creation", "On race creation / import"), ("room_opening", "On room opening"), ("seed_rolling", "On seed reveal")] {
                                    option(value = value, selected? = field_value("choice_resolution").unwrap_or("seed_rolling") == value) : label;
                                }
                            }
                            p(class = "help") : "For OWR and Door Rando mutual choices. Each game's result is saved and reused for all rooms and seed rerolls. Scheduling threads always show agreed settings and rules; only creation/import also reveals random results there. Room opening posts in each race room or private async thread; without a room it falls back to seed reveal. Seed reveal posts settings beside the seed, separately for each async participant. This selector sets choice_resolution in Seed Config JSON; changing it affects only unresolved races.";
                        });

                        : guides::seed_config();
                        : form_field("seed_config", &mut errors, html! {
                            : help::label("seed_config", "Seed Config JSON");
                            textarea(id = "seed_config", name = "seed_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : field_value("seed_config").unwrap_or(&String::new());
                            }
                            label(class = "help") : " (JSON config for the seed gen type. Leave empty if not applicable.)";
                        });
                    }, errors.clone(), "Create Event");
                    script(src = static_url!("event-copy.js")) {}
                    script(src = static_url!("qualifier-score-config.js")) {}
                    script(src = static_url!("setting-help.js")) {}
                }
            }
        } else {
            html! {
                article {
                    p : "You must be a global admin or an admin for this game to access this page.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(create_get(&game.name, _)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to access this page.";
                }
            }
        }
    }
}

async fn create_sources(
    transaction: &mut Transaction<'_, Postgres>,
    game: &Game,
) -> Result<Vec<(String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT e.series, e.event, e.display_name FROM events e JOIN game_series gs ON gs.series = e.series WHERE gs.game_id = $1 ORDER BY e.series, e.event DESC",
    )
    .bind(game.id)
    .fetch_all(&mut **transaction)
    .await
}

fn selected_create_source(
    sources: &[(String, String, String)],
    selected: Option<&str>,
) -> Option<(String, String)> {
    let selected = selected?;
    sources
        .iter()
        .find(|(series, event, _)| selected == format!("{series}/{event}"))
        .map(|(series, event, _)| (series.clone(), event.clone()))
}

async fn create_source_defaults(
    transaction: &mut Transaction<'_, Postgres>,
    game: &Game,
    source: &str,
) -> Result<Option<HashMap<String, String>>, sqlx::Error> {
    let Some((series, event)) = source.split_once('/') else {
        return Ok(None);
    };
    let row: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT to_jsonb(e) FROM events e JOIN game_series gs ON gs.series = e.series WHERE gs.game_id = $1 AND e.series = $2 AND e.event = $3",
    )
    .bind(game.id)
    .bind(series)
    .bind(event)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(row.and_then(|row| {
        row.as_object().map(|values| {
            let mut defaults = HashMap::new();
            for name in [
                "series",
                "display_name",
                "team_config",
                "language",
                "racetime_goal_slug",
                "draft_kind",
                "draft_config",
                "qualifier_mode",
                "qualifier_score_kind",
                "qualifier_score_config",
                "is_single_race",
                "hide_entrants",
                "start_delay",
                "start_delay_open",
                "restrict_chat_in_qualifiers",
                "preroll_mode",
                "spoiler_unlock",
                "is_custom_goal",
                "is_live_event",
                "seed_gen_type",
                "seed_config",
            ] {
                let value = values.get(name).unwrap_or(&serde_json::Value::Null);
                let value = match value {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(value) => if *value { "on" } else { "" }.into(),
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        serde_json::to_string_pretty(value).expect("database JSON serializes")
                    }
                    value => value.to_string(),
                };
                defaults.insert(name.to_owned(), value);
            }
            let resolution = values
                .get("seed_config")
                .and_then(|config| config.get("choice_resolution"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("seed_rolling");
            defaults.insert("choice_resolution".into(), resolution.into());
            defaults
        })
    }))
}

#[rocket::get("/games/<game_name>/event/new?<copy_from>")]
pub(crate) async fn create_get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    copy_from: Option<&str>,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let game = Game::from_name(&mut transaction, game_name)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let series = game.series(&mut transaction).await?;
    let can_manage = if let Some(ref me) = me {
        can_manage_game(&mut transaction, me, &game).await?
    } else {
        false
    };
    let sources = if can_manage {
        create_sources(&mut transaction, &game).await?
    } else {
        Vec::new()
    };
    let source_defaults = if can_manage {
        match copy_from {
            Some(source) if !source.is_empty() => Some(
                create_source_defaults(&mut transaction, &game, source)
                    .await?
                    .ok_or(StatusOrError::Status(Status::NotFound))?,
            ),
            _ => None,
        }
    } else {
        None
    };
    let content = create_form_content(
        &me,
        csrf.as_ref(),
        Context::default(),
        &game,
        &series,
        &sources,
        copy_from,
        source_defaults.as_ref(),
        false,
        can_manage,
    );
    Ok(page(
        transaction,
        &me,
        &uri,
        PageStyle::default(),
        "Create New Event",
        content,
    )
    .await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct CreateEventForm {
    #[field(default = String::new())]
    csrf: String,
    copy_from: Option<String>,
    series: String,
    event: String,
    display_name: String,
    team_config: String,
    language: String,
    listed: bool,
    racetime_goal_slug: Option<String>,
    draft_kind: Option<String>,
    draft_config: Option<String>,
    qualifier_mode: String,
    qualifier_score_kind: Option<String>,
    qualifier_score_config: Option<String>,
    is_single_race: bool,
    hide_entrants: bool,
    #[field(default = 15)]
    start_delay: i32,
    start_delay_open: Option<String>,
    restrict_chat_in_qualifiers: bool,
    #[field(default = String::from("medium"))]
    preroll_mode: String,
    #[field(default = String::from("after"))]
    spoiler_unlock: String,
    is_custom_goal: bool,
    is_live_event: bool,
    seed_gen_type: Option<String>,
    seed_config: Option<String>,
    choice_resolution: Option<String>,
}

#[rocket::post("/games/<game_name>/event/new", data = "<form>")]
pub(crate) async fn create_post(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, CreateEventForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let game = Game::from_name(&mut transaction, game_name)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let allowed_series = game.series(&mut transaction).await?;
    let can_manage = can_manage_game(&mut transaction, &me, &game).await?;
    let sources = if can_manage {
        create_sources(&mut transaction, &game).await?
    } else {
        Vec::new()
    };
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if !can_manage {
            form.context.push_error(form::Error::validation(
                "You must be a global admin or an admin for this game to create events.",
            ));
        }
        let source = selected_create_source(&sources, value.copy_from.as_deref());
        if value
            .copy_from
            .as_deref()
            .is_some_and(|source| !source.is_empty())
            && source.is_none()
        {
            form.context.push_error(
                form::Error::validation("Select an event from this game.").with_name("copy_from"),
            );
        }

        // Parse series
        let series = match value.series.parse::<Series>() {
            Ok(s) if allowed_series.contains(&s) => Some(s),
            Ok(_) => {
                form.context.push_error(
                    form::Error::validation("The selected series does not belong to this game.")
                        .with_name("series"),
                );
                None
            }
            Err(()) => {
                form.context
                    .push_error(form::Error::validation("Invalid series.").with_name("series"));
                None
            }
        };

        // Validate event slug is non-empty
        if value.event.is_empty() {
            form.context
                .push_error(form::Error::validation("Event slug is required.").with_name("event"));
        }

        // Validate display name is non-empty
        if value.display_name.is_empty() {
            form.context.push_error(
                form::Error::validation("Display name is required.").with_name("display_name"),
            );
        }

        // Parse team_config
        let team_config = match value.team_config.as_str() {
            "solo" => TeamConfig::Solo,
            "coop" => TeamConfig::CoOp,
            "tfbcoop" => TeamConfig::TfbCoOp,
            "pictionary" => TeamConfig::Pictionary,
            "multiworld" => TeamConfig::Multiworld,
            _ => {
                form.context.push_error(
                    form::Error::validation("Invalid team configuration.").with_name("team_config"),
                );
                TeamConfig::Solo
            }
        };

        // Parse language
        let language = match value.language.as_str() {
            "en" => English,
            "fr" => French,
            "de" => German,
            "pt" => Portuguese,
            _ => {
                form.context
                    .push_error(form::Error::validation("Invalid language.").with_name("language"));
                English
            }
        };

        // Parse optional string fields
        let racetime_goal_slug = value
            .racetime_goal_slug
            .as_ref()
            .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let draft_kind = value
            .draft_kind
            .as_ref()
            .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let qualifier_score_kind = value
            .qualifier_score_kind
            .as_ref()
            .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let seed_gen_type = value
            .seed_gen_type
            .as_ref()
            .and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

        // Parse draft_config JSON
        let draft_config_json: Option<serde_json::Value> =
            if let Some(ref dc_str) = value.draft_config {
                if dc_str.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str(dc_str) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            form.context.push_error(
                                form::Error::validation(format!("Invalid draft config JSON: {e}"))
                                    .with_name("draft_config"),
                            );
                            None
                        }
                    }
                }
            } else {
                None
            };

        // Parse seed_config JSON
        let mut seed_config_json: Option<serde_json::Value> =
            if let Some(ref sc_str) = value.seed_config {
                if sc_str.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str(sc_str) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            form.context.push_error(
                                form::Error::validation(format!("Invalid seed config JSON: {e}"))
                                    .with_name("seed_config"),
                            );
                            None
                        }
                    }
                }
            } else {
                None
            };

        if let Some(timing) = value.choice_resolution.as_deref() {
            if let Some(config) = seed_config_json.as_mut().and_then(serde_json::Value::as_object_mut) {
                config.insert("choice_resolution".into(), json!(timing));
            }
        }

        // Parse start_delay_open
        let start_delay_open: Option<i32> = if let Some(ref sdo_str) = value.start_delay_open {
            if sdo_str.trim().is_empty() {
                None
            } else {
                match sdo_str.trim().parse::<i32>() {
                    Ok(v) => Some(v),
                    Err(_) => {
                        form.context.push_error(
                            form::Error::validation("Invalid start delay open value.")
                                .with_name("start_delay_open"),
                        );
                        None
                    }
                }
            }
        } else {
            None
        };

        if let Err(error) = event::configuration::validate_qualification(
            &value.qualifier_mode,
            qualifier_score_kind.as_deref(),
        ) {
            form.context
                .push_error(form::Error::validation(error).with_name("qualifier_mode"));
        }
        let qualifier_score_config = parse_score_config(
            value.qualifier_score_config.as_deref(),
            qualifier_score_kind.as_deref(),
            &mut form.context,
        );
        for (field, result) in [
            (
                "seed_config",
                event::configuration::validate_seed(
                    seed_gen_type.as_deref(),
                    seed_config_json.as_ref(),
                ),
            ),
            (
                "preroll_mode",
                event::configuration::validate_seed_policies(
                    &value.preroll_mode,
                    &value.spoiler_unlock,
                    seed_gen_type.as_deref(),
                ),
            ),
            (
                "draft_config",
                event::configuration::validate_draft(
                    draft_kind.as_deref(),
                    draft_config_json.as_ref(),
                    seed_gen_type.as_deref(),
                    seed_config_json.as_ref(),
                    None,
                ),
            ),
            (
                "start_delay",
                event::configuration::validate_start_delay(value.start_delay),
            ),
            (
                "start_delay_open",
                start_delay_open
                    .map(event::configuration::validate_start_delay)
                    .unwrap_or(Ok(())),
            ),
        ] {
            if let Err(error) = result {
                form.context
                    .push_error(form::Error::validation(error).with_name(field));
            }
        }

        if form.context.errors().next().is_some() {
            let me = Some(me);
            let content = create_form_content(
                &me,
                csrf.as_ref(),
                form.context,
                &game,
                &allowed_series,
                &sources,
                value.copy_from.as_deref(),
                None,
                true,
                can_manage,
            );
            return Ok(RedirectOrContent::Content(
                page(
                    transaction,
                    &me,
                    &uri,
                    PageStyle::default(),
                    "Create New Event",
                    content,
                )
                .await?,
            ));
        }

        let series = series.expect("series should be valid if no errors");

        if let Some((source_series, source_event)) = &source {
            // Clone the event configuration, then apply the reviewed creation fields below.
            // Lifecycle dates and external event links refer to the old event and must be reset.
            let inserted = sqlx::query(
                r#"INSERT INTO events
                   SELECT (jsonb_populate_record(NULL::events,
                       to_jsonb(source) || jsonb_build_object(
                           'series', $1::text, 'event', $2::text,
                           'start', NULL, 'end_time', NULL,
                           'url', NULL, 'video_url', NULL,
                           'enter_url', NULL, 'teams_url', NULL,
                           'speedgaming_slug', NULL, 'speedgaming_in_person_id', NULL,
                           'listed', false))).*
                   FROM events source WHERE source.series = $3 AND source.event = $4"#,
            )
            .bind(series.slug())
            .bind(&value.event)
            .bind(source_series)
            .bind(source_event)
            .execute(&mut *transaction)
            .await?;
            if inserted.rows_affected() != 1 {
                return Err(StatusOrError::Status(Status::Conflict));
            }

            sqlx::query(
                r#"UPDATE events SET display_name = $3, team_config = $4, language = $5, listed = $6,
                    racetime_goal_slug = $7, draft_kind = $8, draft_config = $9, qualifier_score_kind = $10,
                    is_single_race = $11, hide_entrants = $12, start_delay = $13, start_delay_open = $14,
                    restrict_chat_in_qualifiers = $15, preroll_mode = $16, spoiler_unlock = $17,
                    is_custom_goal = $18, is_live_event = $19, seed_gen_type = $20, seed_config = $21,
                    settings_string = CASE WHEN $20 = 'twwr' THEN settings_string ELSE NULL END
                   WHERE series = $1 AND event = $2"#,
            )
            .bind(series.slug())
            .bind(&value.event)
            .bind(&value.display_name)
            .bind(team_config)
            .bind(language)
            .bind(value.listed)
            .bind(&racetime_goal_slug)
            .bind(&draft_kind)
            .bind(&draft_config_json)
            .bind(&qualifier_score_kind)
            .bind(value.is_single_race)
            .bind(value.hide_entrants)
            .bind(value.start_delay)
            .bind(start_delay_open)
            .bind(value.restrict_chat_in_qualifiers)
            .bind(&value.preroll_mode)
            .bind(&value.spoiler_unlock)
            .bind(value.is_custom_goal)
            .bind(value.is_live_event)
            .bind(&seed_gen_type)
            .bind(&seed_config_json)
            .execute(&mut *transaction)
            .await?;
        } else {
            sqlx::query!(r#"
            INSERT INTO events (series, event, display_name, team_config, language, listed,
                racetime_goal_slug, draft_kind, draft_config, qualifier_score_kind,
                is_single_race, hide_entrants, start_delay, start_delay_open, restrict_chat_in_qualifiers,
                preroll_mode, spoiler_unlock, is_custom_goal, is_live_event,
                seed_gen_type, seed_config)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
        "#,
                series as _,
                &value.event,
                &value.display_name,
                team_config as _,
                language as _,
                value.listed,
                racetime_goal_slug,
                draft_kind,
                draft_config_json as _,
                qualifier_score_kind,
                value.is_single_race,
                value.hide_entrants,
                value.start_delay,
                start_delay_open,
                value.restrict_chat_in_qualifiers,
                &value.preroll_mode,
                &value.spoiler_unlock,
                value.is_custom_goal,
                value.is_live_event,
                seed_gen_type,
                seed_config_json as _,
            ).execute(&mut *transaction).await?;
        }

        if let Some((source_series, source_event)) = &source {
            copy_event_configuration(
                &mut transaction,
                series,
                &value.event,
                source_series,
                source_event,
            )
            .await?;
        }

        mirror_twwr_permalink(&mut transaction, series, &value.event).await?;
        save_score_config(
            &mut transaction,
            series,
            &value.event,
            qualifier_score_config,
        )
        .await?;
        save_qualifier_mode(
            &mut transaction,
            series,
            &value.event,
            &value.qualifier_mode,
        )
        .await?;
        transaction.commit().await?;
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, &*value.event))))
    } else {
        let me = Some(me);
        let selected_source = form.context.field_value("copy_from").map(str::to_owned);
        let content = create_form_content(
            &me,
            csrf.as_ref(),
            form.context,
            &game,
            &allowed_series,
            &sources,
            selected_source.as_deref(),
            None,
            true,
            can_manage,
        );
        RedirectOrContent::Content(
            page(
                transaction,
                &me,
                &uri,
                PageStyle::default(),
                "Create New Event",
                content,
            )
            .await?,
        )
    })
}

async fn copy_event_configuration(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    source_series: &str,
    source_event: &str,
) -> Result<(), sqlx::Error> {
    // These rows describe the event itself; participant and race records stay behind.
    for statement in [
        "INSERT INTO event_descriptions (series, event, content) SELECT $1, $2, content FROM event_descriptions WHERE series = $3 AND event = $4",
        "INSERT INTO event_round_configs (series, event, round, restream_consent_required, scheduling_deadline) SELECT $1, $2, round, restream_consent_required, NULL FROM event_round_configs WHERE series = $3 AND event = $4",
        "INSERT INTO phase_round_options (series, event, phase, round, display_fr) SELECT $1, $2, phase, round, display_fr FROM phase_round_options WHERE series = $3 AND event = $4",
        "INSERT INTO startgg_phase_round_mappings (series, event, original_phase, original_round, mapped_phase, mapped_round) SELECT $1, $2, original_phase, original_round, mapped_phase, mapped_round FROM startgg_phase_round_mappings WHERE series = $3 AND event = $4",
        "INSERT INTO startgg_pool_name_mappings (series, event, original_identifier, mapped_name) SELECT $1, $2, original_identifier, mapped_name FROM startgg_pool_name_mappings WHERE series = $3 AND event = $4",
        "INSERT INTO event_restreamer_discord_roles (series, event, language, discord_role_id) SELECT $1, $2, language, discord_role_id FROM event_restreamer_discord_roles WHERE series = $3 AND event = $4",
        "INSERT INTO organizers (series, event, organizer) SELECT $1, $2, organizer FROM organizers WHERE series = $3 AND event = $4",
        "INSERT INTO event_volunteer_managers (series, event, user_id) SELECT $1, $2, user_id FROM event_volunteer_managers WHERE series = $3 AND event = $4",
        "INSERT INTO restreamers (series, event, restreamer, language) SELECT $1, $2, restreamer, language FROM restreamers WHERE series = $3 AND event = $4",
        "INSERT INTO role_bindings (series, event, role_type_id, min_count, max_count, discord_role_id, game_id, auto_approve, language, custom_pool) SELECT $1, $2, role_type_id, min_count, max_count, discord_role_id, NULL, auto_approve, language, custom_pool FROM role_bindings WHERE series = $3 AND event = $4 AND game_id IS NULL",
    ] {
        sqlx::query(statement)
            .bind(series.slug())
            .bind(event)
            .bind(source_series)
            .bind(source_event)
            .execute(&mut **transaction)
            .await?;
    }

    // Game-level role bindings retain their IDs; event-level bindings get new IDs.
    for statement in [
        r#"INSERT INTO event_disabled_role_bindings (series, event, role_binding_id)
           SELECT $1, $2, CASE WHEN source_binding.game_id IS NULL THEN new_binding.id ELSE source_binding.id END
           FROM event_disabled_role_bindings source_row
           JOIN role_bindings source_binding ON source_binding.id = source_row.role_binding_id
           LEFT JOIN role_bindings new_binding ON source_binding.game_id IS NULL
               AND new_binding.series = $1 AND new_binding.event = $2
               AND new_binding.role_type_id = source_binding.role_type_id
               AND new_binding.language = source_binding.language
           WHERE source_row.series = $3 AND source_row.event = $4"#,
        r#"INSERT INTO event_role_binding_overrides
               (series, event, role_binding_id, discord_role_id, min_count, max_count)
           SELECT $1, $2, CASE WHEN source_binding.game_id IS NULL THEN new_binding.id ELSE source_binding.id END,
               source_row.discord_role_id, source_row.min_count, source_row.max_count
           FROM event_role_binding_overrides source_row
           JOIN role_bindings source_binding ON source_binding.id = source_row.role_binding_id
           LEFT JOIN role_bindings new_binding ON source_binding.game_id IS NULL
               AND new_binding.series = $1 AND new_binding.event = $2
               AND new_binding.role_type_id = source_binding.role_type_id
               AND new_binding.language = source_binding.language
           WHERE source_row.series = $3 AND source_row.event = $4"#,
    ] {
        sqlx::query(statement)
            .bind(series.slug())
            .bind(event)
            .bind(source_series)
            .bind(source_event)
            .execute(&mut **transaction)
            .await?;
    }

    let pooled = sqlx::query(
        r#"INSERT INTO pooled_qualifier_configs (
            series, event, required_mode_count, pool_seed_count, live_races_per_mode,
            async_run_limit, live_entry_close_lead, retry_limit, allocation_spread,
            par_finishers, score_scale, score_offset, score_minimum, score_maximum,
            requests_paused)
           SELECT $1, $2, required_mode_count, pool_seed_count, live_races_per_mode,
            async_run_limit, live_entry_close_lead, retry_limit, allocation_spread,
            par_finishers, score_scale, score_offset, score_minimum, score_maximum, TRUE
           FROM pooled_qualifier_configs WHERE series = $3 AND event = $4
             AND EXISTS (SELECT 1 FROM events WHERE series = $1 AND event = $2 AND qualifier_mode = 'pooled_by_mode')"#,
    )
    .bind(series.slug()).bind(event).bind(source_series).bind(source_event)
    .execute(&mut **transaction).await?;
    if pooled.rows_affected() != 0 {
        let modes: Vec<(i16, String, String, serde_json::Value, String, String, bool)> =
            sqlx::query_as(
                "SELECT position, display_name, seed_gen_type, seed_config, generator_profile, settings_fingerprint, enabled FROM qualifier_modes WHERE series = $1 AND event = $2 ORDER BY position",
            )
            .bind(source_series).bind(source_event)
            .fetch_all(&mut **transaction).await?;
        for (position, name, generator, config, profile, fingerprint, enabled) in modes {
            sqlx::query(
                "INSERT INTO qualifier_modes (series, event, position, slug, display_name, seed_gen_type, seed_config, generator_profile, settings_fingerprint, enabled) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(series.slug()).bind(event).bind(position)
            .bind(format!("{}-{position}", super::qualifiers::pooled_mode_slug(series, event, &name)))
            .bind(name).bind(generator).bind(config).bind(profile).bind(fingerprint).bind(enabled)
            .execute(&mut **transaction).await?;
        }
    }

    let source_series: Series = source_series
        .parse()
        .map_err(|()| sqlx::Error::Protocol("Unknown source series".into()))?;
    for mut schedule in WeeklySchedule::for_event(transaction, source_series, source_event).await? {
        schedule.id = Id::new(transaction).await?;
        schedule.series = series;
        schedule.event = event.to_owned();
        schedule.active = false;
        schedule.save(transaction).await?;
    }

    let workflow_ids: Vec<i32> = sqlx::query_scalar(
        "SELECT id FROM volunteer_ping_workflows WHERE series = $1 AND event = $2 ORDER BY id",
    )
    .bind(source_series.slug())
    .bind(source_event)
    .fetch_all(&mut **transaction)
    .await?;
    for source_id in workflow_ids {
        let target_id: i32 = sqlx::query_scalar(
            r#"INSERT INTO volunteer_ping_workflows
                (series, event, language, discord_ping_channel, delete_after_race,
                 workflow_type, ping_interval, schedule_time, schedule_day_of_week,
                 schedule_timezone, cutoff_hours, role_binding_ids)
               SELECT $1, $2, language, discord_ping_channel, delete_after_race,
                      workflow_type, ping_interval, schedule_time, schedule_day_of_week,
                      schedule_timezone, cutoff_hours,
                      CASE WHEN role_binding_ids IS NULL THEN NULL ELSE (
                          SELECT COALESCE(jsonb_agg(
                              CASE WHEN source_binding.game_id IS NULL THEN new_binding.id ELSE source_binding.id END
                          ) FILTER (WHERE source_binding.game_id IS NOT NULL OR new_binding.id IS NOT NULL), '[]'::jsonb)
                          FROM jsonb_array_elements_text(role_binding_ids) selected(id)
                          JOIN role_bindings source_binding ON source_binding.id = selected.id::bigint
                          LEFT JOIN role_bindings new_binding ON source_binding.game_id IS NULL
                              AND new_binding.series = $1 AND new_binding.event = $2
                              AND new_binding.role_type_id = source_binding.role_type_id
                              AND new_binding.language = source_binding.language
                      ) END
               FROM volunteer_ping_workflows WHERE id = $3 RETURNING id"#,
        )
        .bind(series.slug())
        .bind(event)
        .bind(source_id)
        .fetch_one(&mut **transaction)
        .await?;
        sqlx::query(
            "INSERT INTO volunteer_ping_lead_times (workflow_id, lead_time_hours) SELECT $1, lead_time_hours FROM volunteer_ping_lead_times WHERE workflow_id = $2",
        )
        .bind(target_id).bind(source_id)
        .execute(&mut **transaction).await?;
    }
    Ok(())
}

async fn mirror_twwr_permalink(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET settings_string = seed_config->>'permalink' WHERE series = $1 AND event = $2 AND seed_gen_type = 'twwr'").bind(series).bind(event).execute(&mut **transaction).await?;

    Ok(())
}

fn parse_score_config(
    value: Option<&str>,
    kind: Option<&str>,
    context: &mut Context<'_>,
) -> Option<serde_json::Value> {
    let result = (|| -> Result<Option<serde_json::Value>, String> {
        if kind.is_some_and(|kind| event::teams::QualifierScoreKind::from_slug(kind).is_none()) {
            return Err("Unknown qualifier scoring strategy.".into());
        }
        let config = value
            .filter(|s| !s.trim().is_empty())
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()
            .map_err(|e| e.to_string())?;
        event::scoring::ParScoreConfig::for_kind(kind.unwrap_or(""), config.as_ref())?;
        Ok(config)
    })();
    match result {
        Ok(config) => config,
        Err(error) => {
            context.push_error(form::Error::validation(error).with_name("qualifier_score_config"));
            None
        }
    }
}

async fn save_score_config(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    config: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET qualifier_score_config = $1 WHERE series = $2 AND event = $3")
        .bind(config)
        .bind(series)
        .bind(event)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_copy_source_must_be_in_this_games_dropdown() {
        let sources = vec![("alttprmain".into(), "2026".into(), "Tournament".into())];
        assert_eq!(
            selected_create_source(&sources, Some("alttprmain/2026")),
            Some(("alttprmain".into(), "2026".into()))
        );
        assert_eq!(selected_create_source(&sources, Some("ootr/2026")), None);
        assert_eq!(selected_create_source(&sources, Some("alttprmain/2025")), None);
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn database_event_copy_preserves_pooled_configuration() {
        let pool = event::configuration::test_pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let source_series: String = sqlx::query_scalar(
            "SELECT e.series FROM events e JOIN game_series gs ON gs.series = e.series LIMIT 1",
        )
        .fetch_one(&mut *transaction).await.unwrap();
        let series: Series = source_series.parse().unwrap();
        let game = Game::from_series(&mut transaction, series).await.unwrap().unwrap();
        let source_event = format!("c{:016x}", rand::random::<u64>());
        let target_event = format!("c{:016x}", rand::random::<u64>());
        for (slug, name) in [(&source_event, "Source"), (&target_event, "Target")] {
            sqlx::query(
                "INSERT INTO events (series, event, display_name, team_config, qualifier_mode) VALUES ($1,$2,$3,'solo','pooled_by_mode')",
            )
            .bind(&source_series).bind(slug).bind(name)
            .execute(&mut *transaction).await.unwrap();
        }
        sqlx::query("INSERT INTO event_descriptions (series, event, content) VALUES ($1,$2,'Copied rules')")
            .bind(&source_series).bind(&source_event)
            .execute(&mut *transaction).await.unwrap();
        sqlx::query("INSERT INTO pooled_qualifier_configs (series, event, required_mode_count, requests_paused) VALUES ($1,$2,4,FALSE)")
            .bind(&source_series).bind(&source_event)
            .execute(&mut *transaction).await.unwrap();
        sqlx::query("INSERT INTO qualifier_modes (series,event,position,slug,display_name,seed_gen_type,seed_config,generator_profile,settings_fingerprint) VALUES ($1,$2,1,$3,'Mode A','ootr','{}'::jsonb,'default','ootr:default:{}')")
            .bind(&source_series).bind(&source_event).bind(format!("{source_series}-{source_event}-mode-a"))
            .execute(&mut *transaction).await.unwrap();

        let sources = create_sources(&mut transaction, &game).await.unwrap();
        assert!(selected_create_source(&sources, Some(&format!("{source_series}/{source_event}"))).is_some());
        copy_event_configuration(&mut transaction, series, &target_event, &source_series, &source_event).await.unwrap();

        let description: String = sqlx::query_scalar("SELECT content FROM event_descriptions WHERE series=$1 AND event=$2")
            .bind(&source_series).bind(&target_event).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!(description, "Copied rules");
        let (required, paused): (i16, bool) = sqlx::query_as("SELECT required_mode_count, requests_paused FROM pooled_qualifier_configs WHERE series=$1 AND event=$2")
            .bind(&source_series).bind(&target_event).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!((required, paused), (4, true));
        let mode_count: i64 = sqlx::query_scalar("SELECT count(*) FROM qualifier_modes WHERE series=$1 AND event=$2")
            .bind(&source_series).bind(&target_event).fetch_one(&mut *transaction).await.unwrap();
        assert_eq!(mode_count, 1);
        transaction.rollback().await.unwrap();
    }

    #[test]
    fn named_baseline_practice_and_help_render_safely() {
        let config = racetime_bot::seed_gen_type::OwrEventConfig::parse(&json!({
            "baselines": {
                "a": {"label": "Mode A", "base_settings": {}},
                "b": {"label": "Mode B", "base_settings": {}},
                "c": {"label": "Mode <C>", "base_settings": {}}
            }
        })).unwrap();
        let selector = super::super::practice_baseline_field(&config).0;
        assert!(selector.contains("Mode &lt;C&gt;"));
        assert!(selector.contains("required"));
        let help = help::label("seed_config", "Seed Config JSON").0;
        assert!(help.contains("Named baselines and a shared mode draft"));
        if let Ok(path) = std::env::var("HTH_TEST_BROWSER_FIXTURE") {
            let styles = std::fs::read_to_string("assets/static/common.css").unwrap();
            let script = std::fs::read_to_string("assets/static/setting-help.js").unwrap();
            std::fs::write(path, format!("<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><style>{styles}</style></head><body><form>{selector}{help}<textarea id=\"seed_config\" name=\"seed_config\">unchanged</textarea><button type=\"submit\">Generate Practice Seed</button></form><script>{script}</script></body></html>")).unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn database_twwr_edits_keep_all_readers_consistent() {
        let pool = event::configuration::test_pool().await;

        let mut transaction = pool.begin().await.unwrap();
        let (series, slug): (String, String) = sqlx::query_as("SELECT series, event FROM events WHERE seed_gen_type = 'twwr' ORDER BY series, event LIMIT 1")
            .fetch_one(&mut *transaction).await.unwrap();
        let series: Series = series.parse().unwrap();
        // Organizer editing updates the canonical setting and legacy mirror together.
        event::configuration::save_twwr_permalink(
            &mut transaction,
            series,
            &slug,
            "organizer-edited-permalink",
        )
        .await
        .unwrap();
        let data = Data::new(&mut transaction, series, &*slug)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(data.twwr_permalink(), Some("organizer-edited-permalink"));
        assert_eq!(data.settings_string.as_deref(), data.twwr_permalink());
        // The basic setup form saves seed_config directly, then mirrors it.
        sqlx::query("UPDATE events SET seed_config = jsonb_set(seed_config, '{permalink}', '\"setup-edited-permalink\"'::jsonb) WHERE series = $1 AND event = $2")
            .bind(
            series).bind(
            &slug
        ).execute(&mut *transaction).await.unwrap();
        mirror_twwr_permalink(&mut transaction, series, &slug)
            .await
            .unwrap();
        let data = Data::new(&mut transaction, series, &*slug)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(data.twwr_permalink(), Some("setup-edited-permalink"));
        assert_eq!(data.settings_string.as_deref(), data.twwr_permalink());

        transaction.rollback().await.unwrap();
    }
}

async fn save_qualifier_mode(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    mode: &str,
) -> sqlx::Result<()> {
    let previous: String = sqlx::query_scalar(
        "SELECT qualifier_mode FROM events WHERE series = $1 AND event = $2 FOR UPDATE",
    )
    .bind(series)
    .bind(event)
    .fetch_one(&mut **transaction)
    .await?;
    if previous == "pooled_by_mode" && mode != "pooled_by_mode" {
        let has_history: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM qualifier_attempts WHERE series = $1 AND event = $2)",
        )
        .bind(series)
        .bind(event)
        .fetch_one(&mut **transaction)
        .await?;
        if has_history {
            return Err(sqlx::Error::Protocol("An event with pooled qualifier attempts cannot be changed back to another qualification method.".into()));
        }
    }
    sqlx::query("UPDATE events SET qualifier_mode = $1 WHERE series = $2 AND event = $3")
        .bind(mode)
        .bind(series)
        .bind(event)
        .execute(&mut **transaction)
        .await?;
    if mode == "pooled_by_mode" {
        sqlx::query("INSERT INTO pooled_qualifier_configs (series, event) VALUES ($1, $2) ON CONFLICT (series, event) DO NOTHING")
            .bind(series).bind(event).execute(&mut **transaction).await?;
    }
    Ok(())
}
