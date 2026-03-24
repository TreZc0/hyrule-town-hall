use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<RawHtml<String>, InfoError> {
    Ok(html! {
        article {
            p {
                : "Day ";
                : data.event;
                : " of the ";
                a(href = "https://docs.google.com/document/d/1xELThZtIctwN-vYtYhUqtd88JigNzabk8OZHANa0gqY/edit") : "9 Days of SAWS";
                : " event, organized by ";
                : English.join_html_opt(data.organizers(transaction).await?);
                : ", will be a ";
                a(href = "https://docs.google.com/document/d/1sbL6Zju943F5qyx4QbTLUsqZqOTMmvqKVbDwJl08SGc/edit") : "Standard Anti-Weekly Settings";
                @match &*data.event {
                    "1" | "9" => : " (S6)";
                    "2" | "6" | "7" => : " (Beginner)";
                    "3" => : " (Advanced)";
                    "4" => : " (S5) + one bonk KO";
                    "5" => : " (Beginner) + mixed pools";
                    "8" => : " (S6) + dungeon ER";
                    _ => @unimplemented
                }
                : " race";
                @match &*data.event {
                    "1" | "3" | "4" | "5" | "7" | "9" => {}
                    "2" | "8" => : " with 2-player co-op teams";
                    "6" => : " with 3-player multiworld teams";
                    _ => @unimplemented
                }
                : ".";
            }
            h2 : "Rules";
            p {
                : "Follow the ";
                a(href = "https://wiki.ootrandomizer.com/index.php?title=Rules#Universal_Rules") : "Universal Rules";
                : " and the ";
                a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "Standard";
                @if data.event == "6" {
                    : " ruleset, with a few exceptions listed below:";
                } else {
                    : " ruleset, with the exception that streaming is not required.";
                }
            }
            @if data.event == "6" {
                ul {
                    li : "Fire Arrow Entry is allowed";
                    li : "Streaming is not required";
                }
            }
            @if let TeamConfig::CoOp | TeamConfig::Multiworld = data.team_config {
                p {
                    : "Each team will be ranked by the average of the finish times of its members. Timing for an individual player ends on the first frame of the cutscene that plays upon killing Ganon. Players are allowed to kill Ganon to stop their timer and then reset their game, allowing them to continue ";
                    @match data.team_config {
                        TeamConfig::CoOp => : "exploring the seed";
                        TeamConfig::Multiworld => : "collecting items for their team";
                        _ => @unimplemented
                    }
                    : " if necessary.";
                }
            }
            h2 : "Settings";
            p {
                : "The seed will be rolled on ";
                a(href = "https://ootrandomizer.com/generatorDev?version=devFenhl_6.9.14") : "version 6.9.14 Fenhl-2";
                : " of the randomizer using the ";
                @match &*data.event {
                    "1" | "4" | "8" | "9" => : "Standard Anti-Weekly Settings (S6)";
                    "2" | "5" | "6" | "7" => : "Standard Anti-Weekly Settings (Beginner)";
                    "3" => : "Standard Anti-Weekly Settings (Advanced)";
                    _ => @unimplemented
                }
                @match &*data.event {
                    "1" | "2" | "3" | "6" | "7" | "9" => : " preset.";
                    "4" | "5" | "8" => : " preset, with the following changes:";
                    _ => @unimplemented
                }
            }
            @match &*data.event {
                "1" | "2" | "3" | "6" | "7" | "9" => {}
                "4" => ul {
                    li : "No dungeon boss shortcuts";
                    li : "Spawn shuffled for both ages";
                    li : "“Fix broken drops” off";
                    li : "Minimal item pool";
                    li : "Blue Fire Arrows off";
                    li : "No ice traps";
                    li : "One Bonk KO";
                    li : "Standard S5 Tournament hint distribution";
                }
                "5" => {
                    ul {
                        li : "All interior entrances shuffled";
                        li : "Grotto entrances shuffled";
                        li : "Dungeon entrances shuffled (including Ganon's Castle)";
                        li : "Overworld entrances shuffled";
                        li : "Mixed entrance pools (interiors, grottos, dungeons, and overworld)";
                        li : "Full spawn shuffle";
                        li : "Gerudo Valley exit to Lake Hylia shuffled (full)";
                        li : "Owl drops shuffled (full)";
                        li : "Warp song destinations shuffled (full)";
                        li : "Blue warps lead to the shuffled entrances of the dungeons they're in";
                    }
                    p : "“Full” one-ways can lead to additional entrances, such as dungeons, bosses, or grottos.";
                }
                "8" => ul {
                    li : "Dungeon entrances shuffled (except Ganon's Castle)";
                    li : "Blue warps lead to the shuffled entrances of the dungeons they're in";
                }
                _ => @unimplemented
            }
        }
    })
}

#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn beginner_preset() -> seed::Settings {
    collect![
        format!("user_message") => json!("Standard Anti-Weekly Settings (Beginner)"),
        format!("reachable_locations") => json!("beatable"),
        format!("bridge") => json!("open"),
        format!("shuffle_bosskeys") => json!("remove"),
        format!("shuffle_smallkeys") => json!("remove"),
        format!("shuffle_mapcompass") => json!("remove"),
        format!("open_forest") => json!(true),
        format!("require_gohma") => json!(false),
        format!("zora_fountain") => json!("open"),
        format!("gerudo_fortress") => json!("open"),
        format!("dungeon_shortcuts_choice") => json!("all"),
        format!("dungeon_shortcuts") => json!([
            "Deku Tree",
            "Dodongos Cavern",
            "Jabu Jabus Belly",
            "Forest Temple",
            "Fire Temple",
            "Water Temple",
            "Shadow Temple",
            "Spirit Temple",
        ]),
        format!("starting_age") => json!("random"),
        format!("shuffle_child_spawn") => json!("balanced"),
        format!("shuffle_adult_spawn") => json!("balanced"),
        format!("blue_warps") => json!("vanilla"),
        format!("exclusive_one_ways") => json!(true),
        format!("bombchus_in_logic") => json!(true),
        format!("shuffle_song_items") => json!("any"),
        format!("shopsanity") => json!("4"),
        format!("tokensanity") => json!("all"),
        format!("shuffle_scrubs") => json!("low"),
        format!("shuffle_cows") => json!(true),
        format!("shuffle_ocarinas") => json!(true),
        format!("shuffle_gerudo_card") => json!(true),
        format!("shuffle_beans") => json!(true),
        format!("shuffle_medigoron_carpet_salesman") => json!(true),
        format!("shuffle_frog_song_rupees") => json!(true),
        format!("disabled_locations") => json!([
            "Song from Impa",
            "Song from Malon",
            "Song from Saria",
            "Song from Royal Familys Tomb",
            "Song from Ocarina of Time",
            "Song from Windmill",
            "Sheik in Forest",
            "Sheik in Crater",
            "Sheik in Ice Cavern",
            "Sheik at Colossus",
            "Sheik in Kakariko",
            "Sheik at Temple",
            "KF Midos Top Left Chest",
            "KF Midos Top Right Chest",
            "KF Midos Bottom Left Chest",
            "KF Midos Bottom Right Chest",
            "KF Kokiri Sword Chest",
            "KF Storms Grotto Chest",
            "LW Ocarina Memory Game",
            "LW Target in Woods",
            "LW Near Shortcuts Grotto Chest",
            "Deku Theater Skull Mask",
            "LW Skull Kid",
            "LW Deku Scrub Near Bridge",
            "LW Deku Scrub Grotto Front",
            "SFM Wolfos Grotto Chest",
            "HF Near Market Grotto Chest",
            "HF Tektite Grotto Freestanding PoH",
            "HF Southeast Grotto Chest",
            "HF Open Grotto Chest",
            "HF Deku Scrub Grotto",
            "Market Shooting Gallery Reward",
            "Market Bombchu Bowling First Prize",
            "Market Bombchu Bowling Second Prize",
            "Market Lost Dog",
            "Market Treasure Chest Game Reward",
            "Market 10 Big Poes",
            "ToT Light Arrows Cutscene",
            "HC Great Fairy Reward",
            "LLR Talons Chickens",
            "LLR Freestanding PoH",
            "Kak Anju as Child",
            "Kak Anju as Adult",
            "Kak Impas House Freestanding PoH",
            "Kak Windmill Freestanding PoH",
            "Kak Man on Roof",
            "Kak Open Grotto Chest",
            "Kak Redead Grotto Chest",
            "Kak Shooting Gallery Reward",
            "Kak 10 Gold Skulltula Reward",
            "Kak 20 Gold Skulltula Reward",
            "Kak 30 Gold Skulltula Reward",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
            "Graveyard Shield Grave Chest",
            "Graveyard Heart Piece Grave Chest",
            "Graveyard Royal Familys Tomb Chest",
            "Graveyard Freestanding PoH",
            "Graveyard Dampe Gravedigging Tour",
            "Graveyard Dampe Race Hookshot Chest",
            "Graveyard Dampe Race Freestanding PoH",
            "DMT Freestanding PoH",
            "DMT Chest",
            "DMT Storms Grotto Chest",
            "DMT Great Fairy Reward",
            "DMT Biggoron",
            "GC Darunias Joy",
            "GC Pot Freestanding PoH",
            "GC Rolling Goron as Child",
            "GC Rolling Goron as Adult",
            "GC Maze Left Chest",
            "GC Maze Right Chest",
            "GC Maze Center Chest",
            "DMC Volcano Freestanding PoH",
            "DMC Wall Freestanding PoH",
            "DMC Upper Grotto Chest",
            "DMC Great Fairy Reward",
            "ZR Open Grotto Chest",
            "ZR Frogs in the Rain",
            "ZR Frogs Ocarina Game",
            "ZR Near Open Grotto Freestanding PoH",
            "ZR Near Domain Freestanding PoH",
            "ZD Diving Minigame",
            "ZD Chest",
            "ZD King Zora Thawed",
            "ZF Great Fairy Reward",
            "ZF Iceberg Freestanding PoH",
            "ZF Bottom Freestanding PoH",
            "LH Underwater Item",
            "LH Child Fishing",
            "LH Adult Fishing",
            "LH Lab Dive",
            "LH Freestanding PoH",
            "LH Sun",
            "GV Crate Freestanding PoH",
            "GV Waterfall Freestanding PoH",
            "GV Chest",
            "GF Chest",
            "GF HBA 1000 Points",
            "GF HBA 1500 Points",
            "Wasteland Chest",
            "Colossus Great Fairy Reward",
            "Colossus Freestanding PoH",
            "OGC Great Fairy Reward",
            "Deku Tree Map Chest",
            "Deku Tree Slingshot Room Side Chest",
            "Deku Tree Slingshot Chest",
            "Deku Tree Compass Chest",
            "Deku Tree Compass Room Side Chest",
            "Deku Tree Basement Chest",
            "Deku Tree Queen Gohma Heart",
            "Dodongos Cavern Map Chest",
            "Dodongos Cavern Compass Chest",
            "Dodongos Cavern Bomb Flower Platform Chest",
            "Dodongos Cavern Bomb Bag Chest",
            "Dodongos Cavern End of Bridge Chest",
            "Dodongos Cavern Boss Room Chest",
            "Dodongos Cavern King Dodongo Heart",
            "Jabu Jabus Belly Boomerang Chest",
            "Jabu Jabus Belly Map Chest",
            "Jabu Jabus Belly Compass Chest",
            "Jabu Jabus Belly Barinade Heart",
            "Bottom of the Well Front Left Fake Wall Chest",
            "Bottom of the Well Front Center Bombable Chest",
            "Bottom of the Well Back Left Bombable Chest",
            "Bottom of the Well Underwater Left Chest",
            "Bottom of the Well Freestanding Key",
            "Bottom of the Well Compass Chest",
            "Bottom of the Well Center Skulltula Chest",
            "Bottom of the Well Right Bottom Fake Wall Chest",
            "Bottom of the Well Fire Keese Chest",
            "Bottom of the Well Like Like Chest",
            "Bottom of the Well Map Chest",
            "Bottom of the Well Underwater Front Chest",
            "Bottom of the Well Invisible Chest",
            "Bottom of the Well Lens of Truth Chest",
            "Forest Temple First Room Chest",
            "Forest Temple First Stalfos Chest",
            "Forest Temple Raised Island Courtyard Chest",
            "Forest Temple Map Chest",
            "Forest Temple Well Chest",
            "Forest Temple Eye Switch Chest",
            "Forest Temple Boss Key Chest",
            "Forest Temple Floormaster Chest",
            "Forest Temple Red Poe Chest",
            "Forest Temple Bow Chest",
            "Forest Temple Blue Poe Chest",
            "Forest Temple Falling Ceiling Room Chest",
            "Forest Temple Basement Chest",
            "Forest Temple Phantom Ganon Heart",
            "Fire Temple Near Boss Chest",
            "Fire Temple Flare Dancer Chest",
            "Fire Temple Boss Key Chest",
            "Fire Temple Big Lava Room Lower Open Door Chest",
            "Fire Temple Big Lava Room Blocked Door Chest",
            "Fire Temple Boulder Maze Lower Chest",
            "Fire Temple Boulder Maze Side Room Chest",
            "Fire Temple Map Chest",
            "Fire Temple Boulder Maze Shortcut Chest",
            "Fire Temple Boulder Maze Upper Chest",
            "Fire Temple Scarecrow Chest",
            "Fire Temple Compass Chest",
            "Fire Temple Megaton Hammer Chest",
            "Fire Temple Highest Goron Chest",
            "Fire Temple Volvagia Heart",
            "Water Temple Compass Chest",
            "Water Temple Map Chest",
            "Water Temple Cracked Wall Chest",
            "Water Temple Torches Chest",
            "Water Temple Boss Key Chest",
            "Water Temple Central Pillar Chest",
            "Water Temple Central Bow Target Chest",
            "Water Temple Longshot Chest",
            "Water Temple River Chest",
            "Water Temple Dragon Chest",
            "Water Temple Morpha Heart",
            "Shadow Temple Map Chest",
            "Shadow Temple Hover Boots Chest",
            "Shadow Temple Compass Chest",
            "Shadow Temple Early Silver Rupee Chest",
            "Shadow Temple Invisible Blades Visible Chest",
            "Shadow Temple Invisible Blades Invisible Chest",
            "Shadow Temple Falling Spikes Lower Chest",
            "Shadow Temple Falling Spikes Upper Chest",
            "Shadow Temple Falling Spikes Switch Chest",
            "Shadow Temple Invisible Spikes Chest",
            "Shadow Temple Freestanding Key",
            "Shadow Temple Wind Hint Chest",
            "Shadow Temple After Wind Enemy Chest",
            "Shadow Temple After Wind Hidden Chest",
            "Shadow Temple Spike Walls Left Chest",
            "Shadow Temple Boss Key Chest",
            "Shadow Temple Invisible Floormaster Chest",
            "Shadow Temple Bongo Bongo Heart",
            "Spirit Temple Child Bridge Chest",
            "Spirit Temple Child Early Torches Chest",
            "Spirit Temple Child Climb North Chest",
            "Spirit Temple Child Climb East Chest",
            "Spirit Temple Map Chest",
            "Spirit Temple Sun Block Room Chest",
            "Spirit Temple Silver Gauntlets Chest",
            "Spirit Temple Compass Chest",
            "Spirit Temple Early Adult Right Chest",
            "Spirit Temple First Mirror Left Chest",
            "Spirit Temple First Mirror Right Chest",
            "Spirit Temple Statue Room Northeast Chest",
            "Spirit Temple Statue Room Hand Chest",
            "Spirit Temple Near Four Armos Chest",
            "Spirit Temple Hallway Right Invisible Chest",
            "Spirit Temple Hallway Left Invisible Chest",
            "Spirit Temple Mirror Shield Chest",
            "Spirit Temple Boss Key Chest",
            "Spirit Temple Topmost Chest",
            "Spirit Temple Twinrova Heart",
            "Ice Cavern Map Chest",
            "Ice Cavern Compass Chest",
            "Ice Cavern Freestanding PoH",
            "Ice Cavern Iron Boots Chest",
            "Gerudo Training Ground Lobby Left Chest",
            "Gerudo Training Ground Lobby Right Chest",
            "Gerudo Training Ground Stalfos Chest",
            "Gerudo Training Ground Before Heavy Block Chest",
            "Gerudo Training Ground Heavy Block First Chest",
            "Gerudo Training Ground Heavy Block Second Chest",
            "Gerudo Training Ground Heavy Block Third Chest",
            "Gerudo Training Ground Heavy Block Fourth Chest",
            "Gerudo Training Ground Eye Statue Chest",
            "Gerudo Training Ground Near Scarecrow Chest",
            "Gerudo Training Ground Hammer Room Clear Chest",
            "Gerudo Training Ground Hammer Room Switch Chest",
            "Gerudo Training Ground Freestanding Key",
            "Gerudo Training Ground Maze Right Central Chest",
            "Gerudo Training Ground Maze Right Side Chest",
            "Gerudo Training Ground Underwater Silver Rupee Chest",
            "Gerudo Training Ground Beamos Chest",
            "Gerudo Training Ground Hidden Ceiling Chest",
            "Gerudo Training Ground Maze Path First Chest",
            "Gerudo Training Ground Maze Path Second Chest",
            "Gerudo Training Ground Maze Path Third Chest",
            "Gerudo Training Ground Maze Path Final Chest",
            "Ganons Castle Forest Trial Chest",
            "Ganons Castle Water Trial Left Chest",
            "Ganons Castle Water Trial Right Chest",
            "Ganons Castle Shadow Trial Front Chest",
            "Ganons Castle Shadow Trial Golden Gauntlets Chest",
            "Ganons Castle Light Trial First Left Chest",
            "Ganons Castle Light Trial Second Left Chest",
            "Ganons Castle Light Trial Third Left Chest",
            "Ganons Castle Light Trial First Right Chest",
            "Ganons Castle Light Trial Second Right Chest",
            "Ganons Castle Light Trial Third Right Chest",
            "Ganons Castle Light Trial Invisible Enemies Chest",
            "Ganons Castle Light Trial Lullaby Chest",
            "Ganons Castle Spirit Trial Crystal Switch Chest",
            "Ganons Castle Spirit Trial Invisible Chest",
            "Ganons Tower Boss Key Chest",
        ]),
        format!("allowed_tricks") => json!([
            "logic_fewer_tunic_requirements",
            "logic_grottos_without_agony",
            "logic_child_deadhand",
            "logic_man_on_roof",
            "logic_dc_jump",
            "logic_rusted_switches",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_forest_vines",
            "logic_lens_botw",
            "logic_lens_castle",
            "logic_lens_gtg",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_spirit",
        ]),
        format!("starting_equipment") => json!([
            "kokiri_sword",
            "wallet",
        ]),
        format!("starting_items") => json!([
            "lens",
        ]),
        format!("start_with_rupees") => json!(true),
        format!("complete_mask_quest") => json!(true),
        format!("useful_cutscenes") => json!(true),
        format!("fast_chests") => json!(false),
        format!("free_scarecrow") => json!(true),
        format!("chicken_count_random") => json!(true),
        format!("big_poe_count_random") => json!(true),
        format!("clearer_hints") => json!(false),
        format!("hint_dist") => json!("weekly"),
        format!("starting_tod") => json!("sunset"),
        format!("blue_fire_arrows") => json!(true),
        format!("fix_broken_drops") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Claim Check",
        ]),
    ]
}

#[cfg_attr(not(unix), allow(dead_code))]

pub(crate) fn advanced_preset() -> seed::Settings {
    let mut settings = beginner_preset();
    settings.insert(format!("user_message"), json!("Standard Anti-Weekly Settings (Advanced)"));
    settings.insert(format!("shuffle_silver_rupees"), json!("anywhere"));
    settings.insert(format!("shuffle_freestanding_items"), json!("all"));
    settings.insert(format!("shuffle_pots"), json!("all"));
    settings.insert(format!("shuffle_crates"), json!("all"));
    settings.insert(format!("shuffle_beehives"), json!(true));
    settings.insert(format!("correct_potcrate_appearances"), json!("textures_content"));
    settings.insert(format!("item_pool_value"), json!("minimal"));
    settings.insert(format!("junk_ice_traps"), json!("on"));
    #[cfg_attr(not(unix), allow(dead_code))]
    settings
}

pub(crate) fn s6_preset() -> seed::Settings {
    let mut settings = beginner_preset();
    settings.insert(format!("user_message"), json!("Standard Anti-Weekly Settings (S6)"));
    settings.insert(format!("open_deku"), json!(true));
    settings.insert(format!("shuffle_child_spawn"), json!("off"));
    settings.insert(format!("hint_dist"), json!("tournament"));
    settings
}
