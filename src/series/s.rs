use {
    chrono::Days,
    derive_more::{
        Display,
        FromStr,
    },
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
        racetime_bot::PrerollMode,
    },
};

pub(crate) struct Setting {
    pub(crate) major: bool,
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) default_display: &'static str,
    pub(crate) other: &'static [(&'static str, &'static str, fn() -> seed::Settings)],
}

impl Setting {
    pub(crate) fn description(&self) -> String {
        let options = iter::once(format!("default ({})", self.default_display))
            .chain(self.other.iter().map(|(name, display, _)| format!("{name} ({display})")));
        format!("{}: {}", self.name, English.join_str_opt_with("or", options).expect("has at least the default option"))
    }
}

pub(crate) const S7_SETTINGS: [Setting; 31] = [
    Setting { major: true, name: "bridge", display: "Rainbow Bridge", default_display: "6 med bridge, GCBK removed", other: &[("open", "Open bridge, 6 med GCBK", || collect![format!("bridge") => json!("open"), format!("shuffle_ganon_bosskey") => json!("medallions")])] },
    Setting { major: true, name: "deku", display: "Kokiri Forest", default_display: "Closed Deku", other: &[("open", "Open Forest", || collect![format!("open_forest") => json!("open")])] },
    Setting { major: true, name: "interiors", display: "Indoor ER", default_display: "Indoor ER Off", other: &[("on", "Indoor ER On (All)", || collect![format!("shuffle_interior_entrances") => json!("all")])] },
    Setting { major: true, name: "dungeons", display: "Dungeon ER", default_display: "Dungeon ER Off", other: &[("on", "Dungeon ER On (no Ganon's Castle)", || collect![format!("shuffle_dungeon_entrances") => json!("simple")])] },
    Setting { major: true, name: "grottos", display: "Grotto ER", default_display: "Grotto ER Off", other: &[("on", "Grotto ER On", || collect![format!("shuffle_grotto_entrances") => json!(true)])] },
    Setting { major: true, name: "shops", display: "Shopsanity", default_display: "Shopsanity Off", other: &[("on", "Shopsanity 4", || collect![format!("shopsanity") => json!("4")])] },
    Setting { major: true, name: "ow_tokens", display: "Overworld Tokens", default_display: "Overworld Tokens Off", other: &[("on", "Overworld Tokens On", || collect![format!("tokensanity") => json!("overworld")])] },
    Setting { major: true, name: "dungeon_tokens", display: "Dungeon Tokens", default_display: "Dungeon Tokens Off", other: &[("on", "Dungeon Tokens On", || collect![format!("tokensanity") => json!("dungeons")])] },
    Setting { major: true, name: "scrubs", display: "Scrub Shuffle", default_display: "Scrub Shuffle Off", other: &[("on", "Scrub Shuffle On (Affordable)", || collect![format!("shuffle_scrubs") => json!("low")])] },
    Setting { major: true, name: "keys", display: "Keys", default_display: "Own Dungeon Keys", other: &[("keysy", "Keysy (both small and BK)", || collect![format!("shuffle_smallkeys") => json!("remove"), format!("shuffle_bosskeys") => json!("remove")]), ("anywhere", "Keyrings anywhere (includes BK)", || collect![format!("shuffle_smallkeys") => json!("keysanity"), format!("key_rings_choice") => json!("all"), format!("keyring_give_bk") => json!(true)])] },
    Setting { major: true, name: "required_only", display: "Guarantee Reachable Locations", default_display: "All Locations Reachable", other: &[("on", "Required Only (Beatable Only)", || collect![format!("reachable_locations") => json!("beatable")])] },
    Setting { major: true, name: "fountain", display: "Zora's Fountain", default_display: "Zora's Fountain Closed", other: &[("open", "Zora's Fountain Open (both ages)", || collect![format!("zora_fountain") => json!("open")])] },
    Setting { major: true, name: "cows", display: "Shuffle Cows", default_display: "Shuffle Cows Off", other: &[("on", "Shuffle Cows On", || collect![format!("shuffle_cows") => json!(true)])] },
    Setting { major: true, name: "gerudo_card", display: "Shuffle Gerudo Card", default_display: "Shuffle Gerudo Card Off", other: &[("on", "Shuffle Gerudo Card On", || collect![format!("shuffle_gerudo_card") => json!(true)])] },
    Setting { major: true, name: "trials", display: "Trials", default_display: "0 Trials", other: &[("on", "3 Trials", || collect![format!("trials") => json!(3)])] },
    Setting { major: true, name: "door_of_time", display: "Open Door of Time", default_display: "Open Door of Time", other: &[("closed", "Closed Door of Time", || collect![format!("open_door_of_time") => json!(false)])] },
    Setting { major: false, name: "starting_age", display: "Starting Age", default_display: "Random Starting Age", other: &[("child", "Child Start", || collect![format!("starting_age") => json!("child")]), ("adult", "Adult Start", || collect![format!("starting_age") => json!("adult")])] },
    Setting { major: false, name: "random_spawns", display: "Random Spawns", default_display: "Random Spawns Off", other: &[("on", "Random Spawns On (both ages)", || collect![format!("spawn_positions") => json!(["child", "adult"])])] },
    Setting { major: false, name: "consumables", display: "Start With Consumables", default_display: "Start With Consumables On", other: &[("none", "Start With Consumables Off", || collect![format!("start_with_consumables") => json!(false)])] },
    Setting { major: false, name: "rupees", display: "Start With Max Rupees", default_display: "Start With Max Rupees Off", other: &[("startwith", "Start With Max Rupees On", || collect![format!("start_with_rupees") => json!(true)])] },
    Setting { major: false, name: "cuccos", display: "Anju's Chickens", default_display: "7 Chickens", other: &[("1", "1 Chicken", || collect![format!("chicken_count") => json!(1)])] },
    Setting { major: false, name: "free_scarecrow", display: "Free Scarecrow", default_display: "Free Scarecrow Off", other: &[("on", "Free Scarecrow On", || collect![format!("free_scarecrow") => json!(true)])] },
    Setting { major: false, name: "camc", display: "CAMC", default_display: "CAMC: Size + Texture", other: &[("off", "CAMC Off", || collect![format!("correct_chest_appearances") => json!("off")])] },
    Setting { major: false, name: "mask_quest", display: "Complete Mask Quest", default_display: "Complete Mask Quest Off", other: &[("complete", "Complete Mask Quest On", || collect![format!("complete_mask_quest") => json!(true), format!("fast_bunny_hood") => json!(false)])] },
    Setting { major: false, name: "blue_fire_arrows", display: "Blue Fire Arrows", default_display: "Blue Fire Arrows Off", other: &[("on", "Blue Fire Arrows On", || collect![format!("blue_fire_arrows") => json!(true)])] },
    Setting { major: false, name: "owl_warps", display: "Random Owl Warps", default_display: "Random Owl Warps Off", other: &[("random", "Random Owl Warps On", || collect![format!("owl_drops") => json!(true)])] },
    Setting { major: false, name: "song_warps", display: "Random Warp Song Destinations", default_display: "Random Warp Song Destinations Off", other: &[("random", "Random Warp Song Destinations On", || collect![format!("warp_songs") => json!(true)])] },
    Setting { major: false, name: "shuffle_beans", display: "Shuffle Magic Beans", default_display: "Shuffle Magic Beans Off", other: &[("on", "Shuffle Magic Beans On", || collect![format!("shuffle_beans") => json!(true)])] },
    Setting { major: false, name: "expensive_merchants", display: "Shuffle Expensive Merchants", default_display: "Shuffle Expensive Merchants Off", other: &[("on", "Shuffle Expensive Merchants On", || collect![format!("shuffle_expensive_merchants") => json!(true)])] },
    Setting { major: false, name: "beans_planted", display: "Pre-planted Magic Beans", default_display: "Pre-planted Magic Beans Off", other: &[("on", "Pre-planted Magic Beans On", || collect![format!("plant_beans") => json!(true)])] },
    Setting { major: false, name: "bombchus_in_logic", display: "Add Bombchu Bag and Drops", default_display: "Bombchu Bag and Drops Off", other: &[("on", "Bombchu Bag and Drops On", || collect![format!("free_bombchu_drops") => json!(true)])] },
];

pub(crate) fn display_s7_draft_picks(picks: &draft::Picks) -> String {
    English.join_str_opt(
        S7_SETTINGS.into_iter()
            .filter_map(|Setting { name, other, .. }| picks.get(name).and_then(|pick| other.iter().find(|(other, _, _)| pick == other)).map(|(_, display, _)| display)),
    ).unwrap_or_else(|| format!("base settings"))
}

pub(crate) fn resolve_s7_draft_settings(picks: &draft::Picks) -> seed::Settings {
    let mut allowed_tricks = vec![
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
        "logic_visible_collisions",
    ];
    if picks.get("dungeons").map(|dungeons| &**dungeons).unwrap_or("default") == "on" {
        allowed_tricks.push("logic_dc_scarecrow_gs");
    }
    let mut settings = collect![as serde_json::Map<_, _>:
        format!("user_message") => json!("S7 Tournament"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("remove"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("free_bombchu_drops") => json!(false),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
        ]),
        format!("allowed_tricks") => json!(allowed_tricks),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("big_poe_count") => json!(1),
        format!("correct_chest_appearances") => json!("both"),
        format!("correct_potcrate_appearances") => json!("textures_content"),
        format!("hint_dist") => json!("tournament"),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "40_skulltulas",
            "50_skulltulas",
            "unique_merchants",
        ]),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
    ];
    for (setting, value) in picks {
        if value != "default" {
            let Setting { other, .. } = S7_SETTINGS.into_iter().find(|Setting { name, .. }| name == setting).expect("unknown setting in draft picks");
            settings.extend(other.iter().find(|(name, _, _)| name == value).expect("unknown setting value in draft picks").2());
        }
    }
    if picks.get("ow_tokens").map(|ow_tokens| &**ow_tokens).unwrap_or("default") == "on" && picks.get("dungeon_tokens").map(|dungeon_tokens| &**dungeon_tokens).unwrap_or("default") == "on" {
        settings.insert(format!("tokensanity"), json!("all"));
    }
    settings
}

#[derive(FromStr, Display, PartialEq, Eq, Hash, Sequence)]
pub(crate) enum WeeklyKind {
    Kokiri,
    Goron,
    Zora,
    Gerudo,
}

impl WeeklyKind {
    pub(crate) fn cal_id_part(&self) -> &'static str {
        match self {
            Self::Kokiri => "kokiri",
            Self::Goron => "goron",
            Self::Zora => "zora",
            Self::Gerudo => "gerudo",
        }
    }

    pub(crate) fn next_weekly_after(&self, min_time: DateTime<impl TimeZone>) -> DateTime<Tz> {
        let mut time = match self {
            Self::Kokiri => Utc.with_ymd_and_hms(2025, 1, 4, 23, 0, 0).single().expect("wrong hardcoded datetime"),
            Self::Goron => Utc.with_ymd_and_hms(2025, 1, 5, 19, 0, 0).single().expect("wrong hardcoded datetime"),
            Self::Zora => Utc.with_ymd_and_hms(2025, 1, 11, 19, 0, 0).single().expect("wrong hardcoded datetime"),
            Self::Gerudo => Utc.with_ymd_and_hms(2025, 1, 12, 14, 0, 0).single().expect("wrong hardcoded datetime"),
        }.with_timezone(&America::New_York);
        while time <= min_time {
            let date = time.date_naive().checked_add_days(Days::new(14)).unwrap();
            time = date.and_hms_opt(match self {
                Self::Kokiri => 18,
                Self::Goron | Self::Zora => 14,
                Self::Gerudo => 9,
            }, 0, 0).unwrap().and_local_timezone(America::New_York).single_ok().expect("error determining weekly time");
        }
        time
    }
}

// Make sure to keep the following in sync with each other and the rando_version and single_settings database entries:
pub(crate) const WEEKLY_PREROLL_MODE: PrerollMode = PrerollMode::Medium;
pub(crate) fn weekly_chest_appearances() -> ChestAppearances {
    static WEIGHTS: LazyLock<Vec<(ChestAppearances, usize)>> = LazyLock::new(|| serde_json::from_str(include_str!("../../assets/event/s/chests-w-8.3.12.json")).expect("failed to parse chest weights"));

    WEIGHTS.choose_weighted(&mut rng(), |(_, weight)| *weight).expect("failed to choose random chest textures").0
}
pub(crate) const SHORT_WEEKLY_SETTINGS: &str = "variety";
fn long_weekly_settings() -> RawHtml<String> {
    html! {
        p {
            : "Settings are typically changed once every 2 or 4 weeks and posted in ";
            a(href = "https://discord.com/channels/274180765816848384/512053754015645696") : "#standard-announcements";
            : " on Discord. Current settings starting with the Kokiri weekly on ";
            : format_datetime(Utc.with_ymd_and_hms(2025, 6, 7, 22, 00, 00).single().expect("wrong hardcoded datetime"), DateTimeFormat { long: false, running_text: true });
            : " are as follows:";
        }
        ul {
            li {
                a(href = uri!(event::info(Series::Standard, "8"))) : "S8";
                : " Base";
            }
            li : "AD Bridge (9 Dungeon Rewards; Gbk removed)";
            li : "2 Precompleted Dungeons (Shadow/Spirit medallion)";
            li : "Start with Light medallion";
            li : "Random Spawns (both ages)";
            li : "Free Scarecrow's Song";
            li : "Preplanted Beans";
            li : "Ruto on F1";
            li : "Fast Shadow Boat";
            li : "TCG requires Lens + Magic";
            li : "Key Appearance Matches Dungeons (cosmetic)";
            li {
                : "Hint Distribution:";
                ul {
                    li : "30/40/50 Skull House";
                    li : "5 Always";
                    li : "5 Path";
                    li : "3 Important Check";
                    li : "2 Dual";
                    li : "4 Sometimes";
                    li : "HC Storms & HF Cow disabled";
                }
            }
            li {
                : "Sometimes Hints added back into pool:";
                ul {
                    li : "Royal Family Tomb (dual)";
                    li : "Ice Cavern (dual)";
                }
            }
            li {
                : "Sometimes Hints removed from pool:";
                ul {
                    li : "Royal Family Tomb Torches";
                    li : "Ice Cavern Final Chest";
                    li : "IGC Shadow Trial 2";
                    li : "IGC Spirit Trial (dual)";
                }
            }
        }
    }
}

pub(crate) async fn info(_transaction: &mut Transaction<'_, Postgres>, _data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    // Content has been migrated to database - see event_info_content table
    // Use the WYSIWYG editor in the event setup page to manage content
    Ok(None)
}
