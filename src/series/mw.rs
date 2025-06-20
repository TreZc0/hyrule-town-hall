use crate::{
    event::{
        Data,
        Error,
        FindTeamError,
        InfoError,
        Tab,
        enter,
    },
    prelude::*,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, FromFormField)]
#[sqlx(type_name = "mw_impl", rename_all = "snake_case")]
pub(crate) enum Impl {
    #[field(value = "bizhawk_co_op")]
    #[sqlx(rename = "bizhawk_co_op")]
    BizHawkCoOp,
    #[field(value = "midos_house")]
    MidosHouse,
}

#[derive(Clone, Copy)]
pub(crate) struct Setting {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) default: &'static str,
    pub(crate) default_display: &'static str,
    pub(crate) other: &'static [(&'static str, &'static str)],
    pub(crate) description: &'static str,
}

pub(crate) const S3_SETTINGS: &[Setting] = &[
    Setting { name: "wincon", display: "win conditions", default: "meds", default_display: "default wincons", other: &[("scrubs", "Scrubs wincons"), ("th", "Triforce Hunt")], description: "wincon: meds (default: 6 Medallion Bridge + Keysy BK), scrubs (3 Stone Bridge + LACS BK), or th (Triforce Hunt 25/30)" },
    Setting { name: "dungeons", display: "dungeons", default: "tournament", default_display: "tournament dungeons", other: &[("skulls", "dungeon tokens"), ("keyrings", "keyrings")], description: "dungeons: tournament (default: keys shuffled in own dungeon), skulls (vanilla keys, dungeon tokens), or keyrings (small keyrings anywhere, vanilla boss keys)" },
    Setting { name: "er", display: "entrance rando", default: "off", default_display: "no ER", other: &[("dungeon", "dungeon ER")], description: "er: off (default) or dungeon" },
    Setting { name: "trials", display: "trials", default: "0", default_display: "0 trials", other: &[("2", "2 trials")], description: "trials: 0 (default) or 2" },
    Setting { name: "shops", display: "shops", default: "4", default_display: "shops 4", other: &[("off", "no shops")], description: "shops: 4 (default) or off" },
    Setting { name: "scrubs", display: "scrubs", default: "affordable", default_display: "affordable scrubs", other: &[("off", "no scrubs")], description: "scrubs: affordable (default) or off" },
    Setting { name: "fountain", display: "fountain", default: "closed", default_display: "closed fountain", other: &[("open", "open fountain")], description: "fountain: closed (default) or open" },
    Setting { name: "spawn", display: "spawns", default: "tot", default_display: "ToT spawns", other: &[("random", "random spawns & starting age")], description: "spawn: tot (default: adult start, vanilla spawns) or random (random spawns and starting age)" },
];

pub(crate) const S4_SETTINGS: &[Setting] = &[
    Setting { name: "gbk", display: "Ganon boss key", default: "meds", default_display: "Ganon bk on 6 medallions", other: &[("stones", "Ganon bk on 3 stones"), ("th", "Triforce Hunt")], description: "gbk (Ganon boss key): meds (default: 6 medallions), stones (3 stones), or th (Triforce Hunt 25/30)" },
    Setting { name: "bridge", display: "rainbow bridge", default: "meds", default_display: "6 medallions bridge", other: &[("dungeons", "7 dungeon rewards bridge"), ("vanilla", "vanilla bridge")], description: "bridge: meds (default: 6 medallions), dungeons (7 rewards), or vanilla" },
    Setting { name: "trials", display: "trials", default: "0", default_display: "0 trials", other: &[("2", "2 trials")], description: "trials: 0 (default) or 2" },
    Setting { name: "bosskeys", display: "boss keys", default: "dungeon", default_display: "own dungeon boss keys", other: &[("regional", "regional boss keys"), ("vanilla", "vanilla boss keys")], description: "bosskeys: dungeon (default), regional, or vanilla" },
    Setting { name: "smallkeys", display: "small keys", default: "dungeon", default_display: "own dungeon small keys", other: &[("regional", "regional keyrings"), ("vanilla", "vanilla small keys")], description: "smallkeys: dungeon (default), regional (with keyrings), or vanilla" },
    Setting { name: "deku", display: "open Deku", default: "open", default_display: "open Deku", other: &[("closed", "closed Deku")], description: "deku: open (Default) or closed" },
    Setting { name: "fountain", display: "fountain", default: "closed", default_display: "closed fountain", other: &[("open", "open fountain")], description: "fountain: closed (default) or open" },
    Setting { name: "spawn", display: "spawns", default: "tot", default_display: "ToT spawns", other: &[("random", "random spawns & starting age")], description: "spawn: tot (default: adult start, vanilla spawns) or random (random spawns and starting age)" },
    Setting { name: "dungeon-er", display: "dungeon entrance rando", default: "off", default_display: "no dungeon ER", other: &[("on", "dungeon ER")], description: "dungeon-er: off (default) or on" },
    Setting { name: "warps", display: "warp song entrance rando", default: "off", default_display: "vanilla warp songs", other: &[("on", "shuffled warp songs")], description: "warps: off (default) or on" },
    Setting { name: "chubags", display: "bombchu drops", default: "off", default_display: "no bombchu drops", other: &[("on", "bombchu drops")], description: "chubags: off (default) or on" },
    Setting { name: "shops", display: "shops", default: "4", default_display: "shops 4", other: &[("off", "no shops")], description: "shops: 4 (default) or off" },
    Setting { name: "skulls", display: "tokens", default: "off", default_display: "no tokens", other: &[("dungeons", "dungeon tokens")], description: "skulls: off (default) or dungeons" },
    Setting { name: "scrubs", display: "scrubs", default: "affordable", default_display: "affordable scrubs", other: &[("off", "no scrubs")], description: "scrubs: affordable (default) or off" },
    Setting { name: "cows", display: "cows", default: "off", default_display: "no cows", other: &[("on", "cows")], description: "cows: off (default) or on" },
    Setting { name: "card", display: "Gerudo card", default: "vanilla", default_display: "vanilla Gerudo card", other: &[("shuffle", "shuffled Gerudo card")], description: "card: vanilla (default) or shuffle" },
    Setting { name: "merchants", display: "merchants", default: "off", default_display: "no merchants", other: &[("shuffle", "shuffled merchants")], description: "merchants: off (defaut) or shuffle" },
    Setting { name: "frogs", display: "frogs", default: "off", default_display: "no frogs", other: &[("shuffle", "shuffled frogs")], description: "frogs: off (defaut) or shuffle" },
    Setting { name: "camc", display: "CAMC", default: "texture", default_display: "chest texture matches contents", other: &[("off", "vanilla chest appearances"), ("both", "chest size & texture match contents")], description: "camc (Chest Appearance Matches Contents): texture (default), off, or both (size & texture)" },
    Setting { name: "hints", display: "hint type", default: "path", default_display: "path hints", other: &[("woth", "Way of the Hero hints")], description: "hints: path (default) or woth" },
];

pub(crate) const S5_SETTINGS: &[Setting] = &[
    Setting { name: "gbk", display: "Ganon boss key", default: "meds", default_display: "Ganon bk on 6 medallions", other: &[("stones", "Ganon bk on 3 stones"), ("th", "Triforce Hunt")], description: "gbk (Ganon boss key): meds (default: 6 medallions), stones (3 stones), or th (Triforce Hunt 24/28)" },
    Setting { name: "bridge", display: "rainbow bridge", default: "meds", default_display: "6 medallions bridge", other: &[("dungeons", "7 dungeon rewards bridge"), ("vanilla", "vanilla bridge")], description: "bridge: meds (default: 6 medallions), dungeons (7 rewards), or vanilla" },
    Setting { name: "trials", display: "trials", default: "0", default_display: "0 trials", other: &[("2", "2 trials")], description: "trials: 0 (default) or 2" },
    Setting { name: "bosskeys", display: "boss keys", default: "dungeon", default_display: "own dungeon boss keys", other: &[("regional", "regional boss keys"), ("vanilla", "vanilla boss keys")], description: "bosskeys: dungeon (default), regional, or vanilla" },
    Setting { name: "smallkeys", display: "small keys", default: "dungeon", default_display: "own dungeon small keys", other: &[("regional", "regional keyrings"), ("vanilla", "vanilla small keys")], description: "smallkeys: dungeon (default), regional (with keyrings), or vanilla" },
    Setting { name: "deku", display: "open Deku", default: "open", default_display: "open Deku", other: &[("closed", "closed Deku")], description: "deku: open (Default) or closed" },
    Setting { name: "fountain", display: "fountain", default: "closed", default_display: "closed fountain", other: &[("open", "open fountain")], description: "fountain: closed (default) or open" },
    Setting { name: "spawn", display: "spawns", default: "tot", default_display: "ToT spawns", other: &[("random", "random spawns & starting age")], description: "spawn: tot (default: adult start, vanilla spawns) or random (random spawns and starting age)" },
    Setting { name: "dungeon-er", display: "dungeon entrance rando", default: "off", default_display: "no dungeon ER", other: &[("on", "dungeon ER")], description: "dungeon-er: off (default) or on" },
    Setting { name: "boss-er", display: "boss entrance rando", default: "off", default_display: "no boss ER", other: &[("full", "full boss ER")], description: "boss-er: off (default) or full" },
    Setting { name: "warps", display: "warp song entrance rando", default: "off", default_display: "vanilla warp songs", other: &[("on", "shuffled warp songs")], description: "warps: off (default) or on" },
    Setting { name: "chubags", display: "bombchu drops", default: "off", default_display: "no bombchu drops", other: &[("on", "bombchu drops")], description: "chubags: off (default) or on" },
    Setting { name: "shops", display: "shops", default: "4", default_display: "shops 4", other: &[("off", "no shops")], description: "shops: 4 (default) or off" },
    Setting { name: "skulls", display: "tokens", default: "dungeons", default_display: "dungeon tokens", other: &[("off", "no tokens")], description: "skulls: dungeons (default) or off" },
    Setting { name: "scrubs", display: "scrubs", default: "affordable", default_display: "affordable scrubs", other: &[("off", "no scrubs")], description: "scrubs: affordable (default) or off" },
    Setting { name: "cows", display: "cows", default: "off", default_display: "no cows", other: &[("on", "cows")], description: "cows: off (default) or on" },
    Setting { name: "card", display: "Gerudo card", default: "vanilla", default_display: "vanilla Gerudo card", other: &[("shuffle", "shuffled Gerudo card")], description: "card: vanilla (default) or shuffle" },
    Setting { name: "frogs", display: "frogs", default: "off", default_display: "no frogs", other: &[("shuffle", "shuffled frogs")], description: "frogs: off (defaut) or shuffle" },
    Setting { name: "camc", display: "CAMC", default: "both", default_display: "chest size & texture match contents", other: &[("off", "vanilla chest appearances")], description: "camc (Chest Appearance Matches Contents): both (default: size & texture) or off" },
    Setting { name: "hints", display: "hint type", default: "path", default_display: "path hints", other: &[("woth", "Way of the Hero hints")], description: "hints: path (default) or woth" },
];

pub(crate) fn display_s3_draft_picks(picks: &draft::Picks) -> String {
    English.join_str_opt(
        S3_SETTINGS.iter().copied()
            .filter_map(|Setting { name, other, .. }| picks.get(name).and_then(|pick| other.iter().find(|(other, _)| pick == other)).map(|(_, display)| display)),
    ).unwrap_or_else(|| format!("base settings"))
}

pub(crate) fn display_s4_draft_picks(picks: &draft::Picks) -> String {
    English.join_str_opt(
        S4_SETTINGS.iter().copied()
            .filter_map(|Setting { name, other, .. }|
                picks.get(name)
                    .cloned()
                    .or_else(|| (name == "camc" && picks.get("special_csmc").map(|special_csmc| &**special_csmc).unwrap_or("no") == "yes").then_some(Cow::Borrowed("both")))
                    .and_then(|pick| other.iter().find(|&(other, _)| pick == *other))
                    .map(|(_, display)| display)
            ),
    ).unwrap_or_else(|| format!("base settings"))
}

pub(crate) fn display_s5_draft_picks(picks: &draft::Picks) -> String {
    English.join_str_opt(
        S5_SETTINGS.iter().copied()
            .filter_map(|Setting { name, other, .. }| picks.get(name).and_then(|pick| other.iter().find(|(other, _)| pick == other)).map(|(_, display)| display)),
    ).unwrap_or_else(|| format!("base settings"))
}

pub(crate) fn resolve_s3_draft_settings(picks: &draft::Picks) -> seed::Settings {
    let wincon = picks.get("wincon").map(|wincon| &**wincon).unwrap_or("meds");
    let dungeons = picks.get("dungeons").map(|dungeons| &**dungeons).unwrap_or("tournament");
    let er = picks.get("er").map(|er| &**er).unwrap_or("off");
    let trials = picks.get("trials").map(|trials| &**trials).unwrap_or("0");
    let shops = picks.get("shops").map(|shops| &**shops).unwrap_or("4");
    let scrubs = picks.get("scrubs").map(|scrubs| &**scrubs).unwrap_or("affordable");
    let fountain = picks.get("fountain").map(|fountain| &**fountain).unwrap_or("closed");
    let spawn = picks.get("spawn").map(|spawn| &**spawn).unwrap_or("tot");
    collect![
        format!("user_message") => json!("3rd Multiworld Tournament"),
        format!("world_count") => json!(3),
        format!("open_forest") => json!("open"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("zora_fountain") => json!(fountain),
        format!("gerudo_fortress") => json!("fast"),
        format!("bridge") => match wincon {
            "meds" => json!("medallions"),
            "scrubs" => json!("stones"),
            "th" => json!("dungeons"),
            _ => unreachable!(),
        },
        format!("bridge_medallions") => json!(6),
        format!("bridge_stones") => json!(3),
        format!("bridge_rewards") => json!(4),
        format!("triforce_hunt") => json!(wincon == "th"),
        format!("triforce_count_per_world") => json!(30),
        format!("triforce_goal_per_world") => json!(25),
        format!("trials") => match trials {
            "0" => json!(0),
            "2" => json!(2),
            _ => unreachable!(),
        },
        format!("shuffle_child_trade") => json!("skip_child_zelda"),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("start_with_consumables") => json!(true),
        format!("big_poe_count") => json!(1),
        format!("shuffle_dungeon_entrances") => match er {
            "off" => json!("off"),
            "dungeon" => json!("simple"),
            _ => unreachable!(),
        },
        format!("spawn_positions") => json!(spawn == "random"),
        format!("shuffle_scrubs") => match scrubs {
            "affordable" => json!("low"),
            "off" => json!("off"),
            _ => unreachable!(),
        },
        format!("shopsanity") => json!(shops),
        format!("tokensanity") => match dungeons {
            "skulls" => json!("dungeons"),
            "tournament" | "keyrings" => json!("off"),
            _ => unreachable!(),
        },
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("shuffle_smallkeys") => match dungeons {
            "tournament" => json!("dungeon"),
            "skulls" => json!("vanilla"),
            "keyrings" => json!("keysanity"),
            _ => unreachable!(),
        },
        format!("key_rings_choice") => match dungeons {
            "keyrings" => json!("all"),
            "tournament" | "skulls" => json!("off"),
            _ => unreachable!(),
        },
        format!("shuffle_bosskeys") => match dungeons {
            "tournament" => json!("dungeon"),
            "skulls" | "keyrings" => json!("vanilla"),
            _ => unreachable!(),
        },
        format!("shuffle_ganon_bosskey") => match wincon {
            "meds" => json!("remove"),
            "scrubs" => json!("on_lacs"),
            "th" => json!("triforce"),
            _ => unreachable!(),
        },
        format!("enhance_map_compass") => json!(true),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
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
            "logic_dc_scarecrow_gs",
        ]),
        format!("adult_trade_start") => json!(["Claim Check"]),
        format!("starting_items") => json!([
            "ocarina",
            "farores_wind",
            "lens",
        ]),
        format!("correct_chest_appearances") => json!("both"),
        format!("hint_dist") => json!("mw3"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("junk_ice_traps") => json!("off"),
        format!("starting_age") => match spawn {
            "tot" => json!("adult"),
            "random" => json!("random"),
            _ => unreachable!(),
        },
    ]
}

pub(crate) fn resolve_s4_draft_settings(picks: &draft::Picks) -> seed::Settings {
    let gbk = picks.get("gbk").map(|gbk| &**gbk).unwrap_or("meds");
    let bridge = picks.get("bridge").map(|bridge| &**bridge).unwrap_or("meds");
    let trials = picks.get("trials").map(|trials| &**trials).unwrap_or("0");
    let bosskeys = picks.get("bosskeys").map(|bosskeys| &**bosskeys).unwrap_or("dungeon");
    let smallkeys = picks.get("smallkeys").map(|smallkeys| &**smallkeys).unwrap_or("dungeon");
    let deku = picks.get("deku").map(|deku| &**deku).unwrap_or("open");
    let fountain = picks.get("fountain").map(|fountain| &**fountain).unwrap_or("closed");
    let spawn = picks.get("spawn").map(|spawn| &**spawn).unwrap_or("tot");
    let dungeon_er = picks.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off");
    let warps = picks.get("warps").map(|warps| &**warps).unwrap_or("off");
    let chubags = picks.get("chubags").map(|chubags| &**chubags).unwrap_or("off");
    let shops = picks.get("shops").map(|shops| &**shops).unwrap_or("4");
    let skulls = picks.get("skulls").map(|skulls| &**skulls).unwrap_or("off");
    let scrubs = picks.get("scrubs").map(|scrubs| &**scrubs).unwrap_or("affordable");
    let cows = picks.get("cows").map(|cows| &**cows).unwrap_or("off");
    let card = picks.get("card").map(|card| &**card).unwrap_or("vanilla");
    let merchants = picks.get("merchants").map(|merchants| &**merchants).unwrap_or("off");
    let frogs = picks.get("frogs").map(|frogs| &**frogs).unwrap_or("off");
    let camc = picks.get("camc").map(|camc| &**camc).unwrap_or(if picks.get("special_csmc").map(|special_csmc| &**special_csmc).unwrap_or("no") == "yes" { "both" } else { "texture" });
    let hints = picks.get("hints").map(|hints| &**hints).unwrap_or("path");
    collect![
        format!("user_message") => json!("4th Multiworld Tournament"),
        format!("world_count") => json!(3),
        format!("triforce_hunt") => json!(gbk == "th"),
        format!("triforce_goal_per_world") => json!(25),
        format!("bridge") => match bridge {
            "meds" => json!("medallions"),
            "dungeons" => json!("dungeons"),
            "vanilla" => json!("vanilla"),
            _ => unreachable!(),
        },
        format!("bridge_rewards") => json!(7),
        format!("trials") => match trials {
            "0" => json!(0),
            "2" => json!(2),
            _ => unreachable!(),
        },
        format!("shuffle_ganon_bosskey") => if let "stones" = gbk {
            json!("stones")
        } else {
            json!("medallions")
        },
        format!("shuffle_bosskeys") => json!(bosskeys),
        format!("shuffle_smallkeys") => json!(smallkeys),
        format!("key_rings_choice") => if smallkeys == "regional" {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("open_forest") => match deku {
            "open" => json!("open"),
            "closed" => json!("closed_deku"),
            _ => unreachable!(),
        },
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("zora_fountain") => json!(fountain),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => match spawn {
            "tot" => json!("adult"),
            "random" => json!("random"),
            _ => unreachable!(),
        },
        format!("spawn_positions") => if spawn == "random" {
            json!(["child", "adult"])
        } else {
            json!([])
        },
        format!("shuffle_dungeon_entrances") => if dungeon_er == "on" {
            json!("simple")
        } else {
            json!("off")
        },
        format!("warp_songs") => json!(warps == "on"),
        format!("free_bombchu_drops") => json!(chubags == "on"),
        format!("shopsanity") => json!(shops),
        format!("tokensanity") => json!(skulls),
        format!("shuffle_scrubs") => match scrubs {
            "affordable" => json!("low"),
            "off" => json!("off"),
            _ => unreachable!(),
        },
        format!("shuffle_cows") => json!(cows == "on"),
        format!("shuffle_gerudo_card") => json!(card == "shuffle"),
        format!("shuffle_expensive_merchants") => json!(merchants == "shuffle"),
        format!("shuffle_frog_song_rupees") => json!(frogs == "shuffle"),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
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
            "logic_visible_collisions",
            "logic_dc_scarecrow_gs",
            "logic_deku_b1_webs_with_bow",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "farores_wind",
            "lens",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("correct_chest_appearances") => match camc {
            "texture" => json!("textures"),
            "off" => json!("off"),
            "both" => json!("both"),
            _ => unreachable!(),
        },
        format!("correct_potcrate_appearances") => json!("textures_content"),
        format!("key_appearance_match_dungeon") => json!(true),
        format!("hint_dist") => match hints {
            "path" => json!("mw_path"),
            "woth" => json!("mw_woth"),
            _ => unreachable!(),
        },
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "20_skulltulas",
            "30_skulltulas",
            "unique_merchants",
            "frogs2",
        ]),
        format!("blue_fire_arrows") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Claim Check",
        ]),
    ]
}

pub(crate) fn resolve_s5_draft_settings(picks: &draft::Picks) -> seed::Settings {
    let gbk = picks.get("gbk").map(|gbk| &**gbk).unwrap_or("meds");
    let bridge = picks.get("bridge").map(|bridge| &**bridge).unwrap_or("meds");
    let trials = picks.get("trials").map(|trials| &**trials).unwrap_or("0");
    let bosskeys = picks.get("bosskeys").map(|bosskeys| &**bosskeys).unwrap_or("dungeon");
    let smallkeys = picks.get("smallkeys").map(|smallkeys| &**smallkeys).unwrap_or("dungeon");
    let deku = picks.get("deku").map(|deku| &**deku).unwrap_or("open");
    let fountain = picks.get("fountain").map(|fountain| &**fountain).unwrap_or("closed");
    let spawn = picks.get("spawn").map(|spawn| &**spawn).unwrap_or("tot");
    let dungeon_er = picks.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off");
    let boss_er = picks.get("boss-er").map(|boss_er| &**boss_er).unwrap_or("off");
    let warps = picks.get("warps").map(|warps| &**warps).unwrap_or("off");
    let chubags = picks.get("chubags").map(|chubags| &**chubags).unwrap_or("off");
    let shops = picks.get("shops").map(|shops| &**shops).unwrap_or("4");
    let skulls = picks.get("skulls").map(|skulls| &**skulls).unwrap_or("dungeons");
    let scrubs = picks.get("scrubs").map(|scrubs| &**scrubs).unwrap_or("affordable");
    let cows = picks.get("cows").map(|cows| &**cows).unwrap_or("off");
    let card = picks.get("card").map(|card| &**card).unwrap_or("vanilla");
    let frogs = picks.get("frogs").map(|frogs| &**frogs).unwrap_or("off");
    let camc = picks.get("camc").map(|camc| &**camc).unwrap_or("both");
    let hints = picks.get("hints").map(|hints| &**hints).unwrap_or("path");
    collect![
        format!("password_lock") => json!(true),
        format!("user_message") => json!("5th Multiworld Tournament"),
        format!("world_count") => json!(3),
        format!("triforce_hunt") => json!(gbk == "th"),
        format!("triforce_count_per_world") => json!(28),
        format!("triforce_goal_per_world") => json!(24),
        format!("bridge") => match bridge {
            "meds" => json!("medallions"),
            "dungeons" => json!("dungeons"),
            "vanilla" => json!("vanilla"),
            _ => unreachable!(),
        },
        format!("bridge_rewards") => json!(7),
        format!("trials") => match trials {
            "0" => json!(0),
            "2" => json!(2),
            _ => unreachable!(),
        },
        format!("shuffle_ganon_bosskey") => if let "stones" = gbk {
            json!("stones")
        } else {
            json!("medallions")
        },
        format!("shuffle_bosskeys") => json!(bosskeys),
        format!("shuffle_smallkeys") => json!(smallkeys),
        format!("key_rings_choice") => if smallkeys == "regional" {
            json!("all")
        } else {
            json!("off")
        },
        format!("open_forest") => match deku {
            "open" => json!("open"),
            "closed" => json!("closed_deku"),
            _ => unreachable!(),
        },
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("zora_fountain") => json!(fountain),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => match spawn {
            "tot" => json!("adult"),
            "random" => json!("random"),
            _ => unreachable!(),
        },
        format!("spawn_positions") => if spawn == "random" {
            json!(["child", "adult"])
        } else {
            json!([])
        },
        format!("shuffle_dungeon_entrances") => if dungeon_er == "on" {
            json!("simple")
        } else {
            json!("off")
        },
        format!("shuffle_bosses") => json!(boss_er),
        format!("warp_songs") => json!(warps == "on"),
        format!("free_bombchu_drops") => json!(chubags == "on"),
        format!("shopsanity") => json!(shops),
        format!("tokensanity") => json!(skulls),
        format!("shuffle_scrubs") => match scrubs {
            "affordable" => json!("low"),
            "off" => json!("off"),
            _ => unreachable!(),
        },
        format!("adult_trade_start") => json!([
            "Claim Check",
        ]),
        format!("shuffle_cows") => json!(cows == "on"),
        format!("shuffle_gerudo_card") => json!(card == "shuffle"),
        format!("shuffle_frog_song_rupees") => json!(frogs == "shuffle"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
            "Kak 40 Gold Skulltula Reward",
            "Kak 50 Gold Skulltula Reward",
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
            "logic_visible_collisions",
            "logic_dc_scarecrow_gs",
            "logic_deku_b1_webs_with_bow",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "farores_wind",
            "lens",
            "zeldas_letter",
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
        format!("fast_shadow_boat") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("hint_dist") => match hints {
            "path" => json!("mw_path"),
            "woth" => json!("mw_woth"),
            _ => unreachable!(),
        },
        format!("misc_hints") => json!([
            "altar",
            "dampe_diary",
            "ganondorf",
            "warp_songs_and_owls",
            "20_skulltulas",
            "30_skulltulas",
            "frogs2",
        ]),
        format!("correct_chest_appearances") => json!(camc),
        format!("correct_potcrate_appearances") => json!("textures_content"),
        format!("key_appearance_match_dungeon") => json!(true),
        format!("blue_fire_arrows") => json!(true),
        format!("tcg_requires_lens") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
    ]
}

pub(crate) fn s3_chests(picks: &draft::Picks) -> ChestAppearances {
    static WEIGHTS: LazyLock<HashMap<String, Vec<(ChestAppearances, usize)>>> = LazyLock::new(|| serde_json::from_str(include_str!("../../assets/event/mw/chests-3-6.2.181.json")).expect("failed to parse chest weights")); //TODO update to 6.2.205

    if let Some(settings_weights) = WEIGHTS.get(&display_s3_draft_picks(picks)) {
        settings_weights.choose_weighted(&mut rng(), |(_, weight)| *weight).expect("failed to choose random chest textures").0
    } else {
        ChestAppearances::INVISIBLE
    }
}

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "1" => Some(html! {
            article {
                p {
                    : "This is an archive of the first Ocarina of Time randomizer multiworld tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
            }
        }),
        "2" => Some(html! {
            article {
                p {
                    : "This is an archive of the second Ocarina of Time randomizer multiworld tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". Click the “teams” link above to see the results of the qualifier async.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://docs.google.com/document/d/e/2PACX-1vS6vGCH8ZTA5bDCv3Z8meiUK4hMEfWN3vLttjNIOXbIAbRFNuGi-NzwJ68o31gVJgUigblLmW2tkZRu/pub") : "Tournament format, rules, and settings";
                    }
                    li {
                        a(href = "https://challonge.com/OoTRMWSeason2Swiss") : "Swiss results";
                    }
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/101zNpL1uvmIONb59kXYVyoa7YaHy8Y_OJv3M3vOKdBA/edit#gid=104642672") : "Tiebreaker scoresheet";
                    }
                    li {
                        a(href = "https://challonge.com/OoTRMWSeason2Finals") : "Top 8 results";
                    }
                }
            }
        }),
        "3" => Some(html! {
            article {
                p {
                    : "Hello and welcome to the official rules document for the Ocarina of Time Randomizer Multiworld Tournament Season 3, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "Tournament Format";
                p : "All teams are required to play a single asynchronous seed with the default race settings to participate. The results of this seed will be used to seed the settings draft.";
                p {
                    : "The tournament itself will begin with a randomly seeded series of ";
                    a(href = "https://en.wikipedia.org/wiki/Swiss-system_tournament") : "Swiss";
                    : " rounds. These will be played as best of 1. There will be 6 rounds, with each round lasting two weeks. In the event that teams do not schedule in time, tournament organizers will use their discretion to determine the correct outcome based on the failures to schedule. In unusual circumstances that affect the entire tournament base (such as a GDQ), a round can be extended at the discretion of tournament organizers.";
                }
                p : "After all Swiss rounds are done, plus an additional tiebreaker async, the top 8 teams will advance to a single elimination bracket to crown the champions. The bracket stage of the tournament will be played as best of 3.";
                h2 : "Match Format";
                p : "Each match will consist of a 3v3 Multiworld where both sides compete to see which team will have all three of its members beat the game with the lowest average time of finish:";
                ul {
                    li : "In a Triforce Hunt seed, timing for an individual player ends on the first completely black frame after that player has obtained the last required piece. (Due to how Triforce Hunt works in multiworld, all players on a team will normally have the same finish time, but if a player savescums a Triforce piece they found, they can have a lower Triforce count than their teammates.)";
                    li : "In all other seeds, timing for an individual player ends on the first frame of the cutscene that plays upon killing Ganon. Players are allowed to kill Ganon to stop their timer and then reset their game, allowing them to continue collecting items for their team if necessary.";
                }
                h2 : "Fair Play Agreement";
                p {
                    : "By joining this tournament, teams must accept the terms of the ";
                    a(href = "https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub") : "Fair Play Agreement (FPA)";
                    : ", a system that can be invoked in the event of technical issues. If playing on BizHawk, it is a strong, strong suggestion to make sure you enable backup saves as documented ";
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Bizhawk#Enable_Save_Backup_In_Case_of_Crashes") : "here";
                    : ".";
                }
                h2 : "Seed Settings";
                p {
                    : "Starting with Swiss round 2, all tournament matches will be played on ";
                    a(href = "https://ootrandomizer.com/generatorDev?version=dev_6.2.205") : "version 6.2.205";
                    : " of the randomizer. (The qualifier async and Swiss round 1 were played on ";
                    a(href = "https://ootrandomizer.com/generatorDev?version=dev_6.2.181") : "version 6.2.181";
                    : ".)";
                }
                p : "The default settings for each race have the following differences to the S5 tournament preset:";
                ul {
                    li : "Forest: Open";
                    li : "Scrub Shuffle: On (Affordable)";
                    li : "Shopsanity: 4 Items per Shop";
                    li : "Starting Inventory: Ocarina, FW, Lens of Truth, Consumables, Rupees (no Deku Shield)";
                    li : "Free Scarecrow's Song";
                    li : "Starting Age: Adult";
                    li : "Randomize Overworld Spawns: Off";
                    li : "Excluded Locations: Kak 40/50 Gold Skulltula Reward";
                    li : "Adult Trade Quest: Claim Check Only";
                    li : "Enable Tricks: Dodongo's Cavern Scarecrow GS with Armos Statue";
                    li : "Chest Appearance Matches Contents: Both Size and Texture";
                    li : "Maps and Compasses Give Information: On";
                }
                p : "You can use the “Multiworld Tournament Season 3” preset to load these settings.";
                p : "However, in every race several of the settings may be modified by the teams. During Swiss, the team that placed higher in the qualifier async gets to pick who starts the procedure. For the first game of a top 8 match, this choice is made by the team with the higher seed in the bracket, and for subsequent games of a match, by the team that lost the previous game. Ties are broken by coin flip. The draft itself follows this pattern:";
                ul {
                    li(class = "sheikah") : "Ban";
                    li(class = "gerudo") : "Ban";
                    li(class = "sheikah") : "Pick";
                    li(class = "gerudo") : "2x Pick";
                    li(class = "sheikah") : "Pick";
                }
                p {
                    : "A ";
                    em : "ban";
                    : " allows a team to lock in a setting of their choice to the default. A ";
                    em : "pick";
                    : " will function just like last season, allowing a team to change a setting or lock it to the default as well. This drafting procedure takes place in the scheduling thread for the match and must be completed at least 30 minutes before the scheduled starting time so the seed can be rolled.";
                }
                p {
                    : "The settings that can be modified and their respective options (";
                    strong : "first";
                    : " being the default) are, roughly ordered by impact on how drastically seeds and their length can change:";
                }
                ul {
                    li {
                        : "Win Condition:";
                        ul {
                            li {
                                strong : "6 Medallion Rainbow Bridge + Remove Ganon’s Boss Key";
                            }
                            li : "3 Stone Rainbow Bridge + Vanilla LACS Ganon’s Boss Key";
                            li : "Triforce Hunt with 25/30 Triforce pieces per world + 4 Reward Rainbow Bridge";
                        }
                    }
                    li {
                        : "Dungeons:";
                        ul {
                            li {
                                strong : "Small Keys and Boss Keys shuffled inside their own dungeons";
                            }
                            li : "Small Keys and Boss Keys in their vanilla chests + dungeon Tokensanity";
                            li : "Boss Keys in their vanilla chests + Small Keys anywhere + Keyrings on";
                        }
                    }
                    li {
                        : "Shuffle Dungeon Entrances:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On";
                        }
                    }
                    li {
                        : "Ganon's Trials:";
                        ul {
                            li {
                                strong : "0";
                            }
                            li : "2";
                        }
                    }
                    li {
                        : "Shopsanity:";
                        ul {
                            li {
                                strong : "4 Items per Shop + Random Prices";
                            }
                            li : "Off";
                        }
                    }
                    li {
                        : "Scrub Shuffle:";
                        ul {
                            li {
                                strong : "On (Affordable)";
                            }
                            li : "Off";
                        }
                    }
                    li {
                        : "Zora's Fountain:";
                        ul {
                            li {
                                strong : "Default Behavior (Closed)";
                            }
                            li : "Always Open";
                        }
                    }
                    li {
                        : "Starting Age/Randomize Overworld Spawns:";
                        ul {
                            li {
                                strong : "Adult/Off";
                            }
                            li : "Random/On";
                        }
                    }
                }
                h2 : "Hint Distribution";
                p : "Because of the somewhat unique settings of multiworld, there will be a custom hint distribution for this tournament. With 40 hint stones, the hint distribution will be as follows, with each hint having one duplicated hint:";
                ul {
                    li {
                        : "7 Goal/Path hints:";
                        ul {
                            li : "No dungeon limit";
                            li : "Zelda's Lullaby is never directly hinted";
                        }
                    }
                    li : "0 Foolish hints";
                    li {
                        : "5-8 “Always” Hints (Settings Dependent):";
                        ul {
                            li : "2 active Trials (if enabled)";
                            li : "Song from Ocarina of Time";
                            li : "Sheik in Kakariko";
                            li : "Deku Theater Skull Mask";
                            li : "Kak 30 Gold Skulltula Reward";
                            li : "ZR Frogs Ocarina Game";
                            li : "DMC Deku Scrub (if Scrubsanity enabled)";
                        }
                    }
                }
                p : "The remainder of the hints will be filled out with selections from the “Sometimes” hint pool for a total of 20 paired hints. The following additional locations are Sometimes hints (if dungeon Tokensanity is enabled):";
                ul {
                    li : "Deku Tree GS Basement Back Room";
                    li : "Water Temple GS River";
                    li : "Spirit Temple GS Hall After Sun Block Room";
                }
                p : "Always and Sometimes hints are upgraded to Dual hints where available.";
                p : "The following Sometimes hints have been removed:";
                ul {
                    li : "Sheik in Crater";
                    li : "Song from Royal Familys Tomb";
                    li : "Sheik in Forest";
                    li : "Sheik at Temple";
                    li : "Sheik at Colossus";
                    li : "LH Sun";
                    li : "GC Maze Left Chest";
                    li : "GV Chest";
                    li : "Graveyard Royal Familys Tomb Chest";
                    li : "GC Pot Freestanding PoH";
                    li : "LH Lab Dive";
                    li : "Fire Temple Megaton Hammer Chest";
                    li : "Fire Temple Scarecrow Chest";
                    li : "Water Temple Boss Key Chest";
                    li : "Water Temple GS Behind Gate";
                    li : "Gerudo Training Ground Maze Path Final Chest";
                    li : "Spirit Temple Silver Gauntlets Chest";
                    li : "Spirit Temple Mirror Shield Chest";
                    li : "Shadow Temple Freestanding Key";
                    li : "Ganons Castle Shadow Trial Golden Gauntlets Chest";
                }
                h2 : "Rules";
                p {
                    : "This tournament will take place under the ";
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "Standard";
                    : " racing ruleset, with some slight modifications:";
                }
                ul {
                    li : "Fire Arrow Entry is allowed";
                    li : "Playing Treasure Chest Game without magic and lens is banned";
                    li : "DMC “pot push” is banned";
                    li : "All custom models are banned";
                }
                h2 : "Multiworld Plugins";
                p {
                    : "There are two plugins that can be used for the item sharing: ";
                    a(href = "https://github.com/TestRunnerSRL/bizhawk-co-op#readme") : "bizhawk-co-op";
                    : " (also known as Multiworld 1.0) and ";
                    a(href = uri!(crate::mw::index)) : "HTH Multiworld";
                    : ". While we recommend using the HTH plugin since it supports Project64 in addition to BizHawk and is easier to use (see ";
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Multiworld#Feature_comparison") : "feature comparison";
                    : "), both plugins are legal in this tournament.";
                }
                p : "We were hopeful to host this season of the tournament on Multiworld 2.0, but there have been further delays with its release. In the event that it does release during this tournament, the plan is to allow Multiworld 2.0 to be used after being cleared by the tournament staff. However, be aware that by using this your team accepts the risks with using it and must abide by the standard FPA rules.";
            }
        }),
        "4" => Some(html! {
            article {
                p {
                    : "Hello and welcome to the official rules document for the Ocarina of Time Randomizer Multiworld Tournament Season 4, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "Ruleset";
                p {
                    : "This tournament will take place under the ";
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "Standard";
                    : " racing ruleset, with some slight modifications:";
                }
                ul {
                    li : "Fire Arrow Entry is allowed";
                    li : "Playing Treasure Chest Game without magic and lens is banned";
                    li : "Bypassing the boulders blocking the DMC fairy fountain using only hover boots is banned in the tiebreaker asyncs";
                }
                p : "All teams are expected to record their matches. Failure to do so may result in a game loss or disqualification from the tournament.";
                h2 : "Fair Play Agreement";
                p {
                    : "By joining this tournament, teams must accept the terms of the ";
                    a(href = "https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub") : "Fair Play Agreement (FPA)";
                    : ", a system that can be invoked in the event of technical issues. If playing on BizHawk, it is a strong, strong suggestion to make sure you enable backup saves as documented ";
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Bizhawk#Enable_Save_Backup_In_Case_of_Crashes") : "here";
                    : ".";
                }
                h2 : "Tournament Format";
                p : "All teams are required to play a single asynchronous seed with the default race settings to participate. The results of this seed will be used to seed the settings draft.";
                p {
                    : "The tournament itself will begin with a randomly seeded series of best-of-1 ";
                    a(href = "https://en.wikipedia.org/wiki/Swiss-system_tournament") : "Swiss";
                    : " rounds. It is expected to require 6 rounds, although this may change depending on the number of teams in the tournament. Each round will last two weeks, with the exception of 3 weeks for rounds 3 and 4 to accommodate the holidays and AGDQ.";
                }
                p : "After all Swiss rounds are done, there will be an additional async as a tiebreaker. The top 8 teams will advance to a single elimination, best-of-3 bracket to crown the champions.";
                p : "In the event that teams do not schedule in time, tournament organizers will use their discretion to determine the correct outcome based on the failures to schedule. In unusual circumstances, the schedule may be adjusted on short notice at the discretion of tournament organizers.";
                h2 : "Match Format";
                p : "Each match will consist of a 3v3 Multiworld where both sides compete to see which team will have all three of its members beat the game with the lowest average time of finish:";
                ul {
                    li : "In a Triforce Hunt seed, timing for an individual player ends on the first completely black frame after that player has obtained the last required piece. (Due to how Triforce Hunt works in multiworld, all players on a team will normally have the same finish time, but if a player savescums a Triforce piece they found, they can have a lower Triforce count than their teammates.)";
                    li : "In all other seeds, timing for an individual player ends on the first frame of the cutscene that plays upon killing Ganon. Players are allowed to kill Ganon to stop their timer and then reset their game, allowing them to continue collecting items for their team if necessary.";
                }
                h2 : "Seed Settings";
                p {
                    : "All tournament matches will be played on ";
                    a(href = "https://ootrandomizer.com/generatorDev?version=dev_7.1.199") : "version 7.1.199";
                    : " of the randomizer.";
                }
                p : "The default settings for each race have the following differences to the S7 tournament preset:";
                ul {
                    li : "Maps and Compasses Give Information: On";
                    li : "Forest: Open";
                    li : "Starting Age: Adult";
                    li : "Shopsanity: 4 Items per Shop";
                    li : "Scrub Shuffle: On (Affordable)";
                    li : "Excluded Locations: Kak 40/50 Gold Skulltula Reward";
                    li {
                        : "Logic Tricks:";
                        ul {
                            li : "Enabled Dodongo's Cavern Scarecrow GS with Armos Statue";
                            li : "Enabled Deku Tree Basement Web to Gohma with Bow";
                        }
                    }
                    li {
                        : "Starting Inventory:";
                        ul {
                            li : "Removed Deku Shield";
                            li : "Added Farores Wind";
                            li : "Added Lens of Truth";
                            li : "Added Rupees";
                        }
                    }
                    li : "Free Scarecrow's Song: On";
                    li : "Chicken Count: 3";
                    li : "Ruto Already at F1: On";
                    li : "Chest Appearance Matches Contents: Texture";
                    li : "Key Appearance Matches Dungeon: On";
                    li {
                        : "Misc. Hints:";
                        ul {
                            li : "Replaced 40/50 GS with 20/30 GS instead";
                            li : "Added Frogs Ocarina Game";
                        }
                    }
                    li : "Blue Fire Arrows: On";
                    li : "Adult Trade Quest: Claim Check Only";
                }
                p : "You can use the “Practice” link at the top of this page to load these settings.";
                p : "However, in every race several of the settings may be modified by the teams. During Swiss, the team that placed higher in the qualifier async gets to pick who starts the procedure. For the first game of a top 8 match, this choice is made by the team with the higher seed in the bracket, and for subsequent games of a match, by the team that lost the previous game. Ties are broken by coin flip. The draft itself follows this pattern:";
                ul {
                    li(class = "sheikah") : "Ban";
                    li(class = "gerudo") : "Ban";
                    li(class = "sheikah") : "Pick";
                    li(class = "gerudo") : "2x Pick";
                    li(class = "sheikah") : "Pick";
                    li(class = "gerudo") : "Ban";
                    li(class = "sheikah") : "Ban";
                    li(class = "gerudo") : "Pick";
                    li(class = "sheikah") : "Pick";
                }
                p {
                    : "A ";
                    em : "ban";
                    : " allows a team to lock in a setting of their choice to the default. A ";
                    em : "pick";
                    : " allows a team to change a setting or lock it to the default as well. This drafting procedure takes place in the scheduling thread for the match and must be completed at least 30 minutes before the scheduled starting time so the seed can be rolled.";
                }
                p {
                    : "The settings that can be modified and their respective options (";
                    strong : "first";
                    : " being the default) are:";
                }
                ul {
                    li {
                        : "Ganon BK:";
                        ul {
                            li {
                                strong : "6 Meds";
                            }
                            li : "3 Stones";
                            li : "25/30 Triforce Hunt";
                        }
                    }
                    li {
                        : "Rainbow Bridge:";
                        ul {
                            li {
                                strong : "6 Meds";
                            }
                            li : "7 Rewards";
                            li : "Vanilla";
                        }
                    }
                    li {
                        : "Ganon's Trials:";
                        ul {
                            li {
                                strong : "0";
                            }
                            li : "2";
                        }
                    }
                    li {
                        : "Boss Keys:";
                        ul {
                            li {
                                strong : "All keys shuffled inside their own dungeons (standard)";
                            }
                            li : "Regional";
                            li : "Vanilla";
                        }
                    }
                    li {
                        : "Small Keys:";
                        ul {
                            li {
                                strong : "All keys shuffled inside their own dungeons (standard)";
                            }
                            li : "Regional with small keyrings";
                            li : "Vanilla";
                        }
                    }
                    li {
                        : "Deku Tree:";
                        ul {
                            li {
                                strong : "Open";
                            }
                            li : "Closed";
                        }
                    }
                    li {
                        : "Zora's Fountain:";
                        ul {
                            li {
                                strong : "Default behavior (Closed)";
                            }
                            li : "Always open";
                        }
                    }
                    li {
                        : "Starting Age/Randomize Overworld Spawns:";
                        ul {
                            li {
                                strong : "Adult/Off";
                            }
                            li : "Random/On";
                        }
                    }
                    li {
                        : "Shuffle Dungeon Entrances:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On (not including Ganon's Castle)";
                        }
                    }
                    li {
                        : "Warp Song Destinations:";
                        ul {
                            li {
                                strong : "Vanilla";
                            }
                            li : "Shuffled";
                        }
                    }
                    li {
                        : "Chu bag and drops:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On";
                        }
                    }
                    li {
                        : "Shopsanity:";
                        ul {
                            li {
                                strong : "4 Items per Shop + Random Prices";
                            }
                            li : "Off";
                        }
                    }
                    li {
                        : "Tokensanity:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "Dungeons only";
                        }
                    }
                    li {
                        : "Scrub Shuffle:";
                        ul {
                            li {
                                strong : "On (Affordable)";
                            }
                            li : "Off";
                        }
                    }
                    li {
                        : "Cow Shuffle:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On";
                        }
                    }
                    li {
                        : "Gerudo Card shuffled:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On";
                        }
                    }
                    li {
                        : "Expensive Merchants:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On";
                        }
                    }
                    li {
                        : "Frog shuffle:";
                        ul {
                            li {
                                strong : "Off";
                            }
                            li : "On";
                        }
                    }
                    li {
                        : "Chest Appearance Matches Contents:";
                        ul {
                            li {
                                strong : "Texture only";
                            }
                            li : "Both Size and Texture";
                            li : "Off";
                        }
                    }
                    li {
                        : "Hint type:";
                        ul {
                            li {
                                strong : "Path";
                            }
                            li : "WOTH";
                        }
                    }
                }
                h2 : "Hint Distribution";
                p : "Because of the somewhat unique settings of multiworld, there will be a custom hint distribution for this tournament. With 40 hint stones, the hint distribution will be as follows, with each hint having one duplicated hint:";
                ul {
                    li {
                        : "7 Path/WOTH hints:";
                        ul {
                            li : "No dungeon limit";
                            li : "Zelda's Lullaby is never directly hinted";
                        }
                    }
                    li : "0 Foolish hints";
                    li {
                        : "3–7 “Always” Hints (Settings Dependent):";
                        ul {
                            li : "2 active Trials (if enabled)";
                            li : "Song from Ocarina of Time";
                            li : "Sheik in Kakariko";
                            li : "Deku Theater Skull Mask";
                            li : "DMC Deku Scrub (if Scrubsanity enabled)";
                            li : "Link's house cow (if Cowsanity enabled)";
                        }
                    }
                }
                p : "The remainder of the hints will be filled out with selections from the “Sometimes” hint pool for a total of 20 paired hints. Always and Sometimes hints are upgraded to Dual hints where available.";
                p : "The following locations are added to the Sometimes hint pool (if dungeon Tokensanity is enabled):";
                ul {
                    li : "Deku Tree GS Basement Back Room";
                    li : "Water Temple GS River";
                    li : "Spirit Temple GS Hall After Sun Block Room";
                }
                p : "The following Sometimes hints have been removed:";
                ul {
                    li : "Sheik in Crater";
                    li : "Song from Royal Familys Tomb";
                    li : "Sheik in Forest";
                    li : "Sheik at Temple";
                    li : "Sheik at Colossus";
                    li : "Graveyard Royal Familys Tomb Chest";
                    li : "Ganons Castle Shadow Trial Golden Gauntlets Chest";
                }
                h2 : "Multiworld Plugins";
                p {
                    : "There are two plugins that can be used for the item sharing: ";
                    a(href = "https://github.com/TestRunnerSRL/bizhawk-co-op#readme") : "bizhawk-co-op";
                    : " (also known as Multiworld 1.0) and ";
                    a(href = uri!(crate::mw::index)) : "HTH Multiworld";
                    : ". While we recommend using the Mido's House plugin since it supports Project64 in addition to BizHawk and is easier to use (see ";
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Multiworld#Feature_comparison") : "feature comparison";
                    : "), both plugins are legal in this tournament. For teams using MH MW, official multiworld rooms will be automatically created once the seed is rolled.";
                }
                p : "In the event that more ways to play multiworld become available during this tournament, such as a release of “Multiworld 2.0” or support for more platforms, the tournament organizers will decide whether to allow them on a case-by-case basis. Until an announcement is made, these are not allowed.";
                h2 : "Tournament Conduct Rules";
                p : "All players are expected to adhere to standards of good conduct and sportsmanship. Players or teams that are found violating this standard may be removed from the tournament. If a player is removed from a team for misconduct, that team may find a replacement player for the remainder of the tournament, although this is subject to approval by administrators.";
                p : "Prohibited actions include, but are not limited to:";
                ul {
                    li : "Threatening to pick specific settings unless the other team adheres to specific demands, including specific race dates/times or settings";
                    li : "Threatening to drop from the race or tournament";
                }
                p : "Forfeits are considered final. If a team forfeits because the opposing team finishes but the results are found to be non-binding (e.g., without video recordings), the forfeit holds and a rematch is not owed to the team that forfeited.";
                p : "The tournament admins reserve the right to use their best judgment when dealing with anything that comes up. This could include overturning match results, expulsion from the tournament, or other rulings.";
                p : "The tournament admins reserve the right to make any minor changes to these rules that are deemed in the best interest of the tournament.";
            }
        }),
        "5" => Some(html! {
            div(class = "toc") {
                article {
                    p {
                        : "Hello and welcome to the official rules document for the Ocarina of Time Randomizer Multiworld Tournament Season 5, organized by ";
                        : English.join_html_opt(data.organizers(transaction).await?);
                        : ".";
                    }
                    h2(id = "ruleset") : "Ruleset";
                    p {
                        : "This tournament will take place under the ";
                        a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "Standard";
                        : " racing ruleset, with one slight modification:";
                    }
                    ul {
                        li : "Fire Arrow Entry into Shadow Temple is allowed";
                    }
                    p : "All teams are expected to record their matches. Failure to do so may result in a game loss or disqualification from the tournament.";
                    h2(id = "fpa") : "Fair Play Agreement";
                    p {
                        : "By joining this tournament, teams must accept the terms of the ";
                        a(href = "https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub") : "Fair Play Agreement (FPA)";
                        : ", a system that can be invoked in the event of technical issues. If playing on BizHawk, it is a strong, strong suggestion to make sure you enable backup saves as documented ";
                        a(href = "https://wiki.ootrandomizer.com/index.php?title=Bizhawk#Enable_Save_Backup_In_Case_of_Crashes") : "here";
                        : ".";
                    }
                    h2(id = "format") : "Tournament Format";
                    p {
                        : "All teams are required to play a single asynchronous seed with the default race settings to participate. This seed can be requested on the “My Status” tab after entering the event. The results of this seed will be used to seed the Swiss matchups and the settings draft. The deadline for finishing the qualifier async ";
                        @if let Some(base_start) = data.base_start {
                            : "is ";
                            : format_datetime(base_start, DateTimeFormat { long: true, running_text: true });
                        } else {
                            : "will be announced soon";
                        }
                        : ". In the event that an odd number of teams is qualified at the time of the deadline, one additional team may qualify within 24 hours. Streaming the qualifier async is allowed as long as it's marked clearly.";
                    }
                    p {
                        : "The tournament itself will begin with a series of best-of-1 ";
                        a(href = "https://en.wikipedia.org/wiki/Swiss-system_tournament") : "Swiss";
                        : " rounds seeded based on the qualifier results. It is expected to require 6 rounds, although this may change depending on the number of teams in the tournament. Each round will last two weeks.";
                    }
                    p : "After all Swiss rounds are done, there will be an additional async as a tiebreaker. The top 8 teams will advance to a single elimination bracket to crown the champions. The quarterfinals will be a single race each, while semifinals and finals will be best-of-3 matches.";
                    p : "In the event that teams do not schedule in time, tournament organizers will use their discretion to determine the correct outcome based on the failures to schedule. In unusual circumstances, the schedule may be adjusted on short notice at the discretion of tournament organizers.";
                    h2(id = "match") : "Match Format";
                    p : "Each match will consist of a 3v3 Multiworld where both sides compete to see which team will have all three of its members beat the game with the lowest average time of finish:";
                    ul {
                        li : "In a Triforce Hunt seed, timing for an individual player ends on the first completely black frame after that player has obtained the last required piece. (Due to how Triforce Hunt works in multiworld, all players on a team will normally have the same finish time, but if a player savescums a Triforce piece they found, they can have a lower Triforce count than their teammates.)";
                        li : "In all other seeds, timing for an individual player ends on the first frame of the cutscene that plays upon killing Ganon.";
                        }
                    p : "Players are allowed to kill Ganon (or collect their final Triforce piece) to stop their timer and then reset their game, allowing them to continue collecting items for their team if necessary.";
                    p : "If any player on the team forfeits (or does not finish within the 24-hour time limit), it counts as a forfeit for the entire team.";
                    h2(id = "settings") : "Seed Settings";
                    p {
                        : "Starting with Swiss round 6, all tournament matches will be played on ";
                        a(href = "https://ootrandomizer.com/generator?version=8.3.0") : "version 8.3";
                        : " of the randomizer. (The qualifier async and Swiss rounds 1–3 were played on ";
                        a(href = "https://ootrandomizer.com/generatorDev?version=dev_8.2.64") : "version 8.2.64";
                        : ", and Swiss rounds 4–5 were played on ";
                        a(href = "https://ootrandomizer.com/generatorDev?version=dev_8.2.76") : "version 8.2.76";
                        : ".) Organizers may change this version between rounds at their discretion.";
                    }
                    p : "The default settings for each race have the following differences to the S8 tournament preset:";
                    ul {
                        li : "Rainbow Bridge: 6 Medallions";
                        li : "Forest: Open";
                        li : "Starting Age: Adult";
                        li : "Spawn Positions: Vanilla";
                        li : "Shopsanity: 4 Items per Shop";
                        li : "Tokensanity: Dungeons Only";
                        li : "Scrub Shuffle: On (Affordable)";
                        li : "Adult Trade Quest: Claim Check Only";
                        li : "Maps and Compasses Give Information: On";
                        li : "Excluded Locations: Kak 40/50 Gold Skulltula Reward";
                        li {
                            : "Logic Tricks:";
                            ul {
                                li : "Pass Through Visible One-Way Collisions";
                                li : "Enabled Dodongo's Cavern Scarecrow GS with Armos Statue";
                                li : "Enabled Deku Tree Basement Web to Gohma with Bow";
                            }
                        }
                        li {
                            : "Starting Inventory:";
                            ul {
                                li : "Removed Deku Shield";
                                li : "Added Farores Wind";
                                li : "Added Lens of Truth";
                                li : "Added Rupees";
                            }
                        }
                        li : "Free Scarecrow's Song: On";
                        li : "Ruto Already at F1: On";
                        li : "Fast Shadow Boat: On";
                        li : "Key Appearance Matches Dungeon: On";
                        li {
                            : "Misc. Hints:";
                            ul {
                                li : "Replaced 30/40/50 GS with 20/30 GS instead";
                                li : "Added Frogs Ocarina Game";
                                li : "Added Dampé Diary (Hookshot)";
                            }
                        }
                        li : "Require Lens of Truth for Treasure Chest Game: On";
                    }
                    p : "You can use the “Practice” link at the top of this page to load these settings.";
                    p : "However, in every race several of the settings may be modified by the teams. During Swiss, the team that placed higher in the qualifier async gets to pick who starts the procedure. For the first game of a top 8 match, this choice is made by the team with the higher seed in the bracket, and for subsequent games of a match, by the team that lost the previous game. Ties are broken by coin flip. The draft itself follows this pattern:";
                    ul {
                        li(class = "sheikah") : "Ban";
                        li(class = "gerudo") : "Ban";
                        li(class = "sheikah") : "Pick";
                        li(class = "gerudo") : "2x Pick";
                        li(class = "sheikah") : "Pick";
                        li(class = "gerudo") : "Ban";
                        li(class = "sheikah") : "Ban";
                        li(class = "gerudo") : "Pick";
                        li(class = "sheikah") : "Pick";
                    }
                    p {
                        : "A ";
                        em : "ban";
                        : " allows a team to lock in a setting of their choice to the default. A ";
                        em : "pick";
                        : " allows a team to change a setting or lock it to the default as well. This drafting procedure takes place in the scheduling thread for the match and must be completed at least 30 minutes before the scheduled starting time so the seed can be rolled.";
                    }
                    p {
                        : "The settings that can be modified and their respective options (";
                        strong : "first";
                        : " being the default) are:";
                    }
                    ul {
                        li {
                            : "Ganon BK:";
                            ul {
                                li {
                                    strong : "6 Meds";
                                }
                                li : "3 Stones";
                                li : "24/28 Triforce Hunt";
                            }
                        }
                        li {
                            : "Rainbow Bridge:";
                            ul {
                                li {
                                    strong : "6 Meds";
                                }
                                li : "7 Rewards";
                                li : "Vanilla";
                            }
                        }
                        li {
                            : "Ganon's Trials:";
                            ul {
                                li {
                                    strong : "0";
                                }
                                li : "2";
                            }
                        }
                        li {
                            : "Boss Keys:";
                            ul {
                                li {
                                    strong : "All keys shuffled inside their own dungeons (standard)";
                                }
                                li : "Regional";
                                li : "Vanilla";
                            }
                        }
                        li {
                            : "Small Keys:";
                            ul {
                                li {
                                    strong : "All keys shuffled inside their own dungeons (standard)";
                                }
                                li : "Regional with small keyrings";
                                li : "Vanilla";
                            }
                        }
                        li {
                            : "Deku Tree:";
                            ul {
                                li {
                                    strong : "Open";
                                }
                                li : "Closed";
                            }
                        }
                        li {
                            : "Zora's Fountain:";
                            ul {
                                li {
                                    strong : "Default behavior (Closed)";
                                }
                                li : "Always open";
                            }
                        }
                        li {
                            : "Starting Age/Randomize Overworld Spawns:";
                            ul {
                                li {
                                    strong : "Adult/Off";
                                }
                                li : "Random/On";
                            }
                        }
                        li {
                            : "Shuffle Dungeon Entrances:";
                            ul {
                                li {
                                    strong : "Off";
                                }
                                li : "On (not including Ganon's Castle)";
                            }
                        }
                        li {
                            : "Shuffle Boss Entrances:";
                            ul {
                                li {
                                    strong : "Off";
                                }
                                li : "Full";
                            }
                        }
                        li {
                            : "Warp Song Destinations:";
                            ul {
                                li {
                                    strong : "Vanilla";
                                }
                                li : "Shuffled";
                            }
                        }
                        li {
                            : "Chu bag and drops:";
                            ul {
                                li {
                                    strong : "Off";
                                }
                                li : "On";
                            }
                        }
                        li {
                            : "Shopsanity:";
                            ul {
                                li {
                                    strong : "4 Items per Shop + Random Prices";
                                }
                                li : "Off";
                            }
                        }
                        li {
                            : "Tokensanity:";
                            ul {
                                li {
                                    strong : "Dungeons only";
                                }
                                li : "Off";
                            }
                        }
                        li {
                            : "Scrub Shuffle:";
                            ul {
                                li {
                                    strong : "On (Affordable)";
                                }
                                li : "Off";
                            }
                        }
                        li {
                            : "Cow Shuffle:";
                            ul {
                                li {
                                    strong : "Off";
                                }
                                li : "On";
                            }
                        }
                        li {
                            : "Gerudo Card shuffled:";
                            ul {
                                li {
                                    strong : "Off";
                                }
                                li : "On";
                            }
                        }
                        li {
                            : "Frog shuffle:";
                            ul {
                                li {
                                    strong : "Off";
                                }
                                li : "On";
                            }
                        }
                        li {
                            : "Chest Appearance Matches Contents:";
                            ul {
                                li {
                                    strong : "Both Size and Texture";
                                }
                                li : "Off";
                            }
                        }
                        li {
                            : "Hint type:";
                            ul {
                                li {
                                    strong : "Path";
                                }
                                li : "WOTH";
                            }
                        }
                    }
                    h2(id = "hints") : "Hint Distribution";
                    p : "Because of the somewhat unique settings of multiworld, there will be a custom hint distribution for this tournament. With 40 hint stones, the hint distribution will be as follows, with each hint having one duplicated hint:";
                    ul {
                        li {
                            : "7 Path/WOTH hints:";
                            ul {
                                li : "No dungeon limit";
                                li : "Zelda's Lullaby, keyrings, or boss keys are never directly hinted";
                                li : "“One Hint per Goal” is enabled";
                            }
                        }
                        li : "0 Foolish hints";
                        li {
                            : "3–6 “Always” Hints (Settings Dependent):";
                            ul {
                                li : "active Trials (if enabled)";
                                li : "Song from Ocarina of Time";
                                li : "Sheik in Kakariko";
                                li : "Deku Theater Skull Mask";
                                li : "DMC Deku Scrub (if Scrubsanity enabled)";
                                li : "Link's house cow (if Cowsanity enabled)";
                            }
                        }
                    }
                    p : "The remainder of the hints will be filled out with selections from the “Sometimes” hint pool for a total of 20 paired hints. Always and Sometimes hints are upgraded to Dual hints where available.";
                    p : "The following locations are added to the Sometimes hint pool (if dungeon tokens are shuffled):";
                    ul {
                        li : "Deku Tree GS Basement Back Room";
                        li : "Water Temple GS River";
                        li : "Spirit Temple GS Hall After Sun Block Room";
                    }
                    p : "The following Sometimes hints have been removed:";
                    ul {
                        li : "Sheik in Crater";
                        li : "Song from Royal Familys Tomb";
                        li : "Sheik in Forest";
                        li : "Sheik at Temple";
                        li : "Sheik at Colossus";
                        li : "Graveyard Royal Familys Tomb Chest";
                        li : "Ganons Castle Shadow Trial Golden Gauntlets Chest";
                    }
                    h2(id = "plugins") : "Multiworld Plugins";
                    p {
                        : "There are two plugins that can be used for the item sharing: ";
                        a(href = "https://github.com/TestRunnerSRL/bizhawk-co-op#readme") : "bizhawk-co-op";
                        : " (also known as Multiworld 1.0) and ";
                        a(href = uri!(crate::mw::index)) : "HTH Multiworld";
                        : ". While we recommend using the Mido's House plugin since it supports Project64 in addition to BizHawk and is easier to use (see ";
                        a(href = "https://wiki.ootrandomizer.com/index.php?title=Multiworld#Feature_comparison") : "feature comparison";
                        : "), both plugins are legal in this tournament. For teams using MH MW, official multiworld rooms will be automatically created once the seed is rolled.";
                    }
                    p : "In the event that more ways to play multiworld become available during this tournament, such as a release of “Multiworld 2.0” or support for more platforms, the tournament organizers will decide whether to allow them on a case-by-case basis. Until an announcement is made, these are not allowed.";
                    h2(id = "conduct") : "Tournament Conduct Rules";
                    p : "All players are expected to adhere to standards of good conduct and sportsmanship. Players or teams that are found violating this standard may be removed from the tournament. If a player is removed from a team for misconduct, that team may find a replacement player for the remainder of the tournament, although this is subject to approval by administrators.";
                    p : "Prohibited actions include, but are not limited to:";
                    ul {
                        li : "Threatening to pick specific settings unless the other team adheres to specific demands, including specific race dates/times or settings";
                        li : "Threatening to drop from the race or tournament";
                    }
                    p : "Forfeits are considered final. If a team forfeits because the opposing team finishes but the results are found to be non-binding (e.g., without video recordings), the forfeit holds and a rematch is not owed to the team that forfeited.";
                    p : "The tournament admins reserve the right to use their best judgment when dealing with anything that comes up. This could include overturning match results, expulsion from the tournament, or other rulings.";
                    p : "The tournament admins reserve the right to make any minor changes to these rules that are deemed in the best interest of the tournament.";
                    h2(id = "restreams") : "Restreams";
                    p : "If you are interested in restreaming one or more races of our tournament, please contact an organizer. We will invite you to a channel for organizing restreams.";
                    p : "When being restreamed, teams that indicated that they use HTH Multiworld must use the provided official multiworld rooms.";
                    p {
                        : "Please also see ";
                        a(href = "https://docs.google.com/document/d/1A3dKYjSERr6MtZWyqOs04CZ8yRdAPvBxORrD0OsG6-s/edit") : "the guidelines for restreaming races";
                        : " and ";
                        a(href = "https://docs.google.com/document/d/1yhLL3tcO5qipe8OXCM0CX7gde822hCYW1ey55HNqaP0/edit") : "the guidelines for being restreamed";
                        : ".";
                    }
                }
                div {
                    nav {
                        strong : "Contents";
                        ul {
                            li {
                                a(href = "#ruleset") : "Ruleset";
                            }
                            li {
                                a(href = "#fpa") : "Fair Play Agreement";
                            }
                            li {
                                a(href = "#format") : "Tournament Format";
                            }
                            li {
                                a(href = "#match") : "Match Format";
                            }
                            li {
                                a(href = "#settings") : "Seed Settings";
                            }
                            li {
                                a(href = "#hints") : "Hint Distribution";
                            }
                            li {
                                a(href = "#plugins") : "Multiworld Plugins";
                            }
                            li {
                                a(href = "#conduct") : "Tournament Conduct Rules";
                            }
                            li {
                                a(href = "#restreams") : "Restreams";
                            }
                        }
                    }
                }
            }
        }),
        _ => None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, FromFormField, Sequence)]
pub(crate) enum Role {
    #[field(value = "power")]
    Power,
    #[field(value = "wisdom")]
    Wisdom,
    #[field(value = "courage")]
    Courage,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Power => write!(f, "player 1"),
            Self::Wisdom => write!(f, "player 2"),
            Self::Courage => write!(f, "player 3"),
        }
    }
}

impl ToHtml for Role {
    fn to_html(&self) -> RawHtml<String> {
        match self {
            Self::Power => html! {
                span(class = "power") : "player 1";
            },
            Self::Wisdom => html! {
                span(class = "wisdom") : "player 2";
            },
            Self::Courage => html! {
                span(class = "courage") : "player 3";
            },
        }
    }
}

impl TryFrom<event::Role> for Role {
    type Error = ();

    fn try_from(role: event::Role) -> Result<Self, ()> {
        match role {
            event::Role::Power => Ok(Self::Power),
            event::Role::Wisdom => Ok(Self::Wisdom),
            event::Role::Courage => Ok(Self::Courage),
            _ => Err(()),
        }
    }
}

impl From<Role> for event::Role {
    fn from(role: Role) -> Self {
        match role {
            Role::Power => Self::Power,
            Role::Wisdom => Self::Wisdom,
            Role::Courage => Self::Courage,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct RaceTimeTeamData {
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) members: Vec<RaceTimeTeamMember>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RaceTimeTeamMember {
    pub(crate) id: String,
    pub(crate) name: String,
}

pub(crate) async fn enter_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, ctx: Context<'_>, http_client: &reqwest::Client) -> Result<RawHtml<String>, Error> {
    let header = data.header(&mut transaction, me.as_ref(), Tab::Enter, false).await?;
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Enter — {}", data.display_name), if let Some(ref me) = me {
        match me.racetime_user_data(http_client).await? {
            Some(Some(racetime_user)) => {
                let mut errors = ctx.errors().collect_vec();
                if racetime_user.teams.is_empty() {
                    html! {
                        : header;
                        article {
                            p {
                                a(href = format!("https://{}/account/teams/create", racetime_host())) : "Create a racetime.gg team";
                                : " to enter this event.";
                            }
                        }
                    }
                } else {
                    html! {
                        : header;
                        : full_form(uri!(enter::post(data.series, &*data.event)), csrf, html! {
                            : form_field("racetime_team", &mut errors, html! {
                                label(for = "racetime_team") : "racetime.gg Team:";
                                select(name = "racetime_team") {
                                    @for team in racetime_user.teams {
                                        option(value = team.slug) : team.name;
                                    }
                                }
                                label(class = "help") {
                                    : "(Or ";
                                    a(href = format!("https://{}/account/teams/create", racetime_host())) : "create a new team";
                                    : ", then come back here.)";
                                }
                            });
                        }, errors, "Next");
                    }
                }
            }
            Some(None) => html! {
                : header;
                article {
                    p {
                        : "Your racetime.gg profile is not public. Please ";
                        a(href = format!("https://{}/account/connections", racetime_host())) : "connect a Twitch or Patreon account to your racetime.gg account";
                        : " or participate in a recorded race.";
                    }
                }
            },
            None => html! {
                : header;
                article {
                    p {
                        a(href = uri!(crate::auth::racetime_login(Some(uri!(enter::get(data.series, &*data.event, _, _)))))) : "Connect a racetime.gg account to your Hyrule Town Hall account";
                        : " to enter this event.";
                    }
                }
            },
        }
    } else {
        html! {
            : header;
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(enter::get(data.series, &*data.event, _, _)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to enter this event.";
                }
            }
        }
    }).await?)
}

//TODO this is no longer needed since the forms have been merged
pub(crate) enum EnterFormStep2Defaults<'a> {
    Context(Context<'a>),
    Values {
        racetime_team: RaceTimeTeamData,
    },
}

impl<'v> EnterFormStep2Defaults<'v> {
    pub(crate) fn errors(&self) -> Vec<&form::Error<'v>> {
        match self {
            Self::Context(ctx) => ctx.errors().collect(),
            Self::Values { .. } => Vec::default(),
        }
    }

    pub(crate) fn field_value(&self, field_name: &str) -> Option<&str> {
        match self {
            Self::Context(ctx) => ctx.field_value(field_name),
            Self::Values { .. } => None,
        }
    }

    pub(crate) fn racetime_team_name(&self) -> Option<&str> {
        match self {
            Self::Context(ctx) => ctx.field_value("racetime_team_name"),
            Self::Values { racetime_team: RaceTimeTeamData { name, .. } } => Some(name),
        }
    }

    pub(crate) fn racetime_team_slug(&self) -> Option<&str> {
        match self {
            Self::Context(ctx) => ctx.field_value("racetime_team"),
            Self::Values { racetime_team: RaceTimeTeamData { slug, .. } } => Some(slug),
        }
    }

    pub(crate) fn racetime_members(&self, http_client: &reqwest::Client) -> impl Future<Output = Result<Vec<RaceTimeTeamMember>, Error>> + use<> {
        match self {
            Self::Context(ctx) => if let Some(team_slug) = ctx.field_value("racetime_team") {
                let http_client = http_client.clone();
                let url = format!("https://{}/team/{team_slug}/data", racetime_host());
                async move {
                    Ok(http_client.get(url)
                        .send().await?
                        .detailed_error_for_status().await?
                        .json_with_text_in_error::<RaceTimeTeamData>().await?
                        .members
                    )
                }.boxed()
            } else {
                future::ok(Vec::default()).boxed()
            }
            Self::Values { racetime_team } => future::ok(racetime_team.members.clone()).boxed(),
        }
    }

    pub(crate) fn role(&self, racetime_id: &str) -> Option<event::Role> {
        match self {
            Self::Context(ctx) => ctx.field_value(&*format!("roles[{racetime_id}]")).and_then(event::Role::from_css_class),
            Self::Values { .. } => None,
        }
    }

    pub(crate) fn startgg_id(&self, racetime_id: &str) -> Option<&str> {
        match self {
            Self::Context(ctx) => ctx.field_value(&*format!("startgg_id[{racetime_id}]")),
            Self::Values { .. } => None, //TODO get default from MH user?
        }
    }

    pub(crate) fn mw_impl(&self) -> Option<Impl> {
        match self {
            Self::Context(ctx) => match ctx.field_value("mw_impl") {
                Some("bizhawk_co_op") => Some(Impl::BizHawkCoOp),
                Some("midos_house") => Some(Impl::MidosHouse),
                _ => None,
            },
            Self::Values { .. } => None,
        }
    }

    pub(crate) fn restream_consent(&self) -> Option<bool> {
        match self {
            Self::Context(ctx) => match ctx.field_value("restream_consent_radio") {
                Some("yes") => Some(true),
                Some("no") => Some(false),
                _ => None,
            },
            Self::Values { .. } => None,
        }
    }
}

pub(crate) async fn find_team_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, FindTeamError> {
    let header = data.header(&mut transaction, me.as_ref(), Tab::FindTeam, false).await?;
    let mut me_listed = false;
    let mut looking_for_team = Vec::default();
    for row in sqlx::query!(r#"SELECT user_id AS "user: Id<Users>", availability, notes FROM looking_for_team WHERE series = $1 AND event = $2"#, data.series as _, &data.event).fetch_all(&mut *transaction).await? {
        let user = User::from_id(&mut *transaction, row.user).await?.ok_or(FindTeamError::UnknownUser)?;
        if me.as_ref().is_some_and(|me| user.id == me.id) { me_listed = true }
        looking_for_team.push((user, row.availability, row.notes));
    }
    let form = if me.is_some() {
        let mut errors = ctx.errors().collect_vec();
        if me_listed {
            None
        } else {
            Some(full_form(uri!(event::find_team_post(data.series, &*data.event)), csrf, html! {
                @if data.is_single_race() {
                    legend {
                        : "Click this button to add yourself to the list below.";
                    }
                } else {
                    legend {
                        : "Fill out this form to add yourself to the list below.";
                    }
                    : form_field("availability", &mut errors, html! {
                        label(for = "availability") : "Timezone/Availability/Commitment:";
                        input(type = "text", name = "availability", value? = ctx.field_value("availability"));
                    });
                    : form_field("notes", &mut errors, html! {
                        label(for = "notes") : "Any Other Notes?";
                        input(type = "text", name = "notes", value? = ctx.field_value("notes"));
                    });
                }
            }, errors, if data.is_single_race() { "Looking for Team" } else { "Submit" }))
        }
    } else {
        Some(html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(event::find_team(data.series, &*data.event)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to add yourself to this list.";
                }
            }
        })
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Find Teammates — {}", data.display_name), html! {
        : header;
        : form;
        table {
            thead {
                tr {
                    th : "User";
                    @if !data.is_single_race() {
                        th : "Timezone/Availability/Commitment";
                        th : "Notes";
                    }
                }
            }
            tbody {
                @if looking_for_team.is_empty() {
                    tr {
                        td(colspan = if data.is_single_race() { "1" } else { "3" }) {
                            i : "(no one currently looking for teammates)";
                        }
                    }
                } else {
                    @for (user, availability, notes) in looking_for_team {
                        tr {
                            td : user;
                            @if !data.is_single_race() {
                                td : availability;
                                td : notes;
                            }
                        }
                    }
                }
            }
        }
    }).await?)
}

pub(crate) fn async_rules(data: &Data<'_>, async_kind: AsyncKind) -> RawHtml<String> {
    html! {
        p : "Rules:";
        ol {
            @match async_kind {
                AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 => li : "In order to play in the tournament, your team must make a reasonable attempt at completing this seed. In the event of a forfeit, you can still participate, but will be considered the bottom seed for settings draft purposes.";
                AsyncKind::Tiebreaker1 => li : "In order to play in the top 8 bracket, your team must make a reasonable attempt at completing this seed. In the event of a forfeit, you can still participate, but will be placed at the bottom of your Swiss point group for matchup and settings draft purposes.";
                AsyncKind::Tiebreaker2 => li : "In order to play in the top 8 bracket, your team must race the other teams of your Swiss point group on this seed.";
            }
            @if let AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 = async_kind {
                li {
                    @if let Some(base_start) = data.base_start {
                        : "The time must be submitted by ";
                        : format_datetime(base_start, DateTimeFormat { long: true, running_text: true });
                        : ". In the event that an odd number of teams is qualified at the time of the deadline, one additional team may qualify within 24 hours.";
                    } else {
                        : "The time must be submitted by the starting time of the tournament, which is yet to be announced.";
                    }
                }
            } else {
                //TODO give deadline of tiebreaker async
            }
            li : "You must start the seed within 30 minutes of obtaining it and submit your time within 30 minutes of the last finish. Any additional time taken will be added to your final time. If anything prevents you from obtaining the seed/submitting your time, please DM an admin (or ping the Discord role) to get it sorted out.";
            @if let AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 = async_kind {
                li : "While required for the tournament, the results from the qualifier seed will only determine which team chooses who goes first in the settings draft. Swiss pairings will be seeded randomly.";
                li : "While you are not strictly required to stream, you must have video proof of your run. Feel free to simply record your run and upload it to YouTube and provide a link. If you do stream or make your upload public, please make sure it is clearly marked so people can avoid spoilers. If you're a big streamer, be extra sure to note what is happening, as several of your viewers are likely going to want to participate as well.";
            } else {
                li : "The results from the tiebreaker seed will determine your seeding in the bracket, as well as which team chooses who goes first in the settings draft for the first race of each match.";
                li : "You must have video proof of your run. Streaming is allowed but discouraged. Feel free to simply record your run and upload it to YouTube and provide a link. If you do stream or make your upload public, please make sure is is clearly marked so people can avoid spoilers.";
            }
            li : "Do not spoil yourself on this seed by watching another playthrough. If you do stream, you are responsible for what your chat says, so either do not read chat, set it to emote only, or take the risk at your own discretion. If you do get spoiled, please report it to the admins, we will try to work out something equitable.";
            li {
                : "You must use the world numbers with which you entered the tournament for this seed.";
                @if let AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 = async_kind {
                    : " Once you request the seed, the world numbers you selected are the world numbers you play with for the rest of the tournament. If you wish to change your player order, do not request the qualifier and contact an admin."; //TODO allow changing player order in options below
                }
            }
            li {
                : "This should be run like an actual race. In the event of a technical issue, teams are allowed to invoke the ";
                a(href = "https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub") : "Fair Play Agreement";
                : " and have up to a 15 minute time where the affected runner can try to catch back up. If you do this, you must fill out the appropriate field when submitting your time so it can be authenticated.";
            }
        }
    }
}
