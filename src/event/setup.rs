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
        if event.organizers(&mut transaction).await?.contains(me) {
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
                        
                        : form_field("manual_reporting_with_breaks", &mut errors, html! {
                            input(type = "checkbox", id = "manual_reporting_with_breaks", name = "manual_reporting_with_breaks", checked? = ctx.field_value("manual_reporting_with_breaks").map_or(event.manual_reporting_with_breaks, |value| value == "on"));
                            label(for = "manual_reporting_with_breaks") : "Manual Reporting with Breaks";
                        });

                        : form_field("asyncs_active", &mut errors, html! {
                            input(type = "checkbox", id = "asyncs_active", name = "asyncs_active", checked? = ctx.field_value("asyncs_active").map_or(event.asyncs_active, |value| value == "on"));
                            label(for = "asyncs_active") : "Asyncs Active";
                            label(class = "help") : " (Enable async qualifier system)";
                        });

                        : form_field("automated_asyncs", &mut errors, html! {
                            input(type = "checkbox", id = "automated_asyncs", name = "automated_asyncs", checked? = ctx.field_value("automated_asyncs").map_or(event.automated_asyncs, |value| value == "on"));
                            label(for = "automated_asyncs") : "Automated Asyncs";
                            label(class = "help") : " (Discord thread-based async workflow)";
                        });

                        : form_field("swiss_standings", &mut errors, html! {
                            input(type = "checkbox", id = "swiss_standings", name = "swiss_standings", checked? = ctx.field_value("swiss_standings").map_or(event.swiss_standings, |value| value == "on"));
                            label(for = "swiss_standings") : "Swiss Standings";
                            label(class = "help") : " (Use Swiss-style standings)";
                        });

                        : form_field("discord_events_enabled", &mut errors, html! {
                            input(type = "checkbox", id = "discord_events_enabled", name = "discord_events_enabled", checked? = ctx.field_value("discord_events_enabled").map_or(event.discord_events_enabled, |value| value == "on"));
                            label(for = "discord_events_enabled") : "Discord Events Enabled";
                            label(class = "help") : " (Create Discord scheduled events for races)";
                        });

                        : form_field("discord_events_require_restream", &mut errors, html! {
                            input(type = "checkbox", id = "discord_events_require_restream", name = "discord_events_require_restream", checked? = ctx.field_value("discord_events_require_restream").map_or(event.discord_events_require_restream, |value| value == "on"));
                            label(for = "discord_events_require_restream") : "Discord Events Require Restream";
                            label(class = "help") : " (Only create Discord events for restreamed races)";
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
                    p : "You must be an organizer to access this page.";
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
    manual_reporting_with_breaks: bool,
    asyncs_active: bool,
    automated_asyncs: bool,
    swiss_standings: bool,
    discord_events_enabled: bool,
    discord_events_require_restream: bool,
    emulator_settings_reminder: bool,
    prevent_late_joins: bool,
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
        if !event_data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
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

            // Update database
            sqlx::query!(r#"
                UPDATE events
                SET display_name = $1, start = $2, end_time = $3, url = $4, video_url = $5,
                    discord_invite_url = $6, discord_guild = $7, discord_race_room_channel = $8,
                    discord_race_results_channel = $9, discord_volunteer_info_channel = $10,
                    discord_organizer_channel = $11, discord_scheduling_channel = $12,
                    discord_async_channel = $13, short_name = $14, speedgaming_slug = $15,
                    listed = $16, manual_reporting_with_breaks = $17, asyncs_active = $18,
                    automated_asyncs = $19, swiss_standings = $20, discord_events_enabled = $21,
                    discord_events_require_restream = $22, emulator_settings_reminder = $23,
                    prevent_late_joins = $24
                WHERE series = $25 AND event = $26
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
                value.manual_reporting_with_breaks,
                value.asyncs_active,
                value.automated_asyncs,
                value.swiss_standings,
                value.discord_events_enabled,
                value.discord_events_require_restream,
                value.emulator_settings_reminder,
                value.prevent_late_joins,
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
        if !event_data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
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
        if !event_data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
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
        if !event_data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
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