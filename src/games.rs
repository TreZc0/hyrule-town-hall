use crate::{
    game::{Game, GameError},
    prelude::*,
    user::User,
    event::roles::{GameRoleBinding, RoleType, RoleRequest, RoleRequestStatus},
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
                    li { a(href = uri!(get(&game.name))) : &game.display_name; }
                }
            }
        },
    ).await.map_err(Error::from)?)
}

#[allow(dead_code)]
#[rocket::get("/games/<game_name>")]
pub(crate) async fn get(
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
    
    let series = game.series(&mut transaction).await.map_err(Error::from)?;
    let _admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    let is_admin = if let Some(ref me) = me {
        game.is_admin(&mut transaction, me).await.map_err(Error::from)?
    } else {
        false
    };
    
    // Get role bindings for this game
    let role_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    
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
                                    : " - ";
                                    a(href = uri!(crate::event::roles::get(*series_item, &event.event))) : "Manage Roles";
                                    @if event.force_custom_role_binding.unwrap_or(false) {
                                        : " (standalone set of volunteer roles)";
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            h2 : "Game Role Bindings";
            p : "Game-level role bindings apply to all events in this game. Event-specific role bindings can override these.";
            
            @if role_bindings.is_empty() {
                p : "No game-level role bindings configured yet.";
            } else {
                @for binding in &role_bindings {
                    @let my_request = my_requests.iter()
                        .filter(|req| req.role_binding_id == binding.id && !matches!(req.status, RoleRequestStatus::Aborted))
                        .max_by_key(|req| req.created_at);
                    @let has_active_request = my_request.map_or(false, |req| matches!(req.status, RoleRequestStatus::Pending | RoleRequestStatus::Approved));
                    
                    div(class = "role-binding") {
                        h4 : binding.role_type_name;
                        p {
                            : "Required: ";
                            : binding.min_count;
                            : " - ";
                            : binding.max_count;
                            : " volunteers";
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
                            @if let Some(ref notes) = request.notes {
                                p(class = "request-notes") {
                                    : "Your notes: ";
                                    : notes;
                                }
                            }
                        } else {
                            @if let Some(ref me) = me {
                                @let mut errors = Vec::new();
                                                                    : full_form(uri!(apply_for_game_role(&game.name)), csrf.as_ref(), html! {
                                    input(type = "hidden", name = "role_binding_id", value = binding.id.to_string());
                                    : form_field("notes", &mut errors, html! {
                                        label(for = "notes") : "Notes:";
                                        input(type = "text", name = "notes", id = "notes", maxlength = "60", size = "30", placeholder = "Optional notes for organizers");
                                    });
                                }, errors, &format!("Apply for {}", binding.role_type_name));
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
            
            @if is_admin {
                h2 : "Admin Actions";
                p {
                    a(href = uri!(manage_admins(&game.name))) : "Manage Game Admins";
                }
                p {
                    a(href = uri!(manage_roles(&game.name))) : "Manage Game Roles";
                }
            }
        }
    };

    Ok(page(
        transaction,
        &me,
        &uri,
        PageStyle::default(),
        &format!("{} — Games", game.display_name),
        content,
    )
    .await.map_err(Error::from)?)
}

#[allow(dead_code)]
#[rocket::get("/games/<game_name>/admins")]
pub(crate) async fn manage_admins(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    _csrf: Option<CsrfToken>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    
    if !game.is_admin(&mut transaction, &me).await.map_err(Error::from)? {
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
                ul {
                    @for admin in &admins {
                        li : admin.display_name();
                    }
                }
            }
            
            h2 : "Add Admin";
            p : "Admin management functionality will be implemented in a future update.";
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
#[rocket::get("/games/<game_name>/roles")]
pub(crate) async fn manage_roles(
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
    
    if !game.is_admin(&mut transaction, &me).await.map_err(Error::from)? {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    
    let role_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    let all_role_types = RoleType::all(&mut transaction).await.map_err(Error::from)?;
    let all_role_requests = RoleRequest::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    let pending_requests = all_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Pending)).collect::<Vec<_>>();
    let approved_requests = all_role_requests.iter().filter(|req| matches!(req.status, RoleRequestStatus::Approved)).collect::<Vec<_>>();
    
    let content = html! {
        article {
            h1 : format!("Manage Roles — {}", game.display_name);
            
            h2 : "Current Role Bindings";
            @if role_bindings.is_empty() {
                p : "No role bindings configured for this game.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Role Type";
                            th : "Min Count";
                            th : "Max Count";
                            th : "Discord Role";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for binding in &role_bindings {
                            tr {
                                td : binding.role_type_name;
                                td : binding.min_count;
                                td : binding.max_count;
                                td {
                                    @if let Some(discord_role_id) = binding.discord_role_id {
                                        : format!("{}", discord_role_id);
                                    } else {
                                        : "None";
                                    }
                                }
                                td {
                                    @let (errors, delete_button) = button_form(
                                        uri!(remove_game_role_binding(&game_name, binding.id)),
                                        csrf.as_ref(),
                                        Vec::new(),
                                        "Delete"
                                    );
                                    : errors;
                                    div(class = "button-row") : delete_button;
                                }
                            }
                        }
                    }
                }
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
                : form_field("discord_role_id", &mut errors, html! {
                    label(for = "discord_role_id") : "Discord Role ID (optional):";
                    input(type = "text", name = "discord_role_id", id = "discord_role_id", placeholder = "e.g. 123456789012345678");
                });
            }, errors, "Add Role Binding");
            
            h2 : "Pending Role Requests";
            @if pending_requests.is_empty() {
                p : "No pending role requests.";
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
                        @for request in pending_requests {
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
            
            h2 : "Approved Role Requests";
            @if approved_requests.is_empty() {
                p : "No approved role requests.";
            } else {
                table {
                    thead {
                        tr {
                            th : "User";
                            th : "Role Type";
                            th : "Notes";
                            th : "Approved";
                        }
                    }
                    tbody {
                        @for request in approved_requests {
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
                                td : format_datetime(request.updated_at, DateTimeFormat { long: false, running_text: false });
                            }
                            }
                        }
                    }
                }
            }
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

#[rocket::post("/games/<game_name>/apply", data = "<form>")]
pub(crate) async fn apply_for_game_role(
    pool: &State<PgPool>,
    me: Option<User>,
    game_name: &str,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ApplyForGameRoleForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        
        // Check if user already has an active request for this role binding
        if RoleRequest::active_for_user(&mut transaction, value.role_binding_id, me.id).await.map_err(Error::from)? {
            return Ok(Redirect::to(uri!(get(game_name))));
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
    }
    
    Ok(Redirect::to(uri!(get(game_name))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddGameRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
    role_type_id: Id<RoleTypes>,
    min_count: i32,
    max_count: i32,
    #[field(default = String::new())]
    discord_role_id: String,
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
        
        if !game.is_admin(&mut transaction, &me).await.map_err(Error::from)? {
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
        if GameRoleBinding::exists_for_role_type(&mut transaction, game.id, value.role_type_id).await.map_err(Error::from)? {
            return Ok(Redirect::to(uri!(manage_roles(game_name))));
        }
        
        // Add role binding
        GameRoleBinding::create(
            &mut transaction,
            game.id,
            value.role_type_id,
            value.min_count,
            value.max_count,
            discord_role_id,
        ).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name))))
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
        
        if !game.is_admin(&mut transaction, &me).await.map_err(Error::from)? {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Delete the role binding
        GameRoleBinding::delete(&mut transaction, binding_id).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name))))
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
        
        if !game.is_admin(&mut transaction, &me).await.map_err(Error::from)? {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Update the role request status
        RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Approved).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name))))
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
        
        if !game.is_admin(&mut transaction, &me).await.map_err(Error::from)? {
            return Err(StatusOrError::Status(Status::Forbidden));
        }
        
        // Update the role request status
        RoleRequest::update_status(&mut transaction, request, RoleRequestStatus::Rejected).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    
    Ok(Redirect::to(uri!(manage_roles(game_name))))
} 