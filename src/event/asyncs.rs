use crate::{
    event::{AsyncKind, Data, Series, Tab},
    hash_icon_db::HashIconData,
    prelude::*,
    racetime_bot::{GlobalState, SeedRollUpdate, seed_gen_type::SeedGenType},
    seed,
};

use rocket::response::Flash;

pub(crate) async fn resolve_control(
    pool: &PgPool,
    team: i64,
    value: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let Some(id) = value
        .strip_prefix('a')
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0)
    else {
        return Ok(None);
    };
    sqlx::query_scalar("SELECT a.id FROM asyncs a JOIN teams t ON t.series=a.series AND t.event=a.event WHERE t.id=$1 AND a.id=$2")
        .bind(team).bind(id).fetch_optional(pool).await
}

pub(crate) async fn next(
    transaction: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    team: Option<Id<Teams>>,
) -> Result<Option<i64>, event::DataError> {
    // Always keep an unfinished request visible, including after its window
    // closes and while its result is awaiting submission or verification.
    if let Some(team) = team {
        if let Some(id) = sqlx::query_scalar("SELECT a.id FROM async_teams entry JOIN asyncs a ON a.id=entry.async_id WHERE entry.team=$1 AND a.series=$2 AND a.event=$3 AND entry.requested IS NOT NULL AND entry.submitted IS NULL ORDER BY entry.requested, a.id LIMIT 1")
            .bind(team).bind(data.series).bind(&*data.event).fetch_optional(&mut **transaction).await? {
            return Ok(Some(id));
        }
    }
    let started = data.is_started(transaction).await?;
    Ok(sqlx::query_scalar("SELECT a.id FROM asyncs a WHERE a.series=$1 AND a.event=$2 AND (a.start IS NULL OR a.start<=NOW()) AND (a.end_time IS NULL OR a.end_time>NOW()) AND (a.seed_data IS NOT NULL OR a.file_stem IS NOT NULL OR a.web_id IS NOT NULL OR a.tfb_uuid IS NOT NULL OR a.xkeys_uuid IS NOT NULL) AND (a.kind='seeding' OR (a.kind='qualifier' AND NOT $4) OR (a.kind IN ('tiebreaker1','tiebreaker2') AND EXISTS (SELECT 1 FROM async_teams WHERE team=$3 AND async_id=a.id))) AND NOT EXISTS (SELECT 1 FROM async_teams WHERE team=$3 AND async_id=a.id AND submitted IS NOT NULL) ORDER BY a.id LIMIT 1")
        .bind(data.series).bind(&*data.event).bind(team).bind(started).fetch_optional(&mut **transaction).await?)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RequestError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("This async does not belong to your event.")]
    WrongEvent,
    #[error("Finish your current async before requesting another.")]
    Unfinished,
}

/// Request an async within the caller's transaction. Availability and signup
/// requirements are checked by the caller. Returns false for an existing
/// request, allowing a returning entrant to retain their original run.
pub(crate) async fn request(
    tx: &mut Transaction<'_, Postgres>,
    team: Id<Teams>,
    async_id: i64,
) -> Result<bool, RequestError> {
    // Serialize requests for this team, including requests from signup.
    let (series, event): (String, String) =
        sqlx::query_as("SELECT series,event FROM teams WHERE id=$1 FOR UPDATE")
            .bind(team)
            .fetch_one(&mut **tx)
            .await?;
    // Seed edits and generation take this same definition lock. Check ownership
    // here because async_teams references the team and async independently.
    let definition: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM asyncs WHERE id=$1 AND series=$2 AND event=$3 FOR UPDATE",
    )
    .bind(async_id)
    .bind(series)
    .bind(event)
    .fetch_optional(&mut **tx)
    .await?;
    definition.ok_or(RequestError::WrongEvent)?;
    let already_requested: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM async_teams WHERE team=$1 AND async_id=$2 AND (requested IS NOT NULL OR submitted IS NOT NULL))",
    ).bind(team).bind(async_id).fetch_one(&mut **tx).await?;
    if already_requested {
        return Ok(false);
    }
    let unfinished: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM async_teams WHERE team=$1 AND requested IS NOT NULL AND submitted IS NULL)",
    ).bind(team).fetch_one(&mut **tx).await?;
    if unfinished {
        return Err(RequestError::Unfinished);
    }
    sqlx::query("INSERT INTO async_teams(team,async_id,requested) VALUES($1,$2,NOW()) ON CONFLICT(team,async_id) DO UPDATE SET requested=EXCLUDED.requested")
        .bind(team).bind(async_id).execute(&mut **tx).await?;
    Ok(true)
}

#[derive(Clone, Copy)]
enum AsyncSeedFormKind {
    Mmr,
    Twwr,
    TriforceBlitz,
    AlttprDoorRando,
    AlttprAvianart,
    FileStemWebId,
}

fn async_seed_form_kind(event: &Data<'_>) -> AsyncSeedFormKind {
    match event.seed_gen_type.as_ref() {
        Some(SeedGenType::Mmr { .. }) => AsyncSeedFormKind::Mmr,
        Some(SeedGenType::TWWR { .. }) => AsyncSeedFormKind::Twwr,
        Some(SeedGenType::OotrTriforceBlitz) => AsyncSeedFormKind::TriforceBlitz,
        Some(SeedGenType::Owr { .. } | SeedGenType::AlttprDoorRando { .. }) => {
            AsyncSeedFormKind::AlttprDoorRando
        }
        Some(SeedGenType::AlttprAvianart { .. }) => AsyncSeedFormKind::AlttprAvianart,
        _ => match event.series {
            Series::TwwrMain => AsyncSeedFormKind::Twwr,
            Series::TriforceBlitz => AsyncSeedFormKind::TriforceBlitz,
            Series::AlttprDe => AsyncSeedFormKind::AlttprAvianart,
            _ => AsyncSeedFormKind::FileStemWebId,
        },
    }
}

async fn resolve_edit(transaction: &mut Transaction<'_, Postgres>, series: Series, event: &str, value: &str) -> Result<Option<i64>, sqlx::Error> {
    if let Ok(id) = value.parse::<i64>() {
        return sqlx::query_scalar("SELECT id FROM asyncs WHERE series=$1 AND event=$2 AND id=$3")
            .bind(series).bind(event).bind(id).fetch_optional(&mut **transaction).await;
    }
    Ok(None)
}

fn avianart_seed_fields(seed_data: &serde_json::Value) -> (String, String) {
    let parsed = seed::Files::from_seed_data(seed_data);
    let hash = match parsed.as_ref() {
        Some(seed::Files::AvianartSeed { hash, .. }) => Some(hash.clone()),
        _ => None,
    }
    .or_else(|| {
        seed_data
            .get("avianart_hash")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    })
    .unwrap_or_default();
    let seed_hash = match parsed.as_ref() {
        Some(seed::Files::AvianartSeed {
            seed_hash: Some(seed_hash),
            ..
        }) => Some(seed_hash.join(", ")),
        _ => None,
    }
    .or_else(|| {
        seed_data
            .get("avianart_seed_hash")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    })
    .unwrap_or_default();
    (hash, seed_hash)
}

async fn asyncs_form(
    mut transaction: Transaction<'_, Postgres>, me: User, uri: Origin<'_>,
    csrf: Option<&CsrfToken>, event: Data<'_>, edit_id: Option<i64>, ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, Some(&me), Tab::Asyncs, true).await?;
    let content = editor(&mut transaction, csrf, &event, edit_id, ctx).await?;
    Ok(page(transaction, &Some(me), &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() },
        &format!("Qualifiers & Asyncs — {}", event.display_name), html! { : header; article { : content; } }).await?)
}

pub(super) async fn editor(
    mut transaction: &mut Transaction<'_, Postgres>, csrf: Option<&CsrfToken>,
    event: &Data<'_>, edit_id: Option<i64>, ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    struct AsyncRow {
        id: i64,
        name: String,
        requested: bool,
        kind: AsyncKind,
        file_stem: Option<String>,
        web_id: Option<i64>,
        tfb_uuid: Option<Uuid>,
        xkeys_uuid: Option<Uuid>,
        seed_data: Option<serde_json::Value>,
        hash1: Option<String>,
        hash2: Option<String>,
        hash3: Option<String>,
        hash4: Option<String>,
        hash5: Option<String>,
        start: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    }

    let asyncs = sqlx::query_as!(
        AsyncRow,
        r#"SELECT id, name, EXISTS(SELECT 1 FROM async_teams WHERE async_id=asyncs.id AND requested IS NOT NULL) AS "requested!", kind AS "kind: AsyncKind", file_stem, web_id, tfb_uuid, xkeys_uuid, seed_data,
               hash1::text as hash1, hash2::text as hash2, hash3::text as hash3, hash4::text as hash4, hash5::text as hash5,
               start, end_time FROM asyncs WHERE series = $1 AND event = $2 ORDER BY id"#,
        event.series as _,
        &event.event
    )
    .fetch_all(&mut **transaction)
    .await?;

    let editing_async = edit_id.and_then(|id| asyncs.iter().find(|row| row.id == id));
    let default_kind = editing_async
        .map(|row| format!("{:?}", row.kind))
        .unwrap_or_else(|| format!("{:?}", AsyncKind::Qualifier));
    let default_name = editing_async.map(|row| row.name.clone()).unwrap_or_else(|| format!("Async {}", asyncs.iter().filter(|row| row.kind == AsyncKind::Qualifier).count() + 1));
    let default_file_stem = editing_async
        .and_then(|row| row.file_stem.as_ref())
        .cloned()
        .unwrap_or_default();
    let default_web_id = editing_async
        .and_then(|row| row.web_id)
        .map(|id| id.to_string())
        .unwrap_or_default();
    let default_tfb_uuid = editing_async
        .and_then(|row| row.tfb_uuid)
        .map(|uuid| uuid.to_string())
        .unwrap_or_default();
    let default_xkeys_uuid = editing_async
        .and_then(|row| {
            row.seed_data
                .as_ref()
                .and_then(seed::Files::from_seed_data)
                .and_then(|files| match files {
                    seed::Files::AlttprDoorRando { uuid, .. } => Some(uuid.to_string()),
                    _ => None,
                })
                .or_else(|| row.xkeys_uuid.as_ref().map(ToString::to_string))
        })
        .unwrap_or_default();
    let default_permalink = editing_async
        .and_then(|row| row.seed_data.as_ref())
        .and_then(|seed_data| seed_data.get("permalink"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let default_seed_hash = editing_async
        .and_then(|row| row.seed_data.as_ref())
        .and_then(|seed_data| seed_data.get("seed_hash"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let (default_avianart_hash, default_avianart_seed_hash) = editing_async
        .and_then(|row| row.seed_data.as_ref())
        .map(avianart_seed_fields)
        .unwrap_or_default();
    let default_hash: [String; 5] = std::array::from_fn(|i| {
        editing_async
            .and_then(|row| match i {
                0 => row.hash1.as_deref(),
                1 => row.hash2.as_deref(),
                2 => row.hash3.as_deref(),
                3 => row.hash4.as_deref(),
                _ => row.hash5.as_deref(),
            })
            .unwrap_or_default()
            .to_owned()
    });
    let default_start = editing_async
        .and_then(|row| row.start)
        .map(|start| start.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default();
    let default_end_time = editing_async
        .and_then(|row| row.end_time)
        .map(|end_time| end_time.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default();

    let seed_form_kind = async_seed_form_kind(&event);
    let hash_icons = if matches!(seed_form_kind, AsyncSeedFormKind::AlttprDoorRando) {
        if let Some(game) = event.game(&mut transaction).await? {
            HashIconData::all_for_game(&mut transaction, game.id).await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    Ok(html! {
                section(id = "async-qualifiers") {
                    h2 : "Async qualifiers & other asyncs";
                    p : "Qualifiers are offered in creation order during their availability windows. Each qualifier has one shared seed.";
                    @if asyncs.is_empty() {
                        p : "No asyncs defined.";
                    } else {
                        table {
                            thead {
                                tr {
                                    th : "Name"; th : "Kind"; th : "Seed status";
                                    @match seed_form_kind {
                                        AsyncSeedFormKind::Mmr => { th : "MMR seed"; }
                                        AsyncSeedFormKind::Twwr => {
                                            th(colspan = "2") : "Seed";
                                        }
                                        AsyncSeedFormKind::TriforceBlitz => {
                                            th : "TFB UUID";
                                        }
                                        AsyncSeedFormKind::AlttprDoorRando => {
                                            th : "ALTTPR UUID";
                                        }
                                        AsyncSeedFormKind::AlttprAvianart => {
                                            th : "Avianart Seed";
                                        }
                                        AsyncSeedFormKind::FileStemWebId => {
                                            th : "File Stem";
                                            th : "Web ID";
                                        }
                                    }
                                    th : "Start";
                                    th : "End";
                                    th : "Actions";
                                }
                            }
                            tbody {
                                @for row in asyncs {
                                    @let has_seed = row.seed_data.is_some() || row.file_stem.is_some() || row.web_id.is_some() || row.tfb_uuid.is_some() || row.xkeys_uuid.is_some();
                                    tr {
                                        td : &row.name;
                                        td : format!("{:?}", row.kind);
                                        td {
                                            : if has_seed { "Ready" } else { "No seed" };
                                            @if row.requested { br; : "Seed locked: already requested"; }
                                        }
                                        @match seed_form_kind {
                                            AsyncSeedFormKind::Mmr => {
                                                td {
                                                    @if let Some(seed::Files::MmrWeb { id, hash }) = row.seed_data.as_ref().and_then(seed::Files::from_seed_data) {
                                                        a(href = format!("https://mmrandomizer.com/seed/get?id={id}"), target = "_blank") : "View seed";
                                                        p : hash.join(", ");
                                                    } else { : "Not generated"; }
                                                }
                                            }
                                            AsyncSeedFormKind::Twwr => {
                                                td(colspan = "2") {
                                                    @let permalink = row.seed_data.as_ref().and_then(|d| d.get("permalink")).and_then(|v| v.as_str()).unwrap_or("");
                                                    @let seed_hash = row.seed_data.as_ref().and_then(|d| d.get("seed_hash")).and_then(|v| v.as_str()).unwrap_or("");
                                                    @if !permalink.is_empty() || !seed_hash.is_empty() {
                                                        span(class = "settings-link twwr-seed-link") {
                                                            : "Hover for Seed";
                                                            span(class = "tooltip-content") {
                                                                @if !permalink.is_empty() {
                                                                    div {
                                                                        strong : "Permalink: ";
                                                                        code(style = "user-select: all") : permalink;
                                                                    }
                                                                }
                                                                @if !seed_hash.is_empty() {
                                                                    div {
                                                                        strong : "Seed Hash: ";
                                                                        : seed_hash;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            AsyncSeedFormKind::TriforceBlitz => {
                                                td {
                                                    @if let Some(seed::Files::TriforceBlitz { uuid, .. }) = row.seed_data.as_ref().and_then(seed::Files::from_seed_data) {
                                                        a(href = format!("https://tfb.midos.house/seed/{uuid}")) : "View seed";
                                                    } else { : row.tfb_uuid.map(|u| u.to_string()).unwrap_or_default(); }
                                                }
                                            }
                                            AsyncSeedFormKind::AlttprDoorRando => {
                                                td {
                                                    @let uuid = row.seed_data.as_ref()
                                                        .and_then(seed::Files::from_seed_data)
                                                        .and_then(|files| match files {
                                                            seed::Files::AlttprDoorRando { uuid, .. } => Some(uuid.to_string()),
                                                            _ => None,
                                                        })
                                                        .or_else(|| row.xkeys_uuid.map(|u| u.to_string()))
                                                        .unwrap_or_default();
                                                    : uuid;
                                                    @if let (Some(hash1), Some(hash2), Some(hash3), Some(hash4), Some(hash5)) = (
                                                        row.hash1.as_deref(),
                                                        row.hash2.as_deref(),
                                                        row.hash3.as_deref(),
                                                        row.hash4.as_deref(),
                                                        row.hash5.as_deref(),
                                                    ) {
                                                        br;
                                                        span(class = "hash-text") {
                                                            : format!("Hash: {hash1}, {hash2}, {hash3}, {hash4}, {hash5}");
                                                        }
                                                    }
                                                }
                                            }
                                            AsyncSeedFormKind::AlttprAvianart => {
                                                td {
                                                    @let (avianart_hash, avianart_seed_hash) = row.seed_data.as_ref().map(avianart_seed_fields).unwrap_or_default();
                                                    @if !avianart_hash.is_empty() {
                                                        a(href = format!("https://avianart.games/perm/{}", avianart_hash), target = "_blank") : avianart_hash;
                                                    }
                                                    @if !avianart_seed_hash.is_empty() {
                                                        br;
                                                        : format!("Hash: {}", avianart_seed_hash);
                                                    }
                                                }
                                            }
                                            AsyncSeedFormKind::FileStemWebId => {
                                                td(colspan = "2") {
                                                    @match row.seed_data.as_ref().and_then(seed::Files::from_seed_data) {
                                                        Some(seed::Files::OotrWeb { id, .. }) => a(href = format!("https://ootrandomizer.com/seed/get?id={id}")) : "View seed";
                                                        Some(seed::Files::MidosHouse { file_stem, .. }) => a(href = format!("{}/seed/{file_stem}.zpfz", base_uri())) : "Download seed";
                                                        _ => { : row.file_stem.as_deref().unwrap_or_default(); : " "; : row.web_id.map(|id| id.to_string()).unwrap_or_default(); }
                                                    }
                                                }
                                            }
                                        }
                                        td : row.start.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default();
                                        td : row.end_time.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default();
                                        td {
                                            @let kind_name = row.id.to_string();
                                            a(class = "button", href = uri!(get(event.series, &*event.event, Some(kind_name.clone())))) : "Edit";
                                            @if event.seed_gen_type.is_some() && !row.requested {
                                                form(action = uri!(generate(event.series, &*event.event, row.id)).to_string(), method = "post", style = "display: inline;") {
                                                    input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                                                    button(type = "submit") : if has_seed { "Regenerate seed" } else { "Generate seed" };
                                                }
                                            }
                                            : " | ";
                                            form(action = uri!(delete(event.series, &*event.event, kind_name)).to_string(), method = "post", style = "display: inline;") {
                                                input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                                                button(type = "submit", disabled? = row.requested, onclick = "return confirm('Are you sure you want to delete this async?')") : "Delete";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    @if edit_id.is_some() {
                        p {
                            : "Editing ";
                            b : &default_name;
                            : ". ";
                            a(href = uri!(get(event.series, &*event.event, None::<String>)).to_string()) : "Cancel edit";
                        }
                    }
                    h3 : if edit_id.is_some() { "Edit Async" } else { "Add async" };
                    @let hidden_fields = match seed_form_kind {
                        AsyncSeedFormKind::Mmr => ["file_stem", "web_id", "tfb_uuid", "xkeys_uuid", "avianart_hash", "avianart_seed_hash", "hash1", "hash2", "hash3", "hash4", "hash5", "permalink", "seed_hash"].as_slice(),
                        AsyncSeedFormKind::Twwr => ["file_stem", "web_id", "tfb_uuid", "xkeys_uuid", "avianart_hash", "avianart_seed_hash", "hash1", "hash2", "hash3", "hash4", "hash5"].as_slice(),
                        AsyncSeedFormKind::TriforceBlitz => ["file_stem", "web_id", "permalink", "seed_hash", "xkeys_uuid", "avianart_hash", "avianart_seed_hash", "hash1", "hash2", "hash3", "hash4", "hash5"].as_slice(),
                        AsyncSeedFormKind::AlttprDoorRando => ["file_stem", "web_id", "permalink", "seed_hash", "tfb_uuid", "avianart_hash", "avianart_seed_hash"].as_slice(),
                        AsyncSeedFormKind::AlttprAvianart => ["file_stem", "web_id", "permalink", "seed_hash", "tfb_uuid", "xkeys_uuid", "hash1", "hash2", "hash3", "hash4", "hash5"].as_slice(),
                        AsyncSeedFormKind::FileStemWebId => ["permalink", "seed_hash", "tfb_uuid", "xkeys_uuid", "avianart_hash", "avianart_seed_hash", "hash1", "hash2", "hash3", "hash4", "hash5"].as_slice(),
                    };
                    @let mut errors = ctx.errors().filter(|e| !hidden_fields.iter().any(|f| e.is_for(f))).collect_vec();
                    : full_form(uri!(post(event.series, &*event.event)), csrf, html! {
                        @let selected_kind = ctx.field_value("kind").unwrap_or(&default_kind);
                        @if let Some(id) = edit_id { input(type = "hidden", name = "id", value = id); }
                        : form_field("name", &mut errors, html! {
                            label(for = "async_name") : "Name";
                            input(type = "text", id = "async_name", name = "name", required, maxlength = "100", value = ctx.field_value("name").unwrap_or(&default_name));
                        });
                        // Hidden inputs for fields not used by this series
                        @for field in hidden_fields {
                            input(type = "hidden", name = *field, value = "");
                        }
                        : form_field("kind", &mut errors, html! {
                            label(for = "kind") : "Kind";
                            select(name = "kind", id = "kind") {
                                @for kind in all::<AsyncKind>() {
                                    @let kind_name = format!("{:?}", kind);
                                    option(value = &kind_name, selected? = selected_kind == kind_name.as_str()) : kind_name;
                                }
                            }
                        });
                        p : "Save this async, then use Generate seed to roll with the event settings. All players receive the same seed. Manual seed details below are optional.";
                        @if edit_id.is_some() {
                            label { input(type = "checkbox", name = "replace_seed", checked? = ctx.field_value("replace_seed") == Some("on")); : "Replace the seed using the manual details below (only before anyone requests it)"; }
                        }
                        @match seed_form_kind {
                            AsyncSeedFormKind::Mmr => {}
                            AsyncSeedFormKind::Twwr => {
                                : form_field("permalink", &mut errors, html! {
                                    label(for = "permalink") : "Permalink";
                                    input(type = "text", name = "permalink", id = "permalink", value = ctx.field_value("permalink").unwrap_or(&default_permalink), style = "width: 100%; max-width: 600px;");
                                });
                                : form_field("seed_hash", &mut errors, html! {
                                    label(for = "seed_hash") : "Seed Hash";
                                    input(type = "text", name = "seed_hash", id = "seed_hash", value = ctx.field_value("seed_hash").unwrap_or(&default_seed_hash));
                                });
                            }
                            AsyncSeedFormKind::TriforceBlitz => {
                                : form_field("tfb_uuid", &mut errors, html! {
                                    label(for = "tfb_uuid") : "TFB UUID";
                                    input(type = "text", name = "tfb_uuid", id = "tfb_uuid", value = ctx.field_value("tfb_uuid").unwrap_or(&default_tfb_uuid));
                                });
                            }
                            AsyncSeedFormKind::AlttprDoorRando => {
                                : form_field("xkeys_uuid", &mut errors, html! {
                                    label(for = "xkeys_uuid") : "ALTTPR UUID";
                                    input(type = "text", name = "xkeys_uuid", id = "xkeys_uuid", value = ctx.field_value("xkeys_uuid").unwrap_or(&default_xkeys_uuid));
                                });
                                p : "Seed Hash (optional — select all 5 icons or leave all blank):";
                                @for (field_name, label, default_val) in [
                                    ("hash1", "Icon 1", &default_hash[0]),
                                    ("hash2", "Icon 2", &default_hash[1]),
                                    ("hash3", "Icon 3", &default_hash[2]),
                                    ("hash4", "Icon 4", &default_hash[3]),
                                    ("hash5", "Icon 5", &default_hash[4]),
                                ] {
                                    : form_field(field_name, &mut errors, html! {
                                        label(for = field_name) : label;
                                        select(name = field_name, id = field_name) {
                                            option(value = "", selected? = ctx.field_value(field_name).unwrap_or(default_val).is_empty()) : "—";
                                            @for hash_icon_data in &hash_icons {
                                                @let icon_name = hash_icon_data.name.as_str();
                                                @let selected = ctx.field_value(field_name).unwrap_or(default_val) == icon_name;
                                                option(value = icon_name, selected? = selected) : icon_name;
                                            }
                                        }
                                    });
                                }
                            }
                            AsyncSeedFormKind::AlttprAvianart => {
                                : form_field("avianart_hash", &mut errors, html! {
                                    label(for = "avianart_hash") : "Avianart Hash";
                                    input(type = "text", name = "avianart_hash", id = "avianart_hash", value = ctx.field_value("avianart_hash").unwrap_or(&default_avianart_hash), style = "width: 100%; max-width: 600px;");
                                    label(class = "help") : " (The hash from avianart.games/perm/{hash})";
                                });
                                : form_field("avianart_seed_hash", &mut errors, html! {
                                    label(for = "avianart_seed_hash") : "Seed Hash (optional)";
                                    input(type = "text", name = "avianart_seed_hash", id = "avianart_seed_hash", value = ctx.field_value("avianart_seed_hash").unwrap_or(&default_avianart_seed_hash), style = "width: 100%; max-width: 600px;");
                                    label(class = "help") : " (e.g. \"Bug Net, Bow, Lamp, Flippers, Boomerang\")";
                                });
                            }
                            AsyncSeedFormKind::FileStemWebId => {
                                : form_field("file_stem", &mut errors, html! {
                                    label(for = "file_stem") : "File Stem";
                                    input(type = "text", name = "file_stem", id = "file_stem", value = ctx.field_value("file_stem").unwrap_or(&default_file_stem));
                                });
                                : form_field("web_id", &mut errors, html! {
                                    label(for = "web_id") : "Web ID (optional)";
                                    input(type = "number", name = "web_id", id = "web_id", value = ctx.field_value("web_id").unwrap_or(&default_web_id));
                                });
                            }
                        }
                        : form_field("start", &mut errors, html! {
                            label(for = "start") : "Start Time (UTC)";
                            input(type = "datetime-local", name = "start", id = "start", value = ctx.field_value("start").unwrap_or(&default_start));
                        });
                        : form_field("end_time", &mut errors, html! {
                            label(for = "end_time") : "End Time (UTC)";
                            input(type = "datetime-local", name = "end_time", id = "end_time", value = ctx.field_value("end_time").unwrap_or(&default_end_time));
                        });
                    }, errors, if edit_id.is_some() { "Update Async" } else { "Save Async" });
                }
    })
}

#[rocket::get("/event/<series>/<event>/asyncs?<edit_kind>")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: String,
    edit_kind: Option<String>,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, &event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !event_data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let edit_kind = if let Some(value) = edit_kind { Some(resolve_edit(&mut transaction, series, &event, &value).await?.ok_or(StatusOrError::Status(Status::NotFound))?) } else { None };
    Ok(asyncs_form(
        transaction,
        me,
        uri,
        csrf.as_ref(),
        event_data,
        edit_kind,
        Context::default(),
    )
    .await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AsyncForm {
    #[field(default = String::new())]
    csrf: String,
    kind: AsyncKind,
    id: Option<i64>,
    #[field(validate = len(1..=100))]
    name: String,
    replace_seed: bool,
    file_stem: Option<String>,
    web_id: Option<i64>,
    tfb_uuid: Option<String>,
    xkeys_uuid: Option<String>,
    permalink: Option<String>,
    seed_hash: Option<String>,
    avianart_hash: Option<String>,
    avianart_seed_hash: Option<String>,
    hash1: Option<String>,
    hash2: Option<String>,
    hash3: Option<String>,
    hash4: Option<String>,
    hash5: Option<String>,
    start: Option<String>,
    end_time: Option<String>,
}

#[rocket::post("/event/<series>/<event>/asyncs", data = "<form>")]
pub(crate) async fn post(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, AsyncForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if !event_data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    Ok(if let Some(ref value) = form.value {
        let seed_form_kind = async_seed_form_kind(&event_data);
        let hidden_fields = match seed_form_kind {
            AsyncSeedFormKind::Mmr => ["file_stem", "web_id", "tfb_uuid", "xkeys_uuid", "avianart_hash", "avianart_seed_hash", "hash1", "hash2", "hash3", "hash4", "hash5", "permalink", "seed_hash"].as_slice(),
            AsyncSeedFormKind::Twwr => [
                "file_stem",
                "web_id",
                "tfb_uuid",
                "xkeys_uuid",
                "avianart_hash",
                "avianart_seed_hash",
                "hash1",
                "hash2",
                "hash3",
                "hash4",
                "hash5",
            ]
            .as_slice(),
            AsyncSeedFormKind::TriforceBlitz => [
                "file_stem",
                "web_id",
                "permalink",
                "seed_hash",
                "xkeys_uuid",
                "avianart_hash",
                "avianart_seed_hash",
                "hash1",
                "hash2",
                "hash3",
                "hash4",
                "hash5",
            ]
            .as_slice(),
            AsyncSeedFormKind::AlttprDoorRando => [
                "file_stem",
                "web_id",
                "permalink",
                "seed_hash",
                "tfb_uuid",
                "avianart_hash",
                "avianart_seed_hash",
            ]
            .as_slice(),
            AsyncSeedFormKind::AlttprAvianart => [
                "file_stem",
                "web_id",
                "permalink",
                "seed_hash",
                "tfb_uuid",
                "xkeys_uuid",
                "hash1",
                "hash2",
                "hash3",
                "hash4",
                "hash5",
            ]
            .as_slice(),
            AsyncSeedFormKind::FileStemWebId => [
                "permalink",
                "seed_hash",
                "tfb_uuid",
                "xkeys_uuid",
                "avianart_hash",
                "avianart_seed_hash",
                "hash1",
                "hash2",
                "hash3",
                "hash4",
                "hash5",
            ]
            .as_slice(),
        };
        let has_relevant_errors = form
            .context
            .errors()
            .any(|e| !hidden_fields.iter().any(|f| e.is_for(f)));
        if has_relevant_errors {
            RedirectOrContent::Content(
                asyncs_form(
                    transaction,
                    me,
                    uri,
                    csrf.as_ref(),
                    event_data,
                    value.id,
                    form.context,
                )
                .await?,
            )
        } else {
            // Parse UUIDs for TFB/ALTTPR.
            let tfb_uuid = if let Some(ref s) = value.tfb_uuid {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Uuid>() {
                        Ok(uuid) => Some(uuid),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid UUID").with_name("tfb_uuid"),
                            );
                            return Ok(RedirectOrContent::Content(
                                asyncs_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    value.id,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let xkeys_uuid_parsed = if let Some(ref s) = value.xkeys_uuid {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    match trimmed.parse::<Uuid>() {
                        Ok(uuid) => Some(uuid),
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid UUID").with_name("xkeys_uuid"),
                            );
                            return Ok(RedirectOrContent::Content(
                                asyncs_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    value.id,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let alttpr_hash = if matches!(seed_form_kind, AsyncSeedFormKind::AlttprDoorRando) {
                let h = [
                    value.hash1.as_deref().unwrap_or("").trim(),
                    value.hash2.as_deref().unwrap_or("").trim(),
                    value.hash3.as_deref().unwrap_or("").trim(),
                    value.hash4.as_deref().unwrap_or("").trim(),
                    value.hash5.as_deref().unwrap_or("").trim(),
                ];
                let filled = h.iter().filter(|s| !s.is_empty()).count();
                if filled == 5 {
                    Some(h.map(str::to_owned))
                } else if filled > 0 {
                    form.context.push_error(
                        form::Error::validation(
                            "All 5 hash icons must be selected, or leave all blank",
                        )
                        .with_name("hash1"),
                    );
                    return Ok(RedirectOrContent::Content(
                        asyncs_form(
                            transaction,
                            me,
                            uri,
                            csrf.as_ref(),
                            event_data,
                            value.id,
                            form.context,
                        )
                        .await?,
                    ));
                } else {
                    None
                }
            } else {
                None
            };

            // Build seed_data JSON.
            let (seed_data, xkeys_uuid) = match seed_form_kind {
                AsyncSeedFormKind::Mmr => (None, None), // Preserved atomically when saving below.
                AsyncSeedFormKind::Twwr => {
                    let permalink = value.permalink.as_deref().unwrap_or("").trim();
                    let seed_hash = value.seed_hash.as_deref().unwrap_or("").trim();
                    let seed_data = if !permalink.is_empty() || !seed_hash.is_empty() {
                        Some(serde_json::json!({
                            "type": "twwr",
                            "permalink": permalink,
                            "seed_hash": seed_hash,
                        }))
                    } else {
                        None
                    };
                    (seed_data, None)
                }
                AsyncSeedFormKind::AlttprAvianart => {
                    let avianart_hash = value.avianart_hash.as_deref().unwrap_or("").trim();
                    let avianart_seed_hash =
                        value.avianart_seed_hash.as_deref().unwrap_or("").trim();
                    let seed_hash = if avianart_seed_hash.is_empty() {
                        None
                    } else {
                        match crate::avianart::parse_file_hash(avianart_seed_hash) {
                            Ok(seed_hash) => Some(seed_hash),
                            Err(_) => {
                                form.context.push_error(
                                    form::Error::validation(
                                        "Expected 5 hash icons separated by comma and space",
                                    )
                                    .with_name("avianart_seed_hash"),
                                );
                                return Ok(RedirectOrContent::Content(
                                    asyncs_form(
                                        transaction,
                                        me,
                                        uri,
                                        csrf.as_ref(),
                                        event_data,
                                        value.id,
                                        form.context,
                                    )
                                    .await?,
                                ));
                            }
                        }
                    };
                    let seed_data = if !avianart_hash.is_empty() {
                        Some(
                            seed::Files::AvianartSeed {
                                hash: avianart_hash.to_owned(),
                                seed_hash,
                            }
                            .to_seed_data_base(),
                        )
                    } else {
                        None
                    };
                    (seed_data, None)
                }
                AsyncSeedFormKind::AlttprDoorRando => {
                    let is_owr = matches!(
                        event_data.seed_gen_type.as_ref(),
                        Some(SeedGenType::Owr { .. })
                    );
                    (
                        xkeys_uuid_parsed.map(|uuid| {
                            seed::Files::AlttprDoorRando { uuid, is_owr }.to_seed_data_base()
                        }),
                        None,
                    )
                }
                AsyncSeedFormKind::TriforceBlitz | AsyncSeedFormKind::FileStemWebId => {
                    (None, xkeys_uuid_parsed)
                }
            };

            let file_stem = value
                .file_stem
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);

            // Parse start/end times
            let start = if let Some(ref start_str) = value.start {
                if !start_str.is_empty() {
                    match NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => {
                            Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc))
                        }
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid start time format")
                                    .with_name("start"),
                            );
                            return Ok(RedirectOrContent::Content(
                                asyncs_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    value.id,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let end_time = if let Some(ref end_str) = value.end_time {
                if !end_str.is_empty() {
                    match NaiveDateTime::parse_from_str(end_str, "%Y-%m-%dT%H:%M") {
                        Ok(naive_dt) => {
                            Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc))
                        }
                        Err(_) => {
                            form.context.push_error(
                                form::Error::validation("Invalid end time format")
                                    .with_name("end_time"),
                            );
                            return Ok(RedirectOrContent::Content(
                                asyncs_form(
                                    transaction,
                                    me,
                                    uri,
                                    csrf.as_ref(),
                                    event_data,
                                    value.id,
                                    form.context,
                                )
                                .await?,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let (hash1, hash2, hash3, hash4, hash5): (
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ) = match alttpr_hash {
                Some([h1, h2, h3, h4, h5]) => (Some(h1), Some(h2), Some(h3), Some(h4), Some(h5)),
                None => (None, None, None, None, None),
            };
            if value.name.trim().is_empty() || start.zip(end_time).is_some_and(|(start, end)| start >= end) {
                form.context.push_error(form::Error::validation("Enter a name and an end time after the start."));
                return Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, value.id, form.context).await?));
            }
            let id = if let Some(id) = value.id {
                let existing: Option<AsyncKind> = sqlx::query_scalar("SELECT kind FROM asyncs WHERE id=$1 AND series=$2 AND event=$3 FOR UPDATE")
                    .bind(id).bind(series).bind(event).fetch_optional(&mut *transaction).await?;
                let kind = existing.ok_or(StatusOrError::Status(Status::NotFound))?;
                let requested: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM async_teams WHERE async_id=$1 AND requested IS NOT NULL)")
                    .bind(id).fetch_one(&mut *transaction).await?;
                if requested && (value.replace_seed || kind != value.kind) {
                    form.context.push_error(form::Error::validation("This seed has already been requested. Its name and dates can still be edited."));
                    return Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), event_data, value.id, form.context).await?));
                }
                sqlx::query("UPDATE asyncs SET kind=$2,name=$3,start=$4,end_time=$5 WHERE id=$1")
                    .bind(id).bind(value.kind).bind(value.name.trim()).bind(start).bind(end_time).execute(&mut *transaction).await?;
                id
            } else {
                sqlx::query_scalar("INSERT INTO asyncs(series,event,kind,name,start,end_time) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
                    .bind(series).bind(event).bind(value.kind).bind(value.name.trim()).bind(start).bind(end_time).fetch_one(&mut *transaction).await?
            };
            if value.id.is_none() || value.replace_seed {
                sqlx::query("UPDATE asyncs SET file_stem=$2,web_id=$3,tfb_uuid=$4,xkeys_uuid=$5,seed_data=$6,hash1=$7,hash2=$8,hash3=$9,hash4=$10,hash5=$11,seed_password=NULL,web_gen_time=NULL WHERE id=$1")
                    .bind(id).bind(file_stem).bind(value.web_id).bind(tfb_uuid).bind(xkeys_uuid).bind(seed_data)
                    .bind(hash1).bind(hash2).bind(hash3).bind(hash4).bind(hash5).execute(&mut *transaction).await?;
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(format!("{}#async-qualifiers", uri!(super::qualifiers::get(series, event)))))
        }
    } else {
        RedirectOrContent::Content(
            asyncs_form(
                transaction,
                me,
                uri,
                csrf.as_ref(),
                event_data,
                None,
                form.context,
            )
            .await?,
        )
    })
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DeleteForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/asyncs/<kind>/delete", data = "<form>")]
pub(crate) async fn delete(
    pool: &State<PgPool>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    kind: String,
    form: Form<Contextual<'_, DeleteForm>>,
) -> Result<Redirect, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    let id = resolve_edit(&mut transaction, series, event, &kind).await?.ok_or(StatusOrError::Status(Status::NotFound))?;

    if !event_data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.value.is_some() && form.context.errors().next().is_none() {
        sqlx::query("SELECT id FROM asyncs WHERE id=$1 FOR UPDATE").bind(id).execute(&mut *transaction).await?;
        let protected: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM async_teams WHERE async_id=$1) OR EXISTS(SELECT 1 FROM async_players WHERE async_id=$1) FROM asyncs WHERE id=$1")
            .bind(id).fetch_one(&mut *transaction).await?;
        if protected { return Err(StatusOrError::Status(Status::Conflict)); }
        sqlx::query("DELETE FROM asyncs WHERE id=$1").bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
    }

    Ok(Redirect::to(format!("{}#async-qualifiers", uri!(super::qualifiers::get(series, event)))))
}

#[rocket::post("/event/<series>/<event>/asyncs/<id>/generate", data = "<form>")]
pub(crate) async fn generate(
    pool: &State<PgPool>,
    state: &State<Arc<GlobalState>>,
    me: User,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    id: i64,
    form: Form<Contextual<'_, DeleteForm>>,
) -> Result<Flash<Redirect>, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_none() || form.context.errors().next().is_some() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }
    let mut tx = pool.begin().await?;
    let data = Data::new(&mut tx, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !data.organizers(&mut tx).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let requested: Option<bool> = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM async_teams WHERE async_id=$1 AND requested IS NOT NULL) FROM asyncs WHERE id=$1 AND series=$2 AND event=$3")
        .bind(id).bind(series).bind(event).fetch_optional(&mut *tx).await?;
    match requested {
        None => return Err(StatusOrError::Status(Status::NotFound)),
        Some(true) => return Err(StatusOrError::Status(Status::Conflict)),
        Some(false) => (),
    }
    tx.commit().await?;

    let redirect = Redirect::to(format!(
        "{}#async-qualifiers",
        uri!(super::qualifiers::get(series, event))
    ));
    let result = async {
        let mut updates = Arc::clone(state.inner()).roll_async_qualifier(&data).await;
        loop {
            match updates.recv().await {
                Some(SeedRollUpdate::Done { mut seed, .. }) => {
                    let extra = seed
                        .extra(Utc::now())
                        .await
                        .map_err(|error| error.to_string())?;
                    seed.file_hash = extra.file_hash;
                    seed.password = extra.password;
                    return Ok::<_, String>(seed);
                }
                Some(SeedRollUpdate::Error(error)) => return Err(error.to_string()),
                None => return Err("Seed generator stopped before returning a seed.".into()),
                _ => (),
            }
        }
    }
    .await;
    match result {
        Ok(seed) => {
            if save_seed(pool, id, seed).await? {
                Ok(Flash::success(redirect, "Seed generated."))
            } else {
                Ok(Flash::new(
                    redirect,
                    "async-seed-error",
                    "This async was requested or deleted before the seed could be saved.",
                ))
            }
        }
        Err(error) => Ok(Flash::new(redirect, "async-seed-error", error)),
    }
}

async fn save_seed(pool: &PgPool, id: i64, seed: seed::Data) -> Result<bool, sqlx::Error> {
    let Some(data) = seed.to_seed_data() else {
        return Ok(false);
    };
    let mut tx = pool.begin().await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM asyncs WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Ok(false);
    }
    let requested: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM async_teams WHERE async_id=$1 AND requested IS NOT NULL)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if requested {
        return Ok(false);
    }
    let password = seed
        .password
        .map(|password| password.into_iter().map(char::from).collect::<String>());
    let hashes = seed
        .file_hash
        .map(|hash| hash.map(Some))
        .unwrap_or_default();
    sqlx::query("UPDATE asyncs SET seed_data=$2,seed_password=$3,hash1=$4,hash2=$5,hash3=$6,hash4=$7,hash5=$8,file_stem=NULL,web_id=NULL,web_gen_time=NULL,tfb_uuid=NULL,xkeys_uuid=NULL WHERE id=$1")
        .bind(id).bind(data).bind(password)
        .bind(&hashes[0]).bind(&hashes[1]).bind(&hashes[2]).bind(&hashes[3]).bind(&hashes[4])
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}
