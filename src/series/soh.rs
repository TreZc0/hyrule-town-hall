use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "1" => Some(html! {
            article {
                p {
                    : "This is the first charity beginner tournament for the ";
                    a(href = "https://autismsociety.org/") : "Autism of Society of America";
                    : ", organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://github.com/Queenhelena/Zootr-Charity") : "Tournament format, rules, and settings";
                    }
                }
            }
        }),
        _ => None,
    })
}

#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("Songs of Hope Charity Tournament"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("on_lacs"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("disabled_locations") => json!([
            "Deku Theater Skull Mask",
            "Deku Theater Mask of Truth",
            "Kak 30 Gold Skulltula Reward",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
            "DMC Deku Scrub",
            "ZR Frogs Ocarina Game",
            "GF HBA 1000 Points",
            "GF HBA 1500 Points",
        ]),
        format!("allowed_tricks") => json!([
            "logic_grottos_without_agony",
            "logic_man_on_roof",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("complete_mask_quest") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("auto_equip_masks") => json!(true),
        format!("plant_beans") => json!(true),
        format!("chicken_count") => json!(0),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("correct_chest_appearances") => json!("both"),
        format!("hint_dist") => json!("tournament"),
        format!("misc_hints") => json!([
            "altar",
            "dampe_diary",
            "ganondorf",
            "warp_songs_and_owls",
            "10_skulltulas",
            "20_skulltulas",
        ]),
        format!("blue_fire_arrows") => json!(true),
    ]
}
