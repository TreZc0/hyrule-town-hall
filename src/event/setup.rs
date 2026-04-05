use crate::{
    event::{Data, Tab, enter},
    prelude::*,
    user::DisplaySource
};
use rocket::response::content::RawText;

async fn setup_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Setup, false).await?;

    // Load enter_flow, rando_version, seed_gen_type and seed_config as raw values for display in form
    let (enter_flow_json, rando_version_json, seed_gen_type_str, seed_config_json) = sqlx::query!(r#"
        SELECT enter_flow AS "enter_flow: serde_json::Value",
               rando_version AS "rando_version: serde_json::Value",
               seed_gen_type,
               seed_config AS "seed_config: serde_json::Value"
        FROM events WHERE series = $1 AND event = $2
    "#, event.series as _, &*event.event)
    .fetch_one(&mut *transaction).await
    .map(|row| (row.enter_flow, row.rando_version, row.seed_gen_type, row.seed_config))?;

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

    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if let Some(ref me) = me {
        if me.is_global_admin() {
            let mut errors = ctx.errors().collect_vec();
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
                            label(for = "start") : "Start Time";
                            input(type = "datetime-local", id = "start", name = "start", value = ctx.field_value("start").unwrap_or(
                                &event.start(&mut transaction).await?.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });
                        
                        : form_field("end", &mut errors, html! {
                            label(for = "end") : "End Time";
                            input(type = "datetime-local", id = "end", name = "end", value = ctx.field_value("end").unwrap_or(
                                &event.end.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });
                        
                        : form_field("url", &mut errors, html! {
                            label(for = "url") : "Event URL (start.gg/Challonge)";
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
                            label(for = "discord_invite_url") : "Discord Invite URL";
                            input(type = "url", id = "discord_invite_url", name = "discord_invite_url", value = ctx.field_value("discord_invite_url").unwrap_or(
                                &event.discord_invite_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });
                        
                        : form_field("discord_guild", &mut errors, html! {
                            label(for = "discord_guild") : "Discord Guild ID";
                            input(type = "text", id = "discord_guild", name = "discord_guild", value = ctx.field_value("discord_guild").unwrap_or(
                                &event.discord_guild.map(|g| g.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });
                        
                        : form_field("discord_race_room_channel", &mut errors, html! {
                            label(for = "discord_race_room_channel") : "Discord Race Room Channel ID";
                            input(type = "text", id = "discord_race_room_channel", name = "discord_race_room_channel", value = ctx.field_value("discord_race_room_channel").unwrap_or(
                                &event.discord_race_room_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });
                        
                        : form_field("discord_race_results_channel", &mut errors, html! {
                            label(for = "discord_race_results_channel") : "Discord Race Results Channel ID";
                            input(type = "text", id = "discord_race_results_channel", name = "discord_race_results_channel", value = ctx.field_value("discord_race_results_channel").unwrap_or(
                                &event.discord_race_results_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });
                        
                        : form_field("discord_volunteer_info_channel", &mut errors, html! {
                            label(for = "discord_volunteer_info_channel") : "Discord Volunteer Info Channel ID";
                            input(type = "text", id = "discord_volunteer_info_channel", name = "discord_volunteer_info_channel", value = ctx.field_value("discord_volunteer_info_channel").unwrap_or(
                                &event.discord_volunteer_info_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_organizer_channel", &mut errors, html! {
                            label(for = "discord_organizer_channel") : "Discord Organizer Channel ID";
                            input(type = "text", id = "discord_organizer_channel", name = "discord_organizer_channel", value = ctx.field_value("discord_organizer_channel").unwrap_or(
                                &event.discord_organizer_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_scheduling_channel", &mut errors, html! {
                            label(for = "discord_scheduling_channel") : "Discord Scheduling Channel ID";
                            input(type = "text", id = "discord_scheduling_channel", name = "discord_scheduling_channel", value = ctx.field_value("discord_scheduling_channel").unwrap_or(
                                &event.discord_scheduling_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("discord_async_channel", &mut errors, html! {
                            label(for = "discord_async_channel") : "Discord Async Channel ID";
                            input(type = "text", id = "discord_async_channel", name = "discord_async_channel", value = ctx.field_value("discord_async_channel").unwrap_or(
                                &event.discord_async_channel.map(|c| c.get().to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("speedgaming_slug", &mut errors, html! {
                            label(for = "speedgaming_slug") : "SpeedGaming Slug";
                            input(type = "text", id = "speedgaming_slug", name = "speedgaming_slug", value = ctx.field_value("speedgaming_slug").unwrap_or(
                                &event.speedgaming_slug.clone().unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
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
                            label(for = "listed") : "Listed";
                            label(class = "help") : " (Show this event on the main page)";
                        });

                        : form_field("emulator_settings_reminder", &mut errors, html! {
                            input(type = "checkbox", id = "emulator_settings_reminder", name = "emulator_settings_reminder", checked? = ctx.field_value("emulator_settings_reminder").map_or(event.emulator_settings_reminder, |value| value == "on"));
                            label(for = "emulator_settings_reminder") : "Emulator Settings Reminder";
                        });

                        : form_field("prevent_late_joins", &mut errors, html! {
                            input(type = "checkbox", id = "prevent_late_joins", name = "prevent_late_joins", checked? = ctx.field_value("prevent_late_joins").map_or(event.prevent_late_joins, |value| value == "on"));
                            label(for = "prevent_late_joins") : "Prevent Late Joins";
                            label(class = "help") : " (Block joining races after they start)";
                        });

                        : form_field("fpa_enabled", &mut errors, html! {
                            input(type = "checkbox", id = "fpa_enabled", name = "fpa_enabled", checked? = ctx.field_value("fpa_enabled").map_or(event.fpa_enabled, |value| value == "on"));
                            label(for = "fpa_enabled") : "FPA Enabled";
                            label(class = "help") : " (Announce fair play agreement when official race rooms open)";
                        });

                        : form_field("rando_version_json", &mut errors, html! {
                            label(for = "rando_version_json") : "Randomizer Version (JSON)";
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

                        h3 : "Additional Settings";

                        : form_field("enter_url", &mut errors, html! {
                            label(for = "enter_url") : "Enter URL";
                            input(type = "url", id = "enter_url", name = "enter_url", value = ctx.field_value("enter_url").unwrap_or(
                                &event.enter_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (URL for signing up/entering the event)";
                        });

                        : form_field("teams_url", &mut errors, html! {
                            label(for = "teams_url") : "Teams URL";
                            input(type = "url", id = "teams_url", name = "teams_url", value = ctx.field_value("teams_url").unwrap_or(
                                &event.teams_url.as_ref().map(|u| u.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (External URL for teams information)";
                        });

                        : form_field("challonge_community", &mut errors, html! {
                            label(for = "challonge_community") : "Challonge Community";
                            input(type = "text", id = "challonge_community", name = "challonge_community", value = ctx.field_value("challonge_community").unwrap_or(
                                &event.challonge_community.clone().unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("team_config", &mut errors, html! {
                            label(for = "team_config") : "Team Configuration";
                            select(id = "team_config", name = "team_config", style = "width: 100%; max-width: 600px;") {
                                option(value = "solo", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::Solo), |v| v == "solo")) : "Solo";
                                option(value = "coop", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::CoOp), |v| v == "coop")) : "Co-op";
                                option(value = "tfbcoop", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::TfbCoOp), |v| v == "tfbcoop")) : "TFB Co-op";
                                option(value = "pictionary", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::Pictionary), |v| v == "pictionary")) : "Pictionary";
                                option(value = "multiworld", selected? = ctx.field_value("team_config").map_or(matches!(event.team_config, TeamConfig::Multiworld), |v| v == "multiworld")) : "Multiworld";
                            }
                        });

                        : form_field("language", &mut errors, html! {
                            label(for = "language") : "Language";
                            select(id = "language", name = "language", style = "width: 100%; max-width: 600px;") {
                                option(value = "en", selected? = ctx.field_value("language").map_or(event.language == English, |v| v == "en")) : "English";
                                option(value = "fr", selected? = ctx.field_value("language").map_or(event.language == French, |v| v == "fr")) : "French";
                                option(value = "de", selected? = ctx.field_value("language").map_or(event.language == German, |v| v == "de")) : "German";
                                option(value = "pt", selected? = ctx.field_value("language").map_or(event.language == Portuguese, |v| v == "pt")) : "Portuguese";
                            }
                        });

                        : form_field("default_game_count", &mut errors, html! {
                            label(for = "default_game_count") : "Default Game Count";
                            input(type = "number", id = "default_game_count", name = "default_game_count", min = "1", value = ctx.field_value("default_game_count").unwrap_or(&event.default_game_count.to_string()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("open_stream_delay", &mut errors, html! {
                            label(for = "open_stream_delay") : "Open Stream Delay";
                            input(type = "text", id = "open_stream_delay", name = "open_stream_delay", value = ctx.field_value("open_stream_delay").unwrap_or(&unparse_duration(event.open_stream_delay)), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Format: '15s')";
                        });

                        : form_field("invitational_stream_delay", &mut errors, html! {
                            label(for = "invitational_stream_delay") : "Invitational Stream Delay";
                            input(type = "text", id = "invitational_stream_delay", name = "invitational_stream_delay", value = ctx.field_value("invitational_stream_delay").unwrap_or(&unparse_duration(event.invitational_stream_delay)), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Format: '30s')";
                        });

                        : form_field("hide_teams_tab", &mut errors, html! {
                            input(type = "checkbox", id = "hide_teams_tab", name = "hide_teams_tab", checked? = ctx.field_value("hide_teams_tab").map_or(event.hide_teams_tab, |value| value == "on"));
                            label(for = "hide_teams_tab") : "Hide Teams Tab";
                        });

                        : form_field("hide_races_tab", &mut errors, html! {
                            input(type = "checkbox", id = "hide_races_tab", name = "hide_races_tab", checked? = ctx.field_value("hide_races_tab").map_or(event.hide_races_tab, |value| value == "on"));
                            label(for = "hide_races_tab") : "Hide Races Tab";
                        });

                        : form_field("show_qualifier_times", &mut errors, html! {
                            input(type = "checkbox", id = "show_qualifier_times", name = "show_qualifier_times", checked? = ctx.field_value("show_qualifier_times").map_or(event.show_qualifier_times, |value| value == "on"));
                            label(for = "show_qualifier_times") : "Show Qualifier Times";
                        });

                        : form_field("swiss_standings", &mut errors, html! {
                            input(type = "checkbox", id = "swiss_standings", name = "swiss_standings", checked? = ctx.field_value("swiss_standings").map_or(event.swiss_standings, |value| value == "on"));
                            label(for = "swiss_standings") : "Show Swiss Standings Tab";
                        });

                        : form_field("automated_asyncs", &mut errors, html! {
                            input(type = "checkbox", id = "automated_asyncs", name = "automated_asyncs", checked? = ctx.field_value("automated_asyncs").map_or(event.automated_asyncs, |value| value == "on"));
                            label(for = "automated_asyncs") : "Use automated Discord threads for qualifier asyncs";
                            label(class = "help") : " (When enabled, qualifier requests create private Discord threads with READY/countdown/FINISH buttons)";
                        });

                        : form_field("async_start_delay", &mut errors, html! {
                            label(for = "async_start_delay") : "Force-Start Delay (minutes)";
                            input(type = "number", id = "async_start_delay", name = "async_start_delay", min = "0", value = ctx.field_value("async_start_delay").unwrap_or(
                                &event.async_start_delay.map(|d| d.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 200px;");
                            label(class = "help") : " (After seed is distributed, auto-start after this many minutes. Leave empty to disable.)";
                        });

                        : form_field("show_opt_out", &mut errors, html! {
                            input(type = "checkbox", id = "show_opt_out", name = "show_opt_out", checked? = ctx.field_value("show_opt_out").map_or(event.show_opt_out, |value| value == "on"));
                            label(for = "show_opt_out") : "Show Opt-Out";
                        });

                        : form_field("force_custom_role_binding", &mut errors, html! {
                            input(type = "checkbox", id = "force_custom_role_binding", name = "force_custom_role_binding", checked? = ctx.field_value("force_custom_role_binding").map_or(event.force_custom_role_binding, |value| value == "on"));
                            label(for = "force_custom_role_binding") : "Use event-specific volunteer roles";
                            label(class = "help") : " (When enabled, uses event-specific role bindings. When disabled, uses game-level volunteer roles.)";
                        });

                        h3 : "Racetime Bot Configuration";

                        : form_field("racetime_goal_slug", &mut errors, html! {
                            label(for = "racetime_goal_slug") : "Goal Slug";
                            input(type = "text", id = "racetime_goal_slug", name = "racetime_goal_slug", value = ctx.field_value("racetime_goal_slug").unwrap_or_else(|| event.racetime_goal_slug.as_deref().unwrap_or("")), style = "width: 100%; max-width: 600px;", placeholder = "Exact goal string on racetime.gg (empty = no goal)");
                        });

                        : form_field("draft_kind", &mut errors, html! {
                            label(for = "draft_kind") : "Draft Kind";
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
                            label(for = "draft_config") : "Draft Config JSON";
                            textarea(id = "draft_config", name = "draft_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("draft_config").unwrap_or(&draft_config_string);
                            }
                            label(class = "help") : " (JSON configuration for generic draft modes. Leave empty if not applicable.)";
                        });

                        : form_field("qualifier_score_kind", &mut errors, html! {
                            label(for = "qualifier_score_kind") : "Qualifier Score Kind";
                            select(id = "qualifier_score_kind", name = "qualifier_score_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("qualifier_score_kind").map_or(event.qualifier_score_kind_str.is_none(), |v| v.is_empty())) : "None";
                                @for (slug, label) in &[
                                    ("standard", "Standard"),
                                    ("sgl_2023_online", "SGL 2023 Online"),
                                    ("sgl_2024_online", "SGL 2024 Online"),
                                    ("sgl_2025_online", "SGL 2025 Online"),
                                    ("twwr_miniblins26", "TWWR Miniblins 26"),
                                ] {
                                    option(value = slug, selected? = ctx.field_value("qualifier_score_kind").map_or(event.qualifier_score_kind_str.as_deref() == Some(slug), |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("is_single_race", &mut errors, html! {
                            input(type = "checkbox", id = "is_single_race", name = "is_single_race", checked? = ctx.field_value("is_single_race").map_or(event.is_single_race, |value| value == "on"));
                            label(for = "is_single_race") : "Single Race Event";
                        });

                        : form_field("hide_entrants", &mut errors, html! {
                            input(type = "checkbox", id = "hide_entrants", name = "hide_entrants", checked? = ctx.field_value("hide_entrants").map_or(event.hide_entrants, |value| value == "on"));
                            label(for = "hide_entrants") : "Hide Entrants";
                        });

                        : form_field("start_delay", &mut errors, html! {
                            label(for = "start_delay") : "Start Delay (seconds)";
                            input(type = "number", id = "start_delay", name = "start_delay", value = ctx.field_value("start_delay").unwrap_or(&event.start_delay.to_string()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("start_delay_open", &mut errors, html! {
                            label(for = "start_delay_open") : "Start Delay Open (seconds)";
                            input(type = "text", id = "start_delay_open", name = "start_delay_open", value = ctx.field_value("start_delay_open").unwrap_or(
                                &event.start_delay_open.map(|d| d.to_string()).unwrap_or_default()
                            ), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Leave empty to use same as Start Delay)";
                        });

                        : form_field("restrict_chat_in_qualifiers", &mut errors, html! {
                            input(type = "checkbox", id = "restrict_chat_in_qualifiers", name = "restrict_chat_in_qualifiers", checked? = ctx.field_value("restrict_chat_in_qualifiers").map_or(event.restrict_chat_in_qualifiers, |value| value == "on"));
                            label(for = "restrict_chat_in_qualifiers") : "Restrict Chat in Qualifiers";
                        });

                        : form_field("preroll_mode", &mut errors, html! {
                            label(for = "preroll_mode") : "Preroll Mode";
                            select(id = "preroll_mode", name = "preroll_mode", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("none", "None"), ("short", "Short"), ("medium", "Medium"), ("long", "Long")] {
                                    option(value = val, selected? = ctx.field_value("preroll_mode").map_or(&*event.preroll_mode == *val, |v| v == *val)) : *label;
                                }
                            }
                        });

                        : form_field("spoiler_unlock", &mut errors, html! {
                            label(for = "spoiler_unlock") : "Spoiler Log Unlock";
                            select(id = "spoiler_unlock", name = "spoiler_unlock", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("never", "Never"), ("after", "After race"), ("immediately", "Immediately")] {
                                    option(value = val, selected? = ctx.field_value("spoiler_unlock").map_or(&*event.spoiler_unlock == *val, |v| v == *val)) : *label;
                                }
                            }
                        });

                        : form_field("is_custom_goal", &mut errors, html! {
                            input(type = "checkbox", id = "is_custom_goal", name = "is_custom_goal", checked? = ctx.field_value("is_custom_goal").map_or(event.is_custom_goal, |value| value == "on"));
                            label(for = "is_custom_goal") : "Is Custom Goal";
                            label(class = "help") : " (When enabled, the racetime.gg goal is a custom goal rather than a standard one.)";
                        });

                        : form_field("startgg_double_rr", &mut errors, html! {
                            input(type = "checkbox", id = "startgg_double_rr", name = "startgg_double_rr", checked? = ctx.field_value("startgg_double_rr").map_or(event.startgg_double_rr, |value| value == "on"));
                            label(for = "startgg_double_rr") : "start.gg double round-robin mode";
                            label(class = "help") : " (When enabled with a start.gg round-robin best-of-1 bracket, HTH schedules 2 games per set and force-closes the start.gg set after both are played.)";
                        });

                        : form_field("is_live_event", &mut errors, html! {
                            input(type = "checkbox", id = "is_live_event", name = "is_live_event", checked? = ctx.field_value("is_live_event").map_or(event.is_live_event, |value| value == "on"));
                            label(for = "is_live_event") : "Is Live Event";
                            label(class = "help") : " (When enabled, rooms are created for scheduled races. Used for SpeedGaming live broadcasts.)";
                        });

                        h3 : "Seed Generation";

                        : form_field("seed_gen_type", &mut errors, html! {
                            label(for = "seed_gen_type") : "Seed Gen Type";
                            select(id = "seed_gen_type", name = "seed_gen_type", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("seed_gen_type").map_or(seed_gen_type_str.is_none(), |v| v.is_empty())) : "None (manual / external)";
                                @for (val, label) in &[
                                    ("alttpr_dr", "ALTTPR Door Rando"),
                                    ("alttpr_avianart", "ALTTPR Avianart"),
                                    ("ootr", "OoTR"),
                                    ("ootr_tfb", "OoTR Triforce Blitz"),
                                    ("ootr_rsl", "OoTR RSL"),
                                    ("twwr", "The Wind Waker Randomizer"),
                                    ("mmr", "MMR"),
                                ] {
                                    option(value = val, selected? = ctx.field_value("seed_gen_type").map_or(seed_gen_type_str.as_deref() == Some(val), |v| v == *val)) : *label;
                                }
                            }
                            label(class = "help") : " (Determines how seeds are generated for races in this event.)";
                        });

                        : form_field("seed_config", &mut errors, html! {
                            label(for = "seed_config") : "Seed Config JSON";
                            textarea(id = "seed_config", name = "seed_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("seed_config").unwrap_or(&seed_config_string);
                            }
                            label(class = "help") : " (JSON config for the seed gen type. Leave empty if not applicable.)";
                            details {
                                summary : "Examples by seed gen type";
                                pre(style = "font-size: 13px; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                    : "// alttpr_dr — boothisman.de presets:\n{\"source\": \"boothisman\"}\n\n// alttpr_dr — teams agree on settings via custom_choices:\n{\"source\": \"mutual_choices\"}\n\n// alttpr_dr — mystery pool from a weights URL:\n{\"source\": \"mystery_pool\", \"mystery_weights_url\": \"https://example.com/weights.yaml\"}\n\n// twwr — default permalink:\n{\"permalink\": \"MS45MC4wAEEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"}";
                                }
                            }
                        });
                    }, errors.clone(), "Save Basic Info");
                    
                    h3 : "Enter Flow Configuration";
                    
                    : full_form(uri!(update_enter_flow(event.series, &*event.event)), csrf, html! {
                        : form_field("enter_flow_json", &mut errors, html! {
                            label(for = "enter_flow_json") : "Enter Flow JSON";
                            textarea(id = "enter_flow_json", name = "enter_flow_json", rows = "10", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("enter_flow_json").unwrap_or(&enter_flow_string);
                            }
                            p(class = "help") : "Configure the signup requirements as JSON. Leave empty for no requirements.";
                            
                            details {
                                summary : "Example enter_flow configurations";
                                div(style = "margin-top: 10px; padding: 15px; background: #f5f5f5; border-radius: 6px; border: 1px solid #ddd;") {
                                    h4(style = "margin-top: 0; color: #333;") : "Basic Discord Account Requirement:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "discord"
    }
  ]
}"#;
                                    }
                                    
                                    h4(style = "margin-top: 20px; color: #333;") : "Multiple Requirements with Deadline:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "discord"
    },
    {
      "type": "raceTime"
    },
    {
      "type": "startGG"
    }
  ],
  "closes": "2024-01-15T23:59:59Z"
}"#;
                                    }
                                    
                                    h4(style = "margin-top: 20px; color: #333;") : "Custom Text Field Requirement:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "textField",
      "label": "What's your favorite Zelda game?",
      "long": false,
      "regex": ".*",
      "regexErrorMessages": [],
      "fallbackErrorMessage": "Please provide an answer"
    }
  ]
}"#;
                                    }

                                    h4(style = "margin-top: 20px; color: #333;") : "Discord Guild with Specific Role:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "discordGuild",
      "name": "My Discord Server",
      "roleId": "123456789012345678"
    }
  ]
}"#;
                                    }

                                    h4(style = "margin-top: 20px; color: #333;") : "Custom Boolean Choice (stored in custom_choices by key):";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "booleanChoice",
      "key": "hard_mode",
      "label": "Difficulty: Hard"
    }
  ]
}"#;
                                    }

                                    h4(style = "margin-top: 20px; color: #333;") : "Qualifier Placement Cutoff:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "qualifierPlacement",
      "numPlayers": 16,
      "minRaces": 1,
      "needFinish": false,
      "excludePlayers": 0
    }
  ]
}"#;
                                    }

                                    h4(style = "margin-top: 20px; color: #333;") : "External / Manual Requirement:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "external",
      "html": "Please fill out <a href=\"https://example.com/form\">this form</a>.",
      "text": "Please fill out the registration form.",
      "blocksSubmit": true
    }
  ]
}"#;
                                    }
                                }
                            }
                        });
                    }, errors.clone(), "Save Enter Flow");
                    
                    h3 : "Organizer Management";
                    
                    : full_form(uri!(add_organizer(event.series, &*event.event)), csrf, html! {
                        : form_field("organizer", &mut errors, html! {
                            label(for = "organizer") : "Add Organizer";
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
                }
            }
        } else {
            html! {
                article {
                    p : "You must be a global admin to access this page.";
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
    
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Setup — {}", event.display_name), html! {
        : header;
        : content;
        script(src = static_url!("user-search.js")) {}
    }).await?)
}

#[rocket::get("/event/<series>/<event>/setup")]
pub(crate) async fn get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(setup_form(transaction, me, uri, csrf.as_ref(), event_data, Context::default()).await?)
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
    speedgaming_slug: Option<String>,
    listed: bool,
    emulator_settings_reminder: bool,
    prevent_late_joins: bool,
    fpa_enabled: bool,
    rando_version_json: Option<String>,
    enter_url: Option<String>,
    teams_url: Option<String>,
    challonge_community: Option<String>,
    team_config: String,
    language: String,
    default_game_count: i16,
    open_stream_delay: String,
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
    qualifier_score_kind: Option<String>,
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
}

#[rocket::post("/event/<series>/<event>/setup", data = "<form>")]
pub(crate) async fn post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, SetupForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be a global admin to configure this event."));
        }
        
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?)
        } else {
            // Parse start time (datetime-local sends YYYY-MM-DDTHH:MM format)
            let start = if let Some(start_str) = &value.start {
                if !start_str.is_empty() {
                    match NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid start time format"));
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
                        Ok(naive_dt) => Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid end time format"));
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
                            form.context.push_error(form::Error::validation("Invalid URL format"));
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
                            form.context.push_error(form::Error::validation("Invalid video URL format"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            let discord_invite_url = if let Some(discord_invite_url_str) = &value.discord_invite_url {
                if !discord_invite_url_str.is_empty() {
                    match discord_invite_url_str.parse::<Url>() {
                        Ok(u) => Some(u),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord invite URL format"));
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
                            form.context.push_error(form::Error::validation("Invalid Discord guild ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            let discord_race_room_channel = if let Some(channel_str) = &value.discord_race_room_channel {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            let discord_race_results_channel = if let Some(channel_str) = &value.discord_race_results_channel {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            
            let discord_volunteer_info_channel = if let Some(channel_str) = &value.discord_volunteer_info_channel {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_organizer_channel = if let Some(channel_str) = &value.discord_organizer_channel {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord organizer channel ID"));
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let discord_scheduling_channel = if let Some(channel_str) = &value.discord_scheduling_channel {
                if !channel_str.is_empty() {
                    match channel_str.parse::<u64>() {
                        Ok(id) => Some(ChannelId::new(id)),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord scheduling channel ID"));
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
                            form.context.push_error(form::Error::validation("Invalid Discord async channel ID"));
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
            let short_name = value.short_name.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let speedgaming_slug = value.speedgaming_slug.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let challonge_community = value.challonge_community.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

            // Parse new racetime bot config fields
            let racetime_goal_slug = value.racetime_goal_slug.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let draft_kind = value.draft_kind.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let qualifier_score_kind = value.qualifier_score_kind.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
            let seed_gen_type = value.seed_gen_type.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

            let draft_config_json: Option<serde_json::Value> = if let Some(ref dc_str) = value.draft_config {
                if dc_str.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str(dc_str) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            form.context.push_error(form::Error::validation(format!("Invalid draft config JSON: {e}")).with_name("draft_config"));
                            None
                        }
                    }
                }
            } else {
                None
            };

            let seed_config_json: Option<serde_json::Value> = if let Some(ref sc_str) = value.seed_config {
                if sc_str.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str(sc_str) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            form.context.push_error(form::Error::validation(format!("Invalid seed config JSON: {e}")).with_name("seed_config"));
                            None
                        }
                    }
                }
            } else {
                None
            };

            let start_delay_open: Option<i32> = if let Some(ref sdo_str) = value.start_delay_open {
                if sdo_str.trim().is_empty() {
                    None
                } else {
                    match sdo_str.trim().parse::<i32>() {
                        Ok(v) => Some(v),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid start delay open value").with_name("start_delay_open"));
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
                            form.context.push_error(form::Error::validation("Invalid enter URL format"));
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
                            form.context.push_error(form::Error::validation("Invalid teams URL format"));
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
                    form.context.push_error(form::Error::validation("Invalid team configuration"));
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
                    form.context.push_error(form::Error::validation("Invalid language"));
                    English // default fallback
                }
            };

            // Parse durations
            let open_stream_delay = if let Some(time) = parse_duration(&value.open_stream_delay, None) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'.").with_name("open_stream_delay"));
                None
            };

            let invitational_stream_delay = if let Some(time) = parse_duration(&value.invitational_stream_delay, None) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'.").with_name("invitational_stream_delay"));
                None
            };

            let rando_version = match value.rando_version_json.as_deref() {
                None | Some("") => Ok(None),
                Some(s) => match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(json) => Ok(Some(json)),
                    Err(_) => {
                        form.context.push_error(form::Error::validation("Invalid JSON format for Randomizer Version").with_name("rando_version_json"));
                        Err(())
                    }
                },
            };

            if form.context.errors().next().is_some() {
                return Ok(RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?));
            }

            // Update database
            sqlx::query!(r#"
                UPDATE events
                SET display_name = $1, start = $2, end_time = $3, url = $4, video_url = $5,
                    discord_invite_url = $6, discord_guild = $7, discord_race_room_channel = $8,
                    discord_race_results_channel = $9, discord_volunteer_info_channel = $10,
                    discord_organizer_channel = $11, discord_scheduling_channel = $12,
                    discord_async_channel = $13, short_name = $14, speedgaming_slug = $15,
                    listed = $16, emulator_settings_reminder = $17,
                    prevent_late_joins = $18, enter_url = $19, teams_url = $20,
                    challonge_community = $21, team_config = $22, language = $23,
                    default_game_count = $24, open_stream_delay = $25, invitational_stream_delay = $26,
                    hide_teams_tab = $27, hide_races_tab = $28, show_qualifier_times = $29,
                    automated_asyncs = $30, show_opt_out = $31, force_custom_role_binding = $32,
                    racetime_goal_slug = $35, draft_kind = $36, draft_config = $37,
                    qualifier_score_kind = $38, is_single_race = $39, hide_entrants = $40,
                    start_delay = $41, start_delay_open = $42, restrict_chat_in_qualifiers = $43,
                    async_start_delay = $44, startgg_double_rr = $45,
                    preroll_mode = $46, spoiler_unlock = $47, is_custom_goal = $48,
                    fpa_enabled = $49, swiss_standings = $50, rando_version = $51,
                    is_live_event = $52, seed_gen_type = $53, seed_config = $54
                WHERE series = $33 AND event = $34
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
                speedgaming_slug,
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

            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?)
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddOrganizerForm {
    #[field(default = String::new())]
    csrf: String,
    organizer: String,
}

#[rocket::post("/event/<series>/<event>/setup/add-organizer", data = "<form>")]
pub(crate) async fn add_organizer(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, AddOrganizerForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be a global admin to configure this event."));
        }
        
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?)
        } else {
            // Find user by ID
            let organizer_id: Id<Users> = value.organizer.parse().map_err(|_| StatusOrError::Status(Status::NotFound))?;
            let user = sqlx::query!(r#"
                SELECT id
                FROM users
                WHERE id = $1
            "#, organizer_id as _)
            .fetch_optional(&mut *transaction).await?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
            
            // Add organizer
            sqlx::query!(r#"
                INSERT INTO organizers (series, event, organizer) 
                VALUES ($1, $2, $3) 
                ON CONFLICT DO NOTHING
            "#, event_data.series as _, &event_data.event, user.id)
            .execute(&mut *transaction).await?;
            
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?)
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveOrganizerForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/setup/remove-organizer/<organizer>", data = "<form>")]
pub(crate) async fn remove_organizer(pool: &State<PgPool>, me: User, _uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, organizer: Id<Users>, form: Form<Contextual<'_, RemoveOrganizerForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    Ok(if form.value.is_some() {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be a global admin to configure this event."));
        }
        
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(setup_form(transaction, Some(me), _uri, csrf.as_ref(), event_data, form.context).await?)
        } else {
            // Remove organizer
            sqlx::query!(r#"
                DELETE FROM organizers 
                WHERE series = $1 AND event = $2 AND organizer = $3
            "#, event_data.series as _, &event_data.event, i64::from(organizer))
            .execute(&mut *transaction).await?;
            
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(setup_form(transaction, Some(me), _uri, csrf.as_ref(), event_data, form.context).await?)
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct UpdateEnterFlowForm {
    #[field(default = String::new())]
    csrf: String,
    enter_flow_json: String,
}

#[rocket::post("/event/<series>/<event>/setup/update-enter-flow", data = "<form>")]
pub(crate) async fn update_enter_flow(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, UpdateEnterFlowForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    Ok(if let Some(ref value) = form.value {
        if event_data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be a global admin to configure this event."));
        }
        
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?)
        } else {
            // Parse enter_flow JSON and validate against Flow struct
            let enter_flow_json = if !value.enter_flow_json.trim().is_empty() {
                match serde_json::from_str::<enter::Flow>(&value.enter_flow_json) {
                    Ok(_) => Some(serde_json::from_str::<serde_json::Value>(&value.enter_flow_json).expect("already validated as JSON")),
                    Err(e) => {
                        form.context.push_error(form::Error::validation(format!("Invalid enter flow: {e}")));
                        None
                    }
                }
            } else {
                None
            };
            
            // Update database
            sqlx::query!(r#"
                UPDATE events 
                SET enter_flow = $1
                WHERE series = $2 AND event = $3
            "#,
                enter_flow_json,
                event_data.series as _,
                &event_data.event
            ).execute(&mut *transaction).await?;
            
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await?)
    })
}

#[rocket::get("/event/setup/search-users?<query>")]
pub(crate) async fn search_users(pool: &State<PgPool>, query: Option<&str>) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let results = search_users_internal(&mut transaction, query).await?;
    transaction.commit().await?;
    Ok(RawText(serde_json::to_string(&results)?))
}

async fn search_users_internal(transaction: &mut Transaction<'_, Postgres>, query: Option<&str>) -> Result<Vec<UserSearchResult>, event::Error> {
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
    
    Ok(rows.into_iter().map(|row| UserSearchResult {
        id: row.id,
        display_name: match row.display_source {
            DisplaySource::RaceTime => row.racetime_display_name.unwrap_or_default(),
            DisplaySource::Discord => row.discord_display_name.unwrap_or_default(),
        },
        racetime_id: row.racetime_id,
        discord_username: row.discord_username,
    }).collect())
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

fn create_form_content(me: &Option<User>, _uri: &Origin<'_>, csrf: Option<&CsrfToken>, ctx: Context<'_>) -> RawHtml<String> {
    if let Some(me) = me {
        if me.is_global_admin() {
            let mut errors = ctx.errors().collect_vec();
            html! {
                article {
                    h2 : "Create New Event";

                    : full_form(uri!(create_post), csrf, html! {
                        : form_field("series", &mut errors, html! {
                            label(for = "series") : "Series";
                            select(id = "series", name = "series", style = "width: 100%; max-width: 600px;") {
                                @for series in all::<Series>() {
                                    option(value = series.slug(), selected? = ctx.field_value("series").map_or(false, |v| v == series.slug())) : series.display_name();
                                }
                            }
                        });

                        : form_field("event", &mut errors, html! {
                            label(for = "event") : "Event Slug";
                            input(type = "text", id = "event", name = "event", value = ctx.field_value("event").unwrap_or(&String::new()), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (e.g. \"2025\", \"s1\")";
                        });

                        : form_field("display_name", &mut errors, html! {
                            label(for = "display_name") : "Display Name";
                            input(type = "text", id = "display_name", name = "display_name", value = ctx.field_value("display_name").unwrap_or(&String::new()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("team_config", &mut errors, html! {
                            label(for = "team_config") : "Team Configuration";
                            select(id = "team_config", name = "team_config", style = "width: 100%; max-width: 600px;") {
                                option(value = "solo", selected? = ctx.field_value("team_config").map_or(true, |v| v == "solo")) : "Solo";
                                option(value = "coop", selected? = ctx.field_value("team_config").map_or(false, |v| v == "coop")) : "Co-op";
                                option(value = "tfbcoop", selected? = ctx.field_value("team_config").map_or(false, |v| v == "tfbcoop")) : "TFB Co-op";
                                option(value = "pictionary", selected? = ctx.field_value("team_config").map_or(false, |v| v == "pictionary")) : "Pictionary";
                                option(value = "multiworld", selected? = ctx.field_value("team_config").map_or(false, |v| v == "multiworld")) : "Multiworld";
                            }
                        });

                        : form_field("language", &mut errors, html! {
                            label(for = "language") : "Language";
                            select(id = "language", name = "language", style = "width: 100%; max-width: 600px;") {
                                option(value = "en", selected? = ctx.field_value("language").map_or(true, |v| v == "en")) : "English";
                                option(value = "fr", selected? = ctx.field_value("language").map_or(false, |v| v == "fr")) : "French";
                                option(value = "de", selected? = ctx.field_value("language").map_or(false, |v| v == "de")) : "German";
                                option(value = "pt", selected? = ctx.field_value("language").map_or(false, |v| v == "pt")) : "Portuguese";
                            }
                        });

                        : form_field("listed", &mut errors, html! {
                            input(type = "checkbox", id = "listed", name = "listed", checked? = ctx.field_value("listed").map_or(false, |value| value == "on"));
                            label(for = "listed") : "Listed";
                            label(class = "help") : " (Show this event on the main page)";
                        });

                        h3 : "Racetime Bot Configuration";

                        : form_field("racetime_goal_slug", &mut errors, html! {
                            label(for = "racetime_goal_slug") : "Goal Slug";
                            input(type = "text", id = "racetime_goal_slug", name = "racetime_goal_slug", value = ctx.field_value("racetime_goal_slug").unwrap_or(""), style = "width: 100%; max-width: 600px;", placeholder = "Exact goal string on racetime.gg (empty = no goal)");
                        });

                        : form_field("draft_kind", &mut errors, html! {
                            label(for = "draft_kind") : "Draft Kind";
                            select(id = "draft_kind", name = "draft_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("draft_kind").map_or(true, |v| v.is_empty())) : "None";
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
                                    option(value = slug, selected? = ctx.field_value("draft_kind").map_or(false, |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("draft_config", &mut errors, html! {
                            label(for = "draft_config") : "Draft Config JSON";
                            textarea(id = "draft_config", name = "draft_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("draft_config").unwrap_or(&String::new());
                            }
                            label(class = "help") : " (JSON configuration for generic draft modes. Leave empty if not applicable.)";
                        });

                        : form_field("qualifier_score_kind", &mut errors, html! {
                            label(for = "qualifier_score_kind") : "Qualifier Score Kind";
                            select(id = "qualifier_score_kind", name = "qualifier_score_kind", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("qualifier_score_kind").map_or(true, |v| v.is_empty())) : "None";
                                @for (slug, label) in &[
                                    ("standard", "Standard"),
                                    ("sgl_2023_online", "SGL 2023 Online"),
                                    ("sgl_2024_online", "SGL 2024 Online"),
                                    ("sgl_2025_online", "SGL 2025 Online"),
                                    ("twwr_miniblins26", "TWWR Miniblins 26"),
                                ] {
                                    option(value = slug, selected? = ctx.field_value("qualifier_score_kind").map_or(false, |v| v == *slug)) : *label;
                                }
                            }
                        });

                        : form_field("is_single_race", &mut errors, html! {
                            input(type = "checkbox", id = "is_single_race", name = "is_single_race", checked? = ctx.field_value("is_single_race").map_or(false, |value| value == "on"));
                            label(for = "is_single_race") : "Single Race Event";
                        });

                        : form_field("hide_entrants", &mut errors, html! {
                            input(type = "checkbox", id = "hide_entrants", name = "hide_entrants", checked? = ctx.field_value("hide_entrants").map_or(false, |value| value == "on"));
                            label(for = "hide_entrants") : "Hide Entrants";
                        });

                        : form_field("start_delay", &mut errors, html! {
                            label(for = "start_delay") : "Start Delay (seconds)";
                            input(type = "number", id = "start_delay", name = "start_delay", value = ctx.field_value("start_delay").unwrap_or(&"15".to_owned()), style = "width: 100%; max-width: 600px;");
                        });

                        : form_field("start_delay_open", &mut errors, html! {
                            label(for = "start_delay_open") : "Start Delay Open (seconds)";
                            input(type = "text", id = "start_delay_open", name = "start_delay_open", value = ctx.field_value("start_delay_open").unwrap_or(&String::new()), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Leave empty to use same as Start Delay)";
                        });

                        : form_field("restrict_chat_in_qualifiers", &mut errors, html! {
                            input(type = "checkbox", id = "restrict_chat_in_qualifiers", name = "restrict_chat_in_qualifiers", checked? = ctx.field_value("restrict_chat_in_qualifiers").map_or(false, |value| value == "on"));
                            label(for = "restrict_chat_in_qualifiers") : "Restrict Chat in Qualifiers";
                        });

                        : form_field("preroll_mode", &mut errors, html! {
                            label(for = "preroll_mode") : "Preroll Mode";
                            select(id = "preroll_mode", name = "preroll_mode", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("none", "None"), ("short", "Short"), ("medium", "Medium"), ("long", "Long")] {
                                    option(value = val, selected? = ctx.field_value("preroll_mode").map_or(*val == "medium", |v| v == *val)) : *label;
                                }
                            }
                        });

                        : form_field("spoiler_unlock", &mut errors, html! {
                            label(for = "spoiler_unlock") : "Spoiler Log Unlock";
                            select(id = "spoiler_unlock", name = "spoiler_unlock", style = "width: 100%; max-width: 600px;") {
                                @for (val, label) in &[("never", "Never"), ("after", "After race"), ("immediately", "Immediately")] {
                                    option(value = val, selected? = ctx.field_value("spoiler_unlock").map_or(*val == "after", |v| v == *val)) : *label;
                                }
                            }
                        });

                        : form_field("is_custom_goal", &mut errors, html! {
                            input(type = "checkbox", id = "is_custom_goal", name = "is_custom_goal", checked? = ctx.field_value("is_custom_goal").map_or(true, |value| value == "on"));
                            label(for = "is_custom_goal") : "Is Custom Goal";
                            label(class = "help") : " (When enabled, the racetime.gg goal is a custom goal rather than a standard one.)";
                        });

                        : form_field("is_live_event", &mut errors, html! {
                            input(type = "checkbox", id = "is_live_event", name = "is_live_event", checked? = ctx.field_value("is_live_event").map_or(false, |value| value == "on"));
                            label(for = "is_live_event") : "Is Live Event";
                            label(class = "help") : " (When enabled, rooms are created for scheduled races. Used for SpeedGaming live broadcasts.)";
                        });

                        h3 : "Seed Generation";

                        : form_field("seed_gen_type", &mut errors, html! {
                            label(for = "seed_gen_type") : "Seed Gen Type";
                            select(id = "seed_gen_type", name = "seed_gen_type", style = "width: 100%; max-width: 600px;") {
                                option(value = "", selected? = ctx.field_value("seed_gen_type").map_or(true, |v| v.is_empty())) : "None (manual / external)";
                                @for (val, label) in &[
                                    ("alttpr_dr", "ALTTPR Door Rando"),
                                    ("alttpr_avianart", "ALTTPR Avianart"),
                                    ("ootr", "OoTR"),
                                    ("ootr_tfb", "OoTR Triforce Blitz"),
                                    ("ootr_rsl", "OoTR RSL"),
                                    ("twwr", "The Wind Waker Randomizer"),
                                    ("mmr", "MMR"),
                                ] {
                                    option(value = val, selected? = ctx.field_value("seed_gen_type").map_or(false, |v| v == *val)) : *label;
                                }
                            }
                            label(class = "help") : " (Determines how seeds are generated for races in this event.)";
                        });

                        : form_field("seed_config", &mut errors, html! {
                            label(for = "seed_config") : "Seed Config JSON";
                            textarea(id = "seed_config", name = "seed_config", rows = "6", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("seed_config").unwrap_or(&String::new());
                            }
                            label(class = "help") : " (JSON config for the seed gen type. Leave empty if not applicable.)";
                            details {
                                summary : "Examples by seed gen type";
                                pre(style = "font-size: 13px; background: #2d2d2d; color: #f8f8f2; padding: 12px; border-radius: 4px; overflow-x: auto;") {
                                    : "// alttpr_dr — boothisman.de presets:\n{\"source\": \"boothisman\"}\n\n// alttpr_dr — teams agree on settings via custom_choices:\n{\"source\": \"mutual_choices\"}\n\n// alttpr_dr — mystery pool from a weights URL:\n{\"source\": \"mystery_pool\", \"mystery_weights_url\": \"https://example.com/weights.yaml\"}\n\n// twwr — default permalink:\n{\"permalink\": \"MS45MC4wAEEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"}";
                                }
                            }
                        });
                    }, errors.clone(), "Create Event");
                }
            }
        } else {
            html! {
                article {
                    p : "You must be a global admin to access this page.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(create_get))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to access this page.";
                }
            }
        }
    }
}

#[rocket::get("/event/new")]
pub(crate) async fn create_get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let transaction = pool.begin().await?;
    let content = create_form_content(&me, &uri, csrf.as_ref(), Context::default());
    Ok(page(transaction, &me, &uri, PageStyle::default(), "Create New Event", content).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct CreateEventForm {
    #[field(default = String::new())]
    csrf: String,
    series: String,
    event: String,
    display_name: String,
    team_config: String,
    language: String,
    listed: bool,
    racetime_goal_slug: Option<String>,
    draft_kind: Option<String>,
    draft_config: Option<String>,
    qualifier_score_kind: Option<String>,
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
}

#[rocket::post("/event/new", data = "<form>")]
pub(crate) async fn create_post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, form: Form<Contextual<'_, CreateEventForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        if !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be a global admin to create events."));
        }

        // Parse series
        let series = match value.series.parse::<Series>() {
            Ok(s) => Some(s),
            Err(()) => {
                form.context.push_error(form::Error::validation("Invalid series.").with_name("series"));
                None
            }
        };

        // Validate event slug is non-empty
        if value.event.is_empty() {
            form.context.push_error(form::Error::validation("Event slug is required.").with_name("event"));
        }

        // Validate display name is non-empty
        if value.display_name.is_empty() {
            form.context.push_error(form::Error::validation("Display name is required.").with_name("display_name"));
        }

        // Parse team_config
        let team_config = match value.team_config.as_str() {
            "solo" => TeamConfig::Solo,
            "coop" => TeamConfig::CoOp,
            "tfbcoop" => TeamConfig::TfbCoOp,
            "pictionary" => TeamConfig::Pictionary,
            "multiworld" => TeamConfig::Multiworld,
            _ => {
                form.context.push_error(form::Error::validation("Invalid team configuration.").with_name("team_config"));
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
                form.context.push_error(form::Error::validation("Invalid language.").with_name("language"));
                English
            }
        };

        // Parse optional string fields
        let racetime_goal_slug = value.racetime_goal_slug.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let draft_kind = value.draft_kind.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let qualifier_score_kind = value.qualifier_score_kind.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });
        let seed_gen_type = value.seed_gen_type.as_ref().and_then(|s| if s.is_empty() { None } else { Some(s.clone()) });

        // Parse draft_config JSON
        let draft_config_json: Option<serde_json::Value> = if let Some(ref dc_str) = value.draft_config {
            if dc_str.trim().is_empty() {
                None
            } else {
                match serde_json::from_str(dc_str) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        form.context.push_error(form::Error::validation(format!("Invalid draft config JSON: {e}")).with_name("draft_config"));
                        None
                    }
                }
            }
        } else {
            None
        };

        // Parse seed_config JSON
        let seed_config_json: Option<serde_json::Value> = if let Some(ref sc_str) = value.seed_config {
            if sc_str.trim().is_empty() {
                None
            } else {
                match serde_json::from_str(sc_str) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        form.context.push_error(form::Error::validation(format!("Invalid seed config JSON: {e}")).with_name("seed_config"));
                        None
                    }
                }
            }
        } else {
            None
        };

        // Parse start_delay_open
        let start_delay_open: Option<i32> = if let Some(ref sdo_str) = value.start_delay_open {
            if sdo_str.trim().is_empty() {
                None
            } else {
                match sdo_str.trim().parse::<i32>() {
                    Ok(v) => Some(v),
                    Err(_) => {
                        form.context.push_error(form::Error::validation("Invalid start delay open value.").with_name("start_delay_open"));
                        None
                    }
                }
            }
        } else {
            None
        };

        if form.context.errors().next().is_some() {
            let me = Some(me);
            let transaction = pool.begin().await?;
            let content = create_form_content(&me, &uri, csrf.as_ref(), form.context);
            return Ok(RedirectOrContent::Content(page(transaction, &me, &uri, PageStyle::default(), "Create New Event", content).await?));
        }

        let series = series.expect("series should be valid if no errors");

        let mut transaction = pool.begin().await?;

        // Insert the new event
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

        transaction.commit().await?;
        RedirectOrContent::Redirect(Redirect::to(uri!(get(series, &*value.event))))
    } else {
        let me = Some(me);
        let transaction = pool.begin().await?;
        let content = create_form_content(&me, &uri, csrf.as_ref(), form.context);
        RedirectOrContent::Content(page(transaction, &me, &uri, PageStyle::default(), "Create New Event", content).await?)
    })
} 