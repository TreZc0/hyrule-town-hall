use {
    rocket::{
        form::Form,
        http::Status,
        response::Redirect,
        State,
    },
    rocket_csrf::CsrfToken,
    rocket_util::{
        CsrfForm,
        Origin,
        html,
    },
    crate::http::page,
    sqlx::PgPool,
    crate::{
        game::{Game, GameError},
        prelude::*,
        user::User,
        event::{self, roles::RoleBinding},
        series::Series,
        id::{RoleTypes, RoleBindings},
    },
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Game(#[from] GameError),
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("unauthorized")]
    Unauthorized,
}

impl From<Error> for StatusOrError<Error> {
    fn from(e: Error) -> Self {
        StatusOrError::Err(e)
    }
}

impl<'r> rocket::response::Responder<'r, 'static> for Error {
    fn respond_to(self, _request: &'r Request<'_>) -> rocket::response::Result<'static> {
        match self {
            Self::Unauthorized => Ok(Status::Forbidden.respond_to(_request)?),
            _ => Ok(Status::InternalServerError.respond_to(_request)?),
        }
    }
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        false
    }
}

fn is_trez(user: &User) -> bool {
    u64::from(user.id) == 16287394041462225947
}

async fn is_game_admin(user: &User, game: &Game, transaction: &mut Transaction<'_, Postgres>) -> Result<bool, GameError> {
    game.is_admin(transaction, user).await
}

#[rocket::get("/admin")]
pub(crate) async fn index(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let games = Game::all(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : "Admin Panel";
            
            h2 : "Games";
            @if games.is_empty() {
                p : "No games configured.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Name";
                            th : "Display Name";
                            th : "Description";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for game in &games {
                            tr {
                                td : &game.name;
                                td : &game.display_name;
                                td : game.description.as_deref().unwrap_or("");
                                td {
                                    a(href = uri!(manage_game(&game.name))) : "Manage";
                                }
                            }
                        }
                    }
                }
            }
            
            h2 : "Add New Game";
            p {
                a(href = uri!(add_game_form)) : "Add New Game";
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        "Admin Panel — Hyrule Town Hall",
        content,
    ).await.map_err(Error::from)?)
}

#[rocket::get("/admin/game/<game_name>")]
pub(crate) async fn manage_game(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    let series = game.series(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : format!("Manage Game — {}", game.display_name);
            
            h2 : "Game Information";
            table {
                tr {
                    td : "Name:";
                    td : &game.name;
                }
                tr {
                    td : "Display Name:";
                    td : &game.display_name;
                }
                tr {
                    td : "Description:";
                    td : game.description.as_deref().unwrap_or("None");
                }
            }
            
            h2 : "Admins";
            @if admins.is_empty() {
                p : "No admins assigned to this game.";
            } else {
                ul {
                    @for admin in &admins {
                        li : admin.display_name();
                    }
                }
            }
            p {
                a(href = uri!(manage_game_admins(&game.name))) : "Manage Admins";
            }
            
            h2 : "Series";
            @if series.is_empty() {
                p : "No series associated with this game.";
            } else {
                ul {
                    @for series_item in &series {
                        li : series_item.display_name();
                    }
                }
            }
            p {
                a(href = uri!(manage_game_series(&game.name))) : "Manage Series";
            }
            
            p {
                a(href = uri!(index)) : "← Back to Admin Panel";
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Manage Game — {}", game.display_name),
        content,
    ).await.map_err(Error::from)?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddGameForm {
    #[field(default = String::new())]
    csrf: String,
    name: String,
    display_name: String,
    description: String,
}

#[rocket::get("/admin/game/add")]
pub(crate) async fn add_game_form(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }

    let content = html! {
        article {
            h1 : "Add New Game";
            
            form(method = "post", action = uri!(add_game_post)) {
                : csrf;
                
                div {
                    label(for = "name") : "Game Name:";
                    input(type = "text", id = "name", name = "name", required);
                    p(class = "help") : "(Internal identifier, e.g., 'ootr', 'alttpr')";
                }
                
                div {
                    label(for = "display_name") : "Display Name:";
                    input(type = "text", id = "display_name", name = "display_name", required);
                    p(class = "help") : "(Human-readable name, e.g., 'Ocarina of Time Randomizer')";
                }
                
                div {
                    label(for = "description") : "Description:";
                    textarea(id = "description", name = "description", rows = "3");
                    p(class = "help") : "(Optional description of the game)";
                }
                
                div {
                    input(type = "submit", value = "Add Game");
                    a(href = uri!(index)) : "Cancel";
                }
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        "Add New Game — Admin Panel",
        content,
    ).await.map_err(Error::from)?)
}

#[rocket::post("/admin/game/add", data = "<form>")]
pub(crate) async fn add_game_post(
    pool: &State<PgPool>,
    me: Option<User>,
    form: Form<AddGameForm>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    
    sqlx::query!(
        r#"INSERT INTO games (name, display_name, description) VALUES ($1, $2, $3)"#,
        form.name,
        form.display_name,
        if form.description.is_empty() { None } else { Some(&form.description) }
    )
    .execute(&mut *transaction)
    .await.map_err(Error::from)?;
    
    transaction.commit().await.map_err(Error::from)?;
    
    Ok(Redirect::to(uri!(index)))
}

#[derive(FromForm, CsrfForm)]
#[allow(dead_code)]
pub(crate) struct AddAdminForm {
    #[field(default = String::new())]
    csrf: String,
    #[allow(dead_code)]
    admin: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveAdminForm {
    #[field(default = String::new())]
    csrf: String,
}

#[allow(dead_code)]
#[rocket::post("/admin/game/<game_name>/admins/<admin_id>/remove")]
pub(crate) async fn remove_game_admin(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    _csrf: Option<CsrfToken>,
    game_name: &str,
    admin_id: Id<Users>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    // Only remove if the user is currently an admin
    let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    if !admins.iter().any(|u| u.id == admin_id) {
        return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
    }
    sqlx::query!("DELETE FROM game_admins WHERE game_id = $1 AND admin_id = $2", game.id, i64::from(admin_id) as i32)
        .execute(&mut *transaction)
        .await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;
    Ok(Redirect::to(uri!(manage_game_admins(game_name))))
}

#[allow(dead_code)]
#[rocket::post("/admin/game/<game_name>/admins", data = "<form>")]
pub(crate) async fn add_game_admin(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    _csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<AddAdminForm>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    // Parse user ID from form
    let admin_id = match form.admin.parse::<u64>() {
        Ok(id) => Id::<Users>::from(id),
        Err(_) => {
            return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
        }
    };
    // Check if user exists
    let _user = match User::from_id(&mut *transaction, admin_id).await.map_err(Error::from)? {
        Some(u) => u,
        None => {
            return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
        }
    };
    // Check if already admin
    let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    if admins.iter().any(|u| u.id == admin_id) {
        return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
    }
    sqlx::query!("INSERT INTO game_admins (game_id, admin_id) VALUES ($1, $2)", game.id, i64::from(admin_id) as i32)
        .execute(&mut *transaction)
        .await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;
    Ok(Redirect::to(uri!(manage_game_admins(game_name))))
}

#[rocket::get("/admin/game/<game_name>/admins")]
pub(crate) async fn manage_game_admins(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let game_name_clone = game.name.clone();
    let game_display_name = game.display_name.clone();

    let content = html! {
        article {
            h1 : format!("Manage Admins — {}", game_display_name);
            
            h2 : "Current Admins";
            @if admins.is_empty() {
                p : "No admins assigned to this game.";
            } else {
                ul {
                    @for admin in &admins {
                        li {
                            : admin.display_name();
                            form(method = "post", action = uri!(remove_game_admin(&game_name_clone, admin.id))) {
                                : csrf;
                                input(type = "submit", value = "Remove", class = "button");
                            }
                        }
                    }
                }
            }
            
            h2 : "Add Admin";
            form(method = "post", action = uri!(add_game_admin(&game_name_clone))) {
                : csrf;
                div(class = "autocomplete-container") {
                    input(type = "text", id = "admin", name = "admin", autocomplete = "off", placeholder = "Type a username...");
                    div(id = "user-suggestions", class = "suggestions", style = "display: none;") {}
                }
                label(class = "help") : "(Start typing a username to search for users. The search will match display names, racetime.gg IDs, and Discord usernames.)";
                div {
                    input(type = "submit", value = "Add Admin");
                }
            }
            script(src = static_url!("user-search.js")) {}
            p {
                a(href = uri!(manage_game(&game_name_clone))) : "← Back to Game Management";
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Manage Admins — {}", game_display_name),
        content,
    ).await.map_err(Error::from)?)
}

#[rocket::get("/admin/game/<game_name>/series")]
pub(crate) async fn manage_game_series(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    let series = game.series(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : format!("Manage Series — {}", game.display_name);
            
            h2 : "Current Series";
            @if series.is_empty() {
                p : "No series associated with this game.";
            } else {
                ul {
                    @for series_item in &series {
                        li : series_item.display_name();
                    }
                }
            }
            
            h2 : "Add Series";
            p : "Series management functionality will be implemented in a future update.";
            
            p {
                a(href = uri!(manage_game(&game.name))) : "← Back to Game Management";
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Manage Series — {}", game.display_name),
        content,
    ).await.map_err(Error::from)?)
} 

#[rocket::get("/game/<game_name>/manage")]
pub(crate) async fn game_management(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check if user is trez or a game admin
    let is_trez_user = is_trez(&me);
    let is_game_admin = if !is_trez_user {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else {
        false
    };
    
    if !is_trez_user && !is_game_admin {
        return Err(Error::Unauthorized.into());
    }
    
    let series = game.series(&mut transaction).await.map_err(Error::from)?;
    let role_bindings = RoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    let role_types = get_role_types(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    // Now, for each series, fetch events using the pool
    let mut all_events = Vec::new();
    for series_item in &series {
        let events_opt = get_series_events(pool, *series_item).await.map_err(Error::from)?;
        if let Some(events) = events_opt {
            for event in events {
                all_events.push((*series_item, event));
            }
        }
    }

    // Build a Vec<(Series, Vec<EventType>)> for macro use
    let series_with_events = {
        let mut result = Vec::new();
        for series_item in &series {
            let events: Vec<_> = all_events.iter()
                .filter(|(s, _)| s == series_item)
                .map(|(_, e)| e)
                .collect();
            result.push((series_item, events));
        }
        result
    };

    let game_name_clone = game.name.clone();
    let game_display_name = game.display_name.clone();

    let content = html! {
        article {
            h1 : format!("Game Management — {}", game_display_name);
            
            h2 : "Game Information";
            table {
                tr {
                    td : "Name:";
                    td : &game_name_clone;
                }
                tr {
                    td : "Display Name:";
                    td : &game_display_name;
                }
                tr {
                    td : "Description:";
                    td : game.description.as_deref().unwrap_or("None");
                }
            }
            
            h2 : "Series and Events";
            @if series_with_events.is_empty() {
                p : "No series associated with this game.";
            } else {
                @for (series_item, series_events) in &series_with_events {
                    h3 : series_item.display_name();
                    @if series_events.is_empty() {
                        p : "No events in this series.";
                    } else {
                        ul {
                            @for event in &**series_events {
                                li {
                                    a(href = uri!(event::info(**series_item, &*event.event))) : &event.display_name;
                                }
                            }
                        }
                    }
                }
            }
            
            h2 : "Role Bindings";
            @if role_bindings.is_empty() {
                p : "No role bindings for this game.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Role Type";
                            th : "Min Count";
                            th : "Max Count";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for binding in &role_bindings {
                            tr {
                                td : &binding.role_type_name;
                                td : binding.min_count;
                                td : binding.max_count;
                                td {
                                    form(method = "post", action = uri!(remove_role_binding(&game_name_clone, binding.id))) {
                                        : csrf;
                                        input(type = "submit", value = "Remove", class = "button");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            h3 : "Add Role Binding";
            form(method = "post", action = uri!(add_role_binding(&game_name_clone))) {
                : csrf;
                div {
                    label(for = "role_type_id") : "Role Type:";
                    select(id = "role_type_id", name = "role_type_id", required) {
                        @for role_type in &role_types {
                            option(value = role_type.id.to_string()) : &role_type.name;
                        }
                    }
                }
                div {
                    label(for = "min_count") : "Min Count:";
                    input(type = "number", id = "min_count", name = "min_count", value = "1", min = "1", required);
                }
                div {
                    label(for = "max_count") : "Max Count:";
                    input(type = "number", id = "max_count", name = "max_count", value = "1", min = "1", required);
                }
                div {
                    input(type = "submit", value = "Add Role Binding");
                }
            }
            
            p {
                @if is_trez_user {
                    a(href = uri!(manage_game(&game_name_clone))) : "← Back to Admin Panel";
                } else {
                    a(href = uri!(index)) : "← Back to Home";
                }
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Game Management — {}", game_display_name),
        content,
    ).await.map_err(Error::from)?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
    role_type_id: String,
    min_count: i32,
    max_count: i32,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/game/<game_name>/role-bindings", data = "<form>")]
pub(crate) async fn add_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    _csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<AddRoleBindingForm>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check if user is trez or a game admin
    let is_trez_user = is_trez(&me);
    let is_game_admin = if !is_trez_user {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else {
        false
    };
    
    if !is_trez_user && !is_game_admin {
        return Err(Error::Unauthorized.into());
    }
    
    // Parse role type ID
    let role_type_id = match form.role_type_id.parse::<i64>() {
        Ok(id) => Id::<RoleTypes>::from(id),
        Err(_) => {
            return Ok(Redirect::to(uri!(game_management(game_name))));
        }
    };
    
    // Check if role binding already exists
    let existing_bindings = RoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    if existing_bindings.iter().any(|b| b.role_type_id == role_type_id) {
        return Ok(Redirect::to(uri!(game_management(game_name))));
    }
    
    // Add role binding
    sqlx::query!(
        r#"INSERT INTO role_bindings (game_id, role_type_id, min_count, max_count) VALUES ($1, $2, $3, $4)"#,
        game.id,
        role_type_id as _,
        form.min_count,
        form.max_count
    )
    .execute(&mut *transaction)
    .await.map_err(Error::from)?;
    
    transaction.commit().await.map_err(Error::from)?;
    Ok(Redirect::to(uri!(game_management(game_name))))
}

#[rocket::post("/game/<game_name>/role-bindings/<binding_id>/remove")]
pub(crate) async fn remove_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    _csrf: Option<CsrfToken>,
    game_name: &str,
    binding_id: Id<RoleBindings>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    
    // Check if user is trez or a game admin
    let is_trez_user = is_trez(&me);
    let is_game_admin = if !is_trez_user {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else {
        false
    };
    
    if !is_trez_user && !is_game_admin {
        return Err(Error::Unauthorized.into());
    }
    
    // Remove role binding
    sqlx::query!(
        r#"DELETE FROM role_bindings WHERE id = $1 AND game_id = $2"#,
        binding_id as _,
        game.id
    )
    .execute(&mut *transaction)
    .await.map_err(Error::from)?;
    
    transaction.commit().await.map_err(Error::from)?;
    Ok(Redirect::to(uri!(game_management(game_name))))
}

async fn get_series_events<'a>(pool: &'a PgPool, series: Series) -> Result<Option<Vec<event::Data<'a>>>, GameError> {
    let rows = sqlx::query!(
        r#"SELECT event FROM events WHERE series = $1 AND listed ORDER BY start ASC"#,
        series as _
    )
    .fetch_all(pool)
    .await?;
    
    let mut events = Vec::new();
    for row in rows {
        let event_name = row.event.clone();
        let mut tx = pool.begin().await?;
        if let Ok(Some(event_data)) = event::Data::new(&mut tx, series, event_name).await {
            events.push(event_data);
        }
        tx.commit().await?;
    }
    
    Ok(Some(events))
}

async fn get_role_types(transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<event::roles::RoleType>, GameError> {
    event::roles::RoleType::all(transaction).await.map_err(GameError::from)
} 