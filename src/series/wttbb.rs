use {
    collect_mac::collect,
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
    },
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "1" => Some(html! {
            article {
                p(lang = "en") {
                    : "This is a friendly invitational tournament organised by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". The tournament is mainly aimed at players with an intermediate level. It allows players to play against each other in a friendly and fun environment and get their first taste of restreaming.";
                }
                p(lang = "fr") {
                    : "Voici la 1ère saison du tournoi. Rejoignez ";
                    a(href = "https://discord.gg/YKvbQSBT5") : "le serveur Discord";
                    : " pour plus de détails.";
                }
                p(lang = "fr") {
                    : "Voir le ";
                    a(href = "https://docs.google.com/document/d/1qXnZTj-2voLKHB0D8Yv9_les7GRInoOwvMW6qMcJkwk/edit") : "règlement du tournoi";
                }
            }
        }),
        "2" => Some(html! {
            article {
                p(lang = "en") {
                    : "WeTryToBeBetter is a friendly invitational tournament organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". The tournament is entering its 2nd season after a successful first edition. It's aimed at players with an intermediate level and give them a chance to compete in a friendly, fun environment and get a taste about restreaming.";
                }
                p(lang = "fr") {
                    : "WeTryToBeBetter est un tournoi amical organisé par ";
                    : French.join_html_opt(data.organizers(transaction).await?);
                    : ". Le tournoi entame sa 2e saison après une première édition réussie. Il s'adresse principalement aux joueurs de niveau intermédiaire et leur donne l'opportunité de s'affronter dans un environnement fun et décontracté et de s'initier au restreaming.";
                }
                p(lang = "fr") {
                    : "Rejoignez ";
                    a(href = "https://discord.gg/ZmNKqrvfcR") : "le serveur Discord";
                    : " pour plus de détails";
                }
                p(lang = "fr") {
                    : "Voir le ";
                    a(href = "https://docs.google.com/document/d/1B6KWh_VK2udpkLOARNVOP4-CTULzX7Z46CiKbG2hcQU/edit") : "règlement du tournoi";
                }
            }
        }),
        _ => None,
    })
}

#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn s1_settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("WeTryToBeBetter"),
        format!("bridge") => json!("dungeons"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("remove"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("empty_dungeons_mode") => json!("count"),
        format!("empty_dungeons_count") => json!(3),
        format!("spawn_positions") => json!([
            "child",
            "adult",
        ]),
        format!("free_bombchu_drops") => json!(false),
        format!("disabled_locations") => json!([
            "Sheik in Ice Cavern",
            "Deku Theater Mask of Truth",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
            "GF HBA 1500 Points",
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
            "logic_lens_spirit",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_songs") => json!([
            "prelude",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "lens",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("starting_hearts") => json!(4),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("correct_chest_appearances") => json!("textures"),
        format!("hint_dist") => json!("scrubs"),
        format!("misc_hints") => json!([
            "ganondorf",
            "20_skulltulas",
            "30_skulltulas",
            "warp_songs_and_owls",
        ]),
        format!("blue_fire_arrows") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Claim Check",
        ]),
    ]
}

#[cfg_attr(not(unix), allow(dead_code))]

pub(crate) fn s2_settings() -> seed::Settings {
    collect![
        format!("password_lock") => json!(true),
        format!("user_message") => json!("WeTryToBeBetter Season 2"),
        format!("bridge") => json!("dungeons"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("remove"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("empty_dungeons_mode") => json!("count"),
        format!("empty_dungeons_count") => json!(3),
        format!("shuffle_dungeon_entrances") => json!("simple"),
        format!("spawn_positions") => json!([
            "child",
            "adult",
        ]),
        format!("free_bombchu_drops") => json!(false),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
            "GF HBA 1500 Points",
        ]),
        format!("allowed_tricks") => json!([
            "logic_grottos_without_agony",
            "logic_fewer_tunic_requirements",
            "logic_rusted_switches",
            "logic_man_on_roof",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_dc_jump",
            "logic_lens_botw",
            "logic_child_deadhand",
            "logic_forest_vines",
            "logic_lens_shadow",
            "logic_lens_spirit",
            "logic_lens_gtg",
            "logic_lens_castle",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "farores_wind",
            "lens",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("starting_hearts") => json!(4),
        format!("skip_reward_from_rauru") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("correct_chest_appearances") => json!("both"),
        format!("hint_dist") => json!("scrubs"),
        format!("misc_hints") => json!([
            "ganondorf",
            "warp_songs_and_owls",
            "20_skulltulas",
            "30_skulltulas",
        ]),
        format!("blue_fire_arrows") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Claim Check",
        ]),
    ]
}
