use crate::{
    event::{Data, Tab},
    prelude::*,
    user::DisplaySource
};
use rocket::response::content::RawText;

async fn setup_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Setup, false).await?;
    
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
                            label(class = "help") : " (Format: '1:23:45' or '1h 23m 45s')";
                        });

                        : form_field("invitational_stream_delay", &mut errors, html! {
                            label(for = "invitational_stream_delay") : "Invitational Stream Delay";
                            input(type = "text", id = "invitational_stream_delay", name = "invitational_stream_delay", value = ctx.field_value("invitational_stream_delay").unwrap_or(&unparse_duration(event.invitational_stream_delay)), style = "width: 100%; max-width: 600px;");
                            label(class = "help") : " (Format: '1:23:45' or '1h 23m 45s')";
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

                        : form_field("show_opt_out", &mut errors, html! {
                            input(type = "checkbox", id = "show_opt_out", name = "show_opt_out", checked? = ctx.field_value("show_opt_out").map_or(event.show_opt_out, |value| value == "on"));
                            label(for = "show_opt_out") : "Show Opt-Out";
                        });

                        : form_field("force_custom_role_binding", &mut errors, html! {
                            input(type = "checkbox", id = "force_custom_role_binding", name = "force_custom_role_binding", checked? = ctx.field_value("force_custom_role_binding").map_or(event.force_custom_role_binding, |value| value == "on"));
                            label(for = "force_custom_role_binding") : "Use event-specific volunteer roles";
                            label(class = "help") : " (When enabled, uses event-specific role bindings. When disabled, uses game-level volunteer roles.)";
                        });
                    }, errors.clone(), "Save Basic Info");
                    
                    h3 : "Enter Flow Configuration";
                    
                    : full_form(uri!(update_enter_flow(event.series, &*event.event)), csrf, html! {
                        : form_field("enter_flow_json", &mut errors, html! {
                            label(for = "enter_flow_json") : "Enter Flow JSON";
                            textarea(id = "enter_flow_json", name = "enter_flow_json", rows = "10", style = "font-family: monospace; width: 100%; max-width: 800px;") {
                                : ctx.field_value("enter_flow_json").unwrap_or_else(|| {
                                    match &event.enter_flow {
                                        Some(_flow) => "{}", // Placeholder since we can't easily serialize the complex Flow
                                        None => "",
                                    }
                                });
                            }
                            p(class = "help") : "Configure the signup requirements as JSON. Leave empty for no requirements.";
                            
                            details {
                                summary : "Example enter_flow configurations";
                                div(style = "margin-top: 10px; padding: 15px; background: #f8f9fa; border-radius: 6px; border: 1px solid #e9ecef;") {
                                    h4(style = "margin-top: 0; color: #495057;") : "Basic Discord Account Requirement:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #ffffff; padding: 12px; border-radius: 4px; border: 1px solid #dee2e6; overflow-x: auto;") {
                                        : r#"{
  "requirements": [
    {
      "type": "discord"
    }
  ]
}"#;
                                    }
                                    
                                    h4(style = "margin-top: 20px; color: #495057;") : "Multiple Requirements with Deadline:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #ffffff; padding: 12px; border-radius: 4px; border: 1px solid #dee2e6; overflow-x: auto;") {
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
                                    
                                    h4(style = "margin-top: 20px; color: #495057;") : "Custom Text Field Requirement:";
                                    pre(style = "font-size: 14px; line-height: 1.4; background: #ffffff; padding: 12px; border-radius: 4px; border: 1px solid #dee2e6; overflow-x: auto;") {
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
                                }
                            }
                        });
                    }, errors.clone(), "Save Enter Flow");
                    
                    h3 : "Organizer Management";
                    
                    : full_form(uri!(add_organizer(event.series, &*event.event)), csrf, html! {
                        : form_field("organizer", &mut errors, html! {
                            label(for = "organizer") : "Add Organizer";
                            input(type = "text", id = "organizer", name = "organizer", autocomplete = "off", style = "width: 100%; max-width: 600px;");
                            div(id = "organizer-suggestions", class = "suggestions");
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
    show_opt_out: bool,
    force_custom_role_binding: bool,
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
                    show_opt_out = $30, force_custom_role_binding = $31
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
                value.show_opt_out,
                value.force_custom_role_binding,
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
            // Find user by display name
            let user = sqlx::query!(r#"
                SELECT id, display_source AS "display_source: DisplaySource", racetime_display_name, discord_display_name 
                FROM users 
                WHERE (display_source = $1 OR display_source = $2) AND (racetime_display_name = $3 OR discord_display_name = $3)
                LIMIT 1
            "#, DisplaySource::RaceTime as _, DisplaySource::Discord as _, &value.organizer)
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
            // Parse enter_flow JSON
            let enter_flow_json = if !value.enter_flow_json.trim().is_empty() {
                match serde_json::from_str::<serde_json::Value>(&value.enter_flow_json) {
                    Ok(json) => Some(json),
                    Err(_) => {
                        form.context.push_error(form::Error::validation("Invalid JSON format"));
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
    serializer.serialize_u64(u64::from(*id))
}

struct UserSearchRow {
    id: Id<Users>,
    display_source: DisplaySource,
    racetime_display_name: Option<String>,
    racetime_id: Option<String>,
    discord_display_name: Option<String>,
    discord_username: Option<String>,
} 