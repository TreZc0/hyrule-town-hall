use rocket::{
    form::{Form, Contextual},
    http::Status,
    response::Redirect,
    State,
};
use rocket_util::Origin;
use rocket::response::content::RawHtml;
use rocket_csrf::CsrfToken;
use rocket_util::{
    CsrfForm,
    html,
};
use crate::http::page;
use sqlx::{Postgres, Transaction};
use crate::{
    game::{Game, GameError},
    prelude::*,
    user::User,
    event::{self, roles::{GameRoleBinding, RoleType}},
    series::Series,
    id::{RoleTypes, RoleBindings},
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

async fn get_accessible_games(user: &User, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<Game>, GameError> {
    if is_trez(user) {
        // Trez can see all games
        Game::all(transaction).await
    } else {
        // Game admins can only see games they're admin of
        let rows = sqlx::query!(
            r#"SELECT DISTINCT g.id, g.name, g.display_name, g.description, g.created_at, g.updated_at 
               FROM games g 
               JOIN game_admins ga ON g.id = ga.game_id 
               WHERE ga.admin_id = $1 
               ORDER BY g.display_name"#,
            i64::from(user.id)
        )
        .fetch_all(&mut **transaction)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Game {
                id: row.id,
                name: row.name,
                display_name: row.display_name,
                description: row.description,
                created_at: row.created_at.expect("created_at should not be null"),
                updated_at: row.updated_at.expect("updated_at should not be null"),
            })
            .collect())
    }
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
                a(href = uri!(game_management_overview)) : "← Back to Game Management";
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

    eprintln!("Starting add_game_post with name: {}, display_name: {}", form.name, form.display_name);

    let mut transaction = match pool.begin().await {
        Ok(t) => {
            eprintln!("Successfully began transaction");
            t
        },
        Err(e) => {
            eprintln!("Failed to begin transaction: {:?}", e);
            return Err(Error::from(e).into());
        }
    };
    
    eprintln!("Executing INSERT query...");
    let insert_result = sqlx::query!(
        r#"INSERT INTO games (name, display_name, description) VALUES ($1, $2, $3)"#,
        form.name,
        form.display_name,
        if form.description.is_empty() { None } else { Some(&form.description) }
    )
    .execute(&mut *transaction)
    .await;
    
    match insert_result {
        Ok(result) => {
            eprintln!("INSERT query successful, affected rows: {}", result.rows_affected());
        },
        Err(e) => {
            eprintln!("INSERT query failed: {:?}", e);
            return Err(Error::from(e).into());
        }
    }
    
    eprintln!("Committing transaction...");
    let commit_result = transaction.commit().await;
    match commit_result {
        Ok(_) => {
            eprintln!("Transaction committed successfully");
        },
        Err(e) => {
            eprintln!("Transaction commit failed: {:?}", e);
            return Err(Error::from(e).into());
        }
    }
    
    eprintln!("Redirecting to admin index");
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
#[rocket::post("/admin/game/<game_name>/admins/<admin_id>/remove", data = "<form>")]
pub(crate) async fn remove_game_admin(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    admin_id: Id<Users>,
    form: Form<Contextual<'_, RemoveAdminForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let game = Game::from_name(&mut transaction, game_name)
            .await.map_err(Error::from)?
            .ok_or(StatusOrError::Status(Status::NotFound))?;
        // Only remove if the user is currently an admin
        let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
        if !admins.iter().any(|u| u.id == admin_id) {
            return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
        }
        sqlx::query!("DELETE FROM game_admins WHERE game_id = $1 AND admin_id = $2", game.id, i64::from(admin_id))
            .execute(&mut *transaction)
            .await.map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
    }
    Ok(Redirect::to(uri!(manage_game_admins(game_name))))
}

#[allow(dead_code)]
#[rocket::post("/admin/game/<game_name>/admins", data = "<form>")]
pub(crate) async fn add_game_admin(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, AddAdminForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !is_trez(&me) {
        return Err(Error::Unauthorized.into());
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
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
            return Err(StatusOrError::Err(Error::Unauthorized));
        }
        
        // Parse user ID from form
        let admin_id = match value.admin.parse::<u64>() {
            Ok(id) => {
                Id::<Users>::from(id)
            },
            Err(_) => {
                return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
            }
        };
        // Check if user exists
        let _user = match User::from_id(&mut *transaction, admin_id).await.map_err(Error::from)? {
            Some(u) => {
                u
            },
            None => {
                return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
            }
        };
        // Check if already admin
        let admins = game.admins(&mut transaction).await.map_err(Error::from)?;
        if admins.iter().any(|u| u.id == admin_id) {
            return Ok(Redirect::to(uri!(manage_game_admins(game_name))));
        }
        
        // Add user as admin
        eprintln!("Adding user {} as admin for game {} (game_id: {})", admin_id, game_name, game.id);
        let insert_result = sqlx::query!(
            r#"INSERT INTO game_admins (game_id, admin_id) VALUES ($1, $2)"#,
            game.id,
            i64::from(admin_id)
        )
        .execute(&mut *transaction)
        .await;
        
        match insert_result {
            Ok(result) => {
                eprintln!("INSERT game_admins successful, affected rows: {}", result.rows_affected());
            },
            Err(e) => {
                eprintln!("INSERT game_admins failed: {:?}", e);
                return Err(Error::from(e).into());
            }
        }
        
        eprintln!("Committing game admin transaction...");
        let commit_result = transaction.commit().await;
        match commit_result {
            Ok(_) => {
                eprintln!("Game admin transaction committed successfully");
            },
            Err(e) => {
                eprintln!("Game admin transaction commit failed: {:?}", e);
                return Err(Error::from(e).into());
            }
        }
    }
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
    
    let mut transaction = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to begin transaction: {:?}", e);
            return Err(StatusOrError::Err(Error::from(e)));
        }
    };
    
    let game = match Game::from_name(&mut transaction, game_name).await {
        Ok(Some(g)) => g,
        Ok(None) => return Err(StatusOrError::Status(Status::NotFound)),
        Err(e) => {
            eprintln!("Failed to get game by name '{}': {:?}", game_name, e);
            return Err(StatusOrError::Err(Error::from(e)));
        }
    };
    
    // Check if user is trez or a game admin
    let is_trez_user = is_trez(&me);
    let is_game_admin = if !is_trez_user {
        match is_game_admin(&me, &game, &mut transaction).await {
            Ok(admin) => admin,
            Err(e) => {
                eprintln!("Failed to check game admin status: {:?}", e);
                return Err(StatusOrError::Err(Error::from(e)));
            }
        }
    } else {
        false
    };
    
    if !is_trez_user && !is_game_admin {
        return Err(StatusOrError::Err(Error::Unauthorized));
    }
    
    let series = match game.series(&mut transaction).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to get game series: {:?}", e);
            return Err(StatusOrError::Err(Error::from(e)));
        }
    };

    // Now, for each series, fetch events using the pool
    let mut series_with_events = Vec::new();
    for series_item in &series {
        match get_series_events(pool, *series_item).await {
            Ok(Some(events)) => {
                series_with_events.push((series_item, events));
            }
            Ok(None) => {
                // No events for this series, but we still want to show it
                series_with_events.push((series_item, Vec::new()));
            }
            Err(e) => {
                eprintln!("Failed to get series events for {:?}: {:?}", series_item, e);
                return Err(StatusOrError::Err(Error::from(e)));
            }
        }
    }

    let game_name_clone = game.name.clone();
    let game_display_name = game.display_name.clone();

    // Get game role bindings and role types
    let game_role_bindings = GameRoleBinding::for_game(&mut transaction, game.id).await.map_err(Error::from)?;
    let all_role_types = RoleType::all(&mut transaction).await.map_err(Error::from)?;

    match transaction.commit().await {
        Ok(_) => {},
        Err(e) => {
            eprintln!("Failed to commit transaction: {:?}", e);
            return Err(StatusOrError::Err(Error::from(e)));
        }
    }

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
                        p : "No events found in this series.";
                    } else {
                        ul {
                            @for (event_name, display_name) in series_events {
                                li {
                                    a(href = uri!(event::info(*series_item, &*event_name))) : display_name;
                                    : " - ";
                                    a(href = uri!(event::roles::get(*series_item, &*event_name))) : "Manage Roles";
                                }
                            }
                        }
                    }
                }
            }
            
            h2 : "Game Role Bindings";
            p : "Game-level role bindings apply to all events in this game. Event-specific role bindings can override these.";
            
            @if game_role_bindings.is_empty() {
                p : "No game-level role bindings configured yet.";
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
                        @for binding in &game_role_bindings {
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
                                        uri!(remove_game_role_binding(&game_name_clone, binding.id)),
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
            
            h3 : "Add Game Role Binding";
            @let mut errors = Vec::new();
            : full_form(uri!(add_game_role_binding(&game_name_clone)), csrf.as_ref(), html! {
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
            }, errors, "Add Game Role Binding");
            
            p {
                a(href = uri!(game_management_overview)) : "← Back to Game Management";
            }
        }
    };

    let page_transaction = pool.begin().await.map_err(Error::from)?;
    match page(
        page_transaction,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Game Management — {}", game_display_name),
        content,
    ).await {
        Ok(page_content) => Ok(page_content),
        Err(e) => {
            eprintln!("Failed to generate page: {:?}", e);
            Err(StatusOrError::Err(Error::from(e)))
        }
    }
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
pub(crate) struct RemoveRoleBindingForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/game/<game_name>/role-bindings", data = "<form>")]
pub(crate) async fn add_game_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, AddGameRoleBindingForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if let Some(ref value) = form.value {
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
            return Err(StatusOrError::Err(Error::Unauthorized));
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
            return Ok(Redirect::to(uri!(game_management(game_name))));
        }
        
        // Add role binding
        GameRoleBinding::create(
            &mut transaction,
            game.id,
            value.role_type_id,
            value.min_count,
            value.max_count,
            discord_role_id,
            false, // auto_approve - default to false for game role bindings
        ).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    Ok(Redirect::to(uri!(game_management(game_name))))
}

#[rocket::post("/game/<game_name>/role-bindings/<binding_id>/remove", data = "<form>")]
pub(crate) async fn remove_game_role_binding(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    binding_id: Id<RoleBindings>,
    form: Form<Contextual<'_, RemoveRoleBindingForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    
    if form.value.is_some() {
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
            return Err(StatusOrError::Err(Error::Unauthorized));
        }
        
        // Delete the role binding
        GameRoleBinding::delete(&mut transaction, binding_id).await.map_err(Error::from)?;
        
        transaction.commit().await.map_err(Error::from)?;
    }
    Ok(Redirect::to(uri!(game_management(game_name))))
}

async fn get_series_events<'a>(pool: &'a PgPool, series: Series) -> Result<Option<Vec<(String, String)>>, GameError> {
    let rows = sqlx::query!(
        r#"SELECT event, display_name FROM events WHERE series = $1 AND listed ORDER BY start ASC"#,
        series.slug()
    )
    .fetch_all(pool)
    .await?;
    
    let events = rows.into_iter()
        .map(|row| (row.event, row.display_name))
        .collect();
    
    Ok(Some(events))
}


#[rocket::get("/admin/game-management")]
pub(crate) async fn game_management_overview(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    
    // Check if user is trez or has any game admin access
    let is_trez_user = is_trez(&me);
    let has_game_admin_access = if !is_trez_user {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let accessible_games = get_accessible_games(&me, &mut transaction).await.map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
        !accessible_games.is_empty()
    } else {
        true
    };
    
    if !is_trez_user && !has_game_admin_access {
        return Err(StatusOrError::Err(Error::Unauthorized));
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let games = get_accessible_games(&me, &mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : "Game Management";
            
            @if games.is_empty() {
                p : "No games available for management.";
            } else {
                div(class = "game-grid") {
                    @for game in &games {
                        div(class = "game-card") {
                            h3 : &game.display_name;
                            @if let Some(description) = &game.description {
                                p : description;
                            }
                            p {
                                a(href = uri!(game_management(&game.name))) : "Manage Game";
                            }
                        }
                    }
                }
            }
            
            @if is_trez_user {
                p {
                    a(href = uri!(index)) : "← Back to Admin Panel";
                }
            } else {
                p {
                    a(href = uri!(crate::http::index)) : "← Back to Home";
                }
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        "Game Management — Hyrule Town Hall",
        content,
    ).await.map_err(Error::from)?)
} 