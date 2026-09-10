//! Compatibility oracles transcribed from main's Crosskeys generator and Cabookey OWR_CONFIG.
//! Expected settings deliberately do not come from the migrated configuration.
use super::*;

#[derive(Deserialize)]
struct GeneratedYaml {
    settings: HashMap<u8, serde_json::Value>,
    #[serde(default)]
    placements: HashMap<u8, serde_json::Value>,
    #[serde(default)]
    start_inventory: HashMap<u8, Vec<String>>,
    meta: serde_json::Value,
}

#[tokio::test]
#[ignore = "requires HTH_TEST_DATABASE_URL"]
async fn migrated_crosskeys_and_cabookey_match_main_for_every_seed_choice_combination() {
    let pool = event::configuration::test_pool().await;
    let mut cases = 0;
    for (series, event, keys) in [
        (
            "xkeys",
            "2025",
            vec![
                "all_dungeons",
                "completionist",
                "flute",
                "inverted",
                "keydrop",
                "mirror_scroll",
                "pseudoboots",
                "zw",
            ],
        ),
        (
            "xkeys",
            "2026",
            vec![
                "all_dungeons",
                "completionist",
                "flute",
                "inverted",
                "keydrop",
                "mirror_scroll",
                "pseudoboots",
                "zw",
            ],
        ),
        (
            "cabookey",
            "2026",
            vec!["keydrop", "100pct", "tileswap", "mirror_scroll", "enemizer"],
        ),
    ] {
        let value: serde_json::Value =
            sqlx::query_scalar("SELECT seed_config FROM events WHERE series = $1 AND event = $2")
                .bind(series)
                .bind(event)
                .fetch_one(&pool)
                .await
                .unwrap();
        let config: seed_gen_type::OwrEventConfig = serde_json::from_value(value).unwrap();
        for mask in 0..(1 << keys.len()) {
            let resolved: HashMap<String, bool> = keys
                .iter()
                .enumerate()
                .map(|(index, key)| ((*key).to_owned(), mask & (1 << index) != 0))
                .collect();
            let enabled = |key: &str| resolved.get(key).copied().unwrap_or(false);
            let mut placements = serde_json::Map::new();
            let mut inventory = Vec::<String>::new();
            let expected = if series == "xkeys" {
                if !enabled("zw") {
                    placements.insert(
                        "Skull Woods - Pinball Room".into(),
                        json!("Small Key (Skull Woods)"),
                    );
                }
                if enabled("flute") {
                    inventory.push("Ocarina (Activated)".into());
                }
                let mut settings = json!({
                    "accessibility":"locations", "bigkeyshuffle":true, "compassshuffle":true,
                    "crystals_ganon":"7", "crystals_gt":"7", "item_functionality":"normal",
                    "key_logic_algorithm":"partial", "keyshuffle":"wild", "linked_drops":"unset",
                    "mapshuffle":true, "shuffle":"crossed", "shuffletavern":0,
                    "dropshuffle": if enabled("keydrop") { "keys" } else { "none" },
                    "pottery": if enabled("keydrop") { "keys" } else { "none" },
                    "flute_mode": if enabled("flute") { "active" } else { "normal" },
                    "goal": if enabled("completionist") { "completionist" } else if enabled("all_dungeons") { "dungeons" } else { "crystals" },
                    "mode": if enabled("inverted") { "inverted" } else { "open" },
                    "mirrorscroll": u8::from(enabled("mirror_scroll")),
                    "pseudoboots": u8::from(enabled("pseudoboots")),
                    "skullwoods": if enabled("zw") { "followlinked" } else { "original" }
                });
                if enabled("all_dungeons") {
                    settings["aga_randomness"] = json!(false);
                }
                settings
            } else {
                inventory.push("Pegasus Boots".into());
                json!({
                    "aga_randomness":false, "accessibility":"locations", "bigkeyshuffle":"wild",
                    "compassshuffle":"wild", "crystals_ganon":"7", "crystals_gt":"7",
                    "flute_mode":"normal", "item_functionality":"normal", "key_logic_algorithm":"partial",
                    "keyshuffle":"wild", "linked_drops":"unset", "mapshuffle":"wild", "mode":"standard",
                    "money_balance":0, "pseudoboots":0, "shuffle":"vanilla", "shuffletavern":1,
                    "skullwoods":"original", "swords":"assured",
                    "dropshuffle": if enabled("keydrop") { "keys" } else { "none" },
                    "pottery": if enabled("keydrop") { "keys" } else { "none" },
                    "goal": if enabled("100pct") { "completionist" } else { "dungeons" },
                    "ow_mixed": u8::from(enabled("tileswap")),
                    "mirrorscroll": u8::from(enabled("mirror_scroll")),
                    "enemy_shuffle": if enabled("enemizer") { "shuffled" } else { "none" },
                    "boss_shuffle": if enabled("enemizer") { "random" } else { "none" }
                })
            };
            let yaml = build_dr_yaml_from_config(&config, &resolved, Uuid::nil()).unwrap();
            let actual: GeneratedYaml = serde_yml::from_str(&yaml).unwrap();
            assert_eq!(
                actual.settings[&1], expected,
                "{series}/{event}: {resolved:?}"
            );
            assert_eq!(
                actual.placements.get(&1).cloned().unwrap_or(json!({})),
                serde_json::Value::Object(placements),
                "{series}/{event}: {resolved:?}"
            );
            assert_eq!(
                actual.start_inventory.get(&1).cloned().unwrap_or_default(),
                inventory
            );
            assert_eq!(
                actual.meta,
                json!({"bps":true, "name":Uuid::nil().to_string(), "race":true, "skip_playthrough":true, "spoiler":"full", "suppress_rom":true})
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 544);
    eprintln!(
        "Verified {cases} migrated seed-choice combinations against main's settings, placements, inventory, and race metadata"
    );
}
