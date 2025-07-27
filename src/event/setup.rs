use crate::{
    event::{
        Data,
        Tab,
        enter,
    },
    prelude::*,
    user::DisplaySource,
    game,
};
use rocket::response::content::RawText;
use serde::Serializer;

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum SetupError {
    #[error(transparent)] Data(#[from] event::DataError),
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] Game(#[from] game::GameError),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[error("unknown user")]
    UnknownUser,
}

impl From<SetupError> for StatusOrError<SetupError> {
    fn from(e: SetupError) -> Self {
        Self::Err(e)
    }
}

async fn setup_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, _uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Setup, false).await?;
    
    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if let Some(ref me) = me {
        // Check if user is organizer or game admin
        let is_organizer = event.organizers(&mut transaction).await?.contains(me);
        let is_game_admin = if let Some(game) = event.game(&mut transaction).await? {
            game.is_admin(&mut transaction, me).await?
        } else {
            false
        };
        
        if !is_organizer && !is_game_admin {
            html! {
                article {
                    p : "You do not have permission to access this page.";
                }
            }
        } else {
            let mut errors = ctx.errors().collect_vec();
            
            // Get current organizers
            let organizers = event.organizers(&mut transaction).await?;
            
            html! {
                article {
                    h2 : "Event Setup";
                    
                    h3 : "Basic Information";
                    : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                        : form_field("display_name", &mut errors, html! {
                            label(for = "display_name") : "Display Name";
                            input(type = "text", id = "display_name", name = "display_name", value = ctx.field_value("display_name").unwrap_or(&event.display_name));
                        });
                        
                        : form_field("start_time", &mut errors, html! {
                            label(for = "start_time") : "Start Time (UTC)";
                            input(type = "datetime-local", id = "start_time", name = "start_time", value = ctx.field_value("start_time").unwrap_or(&event.base_start.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("end_time", &mut errors, html! {
                            label(for = "end_time") : "End Time (UTC)";
                            input(type = "datetime-local", id = "end_time", name = "end_time", value = ctx.field_value("end_time").unwrap_or(&event.end.map(|dt| dt.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("listed", &mut errors, html! {
                            input(type = "checkbox", id = "listed", name = "listed", checked? = ctx.field_value("listed").map_or(true, |value| value == "on"));
                            label(for = "listed") : "Listed on main page";
                        });
                        
                        : form_field("url", &mut errors, html! {
                            label(for = "url") : "Event URL (start.gg, Challonge, etc.)";
                            input(type = "url", id = "url", name = "url", value = ctx.field_value("url").unwrap_or(&event.url.map(|u| u.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("video_url", &mut errors, html! {
                            label(for = "video_url") : "Video URL";
                            input(type = "url", id = "video_url", name = "video_url", value = ctx.field_value("video_url").unwrap_or(&event.video_url.map(|u| u.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_invite_url", &mut errors, html! {
                            label(for = "discord_invite_url") : "Discord Invite URL";
                            input(type = "url", id = "discord_invite_url", name = "discord_invite_url", value = ctx.field_value("discord_invite_url").unwrap_or(&event.discord_invite_url.map(|u| u.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_guild", &mut errors, html! {
                            label(for = "discord_guild") : "Discord Guild ID";
                            input(type = "text", id = "discord_guild", name = "discord_guild", value = ctx.field_value("discord_guild").unwrap_or(&event.discord_guild.map(|g| g.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_race_room_channel", &mut errors, html! {
                            label(for = "discord_race_room_channel") : "Discord Race Room Channel ID";
                            input(type = "text", id = "discord_race_room_channel", name = "discord_race_room_channel", value = ctx.field_value("discord_race_room_channel").unwrap_or(&event.discord_race_room_channel.map(|c| c.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_race_results_channel", &mut errors, html! {
                            label(for = "discord_race_results_channel") : "Discord Race Results Channel ID";
                            input(type = "text", id = "discord_race_results_channel", name = "discord_race_results_channel", value = ctx.field_value("discord_race_results_channel").unwrap_or(&event.discord_race_results_channel.map(|c| c.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_organizer_channel", &mut errors, html! {
                            label(for = "discord_organizer_channel") : "Discord Organizer Channel ID";
                            input(type = "text", id = "discord_organizer_channel", name = "discord_organizer_channel", value = ctx.field_value("discord_organizer_channel").unwrap_or(&event.discord_organizer_channel.map(|c| c.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_scheduling_channel", &mut errors, html! {
                            label(for = "discord_scheduling_channel") : "Discord Scheduling Channel ID";
                            input(type = "text", id = "discord_scheduling_channel", name = "discord_scheduling_channel", value = ctx.field_value("discord_scheduling_channel").unwrap_or(&event.discord_scheduling_channel.map(|c| c.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_volunteer_info_channel", &mut errors, html! {
                            label(for = "discord_volunteer_info_channel") : "Discord Volunteer Info Channel ID";
                            input(type = "text", id = "discord_volunteer_info_channel", name = "discord_volunteer_info_channel", value = ctx.field_value("discord_volunteer_info_channel").unwrap_or(&event.discord_volunteer_info_channel.map(|c| c.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("discord_async_channel", &mut errors, html! {
                            label(for = "discord_async_channel") : "Discord Async Channel ID";
                            input(type = "text", id = "discord_async_channel", name = "discord_async_channel", value = ctx.field_value("discord_async_channel").unwrap_or(&event.discord_async_channel.map(|c| c.to_string()).unwrap_or_default()));
                        });
                        
                        : form_field("hide_races_tab", &mut errors, html! {
                            input(type = "checkbox", id = "hide_races_tab", name = "hide_races_tab", checked? = ctx.field_value("hide_races_tab").map_or(event.hide_races_tab, |value| value == "on"));
                            label(for = "hide_races_tab") : "Hide Races Tab";
                        });
                        
                        : form_field("hide_teams_tab", &mut errors, html! {
                            input(type = "checkbox", id = "hide_teams_tab", name = "hide_teams_tab", checked? = ctx.field_value("hide_teams_tab").map_or(event.hide_teams_tab, |value| value == "on"));
                            label(for = "hide_teams_tab") : "Hide Teams Tab";
                        });
                        
                        : form_field("show_opt_out", &mut errors, html! {
                            input(type = "checkbox", id = "show_opt_out", name = "show_opt_out", checked? = ctx.field_value("show_opt_out").map_or(event.show_opt_out, |value| value == "on"));
                            label(for = "show_opt_out") : "Show Opt Out Option";
                        });
                        
                        : form_field("show_qualifier_times", &mut errors, html! {
                            input(type = "checkbox", id = "show_qualifier_times", name = "show_qualifier_times", checked? = ctx.field_value("show_qualifier_times").map_or(event.show_qualifier_times, |value| value == "on"));
                            label(for = "show_qualifier_times") : "Show Qualifier Times";
                        });
                        
                        : form_field("auto_import", &mut errors, html! {
                            input(type = "checkbox", id = "auto_import", name = "auto_import", checked? = ctx.field_value("auto_import").map_or(event.auto_import, |value| value == "on"));
                            label(for = "auto_import") : "Auto Import from start.gg";
                        });
                        
                        : form_field("manual_reporting_with_breaks", &mut errors, html! {
                            input(type = "checkbox", id = "manual_reporting_with_breaks", name = "manual_reporting_with_breaks", checked? = ctx.field_value("manual_reporting_with_breaks").map_or(event.manual_reporting_with_breaks, |value| value == "on"));
                            label(for = "manual_reporting_with_breaks") : "Manual Reporting with Breaks";
                        });
                    }, errors.clone(), "Save Basic Info");
                }
                
                h3 : "Organizers";
                p : "Current organizers:";
                ul {
                    @for organizer in &organizers {
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
                
                : full_form(uri!(add_organizer(event.series, &*event.event)), csrf, html! {
                    : form_field("organizer", &mut errors, html! {
                        label(for = "organizer") : "Add Organizer";
                        input(type = "text", id = "organizer", name = "organizer", placeholder = "Search for user...", autocomplete = "off");
                        div(id = "organizer-suggestions", class = "suggestions");
                    });
                }, errors.clone(), "Add Organizer");
                
                h3 : "Enter Flow Configuration";
                p : "Configure signup requirements for this event.";
                
                : full_form(uri!(update_enter_flow(event.series, &*event.event)), csrf, html! {
                    : form_field("enter_flow_json", &mut errors, html! {
                        label(for = "enter_flow_json") : "Enter Flow JSON";
                        textarea(id = "enter_flow_json", name = "enter_flow_json", rows = "10", style = "font-family: monospace;") {
                            : ctx.field_value("enter_flow_json").unwrap_or(
                                match &event.enter_flow {
                                    Some(_val) => "{}", // Placeholder since Flow doesn't implement Serialize
                                    None => "",
                                }
                            );
                        }
                        p(class = "help") : "Configure the signup requirements as JSON. Leave empty for no requirements.";
                        details {
                            summary : "Example enter_flow configurations";
                            div(style = "margin-top: 10px; padding: 10px; background: #f5f5f5; border-radius: 4px;") {
                                h4 : "Basic Discord Account Requirement:";
                                pre(style = "font-size: 12px;") {
                                    : r#"{
  "requirements": [
    {
      "type": "discord_account"
    }
  ]
}"#;
                                }
                                h4 : "Discord Account + Racetime Account:";
                                pre(style = "font-size: 12px;") {
                                    : r#"{
  "requirements": [
    {
      "type": "discord_account"
    },
    {
      "type": "racetime_account"
    }
  ]
}"#;
                                }
                                h4 : "With Deadline:";
                                pre(style = "font-size: 12px;") {
                                    : r#"{
  "requirements": [
    {
      "type": "discord_account"
    }
  ],
  "closes": "2024-01-15T23:59:59Z"
}"#;
                                }
                            }
                        }
                    });
                }, errors, "Save Enter Flow");
            }
        }
    } else {
        html! {
            article {
                p : "You must be logged in to access this page.";
            }
        }
    };
    
    Ok(html! {
        : header;
        : content;
        script(src = static_url!("user-search.js")) {}
    })
}

#[rocket::get("/event/<series>/<event>/setup")]
pub(crate) async fn get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<SetupError>> {
    let mut transaction = pool.begin().await.map_err(SetupError::Sql)?;
    let event_data = Data::new(&mut transaction, series, &event).await.map_err(SetupError::Data)?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(setup_form(transaction, me, uri, csrf.as_ref(), event_data, Context::default()).await.map_err(SetupError::Event)?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct SetupForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    display_name: String,
    #[field(default = String::new())]
    start_time: String,
    #[field(default = String::new())]
    end_time: String,
    listed: bool,
    #[field(default = String::new())]
    url: String,
    #[field(default = String::new())]
    video_url: String,
    #[field(default = String::new())]
    discord_invite_url: String,
    #[field(default = String::new())]
    discord_guild: String,
    #[field(default = String::new())]
    discord_race_room_channel: String,
    #[field(default = String::new())]
    discord_race_results_channel: String,
    #[field(default = String::new())]
    discord_organizer_channel: String,
    #[field(default = String::new())]
    discord_scheduling_channel: String,
    #[field(default = String::new())]
    discord_volunteer_info_channel: String,
    #[field(default = String::new())]
    discord_async_channel: String,
    hide_races_tab: bool,
    hide_teams_tab: bool,
    show_opt_out: bool,
    show_qualifier_times: bool,
    auto_import: bool,
    manual_reporting_with_breaks: bool,
}

#[rocket::post("/event/<series>/<event>/setup", data = "<form>")]
pub(crate) async fn post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, SetupForm>>) -> Result<RedirectOrContent, StatusOrError<SetupError>> {
    let mut transaction = pool.begin().await.map_err(SetupError::Sql)?;
    let event_data = Data::new(&mut transaction, series, event).await.map_err(SetupError::Data)?.ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check permissions
    let is_organizer = event_data.organizers(&mut transaction).await.map_err(SetupError::Event)?.contains(&me);
    let is_game_admin = if let Some(game) = event_data.game(&mut transaction).await.map_err(SetupError::Data)? {
        game.is_admin(&mut transaction, &me).await.map_err(SetupError::Game)?
    } else {
        false
    };
    
    if !is_organizer && !is_game_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
        // Parse start time
        let start_time = if !value.start_time.is_empty() {
            Some(DateTime::parse_from_str(&format!("{}:00", value.start_time), "%Y-%m-%dT%H:%M:%S%z").map_err(|_| SetupError::UnknownUser)?.with_timezone(&Utc))
        } else {
            None
        };
        
        // Parse end time
        let end_time = if !value.end_time.is_empty() {
            Some(DateTime::parse_from_str(&format!("{}:00", value.end_time), "%Y-%m-%dT%H:%M:%S%z").map_err(|_| SetupError::UnknownUser)?.with_timezone(&Utc))
        } else {
            None
        };
        
        // Parse URLs
        let url = if !value.url.is_empty() { Some(value.url.parse().map_err(SetupError::Url)?) } else { None };
        let video_url = if !value.video_url.is_empty() { Some(value.video_url.parse().map_err(SetupError::Url)?) } else { None };
        let discord_invite_url = if !value.discord_invite_url.is_empty() { Some(value.discord_invite_url.parse().map_err(SetupError::Url)?) } else { None };
        
        // Parse Discord IDs
        let discord_guild = if !value.discord_guild.is_empty() { 
            Some(GuildId::new(value.discord_guild.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        let discord_race_room_channel = if !value.discord_race_room_channel.is_empty() { 
            Some(ChannelId::new(value.discord_race_room_channel.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        let discord_race_results_channel = if !value.discord_race_results_channel.is_empty() { 
            Some(ChannelId::new(value.discord_race_results_channel.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        let discord_organizer_channel = if !value.discord_organizer_channel.is_empty() { 
            Some(ChannelId::new(value.discord_organizer_channel.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        let discord_scheduling_channel = if !value.discord_scheduling_channel.is_empty() { 
            Some(ChannelId::new(value.discord_scheduling_channel.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        let discord_volunteer_info_channel = if !value.discord_volunteer_info_channel.is_empty() { 
            Some(ChannelId::new(value.discord_volunteer_info_channel.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        let discord_async_channel = if !value.discord_async_channel.is_empty() { 
            Some(ChannelId::new(value.discord_async_channel.parse().map_err(SetupError::ParseInt)?))
        } else { 
            None 
        };
        
        // Update event
        sqlx::query!(r#"
            UPDATE events SET 
                display_name = $1,
                start = $2,
                end_time = $3,
                listed = $4,
                url = $5,
                video_url = $6,
                discord_invite_url = $7,
                discord_guild = $8,
                discord_race_room_channel = $9,
                discord_race_results_channel = $10,
                discord_organizer_channel = $11,
                discord_scheduling_channel = $12,
                discord_volunteer_info_channel = $13,
                discord_async_channel = $14,
                hide_races_tab = $15,
                hide_teams_tab = $16,
                show_opt_out = $17,
                show_qualifier_times = $18,
                auto_import = $19,
                manual_reporting_with_breaks = $20
            WHERE series = $21 AND event = $22
        "#,
            value.display_name,
            start_time,
            end_time,
            value.listed,
            url.map(|u: Url| u.to_string()),
            video_url.map(|u: Url| u.to_string()),
            discord_invite_url.map(|u: Url| u.to_string()),
            discord_guild.map(|g| g.get() as i64),
            discord_race_room_channel.map(|c| c.get() as i64),
            discord_race_results_channel.map(|c| c.get() as i64),
            discord_organizer_channel.map(|c| c.get() as i64),
            discord_scheduling_channel.map(|c| c.get() as i64),
            discord_volunteer_info_channel.map(|c| c.get() as i64),
            discord_async_channel.map(|c| c.get() as i64),
            value.hide_races_tab,
            value.hide_teams_tab,
            value.show_opt_out,
            value.show_qualifier_times,
            value.auto_import,
            value.manual_reporting_with_breaks,
            series as _,
            event
        ).execute(&mut *transaction).await.map_err(SetupError::Sql)?;
        
        transaction.commit().await.map_err(SetupError::Sql)?;
        
        Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))))
    } else {
        let html = setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await.map_err(SetupError::Event)?;
        Ok(RedirectOrContent::Content(html))
    }
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddOrganizerForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    organizer: String,
}

#[rocket::post("/event/<series>/<event>/setup/organizers", data = "<form>")]
pub(crate) async fn add_organizer(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, AddOrganizerForm>>) -> Result<RedirectOrContent, StatusOrError<SetupError>> {
    let mut transaction = pool.begin().await.map_err(SetupError::Sql)?;
    let event_data = Data::new(&mut transaction, series, event).await.map_err(SetupError::Data)?.ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check permissions
    let is_organizer = event_data.organizers(&mut transaction).await.map_err(SetupError::Event)?.contains(&me);
    let is_game_admin = if let Some(game) = event_data.game(&mut transaction).await.map_err(SetupError::Data)? {
        game.is_admin(&mut transaction, &me).await.map_err(SetupError::Game)?
    } else {
        false
    };
    
    if !is_organizer && !is_game_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
        // Find user by search query
        let search_results = search_users_internal(&mut transaction, Some(&value.organizer)).await?;
        
        if search_results.is_empty() {
            form.context.push_error(form::Error::validation("No users found matching that search."));
        } else if search_results.len() == 1 {
            let user_id = search_results[0].id;
            
            // Check if already an organizer
            let is_already_organizer = sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM organizers WHERE series = $1 AND event = $2 AND organizer = $3) AS "exists!""#,
                series as _,
                event,
                i64::from(user_id)
            ).fetch_one(&mut *transaction).await.map_err(SetupError::Sql)?;
            
            if is_already_organizer {
                form.context.push_error(form::Error::validation("User is already an organizer."));
            } else {
                // Add as organizer
                sqlx::query!(
                    r#"INSERT INTO organizers (series, event, organizer) VALUES ($1, $2, $3)"#,
                    series as _,
                    event,
                    i64::from(user_id)
                ).execute(&mut *transaction).await.map_err(SetupError::Sql)?;
                
                transaction.commit().await.map_err(SetupError::Sql)?;
                return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))));
            }
        } else {
            form.context.push_error(form::Error::validation("Multiple users found. Please be more specific."));
        }
    }
    
    let html = setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await.map_err(SetupError::Event)?;
    Ok(RedirectOrContent::Content(html))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveOrganizerForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/setup/organizers/<organizer_id>/remove", data = "<form>")]
pub(crate) async fn remove_organizer(pool: &State<PgPool>, me: User, _uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, organizer_id: Id<Users>, form: Form<Contextual<'_, RemoveOrganizerForm>>) -> Result<RedirectOrContent, StatusOrError<SetupError>> {
    let mut transaction = pool.begin().await.map_err(SetupError::Sql)?;
    let event_data = Data::new(&mut transaction, series, event).await.map_err(SetupError::Data)?.ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check permissions
    let is_organizer = event_data.organizers(&mut transaction).await.map_err(SetupError::Event)?.contains(&me);
    let is_game_admin = if let Some(game) = event_data.game(&mut transaction).await.map_err(SetupError::Data)? {
        game.is_admin(&mut transaction, &me).await.map_err(SetupError::Game)?
    } else {
        false
    };
    
    if !is_organizer && !is_game_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    // Remove organizer
    sqlx::query!(
        r#"DELETE FROM organizers WHERE series = $1 AND event = $2 AND organizer = $3"#,
        series as _,
        event,
        i64::from(organizer_id)
    ).execute(&mut *transaction).await.map_err(SetupError::Sql)?;
    
    transaction.commit().await.map_err(SetupError::Sql)?;
    
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct UpdateEnterFlowForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    enter_flow_json: String,
}

#[rocket::post("/event/<series>/<event>/setup/enter-flow", data = "<form>")]
pub(crate) async fn update_enter_flow(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, UpdateEnterFlowForm>>) -> Result<RedirectOrContent, StatusOrError<SetupError>> {
    let mut transaction = pool.begin().await.map_err(SetupError::Sql)?;
    let event_data = Data::new(&mut transaction, series, event).await.map_err(SetupError::Data)?.ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check permissions
    let is_organizer = event_data.organizers(&mut transaction).await.map_err(SetupError::Event)?.contains(&me);
    let is_game_admin = if let Some(game) = event_data.game(&mut transaction).await.map_err(SetupError::Data)? {
        game.is_admin(&mut transaction, &me).await.map_err(SetupError::Game)?
    } else {
        false
    };
    
    if !is_organizer && !is_game_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
        let enter_flow = if value.enter_flow_json.trim().is_empty() {
            None
        } else {
            let flow: enter::Flow = serde_json::from_str(&value.enter_flow_json).map_err(SetupError::Json)?;
            Some(flow)
        };
        
        // Update enter_flow
        let enter_flow_json = if let Some(_flow) = enter_flow {
            Some(serde_json::from_str::<serde_json::Value>(&value.enter_flow_json).map_err(SetupError::Json)?)
        } else {
            None
        };
        sqlx::query!(
            r#"UPDATE events SET enter_flow = $1 WHERE series = $2 AND event = $3"#,
            enter_flow_json,
            series as _,
            event
        ).execute(&mut *transaction).await.map_err(SetupError::Sql)?;
        
        transaction.commit().await.map_err(SetupError::Sql)?;
        
        Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event)))))
    } else {
        let html = setup_form(transaction, Some(me), uri, csrf.as_ref(), event_data, form.context).await.map_err(SetupError::Event)?;
        Ok(RedirectOrContent::Content(html))
    }
}

#[rocket::get("/event/setup/search-users?<query>")]
pub(crate) async fn search_users(
    pool: &State<PgPool>,
    query: Option<&str>,
) -> Result<RawText<String>, StatusOrError<SetupError>> {
    let mut transaction = pool.begin().await.map_err(SetupError::Sql)?;
    let results = search_users_internal(&mut transaction, query).await?;
    Ok(RawText(serde_json::to_string(&results).map_err(SetupError::Json)?))
}

async fn search_users_internal(transaction: &mut Transaction<'_, Postgres>, query: Option<&str>) -> Result<Vec<UserSearchResult>, SetupError> {
    let query = query.unwrap_or("");
    if query.len() < 2 {
        return Ok(Vec::new());
    }
    
    let rows = sqlx::query_as!(
        UserSearchRow,
        r#"
        SELECT 
            u.id,
            u.display_source AS "display_source: DisplaySource",
            u.racetime_id,
            u.racetime_display_name,
            u.discord_display_name,
            u.discord_username
        FROM users u
        WHERE 
            u.racetime_display_name ILIKE $1 
            OR u.discord_display_name ILIKE $1 
            OR u.discord_username ILIKE $1
        ORDER BY 
            CASE 
                WHEN u.racetime_display_name ILIKE $1 THEN 1
                WHEN u.discord_display_name ILIKE $1 THEN 2
                WHEN u.discord_username ILIKE $1 THEN 3
                ELSE 4
            END,
            u.racetime_display_name,
            u.discord_display_name,
            u.discord_username
        LIMIT 10
        "#,
        format!("%{}%", query)
    )
    .fetch_all(&mut **transaction)
    .await.map_err(SetupError::Sql)?;
    
    Ok(rows
        .into_iter()
        .map(|row| {
            let racetime_id = row.racetime_id.clone();
            let discord_username = row.discord_username.clone();
            UserSearchResult {
                id: row.id,
                display_name: match row.display_source {
                    DisplaySource::RaceTime => row.racetime_display_name.unwrap_or_else(|| format!("racetime user {}", row.racetime_id.unwrap_or_default())),
                    DisplaySource::Discord => row.discord_display_name.unwrap_or_else(|| format!("discord user {}", row.discord_username.unwrap_or_default())),
                },
                racetime_id,
                discord_username,
            }
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
    S: Serializer,
{
    serializer.serialize_i64(i64::from(*id))
}

#[derive(sqlx::FromRow)]
struct UserSearchRow {
    id: Id<Users>,
    display_source: DisplaySource,
    racetime_id: Option<String>,
    racetime_display_name: Option<String>,
    discord_display_name: Option<String>,
    discord_username: Option<String>,
} 