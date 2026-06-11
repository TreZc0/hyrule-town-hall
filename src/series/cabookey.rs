use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
    racetime_bot::{
        AlttprDoorRandoSetting,
        DungeonShuffleVal,
        OwrChoicePatch,
        OwrSeedConfig,
    },
};

pub(crate) static OWR_CONFIG: OwrSeedConfig = OwrSeedConfig {
    base_settings: AlttprDoorRandoSetting {
        aga_randomness: Some(false),
        accessibility: "locations",
        bigkeyshuffle: DungeonShuffleVal::Named("wild"),
        boss_shuffle: Some("none"),
        compassshuffle: DungeonShuffleVal::Named("wild"),
        crystals_ganon: "7",
        crystals_gt: "7",
        dropshuffle: "none",
        enemy_shuffle: Some("none"),
        flute_mode: "normal",
        goal: "dungeons",
        item_functionality: "normal",
        key_logic_algorithm: "partial",
        keyshuffle: "wild",
        linked_drops: "unset",
        mapshuffle: DungeonShuffleVal::Named("wild"),
        mirrorscroll: 0,
        mode: "standard",
        ow_mixed: Some(0),
        pottery: "none",
        pseudoboots: 0,
        shuffle: "vanilla",
        shuffletavern: 1,
        shuffle_followers: None,
        skullwoods: "original",
        swords: Some("assured"),
    },
    start_inventory: &["Pegasus Boots"],
    choice_patches: &[
        OwrChoicePatch {
            key: "keydrop",
            apply: |s| { s.dropshuffle = "keys"; s.pottery = "keys"; },
        },
        OwrChoicePatch {
            key: "100pct",
            apply: |s| { s.goal = "completionist"; },
        },
        OwrChoicePatch {
            key: "tileswap",
            apply: |s| { s.ow_mixed = Some(1); },
        },
        OwrChoicePatch {
            key: "mirror_scroll",
            apply: |s| { s.mirrorscroll = 1; },
        },
        OwrChoicePatch {
            key: "enemizer",
            apply: |s| { s.enemy_shuffle = Some("shuffled"); s.boss_shuffle = Some("random"); },
        },
    ],
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "2026" => Some(html! {
            article {
                p {
                    : "Welcome to Cabookey 2026! The tournament is organised by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
            }
        }),
        _ => None,
    })
}
