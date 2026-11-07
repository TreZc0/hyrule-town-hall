use crate::{game::Game, games::Error, prelude::*};

// Only fetch whether a secret is set; never send stored secrets to the browser.
#[derive(sqlx::FromRow)]
struct Connection {
    id: i32,
    category_slug: String,
    client_id: String,
    has_secret: bool,
}

async fn connections(
    transaction: &mut Transaction<'_, Postgres>,
    game_id: i32,
) -> sqlx::Result<Vec<Connection>> {
    sqlx::query_as(
        "SELECT id, category_slug, client_id, client_secret <> '' AS has_secret
         FROM game_racetime_connection WHERE game_id = $1 ORDER BY id",
    )
    .bind(game_id)
    .fetch_all(&mut **transaction)
    .await
}

async fn authorized_game(
    transaction: &mut Transaction<'_, Postgres>,
    me: &User,
    game_name: &str,
) -> Result<Game, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let game = Game::from_name(transaction, game_name)
        .await
        .map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(game)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ConnectionForm {
    #[field(default = String::new())]
    csrf: String,
    connection_id: Option<i32>,
    category_slug: String,
    client_id: String,
    #[field(default = String::new())]
    client_secret: String,
}

fn validate(value: &ConnectionForm, has_secret: bool) -> Result<(), &'static str> {
    let slug = value.category_slug.trim();
    if slug.is_empty()
        || slug.len() > 255
        || !slug
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(
            "Enter a category slug of up to 255 lowercase letters, numbers, or hyphens (for example, ootr), without a URL.",
        );
    }
    if value.client_id.trim().is_empty() || value.client_id.trim().chars().count() > 255 {
        return Err("Enter a client ID of up to 255 characters.");
    }
    if value.client_secret.trim().is_empty() && !has_secret {
        return Err("Enter a client secret to configure this connection.");
    }
    if value.client_secret.trim().chars().count() > 255 {
        return Err("The client secret must be at most 255 characters.");
    }
    Ok(())
}

async fn save(
    transaction: &mut Transaction<'_, Postgres>,
    game_id: i32,
    value: &ConnectionForm,
) -> sqlx::Result<Result<(), &'static str>> {
    // The legacy schema permits multiple connections per game, but room creation
    // requires exactly one. Serialize saves, including creation and category reuse.
    sqlx::query("LOCK TABLE game_racetime_connection IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut **transaction)
        .await?;
    let current = connections(transaction, game_id).await?;
    if current.len() > 1 {
        return Ok(Err(
            "This game has multiple racetime connections. An administrator must resolve the duplicate database entries before editing here.",
        ));
    }
    let current = current.first();
    if value.connection_id != current.map(|connection| connection.id) {
        return Ok(Err(
            "The connection changed since this form was opened. Reload the page before saving.",
        ));
    }
    if let Err(error) = validate(
        value,
        current.is_some_and(|connection| connection.has_secret),
    ) {
        return Ok(Err(error));
    }
    let category_slug = value.category_slug.trim();
    // Bot credential lookup uses the category alone, so it must be unambiguous.
    let in_use: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM game_racetime_connection
         WHERE category_slug = $1 AND game_id IS DISTINCT FROM $2)",
    )
    .bind(category_slug)
    .bind(game_id)
    .fetch_one(&mut **transaction)
    .await?;
    if in_use {
        return Ok(Err(
            "This racetime category is already connected to another game.",
        ));
    }
    if let Some(current) = current {
        sqlx::query(
            "UPDATE game_racetime_connection
             SET category_slug = $1, client_id = $2,
                 client_secret = COALESCE(NULLIF($3, ''), client_secret), updated_at = NOW()
             WHERE id = $4 AND game_id = $5",
        )
        .bind(category_slug)
        .bind(value.client_id.trim())
        .bind(value.client_secret.trim())
        .bind(current.id)
        .bind(game_id)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO game_racetime_connection (game_id, category_slug, client_id, client_secret)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(game_id)
        .bind(category_slug)
        .bind(value.client_id.trim())
        .bind(value.client_secret.trim())
        .execute(&mut **transaction)
        .await?;
    }
    Ok(Ok(()))
}

fn content(
    game: &Game,
    current: &[Connection],
    csrf: Option<&CsrfToken>,
    ctx: &Context<'_>,
    error: Option<&str>,
    saved: bool,
) -> RawHtml<String> {
    let connection = current.first();
    html! {
        article {
            h1 : format!("Manage racetime Connection — {}", game.display_name);
            p : "Configure the racetime category and bot credentials used to create and manage race rooms for this game.";
            p : "Restart the application after saving so the running racetime bot picks up the changes.";
            @if saved {
                p(class = "success") : "Racetime connection saved.";
            }
            @if let Some(error) = error {
                p(class = "error") : error;
            }
            @if current.len() > 1 {
                p(class = "error") : "This game has multiple racetime connections, but room creation requires exactly one. An administrator must resolve the duplicate database entries before editing here.";
                ul {
                    @for connection in current {
                        li : &connection.category_slug;
                    }
                }
            } else {
                @if connection.is_none() {
                    p : "No racetime connection configured yet.";
                }
                : full_form(uri!(post(&game.name)), csrf, html! {
                    @if let Some(connection) = connection {
                        input(type = "hidden", name = "connection_id", value = connection.id.to_string());
                    }
                    fieldset {
                        label(for = "category_slug") : "Category slug";
                        input(type = "text", name = "category_slug", id = "category_slug", required = "required", maxlength = "255", pattern = "[a-z0-9\\-]+", placeholder = "ootr",
                            value = ctx.field_value("category_slug").unwrap_or_else(|| connection.map_or("", |c| c.category_slug.as_str())));
                        small : "The category portion of the racetime URL, for example ootr.";
                    }
                    fieldset {
                        label(for = "client_id") : "Bot client ID";
                        input(type = "text", name = "client_id", id = "client_id", required = "required", maxlength = "255", autocomplete = "off",
                            value = ctx.field_value("client_id").unwrap_or_else(|| connection.map_or("", |c| c.client_id.as_str())));
                    }
                    fieldset {
                        label(for = "client_secret") : "Bot client secret";
                        input(type = "password", name = "client_secret", id = "client_secret", maxlength = "255", autocomplete = "new-password",
                            required? = !connection.is_some_and(|c| c.has_secret));
                        @if connection.is_some_and(|c| c.has_secret) {
                            small : "A secret is configured. Leave blank to keep it, or enter a replacement.";
                        } else {
                            small : "No secret is configured. Enter the bot client secret.";
                        }
                    }
                }, Vec::new(), "Save Connection");
            }
            p { a(href = uri!(super::get(&game.name, _))) : "Back to Game"; }
        }
    }
}

async fn render(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    game: Game,
    ctx: Context<'_>,
    error: Option<&str>,
    saved: bool,
) -> Result<RawHtml<String>, Error> {
    let current = connections(&mut transaction, game.id).await?;
    let content = content(&game, &current, csrf, &ctx, error, saved);
    Ok(page(
        transaction,
        &Some(me),
        &uri,
        PageStyle::default(),
        "Manage racetime Connection",
        content,
    )
    .await?)
}

#[rocket::get("/games/<game_name>/racetime?<saved>")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    saved: Option<bool>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = authorized_game(&mut transaction, &me, game_name).await?;
    Ok(render(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        game,
        Context::default(),
        None,
        saved == Some(true),
    )
    .await?)
}

#[rocket::post("/games/<game_name>/racetime", data = "<form>")]
pub(crate) async fn post(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, ConnectionForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let me = me.ok_or(StatusOrError::Status(Status::Forbidden))?;
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = authorized_game(&mut transaction, &me, game_name).await?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    // Do not echo parser errors or submitted password values back into the page.
    let error = if form.context.errors().next().is_some() || form.value.is_none() {
        "The form could not be verified. Check the fields and try again."
    } else {
        match save(
            &mut transaction,
            game.id,
            form.value.as_ref().expect("validated form"),
        )
        .await
        .map_err(Error::from)?
        {
            Ok(()) => {
                transaction.commit().await.map_err(Error::from)?;
                return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(
                    game_name,
                    Some(true)
                )))));
            }
            Err(error) => error,
        }
    };
    Ok(RedirectOrContent::Content(
        render(
            transaction,
            me,
            uri,
            csrf.as_ref(),
            game,
            form.context,
            Some(error),
            false,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> ConnectionForm {
        ConnectionForm {
            csrf: String::new(),
            connection_id: None,
            category_slug: "test-category".into(),
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
        }
    }

    #[test]
    fn validates_credentials_and_preserves_blank_secrets_only_when_configured() {
        let mut value = form();
        assert!(validate(&value, false).is_ok());
        value.client_secret = "  ".into();
        assert!(validate(&value, false).is_err());
        assert!(validate(&value, true).is_ok());
        value.client_secret = "s".repeat(256);
        assert!(validate(&value, true).is_err());
        value.client_secret = "s".repeat(255);
        value.client_id = "c".repeat(255);
        value.category_slug = "a".repeat(255);
        assert!(validate(&value, false).is_ok());
        value.client_id = "c".repeat(256);
        assert!(validate(&value, true).is_err());
        value.client_id = " ".into();
        assert!(validate(&value, true).is_err());
        value.client_id = "client".into();
        for slug in [
            "",
            "https://racetime.gg/ootr",
            "ootr/room",
            "UPPER",
            "<script>",
            "two words",
        ] {
            value.category_slug = slug.into();
            assert!(validate(&value, true).is_err(), "{slug}");
        }
    }

    #[test]
    fn rendering_omits_submitted_secret_and_escapes_fields() {
        use kuchiki::traits::TendrilSink as _;

        let game = Game {
            id: 1,
            name: "test".into(),
            display_name: "Test".into(),
            description: None,
            discord_guild: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let parsed = Form::<Contextual<'_, ConnectionForm>>::parse(
            "category_slug=ootr&client_id=<script>&client_secret=never-echo-this",
        )
        .unwrap();
        let current = [Connection {
            id: 123,
            category_slug: "ootr".into(),
            client_id: "client".into(),
            has_secret: true,
        }];
        let html = content(
            &game,
            &current,
            None,
            &parsed.context,
            Some("Try again"),
            false,
        )
        .0;
        assert!(!html.contains("never-echo-this"));
        assert!(!html.contains("<script>"));
        let document = kuchiki::parse_html().one(html.clone());
        let input = document.select_first("#client_id").unwrap();
        assert_eq!(input.attributes.borrow().get("value"), Some("<script>"));
        assert!(html.contains("Leave blank to keep it"));
        assert!(html.contains("type=\"password\""));
    }

    #[test]
    fn missing_csrf_invalidates_an_otherwise_valid_form() {
        let mut form = Form::<Contextual<'_, ConnectionForm>>::parse(
            "category_slug=ootr&client_id=client&client_secret=secret",
        )
        .unwrap();
        assert!(form.value.is_some());
        form.verify(&None);
        assert!(form.context.errors().next().is_some());
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL"]
    async fn database_save_creates_updates_and_rejects_ambiguous_connections() {
        let pool = event::configuration::test_pool().await;
        let mut tx = pool.begin().await.unwrap();
        // Isolate credentials from existing fixtures, whose imported IDs may be
        // ahead of their sequences. Keep the production columns and constraints.
        sqlx::query("CREATE TEMP TABLE game_racetime_connection (LIKE public.game_racetime_connection INCLUDING ALL) ON COMMIT DROP")
            .execute(&mut *tx).await.unwrap();
        let name = format!("racetime-admin-{}", Uuid::new_v4());
        let game_id = (Uuid::new_v4().as_u128() & 0x3fff_ffff) as i32 + 1000;
        sqlx::query(
            "INSERT INTO games (id, name, display_name) VALUES ($1, $2, 'Racetime admin test')",
        )
        .bind(game_id)
        .bind(&name)
        .execute(&mut *tx)
        .await
        .unwrap();
        let user_id = (Uuid::new_v4().as_u128() & 0x3fff_ffff_ffff_ffff) as i64;
        sqlx::query("INSERT INTO users (id, racetime_id, racetime_display_name) VALUES ($1, $2, 'Racetime admin test')")
            .bind(user_id).bind(&name).execute(&mut *tx).await.unwrap();
        let mut user = User::from_id(&mut *tx, Id::from(user_id))
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            authorized_game(&mut tx, &user, &name).await,
            Err(StatusOrError::Status(Status::Forbidden))
        ));
        sqlx::query("INSERT INTO game_admins (game_id, admin_id) VALUES ($1, $2)")
            .bind(game_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        // Per-game administration does not grant access to bot credentials.
        assert!(matches!(
            authorized_game(&mut tx, &user, &name).await,
            Err(StatusOrError::Status(Status::Forbidden))
        ));
        // Global admins have access without a per-game assignment.
        user.id = Id::from(User::GLOBAL_ADMIN_USER_IDS[0]);
        assert!(
            matches!(authorized_game(&mut tx, &user, &name).await, Ok(game) if game.id == game_id)
        );
        let mut value = form();
        value.category_slug = name.clone();
        value.client_secret.clear();
        assert!(save(&mut tx, game_id, &value).await.unwrap().is_err());
        assert!(connections(&mut tx, game_id).await.unwrap().is_empty());
        value.client_secret = "original-secret".into();
        save(&mut tx, game_id, &value).await.unwrap().unwrap();
        // A stale creation form must not overwrite the existing connection.
        assert!(save(&mut tx, game_id, &value).await.unwrap().is_err());
        value.connection_id = Some(connections(&mut tx, game_id).await.unwrap()[0].id);
        value.category_slug = format!("{name}-edited");
        value.client_id = "updated-client".into();
        value.client_secret.clear();
        save(&mut tx, game_id, &value).await.unwrap().unwrap();
        let row: (String, String, String) = sqlx::query_as(
            "SELECT category_slug, client_id, client_secret FROM game_racetime_connection WHERE game_id = $1",
        ).bind(game_id).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(
            row,
            (
                value.category_slug.clone(),
                "updated-client".into(),
                "original-secret".into()
            )
        );
        value.client_secret = "replacement-secret".into();
        save(&mut tx, game_id, &value).await.unwrap().unwrap();
        let secret: String = sqlx::query_scalar(
            "SELECT client_secret FROM game_racetime_connection WHERE game_id = $1",
        )
        .bind(game_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(secret, "replacement-secret");

        let other_id = game_id + 1;
        sqlx::query(
            "INSERT INTO games (id, name, display_name) VALUES ($1, $2, 'Other test game')",
        )
        .bind(other_id)
        .bind(format!("{name}-other"))
        .execute(&mut *tx)
        .await
        .unwrap();
        // Neither a forged connection ID nor a category already in use may be saved.
        assert!(save(&mut tx, other_id, &value).await.unwrap().is_err());
        value.connection_id = None;
        assert!(save(&mut tx, other_id, &value).await.unwrap().is_err());
        assert!(connections(&mut tx, other_id).await.unwrap().is_empty());
        sqlx::query("INSERT INTO game_racetime_connection (game_id, category_slug, client_id, client_secret) VALUES ($1, $2, '', '')")
            .bind(game_id).bind(format!("{name}-duplicate")).execute(&mut *tx).await.unwrap();
        value.connection_id = Some(connections(&mut tx, game_id).await.unwrap()[0].id);
        assert!(save(&mut tx, game_id, &value).await.unwrap().is_err());
        tx.rollback().await.unwrap();
    }
}
