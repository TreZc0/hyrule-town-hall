//! Application-owned MMR hash emojis, shared by every Discord async seed delivery.
use {
    crate::prelude::*,
    serenity::all::{CreateAttachment, Http},
    std::{io::Cursor, sync::RwLock},
};

static EMOJIS: LazyLock<RwLock<HashMap<String, String>>> = LazyLock::new(RwLock::default);
static SYNC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn sync(ctx: &DiscordCtx, pool: &PgPool) -> anyhow::Result<()> {
    // Reconnects (or multiple shards) must not create the same emoji concurrently.
    let _sync = SYNC.lock().await;
    let icons: Vec<(String, String)> = sqlx::query_as(
        "SELECT icons.name, icons.file_name FROM hash_icons icons JOIN games ON games.id = icons.game_id WHERE games.name = 'mmr' ORDER BY icons.name",
    ).fetch_all(pool).await?;
    if icons.is_empty() {
        return Ok(());
    }
    let rendered = load(&ctx.http, &icons).await?;
    *EMOJIS.write().expect("MMR hash emoji lock poisoned") = rendered;
    Ok(())
}

async fn load(http: &Http, icons: &[(String, String)]) -> anyhow::Result<HashMap<String, String>> {
    let mut emojis: HashMap<_, _> = http
        .get_application_emojis()
        .await?
        .into_iter()
        .map(|emoji| (emoji.name.clone(), emoji))
        .collect();
    for (_, file_name) in icons {
        let name = emoji_name(file_name);
        if emojis.contains_key(&name) {
            continue;
        }
        let image = match emoji_image(file_name).await {
            Ok(image) => image,
            Err(error) => {
                log::warn!("Could not read MMR hash icon {file_name}: {error}");
                continue;
            }
        };
        match http
            .create_application_emoji(&json!({"name": name, "image": image}))
            .await
        {
            Ok(emoji) => {
                emojis.insert(name, emoji);
            }
            Err(error) => {
                // Keep all icons that are already available; do not repeat a failing
                // upload for every remaining icon (e.g. an application at its limit).
                log::warn!(
                    "Could not create MMR hash emoji {name}; using readable names for missing icons: {error}"
                );
                break;
            }
        }
    }
    Ok(icons
        .iter()
        .filter_map(|(item, file_name)| {
            emojis
                .get(&emoji_name(file_name))
                .filter(|emoji| emoji.available)
                .map(|emoji| (item.clone(), emoji.to_string()))
        })
        .collect())
}

fn emoji_name(file_name: &str) -> String {
    format!(
        "mmr_{}",
        Path::new(file_name)
            .file_stem()
            .expect("hash icon filename")
            .to_string_lossy()
            .to_ascii_lowercase()
    )
}

async fn emoji_image(file_name: &str) -> anyhow::Result<String> {
    let bytes = fs::read(Path::new("assets/static/hash-icon").join(file_name)).await?;
    let image = image::load_from_memory(&bytes)?.resize_exact(
        128,
        128,
        image::imageops::FilterType::Nearest,
    );
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(CreateAttachment::bytes(png.into_inner(), "hash.png").to_base64())
}

pub(crate) fn format(hash: &[String]) -> String {
    render(hash, &EMOJIS.read().expect("MMR hash emoji lock poisoned"))
}

fn render(hash: &[String], emojis: &HashMap<String, String>) -> String {
    hash.iter()
        .map(|item| {
            emojis
                .get(item)
                .cloned()
                .unwrap_or_else(|| display_name(item))
        })
        .join(", ")
}

fn display_name(item: &str) -> String {
    // Older MMR versions used this value for the Bombers Notebook.
    if item == "0x61" {
        return "Bombers Notebook".into();
    }
    let Some(name) = item.strip_prefix("ITEM_") else {
        return item.into();
    };
    name.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            let first = chars.next().expect("nonempty word");
            first
                .to_uppercase()
                .chain(chars.flat_map(char::to_lowercase))
                .collect::<String>()
        })
        .join(" ")
}
