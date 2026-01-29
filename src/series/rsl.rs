use {
    semver::Version,
    serde_json::Value as Json,
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
    },
};

static REPO_LOCK: LazyLock<Mutex<()>> = LazyLock::new(Mutex::default);

#[derive(Debug, Default, Clone, Copy, Sequence, sqlx::Type)]
#[sqlx(type_name = "rsl_preset", rename_all = "lowercase")]
#[cfg_attr(unix, derive(Protocol))]
pub(crate) enum Preset {
    #[default]
    League,
    Beginner,
    Intermediate,
    Ddr,
    CoOp,
    Multiworld,
}

impl Preset {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::League => "league",
            Self::Beginner => "beginner",
            Self::Intermediate => "intermediate",
            Self::Ddr => "ddr",
            Self::CoOp => "coop",
            Self::Multiworld => "multiworld",
        }
    }

    pub(crate) fn race_info(&self) -> &'static str {
        match self {
            Self::League => "Random Settings League",
            Self::Beginner => "RSL-Lite",
            Self::Intermediate => "Intermediate Random Settings",
            Self::Ddr => "Random Settings DDR",
            Self::CoOp => "Random Settings Co-Op",
            Self::Multiworld => "Random Settings Multiworld",
        }
    }
}

impl FromStr for Preset {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match &*s.to_ascii_lowercase() {
            "league" | "rsl" | "solo" | "advanced" => Self::League,
            "beginner" | "lite" => Self::Beginner,
            "intermediate" => Self::Intermediate,
            "ddr" => Self::Ddr,
            "coop" | "co-op" => Self::CoOp,
            "multiworld" | "mw" => Self::Multiworld,
            _ => return Err(()),
        })
    }
}

#[derive(Default)]
pub(crate) enum DevFenhlPreset {
    #[default]
    Fenhl,
    Pictionary,
}

impl DevFenhlPreset {
    fn name(&self) -> &'static str {
        match self {
            Self::Fenhl => "fenhl",
            Self::Pictionary => "pictionary",
        }
    }
}

impl FromStr for DevFenhlPreset {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match &*s.to_ascii_lowercase() {
            "fenhl" => Self::Fenhl,
            "pic" | "pictionary" => Self::Pictionary,
            _ => return Err(()),
        })
    }
}

pub(crate) enum VersionedPreset {
    Xopar {
        version: Option<Version>,
        preset: Preset,
    },
    XoparCustom {
        version: Option<Version>,
        weights: Weights,
    },
    Fenhl {
        version: Option<(Version, u8)>,
        preset: DevFenhlPreset,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ScriptPathError {
    #[error(transparent)] GitConnect(#[from] gix::remote::connect::Error),
    #[error(transparent)] GitFetch(#[from] gix::remote::fetch::Error),
    #[error(transparent)] GitFindRemote(#[from] gix::remote::find::for_fetch::Error),
    #[error(transparent)] GitOpen(#[from] gix::open::Error),
    #[error(transparent)] GitPrepareFetch(#[from] gix::remote::fetch::prepare::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[cfg(unix)]
    #[error("RSL script not found")]
    NotFound,
    #[cfg(unix)]
    #[error("base rom not found")]
    RomPath,
}

impl VersionedPreset {
    #[cfg(unix)] pub(crate) fn new_unversioned(branch: &str, preset: Option<&str>) -> Result<Self, ()> {
        Ok(match branch {
            "xopar" => Self::Xopar { version: None, preset: preset.map(Preset::from_str).transpose()?.unwrap_or_default() },
            "fenhl" => Self::Fenhl { version: None, preset: preset.map(DevFenhlPreset::from_str).transpose()?.unwrap_or_default() },
            _ => return Err(()),
        })
    }

    #[cfg(unix)] pub(crate) fn new_versioned(version: ootr_utils::Version, preset: Option<&str>) -> Result<Self, ()> {
        Ok(match version.branch() {
            ootr_utils::Branch::DevR | ootr_utils::Branch::DevRob => Self::Xopar { version: Some(version.base().clone()), preset: preset.map(Preset::from_str).transpose()?.unwrap_or_default() },
            ootr_utils::Branch::DevFenhl => Self::Fenhl { version: Some((version.base().clone(), version.supplementary().unwrap())), preset: preset.map(DevFenhlPreset::from_str).transpose()?.unwrap_or_default() },
            _ => return Err(()),
        })
    }

    pub(crate) fn base_version(&self) -> Option<&Version> {
        match self {
            Self::Xopar { version, .. } | Self::XoparCustom { version, .. } => version.as_ref(),
            Self::Fenhl { version, .. } => version.as_ref().map(|(base, _)| base),
        }
    }

    pub(crate) fn name_or_weights(&self) -> Either<&'static str, &Weights> {
        match self {
            Self::Xopar { preset, .. } => Either::Left(preset.name()),
            Self::Fenhl { preset, .. } => Either::Left(preset.name()),
            Self::XoparCustom { weights, .. } => Either::Right(weights),
        }
    }

    fn is_version_locked(&self) -> bool {
        match self {
            Self::Xopar { version, .. } | Self::XoparCustom { version, .. } => version.is_some(),
            Self::Fenhl { version, .. } => version.is_some(),
        }
    }

    pub(crate) async fn script_path(&self) -> Result<Cow<'static, Path>, ScriptPathError> {
        let path = {
            #[cfg(unix)] {
                match self {
                    Self::Fenhl { version: None, .. } => Cow::Borrowed(Path::new("/opt/git/github.com/fenhl/plando-random-settings/main")),
                    Self::Fenhl { version: Some((base, supplementary)), .. } => Cow::Owned(BaseDirectories::new().find_data_file(Path::new("midos-house").join(format!("rsl-dev-fenhl-{base}-{supplementary}"))).ok_or(ScriptPathError::NotFound)?),
                    Self::Xopar { version: None, .. } | Self::XoparCustom { version: None, .. } => Cow::Owned(BaseDirectories::new().find_data_file("fenhl/rslbot/plando-random-settings").ok_or(ScriptPathError::NotFound)?),
                    Self::Xopar { version: Some(version), .. } | Self::XoparCustom { version: Some(version), .. } => Cow::Owned(BaseDirectories::new().find_data_file(Path::new("midos-house").join(format!("rsl-{version}"))).ok_or(ScriptPathError::NotFound)?),
                }
            }
            #[cfg(windows)] {
                match self {
                    Self::Fenhl { .. } => Cow::Borrowed(Path::new("C:/Users/fenhl/git/github.com/fenhl/plando-random-settings/main")), //TODO respect script version field
                    Self::Xopar { .. } | Self::XoparCustom { .. } => Cow::Borrowed(Path::new("C:/Users/fenhl/git/github.com/matthewkirby/plando-random-settings/main")), //TODO respect script version field
                }
            }
        };
        if fs::exists(&path).await? {
            if !self.is_version_locked() {
                // update the RSL script
                lock!(rsl_repo_lock = REPO_LOCK; { // git is not reentrant
                    let repo = gix::open(path.to_owned())?;
                    let branch_name = match self {
                        Self::Xopar { .. } | Self::XoparCustom { .. } => "release",
                        Self::Fenhl { .. } => "dev-fenhl",
                    };
                    repo.find_fetch_remote(Some("origin".into()))?
                        .connect(gix::remote::Direction::Fetch)?
                        .prepare_fetch(gix::progress::Discard, Default::default())?
                        .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(NonZero::<u32>::MIN))
                        .receive(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
                    Command::new("git").arg("reset").arg("--hard").arg(format!("origin/{branch_name}")).current_dir(&path).check("git reset").await?; //TODO use gix, blocked on https://github.com/GitoxideLabs/gitoxide/issues/301
                });
            }
        } else {
            if self.is_version_locked() {
                unimplemented!("clone version-locked RSL script repo") //TODO
            } else {
                #[cfg(unix)] {
                    fs::create_dir_all(path.parent().expect("RSL script repo at file system root")).await?;
                    let mut cmd = Command::new("git");
                    cmd.arg("clone");
                    cmd.arg("--depth=1");
                    cmd.arg(format!("https://github.com/{}/plando-random-settings.git", match self {
                        Self::Xopar { .. } | Self::XoparCustom { .. } => "matthewkirby",
                        Self::Fenhl { .. } => "fenhl",
                    }));
                    match self {
                        Self::Xopar { .. } | Self::XoparCustom { .. } => { cmd.arg("--branch=release"); }
                        Self::Fenhl { .. } => {}
                    }
                    cmd.arg(&*path).check("git clone").await?;
                    let rsl_data_dir = path.join("data");
                    fs::create_dir_all(&rsl_data_dir).await?;
                    fs::copy(BaseDirectories::new().find_data_file(Path::new("midos-house").join("oot-ntscu-1.0.z64")).ok_or(ScriptPathError::RomPath)?, rsl_data_dir.join("oot-ntscu-1.0.z64")).await?; //TODO decompress?
                }
                #[cfg(not(unix))] { unimplemented!("clone RSL script repo on Windows") } //TODO
            }
        }
        Ok(path)
    }
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct Weights {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    options: HashMap<String, Json>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    conditionals: HashMap<String, Vec<Json>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    multiselect: HashMap<String, u8>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) weights: HashMap<String, HashMap<String, usize>>,
}

#[derive(Deserialize)]
pub(crate) struct Leaderboard {
    pub(crate) metadata: LeaderboardMetadata,
    pub(crate) qualified: Vec<LeaderboardPlayer>,
}

impl Leaderboard {
    pub(crate) async fn get(http_client: &reqwest::Client) -> wheel::Result<Self> {
        http_client.get("https://rsl.one/api/leaderboard")
            .send().await?
            .detailed_error_for_status().await?
            .json_with_text_in_error().await
    }
}

#[derive(Deserialize)]
pub(crate) struct LeaderboardMetadata {
    pub(crate) required_races: u32,
    pub(crate) season: String,
}

#[derive(Deserialize)]
pub(crate) struct LeaderboardPlayer {
    pub(crate) userid: String,
}

pub(crate) struct ForceOffSetting {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) lite: bool,
    ban: Option<fn(&mut Weights)>,
}

pub(crate) const FORCE_OFF_SETTINGS: [ForceOffSetting; 17] = [
    ForceOffSetting { name: "triforce_hunt", display: "Triforce Hunt", lite: true, ban: None },
    ForceOffSetting { name: "shuffle_boulders", display: "Boulder Shuffle", lite: true, ban: None },
    ForceOffSetting { name: "shuffle_individual_ocarina_notes", display: "Ocarina Note Shuffle", lite: false, ban: None },
    ForceOffSetting { name: "split_collectible_bridge_conditions", display: "Token/Heart Bridge & GBK", lite: true, ban: Some(|weights| { weights.conditionals.insert(format!("split_collectible_bridge_conditions"), vec![json!(false)]); }) },
    ForceOffSetting { name: "shuffle_frog_song_rupees", display: "Frog Song Rupees", lite: true, ban: None },
    ForceOffSetting { name: "select_one_pots_crates_freestanding", display: "Pots & Crates & Freestanding", lite: false, ban: Some(|weights| { weights.conditionals.insert(format!("select_one_pots_crates_freestanding"), vec![json!(false)]); }) },
    ForceOffSetting { name: "shuffle_bosses", display: "Boss Door ER", lite: true, ban: Some(|weights| weights.weights.get_mut("shuffle_bosses").unwrap().retain(|key, _| key == "off")) },
    ForceOffSetting { name: "adult_trade_shuffle", display: "Adult Trade Shuffle", lite: true, ban: None },
    ForceOffSetting { name: "key_rings", display: "Keyrings", lite: true, ban: Some(|weights| { weights.multiselect.insert(format!("key_rings"), 0); }) },
    ForceOffSetting { name: "geometrically_draw_dungeon_shortcuts", display: "Dungeon Shortcuts", lite: true, ban: Some(|weights| { weights.conditionals.insert(format!("geometrically_draw_dungeon_shortcuts"), vec![json!(false)]); }) },
    ForceOffSetting { name: "keyring_give_bk", display: "Keyrings give BK", lite: true, ban: None },
    ForceOffSetting { name: "shuffle_overworld_entrances", display: "Overworld ER", lite: false, ban: None },
    ForceOffSetting { name: "mix_entrance_pools", display: "Mix Entrance Pools", lite: true, ban: Some(|weights| { weights.multiselect.insert(format!("mix_entrance_pools"), 0); }) },
    ForceOffSetting { name: "blue_fire_arrows", display: "Blue Fire Arrows", lite: true, ban: None },
    ForceOffSetting { name: "no_epona_race", display: "Epona Race", lite: false, ban: Some(|weights| { weights.weights.get_mut("no_epona_race").unwrap().remove("false"); }) },
    ForceOffSetting { name: "no_escape_sequence", display: "Collapse", lite: false, ban: Some(|weights| { weights.weights.get_mut("no_escape_sequence").unwrap().remove("false"); }) },
    ForceOffSetting { name: "shuffle_expensive_merchants", display: "Expensive Merchants", lite: true, ban: None },
];

pub(crate) struct MultiOptionSetting {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) options: &'static [(&'static str, &'static str, bool, Option<fn(&mut Weights)>)],
}

pub(crate) const FIFTY_FIFTY_SETTINGS: [MultiOptionSetting; 18] = [
    MultiOptionSetting { name: "complete_mask_quest", display: "Complete Mask Quest", options: &[("true", "Complete Mask Quest", true, None), ("false", "Complete Mask Quest Off", true, None)] },
    MultiOptionSetting { name: "free_bombchu_drops", display: "Free Bombchu Drops", options: &[("true", "Free Bombchu Drops", true, None), ("false", "Free Bombchu Drops Off", true, None)] },
    MultiOptionSetting { name: "free_scarecrow", display: "Free Scarecrow", options: &[("true", "Free Scarecrow", true, None), ("false", "Free Scarecrow Off", true, None)] },
    MultiOptionSetting { name: "open_door_of_time", display: "Closed Door of Time", options: &[("false", "Closed Door of Time", true, None), ("true", "Open Door of Time", true, None)] },
    MultiOptionSetting { name: "open_forest", display: "Closed Forest", options: &[("open", "Open Forest", true, None), ("closed_deku", "Closed Deku", true, None)] },
    MultiOptionSetting { name: "owl_drops", display: "Owl Drops", options: &[("true", "Owl Drops", true, None), ("false", "Owl Drops Off", true, None)] },
    MultiOptionSetting { name: "reachable_locations", display: "Reachable Locations", options: &[("all", "All Locations Reachable", true, None), ("beatable", "Only Required Locations Reachable", true, None)] },
    MultiOptionSetting { name: "ruto_already_f1_jabu", display: "Ruto already at F1", options: &[("true", "Ruto already at F1", true, None), ("false", "Ruto already at F1 Off", true, None)] },
    MultiOptionSetting { name: "shuffle_beans", display: "Shuffle Beans", options: &[("true", "Shuffle Beans", true, None), ("false", "Shuffle Beans Off", true, None)] },
    MultiOptionSetting { name: "shuffle_cows", display: "Shuffle Cows", options: &[("true", "Shuffle Cows", true, None), ("false", "Shuffle Cows Off", true, None)] },
    MultiOptionSetting { name: "shuffle_gerudo_card", display: "Shuffle Gerudo Card", options: &[("true", "Shuffle Gerudo Card", true, None), ("false", "Shuffle Gerudo Card Off", true, None)] },
    MultiOptionSetting { name: "shuffle_kokiri_sword", display: "Shuffle Kokiri Sword", options: &[("true", "Shuffle Kokiri Sword", true, None), ("false", "Shuffle Kokiri Sword Off", false, None)] },
    MultiOptionSetting { name: "shuffle_ocarinas", display: "Shuffle Ocarina", options: &[("true", "Shuffle Ocarina", true, None), ("false", "Shuffle Ocarina Off", true, None)] },
    MultiOptionSetting { name: "start_with_consumables", display: "Start with Consumables", options: &[("true", "Start with Consumables", true, None), ("false", "Start with Consumables Off", true, None)] },
    MultiOptionSetting { name: "start_with_rupees", display: "Start with Rupees", options: &[("true", "Start with Rupees", true, None), ("false", "Start with Rupees Off", true, None)] },
    MultiOptionSetting { name: "warp_songs", display: "Random Warp Songs", options: &[("true", "Random Warp Songs", true, None), ("false", "Random Warp Songs Off", true, None)] },
    MultiOptionSetting { name: "zora_fountain", display: "Zora's Fountain", options: &[("open", "Open Fountain", true, None), ("closed", "Closed Fountain", true, None)] },
    MultiOptionSetting { name: "shuffle_grotto_entrances", display: "Grotto ER", options: &[("true", "Grotto ER", true, None), ("false", "Grotto ER Off", true, None)] },
];

pub(crate) const MULTI_OPTION_SETTINGS: [MultiOptionSetting; 19] = [
    MultiOptionSetting { name: "bridge", display: "Rainbow Bridge", options: &[("open", "Open", true, None)] },
    MultiOptionSetting { name: "shuffle_ganon_bosskey", display: "Ganon BK", options: &[("remove", "Remove", true, None)] },
    MultiOptionSetting { name: "correct_chest_appearances", display: "Correct Chest Appearance", options: &[("textures", "Textures", false, None), ("both", "Both", true, None), ("off", "Off", true, None)] },
    MultiOptionSetting { name: "damage_multiplier", display: "Damage Multiplier", options: &[("quadruple", "Quadruple", false, None), ("double", "Double", true, None), ("normal", "Normal", true, None), ("half", "Half", true, None)] },
    MultiOptionSetting { name: "gerudo_fortress", display: "Gerudo Fortress Guards", options: &[("open", "Open", true, None), ("fast", "Fast", true, None), ("normal", "Normal", true, None)] },
    MultiOptionSetting { name: "item_pool_value", display: "Item Pool", options: &[("minimal", "Minimal", false, None), ("scarce", "Scarce", true, None), ("balanced", "Balanced", true, None), ("plentiful", "Plentiful", true, None)] },
    MultiOptionSetting { name: "shuffle_dungeon_entrances", display: "Dungeon ER", options: &[("all_simple", "All + Simple", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_dungeon_entrances").unwrap(); weight.remove("all"); weight.remove("simple"); })), ("all_off", "All + Off", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_dungeon_entrances").unwrap(); weight.remove("all"); weight.remove("off"); })), ("all", "All", true, None)] },
    MultiOptionSetting { name: "shuffle_interior_entrances", display: "Indoor ER", options: &[("all_simple", "All + Simple", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_interior_entrances").unwrap(); weight.remove("all"); weight.remove("simple"); })), ("off", "Off", true, None)] },
    MultiOptionSetting { name: "shopsanity", display: "Shop Items", options: &[("off_0_1", "Off + 0 + 1", true, Some(|weights| { let weight = weights.weights.get_mut("shopsanity").unwrap(); weight.remove("off"); weight.remove("0"); weight.remove("1"); })), ("random", "Random", false, None), ("2_3_4", "2 + 3 + 4", true, Some(|weights| { let weight = weights.weights.get_mut("shopsanity").unwrap(); weight.remove("2"); weight.remove("3"); weight.remove("4"); }))] },
    MultiOptionSetting { name: "shopsanity_prices", display: "Shop Prices", options: &[("giant_random", "Giant + Random", true, Some(|weights| { let weight = weights.weights.get_mut("shopsanity_prices").unwrap(); weight.remove("random_giant"); weight.remove("random"); })), ("adult", "Adult", true, Some(|weights| { weights.weights.get_mut("shopsanity_prices").unwrap().remove("random_adult"); })), ("starting", "Starting", true, Some(|weights| { weights.weights.get_mut("shopsanity_prices").unwrap().remove("random_starting"); })), ("affordable", "Affordable", true, None)] },
    MultiOptionSetting { name: "shuffle_scrubs", display: "Scrub Shuffle", options: &[("random_regular", "Random + Regular", false, Some(|weights| { let weight = weights.weights.get_mut("shuffle_scrubs").unwrap(); weight.remove("random"); weight.remove("regular"); })), ("low", "Low", true, None), ("off", "Off", true, None)] },
    MultiOptionSetting { name: "tokensanity", display: "Token Shuffle", options: &[("all_dungeons", "All + Dungeon", true, Some(|weights| { let weight = weights.weights.get_mut("tokensanity").unwrap(); weight.remove("all"); weight.remove("dungeons"); })), ("all_overworld", "All + Overworld", false, Some(|weights| { let weight = weights.weights.get_mut("tokensanity").unwrap(); weight.remove("all"); weight.remove("overworld"); })), ("off", "Off", true, None)] },
    MultiOptionSetting { name: "shuffle_tcgkeys", display: "Treasure Chest Game Keys", options: &[("keysanity", "Keysanity", true, None), ("keysanity_regional", "Keysanity + Regional", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_tcgkeys").unwrap(); weight.remove("keysanity"); weight.remove("regional"); }))] },
    MultiOptionSetting { name: "shuffle_song_items", display: "Song Shuffle", options: &[("dungeon", "Songs On Dungeon", true, None), ("any", "Anywhere", true, None), ("song", "Songs on songs", true, None)] },
    MultiOptionSetting { name: "shuffle_enemy_spawns", display: "Enemy Souls", options: &[("all", "All", false, None), ("all_regional_bosses", "All + Regional + Boss", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_enemy_spawns").unwrap(); weight.remove("all"); weight.remove("regional"); weight.remove("bosses"); }))] },
    MultiOptionSetting { name: "shuffle_smallkeys", display: "Small Keys", options: &[("dungeon_vanilla_remove", "Own Dungeon + Vanilla + Remove", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_smallkeys").unwrap(); weight.remove("dungeon"); weight.remove("vanilla"); weight.remove("remove"); })), ("regional", "Regional", true, None), ("anydungeon_overworld_keysanity", "Any Dungeon + Overworld + Keysanity", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_smallkeys").unwrap(); weight.remove("any_dungeon"); weight.remove("overworld"); weight.remove("keysanity"); }))] },
    MultiOptionSetting { name: "shuffle_bosskeys", display: "Boss Keys", options: &[("dungeon_vanilla_remove", "Own Dungeon + Vanilla + Remove", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_smallkeys").unwrap(); weight.remove("dungeon"); weight.remove("vanilla"); weight.remove("remove"); })), ("regional", "Regional", true, None), ("keysanity", "Keysanity", false, None)] },
    MultiOptionSetting { name: "shuffle_silver_rupees", display: "Silver Rupees Shuffle", options: &[("anywhere", "Anywhere", true, None), ("anywhere_dungeon_regional_remove", "Anywhere + Dungeon + Regional + Remove", true, Some(|weights| { let weight = weights.weights.get_mut("shuffle_silver_rupees").unwrap(); weight.remove("anywhere"); weight.remove("dungeon"); weight.remove("regional"); weight.remove("remove"); }))] },
    MultiOptionSetting { name: "trials", display: "Trials", options: &[("0_1", "0 + 1", true, Some(|weights| { weights.weights.insert(format!("trials_random"), collect![format!("false") => 1]); let weight = weights.weights.get_mut("trials").unwrap(); weight.remove("0"); weight.remove("1"); })), ("2_3_4", "2 + 3 + 4", true, Some(|weights| { weights.weights.insert(format!("trials_random"), collect![format!("false") => 1]); let weight = weights.weights.get_mut("trials").unwrap(); weight.remove("2"); weight.remove("3"); weight.remove("4"); })), ("5_6", "5 + 6", false, Some(|weights| { weights.weights.insert(format!("trials_random"), collect![format!("false") => 1]); let weight = weights.weights.get_mut("trials").unwrap(); weight.remove("5"); weight.remove("6"); }))] },
];

pub(crate) fn display_s7_draft_picks(picks: &draft::Picks) -> String {
    format!(
        "{} and {}",
        if picks.get("preset").map(|preset| &**preset).unwrap_or("league") == "lite" { "RSL-Lite weights" } else { "RSL weights" },
        English.join_str_opt(
            FORCE_OFF_SETTINGS.into_iter()
                .filter(|ForceOffSetting { name, .. }| picks.get(*name).is_some_and(|pick| pick == "banned"))
                .map(|ForceOffSetting { display, .. }| Cow::Borrowed(display))
            .chain(FIFTY_FIFTY_SETTINGS.into_iter().chain(MULTI_OPTION_SETTINGS)
                .flat_map(|MultiOptionSetting { name, options, display }| options.into_iter()
                    .filter(|(option_name, _, _, _)| picks.get(name).is_some_and(|pick| pick.split(',').any(|pick| pick == *option_name)))
                    .map(move |(_, option_display, _, _)| Cow::Owned(format!("{display}: {option_display}")))
                )
            )
        ).map(|bans| format!("the following bans: {bans}")).unwrap_or_else(|| format!("no bans"))
    )
}

pub(crate) async fn resolve_s7_draft_weights(script_path: &Path, picks: &draft::Picks) -> Result<Weights, draft::Error> {
    let is_lite = picks.get("preset").map(|preset| &**preset).unwrap_or("league") == "lite";
    let mut weights = fs::read_json::<Weights>(script_path.join("weights").join("rsl_season7.json")).await?;
    weights.weights.insert(format!("password_lock"), collect![format!("true") => 1, format!("false") => 0]);
    if is_lite {
        let ovr = fs::read_json::<Weights>(script_path.join("weights").join("beginner_override.json")).await?;
        for (option, value) in ovr.options {
            if let Some(option) = option.strip_prefix("extra_") {
                match weights.options.entry(option.to_owned()) {
                    hash_map::Entry::Occupied(mut entry) => match (entry.get_mut(), value) {
                        (Json::Object(entry), Json::Object(value)) => entry.extend(value),
                        (Json::Array(entry), Json::Array(value)) => {
                            entry.extend(value);
                            entry.sort_unstable_by(|v1, v2| v1.as_str().cmp(&v2.as_str()));
                            entry.dedup();
                        }
                        (_, _) => return Err(draft::Error::RslExtraType),
                    },
                    hash_map::Entry::Vacant(entry) => { entry.insert(value); }
                }
            } else if let Some(option) = option.strip_prefix("remove_") {
                if let Some(entry) = weights.options.get_mut(option) {
                    let value = value.as_array().ok_or(draft::Error::RslRemoveType)?;
                    entry.as_array_mut().ok_or(draft::Error::RslRemoveType)?.retain(|item| !value.contains(item));
                }
            } else {
                weights.options.insert(option, value);
            }
        }
        weights.conditionals.extend(ovr.conditionals);
        weights.multiselect.extend(ovr.multiselect);
        weights.weights.extend(ovr.weights);
    }
    for ForceOffSetting { name, ban, .. } in FORCE_OFF_SETTINGS {
        if picks.get(name).is_some_and(|pick| pick == "banned") {
            if let Some(ban) = ban {
                ban(&mut weights);
            } else {
                weights.weights.get_mut(name).unwrap().remove("true");
            }
        }
    }
    for MultiOptionSetting { name, options, .. } in FIFTY_FIFTY_SETTINGS.into_iter().chain(MULTI_OPTION_SETTINGS) {
        for (option, _, _, ban) in options {
            if picks.get(name).is_some_and(|pick| pick.split(',').contains(option)) {
                if let Some(ban) = ban {
                    ban(&mut weights);
                } else {
                    weights.weights.get_mut(name).unwrap().remove(*option);
                }
            }
        }
    }
    Ok(weights)
}

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "1" => Some(html! {
            article {
                p {
                    : "This is an archive of the 1st season of the Random Settings League tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1wmoZHdwYijJwXLYgZbadjRYOGBNXio2hhKEIkFNgDgU/edit") : "the official document";
                    : " for details.";
                }
            }
        }),
        "2" => Some(html! {
            article {
                p {
                    : "This is an archive of the 2nd season of the Random Settings League tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://rsl-leaderboard.web.app/season2") : "Leaderboard (qualifiers)";
                    }
                }
            }
        }),
        "3" => Some(html! {
            article {
                p {
                    : "This is an archive of the 3rd season of the Random Settings League tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://rsl-leaderboard.web.app/season3") : "Leaderboard (qualifiers)";
                    }
                }
            }
        }),
        "4" => Some(html! {
            article {
                p {
                    : "This is an archive of the 4th season of the Random Settings League tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://rsl-leaderboard.web.app/season4") : "Leaderboard (qualifiers)";
                    }
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/1IyXCCq0iowzCoUH7mB8oSduiQU6QqLY6LE1nJEKUOMs/edit") : "Swiss pairings";
                    }
                }
            }
        }),
        "5" => Some(html! {
            article {
                p {
                    : "This is the 5th season of the Random Settings League tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1Js03yFcMw_mWx4UO_3UJB39CNCKa0bsxlBEYrHPq5Os/edit") : "the official document";
                    : " for details.";
                }
            }
        }),
        "6" => Some(html! {
            article {
                p {
                    : "This is the 6th season of the Random Settings League tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1xpZIVh6znG7mgyEUQk8J2B-_5PfbcERen-P4tDX6VqE/edit") : "the official document";
                    : " for details.";
                }
            }
        }),
        "7" => Some(html! {
            article {
                p {
                    : "Welcome to the 7th season of RSL, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : "!";
                }
                h2 : "Signing up";
                p : "Deadline for signing up is June 7th! The first round will start on Monday the 9th of June.";
                h2 : "Tournament format";
                p : "The tournament structure will begin with swiss rounds followed by a single elimination top 8 bracket. Semifinals and finals will be best of 3.";
                p : "The number of swiss rounds will be determined by the number of entrants. We are aiming for 5-6 rounds.";
                p : "To incorporate the leaderboard for this season, the higher seeded person will get the option to ban first. More on bans/blocks see below.";
                h2 : "RSL vs. RSL-Lite";
                p {
                    : "New to this year is the introduction of RSL-Lite! On sign-up, players will have the option to opt in for RSL-Lite, a version of RSL with a simpler set of setting weights. If both players have opted in for RSL-Lite, Mido will offer an option to play with the RSL-Lite weights instead of normal RSL weights. For more information about the weights for RLS and RSL-Lite please check ";
                    a(href = "https://rsl.one/weights") : "https://rsl.one/weights";
                }
                : "Bans and Blocks";
                p : "This year we will be implementing a draft feature for the matches. Players can ban and block settings. The blocking and banning will be handled by Mido. Mido will open up scheduling threads where you can put in commands to ban and block.For more information on bans and blocks please check the references at the bottom.";
                p {
                    : "For RSL the following structure will be used:";
                    br;
                    : "Player 1 ban";
                    br;
                    : "Player 2 ban and block";
                    br;
                    : "Player 1 block and ban";
                    br;
                    : "Player 2 ban";
                }
                p {
                    : "For RSL-Lite it will be as followed:";
                    br;
                    : "Player 1 ban";
                    br;
                    : "Player 2 ban";
                }
                h2 : "Gameplay Rules";
                p {
                    : "All races will be completed abiding by the Standard ruleset (";
                    a(href = "https://docs.google.com/document/d/1BbvHJF8vtyrte76jpoCVQBTy9MYStpN3vr2PLdiCIMk/edit") : "https://zsr.link/rsls6fpa";
                    : "), with a single exception:";
                }
                ul {
                    li : "Crossing the Gerudo Valley bridge as a child shall be banned unless it is from back to front.";
                }
                p : "Timing will end differently based on the gamemode:";
                ul {
                    li : "If it is a “Beat Ganon” seed, standard timing rules apply; .done is on the first frame of the cutscene that plays after beating Ganon.";
                    li : "If it is a “Triforce Hunt” seed, .done will be on the first frame the game fades completely to black after obtaining the last required piece.";
                }
                h2 : "Scheduling";
                p : "This year the scheduling will be handled by Mido. Mido will open up scheduling threads where players can pick their blocks/bans and players can schedule their matches. Mido will also open up a raceroom, post them in the thread, and roll the seed for players with the blocks/bans the players used.";
                h2 : "Breaks";
                p : "While breaks are not required, we encourage racers to organize mid-race breaks every 2 hours or so.";
                p : "If all players agree to a break, the players must pause their game when their timer reaches the agreed upon time and must remain paused until the break is over.";
                p : "If a player misses the start of the break, they must pause for the same duration as the agreed upon break, as soon as it becomes apparent.";
                p : "In the case you forgot a break, the time will be added to your final time. Do not intentionally do this and repeated instances may result in disciplinary action or removing your break privileges.";
                p {
                    : "You can use the following command in the raceroom before the race begins to get a reminder when to pause your game: ";
                    code : "!breaks 5 every 2h30";
                    : ", where you can set your own agreed upon length and interval.";
                }
                p : "Huge thank you to Fenhl for helping with the bans/blocks and making it possible to run this on Midos House. You are the best!";
                h2 : "References";
                p {
                    : "For more information about bans and blocks please check ";
                    a(href = "https://docs.google.com/document/d/1lJk1KzRG3gRhDr1oRZq-N81UbJ1e2nISICb8o2G2A_A/edit") : "https://zsr.link/RSLs7";
                    : " at the bottom of the page";
                }
                p {
                    : "For current standings and amount of seed completed check ";
                    a(href = "https://rsl.one/") : "https://rsl.one/";
                }
            }
        }),
        _ => None,
    })
}
