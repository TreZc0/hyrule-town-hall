use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "8" => Some(html! {
            article {
                p {
                    : "This is OoTR League season 8, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/101R4sZqZpslI0E77sU4jtBB5fEy0RSpEnZ99PnoUV1k/edit") : "the rules document"; //TODO import text
                    : " for details.";
                }
            }
        }),
        _ => Some(html! {
            article {
                p {
                    : "This is OoTR League season ";
                    : data.event;
                    : ", organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://league.ootrandomizer.com/") : "the official website";
                    : " for details.";
                }
            }
        }),
    })
}

#[derive(Deserialize)]
#[serde(transparent)]
struct JsonScheduleVersion(u8);

#[derive(Debug, thiserror::Error)]
#[error("expected League schedule format version 1, got version {0}")]
struct ScheduleVersionMismatch(u8);

impl TryFrom<JsonScheduleVersion> for ScheduleVersion {
    type Error = ScheduleVersionMismatch;

    fn try_from(JsonScheduleVersion(version): JsonScheduleVersion) -> Result<Self, ScheduleVersionMismatch> {
        if version == 1 {
            Ok(Self)
        } else {
            Err(ScheduleVersionMismatch(version))
        }
    }
}

#[derive(Deserialize)]
#[serde(try_from = "JsonScheduleVersion")]
struct ScheduleVersion;

#[derive(Deserialize)]
pub(crate) struct Schedule {
    #[allow(unused)] // version check
    version: ScheduleVersion,
    pub(crate) matches: Vec<Match>,
}

fn deserialize_datetime<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
    // workaround for https://github.com/chronotope/chrono/issues/330
    Ok(NaiveDateTime::parse_from_str(&format!("{}:00", <&str>::deserialize(deserializer)?), "%Y-%m-%d %H:%M:%S").map_err(D::Error::custom)?.and_utc())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Match {
    #[serde(rename = "timeUTC", deserialize_with = "deserialize_datetime")]
    pub(crate) time_utc: DateTime<Utc>,
    pub(crate) player_a: User,
    pub(crate) player_b: User,
    pub(crate) id: i32,
    pub(crate) division: String,
    pub(crate) status: MatchStatus,
    pub(crate) restreamers: Vec<User>,
    pub(crate) restream_language: Option<Language>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct User {
    pub(crate) discord_id: Option<UserId>,
    /// Not deserialized as a URL since the League API may return strings that aren't valid URLs
    pub(crate) racetime_url: Option<String>,
    pub(crate) username: String,
    pub(crate) twitch_username: Option<String>,
}

impl User {
    pub(crate) async fn racetime_id(&self, http_client: &reqwest::Client) -> wheel::Result<Option<String>> {
        let url_part = self.racetime_url.as_deref().and_then(|url| {
            let (_, id) = regex_captures!("^https://racetime.gg/user/([0-9A-Za-z]+)(?:/.*)?$", url)?;
            Some(id.to_owned())
        });
        Ok(if let Some(url_part) = url_part {
            racetime_bot::user_data(http_client, &url_part).await?.map(|user_data| user_data.id)
        } else {
            None
        })
    }

    pub(crate) async fn into_entrant(self, http_client: &reqwest::Client) -> wheel::Result<Entrant> {
        Ok(if let Some(id) = self.discord_id {
            Entrant::Discord {
                racetime_id: self.racetime_id(http_client).await?,
                twitch_username: self.twitch_username,
                id,
            }
        } else {
            Entrant::Named {
                racetime_id: self.racetime_id(http_client).await?,
                name: self.username,
                twitch_username: self.twitch_username,
            }
        })
    }
}

#[derive(Deserialize)]
pub(crate) enum MatchStatus {
    Canceled,
    Confirmed,
}

#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn s8_settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("OoTR League S8"),
        format!("password_lock") => json!(true),
        format!("reachable_locations") => json!("beatable"),
        format!("bridge_medallions") => json!(5),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("medallions"),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("spawn_positions") => json!([
            "child",
            "adult",
        ]),
        format!("free_bombchu_drops") => json!(false),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("key_rings_choice") => json!("all"),
        format!("key_rings") => json!([
            "Thieves Hideout",
            "Treasure Chest Game",
            "Forest Temple",
            "Fire Temple",
            "Water Temple",
            "Shadow Temple",
            "Spirit Temple",
            "Bottom of the Well",
            "Gerudo Training Ground",
            "Ganons Castle",
        ]),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
            "Song from Impa",
        ]),
        format!("allowed_tricks") => json!([
            "logic_fewer_tunic_requirements",
            "logic_grottos_without_agony",
            "logic_child_deadhand",
            "logic_rusted_switches",
            "logic_forest_vines",
            "logic_lens_botw",
            "logic_lens_castle",
            "logic_lens_gtg",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_bongo",
            "logic_lens_spirit",
            "logic_man_on_roof",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_dc_jump",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "lens",
            "farores_wind",
            "zeldas_letter",
        ]),
        format!("starting_songs") => json!([
            "prelude",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("skip_reward_from_rauru") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("hint_dist") => json!("custom"),
        format!("hint_dist_user") => json!({
            "name":                  "league",
            "gui_name":              "League",
            "description":           "Hint Distribution for the S8 of League.",
            "add_locations":         [
                { "location": "Deku Theater Skull Mask", "types": ["always"] },
                { "location": "Sheik in Kakariko", "types": ["always"] },
            ],
            "remove_locations":      [
                { "location": "Sheik in Crater", "types": ["sometimes"] },
                { "location": "Song from Royal Familys Tomb", "types": ["sometimes"] },
                { "location": "Sheik in Forest", "types": ["sometimes"] },
                { "location": "Sheik at Temple", "types": ["sometimes"] },
                { "location": "Sheik at Colossus", "types": ["sometimes"] },
                { "location": "Sheik in Ice Cavern", "types": ["sometimes"] },
                { "location": "LH Sun", "types": ["sometimes"] },
                { "location": "HC Great Fairy Reward", "types": ["sometimes"] },
                { "location": "OGC Great Fairy Reward", "types": ["sometimes"] },
                { "location": "Kak 20 Gold Skulltula Reward", "types": ["sometimes"] },
                { "location": "GF HBA 1500 Points", "types": ["sometimes"] },
                { "location": "GC Maze Left Chest", "types": ["sometimes"] },
                { "location": "GV Chest", "types": ["sometimes"] },
                { "location": "Graveyard Royal Familys Tomb Chest", "types": ["sometimes"] },
                { "location": "GC Pot Freestanding PoH", "types": ["sometimes"] },
                { "location": "LH Lab Dive", "types": ["sometimes"] },
                { "location": "Water Temple Boss Key Chest", "types": ["sometimes"] },
                { "location": "Water Temple River Chest", "types": ["sometimes"] },
                { "location": "Gerudo Training Ground Maze Path Final Chest", "types": ["sometimes"] },
                { "location": "Spirit Temple Silver Gauntlets Chest", "types": ["sometimes"] },
                { "location": "Spirit Temple Mirror Shield Chest", "types": ["sometimes"] },
                { "location": "Shadow Temple Freestanding Key", "types": ["sometimes"] },
                { "location": "Ice Cavern Iron Boots Chest", "types": ["sometimes"] },
                { "location": "Ganons Castle Shadow Trial Golden Gauntlets Chest", "types": ["sometimes"] },
                { "location": "GV Pieces of Heart Ledges", "types": ["dual"] },
                { "location": "Graveyard Dampe Race Rewards", "types": ["dual"] },
                { "location": "Fire Temple Lower Loop", "types": ["dual"] },
                { "location": "ZR Frogs Rewards", "types": ["dual"] },
                { "location": "Deku Theater Rewards", "types": ["dual"] },
                { "location": "Bottom of the Well Dead Hand Room", "types": ["dual"] },
                { "location": "Spirit Temple Child Lower", "types": ["dual"] },
                { "location": "Spirit Temple Child Top", "types": ["dual"] },
                { "location": "Spirit Temple Adult Lower", "types": ["dual"] },
                { "location": "Shadow Temple Invisible Blades Chests", "types": ["dual"] },
                { "location": "Shadow Temple Spike Walls Room", "types": ["dual"] },
                { "location": "Ganons Castle Spirit Trial Chests", "types": ["dual"] },
                { "location": "Dodongos Cavern Upper Business Scrubs", "types": ["dual"] },
            ],
            "add_items":             [],
            "remove_items":          [
                { "item": "Minuet of Forest", "types": ["goal"] },
                { "item": "Bolero of Fire", "types": ["goal"] },
                { "item": "Serenade of Water", "types": ["goal"] },
                { "item": "Requiem of Spirit", "types": ["goal"] },
                { "item": "Nocturne of Shadow", "types": ["goal"] },
                { "item": "Prelude of Light", "types": ["goal"] },
                { "item": "Zeldas Lullaby", "types": ["goal"] },
                { "item": "Eponas Song", "types": ["goal"] },
                { "item": "Sarias Song", "types": ["goal"] },
                { "item": "Suns Song", "types": ["goal"] },
                { "item": "Song of Time", "types": ["goal"] },
                { "item": "Song of Storms", "types": ["goal"] },
                { "item": "Nocturne of Shadow", "types": ["goal"] },
            ],
            "disabled": [
                "HF (Cow Grotto)",
                "HC (Storms Grotto)",
            ],
            "dungeons_woth_limit":   2,
            "dungeons_barren_limit": 1,
            "named_items_required":  true,
            "vague_named_items":     false,
            "use_default_goals":     true,
            "distribution":          {
                "trial":           {"order": 1, "weight": 0.0, "fixed":  0, "copies": 2},
                "entrance_always": {"order": 2, "weight": 0.0, "fixed":  0, "copies": 2},
                "always":          {"order": 3, "weight": 0.0, "fixed":  0, "copies": 2},
                "goal":            {"order": 4, "weight": 0.0, "fixed":  5, "copies": 2},
                "important_check": {"order": 5, "weight": 0.0, "fixed":  3, "copies": 2},
                "dual":            {"order": 6, "weight": 0.0, "fixed":  2, "copies": 2},
                "sometimes":       {"order": 7, "weight": 0.0, "fixed": 40, "copies": 2},
                "junk":            {"order": 8, "weight": 1.0, "fixed":  0, "copies": 2},
            },
        }),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "30_skulltulas",
            "40_skulltulas",
            "50_skulltulas",
        ]),
        format!("correct_chest_appearances") => json!("both"),
        format!("correct_potcrate_appearances") => json!("textures_content"),
        format!("blue_fire_arrows") => json!(true),
        format!("tcg_requires_lens") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
    ]
}
