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
    lang::Language,
    prelude::*,
    user::User,
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

async fn is_game_admin(user: &User, game: &Game, transaction: &mut Transaction<'_, Postgres>) -> Result<bool, GameError> {
    game.is_admin(transaction, user).await
}

async fn get_accessible_games(user: &User, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<Game>, GameError> {
    if user.is_global_admin() {
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
    csrf: Option<CsrfToken>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let games = Game::all(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : "Admin Panel";
            : csrf;

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
                            tr(data_game_name = &game.name) {
                                td : &game.name;
                                td(class = "game-display-name", data_value = &game.display_name) : &game.display_name;
                                td(class = "game-description", data_value = game.description.as_deref().unwrap_or("")) : game.description.as_deref().unwrap_or("");
                                td {
                                    div(class = "actions", style = "display: flex; gap: 8px;") {
                                        button(class = "button edit-btn", onclick = format!("startEditGame('{}')", game.name)) : "Edit";
                                        a(href = uri!(crate::games::get(&game.name, _))) : "Manage";
                                    }
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

            h2 : "Events";
            p {
                a(href = uri!(crate::event::setup::create_get)) : "Create New Event";
            }

            h2 : "ZSR Restreaming Backends";
            p {
                a(href = uri!(zsr_backends)) : "Manage ZSR Backends";
            }

            script(src = static_url!("game-edit.js")) {}
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
    if !me.is_global_admin() {
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
    if !me.is_global_admin() {
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
    .await
    .map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;
    Ok(Redirect::to(uri!(index)))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditGameForm {
    #[field(default = String::new())]
    csrf: String,
    display_name: String,
    #[field(default = String::new())]
    description: String,
}

#[rocket::post("/admin/game/<game_name>/edit", data = "<form>")]
pub(crate) async fn edit_game(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, EditGameForm>>,
) -> Result<rocket::http::Status, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }
    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let description = if value.description.trim().is_empty() { None } else { Some(value.description.trim()) };
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        sqlx::query!(
            "UPDATE games SET display_name = $1, description = $2 WHERE name = $3",
            value.display_name.trim(),
            description,
            game_name,
        )
        .execute(&mut *transaction)
        .await.map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
    }
    Ok(rocket::http::Status::Ok)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddAdminForm {
    #[field(default = String::new())]
    csrf: String,
    admin: String,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct RemoveAdminForm {
    #[field(default = String::new())]
    csrf: String,
}

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
    if !me.is_global_admin() {
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
    if !me.is_global_admin() {
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
        let is_global_admin = me.is_global_admin();
        let is_game_admin = if !is_global_admin {
            is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
        } else {
            false
        };
        
        if !is_global_admin && !is_game_admin {
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
        sqlx::query!(
            r#"INSERT INTO game_admins (game_id, admin_id) VALUES ($1, $2)"#,
            game.id,
            i64::from(admin_id)
        )
        .execute(&mut *transaction)
        .await
        .map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
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
    let is_global_admin = me.is_global_admin();
    let is_game_admin = if !is_global_admin {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else {
        false
    };
    
    if !is_global_admin && !is_game_admin {
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
                a(href = uri!(crate::games::get(&game_name_clone, _))) : "← Back to Game Management";
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


#[rocket::get("/admin/game-management")]
pub(crate) async fn game_management_overview(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    
    // Check if user is trez or has any game admin access
    let is_global_admin = me.is_global_admin();
    let has_game_admin_access = if !is_global_admin {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        let accessible_games = get_accessible_games(&me, &mut transaction).await.map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
        !accessible_games.is_empty()
    } else {
        true
    };
    
    if !is_global_admin && !has_game_admin_access {
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
                                a(href = uri!(crate::games::get(&game.name, _))) : "Manage Game";
                            }
                        }
                    }
                }
            }
            
            @if is_global_admin {
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

// ============================================================================
// ZSR Restreaming Backends Management
// ============================================================================

#[rocket::get("/admin/zsr-backends")]
pub(crate) async fn zsr_backends(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let backends = crate::zsr_export::RestreamingBackend::all(&mut transaction).await.map_err(Error::from)?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : "ZSR Restreaming Backends";

            @if backends.is_empty() {
                p : "No backends configured.";
            } else {
                table {
                    thead {
                        tr {
                            th : "Name";
                            th : "Language";
                            th : "Sheet ID";
                            th : "Columns (ID/Comm/Track/Channel/Notes)";
                            th : "Actions";
                        }
                    }
                    tbody {
                        @for backend in &backends {
                            tr {
                                td : &backend.name;
                                td : backend.language.to_string();
                                td(style = "max-width: 200px; overflow: hidden; text-overflow: ellipsis;") : &backend.google_sheet_id;
                                td : format!("{}/{}/{}/{}/{}",
                                    &backend.hth_export_id_col,
                                    &backend.commentators_col,
                                    &backend.trackers_col,
                                    backend.restream_channel_col.as_deref().unwrap_or("—"),
                                    &backend.notes_col
                                );
                                td {
                                    a(href = uri!(edit_zsr_backend(backend.id))) : "Edit";
                                    : " | ";
                                    form(method = "post", action = uri!(delete_zsr_backend(backend.id)), style = "display: inline;") {
                                        input(type = "hidden", name = "csrf", value = csrf.as_ref().map(|t| t.authenticity_token().to_string()).unwrap_or_default());
                                        button(type = "submit", onclick = "return confirm('Delete this backend? All exports using it will also be deleted.')") : "Delete";
                                    }
                                }
                            }
                        }
                    }
                }
            }

            h2 : "Add New Backend";
            : full_form(uri!(add_zsr_backend), csrf.as_ref(), html! {
                : form_field("name", &mut Vec::new(), html! {
                    label(for = "name") : "Name";
                    input(type = "text", id = "name", name = "name", required, placeholder = "e.g., ZSR, ZSRDE, ZSRFR");
                });
                : form_field("google_sheet_id", &mut Vec::new(), html! {
                    label(for = "google_sheet_id") : "Google Sheet ID";
                    input(type = "text", id = "google_sheet_id", name = "google_sheet_id", required, placeholder = "e.g., 1TDREocBAHKxokCZCfyHUWtMkHoUFwaNR3srU3Wljo1A");
                });
                : form_field("language", &mut Vec::new(), html! {
                    label(for = "language") : "Language";
                    select(id = "language", name = "language", required) {
                        option(value = "en") : "English";
                        option(value = "fr") : "French";
                        option(value = "de") : "German";
                        option(value = "pt") : "Portuguese";
                    }
                });
                : form_field("hth_export_id_col", &mut Vec::new(), html! {
                    label(for = "hth_export_id_col") : "HTH Export ID Column";
                    input(type = "text", id = "hth_export_id_col", name = "hth_export_id_col", required, value = "R", placeholder = "e.g., R");
                });
                : form_field("commentators_col", &mut Vec::new(), html! {
                    label(for = "commentators_col") : "Commentators Column";
                    input(type = "text", id = "commentators_col", name = "commentators_col", required, value = "P", placeholder = "e.g., P");
                });
                : form_field("trackers_col", &mut Vec::new(), html! {
                    label(for = "trackers_col") : "Trackers Column";
                    input(type = "text", id = "trackers_col", name = "trackers_col", required, value = "Q", placeholder = "e.g., Q");
                });
                : form_field("restream_channel_col", &mut Vec::new(), html! {
                    label(for = "restream_channel_col") : "Restream Channel Column (optional)";
                    input(type = "text", id = "restream_channel_col", name = "restream_channel_col", placeholder = "e.g., I");
                });
                : form_field("notes_col", &mut Vec::new(), html! {
                    label(for = "notes_col") : "Notes Column";
                    input(type = "text", id = "notes_col", name = "notes_col", required, value = "N", placeholder = "e.g., N");
                });
                : form_field("dst_formula_standard", &mut Vec::new(), html! {
                    label(for = "dst_formula_standard") : "Standard Time Formula";
                    input(type = "text", id = "dst_formula_standard", name = "dst_formula_standard", required,
                        value = "=IF(A{row}=\"\",\"\",A{row}-Sheet2!$A$1)", placeholder = "Use {row} for row number");
                });
                : form_field("dst_formula_dst", &mut Vec::new(), html! {
                    label(for = "dst_formula_dst") : "Daylight Saving Time Formula";
                    input(type = "text", id = "dst_formula_dst", name = "dst_formula_dst", required,
                        value = "=IF(A{row}=\"\",\"\",A{row}-Sheet2!$A$2)", placeholder = "Use {row} for row number");
                });
            }, Vec::new(), "Add Backend");

            p {
                a(href = uri!(index)) : "Back to Admin Panel";
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        "ZSR Backends — Hyrule Town Hall",
        content,
    ).await.map_err(Error::from)?)
}

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct ZsrBackendForm {
    #[field(default = String::new())]
    csrf: String,
    name: String,
    google_sheet_id: String,
    language: Language,
    hth_export_id_col: String,
    commentators_col: String,
    trackers_col: String,
    restream_channel_col: Option<String>,
    notes_col: String,
    dst_formula_standard: String,
    dst_formula_dst: String,
}

#[rocket::post("/admin/zsr-backends", data = "<form>")]
pub(crate) async fn add_zsr_backend(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ZsrBackendForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let mut transaction = pool.begin().await.map_err(Error::from)?;

        crate::zsr_export::RestreamingBackend::create(
            &mut transaction,
            &value.name,
            &value.google_sheet_id,
            value.language,
            &value.hth_export_id_col,
            &value.commentators_col,
            &value.trackers_col,
            value.restream_channel_col.as_deref().filter(|s| !s.is_empty()),
            &value.notes_col,
            &value.dst_formula_standard,
            &value.dst_formula_dst,
        ).await.map_err(Error::from)?;

        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(zsr_backends)))
}

#[rocket::get("/admin/zsr-backends/<backend_id>/edit")]
pub(crate) async fn edit_zsr_backend(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    backend_id: i32,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }

    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let backend = crate::zsr_export::RestreamingBackend::from_id(&mut transaction, backend_id).await
        .map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    transaction.commit().await.map_err(Error::from)?;

    let content = html! {
        article {
            h1 : format!("Edit Backend — {}", backend.name);

            : full_form(uri!(update_zsr_backend(backend_id)), csrf.as_ref(), html! {
                : form_field("name", &mut Vec::new(), html! {
                    label(for = "name") : "Name";
                    input(type = "text", id = "name", name = "name", required, value = &backend.name);
                });
                : form_field("google_sheet_id", &mut Vec::new(), html! {
                    label(for = "google_sheet_id") : "Google Sheet ID";
                    input(type = "text", id = "google_sheet_id", name = "google_sheet_id", required, value = &backend.google_sheet_id);
                });
                : form_field("language", &mut Vec::new(), html! {
                    label(for = "language") : "Language";
                    select(id = "language", name = "language", required) {
                        option(value = "en", selected? = backend.language == English) : "English";
                        option(value = "fr", selected? = backend.language == French) : "French";
                        option(value = "de", selected? = backend.language == German) : "German";
                        option(value = "pt", selected? = backend.language == Portuguese) : "Portuguese";
                    }
                });
                : form_field("hth_export_id_col", &mut Vec::new(), html! {
                    label(for = "hth_export_id_col") : "HTH Export ID Column";
                    input(type = "text", id = "hth_export_id_col", name = "hth_export_id_col", required, value = &backend.hth_export_id_col);
                });
                : form_field("commentators_col", &mut Vec::new(), html! {
                    label(for = "commentators_col") : "Commentators Column";
                    input(type = "text", id = "commentators_col", name = "commentators_col", required, value = &backend.commentators_col);
                });
                : form_field("trackers_col", &mut Vec::new(), html! {
                    label(for = "trackers_col") : "Trackers Column";
                    input(type = "text", id = "trackers_col", name = "trackers_col", required, value = &backend.trackers_col);
                });
                : form_field("restream_channel_col", &mut Vec::new(), html! {
                    label(for = "restream_channel_col") : "Restream Channel Column (optional)";
                    input(type = "text", id = "restream_channel_col", name = "restream_channel_col", value = backend.restream_channel_col.as_deref().unwrap_or_default());
                });
                : form_field("notes_col", &mut Vec::new(), html! {
                    label(for = "notes_col") : "Notes Column";
                    input(type = "text", id = "notes_col", name = "notes_col", required, value = &backend.notes_col);
                });
                : form_field("dst_formula_standard", &mut Vec::new(), html! {
                    label(for = "dst_formula_standard") : "Standard Time Formula";
                    input(type = "text", id = "dst_formula_standard", name = "dst_formula_standard", required, value = &backend.dst_formula_standard);
                });
                : form_field("dst_formula_dst", &mut Vec::new(), html! {
                    label(for = "dst_formula_dst") : "Daylight Saving Time Formula";
                    input(type = "text", id = "dst_formula_dst", name = "dst_formula_dst", required, value = &backend.dst_formula_dst);
                });
            }, Vec::new(), "Save Changes");

            p {
                a(href = uri!(zsr_backends)) : "Back to ZSR Backends";
            }
        }
    };

    Ok(page(
        pool.begin().await.map_err(Error::from)?,
        &Some(me),
        &uri,
        PageStyle { kind: PageKind::Other, ..PageStyle::default() },
        &format!("Edit Backend — {}", backend.name),
        content,
    ).await.map_err(Error::from)?)
}

#[rocket::post("/admin/zsr-backends/<backend_id>/edit", data = "<form>")]
pub(crate) async fn update_zsr_backend(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    backend_id: i32,
    form: Form<Contextual<'_, ZsrBackendForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let mut transaction = pool.begin().await.map_err(Error::from)?;

        crate::zsr_export::RestreamingBackend::update(
            &mut transaction,
            backend_id,
            &value.name,
            &value.google_sheet_id,
            value.language,
            &value.hth_export_id_col,
            &value.commentators_col,
            &value.trackers_col,
            value.restream_channel_col.as_deref().filter(|s| !s.is_empty()),
            &value.notes_col,
            &value.dst_formula_standard,
            &value.dst_formula_dst,
        ).await.map_err(Error::from)?;

        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(zsr_backends)))
}

#[derive(Debug, FromForm, CsrfForm)]
pub(crate) struct DeleteBackendForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/admin/zsr-backends/<backend_id>/delete", data = "<form>")]
pub(crate) async fn delete_zsr_backend(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    backend_id: i32,
    form: Form<Contextual<'_, DeleteBackendForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        let mut transaction = pool.begin().await.map_err(Error::from)?;
        crate::zsr_export::RestreamingBackend::delete(&mut transaction, backend_id).await.map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(zsr_backends)))
}

// ─── Game Ping Workflow CRUD ─────────────────────────────────────────────────

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddGamePingWorkflowForm {
    #[field(default = String::new())]
    csrf: String,
    language: Language,
    workflow_type: String,
    #[field(default = String::new())]
    ping_interval: String,
    #[field(default = String::new())]
    schedule_time: String,
    #[field(default = String::new())]
    schedule_day_of_week: String,
    #[field(default = String::new())]
    discord_ping_channel: String,
    #[field(default = String::new())]
    lead_times: String,
    #[field(default = false)]
    delete_after_race: bool,
}

#[rocket::post("/game/<game_name>/ping-workflows/add", data = "<form>")]
pub(crate) async fn add_game_ping_workflow(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, AddGamePingWorkflowForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let is_global_admin = me.is_global_admin();
    let is_admin = if !is_global_admin {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else { false };
    if !is_global_admin && !is_admin {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let is_scheduled = value.workflow_type == "scheduled";
        let is_per_race = value.workflow_type == "per_race";

        if !is_scheduled && !is_per_race {
            return Ok(Redirect::to(uri!(crate::games::manage_roles(game_name, _, _))));
        }

        let discord_ping_channel = if value.discord_ping_channel.is_empty() {
            None
        } else {
            value.discord_ping_channel.parse::<i64>().ok()
        };

        if is_scheduled {
            let ping_interval = if value.ping_interval == "weekly" { "weekly" } else { "daily" };
            let schedule_time_str = if value.schedule_time.is_empty() {
                "18:00".to_string()
            } else {
                value.schedule_time.clone()
            };
            let schedule_day: Option<i16> = if value.ping_interval == "weekly" {
                value.schedule_day_of_week.parse::<i16>().ok()
            } else {
                None
            };

            sqlx::query_unchecked!(
                r#"INSERT INTO volunteer_ping_workflows
                    (game_id, language, discord_ping_channel, delete_after_race, workflow_type, ping_interval, schedule_time, schedule_day_of_week)
                VALUES ($1, $2, $3, $4, 'scheduled', $5::ping_interval, $6::time, $7)"#,
                game.id,
                value.language as _,
                discord_ping_channel,
                value.delete_after_race,
                ping_interval,
                schedule_time_str,
                schedule_day,
            )
            .execute(&mut *transaction)
            .await
            .map_err(Error::from)?;
        } else {
            let workflow_id = sqlx::query_scalar!(
                r#"INSERT INTO volunteer_ping_workflows
                    (game_id, language, discord_ping_channel, delete_after_race, workflow_type)
                VALUES ($1, $2, $3, $4, 'per_race') RETURNING id"#,
                game.id,
                value.language as _,
                discord_ping_channel,
                value.delete_after_race,
            )
            .fetch_one(&mut *transaction)
            .await
            .map_err(Error::from)?;

            for part in value.lead_times.split(',') {
                let part = part.trim();
                if part.is_empty() { continue; }
                if let Ok(hours) = part.parse::<i32>() {
                    if hours >= 1 {
                        sqlx::query!(
                            "INSERT INTO volunteer_ping_lead_times (workflow_id, lead_time_hours) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                            workflow_id,
                            hours,
                        )
                        .execute(&mut *transaction)
                        .await
                        .map_err(Error::from)?;
                    }
                }
            }
        }

        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(crate::games::manage_roles(game_name, _, _))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DeleteGamePingWorkflowForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/game/<game_name>/ping-workflows/<workflow_id>/delete", data = "<form>")]
pub(crate) async fn delete_game_ping_workflow(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    workflow_id: i32,
    form: Form<Contextual<'_, DeleteGamePingWorkflowForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let is_global_admin = me.is_global_admin();
    let is_admin = if !is_global_admin {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else { false };
    if !is_global_admin && !is_admin {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        sqlx::query!(
            "DELETE FROM volunteer_ping_workflows WHERE id = $1 AND game_id = $2",
            workflow_id,
            game.id,
        )
        .execute(&mut *transaction)
        .await
        .map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(crate::games::manage_roles(game_name, _, _))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditGamePingWorkflowForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    discord_ping_channel: String,
    #[field(default = false)]
    delete_after_race: bool,
    #[field(default = String::new())]
    ping_interval: String,
    #[field(default = String::new())]
    schedule_time: String,
    #[field(default = String::new())]
    schedule_day_of_week: String,
    #[field(default = String::new())]
    lead_times: String,
}

#[rocket::post("/game/<game_name>/ping-workflows/<workflow_id>/edit", data = "<form>")]
pub(crate) async fn edit_game_ping_workflow(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    workflow_id: i32,
    form: Form<Contextual<'_, EditGamePingWorkflowForm>>,
) -> Result<rocket::http::Status, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let is_global_admin = me.is_global_admin();
    let is_admin = if !is_global_admin {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else { false };
    if !is_global_admin && !is_admin {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        let discord_ping_channel = if value.discord_ping_channel.is_empty() {
            None
        } else {
            value.discord_ping_channel.parse::<i64>().ok()
        };

        let wf = sqlx::query!(
            r#"SELECT workflow_type AS "workflow_type: crate::volunteer_pings::PingWorkflowTypeDb"
               FROM volunteer_ping_workflows WHERE id = $1 AND game_id = $2"#,
            workflow_id,
            game.id,
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(Error::from)?;

        if let Some(wf) = wf {
            match wf.workflow_type {
                crate::volunteer_pings::PingWorkflowTypeDb::Scheduled => {
                    let ping_interval = if value.ping_interval == "weekly" { "weekly" } else { "daily" };
                    let schedule_time_str = if value.schedule_time.is_empty() {
                        "18:00".to_string()
                    } else {
                        value.schedule_time.clone()
                    };
                    let schedule_day: Option<i16> = if value.ping_interval == "weekly" {
                        value.schedule_day_of_week.parse::<i16>().ok()
                    } else {
                        None
                    };
                    sqlx::query_unchecked!(
                        r#"UPDATE volunteer_ping_workflows
                           SET discord_ping_channel = $1, delete_after_race = $2,
                               ping_interval = $3::ping_interval, schedule_time = $4::time,
                               schedule_day_of_week = $5, updated_at = NOW()
                           WHERE id = $6"#,
                        discord_ping_channel,
                        value.delete_after_race,
                        ping_interval,
                        schedule_time_str,
                        schedule_day,
                        workflow_id,
                    )
                    .execute(&mut *transaction)
                    .await
                    .map_err(Error::from)?;
                }
                crate::volunteer_pings::PingWorkflowTypeDb::PerRace => {
                    sqlx::query!(
                        "UPDATE volunteer_ping_workflows SET discord_ping_channel = $1, delete_after_race = $2, updated_at = NOW() WHERE id = $3",
                        discord_ping_channel,
                        value.delete_after_race,
                        workflow_id,
                    )
                    .execute(&mut *transaction)
                    .await
                    .map_err(Error::from)?;

                    sqlx::query!("DELETE FROM volunteer_ping_lead_times WHERE workflow_id = $1", workflow_id)
                        .execute(&mut *transaction)
                        .await
                        .map_err(Error::from)?;

                    for part in value.lead_times.split(',') {
                        let part = part.trim();
                        if part.is_empty() { continue; }
                        if let Ok(hours) = part.parse::<i32>() {
                            if hours >= 1 {
                                sqlx::query!(
                                    "INSERT INTO volunteer_ping_lead_times (workflow_id, lead_time_hours) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                                    workflow_id,
                                    hours,
                                )
                                .execute(&mut *transaction)
                                .await
                                .map_err(Error::from)?;
                            }
                        }
                    }
                }
            }
            transaction.commit().await.map_err(Error::from)?;
        }
    }

    Ok(rocket::http::Status::Ok)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddGamePingWorkflowLeadTimeForm {
    #[field(default = String::new())]
    csrf: String,
    lead_time_hours: i32,
}

#[rocket::post("/game/<game_name>/ping-workflows/<workflow_id>/lead-time/add", data = "<form>")]
pub(crate) async fn add_game_ping_workflow_lead_time(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    workflow_id: i32,
    form: Form<Contextual<'_, AddGamePingWorkflowLeadTimeForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let is_global_admin = me.is_global_admin();
    let is_admin = if !is_global_admin {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else { false };
    if !is_global_admin && !is_admin {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if let Some(ref value) = form.value {
        if value.lead_time_hours >= 1 {
            sqlx::query!(
                "INSERT INTO volunteer_ping_lead_times (workflow_id, lead_time_hours) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                workflow_id,
                value.lead_time_hours,
            )
            .execute(&mut *transaction)
            .await
            .map_err(Error::from)?;
            transaction.commit().await.map_err(Error::from)?;
        }
    }

    Ok(Redirect::to(uri!(crate::games::manage_roles(game_name, _, _))))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DeleteGamePingWorkflowLeadTimeForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/game/<game_name>/ping-workflows/<workflow_id>/lead-time/<hours>/delete", data = "<form>")]
pub(crate) async fn delete_game_ping_workflow_lead_time(
    pool: &State<PgPool>,
    me: Option<User>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    workflow_id: i32,
    hours: i32,
    form: Form<Contextual<'_, DeleteGamePingWorkflowLeadTimeForm>>,
) -> Result<Redirect, StatusOrError<Error>> {
    let me = me.ok_or(Error::Unauthorized)?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await.map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let is_global_admin = me.is_global_admin();
    let is_admin = if !is_global_admin {
        is_game_admin(&me, &game, &mut transaction).await.map_err(Error::from)?
    } else { false };
    if !is_global_admin && !is_admin {
        return Err(Error::Unauthorized.into());
    }

    let mut form = form.into_inner();
    form.verify(&csrf);

    if form.value.is_some() {
        sqlx::query!(
            "DELETE FROM volunteer_ping_lead_times WHERE workflow_id = $1 AND lead_time_hours = $2",
            workflow_id,
            hours,
        )
        .execute(&mut *transaction)
        .await
        .map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
    }

    Ok(Redirect::to(uri!(crate::games::manage_roles(game_name, _, _))))
}