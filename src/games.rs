use crate::{
    game::{Game, GameError},
    prelude::*,
    user::User,
    event::roles::{GameRoleBinding, RoleType, RoleRequest, RoleRequestStatus, render_language_tabs, render_language_content_box_start, render_language_content_box_end},
    http::{PageError, StatusOrError},
    form::{form_field, full_form, button_form},
    id::{RoleBindings, RoleRequests, RoleTypes},
    time::{format_datetime, DateTimeFormat},
};
use rocket::{uri, form::{Form, Contextual}};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Game(#[from] GameError),
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl From<Error> for StatusOrError<Error> {
    fn from(e: Error) -> Self {
        StatusOrError::Err(e)
    }
}

impl<'r> rocket::response::Responder<'r, 'static> for Error {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'static> {
        let status = if self.is_network_error() {
            Status::BadGateway
        } else {
            Status::InternalServerError
        };
        eprintln!("responded with {status} to request to {}", request.uri());
        eprintln!("display: {self}");
        eprintln!("debug: {self:?}");
        Err(status)
    }
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Game(_) => false,
            Self::Page(e) => e.is_network_error(),
            Self::Sql(_) => false,
        }
    }
}

#[allow(dead_code)]
#[rocket::get("/games")]
pub(crate) async fn list(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let games = Game::all(&mut transaction).await.map_err(Error::from)?;
    Ok(page(
        transaction,
        &me,
        &uri,
        PageStyle::default(),
        "Games",
        html! {
            h1 : "Games";
            ul {
                @for game in &games {
                    li { a(href = uri!(get(&game.name, _))) : &game.display_name; }
                }
            }
        },
    ).await.map_err(Error::from)?)
}

async fn game_page<'a>(
    mut transaction: Transaction<'_, Postgres>,
    me: Option<User>,
    uri: &Origin<'_>,
    game: Game,
    form_errors: Vec<form::Error<'a>>,
    csrf: Option<CsrfToken>,
    lang: Option<Language>,
) -> Result<RawHtml<String>, Error> {
    let series = game.series(&mut transaction).await?;
    let _admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    let is_admin = if let Some(ref me) = me {
        game.is_admin(&mut transaction, me).await.map_err(Error::from)?
    } else {
        false
    };
    
    // Get role bindings for this game
    let role_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;

    // Get active languages and filter bindings
    let active_languages: Vec<Language> = {
        let mut langs: Vec<Language> = role_bindings.iter().map(|b| b.language).collect();
        langs.sort_by_key(|l| l.short_code());
        langs.dedup();
        langs
    };
    let current_language = lang
        .filter(|l| active_languages.contains(l))
        .or_else(|| active_languages.first().copied())
        .unwrap_or(English);
    let filtered_bindings: Vec<&GameRoleBinding> = role_bindings.iter().filter(|b| b.language == current_language).collect();

    // Get user's role requests if logged in
    let my_requests = if let Some(ref me) = me {
        RoleRequest::for_user(&mut transaction, me.id).await.map_err(Error::from)?
    } else {
        Vec::new()
    };
    
    // Get events for each series
    let mut series_with_events = Vec::new();
    for series_item in &series {
        let events = sqlx::query!(
            r#"SELECT event, display_name, force_custom_role_binding 
               FROM events WHERE series = $1 AND listed ORDER BY start ASC"#,
            series_item as _
        )
        .fetch_all(&mut *transaction)
        .await.map_err(Error::from)?;
        
        series_with_events.push((*series_item, events));
    }
    
    let content = html! {
        article {
            h1 : game.display_name;
            
            @if let Some(description) = &game.description {
                p : description;
            }
            
            h2 : "Series and Events";
            @if series_with_events.is_empty() {
                p : "No series associated with this game.";
            } else {
                @for (series_item, events) in &series_with_events {
                    h3 : series_item.display_name();
                    @if events.is_empty() {
                        p : "No events found in this series.";
                    } else {
                        ul {
                            @for event in events {
                                li {
                                    a(href = uri!(crate::event::info(*series_item, &event.event))) : &event.display_name;
                                    @if is_admin || me.as_ref().map_or(false, |me| u64::from(me.id) == 16287394041462225947_u64) {
                                        : " - ";
                                        a(href = uri!(crate::event::roles::get(*series_item, &event.event, _))) : "Manage Roles";
                                    }
                                    @if event.force_custom_role_binding.unwrap_or(false) {
                                        : " (standalone set of volunteer roles)";
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            h2 : "Game Volunteer Roles";
            p : "The coverage through restreams of matches and events requires volunteers. We are very grateful for anyone stepping up to help!";

            @let base_url = format!("/games/{}", game.name);

            // Language tabs (only shown if multiple languages)
            : render_language_tabs(&active_languages, current_language, &base_url);

            // Start content box if we have tabs
            @if active_languages.len() > 1 {
                : render_language_content_box_start();
            }

            @if filtered_bindings.is_empty() {
                p : "No game-level roles available for this language.";
            } else {
                @for binding in &filtered_bindings {
                    @let my_request = my_requests.iter()
                        .filter(|req| req.role_binding_id == binding.id && !matches!(req.status, RoleRequestStatus::Aborted))
                        .max_by_key(|req| req.created_at);
                    @let has_active_request = my_request.map_or(false, |req| matches!(req.status, RoleRequestStatus::Pending | RoleRequestStatus::Approved));
                    
                    div(class = "role-binding") {
                        h4 : binding.role_type_name;
                        p {
                            @if binding.min_count == binding.max_count {
                                : "Required: ";
                                : binding.min_count;
                                @if binding.min_count == 1 {
                                    : " volunteer";
                                } else {
                                    : " volunteers";
                                }
                            } else {
                                : "Required: ";
                                : binding.min_count;
                                : " - ";
                                : binding.max_count;
                                : " volunteers";
                            }
                        }
                        @if let Some(discord_role_id) = binding.discord_role_id {
                            p {
                                : "Discord Role: ";
                                : format!("{}", discord_role_id);
                            }
                        }
                        
                        @if has_active_request {
                            @let request = my_request.unwrap();
                            p(class = "request-status") {
                                : "Your request status: ";
                                @match request.status {
                                    RoleRequestStatus::Pending => {
                                        span(class = "status-pending") : "Pending";
                                    }
                                    RoleRequestStatus::Approved => {
                                        span(class = "status-approved") : "Approved";
                                    }
                                    RoleRequestStatus::Rejected => {
                                        span(class = "status-rejected") : "Rejected";
                                    }
                                    RoleRequestStatus::Aborted => {
                                        span(class = "status-aborted") : "Aborted";
                                    }
                                }
                            }
                            @if request.status == RoleRequestStatus::Pending || request.status == RoleRequestStatus::Rejected {
                                @if let Some(ref notes) = request.notes {
                                    @if !notes.is_empty() {
                                        p(class = "request-notes") {
                                            : "Your application note: ";
                                            : notes;
                                        }
                                    }
                                }
                            }
                            @let errors = form_errors.iter().collect::<Vec<_>>();
                            : full_form(uri!(forfeit_game_role(&game.name)), csrf.as_ref(), html! {
                                input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                            }, errors, "Forfeit Role");
                        } else {
                            @if let Some(ref me) = me {
                                @let mut errors = form_errors.iter().collect::<Vec<_>>();
                                @let button_text = if binding.auto_approve {
                                    format!("Volunteer for {} role", binding.role_type_name)
                                } else {
                                    format!("Apply for {} role", binding.role_type_name)
                                };
                                : full_form(uri!(apply_for_game_role(&game.name)), csrf.as_ref(), html! {
                                    input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                    @if !binding.auto_approve {
                                        : form_field("notes", &mut errors, html! {
                                            label(for = "notes") : "Notes:";
                                            input(type = "text", name = "notes", id = "notes", maxlength = "60", size = "30", placeholder = "Optional notes for organizers");
                                        });
                                    }
                                }, errors, &button_text);
                            } else {
                                p {
                                    a(href = "/login") : "Sign in";
                                    : " to apply for this role.";
                                }
                            }
                        }
                    }
                }
            }

            // Close content box if we have tabs
            @if active_languages.len() > 1 {
                : render_language_content_box_end();
            }

            @if is_admin || me.as_ref().map_or(false, |me| u64::from(me.id) == 16287394041462225947_u64) {
                h2 : "Admin Actions";
                p {
                    a(href = uri!(manage_admins(&game.name))) : "Manage Game Admins";
                }
                p {
                    a(href = uri!(manage_roles(&game.name, _))) : "Manage Game Roles";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        uri,
        PageStyle::default(),
        &format!("{} — Games", game.display_name),
        content,
    )
    .await?)
}

#[allow(dead_code)]
#[rocket::get("/games/<game_name>?<lang>")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    lang: Option<Language>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await.map_err(Error::from)?;

    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    Ok(game_page(transaction, me, &uri, game, Vec::new(), csrf, lang).await.map_err(Error::from)?)
}

#[allow(dead_code)]
#[rocket::get("/games/<game_name>/admins")]
pub(crate) async fn manage_admins(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    
    let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
    let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
    
    if !is_game_admin && !is_global_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    
    let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    
    let content = html! {
        article {
            h1 : format!("Manage Admins — {}", game.display_name);
            
            h2 : "Current Admins";
            @if admins.is_empty() {
                p : "No admins assigned to this game.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Admin";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for admin in &admins {
                            tr {
                                td : admin.display_name();
                                td {
                                    @let (errors, remove_button) = button_form(
                                        uri!(remove_game_admin(&game_name, admin.id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        "Remove"
                                    );
                                    : errors;
                                    div(class = "button-row") : remove_button;
                                }
                            }
                        }
                    }
                }
            }
            
            h3 : "Add Admin";
            @let mut errors = Vec::new();
            : full_form(uri!(add_game_admin(&game_name)), csrf.as_ref(), html! {
                : form_field("admin", &mut errors, html! {
                    label(for = "admin") : "Admin:";
                    div(class = "autocomplete-container") {
                        input(type = "text", id = "admin", name = "admin", autocomplete = "off");
                        div(id = "user-suggestions", class = "suggestions", style = "display: none;") {}
                    }
                    label(class = "help") : "(Start typing a username to search for users. The search will match display names, racetime.gg IDs, and Discord usernames.)";
                });
            }, errors, "Add Admin");
            
            script(src = static_url!("user-search.js")) {}
            
            p {
                a(href = uri!(get(&game_name, _))) : "← Back to Game";
            }
        }
    };

    Ok(page(
        transaction,
        &Some(me),
        &uri,
        PageStyle::default(),
        &format!("Manage Admins — {}", game.display_name),
        content,
    )
    .await.map_err(Error::from)?)
}

#[allow(dead_code)]
#[rocket::get("/games/<game_name>/roles?<lang>")]
pub(crate) async fn manage_roles(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    lang: Option<Language>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await.map_err(Error::from)?;

    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;

    let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
    let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;

    if !is_game_admin && !is_global_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    let role_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    let all_role_types = RoleType::all(&mut transaction).await.map_err(Error::from)?;
    let all_role_requests = RoleRequest::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    let pending_requests = all_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Pending)).collect::<Vec<_>>();
    let approved_requests = all_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Approved)).collect::<Vec<_>>();

    // Get active languages and filter bindings
    let active_languages: Vec<Language> = {
        let mut langs: Vec<Language> = role_bindings.iter().map(|b| b.language).collect();
        langs.sort_by_key(|l| l.short_code());
        langs.dedup();
        langs
    };
    let current_language = lang
        .filter(|l| active_languages.contains(l))
        .or_else(|| active_languages.first().copied())
        .unwrap_or(English);
    let filtered_bindings: Vec<&GameRoleBinding> = role_bindings.iter().filter(|b| b.language == current_language).collect();
    let base_url = format!("/games/{}/roles", game_name);

    let content = html! {
        article {
            h1 : format!("Manage Roles — {}", game.display_name);

            // Language tabs
            : render_language_tabs(&active_languages, current_language, &base_url);

            // Start content box if we have tabs
            @if active_languages.len() > 1 {
                : render_language_content_box_start();
            }

            h2 : format!("Current Role Bindings ({})", current_language);
            @if filtered_bindings.is_empty() {
                p : "No role bindings configured for this language.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Role Type";
                            th : "Min Count";
                            th : "Max Count";
                            th : "Auto-approve";
                            th : "Discord Role";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for binding in &filtered_bindings {
                            tr(data_binding_id = binding.id.to_string()) {
                                td(class = "role-type") : binding.role_type_name;
                                td(class = "min-count", data_value = binding.min_count.to_string()) : binding.min_count;
                                td(class = "max-count", data_value = binding.max_count.to_string()) : binding.max_count;
                                td(class = "auto-approve", data_value = binding.auto_approve.to_string()) {
                                    @if binding.auto_approve {
                                        span(style = "color: green;") : "✓ Yes";
                                    } else {
                                        span(style = "color: red;") : "✗ No";
                                    }
                                }
                                td(class = "discord-role", data_value = binding.discord_role_id.map(|id| id.to_string()).unwrap_or_default()) {
                                    @if let Some(discord_role_id) = binding.discord_role_id {
                                        : format!("{}", discord_role_id);
                                    } else {
                                        : "None";
                                    }
                                }
                                td(style = "text-align: center;") {
                                    div(class = "actions", style = "display: flex; justify-content: center; gap: 8px; flex-wrap: wrap;") {
                                        button(class = "button edit-btn", onclick = format!("startEdit({})", binding.id)) : "Edit";

                                        @let (errors, delete_button) = button_form(
                                            uri!(remove_game_role_binding(&game_name, binding.id)),
                                            csrf.as_ref(),
                                            Vec::new(),
                                            "Delete"
                                        );
                                        : errors;
                                        : delete_button;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Close content box if we have tabs
            @if active_languages.len() > 1 {
                : render_language_content_box_end();
            }

            h3 : "Add Role Binding";
            @let mut errors = Vec::new();
            : full_form(uri!(add_game_role_binding(&game_name)), csrf.as_ref(), html! {
                : form_field("role_type_id", &mut errors, html! {
                    label(for = "role_type_id") : "Role Type:";
                    select(name = "role_type_id", id = "role_type_id") {
                        @for role_type in all_role_types {
                            option(value = role_type.id.to_string()) : role_type.name;
                        }
                    }
                });
                : form_field("min_count", &mut errors, html! {
                    label(for = "min_count") : "Minimum Count:";
                    input(type = "number", name = "min_count", id = "min_count", value = "1", min = "1");
                });
                : form_field("max_count", &mut errors, html! {
                    label(for = "max_count") : "Maximum Count:";
                    input(type = "number", name = "max_count", id = "max_count", value = "1", min = "1");
                });
                : form_field("auto_approve", &mut errors, html! {
                    label(for = "auto_approve") : "Auto-approve:";
                    input(type = "checkbox", name = "auto_approve", id = "auto_approve");
                    label(class = "help") : "(If checked, role requests will be automatically approved without manual review)";
                });
                : form_field("discord_role_id", &mut errors, html! {
                    label(for = "discord_role_id") : "Discord Role ID (optional):";
                    input(type = "text", name = "discord_role_id", id = "discord_role_id", placeholder = "e.g. 123456789012345678");
                });
                : form_field("language", &mut errors, html! {
                    label(for = "language") : "Language:";
                    select(name = "language", id = "language") {
                        option(value = "en") : "English";
                        option(value = "fr") : "French";
                        option(value = "de") : "German";
                        option(value = "pt") : "Portuguese";
                    }
                });
            }, errors, "Add Role Binding");

            h2 : format!("Pending Role Requests ({})", current_language);
            @let filtered_pending_requests = pending_requests.iter().filter(|req| req.language == current_language).collect::<Vec<_>>();
            @if filtered_pending_requests.is_empty() {
                p : "No pending role requests for this language.";
            } else {
                table {
                    thead {
                        tr {
                            th : "User";
                            th : "Role Type";
                            th : "Notes";
                            th : "Applied";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for request in filtered_pending_requests {
                            @if let Some(user) = User::from_id(&mut *transaction, request.user_id).await.map_err(Error::from)? {
                                tr {
                                    td : user.display_name();
                                td : request.role_type_name;
                                td {
                                    @if let Some(ref notes) = request.notes {
                                        : notes;
                                    } else {
                                        : "None";
                                    }
                                }
                                td : format_datetime(request.created_at, DateTimeFormat { long: false, running_text: false });
                                td {
                                    @let (errors, approve_button) = button_form(
                                        uri!(approve_game_role_request(&game_name, request.id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        "Approve"
                                    );
                                    : errors;
                                    div(class = "button-row") : approve_button;
                                    
                                    @let (errors, reject_button) = button_form(
                                        uri!(reject_game_role_request(&game_name, request.id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        "Reject"
                                    );
                                    : errors;
                                    div(class = "button-row") : reject_button;
                                }
                            }
                            }
                        }
                    }
                }
            }

            h2 : format!("Approved Role Requests ({})", current_language);
            @let filtered_approved_requests = approved_requests.iter().filter(|req| req.language == current_language).collect::<Vec<_>>();
            @if filtered_approved_requests.is_empty() {
                p : "No approved role requests for this language.";
            } else {
                // Group by role_binding_id to get per-language grouping
                @let grouped = {
                    let mut map = std::collections::BTreeMap::new();
                    for request in &filtered_approved_requests {
                        map.entry(request.role_binding_id).or_insert_with(Vec::new).push(request);
                    }
                    map
                };
                @for (binding_id, requests) in grouped.iter() {
                    // Look up the binding to get role type name and language
                    @let binding = role_bindings.iter().find(|b| b.id == *binding_id);
                    @if let Some(binding) = binding {
                        details {
                            summary {
                                : format!("{} ({}) ({})", binding.role_type_name, binding.language, requests.len());
                            }
                            table {
                                thead {
                                    tr {
                                        th : "User";
                                        th : "Notes";
                                        th : "Approved";
                                        th : "Actions";
                                    }
                                }
                                tbody {
                                    @for request in requests.iter().sorted_by_key(|r| r.updated_at) {
                                        tr {
                                            td {
                                                @if let Some(user) = User::from_id(&mut *transaction, request.user_id).await.map_err(Error::from)? {
                                                    : user.display_name();
                                                } else {
                                                    : "Unknown User";
                                                }
                                            }
                                            td {
                                                @if let Some(ref notes) = request.notes {
                                                    : notes;
                                                } else {
                                                    : "None";
                                                }
                                            }
                                            td : format_datetime(request.updated_at, DateTimeFormat { long: false, running_text: false });
                                            td {
                                                @let (errors, revoke_button) = button_form(
                                                    uri!(revoke_game_role_request(&game_name, request.id)),
                                                    csrf.as_ref(),
                                                    Vec::new(),
                                                    "Remove"
                                                );
                                                : errors;
                                                div(class = "button-row") : revoke_button;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            script(src = static_url!("game-role-binding-edit.js")) {}
        }
    };

    Ok(page(
        transaction,
        &Some(me),
        &uri,
        PageStyle::default(),
        &format!("Manage Roles — {}", game.display_name),
        content,
    )
    .await.map_err(Error::from)?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ApplyForGameRoleForm {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
    #[field(default = String::new())]
    notes: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ForfeitGameRoleForm {
    #[field(default = String::new())]
    csrf: String,
    role_binding_id: Id<RoleBindings>,
}

#[rocket::post("/games/<game_name>/apply", data = "<form>")]
pub(crate) async fn apply_for_game_role(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    game_name: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ApplyForGameRoleForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    Ok(if let Some(ref value) = form.value {
        let mut transaction = pool.begin().await.map_err(Error::from)?;

        // Check if user already has an active request for this role binding
        if RoleRequest::active_for_user(&mut transaction, value.role_binding_id, me.id).await.map_err(Error::from)? {
            return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(game_name, _)))));
        }

        // Create the role request
        let notes = if value.notes.is_empty() {
            String::new()
        } else {
            value.notes.clone()
        };

        RoleRequest::create(
            &mut transaction,
            value.role_binding_id,
            me.id,
            notes,
        ).await.map_err(Error::from)?;

        transaction.commit().await.map_err(Error::from)?;
        RedirectOrContent::Redirect(Redirect::to(uri!(get(game_name, _))))
    } else {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        let errors = form.context.errors().map(|e| e.clone()).collect::<Vec<_>>();
        RedirectOrContent::Content(
            game_page(transaction, Some(me), &uri, game, errors, csrf, None).await.map_err(Error::from)?
        )
    })
}

#[rocket::post("/games/<game_name>/forfeit", data = "<form>")]
pub(crate) async fn forfeit_game_role(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    game_name: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ForfeitGameRoleForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    Ok(if let Some(ref value) = form.value {
        // Find the role request for this user and role binding
        let role_request = sqlx::query_as!(
            RoleRequest,
            r#"
                SELECT
                    rr.id AS "id: Id<RoleRequests>",
                    rr.role_binding_id AS "role_binding_id: Id<RoleBindings>",
                    rr.user_id AS "user_id: Id<Users>",
                    rr.status AS "status: RoleRequestStatus",
                    rr.notes,
                    rr.created_at,
                    rr.updated_at,
                    rb.series AS "series?: Series",
                    rb.event,
                    rb.min_count,
                    rb.max_count,
                    rt.name AS "role_type_name",
                    rb.language AS "language: Language"
                FROM role_requests rr
                JOIN role_bindings rb ON rr.role_binding_id = rb.id
                JOIN role_types rt ON rb.role_type_id = rt.id
                WHERE rr.role_binding_id = $1 AND rr.user_id = $2 AND rr.status IN ('pending', 'approved')
                ORDER BY rr.created_at DESC
                LIMIT 1
            "#,
            value.role_binding_id as _,
            me.id as _
        )
        .fetch_optional(&mut *transaction)
        .await.map_err(Error::from)?;

        if let Some(request) = role_request {
            // Update the status to aborted
            RoleRequest::update_status(&mut transaction, request.id, RoleRequestStatus::Aborted).await.map_err(Error::from)?;
            transaction.commit().await.map_err(Error::from)?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(game_name, _))))
        } else {
            form.context.push_error(form::Error::validation(
                "No active role request found to forfeit",
            ));
            let errors = form.context.errors().map(|e| e.clone()).collect::<Vec<_>>();
            RedirectOrContent::Content(
                game_page(transaction, Some(me), &uri, game, errors, csrf, None).await.map_err(Error::from)?
            )
        }
    } else {
        let errors = form.context.errors().map(|e| e.clone()).collect::<Vec<_>>();
        RedirectOrContent::Content(
            game_page(transaction, Some(me), &uri, game, errors, csrf, None).await.map_err(Error::from)?
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddGameRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
    role_type_id: Id<RoleTypes>,
    min_count: i32,
    max_count: i32,
    auto_approve: bool,
    #[field(default = String::new())]
    discord_role_id: String,
    language: Language,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveGameRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ApproveGameRoleRequestForm {
    #[field(default = String::new())]
    csrf: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RejectGameRoleRequestForm {
    #[field(default = String::new())]
    csrf: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RevokeGameRoleRequestForm {
    #[field(default = String::new())]
    csrf: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditGameRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
    min_count: i32,
    max_count: i32,
    #[field(default = String::new())]
    discord_role_id: String,
    auto_approve: bool,
}

#[rocket::post("/games/<game_name>/role-bindings", data = "<form>")]
pub(crate) async fn add_game_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, AddGameRoleBindingForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        
        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
        
        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Parse discord_role_id (optional)
        let discord_role_id = if value.discord_role_id.trim().is_empty() {
            None
        } else {
            match value.discord_role_id.trim().parse::<i64>() {
                Ok(id) => Some(id),
                Err(_) => None,
            }
        };
        
        // Check if role binding already exists
        if GameRoleBinding::exists_for_role_type(&mut transaction, game.id, value.role_type_id, value.language).await.map_err(Error::from)? {
            return Ok(Redirect::to(uri!(manage_roles(game_name, _))));
        }

        // Add role binding
        GameRoleBinding::create(
            &mut transaction,
            game.id,
            value.role_type_id,
            value.min_count,
            value.max_count,
            discord_role_id,
            value.auto_approve,
            value.language,
        ).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name, _))))
}

#[rocket::post("/games/<game_name>/role-bindings/<binding_id>/remove", data = "<form>")]
pub(crate) async fn remove_game_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    binding_id: Id<RoleBindings>,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, RemoveGameRoleBindingForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        
        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
        
        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Delete the role binding
        GameRoleBinding::delete(&mut transaction, binding_id).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name, _))))
}

#[rocket::post("/games/<game_name>/roles/<request>/approve", data = "<form>")]
pub(crate) async fn approve_game_role_request(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    request: Id<RoleRequests>,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ApproveGameRoleRequestForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        
        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
        
        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Update the role request status
        RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Approved).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name, _))))
}

#[rocket::post("/games/<game_name>/roles/<request>/reject", data = "<form>")]
pub(crate) async fn reject_game_role_request(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    request: Id<RoleRequests>,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, RejectGameRoleRequestForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        
        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
        
        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Update the role request status
        RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Rejected).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name, _))))
}

#[rocket::post("/games/<game_name>/roles/<request>/revoke", data = "<form>")]
pub(crate) async fn revoke_game_role_request(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    request: Id<RoleRequests>,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, RevokeGameRoleRequestForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;

        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;

        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }

        // Update the role request status to Aborted
        RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Aborted).await.map_err(Error::from)?;

        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(manage_roles(game_name, _))))
}

#[rocket::post("/games/<game_name>/roles/binding/<binding_id>/edit", data = "<form>")]
pub(crate) async fn edit_game_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    binding_id: Id<RoleBindings>,
    _csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, EditGameRoleBindingForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let form = form.into_inner();

    let value = form.value.as_ref().ok_or(StatusOrError::Status(Status::BadRequest))?;

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
    let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;

    if !is_game_admin && !is_global_admin {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    // Validate form data
    if value.min_count < 1 {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    if value.max_count < value.min_count {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    // Parse Discord role ID
    let discord_role_id = if value.discord_role_id.trim().is_empty() {
        None
    } else {
        Some(value.discord_role_id.parse::<i64>().map_err(|_| StatusOrError::Status(Status::BadRequest))?)
    };

    // Update the role binding
    sqlx::query!(
        r#"UPDATE role_bindings
           SET min_count = $1, max_count = $2, discord_role_id = $3, auto_approve = $4
           WHERE id = $5 AND game_id = $6"#,
        value.min_count,
        value.max_count,
        discord_role_id,
        value.auto_approve,
        binding_id as _,
        game.id
    )
    .execute(&mut *transaction)
    .await.map_err(Error::from)?;

    transaction.commit().await.map_err(Error::from)?;
    Ok(Redirect::to(uri!(manage_roles(game_name, _))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddGameAdminForm {
    #[field(default = String::new())]
    csrf: String,
    admin: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveGameAdminForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/games/<game_name>/admins", data = "<form>")]
pub(crate) async fn add_game_admin(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, AddGameAdminForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        
        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
        
        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Parse user ID from form
        let admin_id = match value.admin.parse::<u64>() {
            Ok(id) => Id::<Users>::from(id),
            Err(_) => {
                return Ok(Redirect::to(uri!(manage_admins(game_name))));
            }
        };
        
        // Check if user exists
        let _user = match User::from_id(&mut *transaction, admin_id).await.map_err(Error::from)? {
            Some(u) => u,
            None => {
                return Ok(Redirect::to(uri!(manage_admins(game_name))));
            }
        };
        
        // Check if already admin
        let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
        if admins.iter().any(|u| u.id == admin_id) {
            return Ok(Redirect::to(uri!(manage_admins(game_name))));
        }
        
        // Add user as admin
        sqlx::query!(
            r#"INSERT INTO game_admins (game_id, admin_id) VALUES ($1, $2)"#,
            game.id,
            i64::from(admin_id)
        )
        .execute(&mut *transaction)
        .await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_admins(game_name))))
}

#[rocket::post("/games/<game_name>/admins/<admin_id>/remove", data = "<form>")]
pub(crate) async fn remove_game_admin(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    admin_id: Id<Users>,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, RemoveGameAdminForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        
        let is_game_admin = game.is_admin(&mut transaction, &me).await.map_err(Error::from)?;
        let is_global_admin = u64::from(me.id) == 16287394041462225947_u64;
        
        if !is_game_admin && !is_global_admin {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Remove user as admin
        sqlx::query!(
            r#"DELETE FROM game_admins WHERE game_id = $1 AND admin_id = $2"#,
            game.id,
            i64::from(admin_id)
        )
        .execute(&mut *transaction)
        .await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_admins(game_name))))
}