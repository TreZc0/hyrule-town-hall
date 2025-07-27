use crate::{
    event::{
        Data,
        Tab,
    },
    prelude::*,
    racetime_bot::VersionedBranch,
    startgg,
    user::DisplaySource,
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
        if event.organizers(&mut transaction).await?.contains(me) {
            let mut errors = ctx.errors().collect_vec();
            html! {
                @if event.series == Series::Standard && event.event == "w" {
                    p {
                        : "Preroll mode: ";
                        : format!("{:?}", s::WEEKLY_PREROLL_MODE);
                    }
                    p {
                        : "Short settings description (for race room welcome message): ";
                        : s::SHORT_WEEKLY_SETTINGS;
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
                        }
                    }
                    p : "Settings:";
                    pre : serde_json::to_string_pretty(event.single_settings.as_ref().expect("no settings configured for weeklies"))?;
                    p {
                        : "The data above is currently not editable for technical reasons. Please contact ";
                        : User::from_id(&mut *transaction, Id::<Users>::from(14571800683221815449_u64)).await?.ok_or(PageError::TrezUserData(1))?; // Fenhl
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
                    }, errors, "Save");
                }
                h2 : "More options";
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
        if !data.organizers(&mut transaction).await?.contains(&me) {
            form.context.push_error(form::Error::validation("You must be an organizer to configure this event."));
        }
        let min_schedule_notice = if let Some(time) = parse_duration(&value.min_schedule_notice, DurationUnit::Hours) {
            Some(time)
        } else {
            form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'.").with_name("min_schedule_notice"));
            None
        };
        let retime_window = if let Some(retime_window) = &value.retime_window {
            if let Some(time) = parse_duration(retime_window, DurationUnit::Hours) {
                Some(time)
            } else {
                form.context.push_error(form::Error::validation("Duration must be formatted like '1:23:45' or '1h 23m 45s'.").with_name("retime_window"));
                None
            }
        } else {
            None
        };
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
                }
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(super::info(series, event))))
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
        if event.organizers(&mut transaction).await?.contains(me) {
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
        if !data.organizers(&mut transaction).await?.contains(&me) {
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
        if !data.organizers(&mut transaction).await?.contains(&me) {
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
    let config = Config::load().await.map_err(|e| format!("Failed to load config: {}", e))?;
    

    let entrants = startgg::fetch_event_entrants(&http_client, &config, event_slug).await
        .map_err(|e| format!("Failed to fetch entrants from StartGG: {}", e))?;
    
    let teams = sqlx::query_as!(Team, r#"
        SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", NULL as challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank 
        FROM teams 
        WHERE series = $1 AND event = $2 AND startgg_id IS NULL AND NOT resigned
    "#, event.series as _, &event.event).fetch_all(&mut **transaction).await?;
    
    let mut synced_count = 0;
    let mut failed_teams = Vec::new();
    
    for team in teams {
        let team_members = sqlx::query!(r#"
            SELECT member, startgg_id AS "startgg_id: startgg::ID"
            FROM team_members 
            WHERE team = $1
        "#, team.id as _).fetch_all(&mut **transaction).await?;
        
        if team_members.len() == 1 {
            let member = &team_members[0];
            
            if let Some(entrant_id) = find_matching_entrant(&entrants, member.member.into(), transaction).await? {
                sqlx::query!(r#"
                    UPDATE teams 
                    SET startgg_id = $1 
                    WHERE id = $2
                "#, entrant_id as _, team.id as _).execute(&mut **transaction).await?;
                
                synced_count += 1;
            } else {
                // Could not find a matching entrant
                failed_teams.push(team.name.clone().unwrap_or_else(|| format!("Team {}", team.id)));
            }
        }
    }
    
    let failed_count = failed_teams.len();
    
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
