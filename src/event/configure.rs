use {
    serenity::model::id::{ChannelId, RoleId},
    crate::{
        discord_bot::PgSnowflake,
        event::{
            Data,
            QualifierScoreHiding,
            Tab,
        },
        prelude::*,
        racetime_bot::{Goal, VersionedBranch},
        startgg,
        user::DisplaySource,
    },
};
use rocket::response::content::RawText;
use serde::Serializer;

async fn configure_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let query_string = uri.0.query().map(|q| q.to_string());
    let sync_success = query_string.as_deref().and_then(|q| {
        q.split('&')
            .find(|param| param.starts_with("sync_success="))
            .and_then(|param| param.split('=').nth(1))
            .and_then(|encoded| urlencoding::decode(encoded).ok())
    });
    
    let sync_failed = query_string.as_deref().and_then(|q| {
        q.split('&')
            .find(|param| param.starts_with("sync_failed="))
            .and_then(|param| param.split('=').nth(1))
            .and_then(|encoded| urlencoding::decode(encoded).ok())
    });
    let header = event.header(&mut transaction, me.as_ref(), Tab::Configure, false).await?;
    let success_message = if let Some(success) = sync_success {
        if let Some(failed) = sync_failed {
            html! {
                div(class = "success") {
                    p : format!("{}. Failed to sync: {}", success, failed);
                }
            }
        } else {
            html! {
                div(class = "success") {
                    p : success;
                }
            }
        }
    } else {
        html! {}
    };
    
    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if let Some(ref me) = me {
        let is_organizer_or_global = event.organizers(&mut transaction).await?.contains(me) || me.is_global_admin();
        let is_game_admin = if let Some(game) = event.game(&mut transaction).await? {
            game.is_admin(&mut transaction, me).await.map_err(event::Error::from)?
        } else {
            false
        };
        if is_organizer_or_global {
            let mut errors = ctx.errors().collect_vec();
            html! {
                @if event.series == Series::Standard && event.event == "w" {
                    p {
                        : "Preroll mode: ";
                        : format!("{:?}", s::WEEKLY_PREROLL_MODE);
                    }
                    p {
                        : "Randomizer version: ";
                        @match event.rando_version.as_ref().expect("no randomizer version configured for weeklies") {
                            VersionedBranch::Pinned { version } => : version.to_string();
                            VersionedBranch::Latest { branch } => {
                                : "latest ";
                                : branch.to_string();
                                : " branch (updates automatically)";
                            }
                            VersionedBranch::Custom { github_username, branch } => {
                                : "custom (GitHub user/organization name: ";
                                : github_username;
                                : ", branch: ";
                                : branch;
                                : ")";
                            }
                            VersionedBranch::Tww { identifier, github_url } => {
                                : "The Wind Waker Randomizer (build ";
                                : identifier;
                                : ", download: ";
                                : github_url;
                                : ")";
                            }
                        }
                    }
                    p : "Settings:";
                    pre : serde_json::to_string_pretty(event.single_settings.as_ref().expect("no settings configured for weeklies"))?;
                    p {
                        : "The data above is currently not editable for technical reasons. Please contact ";
                        : User::from_id(&mut *transaction, Id::<Users>::from(16287394041462225947_u64)).await?.ok_or(PageError::AdminUserData(1))?; // TreZ
                        : " if you've spotted an error in it.";
                    } //TODO make editable
                } else {
                    : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                        @if let MatchSource::StartGG(_) = event.match_source() {
                            : form_field("auto_import", &mut errors, html! {
                                input(type = "checkbox", id = "auto_import", name = "auto_import", checked? = ctx.field_value("auto_import").map_or(event.auto_import, |value| value == "on"));
                                label(for = "auto_import") : "Automatically import new races from start.gg";
                                label(class = "help") : " (If this option is turned off, you can import races by clicking the Import button on the Races tab.)";
                            });
                            : form_field("sync_startgg_ids", &mut errors, html! {
                                button(type = "submit", name = "sync_startgg_ids", value = "sync") : "Sync StartGG Participant IDs";
                                label(class = "help") : "(This will attempt to match teams with solo players to their StartGG entrant IDs.)";
                            });
                        }
                        : form_field("min_schedule_notice", &mut errors, html! {
                            label(for = "min_schedule_notice") : "Minimum scheduling notice:";
                            input(type = "text", name = "min_schedule_notice", value = ctx.field_value("min_schedule_notice").map(Cow::Borrowed).unwrap_or_else(|| Cow::Owned(unparse_duration(event.min_schedule_notice)))); //TODO h:m:s fields?
                            br;
                            label(class = "help") : "(Races must be scheduled at least this far in advance. Can be configured to be as low as 0 seconds, but note that if a race is scheduled less than 30 minutes in advance, the room is opened immediately, and if a race is scheduled less than 15 minutes in advance, the seed is posted immediately.)";
                        });
                        @if matches!(event.match_source(), MatchSource::StartGG(_)) || event.discord_race_results_channel.is_some() {
                            : form_field("retime_window", &mut errors, html! {
                                label(for = "retime_window") : "Retime window:";
                                input(type = "text", name = "retime_window", value = ctx.field_value("retime_window").map(Cow::Borrowed).unwrap_or_else(|| Cow::Owned(unparse_duration(event.retime_window)))); //TODO h:m:s fields?
                                br;
                                label(class = "help") {
                                    : "(If the time difference between ";
                                    @if event.team_config.is_racetime_team_format() {
                                        : "teams'";
                                    } else {
                                        : "runners'";
                                    }
                                    : " finish times is less than this, the result is not auto-reported.)";
                                }
                            });
                            : form_field("manual_reporting_with_breaks", &mut errors, html! {
                                input(type = "checkbox", id = "manual_reporting_with_breaks", name = "manual_reporting_with_breaks", checked? = ctx.field_value("manual_reporting_with_breaks").map_or(event.manual_reporting_with_breaks, |value| value == "on"));
                                label(for = "manual_reporting_with_breaks") : "Disable automatic result reporting if !breaks command is used";
                            });
                        }
                        @if let Some(VersionedBranch::Tww { identifier, .. }) = &event.rando_version {
                            : form_field("settings_string", &mut errors, html! {
                                label(for = "settings_string") : "Settings string:";
                                input(type = "text", id = "settings_string", name = "settings_string", value? = ctx.field_value("settings_string").or(event.settings_string.as_deref()));
                                label(class = "help") : format!("(needs to be compatible with version {})", identifier);
                            });
                        }
                        : form_field("asyncs_active", &mut errors, html! {
                            input(type = "checkbox", id = "asyncs_active", name = "asyncs_active", checked? = ctx.field_value("asyncs_active").map_or(event.asyncs_active, |value| value == "on"));
                            label(for = "asyncs_active") : "Allow async races";
                            label(class = "help") : "(If disabled, Discord scheduling threads will not mention the /schedule-async command and async races will not be possible)";
                        });
                        : form_field("swiss_standings", &mut errors, html! {
                            input(type = "checkbox", id = "swiss_standings", name = "swiss_standings", checked? = ctx.field_value("swiss_standings").map_or(event.swiss_standings, |value| value == "on"));
                            label(for = "swiss_standings") : "Show Swiss standings tab";
                            label(class = "help") : "(If enabled, the Swiss standings tab will be visible for this event)";
                        });
                        @if event.discord_guild.is_some() {
                            : form_field("discord_events_enabled", &mut errors, html! {
                                input(type = "checkbox", id = "discord_events_enabled", name = "discord_events_enabled", checked? = ctx.field_value("discord_events_enabled").map_or(event.discord_events_enabled, |value| value == "on"));
                                label(for = "discord_events_enabled") : "Create Discord scheduled events for races";
                                label(class = "help") : "(If enabled, Discord scheduled events will be automatically created when races are scheduled)";
                            });
                            : form_field("discord_events_require_restream", &mut errors, html! {
                                input(type = "checkbox", id = "discord_events_require_restream", name = "discord_events_require_restream", checked? = ctx.field_value("discord_events_require_restream").map_or(event.discord_events_require_restream, |value| value == "on"));
                                label(for = "discord_events_require_restream") : "Only create Discord events for races with restreams";
                                label(class = "help") : "(If enabled, Discord scheduled events will only be created for races that have at least one restream URL set)";
                            });
                            : form_field("automated_asyncs", &mut errors, html! {
                                input(type = "checkbox", id = "automated_asyncs", name = "automated_asyncs", checked? = ctx.field_value("automated_asyncs").map_or(event.automated_asyncs, |value| value == "on"));
                                label(for = "automated_asyncs") : "Use automated Discord threads for qualifier asyncs";
                                label(class = "help") : "(When enabled, qualifier requests create private Discord threads with READY/countdown/FINISH buttons. Staff validate results via /result-async command.)";
                            });
                        }
                        : form_field("qualifier_score_hiding", &mut errors, html! {
                            label(for = "qualifier_score_hiding") : "Qualifier Score Hiding";
                            select(id = "qualifier_score_hiding", name = "qualifier_score_hiding", style = "width: 100%; max-width: 600px;") {
                                option(value = "none", selected? = ctx.field_value("qualifier_score_hiding").map_or(matches!(event.qualifier_score_hiding, QualifierScoreHiding::None), |v| v == "none")) : "None (all scores visible)";
                                option(value = "async_only", selected? = ctx.field_value("qualifier_score_hiding").map_or(matches!(event.qualifier_score_hiding, QualifierScoreHiding::AsyncOnly), |v| v == "async_only")) : "Async Only (async scores hidden until window closes)";
                                option(value = "full_points", selected? = ctx.field_value("qualifier_score_hiding").map_or(matches!(event.qualifier_score_hiding, QualifierScoreHiding::FullPoints), |v| v == "full_points")) : "Hide Points (names and counts visible)";
                                option(value = "full_points_counts", selected? = ctx.field_value("qualifier_score_hiding").map_or(matches!(event.qualifier_score_hiding, QualifierScoreHiding::FullPointsCounts), |v| v == "full_points_counts")) : "Hide Points & Counts (names visible)";
                                option(value = "full_complete", selected? = ctx.field_value("qualifier_score_hiding").map_or(matches!(event.qualifier_score_hiding, QualifierScoreHiding::FullComplete), |v| v == "full_complete")) : "Hide Everything (table hidden until quals end)";
                            }
                            label(class = "help") : "(Controls what qualifier information is visible to non-organizers before all qualifiers have ended)";
                        });
                    }, errors, "Save");
                }
                h2 : "More options";
                ul {
                    li {
                        a(href = uri!(restreamers_get(event.series, &*event.event))) : "Manage restream coordinators";
                    }
                    li {
                        a(href = uri!(weekly_schedules_get(event.series, &*event.event))) : "Manage weekly schedules";
                    }
                    li {
                        a(href = uri!(info_page_get(event.series, &*event.event))) : "Edit info page";
                    }
                }
            }
        } else if is_game_admin {
            html! {
                ul {
                    li {
                        a(href = uri!(restreamers_get(event.series, &*event.event))) : "Manage restream coordinators";
                    }
                }
            }
        } else {
            html! {
                article {
                    p : "This page is for organizers of this event only.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(get(event.series, &*event.event)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to configure this event.";
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Configure — {}", event.display_name), html! {
        : header;
        : success_message;
        : content;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/configure")]
pub(crate) async fn get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(configure_form(transaction, me, uri, csrf.as_ref(), data, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ConfigureForm {
    #[field(default = String::new())]
    csrf: String,
    auto_import: bool,
    #[field(default = String::new())]
    min_schedule_notice: String,
    retime_window: Option<String>,
    manual_reporting_with_breaks: bool,
    sync_startgg_ids: Option<String>,
    asyncs_active: bool,
    swiss_standings: bool,
    discord_events_enabled: bool,
    discord_events_require_restream: bool,
    automated_asyncs: bool,
    settings_string: Option<String>,
    qualifier_score_hiding: String,
}

#[rocket::post("/event/<series>/<event>/configure", data = "<form>")]
pub(crate) async fn post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, ConfigureForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        let min_schedule_notice = if let Some(time) = parse_duration(&value.min_schedule_notice, None) {
            Some(time)
        } else {
            form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'.").with_name("min_schedule_notice"));
            None
        };
        let retime_window = if let Some(retime_window) = &value.retime_window {
            if let Some(time) = parse_duration(retime_window, None) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'.").with_name("retime_window"));
                None
            }
        } else {
            None
        };
        // Handle StartGG sync first, regardless of other validation errors
        if let Some(_) = value.sync_startgg_ids {
            if let MatchSource::StartGG(event_slug) = data.match_source() {
                match sync_startgg_participant_ids(&mut transaction, &data, &event_slug).await {
                    Ok(sync_result) => {
                        transaction.commit().await?;
                        let success_msg = format!("Sync completed: {} teams synced, {} teams could not be synced", 
                            sync_result.synced_count, sync_result.failed_count);
                        let redirect_url = if !sync_result.failed_teams.is_empty() {
                            let failed_list = sync_result.failed_teams.join(", ");
                            format!("{}?sync_success={}&sync_failed={}", 
                                uri!(get(series, event)), 
                                urlencoding::encode(&success_msg),
                                urlencoding::encode(&failed_list))
                        } else {
                            format!("{}?sync_success={}", 
                                uri!(get(series, event)), 
                                urlencoding::encode(&success_msg))
                        };
                        return Ok(RedirectOrContent::Redirect(Redirect::to(redirect_url)));
                    }
                    Err(sync_error) => {
                        form.context.push_error(form::Error::validation(format!("Failed to sync StartGG participant IDs: {}", sync_error)));
                    }
                }
            } else {
                form.context.push_error(form::Error::validation("This event does not have a StartGG source configured."));
            }
        }

        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(configure_form(transaction, Some(me), uri, csrf.as_ref(), data, form.context).await?)
        } else {
            if let MatchSource::StartGG(_) = data.match_source() {
                sqlx::query!("UPDATE events SET auto_import = $1 WHERE series = $2 AND event = $3", value.auto_import, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if let Some(min_schedule_notice) = min_schedule_notice {
                sqlx::query!("UPDATE events SET min_schedule_notice = $1 WHERE series = $2 AND event = $3", min_schedule_notice as _, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if let Some(retime_window) = retime_window {
                sqlx::query!("UPDATE events SET retime_window = $1 WHERE series = $2 AND event = $3", retime_window as _, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if matches!(data.match_source(), MatchSource::StartGG(_)) || data.discord_race_results_channel.is_some() {
                sqlx::query!("UPDATE events SET manual_reporting_with_breaks = $1 WHERE series = $2 AND event = $3", value.manual_reporting_with_breaks, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if value.asyncs_active != data.asyncs_active {
                sqlx::query!("UPDATE events SET asyncs_active = $1 WHERE series = $2 AND event = $3", value.asyncs_active, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if value.swiss_standings != data.swiss_standings {
                sqlx::query!("UPDATE events SET swiss_standings = $1 WHERE series = $2 AND event = $3", value.swiss_standings, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if value.discord_events_enabled != data.discord_events_enabled {
                sqlx::query!("UPDATE events SET discord_events_enabled = $1 WHERE series = $2 AND event = $3", value.discord_events_enabled, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if value.discord_events_require_restream != data.discord_events_require_restream {
                sqlx::query!("UPDATE events SET discord_events_require_restream = $1 WHERE series = $2 AND event = $3", value.discord_events_require_restream, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if value.automated_asyncs != data.automated_asyncs {
                sqlx::query!("UPDATE events SET automated_asyncs = $1 WHERE series = $2 AND event = $3", value.automated_asyncs, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            // Parse and update qualifier_score_hiding
            let qualifier_score_hiding = match value.qualifier_score_hiding.as_str() {
                "none" => QualifierScoreHiding::None,
                "async_only" => QualifierScoreHiding::AsyncOnly,
                "full_points" => QualifierScoreHiding::FullPoints,
                "full_points_counts" => QualifierScoreHiding::FullPointsCounts,
                "full_complete" => QualifierScoreHiding::FullComplete,
                _ => QualifierScoreHiding::None,
            };
            if qualifier_score_hiding != data.qualifier_score_hiding {
                sqlx::query!("UPDATE events SET qualifier_score_hiding = $1 WHERE series = $2 AND event = $3", qualifier_score_hiding as _, data.series as _, &data.event).execute(&mut *transaction).await?;
            }
            if matches!(data.rando_version, Some(VersionedBranch::Tww { .. })) {
                let new_settings_string = value.settings_string.as_deref().filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_owned());
                if new_settings_string != data.settings_string {
                    sqlx::query!("UPDATE events SET settings_string = $1 WHERE series = $2 AND event = $3", new_settings_string, data.series as _, &data.event).execute(&mut *transaction).await?;
                }
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event))))
        }
    } else {
        RedirectOrContent::Content(configure_form(transaction, Some(me), uri, csrf.as_ref(), data, form.context).await?)
    })
}

enum RestreamersFormDefaults<'v> {
    None,
    AddContext(Context<'v>),
    RemoveContext(Id<Users>, Context<'v>),
}

impl<'v> RestreamersFormDefaults<'v> {
    fn remove_errors(&self, for_restreamer: Id<Users>) -> Vec<&form::Error<'v>> {
        match self {
            Self::RemoveContext(restreamer, ctx) if *restreamer == for_restreamer => ctx.errors().collect(),
            _ => Vec::default(),
        }
    }

    fn add_errors(&self) -> Vec<&form::Error<'v>> {
        if let Self::AddContext(ctx) = self {
            ctx.errors().collect()
        } else {
            Vec::default()
        }
    }

    fn add_restreamer(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("restreamer")
        } else {
            None
        }
    }
}

async fn restreamers_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, defaults: RestreamersFormDefaults<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Configure, true).await?;
    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if let Some(ref me) = me {
        let is_game_admin = if let Some(game) = event.game(&mut transaction).await? {
            game.is_admin(&mut transaction, me).await.map_err(event::Error::from)?
        } else {
            false
        };
        if event.organizers(&mut transaction).await?.contains(me) || me.is_global_admin() || is_game_admin {
            let restreamers = event.restreamers(&mut transaction).await?;
            html! {
                h2 : "Manage restream coordinators";
                p : "Restream coordinators can add/edit restream URLs and assign restreamers to this event's races.";
                @if restreamers.is_empty() {
                    p : "No restream coordinators so far.";
                } else {
                    table {
                        thead {
                            tr {
                                th : "Restream coordinator";
                                th;
                            }
                        }
                        tbody {
                            @for restreamer in restreamers {
                                tr {
                                    td : restreamer;
                                    td {
                                        @let errors = defaults.remove_errors(restreamer.id);
                                        @let (errors, button) = button_form(uri!(remove_restreamer(event.series, &*event.event, restreamer.id)), csrf, errors, "Remove");
                                        : errors;
                                        div(class = "button-row") : button;
                                    }
                                }
                            }
                        }
                    }
                }
                h3 : "Add restream coordinator";
                @let mut errors = defaults.add_errors();
                : full_form(uri!(add_restreamer(event.series, &*event.event)), csrf, html! {
                    : form_field("restreamer", &mut errors, html! {
                        label(for = "restreamer") : "Restream coordinator:";
                        div(class = "autocomplete-container") {
                            input(type = "text", id = "restreamer", name = "restreamer", value? = defaults.add_restreamer(), autocomplete = "off");
                            div(id = "user-suggestions", class = "suggestions", style = "display: none;") {}
                        }
                        label(class = "help") : "(Start typing a username to search for users. The search will match display names, racetime.gg IDs, and Discord usernames.)";
                    });
                }, errors, "Add");

                script(src = static_url!("user-search.js")) {}
            }
        } else {
            html! {
                article {
                    p : "This page is for organizers of this event only.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(restreamers_get(event.series, &*event.event)))))) : "Sign in or create a Mido's House account";
                    : " to configure this event.";
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Manage restream coordinators — {}", event.display_name), html! {
        : header;
        : content;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/configure/restreamers")]
pub(crate) async fn restreamers_get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(restreamers_form(transaction, me, uri, csrf.as_ref(), data, RestreamersFormDefaults::None).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddRestreamerForm {
    #[field(default = String::new())]
    csrf: String,
    restreamer: String,
}

#[rocket::post("/event/<series>/<event>/configure/restreamers", data = "<form>")]
pub(crate) async fn add_restreamer(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, AddRestreamerForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        let is_game_admin = if let Some(game) = data.game(&mut transaction).await? {
            game.is_admin(&mut transaction, &me).await.map_err(event::Error::from)?
        } else {
            false
        };
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() && !is_game_admin {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        let restreamer_id = match value.restreamer.parse::<u64>() {
            Ok(id) => id,
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid user ID format.").with_name("restreamer"));
                return Ok(RedirectOrContent::Content(restreamers_form(transaction, Some(me), uri, csrf.as_ref(), data, RestreamersFormDefaults::AddContext(form.context)).await?));
            }
        };
        let restreamer_id = Id::<Users>::from(restreamer_id);
        
        if let Some(restreamer) = User::from_id(&mut *transaction, restreamer_id).await? {
            if data.restreamers(&mut transaction).await?.contains(&restreamer) {
                form.context.push_error(form::Error::validation("This user is already a restream coordinator for this event.").with_name("restreamer"));
            }
        } else {
            form.context.push_error(form::Error::validation("There is no user with this ID.").with_name("restreamer"));
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(restreamers_form(transaction, Some(me), uri, csrf.as_ref(), data, RestreamersFormDefaults::AddContext(form.context)).await?)
        } else {
            sqlx::query!("INSERT INTO restreamers (series, event, restreamer) VALUES ($1, $2, $3)", data.series as _, &data.event, restreamer_id as _).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(restreamers_get(series, event))))
        }
    } else {
        RedirectOrContent::Content(restreamers_form(transaction, Some(me), uri, csrf.as_ref(), data, RestreamersFormDefaults::AddContext(form.context)).await?)
    })
}

#[rocket::post("/event/<series>/<event>/configure/restreamers/<restreamer>/remove", data = "<form>")]
pub(crate) async fn remove_restreamer(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, restreamer: Id<Users>, form: Form<Contextual<'_, EmptyForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        let is_game_admin = if let Some(game) = data.game(&mut transaction).await? {
            game.is_admin(&mut transaction, &me).await.map_err(event::Error::from)?
        } else {
            false
        };
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() && !is_game_admin {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        if let Some(restreamer) = User::from_id(&mut *transaction, restreamer).await? {
            if !data.restreamers(&mut transaction).await?.contains(&restreamer) {
                form.context.push_error(form::Error::validation("This user is already not a restream coordinator for this event."));
            }
        } else {
            form.context.push_error(form::Error::validation("There is no user with this ID."));
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(restreamers_form(transaction, Some(me), uri, csrf.as_ref(), data, RestreamersFormDefaults::RemoveContext(restreamer, form.context)).await?)
        } else {
            sqlx::query!("DELETE FROM restreamers WHERE series = $1 AND event = $2 AND restreamer = $3", data.series as _, &data.event, restreamer as _).execute(&**pool).await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(restreamers_get(series, event))))
        }
    } else {
        RedirectOrContent::Content(restreamers_form(transaction, Some(me), uri, csrf.as_ref(), data, RestreamersFormDefaults::RemoveContext(restreamer, form.context)).await?)
    })
}

#[derive(Debug)]
struct SyncResult {
    synced_count: usize,
    failed_count: usize,
    failed_teams: Vec<String>,
}

async fn sync_startgg_participant_ids(transaction: &mut Transaction<'_, Postgres>, event: &Data<'_>, event_slug: &str) -> Result<SyncResult, Box<dyn std::error::Error + Send + Sync>> {
    use crate::config::Config;
    
    let http_client = reqwest::Client::new();
    let config = Config::load().await.map_err(|e| {
        log::error!("Failed to load config for StartGG sync: {}", e);
        format!("Failed to load config: {}", e)
    })?;
    
    log::info!("Starting StartGG participant sync for event: {} (series: {})", event_slug, event.series.slug());

    let entrants = startgg::fetch_event_entrants(&http_client, &config, event_slug).await
        .map_err(|e| {
            // Log detailed error information to systemd log
            match &e {
                startgg::Error::GraphQL(errors) => {
                    log::error!("StartGG GraphQL errors during participant sync for event '{}':", event_slug);
                    for (i, error) in errors.iter().enumerate() {
                        log::error!("  Error {}: {}", i + 1, error.message);
                        if let Some(locations) = &error.locations {
                            for location in locations {
                                log::error!("    Location: line {}, column {}", location.line, location.column);
                            }
                        }
                        if let Some(path) = &error.path {
                            log::error!("    Path: {:?}", path);
                        }
                    }
                }
                startgg::Error::Reqwest(reqwest_err) => {
                    log::error!("StartGG HTTP request failed for event '{}': {}", event_slug, reqwest_err);
                    if let Some(url) = reqwest_err.url() {
                        log::error!("  Request URL: {}", url);
                    }
                }
                startgg::Error::Wheel(wheel_err) => {
                    log::error!("StartGG wheel error for event '{}': {}", event_slug, wheel_err);
                }
                startgg::Error::NoDataNoErrors => {
                    log::error!("StartGG API returned no data and no errors for event '{}'", event_slug);
                }
                startgg::Error::NoQueryMatch(response_data) => {
                    log::error!("StartGG query did not match expected response format for event '{}': {:?}", event_slug, response_data);
                }
            }
            format!("Failed to fetch entrants from StartGG: {}", e)
        })?;
    
    let teams = sqlx::query_as!(Team, r#"
        SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", NULL as challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank 
        FROM teams 
        WHERE series = $1 AND event = $2 AND startgg_id IS NULL AND NOT resigned
    "#, event.series as _, &event.event).fetch_all(&mut **transaction).await
        .map_err(|e| {
            log::error!("Database error while fetching teams for StartGG sync (event: {}, series: {}): {}", event_slug, event.series.slug(), e);
            e
        })?;
    
    log::info!("Found {} teams to sync for event '{}'", teams.len(), event_slug);
    
    let mut synced_count = 0;
    let mut failed_teams = Vec::new();
    
    for team in teams {
        let team_members = sqlx::query!(r#"
            SELECT tm.member, tm.startgg_id AS "startgg_id: startgg::ID",
                   CASE WHEN u.display_source = 'racetime' THEN u.racetime_display_name
                        WHEN u.display_source = 'discord' THEN u.discord_display_name
                   END AS display_name
            FROM team_members tm
            LEFT JOIN users u ON u.id = tm.member
            WHERE tm.team = $1
        "#, team.id as _).fetch_all(&mut **transaction).await
            .map_err(|e| {
                log::error!("Database error while fetching team members for team {} (event: {}): {}", team.id, event_slug, e);
                e
            })?;
        
        if team_members.len() == 1 {
            let member = &team_members[0];
            
            if let Some(entrant_id) = find_matching_entrant(&entrants, member.member.into(), transaction).await
                .map_err(|e| {
                    log::error!("Error finding matching entrant for team {} (event: {}): {}", team.id, event_slug, e);
                    e
                })? {
                sqlx::query!(r#"
                    UPDATE teams 
                    SET startgg_id = $1 
                    WHERE id = $2
                "#, entrant_id as _, team.id as _).execute(&mut **transaction).await
                    .map_err(|e| {
                        log::error!("Database error while updating team {} with StartGG ID {} (event: {}): {}", team.id, entrant_id, event_slug, e);
                        e
                    })?;
                
                synced_count += 1;
                log::debug!("Successfully synced team '{}' (ID: {}) with StartGG entrant {}", 
                    team.name.as_deref().unwrap_or("Unknown"), team.id, entrant_id);
            } else {
                // Could not find a matching entrant
                let team_label = team.name.clone().unwrap_or_else(|| format!("Team {}", team.id));
                let display = if let Some(ref display_name) = member.display_name {
                    format!("{} [{}]", team_label, display_name)
                } else {
                    team_label
                };
                log::warn!("Could not find matching StartGG entrant for team '{}' (ID: {}) in event '{}'",
                    display, team.id, event_slug);
                failed_teams.push(display);
            }
        } else {
            log::warn!("Team '{}' (ID: {}) has {} members, expected 1 for StartGG sync (event: '{}')", 
                team.name.as_deref().unwrap_or("Unknown"), team.id, team_members.len(), event_slug);
        }
    }
    
    let failed_count = failed_teams.len();
    
    log::info!("StartGG participant sync completed for event '{}': {} teams synced, {} teams failed", 
        event_slug, synced_count, failed_count);
    
    if !failed_teams.is_empty() {
        log::warn!("Failed to sync teams for event '{}': {}", event_slug, failed_teams.join(", "));
    }
    
    Ok(SyncResult {
        synced_count,
        failed_count,
        failed_teams,
    })
}

async fn find_matching_entrant(
    entrants: &[(startgg::ID, String, Vec<Option<startgg::ID>>)], 
    member_id: Id<Users>, 
    transaction: &mut Transaction<'_, Postgres>
) -> Result<Option<startgg::ID>, Box<dyn std::error::Error + Send + Sync>> {
    let user = User::from_id(&mut **transaction, member_id).await?.ok_or("User not found")?;
    
    for (entrant_id, entrant_name, participant_user_ids) in entrants {
        if let Some(user_startgg_id) = &user.startgg_id {
            if participant_user_ids.iter().any(|id| id.as_ref() == Some(user_startgg_id)) {
                return Ok(Some(entrant_id.clone()));
            }
        }
        
        let entrant_name_lower = entrant_name.to_lowercase();
        
        let user_names = [
            user.racetime.as_ref().map(|r| r.display_name.as_str()),
            user.discord.as_ref().map(|d| d.display_name.as_str()),
            user.discord.as_ref().and_then(|d| match &d.username_or_discriminator {
                Either::Left(username) => Some(username.as_str()),
                Either::Right(_) => None,
            }),
        ];
        
        for user_name in user_names.iter().filter_map(|&name| name) {
            let user_name_lower = user_name.to_lowercase();
            
            // First try exact match
            if entrant_name_lower == user_name_lower {
                return Ok(Some(entrant_id.clone()));
            }
            
            // If that fails, check if there's a clan tag (| character)
            if entrant_name_lower.contains('|') {
                if let Some(stripped_entrant_name) = entrant_name_lower.split('|').nth(1) {
                    let stripped_entrant_name = stripped_entrant_name.trim();
                    if stripped_entrant_name == user_name_lower {
                        return Ok(Some(entrant_id.clone()));
                    }
                }
            }
        }
    }
    
    Ok(None)
}

#[rocket::get("/api/users/search?<query>")]
pub(crate) async fn search_users(
    pool: &State<PgPool>,
    query: Option<&str>,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    
    let query = query.unwrap_or("").trim();
    if query.is_empty() {
        return Ok(RawText(serde_json::to_string(&Vec::<UserSearchResult>::new())?));
    }
    
    // Search for users by display name, racetime ID, or Discord username
    let users = sqlx::query_as!(
        UserSearchRow,
        r#"
        SELECT 
            id as "id: Id<Users>",
            display_source as "display_source: DisplaySource",
            racetime_id,
            racetime_display_name,
            discord_display_name,
            discord_username
        FROM users 
        WHERE 
            racetime_display_name ILIKE $1 
            OR discord_display_name ILIKE $1 
            OR discord_username ILIKE $1
            OR racetime_id ILIKE $1
        ORDER BY 
            CASE 
                WHEN racetime_display_name ILIKE $1 THEN 1
                WHEN discord_display_name ILIKE $1 THEN 2
                WHEN discord_username ILIKE $1 THEN 3
                WHEN racetime_id ILIKE $1 THEN 4
                ELSE 5
            END,
            racetime_display_name,
            discord_display_name
        LIMIT 20
        "#,
        format!("%{}%", query)
    )
    .fetch_all(&mut *transaction)
    .await?;
    
    let results: Vec<UserSearchResult> = users
        .into_iter()
        .map(|row| {
            let display_name = match row.display_source {
                DisplaySource::RaceTime => row.racetime_display_name.unwrap_or_else(|| "Unknown".to_string()),
                DisplaySource::Discord => row.discord_display_name.unwrap_or_else(|| "Unknown".to_string()),
            };
            
            UserSearchResult {
                id: row.id,
                display_name,
                racetime_id: row.racetime_id,
                discord_username: row.discord_username,
            }
        })
        .collect();
    
    Ok(RawText(serde_json::to_string(&results)?))
}

#[derive(Serialize)]
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
    serializer.serialize_str(&id.to_string())
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

#[rocket::get("/api/restreamers/search?<query>")]
pub(crate) async fn restreamer_search(
    pool: &State<PgPool>,
    query: Option<&str>,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;

    let query = query.unwrap_or("").trim();
    if query.is_empty() {
        return Ok(RawText(serde_json::to_string(&Vec::<UserSearchResult>::new())?));
    }

    // Search for users with racetime connections by display name, racetime ID, or Discord username
    let users = sqlx::query_as!(
        UserSearchRow,
        r#"
        SELECT
            id as "id: Id<Users>",
            display_source as "display_source: DisplaySource",
            racetime_id,
            racetime_display_name,
            discord_display_name,
            discord_username
        FROM users
        WHERE
            racetime_id IS NOT NULL
            AND (
                racetime_display_name ILIKE $1
                OR discord_display_name ILIKE $1
                OR discord_username ILIKE $1
                OR racetime_id ILIKE $1
            )
        ORDER BY
            CASE
                WHEN racetime_display_name ILIKE $1 THEN 1
                WHEN discord_display_name ILIKE $1 THEN 2
                WHEN discord_username ILIKE $1 THEN 3
                WHEN racetime_id ILIKE $1 THEN 4
                ELSE 5
            END,
            racetime_display_name,
            discord_display_name
        LIMIT 20
        "#,
        format!("%{}%", query)
    )
    .fetch_all(&mut *transaction)
    .await?;

    let results: Vec<UserSearchResult> = users
        .into_iter()
        .map(|row| {
            let display_name = match row.display_source {
                DisplaySource::RaceTime => row.racetime_display_name.unwrap_or_else(|| "Unknown".to_string()),
                DisplaySource::Discord => row.discord_display_name.unwrap_or_else(|| "Unknown".to_string()),
            };

            UserSearchResult {
                id: row.id,
                display_name,
                racetime_id: row.racetime_id,
                discord_username: row.discord_username,
            }
        })
        .collect();

    Ok(RawText(serde_json::to_string(&results)?))
}

#[rocket::get("/api/video-urls/suggestions")]
pub(crate) async fn video_url_suggestions(
    pool: &State<PgPool>,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;

    let video_urls = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT video_url::TEXT FROM (
            SELECT video_url FROM races WHERE video_url IS NOT NULL
            UNION
            SELECT video_url_fr FROM races WHERE video_url_fr IS NOT NULL
            UNION
            SELECT video_url_de FROM races WHERE video_url_de IS NOT NULL
            UNION
            SELECT video_url_pt FROM races WHERE video_url_pt IS NOT NULL
        ) AS all_video_urls
        ORDER BY video_url::TEXT
        LIMIT 20
        "#
    )
    .fetch_all(&mut *transaction)
    .await?;

    Ok(RawText(serde_json::to_string(&video_urls)?))
}

// Weekly Schedules Management

enum WeeklySchedulesFormDefaults<'v> {
    None,
    AddContext(Context<'v>),
    DeleteContext(Id<WeeklySchedules>, Context<'v>),
    ToggleContext(Id<WeeklySchedules>, Context<'v>),
}

impl<'v> WeeklySchedulesFormDefaults<'v> {
    fn delete_errors(&self, for_schedule: Id<WeeklySchedules>) -> Vec<&form::Error<'v>> {
        match self {
            Self::DeleteContext(schedule_id, ctx) if *schedule_id == for_schedule => ctx.errors().collect(),
            _ => Vec::default(),
        }
    }

    fn toggle_errors(&self, for_schedule: Id<WeeklySchedules>) -> Vec<&form::Error<'v>> {
        match self {
            Self::ToggleContext(schedule_id, ctx) if *schedule_id == for_schedule => ctx.errors().collect(),
            _ => Vec::default(),
        }
    }

    fn add_errors(&self) -> Vec<&form::Error<'v>> {
        if let Self::AddContext(ctx) = self {
            ctx.errors().collect()
        } else {
            Vec::default()
        }
    }

    fn add_name(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("name")
        } else {
            None
        }
    }

    fn add_frequency(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("frequency_days")
        } else {
            None
        }
    }

    fn add_time(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("time_of_day")
        } else {
            None
        }
    }

    fn add_timezone(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("timezone")
        } else {
            None
        }
    }

    fn add_anchor_date(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("anchor_date")
        } else {
            None
        }
    }

    fn add_active(&self) -> Option<bool> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("active").map(|v| v == "on")
        } else {
            None
        }
    }

    fn add_settings_description(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("settings_description")
        } else {
            None
        }
    }

    fn add_room_open_minutes(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("room_open_minutes_before")
        } else {
            None
        }
    }

    fn add_notification_channel(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("notification_channel_id")
        } else {
            None
        }
    }

    fn add_notification_role(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("notification_role_id")
        } else {
            None
        }
    }

    fn add_racetime_goal(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("racetime_goal")
        } else {
            None
        }
    }

    fn add_racetime_goal_custom(&self) -> Option<&str> {
        if let Self::AddContext(ctx) = self {
            ctx.field_value("racetime_goal_custom")
        } else {
            None
        }
    }
}

fn frequency_display(days: i16) -> &'static str {
    match days {
        7 => "Weekly",
        14 => "Biweekly",
        28 | 30 => "Monthly",
        _ => "Custom",
    }
}

async fn weekly_schedules_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, defaults: WeeklySchedulesFormDefaults<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Configure, true).await?;
    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if let Some(ref me) = me {
        if event.organizers(&mut transaction).await?.contains(me) || me.is_global_admin() {
            let schedules = WeeklySchedule::for_event(&mut transaction, event.series, &event.event).await?;
            let now = Utc::now();
            html! {
                h2 : "Manage Weekly Schedules";
                p : "Weekly schedules define recurring race times for this event. Races are automatically created based on these schedules.";
                @if schedules.is_empty() {
                    p : "No weekly schedules configured for this event.";
                } else {
                    table {
                        thead {
                            tr {
                                th : "Name";
                                th : "Frequency";
                                th : "Time";
                                th : "Timezone";
                                th : "Next Race";
                                th : "Active";
                                th;
                            }
                        }
                        tbody {
                            @for schedule in &schedules {
                                tr {
                                    td : &schedule.name;
                                    td : format!("{} ({} days)", frequency_display(schedule.frequency_days), schedule.frequency_days);
                                    td : schedule.time_of_day.format("%H:%M").to_string();
                                    td : schedule.timezone.name();
                                    td {
                                        @if schedule.active {
                                            : format_datetime(schedule.next_after(now), DateTimeFormat { long: false, running_text: false });
                                        } else {
                                            : "(inactive)";
                                        }
                                    }
                                    td {
                                        @if schedule.active {
                                            : "Yes";
                                        } else {
                                            : "No";
                                        }
                                    }
                                    td {
                                        div(class = "button-row") {
                                            a(class = "button config-edit-btn", href = uri!(weekly_schedule_edit_get(event.series, &*event.event, schedule.id))) : "Edit";
                                            @let toggle_text = if schedule.active { "Pause" } else { "Resume" };
                                            @let toggle_errors = defaults.toggle_errors(schedule.id);
                                            @let (toggle_errors, toggle_button) = button_form(uri!(weekly_schedule_toggle(event.series, &*event.event, schedule.id)), csrf, toggle_errors, toggle_text);
                                            : toggle_errors;
                                            : toggle_button;
                                            @let errors = defaults.delete_errors(schedule.id);
                                            @let (errors, button) = button_form(uri!(weekly_schedule_delete(event.series, &*event.event, schedule.id)), csrf, errors, "Delete");
                                            : errors;
                                            : button;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                h3 : "Add New Schedule";
                @let mut errors = defaults.add_errors();
                : full_form(uri!(weekly_schedule_add(event.series, &*event.event)), csrf, html! {
                    : form_field("name", &mut errors, html! {
                        label(for = "name") : "Schedule Name:";
                        input(type = "text", id = "name", name = "name", value? = defaults.add_name(), placeholder = "e.g., Saturday, Kokiri");
                        label(class = "help") : "(A unique name for this schedule)";
                    });
                    : form_field("frequency_days", &mut errors, html! {
                        label(for = "frequency_days") : "Frequency:";
                        select(id = "frequency_days", name = "frequency_days") {
                            option(value = "7", selected? = defaults.add_frequency().map_or(true, |v| v == "7")) : "Weekly (7 days)";
                            option(value = "14", selected? = defaults.add_frequency().map_or(false, |v| v == "14")) : "Biweekly (14 days)";
                            option(value = "28", selected? = defaults.add_frequency().map_or(false, |v| v == "28")) : "Monthly (28 days)";
                        }
                    });
                    : form_field("time_of_day", &mut errors, html! {
                        label(for = "time_of_day") : "Time of Day:";
                        input(type = "time", id = "time_of_day", name = "time_of_day", value = defaults.add_time().unwrap_or("18:00"));
                    });
                    : form_field("timezone", &mut errors, html! {
                        label(for = "timezone") : "Timezone:";
                        select(id = "timezone", name = "timezone") {
                            option(value = "", disabled = "disabled", selected? = defaults.add_timezone().map_or(true, |v| v.is_empty())) : "Select timezone";
                            option(value = "", id = "local-tz-option") : "Local timezone (detecting...)";
                            option(value = "UTC", selected? = defaults.add_timezone().map_or(false, |v| v == "UTC")) : "UTC";
                            option(value = "Europe/Berlin", selected? = defaults.add_timezone().map_or(false, |v| v == "Europe/Berlin")) : "Europe/Berlin (CET/CEST)";
                            option(value = "America/New_York", selected? = defaults.add_timezone().map_or(false, |v| v == "America/New_York")) : "America/New_York (Eastern)";
                            option(value = "America/Los_Angeles", selected? = defaults.add_timezone().map_or(false, |v| v == "America/Los_Angeles")) : "America/Los_Angeles (Pacific)";
                        }
                        label(class = "help") : "(Defaults to your local timezone)";
                    });
                    : form_field("anchor_date", &mut errors, html! {
                        label(for = "anchor_date") : "Anchor Date:";
                        input(type = "date", id = "anchor_date", name = "anchor_date", value? = defaults.add_anchor_date());
                        label(class = "help") : "(The first occurrence date; future races are calculated from this)";
                    });
                    : form_field("settings_description", &mut errors, html! {
                        label(for = "settings_description") : "Settings Description:";
                        input(type = "text", id = "settings_description", name = "settings_description", value? = defaults.add_settings_description(), placeholder = "e.g., variety, standard");
                        label(class = "help") : "(Short description shown in race room welcome message)";
                    });
                    : form_field("notification_channel_id", &mut errors, html! {
                        label(for = "notification_channel_id") : "Notification Channel ID:";
                        input(type = "text", id = "notification_channel_id", name = "notification_channel_id", value? = defaults.add_notification_channel(), placeholder = "Discord channel ID (optional)");
                        label(class = "help") : "(Discord channel to post race notifications when room opens; leave empty for default)";
                    });
                    : form_field("notification_role_id", &mut errors, html! {
                        label(for = "notification_role_id") : "Notification Role ID:";
                        input(type = "text", id = "notification_role_id", name = "notification_role_id", value? = defaults.add_notification_role(), placeholder = "Discord role ID (optional)");
                        label(class = "help") : "(Discord role to ping in the race announcement; leave empty for no ping)";
                    });
                    : form_field("room_open_minutes_before", &mut errors, html! {
                        label(for = "room_open_minutes_before") : "Open Room (minutes before):";
                        input(type = "number", id = "room_open_minutes_before", name = "room_open_minutes_before", value = defaults.add_room_open_minutes().unwrap_or("30"), min = "1", max = "60");
                        label(class = "help") : "(How many minutes before the race start time to open the room)";
                    });
                    : form_field("racetime_goal", &mut errors, html! {
                        label(for = "racetime_goal") : "racetime.gg Goal Override:";
                        @let current_goal = defaults.add_racetime_goal().unwrap_or("");
                        @let unique_goals = {
                            let mut seen = HashSet::new();
                            all::<Goal>().filter_map(|g| if seen.insert(g.as_str()) { Some(g.as_str()) } else { None }).collect::<Vec<_>>()
                        };
                        select(id = "racetime_goal", name = "racetime_goal") {
                            option(value = "", selected? = current_goal.is_empty()) : "None (use event default)";
                            @for goal_str in &unique_goals {
                                option(value = goal_str, selected? = current_goal == *goal_str) : goal_str;
                            }
                            option(value = "custom", selected? = current_goal == "custom") : "Custom...";
                        }
                        label(class = "help") : "(Override the racetime.gg goal used when opening rooms for this schedule)";
                    });
                    : form_field("racetime_goal_custom", &mut errors, html! {
                        label(for = "racetime_goal_custom") : "Custom Goal String:";
                        input(type = "text", id = "racetime_goal_custom", name = "racetime_goal_custom", value? = defaults.add_racetime_goal_custom(), placeholder = "Exact goal string on racetime.gg");
                        label(class = "help") : "(Only used when \"Custom...\" is selected above)";
                    });
                    : form_field("active", &mut errors, html! {
                        input(type = "checkbox", id = "active", name = "active", checked? = defaults.add_active().unwrap_or(true));
                        label(for = "active") : "Active";
                        label(class = "help") : "(Inactive schedules do not generate races)";
                    });
                }, errors, "Add Schedule");
            }
        } else {
            html! {
                article {
                    p : "This page is for organizers of this event only.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(weekly_schedules_get(event.series, &*event.event)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to configure this event.";
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Manage Weekly Schedules — {}", event.display_name), html! {
        : header;
        : content;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/configure/weekly-schedules")]
pub(crate) async fn weekly_schedules_get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(weekly_schedules_form(transaction, me, uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::None).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddWeeklyScheduleForm {
    #[field(default = String::new())]
    csrf: String,
    name: String,
    frequency_days: i16,
    time_of_day: String,
    timezone: String,
    anchor_date: String,
    active: bool,
    #[field(default = None)]
    settings_description: Option<String>,
    #[field(default = None)]
    notification_channel_id: Option<String>,
    #[field(default = None)]
    notification_role_id: Option<String>,
    #[field(default = Some(30))]
    room_open_minutes_before: Option<i16>,
    /// Empty string = use event default, "custom" = use racetime_goal_custom, otherwise the literal goal string.
    #[field(default = String::new())]
    racetime_goal: String,
    #[field(default = None)]
    racetime_goal_custom: Option<String>,
}

#[rocket::post("/event/<series>/<event>/configure/weekly-schedules", data = "<form>")]
pub(crate) async fn weekly_schedule_add(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, AddWeeklyScheduleForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        if value.name.trim().is_empty() {
            form.context.push_error(form::Error::validation("Schedule name is required.").with_name("name"));
        }
        // Check for duplicate name
        let existing = WeeklySchedule::for_event(&mut transaction, data.series, &data.event).await?;
        if existing.iter().any(|s| s.name.eq_ignore_ascii_case(value.name.trim())) {
            form.context.push_error(form::Error::validation("A schedule with this name already exists.").with_name("name"));
        }
        let time_of_day = match NaiveTime::parse_from_str(&value.time_of_day, "%H:%M") {
            Ok(t) => Some(t),
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid time format. Use HH:MM.").with_name("time_of_day"));
                None
            }
        };
        let timezone: Option<Tz> = match value.timezone.parse() {
            Ok(tz) => Some(tz),
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid timezone.").with_name("timezone"));
                None
            }
        };
        let anchor_date = match NaiveDate::parse_from_str(&value.anchor_date, "%Y-%m-%d") {
            Ok(d) => Some(d),
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid date format. Use YYYY-MM-DD.").with_name("anchor_date"));
                None
            }
        };
        let notification_channel_id = value.notification_channel_id.as_ref()
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    match trimmed.parse::<u64>() {
                        Ok(id) => Some(PgSnowflake(ChannelId::new(id))),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord channel ID. Must be a number.").with_name("notification_channel_id"));
                            None
                        }
                    }
                }
            });
        let notification_role_id = value.notification_role_id.as_ref()
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    match trimmed.parse::<u64>() {
                        Ok(id) => Some(PgSnowflake(RoleId::new(id))),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord role ID. Must be a number.").with_name("notification_role_id"));
                            None
                        }
                    }
                }
            });
        let racetime_goal = match value.racetime_goal.as_str() {
            "" => None,
            "custom" => {
                let custom = value.racetime_goal_custom.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                if custom.is_none() {
                    form.context.push_error(form::Error::validation("Please enter a custom goal string.").with_name("racetime_goal_custom"));
                }
                custom
            }
            other => Some(other.to_string()),
        };
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(weekly_schedules_form(transaction, Some(me), uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::AddContext(form.context)).await?)
        } else {
            let schedule = WeeklySchedule {
                id: Id::new(&mut transaction).await?,
                series: data.series,
                event: data.event.to_string(),
                name: value.name.trim().to_string(),
                frequency_days: value.frequency_days,
                time_of_day: time_of_day.unwrap(),
                timezone: timezone.unwrap(),
                anchor_date: anchor_date.unwrap(),
                active: value.active,
                settings_description: value.settings_description.as_ref().and_then(|s| if s.trim().is_empty() { None } else { Some(s.trim().to_string()) }),
                notification_channel_id,
                notification_role_id,
                room_open_minutes_before: value.room_open_minutes_before.unwrap_or(30),
                racetime_goal,
            };
            schedule.save(&mut transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(weekly_schedules_get(series, event))))
        }
    } else {
        RedirectOrContent::Content(weekly_schedules_form(transaction, Some(me), uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::AddContext(form.context)).await?)
    })
}

#[rocket::post("/event/<series>/<event>/configure/weekly-schedules/<schedule_id>/delete", data = "<form>")]
pub(crate) async fn weekly_schedule_delete(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, schedule_id: Id<WeeklySchedules>, form: Form<Contextual<'_, EmptyForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        if WeeklySchedule::from_id(&mut transaction, schedule_id).await?.is_none() {
            form.context.push_error(form::Error::validation("Schedule not found."));
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(weekly_schedules_form(transaction, Some(me), uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::DeleteContext(schedule_id, form.context)).await?)
        } else {
            WeeklySchedule::delete(&mut transaction, schedule_id).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(weekly_schedules_get(series, event))))
        }
    } else {
        RedirectOrContent::Content(weekly_schedules_form(transaction, Some(me), uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::DeleteContext(schedule_id, form.context)).await?)
    })
}

#[rocket::post("/event/<series>/<event>/configure/weekly-schedules/<schedule_id>/toggle", data = "<form>")]
pub(crate) async fn weekly_schedule_toggle(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, schedule_id: Id<WeeklySchedules>, form: Form<Contextual<'_, EmptyForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if form.value.is_some() {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        let schedule = WeeklySchedule::from_id(&mut transaction, schedule_id).await?;
        if schedule.is_none() {
            form.context.push_error(form::Error::validation("Schedule not found."));
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(weekly_schedules_form(transaction, Some(me), uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::ToggleContext(schedule_id, form.context)).await?)
        } else {
            let mut schedule = schedule.unwrap();
            schedule.active = !schedule.active;
            schedule.save(&mut transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(weekly_schedules_get(series, event))))
        }
    } else {
        RedirectOrContent::Content(weekly_schedules_form(transaction, Some(me), uri, csrf.as_ref(), data, WeeklySchedulesFormDefaults::ToggleContext(schedule_id, form.context)).await?)
    })
}

async fn weekly_schedule_edit_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: Data<'_>, schedule: WeeklySchedule, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    // Get the racetime category slug for this series
    let racetime_category = sqlx::query_scalar!(
        r#"
        SELECT grc.category_slug
        FROM game_series gs
        JOIN game_racetime_connection grc ON gs.game_id = grc.game_id
        WHERE gs.series = $1
        LIMIT 1
        "#,
        event.series as _
    )
    .fetch_optional(&mut *transaction)
    .await?;

    let header = event.header(&mut transaction, me.as_ref(), Tab::Configure, true).await?;
    let content = if event.is_ended() {
        html! {
            article {
                p : "This event has ended and can no longer be configured.";
            }
        }
    } else if let Some(ref me) = me {
        if event.organizers(&mut transaction).await?.contains(me) || me.is_global_admin() {
            let mut errors = ctx.errors().collect_vec();
            html! {
                h2 : format!("Edit Schedule: {}", schedule.name);
                : full_form(uri!(weekly_schedule_edit_post(event.series, &*event.event, schedule.id)), csrf, html! {
                    : form_field("name", &mut errors, html! {
                        label(for = "name") : "Schedule Name:";
                        input(type = "text", id = "name", name = "name", value = ctx.field_value("name").unwrap_or(&schedule.name));
                    });
                    : form_field("frequency_days", &mut errors, html! {
                        label(for = "frequency_days") : "Frequency:";
                        select(id = "frequency_days", name = "frequency_days") {
                            @let current_freq = ctx.field_value("frequency_days").and_then(|v| v.parse::<i16>().ok()).unwrap_or(schedule.frequency_days);
                            option(value = "7", selected? = current_freq == 7) : "Weekly (7 days)";
                            option(value = "14", selected? = current_freq == 14) : "Biweekly (14 days)";
                            option(value = "28", selected? = current_freq == 28) : "Monthly (28 days)";
                        }
                    });
                    : form_field("time_of_day", &mut errors, html! {
                        label(for = "time_of_day") : "Time of Day:";
                        input(type = "time", id = "time_of_day", name = "time_of_day", value = ctx.field_value("time_of_day").unwrap_or(&schedule.time_of_day.format("%H:%M").to_string()));
                    });
                    : form_field("timezone", &mut errors, html! {
                        label(for = "timezone") : "Timezone:";
                        @let current_tz = ctx.field_value("timezone").unwrap_or(schedule.timezone.name());
                        select(id = "timezone", name = "timezone") {
                            @if current_tz != "UTC" && current_tz != "Europe/Berlin" && current_tz != "America/New_York" && current_tz != "America/Los_Angeles" {
                                option(value = current_tz, selected = "selected") : format!("{} (current)", current_tz);
                            }
                            option(value = "", id = "local-tz-option") : "Local timezone (detecting...)";
                            option(value = "UTC", selected? = current_tz == "UTC") : "UTC";
                            option(value = "Europe/Berlin", selected? = current_tz == "Europe/Berlin") : "Europe/Berlin (CET/CEST)";
                            option(value = "America/New_York", selected? = current_tz == "America/New_York") : "America/New_York (Eastern)";
                            option(value = "America/Los_Angeles", selected? = current_tz == "America/Los_Angeles") : "America/Los_Angeles (Pacific)";
                        }
                        label(class = "help") : "(Defaults to your local timezone)";
                    });
                    : form_field("anchor_date", &mut errors, html! {
                        label(for = "anchor_date") : "Anchor Date:";
                        input(type = "date", id = "anchor_date", name = "anchor_date", value = ctx.field_value("anchor_date").unwrap_or(&schedule.anchor_date.format("%Y-%m-%d").to_string()));
                    });
                    : form_field("settings_description", &mut errors, html! {
                        label(for = "settings_description") : "Settings Description:";
                        input(type = "text", id = "settings_description", name = "settings_description", value = ctx.field_value("settings_description").unwrap_or(schedule.settings_description.as_deref().unwrap_or("")), placeholder = "e.g., variety, standard");
                        label(class = "help") : "(Short description shown in race room welcome message)";
                    });
                    @let notification_channel_str = schedule.notification_channel_id.map(|PgSnowflake(id)| id.get().to_string()).unwrap_or_default();
                    : form_field("notification_channel_id", &mut errors, html! {
                        label(for = "notification_channel_id") : "Notification Channel ID:";
                        input(type = "text", id = "notification_channel_id", name = "notification_channel_id", value = ctx.field_value("notification_channel_id").unwrap_or(&notification_channel_str), placeholder = "Discord channel ID (optional)");
                        label(class = "help") : "(Discord channel to post race notifications when room opens; leave empty for default)";
                    });
                    @let notification_role_str = schedule.notification_role_id.map(|PgSnowflake(id)| id.get().to_string()).unwrap_or_default();
                    : form_field("notification_role_id", &mut errors, html! {
                        label(for = "notification_role_id") : "Notification Role ID:";
                        input(type = "text", id = "notification_role_id", name = "notification_role_id", value = ctx.field_value("notification_role_id").unwrap_or(&notification_role_str), placeholder = "Discord role ID (optional)");
                        label(class = "help") : "(Discord role to ping in the race announcement; leave empty for no ping)";
                    });
                    : form_field("room_open_minutes_before", &mut errors, html! {
                        label(for = "room_open_minutes_before") : "Open Room (minutes before):";
                        @let current_minutes = ctx.field_value("room_open_minutes_before").and_then(|v| v.parse::<i16>().ok()).unwrap_or(schedule.room_open_minutes_before);
                        input(type = "number", id = "room_open_minutes_before", name = "room_open_minutes_before", value = current_minutes.to_string(), min = "1", max = "60");
                        label(class = "help") : "(How many minutes before the race start time to open the room)";
                    });
                    : form_field("racetime_goal", &mut errors, html! {
                        label(for = "racetime_goal") : "racetime.gg Goal Override:";
                        @let current_goal = ctx.field_value("racetime_goal").unwrap_or_else(|| {
                            match schedule.racetime_goal.as_deref() {
                                None => "",
                                Some(g) if all::<Goal>().any(|goal| goal.as_str() == g) => g,
                                Some(_) => "custom",
                            }
                        });
                        @let unique_goals = {
                            let mut seen = HashSet::new();
                            all::<Goal>().filter_map(|g| if seen.insert(g.as_str()) { Some(g.as_str()) } else { None }).collect::<Vec<_>>()
                        };
                        select(id = "racetime_goal", name = "racetime_goal", data_racetime_category = racetime_category.as_deref().unwrap_or(""), data_current_goal = current_goal) {
                            option(value = "", selected? = current_goal.is_empty()) : "None (use event default)";
                            @for goal_str in &unique_goals {
                                option(value = goal_str, selected? = current_goal == *goal_str) : goal_str;
                            }
                            option(value = "custom", selected? = current_goal == "custom") : "Custom...";
                        }
                        label(class = "help") : "(Override the racetime.gg goal used when opening rooms for this schedule)";
                    });
                    : form_field("racetime_goal_custom", &mut errors, html! {
                        label(for = "racetime_goal_custom") : "Custom Goal String:";
                        @let custom_val = ctx.field_value("racetime_goal_custom").unwrap_or_else(|| {
                            match schedule.racetime_goal.as_deref() {
                                Some(g) if !all::<Goal>().any(|goal| goal.as_str() == g) => g,
                                _ => "",
                            }
                        });
                        input(type = "text", id = "racetime_goal_custom", name = "racetime_goal_custom", value = custom_val, placeholder = "Exact goal string on racetime.gg");
                        label(class = "help") : "(Only used when \"Custom...\" is selected above)";
                    });
                    : form_field("active", &mut errors, html! {
                        input(type = "checkbox", id = "active", name = "active", checked? = ctx.field_value("active").map_or(schedule.active, |v| v == "on"));
                        label(for = "active") : "Active";
                    });
                }, errors, "Save");
                p {
                    a(href = uri!(weekly_schedules_get(event.series, &*event.event))) : "Back to schedule list";
                }
            }
        } else {
            html! {
                article {
                    p : "This page is for organizers of this event only.";
                }
            }
        }
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(weekly_schedule_edit_get(event.series, &*event.event, schedule.id)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to configure this event.";
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Edit Schedule — {}", event.display_name), html! {
        : header;
        : content;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/configure/weekly-schedules/<schedule_id>/edit")]
pub(crate) async fn weekly_schedule_edit_get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String, schedule_id: Id<WeeklySchedules>) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let schedule = WeeklySchedule::from_id(&mut transaction, schedule_id).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(weekly_schedule_edit_form(transaction, me, uri, csrf.as_ref(), data, schedule, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditWeeklyScheduleForm {
    #[field(default = String::new())]
    csrf: String,
    name: String,
    frequency_days: i16,
    time_of_day: String,
    timezone: String,
    anchor_date: String,
    active: bool,
    #[field(default = None)]
    settings_description: Option<String>,
    #[field(default = None)]
    notification_channel_id: Option<String>,
    #[field(default = None)]
    notification_role_id: Option<String>,
    #[field(default = Some(30))]
    room_open_minutes_before: Option<i16>,
    /// Empty string = use event default, "custom" = use racetime_goal_custom, otherwise the literal goal string.
    #[field(default = String::new())]
    racetime_goal: String,
    #[field(default = None)]
    racetime_goal_custom: Option<String>,
}

#[rocket::post("/event/<series>/<event>/configure/weekly-schedules/<schedule_id>/edit", data = "<form>")]
pub(crate) async fn weekly_schedule_edit_post(pool: &State<PgPool>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, schedule_id: Id<WeeklySchedules>, form: Form<Contextual<'_, EditWeeklyScheduleForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut schedule = WeeklySchedule::from_id(&mut transaction, schedule_id).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    Ok(if let Some(ref value) = form.value {
        if data.is_ended() {
            form.context.push_error(form::Error::validation("This event has ended and can no longer be configured"));
        }
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        if value.name.trim().is_empty() {
            form.context.push_error(form::Error::validation("Schedule name is required.").with_name("name"));
        }
        // Check for duplicate name (excluding current schedule)
        let existing = WeeklySchedule::for_event(&mut transaction, data.series, &data.event).await?;
        if existing.iter().any(|s| s.id != schedule_id && s.name.eq_ignore_ascii_case(value.name.trim())) {
            form.context.push_error(form::Error::validation("A schedule with this name already exists.").with_name("name"));
        }
        let time_of_day = match NaiveTime::parse_from_str(&value.time_of_day, "%H:%M") {
            Ok(t) => Some(t),
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid time format. Use HH:MM.").with_name("time_of_day"));
                None
            }
        };
        let timezone: Option<Tz> = match value.timezone.parse() {
            Ok(tz) => Some(tz),
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid timezone.").with_name("timezone"));
                None
            }
        };
        let anchor_date = match NaiveDate::parse_from_str(&value.anchor_date, "%Y-%m-%d") {
            Ok(d) => Some(d),
            Err(_) => {
                form.context.push_error(form::Error::validation("Invalid date format. Use YYYY-MM-DD.").with_name("anchor_date"));
                None
            }
        };
        let notification_channel_id = value.notification_channel_id.as_ref()
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    match trimmed.parse::<u64>() {
                        Ok(id) => Some(PgSnowflake(ChannelId::new(id))),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord channel ID. Must be a number.").with_name("notification_channel_id"));
                            None
                        }
                    }
                }
            });
        let notification_role_id = value.notification_role_id.as_ref()
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    match trimmed.parse::<u64>() {
                        Ok(id) => Some(PgSnowflake(RoleId::new(id))),
                        Err(_) => {
                            form.context.push_error(form::Error::validation("Invalid Discord role ID. Must be a number.").with_name("notification_role_id"));
                            None
                        }
                    }
                }
            });
        let racetime_goal = match value.racetime_goal.as_str() {
            "" => None,
            "custom" => {
                let custom = value.racetime_goal_custom.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                if custom.is_none() {
                    form.context.push_error(form::Error::validation("Please enter a custom goal string.").with_name("racetime_goal_custom"));
                }
                custom
            }
            other => Some(other.to_string()),
        };
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(weekly_schedule_edit_form(transaction, Some(me), uri, csrf.as_ref(), data, schedule, form.context).await?)
        } else {
            schedule.name = value.name.trim().to_string();
            schedule.frequency_days = value.frequency_days;
            schedule.time_of_day = time_of_day.unwrap();
            schedule.timezone = timezone.unwrap();
            schedule.anchor_date = anchor_date.unwrap();
            schedule.active = value.active;
            schedule.settings_description = value.settings_description.as_ref().and_then(|s| if s.trim().is_empty() { None } else { Some(s.trim().to_string()) });
            schedule.notification_channel_id = notification_channel_id;
            schedule.notification_role_id = notification_role_id;
            schedule.room_open_minutes_before = value.room_open_minutes_before.unwrap_or(30);
            schedule.racetime_goal = racetime_goal;
            schedule.save(&mut transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(weekly_schedules_get(series, event))))
        }
    } else {
        RedirectOrContent::Content(weekly_schedule_edit_form(transaction, Some(me), uri, csrf.as_ref(), data, schedule, form.context).await?)
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct InfoPageForm {
    #[field(default = String::new())]
    csrf: String,
    content: String,
    #[field(default = false)]
    reset: bool,
}

#[rocket::get("/event/<series>/<event>/configure/info-page")]
pub(crate) async fn info_page_get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let is_authorized = if let Some(ref me) = me {
        data.organizers(&mut transaction).await?.contains(me) || me.is_global_admin()
    } else {
        false
    };
    if !is_authorized {
        let header = data.header(&mut transaction, me.as_ref(), Tab::Configure, false).await?;
        return Ok(page(transaction, &me, &uri,
            PageStyle { chests: data.chests().await?, ..PageStyle::default() },
            &format!("Edit Info Page — {}", data.display_name),
            html! {
                : header;
                article {
                    p : "This page is for organizers of this event only.";
                }
            }
        ).await?);
    }
    let existing: Option<String> = sqlx::query_scalar!(
        "SELECT content FROM event_descriptions WHERE series = $1 AND event = $2",
        data.series as _,
        &*data.event,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let has_custom = existing.is_some();
    let initial_content = existing.unwrap_or_else(|| {
        let dn_escaped = data.display_name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        format!("<p>Welcome to the <strong>{}</strong> event.</p>", dn_escaped)
    });
    let csrf = csrf.as_ref();
    let header = data.header(&mut transaction, me.as_ref(), Tab::Configure, false).await?;
    let content = html! {
        : header;
        article {
            h2 : "Edit Info Page";
            p {
                : "Use the editor below to customize the info page for this event. ";
                : "Use the ";
                strong : "Insert organizer list";
                : " button to add a placeholder that automatically shows the current organizer names.";
            }
            form(action = uri!(info_page_post(data.series, &*data.event)), method = "post") {
                : csrf;
                textarea(name = "content", id = "editor-content", style = "min-height: 300px; width: 100%;") {
                    : RawHtml(initial_content);
                }
                div(style = "margin-top: 1em;") {
                    input(type = "submit", value = "Save");
                    @if has_custom {
                        : " ";
                        button(
                            type = "submit",
                            name = "reset",
                            value = "true",
                            style = "margin-left: 1em;",
                            onclick = "return confirm('Delete custom description? The info page will revert to the default content.')"
                        ) : "Delete custom description";
                    }
                }
            }
            script(src = "https://cdnjs.cloudflare.com/ajax/libs/tinymce/8.1.2/tinymce.min.js") {}
            script(src = static_url!("info-page-editor.js")) {}
        }
    };
    Ok(page(transaction, &me, &uri,
        PageStyle { chests: data.chests().await?, ..PageStyle::default() },
        &format!("Edit Info Page — {}", data.display_name),
        content,
    ).await?)
}

#[rocket::post("/event/<series>/<event>/configure/info-page", data = "<form>")]
pub(crate) async fn info_page_post(
    pool: &State<PgPool>,
    me: User,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, InfoPageForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.context.errors().next().is_some() {
        return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(info_page_get(data.series, &*data.event)))));
    }
    if let Some(ref value) = form.value {
        if !data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin() {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        if value.reset {
            sqlx::query!(
                "DELETE FROM event_descriptions WHERE series = $1 AND event = $2",
                data.series as _,
                &*data.event,
            )
            .execute(&mut *transaction)
            .await?;
        } else if !value.content.trim().is_empty() {
            sqlx::query!(
                "INSERT INTO event_descriptions (series, event, content) VALUES ($1, $2, $3)
                 ON CONFLICT (series, event) DO UPDATE SET content = EXCLUDED.content",
                data.series as _,
                &*data.event,
                value.content,
            )
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
    }
    Ok(RedirectOrContent::Redirect(Redirect::to(uri!(crate::event::info(data.series, &*data.event)))))
}
