use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
    racetime_bot::seed_gen_type::OwrEventConfig,
};

pub(crate) fn owr_config() -> OwrEventConfig {
    OwrEventConfig {
        base_settings: serde_json::json!({
            "accessibility": "locations",
            "aga_randomness": 0,
            "bigkeyshuffle": "wild",
            "compassshuffle": "wild",
            "crystals_ganon": "7",
            "crystals_gt": "7",
            "dropshuffle": "none",
            "flute_mode": "normal",
            "goal": "dungeons",
            "item_functionality": "normal",
            "key_logic_algorithm": "partial",
            "keyshuffle": "wild",
            "linked_drops": "unset",
            "mapshuffle": "wild",
            "mirrorscroll": 0,
            "mode": "standard",
            "ow_mixed": 0,
            "pottery": "none",
            "pseudoboots": 0,
            "shuffle": "vanilla",
            "shuffletavern": 1,
            "skullwoods": "original",
            "swords": "assured"
        }),
        start_inventory: vec!["Pegasus Boots".to_owned()],
        choice_patches: serde_json::json!({
            "keydrop": { "dropshuffle": "keys", "pottery": "keys" },
            "100pct":  { "goal": "completionist" },
            "tileswap": { "ow_mixed": 1 }
        }),
    }
}

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
