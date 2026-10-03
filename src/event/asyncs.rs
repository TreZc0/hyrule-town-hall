use crate::{
    event::{AsyncKind, Data, Series, Tab},
    hash_icon_db::HashIconData,
    prelude::*,
    racetime_bot::seed_gen_type::SeedGenType,
    seed,
};

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

fn parse_async_kind(value: &str) -> Option<AsyncKind> {
    all::<AsyncKind>().find(|kind| format!("{:?}", kind) == value)
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
    mut transaction: Transaction<'_, Postgres>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<&CsrfToken>,
    event: Data<'_>,
    edit_kind: Option<AsyncKind>,
    ctx: Context<'_>,
) -> Result<RawHtml<String>, event::Error> {
    let header = event
        .header(&mut transaction, Some(&me), Tab::Asyncs, false)
        .await?;

    struct AsyncRow {
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
        r#"SELECT kind AS "kind: AsyncKind", file_stem, web_id, tfb_uuid, xkeys_uuid, seed_data,
               hash1::text as hash1, hash2::text as hash2, hash3::text as hash3, hash4::text as hash4, hash5::text as hash5,
               start, end_time FROM asyncs WHERE series = $1 AND event = $2 ORDER BY kind"#,
        event.series as _,
        &event.event
    )
    .fetch_all(&mut *transaction)
    .await?;

    let editing_async = edit_kind.and_then(|kind| asyncs.iter().find(|row| row.kind == kind));
    let default_kind = editing_async
        .map(|row| format!("{:?}", row.kind))
        .unwrap_or_else(|| format!("{:?}", AsyncKind::Qualifier1));
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

    Ok(
        page(
            transaction,
            &Some(me),
            &uri,
            PageStyle {
                chests: event.chests().await?,
                ..PageStyle::default()
            },
            &format!("Asyncs — {}", event.display_name),
            html! {
                : header;
                article {
                    h2 : "Async Qualifiers";
                    @if asyncs.is_empty() {
                        p : "No asyncs defined.";
                    } else {
                        table {
                            thead {
                                tr {
                                    th : "Kind";
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
                                    tr {
                                        td : format!("{:?}", row.kind);
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
                                                td : row.tfb_uuid.map(|u| u.to_string()).unwrap_or_default();
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
                                                td : row.file_stem.unwrap_or_default();
                                                td : row.web_id.map(|id| id.to_string()).unwrap_or_default();
                                            }
                                        }
                                        td : row.start.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default();
                                        td : row.end_time.map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default();
                                        td {
                                            @let kind_name = format!("{:?}", row.kind);
                                            a(class = "button", href = uri!(get(event.series, &*event.event, Some(kind_name.clone())))) : "Edit";
                                            @if matches!(seed_form_kind, AsyncSeedFormKind::Mmr) {
                                                form(action = uri!(generate_mmr(event.series, &*event.event, kind_name.clone())).to_string(), method = "post", style = "display: inline;") {
                                                    input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                                                    button(type = "submit") : if row.seed_data.is_some() { "Regenerate seed" } else { "Generate seed" };
                                                }
                                            }
                                            : " | ";
                                            form(action = uri!(delete(event.series, &*event.event, kind_name)).to_string(), method = "post", style = "display: inline;") {
                                                input(type = "hidden", name = "csrf", value? = csrf.map(|token| token.authenticity_token()));
                                                button(type = "submit", onclick = "return confirm('Are you sure you want to delete this async?')") : "Delete";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    @if let Some(edit_kind) = edit_kind {
                        p {
                            : "Editing ";
                            b : format!("{:?}", edit_kind);
                            : ". ";
                            a(href = uri!(get(event.series, &*event.event, None::<String>)).to_string()) : "Cancel edit";
                        }
                    }
                    h3 : if edit_kind.is_some() { "Edit Async" } else { "Add/Update Async" };
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
                        @match seed_form_kind {
                            AsyncSeedFormKind::Mmr => { p : "Save the async, then use Generate seed. MMR seeds use the event settings and cannot be replaced after an entrant requests this async."; }
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
                    }, errors, if edit_kind.is_some() { "Update Async" } else { "Save Async" });
                }
            },
        )
        .await?,
    )
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
    let edit_kind = edit_kind.as_deref().and_then(parse_async_kind);
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
    #[field(default = None)]
    file_stem: Option<String>,
    #[field(default = None)]
    web_id: Option<i64>,
    #[field(default = None)]
    tfb_uuid: Option<String>,
    #[field(default = None)]
    xkeys_uuid: Option<String>,
    #[field(default = None)]
    permalink: Option<String>,
    #[field(default = None)]
    seed_hash: Option<String>,
    #[field(default = None)]
    avianart_hash: Option<String>,
    #[field(default = None)]
    avianart_seed_hash: Option<String>,
    #[field(default = None)]
    hash1: Option<String>,
    #[field(default = None)]
    hash2: Option<String>,
    #[field(default = None)]
    hash3: Option<String>,
    #[field(default = None)]
    hash4: Option<String>,
    #[field(default = None)]
    hash5: Option<String>,
    #[field(default = None)]
    start: Option<String>,
    #[field(default = None)]
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
                    Some(value.kind),
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
                                    Some(value.kind),
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
                                    Some(value.kind),
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
                            Some(value.kind),
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
                                        Some(value.kind),
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
                                    Some(value.kind),
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
                                    Some(value.kind),
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

            if matches!(seed_form_kind, AsyncSeedFormKind::Mmr) {
                // Editing dates must not overwrite a seed generated concurrently.
                sqlx::query("INSERT INTO asyncs (series, event, kind, start, end_time) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (series,event,kind) DO UPDATE SET start=EXCLUDED.start, end_time=EXCLUDED.end_time")
                    .bind(series).bind(event).bind(value.kind).bind(start).bind(end_time)
                    .execute(&mut *transaction).await?;
                transaction.commit().await?;
                return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, None::<String>)))));
            }

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
            sqlx::query!(
                r#"INSERT INTO asyncs (series, event, kind, file_stem, web_id, tfb_uuid, xkeys_uuid, seed_data, hash1, hash2, hash3, hash4, hash5, start, end_time)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                   ON CONFLICT (series, event, kind) DO UPDATE SET
                       file_stem = EXCLUDED.file_stem,
                       web_id = EXCLUDED.web_id,
                       tfb_uuid = EXCLUDED.tfb_uuid,
                       xkeys_uuid = EXCLUDED.xkeys_uuid,
                       seed_data = EXCLUDED.seed_data,
                       hash1 = EXCLUDED.hash1,
                       hash2 = EXCLUDED.hash2,
                       hash3 = EXCLUDED.hash3,
                       hash4 = EXCLUDED.hash4,
                       hash5 = EXCLUDED.hash5,
                       start = EXCLUDED.start,
                       end_time = EXCLUDED.end_time"#,
                event_data.series as _,
                &event_data.event,
                value.kind as _,
                file_stem,
                value.web_id,
                tfb_uuid,
                xkeys_uuid,
                seed_data,
                hash1 as _,
                hash2 as _,
                hash3 as _,
                hash4 as _,
                hash5 as _,
                start,
                end_time
            )
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, None::<String>))))
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

async fn save_mmr_seed(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    kind: AsyncKind,
    seed_data: serde_json::Value,
    previous: Option<serde_json::Value>,
) -> Result<bool, sqlx::Error> {
    if previous.is_some() {
        // Briefly block request writes while checking replacement eligibility.
        // No lock is held during the API call, and request handlers need no MMR logic.
        sqlx::query("LOCK TABLE async_teams IN SHARE MODE").execute(&mut **transaction).await?;
    }
    let changed = sqlx::query("UPDATE asyncs SET seed_data=$4 WHERE series=$1 AND event=$2 AND kind=$3 AND seed_data IS NOT DISTINCT FROM $5 AND (seed_data IS NULL OR NOT EXISTS (SELECT 1 FROM async_teams a JOIN teams t ON t.id=a.team WHERE t.series=$1 AND t.event=$2 AND a.kind=$3 AND a.requested IS NOT NULL))")
        .bind(series).bind(event).bind(kind).bind(seed_data).bind(previous).execute(&mut **transaction).await?;
    Ok(changed.rows_affected() == 1)
}

#[rocket::post("/event/<series>/<event>/asyncs/<kind>/generate-mmr", data = "<form>")]
pub(crate) async fn generate_mmr(
    pool: &State<PgPool>,
    state: &State<Arc<racetime_bot::GlobalState>>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    kind: String,
    form: Form<Contextual<'_, DeleteForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    if form.value.is_none() { return Err(StatusOrError::Status(Status::BadRequest)); }
    let kind = parse_async_kind(&kind).ok_or(StatusOrError::Status(Status::BadRequest))?;
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if !data.asyncs_active { return Err(StatusOrError::Status(Status::NotFound)); }
    if !me.is_global_admin() && !data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let Some(SeedGenType::Mmr { config }) = data.seed_gen_type.clone() else {
        return Err(StatusOrError::Status(Status::BadRequest));
    };
    // Serializes generation with scheduled MMR async matches, without a DB job.
    let _guard = state.mmr_async_lock.lock().await;
    let previous: Option<serde_json::Value> = sqlx::query_scalar("SELECT seed_data FROM asyncs WHERE series=$1 AND event=$2 AND kind=$3")
        .bind(series).bind(event).bind(kind).fetch_optional(&mut *transaction).await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if previous.is_some() {
        let requested: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM async_teams a JOIN teams t ON t.id=a.team WHERE t.series=$1 AND t.event=$2 AND a.kind=$3 AND a.requested IS NOT NULL)")
            .bind(series).bind(event).bind(kind).fetch_one(&mut *transaction).await?;
        if requested { return Err(StatusOrError::Status(Status::Conflict)); }
    }
    transaction.commit().await?;
    let (updates, _receiver) = mpsc::channel(16);
    let result = state.mmr_api_client.roll(config, crate::mmr_web::Purpose::Competition, &updates).await;
    let mut transaction = pool.begin().await?;
    match result {
        Ok(seed) => {
            if !save_mmr_seed(&mut transaction, series, event, kind, seed.to_seed_data().expect("MMR seed has metadata"), previous).await? {
                return Err(StatusOrError::Status(Status::Conflict));
            }
            transaction.commit().await?;
            Ok(RedirectOrContent::Redirect(Redirect::to(uri!(get(series, event, None::<String>)))))
        }
        Err(error) => {
            form.context.push_error(form::Error::validation(error.to_string()));
            Ok(RedirectOrContent::Content(asyncs_form(transaction, me, uri, csrf.as_ref(), data, Some(kind), form.context).await?))
        }
    }
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
    let kind = parse_async_kind(&kind).ok_or(StatusOrError::Status(Status::BadRequest))?;

    if !event_data.asyncs_active {
        return Err(StatusOrError::Status(Status::NotFound));
    }
    if !me.is_global_admin() && !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.value.is_some() {
        if matches!(event_data.seed_gen_type, Some(SeedGenType::Mmr { .. })) {
            sqlx::query("LOCK TABLE async_teams IN SHARE MODE").execute(&mut *transaction).await?;
            let requested: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM async_teams a JOIN teams t ON t.id=a.team WHERE t.series=$1 AND t.event=$2 AND a.kind=$3 AND a.requested IS NOT NULL)")
                .bind(series).bind(event).bind(kind).fetch_one(&mut *transaction).await?;
            if requested { return Err(StatusOrError::Status(Status::Conflict)); }
        }
        sqlx::query!(
            "DELETE FROM asyncs WHERE series = $1 AND event = $2 AND kind = $3",
            series as _,
            event,
            kind as _
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }

    Ok(Redirect::to(uri!(get(series, event, None::<String>))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires HTH_MMR_TEST_DATABASE_URL; uses connection-local temporary tables"]
    async fn mmr_shared_async_allows_initial_generation_after_requests_but_not_replacement() {
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1)
            .connect(&std::env::var("HTH_MMR_TEST_DATABASE_URL").unwrap()).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(r#"
            CREATE TEMP TABLE teams (id BIGINT, series TEXT, event TEXT);
            CREATE TYPE pg_temp.async_kind AS ENUM ('qualifier', 'qualifier2');
            CREATE TEMP TABLE asyncs (series TEXT, event TEXT, kind pg_temp.async_kind, seed_data JSONB);
            CREATE TEMP TABLE async_teams (team BIGINT, kind pg_temp.async_kind, requested TIMESTAMPTZ);
            INSERT INTO teams VALUES (1,'mmrmain','event'), (2,'mmrmain','other-event');
            INSERT INTO asyncs VALUES ('mmrmain','event','qualifier',NULL), ('mmrmain','event','qualifier2',NULL);
            INSERT INTO async_teams VALUES (1,'qualifier',NOW()), (2,'qualifier2',NOW());
        "#).execute(&mut *tx).await.unwrap();
        let first = json!({"type":"mmr", "id":"first", "hash":["ITEM_BOW"], "encrypted":true, "locked":true});
        let second = json!({"type":"mmr", "id":"second", "hash":["ITEM_BOMB"], "encrypted":true, "locked":true});

        // Signup already requested this qualifier before its first seed was generated.
        assert!(save_mmr_seed(&mut tx, Series::MmrMain, "event", AsyncKind::Qualifier1, first.clone(), None).await.unwrap());
        assert!(!save_mmr_seed(&mut tx, Series::MmrMain, "event", AsyncKind::Qualifier1, second.clone(), Some(first.clone())).await.unwrap());

        // Requests for other qualifiers/events do not prevent an unused seed's replacement.
        assert!(save_mmr_seed(&mut tx, Series::MmrMain, "event", AsyncKind::Qualifier2, first.clone(), None).await.unwrap());
        assert!(save_mmr_seed(&mut tx, Series::MmrMain, "event", AsyncKind::Qualifier2, second.clone(), Some(first.clone())).await.unwrap());
        // A generation response based on older data cannot overwrite the current seed.
        assert!(!save_mmr_seed(&mut tx, Series::MmrMain, "event", AsyncKind::Qualifier2, first.clone(), None).await.unwrap());

        // A request arriving during the API call prevents publishing a replacement.
        sqlx::query("INSERT INTO async_teams VALUES (1,'qualifier2',NOW())").execute(&mut *tx).await.unwrap();
        assert!(!save_mmr_seed(&mut tx, Series::MmrMain, "event", AsyncKind::Qualifier2, first.clone(), Some(second.clone())).await.unwrap());
        let saved: Vec<serde_json::Value> = sqlx::query_scalar("SELECT seed_data FROM asyncs ORDER BY kind")
            .fetch_all(&mut *tx).await.unwrap();
        assert_eq!(saved, [first, second]);
        tx.rollback().await.unwrap();
        pool.close().await;
    }
}
