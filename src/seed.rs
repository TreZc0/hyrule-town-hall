use {
    chrono::TimeDelta,
    futures::stream::Stream,
    hyper::header::{
        ACCESS_CONTROL_ALLOW_ORIGIN,
        CONTENT_DISPOSITION,
        LINK,
    },
    ootr_utils::spoiler::OcarinaNote,
    rocket::{
        fs::NamedFile,
        http::Header,
        response::content::{
            RawJson,
            RawHtml,
        },
        uri,
    },
    rocket_util::{
        html,
        OptSuffix,
    },
    serde::Deserialize,
    crate::{
        hash_icon::SpoilerLog,
        hash_icon_db::HashIconData,
        prelude::*,
        racetime_bot::SeedMetadata,
    }
};

#[cfg(unix)] pub(crate) const DIR: &str = "/var/www/midos.house/seed";
#[cfg(windows)] pub(crate) const DIR: &str = "G:/source/hth-seeds";

/// ootrandomizer.com seeds are deleted after 60 days (https://discord.com/channels/274180765816848384/1248210891636342846/1257367685658837126)
const WEB_TIMEOUT: TimeDelta = TimeDelta::days(60);

pub(crate) type Settings = serde_json::Map<String, serde_json::Value>;

#[derive(Default, Debug, Clone)]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) struct Data {
    pub(crate) file_hash: Option<[String; 5]>,
    pub(crate) password: Option<[OcarinaNote; 6]>,
    pub(crate) seed_data: Option<serde_json::Value>,
    pub(crate) progression_spoiler: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Files {
    AlttprDoorRando {
        uuid: Uuid,
    },
    MidosHouse {
        file_stem: Cow<'static, str>,
        locked_spoiler_log_path: Option<String>,
    },
    OotrWeb {
        id: i64,
        gen_time: DateTime<Utc>,
        file_stem: Cow<'static, str>,
    },
    TriforceBlitz {
        is_dev: bool,
        uuid: Uuid,
    },
    TfbSotd {
        date: NaiveDate,
        ordinal: u64,
    },
    TwwrPermalink {
        permalink: String,
        seed_hash: String,
    },
    AvianartSeed {
        hash: String,
        seed_hash: Option<[String; 5]>,
    },
}

impl Files {
    /// Serialize to the unified `seed_data` JSONB format stored on the `races` table.
    ///
    /// For `AlttprDoorRando`, the file hash icons are stored separately on `Data` and must be
    /// merged in via `Data::to_seed_data()` — this method omits them.
    pub(crate) fn to_seed_data_base(&self) -> serde_json::Value {
        match self {
            Self::AlttprDoorRando { uuid } => serde_json::json!({
                "type": "alttpr_dr",
                "uuid": uuid.to_string(),
            }),
            Self::AvianartSeed { hash, seed_hash } => {
                let mut obj = serde_json::json!({"type": "alttpr_avianart", "hash": hash});
                if let Some(sh) = seed_hash {
                    // Store as JSON array (canonical new format)
                    obj["seed_hash"] = serde_json::Value::Array(
                        sh.iter().map(|s| serde_json::Value::String(s.clone())).collect()
                    );
                }
                obj
            }
            Self::OotrWeb { id, gen_time, file_stem } => serde_json::json!({
                "type": "ootr_web",
                "id": id,
                "gen_time": gen_time.to_rfc3339(),
                "file_stem": file_stem.as_ref(),
            }),
            Self::TriforceBlitz { is_dev, uuid } => serde_json::json!({
                "type": "ootr_tfb",
                "uuid": uuid.to_string(),
                "is_dev": is_dev,
            }),
            Self::MidosHouse { file_stem, locked_spoiler_log_path } => {
                let mut obj = serde_json::json!({
                    "type": "midos_house",
                    "file_stem": file_stem.as_ref(),
                });
                if let Some(path) = locked_spoiler_log_path {
                    obj["locked_spoiler_log_path"] = serde_json::Value::String(path.clone());
                }
                obj
            }
            Self::TwwrPermalink { permalink, seed_hash } => serde_json::json!({
                "type": "twwr",
                "permalink": permalink,
                "seed_hash": seed_hash,
            }),
            Self::TfbSotd { date, ordinal } => serde_json::json!({
                "type": "tfb_sotd",
                "date": date.to_string(),
                "ordinal": ordinal,
            }),
        }
    }

    /// Parse from the unified `seed_data` JSONB column.
    ///
    /// Returns `None` if `value` has an unrecognised `type` field or is missing required fields.
    pub(crate) fn from_seed_data(value: &serde_json::Value) -> Option<Self> {
        match value.get("type").and_then(|v| v.as_str())? {
            "alttpr_dr" => {
                let uuid = value.get("uuid")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())?;
                Some(Self::AlttprDoorRando { uuid })
            }
            "alttpr_avianart" => {
                let hash = value.get("hash").and_then(|v| v.as_str())?.to_owned();
                // seed_hash may be a JSON array (new format) or a comma-separated string (migrated)
                let seed_hash = if let Some(arr) = value.get("seed_hash").and_then(|v| v.as_array()) {
                    let parts: Vec<String> = arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect();
                    parts.try_into().ok()
                } else {
                    value.get("seed_hash")
                        .and_then(|v| v.as_str())
                        .and_then(|s| crate::avianart::parse_file_hash(s).ok())
                };
                Some(Self::AvianartSeed { hash, seed_hash })
            }
            "ootr_web" => {
                let id = value.get("id").and_then(|v| v.as_i64())?;
                let gen_time: DateTime<Utc> = value.get("gen_time")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())?;
                let file_stem = value.get("file_stem").and_then(|v| v.as_str())?.to_owned();
                Some(Self::OotrWeb { id, gen_time, file_stem: Cow::Owned(file_stem) })
            }
            "ootr_tfb" => {
                let uuid = value.get("uuid")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())?;
                let is_dev = value.get("is_dev").and_then(|v| v.as_bool()).unwrap_or(false);
                Some(Self::TriforceBlitz { is_dev, uuid })
            }
            "midos_house" => {
                let file_stem = value.get("file_stem").and_then(|v| v.as_str())?.to_owned();
                let locked_spoiler_log_path = value.get("locked_spoiler_log_path")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                Some(Self::MidosHouse { file_stem: Cow::Owned(file_stem), locked_spoiler_log_path })
            }
            "twwr" => {
                let permalink = value.get("permalink").and_then(|v| v.as_str())?.to_owned();
                let seed_hash = value.get("seed_hash").and_then(|v| v.as_str())?.to_owned();
                Some(Self::TwwrPermalink { permalink, seed_hash })
            }
            "tfb_sotd" => {
                let date = value.get("date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())?;
                let ordinal = value.get("ordinal").and_then(|v| v.as_u64())?;
                Some(Self::TfbSotd { date, ordinal })
            }
            _ => None,
        }
    }
}

impl Data {
    /// Return a typed view of the seed data, parsed from the JSON blob.
    pub(crate) fn files(&self) -> Option<Files> {
        self.seed_data.as_ref().and_then(Files::from_seed_data)
    }

    /// Serialize to the unified `seed_data` JSONB value for storage in the `races` table.
    ///
    /// For `AlttprDoorRando`, merges in `file_hash` (hash1..5) so they survive the column drop.
    pub(crate) fn to_seed_data(&self) -> Option<serde_json::Value> {
        let mut obj = self.seed_data.clone()?;
        // Merge hash icons into alttpr_dr entries so they're preserved after column drop
        if obj.get("type").and_then(|v| v.as_str()) == Some("alttpr_dr") {
            if let Some([h1, h2, h3, h4, h5]) = &self.file_hash {
                obj["hash1"] = h1.as_str().into();
                obj["hash2"] = h2.as_str().into();
                obj["hash3"] = h3.as_str().into();
                obj["hash4"] = h4.as_str().into();
                obj["hash5"] = h5.as_str().into();
            }
        }
        Some(obj)
    }

    /// Construct `Data` purely from the unified `seed_data` JSONB column (post-migration races).
    ///
    /// This is the simplified constructor used after migration 060 drops the old individual
    /// seed columns from the `races` table.
    pub(crate) fn from_seed_data_only(
        seed_data: Option<serde_json::Value>,
        password: Option<&str>,
        progression_spoiler: bool,
    ) -> Self {
        // For AlttprDR: hash icons are stored inline in seed_data; extract them for the field.
        let file_hash = if let Some(ref data) = seed_data {
            if data.get("type").and_then(|v| v.as_str()) == Some("alttpr_dr") {
                let h1 = data.get("hash1").and_then(|v| v.as_str()).map(str::to_owned);
                let h2 = data.get("hash2").and_then(|v| v.as_str()).map(str::to_owned);
                let h3 = data.get("hash3").and_then(|v| v.as_str()).map(str::to_owned);
                let h4 = data.get("hash4").and_then(|v| v.as_str()).map(str::to_owned);
                let h5 = data.get("hash5").and_then(|v| v.as_str()).map(str::to_owned);
                match (h1, h2, h3, h4, h5) {
                    (Some(h1), Some(h2), Some(h3), Some(h4), Some(h5)) => Some([h1, h2, h3, h4, h5]),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        Self {
            file_hash,
            password: password.map(|pw| {
                pw.chars()
                    .map(|note| OcarinaNote::try_from(note)
                        .expect("invalid ocarina note in password, should be prevented by SQL constraint"))
                    .collect_vec()
                    .try_into()
                    .expect("invalid password length, should be prevented by SQL constraint")
            }),
            seed_data,
            progression_spoiler,
        }
    }

    pub(crate) fn from_db(
        start: Option<DateTime<Utc>>,
        async_start1: Option<DateTime<Utc>>,
        async_start2: Option<DateTime<Utc>>,
        async_start3: Option<DateTime<Utc>>,
        file_stem: Option<String>,
        locked_spoiler_log_path: Option<String>,
        web_id: Option<i64>,
        web_gen_time: Option<DateTime<Utc>>,
        is_tfb_dev: bool,
        tfb_uuid: Option<Uuid>,
        xkeys_uuid: Option<Uuid>,
        seed_data: Option<serde_json::Value>,
        hash1: Option<String>,
        hash2: Option<String>,
        hash3: Option<String>,
        hash4: Option<String>,
        hash5: Option<String>,
        password: Option<&str>,
        progression_spoiler: bool,
    ) -> Self {
        Self {
            file_hash: match (hash1, hash2, hash3, hash4, hash5) {
                (Some(hash1), Some(hash2), Some(hash3), Some(hash4), Some(hash5)) => Some([hash1, hash2, hash3, hash4, hash5]),
                (None, None, None, None, None) => None,
                _ => unreachable!("only some hash icons present, should be prevented by SQL constraint"),
            },
            password: password.map(|pw| pw.chars().map(|note| OcarinaNote::try_from(note).expect("invalid ocarina note in password, should be prevented by SQL constraint")).collect_vec().try_into().expect("invalid password length, should be prevented by SQL constraint")),
            seed_data: match (file_stem, locked_spoiler_log_path, web_id, web_gen_time, tfb_uuid, xkeys_uuid, seed_data) {
                (_, _, _, _, Some(uuid), None, None) => Some(Files::TriforceBlitz { is_dev: is_tfb_dev, uuid }.to_seed_data_base()),
                (Some(file_stem), _, Some(id), Some(gen_time), None, None, None) => Some(Files::OotrWeb { id, gen_time, file_stem: Cow::Owned(file_stem) }.to_seed_data_base()),
                (Some(file_stem), locked_spoiler_log_path, Some(id), None, None, None, None) => Some(if let Some(first_start) = [start, async_start1, async_start2, async_start3].into_iter().filter_map(identity).min() {
                    Files::OotrWeb { id, gen_time: first_start - TimeDelta::days(1), file_stem: Cow::Owned(file_stem) }.to_seed_data_base()
                } else {
                    Files::MidosHouse { file_stem: Cow::Owned(file_stem), locked_spoiler_log_path }.to_seed_data_base()
                }),
                (Some(file_stem), locked_spoiler_log_path, None, _, None, None, None) => Some(Files::MidosHouse { file_stem: Cow::Owned(file_stem), locked_spoiler_log_path }.to_seed_data_base()),
                (_, _, _, _, _, Some(uuid), None) => Some(Files::AlttprDoorRando { uuid }.to_seed_data_base()),
                (_, _, _, _, _, _, Some(ref old_data)) => (|| {
                    if let Some(hash) = old_data.get("avianart_hash").and_then(|v| v.as_str()) {
                        let seed_hash = old_data.get("avianart_seed_hash")
                            .and_then(|v| v.as_str())
                            .and_then(|s| crate::avianart::parse_file_hash(s).ok());
                        return Some(Files::AvianartSeed { hash: hash.to_owned(), seed_hash }.to_seed_data_base());
                    }
                    let permalink = old_data.get("permalink")?.as_str()?.to_owned();
                    let seed_hash = old_data.get("seed_hash")?.as_str()?.to_owned();
                    Some(Files::TwwrPermalink { permalink, seed_hash }.to_seed_data_base())
                })(),
                (None, _, _, _, None, None, None) => None,
            },
            progression_spoiler,
        }
    }

    pub(crate) async fn extra(&self, now: DateTime<Utc>) -> Result<ExtraData, ExtraDataError> {
        /// If some other part of the log like settings or version number can't be parsed, we may still be able to read the file hash and password from the log
        #[derive(Deserialize)]
        struct SparseSpoilerLog {
            file_hash: [String; 5],
            password: Option<[OcarinaNote; 6]>,
        }

        if_chain! {
            if self.file_hash.is_none() || self.password.is_none() || match self.files() {
                Some(Files::AlttprDoorRando { .. }) => false,
                Some(Files::MidosHouse { .. }) => true,
                Some(Files::OotrWeb { gen_time, .. }) => gen_time <= now - WEB_TIMEOUT,
                Some(Files::TriforceBlitz { .. }) => false,
                Some(Files::TfbSotd { .. }) => false,
                Some(Files::TwwrPermalink { .. }) => false,
                Some(Files::AvianartSeed { .. }) => false,
                None => false,
            };
            if let Some((spoiler_path, spoiler_file_name)) = match self.files() {
                Some(Files::MidosHouse { locked_spoiler_log_path: Some(ref spoiler_path), .. }) if fs::exists(spoiler_path).await? => Some((PathBuf::from(spoiler_path), None)),
                Some(Files::MidosHouse { ref file_stem, .. } | Files::OotrWeb { ref file_stem, .. }) => {
                    let spoiler_file_name = format!("{file_stem}_Spoiler.json");
                    Some((Path::new(DIR).join(&spoiler_file_name).to_owned(), Some(spoiler_file_name)))
                }
                _ => None,
            };
            then {
                let spoiler_path_exists = spoiler_path.exists();
                let (file_hash, password, world_count, chests) = if spoiler_path_exists {
                    let log = fs::read_to_string(&spoiler_path).await?;
                    if let Ok(log) = serde_json::from_str::<SpoilerLog>(&log) {
                                            (Some(log.file_hash.clone()), log.password, Some(log.settings[0].world_count), if spoiler_file_name.is_some() {
                        ChestAppearances::from(log)
                    } else {
                        ChestAppearances::random() // keeping chests random for locked spoilers to avoid leaking seed info
                    })
                    } else if let Ok(log) = serde_json::from_str::<SparseSpoilerLog>(&log) {
                        (Some(log.file_hash), self.password.or(log.password), None, ChestAppearances::random())
                    } else {
                        (self.file_hash.clone(), self.password, None, ChestAppearances::random())
                    }
                } else {
                    (self.file_hash.clone(), self.password, None, ChestAppearances::random())
                };
                //TODO if file_hash.is_none() and a patch file is available, read the file hash from the patched rom?
                return Ok(ExtraData {
                    spoiler_status: if spoiler_path_exists {
                        if let Some(spoiler_file_name) = spoiler_file_name {
                            SpoilerStatus::Unlocked(spoiler_file_name)
                        } else if self.progression_spoiler {
                            SpoilerStatus::Progression
                        } else {
                            SpoilerStatus::Locked
                        }
                    } else {
                        SpoilerStatus::NotFound
                    },
                    file_hash, password, world_count, chests,
                })
            }
        }
        //TODO if file_hash.is_none() and a patch file is available, read the file hash from the patched rom?
        let file_hash = self.file_hash.clone().or_else(|| {
            if let Some(Files::AvianartSeed { seed_hash, .. }) = self.files() {
                seed_hash
            } else {
                None
            }
        });
        Ok(ExtraData {
            spoiler_status: SpoilerStatus::NotFound,
            file_hash,
            password: self.password,
            world_count: None,
            chests: ChestAppearances::random(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExtraDataError {
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
}

impl IsNetworkError for ExtraDataError {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Json(_) => false,
            Self::Sql(_) => false,
            Self::Wheel(e) => e.is_network_error(),
        }
    }
}

pub(crate) struct ExtraData {
    spoiler_status: SpoilerStatus,
    pub(crate) file_hash: Option<[String; 5]>,
    pub(crate) password: Option<[OcarinaNote; 6]>,
    pub(crate) world_count: Option<NonZero<u8>>,
    chests: ChestAppearances,
}

enum SpoilerStatus {
    Unlocked(String),
    Progression,
    Locked,
    NotFound,
}

pub(crate) async fn table_cell(now: DateTime<Utc>, seed: &Data, spoiler_logs: bool, add_hash_url: Option<rocket::http::uri::Origin<'_>>, transaction: &mut Transaction<'_, Postgres>, game_id: i32, draft_mode: Option<&str>) -> Result<RawHtml<String>, ExtraDataError> {
    //TODO show seed password when appropriate
    let extra = seed.extra(now).await?;
    let mut seed_links = match seed.files() {
        Some(Files::AlttprDoorRando { uuid }) => {
            let mut patcher_url = Url::parse("https://alttprpatch.synack.live/patcher.html").expect("wrong hardcoded URL");
            patcher_url.query_pairs_mut().append_pair("patch", &format!("{}/seed/DR_{uuid}.bps", base_uri()));
            Some(html! {
                a(href = patcher_url.to_string(), target = "_blank") : "View";
            })
        }
        Some(Files::OotrWeb { id, gen_time, .. }) if gen_time > now - WEB_TIMEOUT => Some(html! {
            a(href = format!("https://ootrandomizer.com/seed/get?id={id}"), target = "_blank") : "View";
        }),
        Some(Files::OotrWeb { ref file_stem, .. } | Files::MidosHouse { ref file_stem, .. }) => Some(html! {
            a(href = format!("/seed/{file_stem}.{}", if let Some(world_count) = extra.world_count {
                if world_count.get() > 1 { "zpfz" } else { "zpf" }
            } else if Path::new(DIR).join(format!("{file_stem}.zpfz")).exists() {
                "zpfz"
            } else {
                "zpf"
            })) : "Patch File";
            @if spoiler_logs {
                @match extra.spoiler_status {
                    SpoilerStatus::Unlocked(spoiler_file_name) => {
                        : " • ";
                        a(href = format!("/seed/{spoiler_file_name}")) : "Spoiler Log";
                    }
                    SpoilerStatus::Progression => {
                        : " • ";
                        a(href = format!("/seed/{file_stem}_Progression.json")) : "Progression Spoiler";
                    }
                    SpoilerStatus::Locked | SpoilerStatus::NotFound => {}
                }
            }
        }),
        Some(Files::TriforceBlitz { is_dev, uuid }) => Some(html! {
            a(href = if is_dev {
                format!("https://dev.triforceblitz.com/seeds/{uuid}")
            } else {
                format!("https://www.triforceblitz.com/seed/{uuid}")
            }) : "View";
        }),
        Some(Files::TfbSotd { ordinal, .. }) => Some(html! {
            a(href = format!("https://www.triforceblitz.com/seed/daily/{ordinal}"), target = "_blank") : "View";
        }),
        Some(Files::TwwrPermalink { ref permalink, ref seed_hash }) => Some(html! {
            span(class = "settings-link twwr-seed-link") {
                : "Hover for Seed";
                span(class = "tooltip-content") {
                    div {
                        strong : "Permalink: ";
                        code(style = "user-select: all") : permalink;
                    }
                    div {
                        strong : "Seed Hash: ";
                        : seed_hash;
                    }
                }
            }
        }),
        Some(Files::AvianartSeed { ref hash, .. }) => Some(html! {
            a(href = format!("https://avianart.games/perm/{hash}"), target = "_blank") : "View Seed";
        }),
        None => None,
    };
    if extra.file_hash.is_none() {
        if let Some(add_hash_url) = add_hash_url {
            seed_links = Some(html! {
                @if let Some(seed_links) = seed_links {
                    : seed_links;
                    : " • ";
                }
                a(class = "clean_button", href = add_hash_url.to_string()) : "Add Hash";
            });
        }
    }
    Ok(match (extra.file_hash, seed_links, draft_mode) {
        (None, None, None) => html! {},
        (None, None, Some(mode)) => html! {
            div(class = "draft-mode") {
                strong : mode;
            }
        },
        (None, Some(seed_links), None) => seed_links,
        (None, Some(seed_links), Some(mode)) => html! {
            div(class = "draft-mode") {
                strong : mode;
            }
            : seed_links;
        },
        (Some(file_hash), None, draft_mode) => html! {
            @if let Some(mode) = draft_mode {
                div(class = "draft-mode") {
                    strong : mode;
                }
            }
            div(class = "hash") {
                @for hash_icon_name in file_hash {
                    @if let Some(hash_icon_data) = HashIconData::by_name(transaction, game_id, &hash_icon_name).await? {
                        @let file_name = &hash_icon_data.file_name;
                        @let src = format!("/static/hash-icon/{}", file_name);
                        img(class = "hash-icon", alt = hash_icon_name, src = src);
                    }
                }
            }
        },
        (Some(file_hash), Some(seed_links), draft_mode) => html! {
            div(class = "seed") {
                @if let Some(mode) = draft_mode {
                    div(class = "draft-mode") {
                        strong : mode;
                    }
                }
                div(class = "hash") {
                    @for hash_icon_name in file_hash {
                        @if let Some(hash_icon_data) = HashIconData::by_name(transaction, game_id, &hash_icon_name).await? {
                            @let file_name = &hash_icon_data.file_name;
                            @let src = format!("/static/hash-icon/{}", file_name);
                            img(class = "hash-icon", alt = hash_icon_name, src = src);
                        }
                    }
                }
                div(class = "seed-links") : seed_links;
            }
        },
    })
}

pub(crate) async fn table(seeds: impl Stream<Item = Data>, spoiler_logs: bool, transaction: &mut Transaction<'_, Postgres>, game_id: i32) -> Result<RawHtml<String>, ExtraDataError> {
    let mut seeds = pin!(seeds);
    let now = Utc::now();
    Ok(html! {
        table(class = "seeds") {
            thead {
                tr {
                    th : "Seed";
                }
            }
            tbody {
                @while let Some(seed) = seeds.next().await {
                    tr {
                        td : table_cell(now, &seed, spoiler_logs, None, transaction, game_id, None).await?;
                    }
                }
            }
        }
    })
}

#[derive(Responder)]
pub(crate) enum GetResponse {
    Page(RawHtml<String>),
    Patch {
        inner: NamedFile,
        content_disposition: Header<'static>,
        access_control_allow_origin: Header<'static>,
    },
    Spoiler {
        inner: RawJson<Vec<u8>>,
        content_disposition: Header<'static>,
        link: Header<'static>,
    },
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum GetError {
    #[error(transparent)] ExtraData(#[from] ExtraDataError),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
}

impl<E: Into<GetError>> From<E> for StatusOrError<GetError> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

#[rocket::get("/seed/<filename>")]
pub(crate) async fn get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, seed_metadata: &State<Arc<RwLock<HashMap<String, SeedMetadata>>>>, filename: OptSuffix<'_, &str>) -> Result<GetResponse, StatusOrError<GetError>> {
    let OptSuffix(file_stem, suffix) = filename;
    if !regex_is_match!("^[0-9A-Za-z_-]+$", file_stem) { return Err(StatusOrError::Status(Status::NotFound)) }
    Ok(match suffix {
        Some(suffix @ ("bps" | "zpf" | "zpfz")) => {
            let path = Path::new(DIR).join(format!("{file_stem}.{suffix}"));
            let access_control = match suffix {
                "bps" => "*",
                _ => "null"
            };
            GetResponse::Patch {
                inner: match NamedFile::open(&path).await {
                    Ok(file) => file,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(StatusOrError::Status(Status::NotFound)),
                    Err(e) => return Err(e).at(path).map_err(|e| StatusOrError::Err(GetError::Wheel(e))),
                },
                content_disposition: Header::new(CONTENT_DISPOSITION.as_str(), "attachment"),
                access_control_allow_origin: Header::new(ACCESS_CONTROL_ALLOW_ORIGIN.as_str(), access_control)
            }
        }
        Some("json") => if let Some(file_stem) = file_stem.strip_suffix("_Progression") {
            let mut transaction = pool.begin().await?;
            let SeedMetadata { locked_spoiler_log_path, progression_spoiler } = if let Some(info) = lock!(@read seed_metadata = seed_metadata; seed_metadata.get(file_stem).cloned()) {
                info
            } else if let Some(locked_spoiler_log_path) = sqlx::query_scalar!(
                "SELECT seed_data->>'locked_spoiler_log_path' FROM races WHERE seed_data->>'file_stem' = $1 AND seed_data->>'type' = 'midos_house'",
                file_stem
            ).fetch_optional(&mut *transaction).await? {
                SeedMetadata { locked_spoiler_log_path, progression_spoiler: false /* no official races with progression spoilers so far */ }
            } else {
                SeedMetadata::default()
            };
            let seed = Data {
                password: None, // not displayed
                seed_data: Some(Files::MidosHouse {
                    file_stem: Cow::Owned(file_stem.to_owned()),
                    locked_spoiler_log_path,
                }.to_seed_data_base()),
                file_hash: None,
                progression_spoiler,
            };
            let extra = seed.extra(Utc::now()).await?;
            match extra.spoiler_status {
                SpoilerStatus::Unlocked(_) | SpoilerStatus::Progression => {}
                SpoilerStatus::Locked | SpoilerStatus::NotFound => return Err(StatusOrError::Status(Status::NotFound)),
            }
            let spoiler_path = if let Some(Files::MidosHouse { locked_spoiler_log_path: Some(path), .. }) = seed.files() {
                PathBuf::from(path)
            } else {
                Path::new(DIR).join(format!("{file_stem}.json"))
            };
            let spoiler = match fs::read_json(spoiler_path).await {
                Ok(spoiler) => spoiler,
                Err(wheel::Error::Io { inner, .. }) if inner.kind() == io::ErrorKind::NotFound => return Err(StatusOrError::Status(Status::NotFound)),
                Err(e) => return Err(e.into()),
            };
            GetResponse::Spoiler {
                inner: RawJson(serde_json::to_vec_pretty(&tfb::progression_spoiler(spoiler))?),
                content_disposition: Header::new(CONTENT_DISPOSITION.as_str(), "inline"),
                // may not work in all browsers, see https://bugzilla.mozilla.org/show_bug.cgi?id=1185705
                link: Header::new(LINK.as_str(), format!(r#"<{}>; rel="icon"; sizes="1024x1024""#, uri!(favicon::favicon_png(Suffix(extra.chests.textures(), "png"))))),
            }
        } else {
            let spoiler = match fs::read(Path::new(DIR).join(format!("{file_stem}.json"))).await {
                Ok(spoiler) => spoiler,
                Err(wheel::Error::Io { inner, .. }) if inner.kind() == io::ErrorKind::NotFound => return Err(StatusOrError::Status(Status::NotFound)),
                Err(e) => return Err(e.into()),
            };
            let chests = match serde_json::from_slice::<SpoilerLog>(&spoiler) {
                Ok(spoiler) => ChestAppearances::from(spoiler),
                Err(e) => {
                    eprintln!("failed to add favicon to {file_stem}.json: {e} ({e:?})");
                    if let Environment::Production = Environment::default() {
                        log::error!("failed to add favicon to {file_stem}.json: {e} ({e:?})");
                    }
                    ChestAppearances::random()
                }
            };
            GetResponse::Spoiler {
                inner: RawJson(spoiler),
                content_disposition: Header::new(CONTENT_DISPOSITION.as_str(), "inline"),
                // may not work in all browsers, see https://bugzilla.mozilla.org/show_bug.cgi?id=1185705
                link: Header::new(LINK.as_str(), format!(r#"<{}>; rel="icon"; sizes="1024x1024""#, uri!(favicon::favicon_png(Suffix(chests.textures(), "png"))))),
            }
        },
        Some(_) => return Err(StatusOrError::Status(Status::NotFound)),
        None => {
            let mut transaction = pool.begin().await?;
            let SeedMetadata { locked_spoiler_log_path, progression_spoiler } = if let Some(info) = lock!(@read seed_metadata = seed_metadata; seed_metadata.get(file_stem).cloned()) {
                info
            } else if let Some(locked_spoiler_log_path) = sqlx::query_scalar!(
                "SELECT seed_data->>'locked_spoiler_log_path' FROM races WHERE seed_data->>'file_stem' = $1 AND seed_data->>'type' = 'midos_house'",
                file_stem
            ).fetch_optional(&mut *transaction).await? {
                SeedMetadata { locked_spoiler_log_path, progression_spoiler: false /* no official races with progression spoilers so far */ }
            } else {
                SeedMetadata::default()
            };
            let seed = Data {
                password: None, // not displayed
                seed_data: Some(Files::MidosHouse {
                    file_stem: Cow::Owned(file_stem.to_owned()),
                    locked_spoiler_log_path,
                }.to_seed_data_base()),
                file_hash: None,
                progression_spoiler,
            };
            let extra = seed.extra(Utc::now()).await?;
            let patch_suffix = if let Some(world_count) = extra.world_count {
                if world_count.get() > 1 { "zpfz" } else { "zpf" }
            } else if Path::new(DIR).join(format!("{file_stem}.zpfz")).exists() {
                "zpfz"
            } else {
                "zpf"
            };
            let hash_html = if let Some(hash) = extra.file_hash {
                html! {
                    h1(class = "hash") {
                        @for hash_icon_name in hash {
                            @if let Some(hash_icon_data) = HashIconData::by_name(&mut transaction, 1, &hash_icon_name).await? {
                                @let file_name = &hash_icon_data.file_name;
                                @let src = format!("/static/hash-icon/{}", file_name);
                                img(class = "hash-icon", alt = hash_icon_name, src = src);
                            }
                        }
                    }
                }
            } else {
                html! {
                    h1 : "Seed";
                }
            };
            GetResponse::Page(page(transaction, &me, &uri, PageStyle { kind: PageKind::Center, chests: extra.chests, ..PageStyle::default() }, "Seed — Hyrule Town Hall", html! {
                : hash_html;
                @match extra.spoiler_status {
                    SpoilerStatus::Unlocked(spoiler_filename) => div(class = "button-row") {
                        a(class = "button", href = format!("/seed/{file_stem}.{patch_suffix}")) : "Patch File";
                        a(class = "button", href = format!("/seed/{spoiler_filename}")) : "Spoiler Log";
                    }
                    SpoilerStatus::Progression => {
                        div(class = "button-row") {
                            a(class = "button", href = format!("/seed/{file_stem}.{patch_suffix}")) : "Patch File";
                            a(class = "button", href = format!("/seed/{file_stem}_Progression.json")) : "Progression Spoiler";
                        }
                        p : "Full spoiler log locked (race is still in progress)";
                    }
                    SpoilerStatus::Locked => {
                        div(class = "button-row") {
                            a(class = "button", href = format!("/seed/{file_stem}.{patch_suffix}")) : "Patch File";
                        }
                        p : "Spoiler log locked (race is still in progress)";
                    }
                    SpoilerStatus::NotFound => {
                        div(class = "button-row") {
                            a(class = "button", href = format!("/seed/{file_stem}.{patch_suffix}")) : "Patch File";
                        }
                        p : "Spoiler log not found";
                    }
                }
            }).await?)
        }
    })
}
