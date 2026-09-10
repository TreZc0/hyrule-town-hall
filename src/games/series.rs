use crate::{game::Game, games::Error, http::StatusOrError, prelude::*, series::Series};

async fn render(
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    game: Game,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, Error> {
    let games = Game::all(&mut transaction).await?;
    let mappings: Vec<(String, Option<i32>)> =
        sqlx::query_as("SELECT series, game_id FROM game_series ORDER BY series")
            .fetch_all(&mut *transaction)
            .await?;
    let content = html! {
        article {
            h1 : format!("Series — {}", game.display_name);
            p : "Assign a predefined series to this game. Moving a series also changes the game used by all its events, including inherited administration, volunteer settings, and racetime connections.";
            h2 : "Connected series";
            ul {
                @for series in all::<Series>().filter(|series| mappings.iter().any(|(slug, id)| slug == series.slug() && *id == Some(game.id))) {
                    li : series.display_name();
                }
            }
            : full_form(uri!(post(&game.name)), csrf, html! {
                : form_field("series", &mut ctx.errors().collect_vec(), html! {
                    label(for = "series") : "Series";
                    select(name = "series", id = "series", required = "required") {
                        option(value = "") : "Choose a series";
                        @for series in all::<Series>() {
                            @let current = mappings.iter().find(|(slug, _)| slug == series.slug()).and_then(|(_, id)| games.iter().find(|game| Some(game.id) == *id));
                            option(value = series.slug(), selected? = ctx.field_value("series") == Some(series.slug())) : format!("{} — {}", series.display_name(), current.map_or("Unassigned", |game| game.display_name.as_str()));
                        }
                    }
                });
            }, ctx.errors().collect_vec(), &format!("Assign to {}", game.display_name));
            p { a(href = uri!(super::get(&game.name, _))) : "Back to game"; }
        }
    };
    Ok(page(
        transaction,
        &Some(me),
        &uri,
        PageStyle::default(),
        "Manage series",
        content,
    )
    .await?)
}

#[rocket::get("/games/<game_name>/series")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await
        .map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(render(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        game,
        Context::default(),
    )
    .await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AssignForm {
    #[field(default = String::new())]
    csrf: String,
    series: String,
}

async fn assign(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    game_id: i32,
) -> sqlx::Result<()> {
    // Serialize assignments, including first assignment of an unmapped series.
    // Existing readers expect one game per series, although the old schema allows more.
    sqlx::query("LOCK TABLE game_series IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut **transaction)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM game_series WHERE series = $1")
        .bind(series.slug())
        .fetch_one(&mut **transaction)
        .await?;
    if count > 1 {
        return Err(sqlx::Error::Protocol(
            "This series has multiple game mappings; resolve them before reassigning it.".into(),
        ));
    }
    if count == 0 {
        sqlx::query("INSERT INTO game_series (series, game_id) VALUES ($1, $2)")
            .bind(series.slug())
            .bind(game_id)
            .execute(&mut **transaction)
            .await?;
    } else {
        sqlx::query("UPDATE game_series SET game_id = $2 WHERE series = $1")
            .bind(series.slug())
            .bind(game_id)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

#[rocket::post("/games/<game_name>/series", data = "<form>")]
pub(crate) async fn post(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    game_name: &str,
    form: Form<Contextual<'_, AssignForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    if !me.is_global_admin() {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut transaction = pool.begin().await.map_err(Error::from)?;
    let game = Game::from_name(&mut transaction, game_name)
        .await
        .map_err(Error::from)?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    let series = form
        .value
        .as_ref()
        .and_then(|value| value.series.parse::<Series>().ok());
    if series.is_none() {
        form.context
            .push_error(form::Error::validation("Choose a predefined series.").with_name("series"));
    }
    if form.context.errors().next().is_none() {
        assign(&mut transaction, series.expect("validated"), game.id)
            .await
            .map_err(Error::from)?;
        transaction.commit().await.map_err(Error::from)?;
        Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(
            game_name
        )))))
    } else {
        Ok(RedirectOrContent::Content(
            render(transaction, me, uri, csrf.as_ref(), game, form.context).await?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL"]
    async fn database_series_assignment_moves_without_duplicates() {
        let pool = event::configuration::test_pool().await;
        let mut tx = pool.begin().await.unwrap();
        let games = Game::all(&mut tx).await.unwrap();
        assert!(games.len() >= 2);
        let series = Series::BotwAny;
        sqlx::query("DELETE FROM game_series WHERE series = $1")
            .bind(series.slug())
            .execute(&mut *tx)
            .await
            .unwrap();

        assign(&mut tx, series, games[0].id).await.unwrap();
        assign(&mut tx, series, games[1].id).await.unwrap();
        assign(&mut tx, series, games[1].id).await.unwrap();
        assert_eq!(
            Game::from_series(&mut tx, series)
                .await
                .unwrap()
                .unwrap()
                .id,
            games[1].id
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM game_series WHERE series = $1")
            .bind(series.slug())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 1);
        tx.rollback().await.unwrap();
    }
}
