use {
    racetime::model::*,
    serde_json::Value as Json,
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
    },
};

pub(crate) fn piece_count(team_config: TeamConfig) -> u8 {
    3 * team_config.roles().len() as u8
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Score {
    pub(crate) team_config: TeamConfig,
    pub(crate) pieces: u8,
    pub(crate) last_collection_time: Duration,
}

impl Score {
    pub(crate) fn dnf(team_config: TeamConfig) -> Self {
        Self {
            pieces: 0,
            last_collection_time: Duration::default(),
            team_config,
        }
    }
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pieces == 0 {
            write!(f, "0/{}", piece_count(self.team_config))
        } else {
            write!(f, "{}/{} in {}", self.pieces, piece_count(self.team_config), English.format_duration(self.last_collection_time, false))
        }
    }
}

pub(crate) fn report_score_button(team_config: TeamConfig, finish_time: Option<Duration>) -> (&'static str, ActionButton) {
    ("Report score", ActionButton::Message {
        message: format!("!score ${{pieces}} ${{last_collection_time}}"),
        help_text: Some(format!("Report your Triforce Blitz score for this race.")),
        survey: Some(vec![
            SurveyQuestion {
                name: format!("pieces"),
                label: format!("Pieces found"),
                default: Some(json!(if let Some(finish_time) = finish_time {
                    if finish_time < Duration::from_secs(2 * 60 * 60) {
                        Cow::Owned(piece_count(team_config).to_string())
                    } else {
                        "1".into()
                    }
                } else {
                    "0".into()
                })),
                help_text: None,
                kind: SurveyQuestionKind::Radio,
                placeholder: None,
                options: (0..=piece_count(team_config)).map(|n| (n.to_string(), n.to_string())).collect(),
            },
            SurveyQuestion {
                name: format!("last_collection_time"),
                label: format!("Most recent collection time"),
                default: finish_time.map(|finish_time| json!(unparse_duration(finish_time))),
                help_text: Some(format!("Leave blank if you didn't collect any pieces.")),
                kind: SurveyQuestionKind::Input,
                placeholder: Some(format!("e.g. 1h23m45s")),
                options: Vec::default(),
            },
        ]),
        submit: Some(format!("Submit")),
    })
}

pub(crate) fn parse_seed_url(seed: &Url) -> Option<(bool, Uuid)> {
    if_chain! {
        if let Some(is_dev) = match seed.host_str() {
            Some("triforceblitz.com" | "www.triforceblitz.com") => Some(false),
            Some("dev.triforceblitz.com") => Some(true),
            _ => None,
        };
        if let Some(mut path_segments) = seed.path_segments();
        if path_segments.next() == Some(if is_dev { "seeds" } else { "seed" });
        if let Some(segment) = path_segments.next();
        if let Ok(uuid) = Uuid::parse_str(segment);
        if path_segments.next().is_none();
        then {
            Some((is_dev, uuid))
        } else {
            None
        }
    }
}

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "2" => Some(html! {
            article {
                p {
                    : "This is the 2nd season of the Triforce Blitz tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1p8HAwWsjsLW7tjfDl2SK-yQ35pVqbAS9GB72bkOIDFI/edit") : "the official document";
                    : " for details.";
                }
            }
        }),
        "3" => Some(html! {
            article {
                p {
                    : "This is the 3rd season of the Triforce Blitz tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1p8HAwWsjsLW7tjfDl2SK-yQ35pVqbAS9GB72bkOIDFI/edit") : "the official document";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://challonge.com/sugcp0b") : "Group brackets (not adjusted for cross-group tiebreakers)";
                    }
                }
            }
        }),
        _ => None,
    })
}

pub(crate) fn qualifier_async_rules() -> RawHtml<String> {
    html! {
        p : "Rules:";
        ol {
            li : "You must start the seed within 15 minutes of obtaining it and submit your time within 10 minutes of finishing. Any additional time taken will be added to your final time. If technical difficulties arise with obtaining the seed/submitting your time, please DM one of the Triforce Blitz Tournament Organizers to get it sorted out. (Discord role “Triforce Blitz Organisation” for pings)";
            li : "If you obtain a seed but do not submit a finish time before submissions close, it will count as a forfeit.";
            li {
                : "Requesting the seed for async will make you ";
                strong : "ineligible";
                : " to participate in the respective live qualifier.";
            }
            li {
                : "To avoid accidental spoilers, the qualifier async ";
                strong : "CANNOT";
                : " be streamed. You must local record and upload to YouTube as an unlisted video.";
            }
            li {
                : "This should be run like an actual race. In the event of a technical issue, you are allowed to invoke the ";
                a(href = "https://docs.google.com/document/d/1BbvHJF8vtyrte76jpoCVQBTy9MYStpN3vr2PLdiCIMk/edit") : "Fair Play Agreement";
                : " and have up to a 15 minute time where you can try to catch back up. If you do this, you must fill out the appropriate field when submitting your time so it can be authenticated.";
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, FromFormField, Sequence)]
pub(crate) enum CoOpRole {
    #[field(value = "sheikah")]
    Sheikah,
    #[field(value = "gerudo")]
    Gerudo,
}

impl fmt::Display for CoOpRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sheikah => write!(f, "player 1"),
            Self::Gerudo => write!(f, "player 2"),
        }
    }
}

impl ToHtml for CoOpRole {
    fn to_html(&self) -> RawHtml<String> {
        match self {
            Self::Sheikah => html! {
                span(class = "sheikah") : "player 1";
            },
            Self::Gerudo => html! {
                span(class = "gerudo") : "player 2";
            },
        }
    }
}

impl TryFrom<event::Role> for CoOpRole {
    type Error = ();

    fn try_from(role: event::Role) -> Result<Self, ()> {
        match role {
            event::Role::Sheikah => Ok(Self::Sheikah),
            event::Role::Gerudo => Ok(Self::Gerudo),
            _ => Err(()),
        }
    }
}

impl From<CoOpRole> for event::Role {
    fn from(role: CoOpRole) -> Self {
        match role {
            CoOpRole::Sheikah => Self::Sheikah,
            CoOpRole::Gerudo => Self::Gerudo,
        }
    }
}

pub(crate) fn progression_spoiler_settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("Triforce Blitz Progression Spoiler"),
        format!("bridge") => json!("dungeons"),
        format!("bridge_rewards") => json!(2),
        format!("trials") => json!(0),
        format!("triforce_blitz") => json!(true),
        format!("triforce_blitz_minimum_path_count") => json!(12),
        format!("triforce_blitz_maximum_empty_paths") => json!(1),
        format!("triforce_blitz_hint_shop") => json!(true),
        format!("shuffle_ganon_bosskey") => json!("remove"),
        format!("shuffle_bosskeys") => json!("remove"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("dungeon_shortcuts_choice") => json!("choice"),
        format!("dungeon_shortcuts") => json!([
            "Jabu Jabus Belly",
            "Forest Temple",
        ]),
        format!("starting_age") => json!("adult"),
        format!("free_bombchu_drops") => json!(false),
        format!("shopsanity") => json!("0"),
        format!("disabled_locations") => json!([
            "Deku Theater Skull Mask",
            "Deku Theater Mask of Truth",
            "Kak 30 Gold Skulltula Reward",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
            "ZR Frogs Ocarina Game",
            "Jabu Jabus Belly Boomerang Chest",
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
            "logic_lens_bongo",
            "logic_lens_spirit",
            "logic_water_gold_scale_no_entry",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "farores_wind",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("lock_reverse_shadow") => json!(true),
        format!("correct_chest_appearances") => json!("both"),
        format!("minor_items_special_texture") => json!([
            "bombchus",
            "hearts",
        ]),
        format!("hint_dist") => json!("triforce-blitz-s3"),
        format!("plandomized_locations") => json!({
            "Shadow Temple Invisible Floormaster Chest": "Small Key (Shadow Temple)",
            "KF Shop Item 1": {
                "item": "Buy Deku Shield",
                "price": 40,
            },
            "KF Shop Item 2": {
                "item": "Buy Deku Nut (5)",
                "price": 15,
            },
            "KF Shop Item 3": {
                "item": "Buy Deku Nut (10)",
                "price": 30,
            },
            "KF Shop Item 4": {
                "item": "Buy Deku Stick (1)",
                "price": 10,
            },
            "KF Shop Item 5": {
                "item": "Buy Deku Seeds (30)",
                "price": 30,
            },
            "KF Shop Item 6": {
                "item": "Buy Arrows (30)",
                "price": 60,
            },
            "KF Shop Item 7": {
                "item": "Buy Arrows (10)",
                "price": 20,
            },
            "KF Shop Item 8": {
                "item": "Buy Heart",
                "price": 10,
            },
            "Market Bazaar Item 1": {
                "item": "Buy Hylian Shield",
                "price": 80,
            },
            "Market Bazaar Item 2": {
                "item": "Buy Bombs (5) for 35 Rupees",
                "price": 35,
            },
            "Market Bazaar Item 3": {
                "item": "Buy Deku Nut (5)",
                "price": 15,
            },
            "Market Bazaar Item 4": {
                "item": "Buy Deku Stick (1)",
                "price": 10,
            },
            "Market Bazaar Item 5": "Bow Hint",
            "Market Bazaar Item 6": "Silver Scale Hint",
            "Market Bazaar Item 7": "Bomb Bag Hint",
            "Market Bazaar Item 8": "Wallet Hint",
            "Market Potion Shop Item 1": {
                "item": "Buy Green Potion",
                "price": 30,
            },
            "Market Potion Shop Item 2": {
                "item": "Buy Blue Fire",
                "price": 300,
            },
            "Market Potion Shop Item 3": {
                "item": "Buy Red Potion for 30 Rupees",
                "price": 30,
            },
            "Market Potion Shop Item 4": {
                "item": "Buy Fairy's Spirit",
                "price": 50,
            },
            "Market Potion Shop Item 5": "Goron Bracelet Hint",
            "Market Potion Shop Item 6": "Magic Hint",
            "Market Potion Shop Item 7": "Silver Gauntlets Hint",
            "Market Potion Shop Item 8": "Hookshot Hint",
            "Market Bombchu Shop Item 1": {
                "item": "Buy Bombchu (5)",
                "price": 60,
            },
            "Market Bombchu Shop Item 2": {
                "item": "Buy Bombchu (10)",
                "price": 99,
            },
            "Market Bombchu Shop Item 3": {
                "item": "Buy Bombchu (10)",
                "price": 99,
            },
            "Market Bombchu Shop Item 4": {
                "item": "Buy Bombchu (10)",
                "price": 99,
            },
            "Market Bombchu Shop Item 5": {
                "item": "Buy Bombchu (20)",
                "price": 180,
            },
            "Market Bombchu Shop Item 6": {
                "item": "Buy Bombchu (20)",
                "price": 180,
            },
            "Market Bombchu Shop Item 7": {
                "item": "Buy Bombchu (20)",
                "price": 180,
            },
            "Market Bombchu Shop Item 8": {
                "item": "Buy Bombchu (20)",
                "price": 180,
            },
            "Kak Bazaar Item 1": {
                "item": "Buy Hylian Shield",
                "price": 80,
            },
            "Kak Bazaar Item 2": {
                "item": "Buy Bombs (5) for 35 Rupees",
                "price": 35,
            },
            "Kak Bazaar Item 3": {
                "item": "Buy Deku Nut (5)",
                "price": 15,
            },
            "Kak Bazaar Item 4": {
                "item": "Buy Heart",
                "price": 10,
            },
            "Kak Bazaar Item 5": {
                "item": "Buy Arrows (10)",
                "price": 20,
            },
            "Kak Bazaar Item 6": {
                "item": "Buy Arrows (30)",
                "price": 60,
            },
            "Kak Bazaar Item 7": {
                "item": "Buy Deku Stick (1)",
                "price": 10,
            },
            "Kak Bazaar Item 8": {
                "item": "Buy Arrows (50)",
                "price": 90,
            },
            "Kak Potion Shop Item 1": {
                "item": "Buy Green Potion",
                "price": 30,
            },
            "Kak Potion Shop Item 2": {
                "item": "Buy Blue Fire",
                "price": 300,
            },
            "Kak Potion Shop Item 3": {
                "item": "Buy Red Potion for 30 Rupees",
                "price": 30,
            },
            "Kak Potion Shop Item 4": {
                "item": "Buy Fairy's Spirit",
                "price": 50,
            },
            "Kak Potion Shop Item 5": {
                "item": "Buy Deku Nut (5)",
                "price": 15,
            },
            "Kak Potion Shop Item 6": {
                "item": "Buy Bottle Bug",
                "price": 50,
            },
            "Kak Potion Shop Item 7": {
                "item": "Buy Poe",
                "price": 30,
            },
            "Kak Potion Shop Item 8": {
                "item": "Buy Fish",
                "price": 200,
            },
            "GC Shop Item 1": {
                "item": "Buy Bombs (5) for 25 Rupees",
                "price": 25,
            },
            "GC Shop Item 2": {
                "item": "Buy Bombs (10)",
                "price": 50,
            },
            "GC Shop Item 3": {
                "item": "Buy Bombs (20)",
                "price": 80,
            },
            "GC Shop Item 4": {
                "item": "Buy Bombs (30)",
                "price": 120,
            },
            "GC Shop Item 5": {
                "item": "Buy Goron Tunic",
                "price": 200,
            },
            "GC Shop Item 6": {
                "item": "Buy Heart",
                "price": 10,
            },
            "GC Shop Item 7": {
                "item": "Buy Red Potion for 40 Rupees",
                "price": 40,
            },
            "GC Shop Item 8": {
                "item": "Buy Heart",
                "price": 10,
            },
            "ZD Shop Item 1": {
                "item": "Buy Zora Tunic",
                "price": 300,
            },
            "ZD Shop Item 2": {
                "item": "Buy Arrows (10)",
                "price": 20,
            },
            "ZD Shop Item 3": {
                "item": "Buy Heart",
                "price": 10,
            },
            "ZD Shop Item 4": {
                "item": "Buy Arrows (30)",
                "price": 60,
            },
            "ZD Shop Item 5": {
                "item": "Buy Deku Nut (5)",
                "price": 15,
            },
            "ZD Shop Item 6": {
                "item": "Buy Arrows (50)",
                "price": 90,
            },
            "ZD Shop Item 7": {
                "item": "Buy Fish",
                "price": 200,
            },
            "ZD Shop Item 8": {
                "item": "Buy Red Potion for 50 Rupees",
                "price": 50,
            },
        }),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "20_skulltulas",
        ]),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Claim Check",
        ]),
    ]
}

#[serde_as]
#[derive(Default, Serialize)]
pub(crate) struct ProgressionSpoiler {
    #[serde_as(as = "serde_with::Map<_, _>")]
    locations: Vec<(String, String)>,
    #[serde_as(as = "serde_with::Map<_, _>")]
    gossip_stones_count: Vec<(String, String)>,
    #[serde_as(as = "serde_with::Map<_, _>")]
    gossip_stones_lock: Vec<(String, String)>,
    #[serde_as(as = "serde_with::Map<_, _>")]
    gossip_stones_path: Vec<(String, String)>,
    #[serde_as(as = "serde_with::Map<_, _>")]
    gossip_stones_foolish: Vec<(String, String)>,
    #[serde_as(as = "serde_with::Map<_, _>")]
    gossip_stones_other: Vec<(String, String)>,
}

pub(crate) fn progression_spoiler(spoiler: Json) -> ProgressionSpoiler {
    let mut spoiler_json = ProgressionSpoiler::default();
    for (key, value) in spoiler["locations"].as_object().unwrap() {
        let item_name = match value {
            Json::String(value) => &**value,
            _ => value["item"].as_str().unwrap(),
        };
        match item_name {
            | "Bottle"
            | "Bottle with Milk"
            | "Bottle with Poe"
            | "Bottle with Big Poe"
            | "Bottle with Bugs"
            | "Bottle with Blue Fire"
            | "Bottle with Fish"
            | "Bottle with Blue Potion"
            | "Progressive Strength Upgrade"
            | "Nocturne of Shadow"
            | "Small Key (Water Temple)"
            | "Bombchus (10)"
            | "Zora Tunic"
            | "Small Key (Fire Temple)"
            | "Bolero of Fire"
            | "Bomb Bag"
            | "Goron Tunic"
            | "Small Key (Gerudo Training Ground)"
            | "Zeldas Lullaby"
            | "Sarias Song"
            | "Iron Boots"
            | "Prelude of Light"
            | "Goron Ruby"
            | "Song of Time"
            | "Dins Fire"
            | "Lens of Truth"
            | "Hover Boots"
            | "Shadow Medallion"
            | "Mirror Shield"
            | "Light Medallion"
            | "Bombchus (5)"
            | "Minuet of Forest"
            | "Fire Arrows"
            | "Song of Storms"
            | "Rutos Letter"
            | "Small Key (Spirit Temple)"
            | "Progressive Scale"
            | "Double Defense"
            | "Suns Song"
            | "Small Key (Bottom of the Well)"
            | "Biggoron Sword"
            | "Progressive Hookshot"
            | "Kokiri Sword"
            | "Magic Meter"
            | "Bow"
            | "Claim Check"
            | "Requiem of Spirit"
            | "Kokiri Emerald"
            | "Water Medallion"
            | "Small Key (Ganons Castle)"
            | "Slingshot"
            | "Bottle with Green Potion"
            | "Bombchus (20)"
            | "Fire Medallion"
            | "Small Key (Forest Temple)"
            | "Zora Sapphire"
            | "Eponas Song"
            | "Megaton Hammer"
            | "Farores Wind"
            | "Bottle with Red Potion"
            | "Spirit Medallion"
            | "Boomerang"
            | "Serenade of Water"
            | "Bottle with Fairy"
            | "Progressive Wallet"
            | "Small Key (Shadow Temple)"
            | "Forest Medallion"
                => spoiler_json.locations.push((key.clone(), item_name.to_owned())),
            _ => {}
        }
    }
    let mut duplicate_hints = HashSet::new();
    for (key, value) in spoiler["gossip_stones"].as_object().unwrap() {
        if !duplicate_hints.remove(&value["text"]) {
            duplicate_hints.insert(value["text"].clone());
            let text = value["text"].as_str().unwrap().to_owned();
            if !text.contains("echo") {
                if text.contains("steps") {
                    &mut spoiler_json.gossip_stones_count
                } else if text.contains("unlocks") {
                    &mut spoiler_json.gossip_stones_lock
                } else if text.contains("is on the") {
                    &mut spoiler_json.gossip_stones_path
                } else if text.contains("foolish") {
                    &mut spoiler_json.gossip_stones_foolish
                } else {
                    &mut spoiler_json.gossip_stones_other
                }.push((key.clone(), text));
            }
        }
    }
    spoiler_json
}
