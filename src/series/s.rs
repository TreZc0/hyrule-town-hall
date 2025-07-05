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

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "w" => {
            let organizers = data.organizers(transaction).await?;
            let main_tournament_season = sqlx::query_scalar!("SELECT event FROM events WHERE series = 's'")
                .fetch_all(&mut **transaction).await?
                .into_iter()
                .filter_map(|event| event.parse::<u32>().ok())
                .max().expect("no main tournaments in database");
            let main_tournament = Data::new(transaction, Series::Standard, main_tournament_season.to_string()).await?.expect("database changed during transaction");
            let main_tournament_organizers = main_tournament.organizers(transaction).await?;
            let (main_tournament_organizers, race_mods) = organizers.into_iter().partition::<Vec<_>, _>(|organizer| main_tournament_organizers.contains(organizer));
            let now = Utc::now();
            Some(html! {
                article {
                    p {
                        : "The Standard weeklies are a set of community races organized by the race mods (";
                        : English.join_html_opt(race_mods);
                        : ") and main tournament organizers (";
                        : English.join_html_opt(main_tournament_organizers);
                        : ") in cooperation with ZeldaSpeedRuns. The races are open to all participants.";
                    }
                    p : "Starting from January 4, 2025, there will be alternating schedules:";
                    ol {
                        li {
                            : "The Kokiri weekly, Saturdays of week A at 6PM Eastern Time (next: ";
                            : format_datetime(WeeklyKind::Kokiri.next_weekly_after(now), DateTimeFormat { long: true, running_text: false });
                            : ")";
                        }
                        li {
                            : "The Goron weekly, Sundays of week A at 2PM Eastern Time (next: ";
                            : format_datetime(WeeklyKind::Goron.next_weekly_after(now), DateTimeFormat { long: true, running_text: false });
                            : ")";
                        }
                        li {
                            : "The Zora weekly, Saturdays of week B at 2PM Eastern Time (next: ";
                            : format_datetime(WeeklyKind::Zora.next_weekly_after(now), DateTimeFormat { long: true, running_text: false });
                            : ")";
                        }
                        li {
                            : "The Gerudo weekly, Sundays of week B at 9AM Eastern Time (next: ";
                            : format_datetime(WeeklyKind::Gerudo.next_weekly_after(now), DateTimeFormat { long: true, running_text: false });
                            : ")";
                        }
                    }
                    : long_weekly_settings();
                }
            })
        }
        "6" => Some(html! {
            article {
                p {
                    : "This is the 6th season of the main Ocarina of Time randomizer tournament. See ";
                    a(href = "https://docs.google.com/document/d/1Hkrh2A_szTUTgPqkzrqjSF0YWTtU34diLaypX9pyzUI/edit") : "the official document";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://challonge.com/ChallengeCupSeason6") : "Challenge Cup groups and bracket";
                    }
                }
            }
        }),
        "7" => Some(html! {
            article {
                p {
                    : "This is the main portion of the 7th season of the main Ocarina of Time randomizer tournament. See ";
                    a(href = "https://docs.google.com/document/d/1iN1q3NArRoQhean5W0qfoTSM2xLlj9QjuWkzDO0xME0/edit") : "the official document";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = uri!(event::info(Series::Standard, "7cc"))) : "Challenge Cup";
                    }
                }
            }
        }),
        "7cc" => Some(html! {
            article {
                p {
                    : "This is the Challenge Cup portion of the 7th season of the main Ocarina of Time randomizer tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1zMbko0OG0UKQ6Mvc48if9hJEU5svC-aM9xv3J_Lkzn0/edit") : "the official document";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = uri!(event::info(Series::Standard, "7"))) : "main bracket";
                    }
                }
            }
        }),
        "8" => Some(html! {
            div(class = "toc") {
                article {
                    h2 : "Welcome to the Ocarina of Time Randomizer Standard Tournament Season 8";
                    p : "The tournament will be hosted through a partnership between ZeldaSpeedRuns and The Silver Gauntlets to give 96 players a chance to participate in Season 8.";
                    p {
                        : "This event is organized by ";
                        : English.join_html_opt(data.organizers(transaction).await?);
                        : ". Please contact us if you have any questions or concerns. We can be reached by pinging the ";
                        strong : "@Tourney Organisation";
                        : " role on Discord.";
                    };
                    h2(id = "links") : "Important Links";
                    ul {
                        li {
                            a(href = "https://discord.gg/ootrandomizer") : "Ocarina of Time Randomizer Discord";
                        }
                        li {
                            a(href = "https://discord.gg/zsr") : "ZeldaSpeedRuns Discord";
                        }
                        li {
                            a(href = "https://discord.gg/qrGf6yNY4C") : "The Silver Gauntlets Discord";
                        }
                        li {
                            a(href = uri!(event::races(Series::Standard, "8"))) : "Qualifier Schedule";
                        }
                        li {
                            a(href = "https://www.start.gg/tournament/ocarina-of-time-randomizer-standard-tournament-season-8/event/main-tournament") : "Brackets";
                        }
                        li {
                            a(href = uri!(event::info(Series::Standard, "8cc"))) : "Challenge Cup";
                        }
                        li {
                            a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "OoTR Standard Racing Ruleset";
                        }
                        li {
                            a(href = "https://wiki.ootrandomizer.com/index.php?title=Rules#Universal_Rules") : "Universal Racing Rules";
                        }
                        li {
                            a(href = "https://docs.google.com/document/d/1BbvHJF8vtyrte76jpoCVQBTy9MYStpN3vr2PLdiCIMk/edit") : "Fair Play Agreement";
                        }
                        li {
                            a(href = "https://docs.google.com/document/d/1xJQ8DKFhBelfDSTih324h90mS1KNtugEf-b0O5hlsnw/edit") : "Hint Prioritization Document";
                        }
                        li {
                            : "Settings List: ";
                            strong {
                                : "see ";
                                a(href = "#all-settings") : "Appendix 1";
                            }
                        }
                        li {
                            : "Sometimes/Dual Hints: ";
                            strong {
                                : "see ";
                                a(href = "#sometimes-hints") : "Appendix 2";
                            }
                        }
                    }
                    h2(id = "format") : "Tournament Format";
                    p {
                        : "Season 8 will include a ";
                        strong : "qualifying stage";
                        : ", followed by a 1v1 format.";
                    }
                    p {
                        : "ZeldaSpeedRuns will be hosting the main tournament series. The ";
                        strong : "top 32";
                        : " players after the qualifiers will be eligible to participate in the next phase of the tournament, featuring a double-elimination bracket.";
                    }
                    p {
                        : "The Silver Gauntlets will be hosting the ";
                        strong : "Challenge Cup";
                        : ", a 64-player event that will include a ";
                        strong : "group stage and a bracket stage";
                        : ". The Challenge Cup is available to players ranked 33–96 after the qualifiers.";
                    }
                    p : "More information on the format of the qualifiers and both tournaments can be found below.";
                    h2(id = "ruleset") : "Ruleset";
                    p {
                        : "The Season 8 tournament will be operating under the Standard ruleset. You can find the ruleset here: ";
                        a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "OoTR Standard Racing Ruleset";
                    }
                    h2(id = "settings") : "Settings";
                    p : "Season 8 qualifiers and both tournaments will be played on OoTR version 8.2";
                    p : "The following list is an overview of the main settings for the tournament:";
                    ul {
                        li {
                            : "Vanilla Rainbow Bridge (requires Shadow ";
                            em : "and";
                            : " Spirit Medallion ";
                            em : "as well as";
                            : " Light Arrows)";
                        }
                        li : "Ganon's Castle boss key on 6 medallions";
                        li : "All Locations Reachable";
                        li : "Chest Appearance Matches Contents (CAMC)";
                        li : "Random child spawn | Vanilla adult spawn";
                        li : "Blue Fire Arrows: on";
                        li : "Cuccos: 3";
                    }
                    p {
                        strong {
                            : "To find a full list of the settings, please check ";
                            a(href = "#all-settings") : "Appendix 1";
                            : " at the end of this document.";
                        }
                    }
                    h2(id = "hints") : "Hint Distribution";
                    ul {
                        li {
                            : "5 x 2 Always hints: ";
                            em : "Skull Mask, Biggoron, Frogs 2, Burning Kakariko & Ocarina of Time";
                        }
                        li : "5 x 2 Goal (path) hints";
                        li : "3 x 1 Barren (foolish) hints at Temple of Time";
                        li : "1 x 1 Song hint at Temple of Time";
                        li : "2 x 2 Dual hints";
                        li : "6 x 2 Sometimes hints";
                        li : "Skull hints (30/40/50) in the House of Skulltula";
                    }
                    p {
                        strong {
                            : "To find a full list of the Sometimes & Dual hints for Season 8, check ";
                            a(href = "#sometimes-hints") : "Appendix 2";
                            : " at the end of this document.";
                        }
                    }
                    p {
                        strong : "Other notes on hints:";
                    }
                    ul {
                        li {
                            : "Zelda's Lullaby cannot be ";
                            em : "directly";
                            : " hinted on a Path";
                        }
                        li : "Maximum of 1 dungeon can be hinted as Barren";
                        li {
                            : "One-hint-per-goal enabled ";
                            em : "(see document below for details)";
                        }
                        li {
                            : "Song hint at the Temple of Time can only hint the following song locations:";
                            br;
                            em : "Royal Tomb, Adult Sacred Forest Meadow, Death Mountain Crater, Ice Cavern, Desert Colossus, and Temple of Time";
                        }
                    }
                    p {
                        strong {
                            : "Please also take a look at ";
                            a(href = "https://docs.google.com/document/d/1xJQ8DKFhBelfDSTih324h90mS1KNtugEf-b0O5hlsnw/edit") : "this document";
                            : " explaining the hint prioritization for the split win condition.";
                        }
                    }
                    h2(id = "password") : "Password Protection";
                    p : "As a new feature, this season will implement password protection to unlock the seed after patching. As soon as the countdown in the race room starts, the players will receive a 6-character passcode in the form of ocarina note buttons to enter in the file select menu. The players will then have 15 seconds (30 seconds in qualifiers) to enter the password correctly. Once the seed has been successfully unlocked, players must wait until the race timer hits zero before commencing the race.";
                    p {
                        a(href = "https://www.youtube.com/watch?v=PZLTvNjb_kg") : "Password Protection Demonstration Video";
                    }
                    h2(id = "qualifiers") : "Qualifiers";
                    p {
                        : "Anyone is free to enter the qualification races. There is no sign-up necessary to enter races or to have your points calculated on the leaderboard other than having a ";
                        a(href = "https://racetime.gg/") : "racetime.gg";
                        : " account. Please also make sure to read through the streaming rules further below.";
                    }
                    ul {
                        li : "There will be 20 total qualifiers";
                        li : "Only your first 8 races will count towards your score on the leaderboard";
                        li : "To be eligible for qualification, you need to complete a minimum of 5 non-zero qualifiers";
                        li : "Each player's highest score will be dropped, and 2nd through 5th scores will be used to calculate their overall points; any possible 6th through 8th scores will be dropped";
                        li : "Baseline for point calculation is 1000";
                        li : "Maximum amount of points per race is 1100";
                        li : "Minimum amount of points per race is 100";
                        li : "Forfeits will count as 0 points";
                    }
                    p {
                        : "Points will be calculated in the same manner as Seasons 4–7:";
                        br;
                        a(href = "https://docs.google.com/document/d/1IHrOGxFQpt3HpQ-9kQ6AVAARc04x6c96N1aHnHfHaKM/edit") : "Point Calculation Formula";
                        : " — Credit to mracsys";
                    }
                    p {
                        : "Dates for all Qualifiers can be found here: ";
                        a(href = uri!(event::races(Series::Standard, "8"))) : "Season 8 Qualifier Schedule";
                    }
                    p {
                        img(src = static_url!("event/s/8-qualifier-schedule.png"), style = "max-width: 100%; max-height: 100vh;");
                    }
                    p {
                        strong : "Note: Qualifiers will be force-started at the designated starting time. Any participant who is not ready at that time will be removed from the race. Race rooms will be automatically opened 1 hour before the scheduled start time by the Hyrule Town Hall bot. Mido will then provide a seed 15 minutes before the race is due to start.";
                    }
                    h2(id = "brackets") : "Brackets";
                    p {
                        : "After all 20 Qualification Races have concluded, the ";
                        strong : "top 32 players";
                        : " will qualify for the bracket stage of the main tournament and players ";
                        strong : "ranked 33–96";
                        : " will qualify for the Challenge Cup. If players are tied on points, the player with the highest individual point value from any of their eligible scoring races will win the tiebreaker. If it is still tied, the second-highest race will be considered next, and so on until the tie is broken.";
                    }
                    p : "If a player decides to opt out of the bracket phase of the tournament, their spot will go to the next person in line. This means that players ranked outside of the top 32 or 96 may still qualify for one of the tournaments, depending on players dropping out before the next phase begins. If you qualify for the main tournament, you cannot opt out to play in the Challenge Cup.";
                    p {
                        strong : "To opt in for the tournament, you are required to have a Hyrule Town Hall account.";
                        br;
                        : "The progress of the bracket will be organized on ";
                        a(href = "https://www.start.gg/tournament/ocarina-of-time-randomizer-standard-tournament-season-8/event/main-tournament") : "start.gg";
                        : ". A start.gg account is not required, but may optionally be provided to receive notifications about your matches.";
                    }
                    p : "Timeline for Season 8:";
                    ul {
                        li {
                            : "Qualifiers ";
                            : format_date_range(Utc.with_ymd_and_hms(2024, 11, 16, 19, 0, 0).single().expect("wrong hardcoded datetime"), Utc.with_ymd_and_hms(2024, 12, 22, 19, 0, 0).single().expect("wrong hardcoded datetime"));
                        }
                        li {
                            : "Opt-ins until ";
                            : format_datetime(Utc.with_ymd_and_hms(2024, 12, 27, 23, 59, 59).single().expect("wrong hardcoded datetime"), DateTimeFormat { long: true, running_text: true });
                        }
                        li {
                            : "Start of the bracket phase: ";
                            : format_datetime(Utc.with_ymd_and_hms(2024, 12, 28, 0, 0, 0).single().expect("wrong hardcoded datetime"), DateTimeFormat { long: true, running_text: true });
                        }
                    }
                    h3(id = "top32") : "Main Tournament";
                    ul {
                        li : "Players will face off in a 1v1 double-elimination bracket";
                        li : "Players will be seeded based on qualification rank";
                        li : "Players will have 2 weeks to schedule their matches in the Winner's bracket, and 1 week to schedule their matches in the Lower bracket";
                    }
                    h3(id = "cc") : "Challenge Cup";
                    ul {
                        li : "Players will face off in 1v1 races";
                        li : "Players will be seeded into 16 groups of 4 based on qualification rank";
                        li : "Players finishing in the top 2 of their group will advance to a 32-player single-elimination bracket (semifinals and finals will be best-of-3 matches)";
                    }
                    h3(id = "scheduling") : "Scheduling";
                    p {
                        : "Matches will be organized via the Hyrule Town Hall bot. To schedule your match, please use the ";
                        code : "/schedule";
                        : " command in your scheduling thread. A race room will automatically be opened 30 minutes before the designated time of the race. Mido will automatically roll the seed 15 minutes before the race is due to start. Unless the Fair Play Agreement (FPA) command has been utilized, the result will be automatically processed and posted in the #s8-results channel (after a 20-minute delay to avoid spoiling spectators).";
                    }
                    p {
                        : "The race can be rescheduled by using ";
                        code : "/schedule";
                        : " again, or removed from the schedule using ";
                        code : "/schedule-remove";
                        : ". If the room has already been opened or if there are any technical difficulties, please contact the tournament organizers.";
                    }
                    h3(id = "asyncs") : "Asynchronous Matches (asyncs)";
                    p {
                        : "To ensure we run a smooth tournament, we are going to allow asynchronous matches in the bracket phase of the tournament. ";
                        strong : "Before requesting an async, the players must have made significant efforts to attempt to schedule a live match.";
                        : " Permission from tournament organizers needs to be requested at least 24 hours in advance to make sure an organizer is available for the scheduled time.";
                    }
                    p : "Here are the guidelines:";
                    ul {
                        li : "No breaks";
                        li : "15 minute FPA time allowed";
                        li : "After scheduling your async, you will receive the seed from a tournament organizer. You must start the race within 10 minutes of obtaining the seed and submit your time within 5 minutes of finishing.";
                        li : "If you obtain a seed but do not submit a finish time, it will count as a forfeit.";
                        li : "Async races will no longer be an option for players from Winner's Bracket Semifinals and Lower Bracket Round 6.";
                    }
                    p {
                        : "Here are ";
                        em : "additional";
                        : " guidelines for the ";
                        strong : "first person playing";
                        : ":";
                    }
                    ul {
                        li : "No streaming allowed";
                        li : "Unlisted upload on YouTube. Please note that the result can already be submitted before YouTube has fully processed the upload.";
                        li : "A tournament organizer or volunteer will be in a voice call with the racer, so screen sharing on Discord is required.";
                    }
                    p {
                        : "If you are the ";
                        strong : "second person playing";
                        : ", you must stream your async live on Twitch.";
                    }
                    h2(id = "fpa") : "Fair Play Agreement (FPA)";
                    p {
                        : "The ";
                        a(href = "https://docs.google.com/document/d/1BbvHJF8vtyrte76jpoCVQBTy9MYStpN3vr2PLdiCIMk/edit") : "Fair Play Agreement";
                        : " is mandatory for all runners.";
                    }
                    p : "Mido will automatically enable FPA for all 1v1 matches. FPA is not available during qualifiers.";
                    h2(id = "rules") : "Tournament and Streaming Rules";
                    p : "Players may participate on N64 (EverDrive), Wii Virtual Console, and any race-legal emulator.";
                    p {
                        : "Please read and follow the ";
                        a(href = "https://wiki.ootrandomizer.com/index.php?title=Rules#Universal_Rules") : "Universal Racing Rules";
                        : " for all races.";
                    }
                    p {
                        : "Streaming will be required during every tournament race. Please refer to the ";
                        a(href = "#streaming") : "streaming setup";
                        : " section for further information.";
                    }
                    p : "There will be no additional streaming rules during qualifiers, but we encourage all racers to take precautions to protect themselves against malicious behavior (such as spoilers).";
                    p {
                        strong : "Bracket matches in the main tournament will enforce a strict stream delay of 20 minutes.";
                    }
                    p : "We ask that you test to ensure your streaming setup is capable of handling delayed streaming if you would like to participate in the bracket stage of the main tournament.";
                    p : "Any additional rules will be given to bracket participants at a later date.";
                    p {
                        : "All races will be hosted on ";
                        a(href = "https://racetime.gg/") : "racetime.gg";
                        : ". A racetime.gg account is mandatory to officially participate in races and have your qualifier scores calculated.";
                    }
                    p {
                        : "Please be courteous toward your fellow racers! Do not post information about ongoing seeds anywhere outside of the designated spoiler discussion channels. Please discuss seeds in the #s8-results-discussion channel of the OoTR Discord after finishing a race. ";
                        strong : "Depending on the severity of the offense, spoilers may result in disqualification from the entire tournament.";
                    }
                    p {
                        strong {
                            : "If you do not wish to participate in the bracket stage of either tournament, please ";
                            a(href = uri!(event::opt_out(Series::Standard, "8"))) : "opt out";
                            : " as soon as possible.";
                        }
                    }
                    h2(id = "streaming") : "Streaming Setup";
                    p : "To make the entire race workflow from race monitoring to restreaming as straightforward and barrier-free as possible, the following settings should be ensured regarding bitrate, resolution, and framerate:";
                    p : "Resolution 420p, Frame Rate 30 fps → Bitrate of 1000–1500 kbps";
                    p : "Resolution 720p, Frame Rate 30 fps → Bitrate of 2000–2600 kbps";
                    p {
                        : "Resolution 720p, Frame Rate 60 fps → Bitrate of 2800–3400 kbps";
                        br;
                        : "(bleeding edge without partner / assured quality options)";
                    }
                    p {
                        : "Resolution 1080p, Frame Rate 30 fps → Bitrate of 3000–3500 kbps";
                        br;
                        : "(not recommended without partner / assured quality options)";
                    }
                    p {
                        : "Resolution 1080p, Frame Rate 60 fps → Bitrate of 4500–5000 kbps";
                        br;
                        : "(not recommended without partner / assured quality options)";
                    }
                    p {
                        : "Note that the default settings OBS ships with are a ";
                        strong : "TON";
                        : " higher. Therefore, it is important you check your bitrate and resolution settings before your first tournament participation.";
                    }
                    p {
                        : "During the bracket stage of the main tournament, a stream delay of 20 minutes is enforced. Please ensure that the option ";
                        strong : "Preserve cutoff point when reconnecting";
                        : " is ";
                        strong : "DISABLED";
                        : " at all costs to prevent the loss of footage and desyncing issues. The ";
                        strong : "Delay";
                        : " setting needs to be set to ";
                        strong : "1200s";
                        : ". You can find both settings in ";
                        em : "General › Advanced › Stream Delay";
                        : ".";
                    }
                    h2(id = "coverage") : "Coverage";
                    p : "Tournament qualifiers and bracket matches will be streamed live on several different Twitch channels. Be sure to follow all of them to catch all the S8 action you can!";
                    p {
                        : "English Coverage:";
                        br;
                        a(href = "https://twitch.tv/zeldaspeedruns") : "twitch.tv/zeldaspeedruns";
                        br;
                        a(href = "https://twitch.tv/zeldaspeedruns2") : "twitch.tv/zeldaspeedruns2";
                        br;
                        a(href = "https://twitch.tv/zeldaspeedruns_3") : "twitch.tv/zeldaspeedruns_3";
                        br;
                        a(href = "https://twitch.tv/zeldaspeedruns_4") : "twitch.tv/zeldaspeedruns_4";
                        br;
                        a(href = "https://twitch.tv/thesilvergauntlets") : "twitch.tv/thesilvergauntlets";
                    }
                    p {
                        : "French Coverage:";
                        br;
                        : "This year, the coverage will be handled by papy_grant and the French Restream Coordination.";
                        br;
                        a(href = "https://twitch.tv/papy_grant") : "twitch.tv/papy_grant";
                    }
                    p {
                        : "German Coverage:";
                        br;
                        a(href = "https://twitch.tv/utzstauder") : "twitch.tv/utzstauder";
                    }
                    p {
                        : "Brazilian Portuguese:";
                        br;
                        : "RandoBrasil ";
                        a(href = "https://twitch.tv/randomizerbrasil") : "twitch.tv/randomizerbrasil";
                    }
                    p {
                        : "Due to the high amount of matches, it is very unlikely that all matches of the bracket stage will be restreamed. To prevent burnout for both restreamers and volunteers, coverage will be limited to a maximum of ";
                        strong : "two";
                        : " matches per day, unless a third is being requested by a full set of volunteers.";
                    }
                    p : "While most streams will have either dual language restreams or just English coverage, there might be matches only covered in a single language, especially in the earlier stages of the tournament.";
                    p : "Racers may deny a restream of their match. This needs to be done at least 24 hours in advance through the Hyrule Town Hall event status page. From Winner's bracket semifinals and Lower bracket round 6 onwards (as applicable), ZeldaSpeedRuns, The Silver Gauntlets, and their international partners reserve the right to restream the matches for their respective tournaments.";
                    p : "Beyond this pre-approval, the following rules apply: If a race cannot be covered by the pre-approved channel for a given language, other channels are welcome to request permission to restream in the #s8-restream-planning channel, and will be approved on a case-by-case basis given participants also consent. If a channel is interested in covering one or more races in a language not listed above, please also reach out in the #s8-restream-planning channel to ask for restream permission. Note that we will only allow a low number of additional channels per language. In all cases, restreams must be scheduled at least 24 hours in advance, to ensure that racers and volunteers have appropriate notice. We reserve the right to restream all semifinals and finals matches.";
                    h2(id = "special-thanks") : "Special Thanks";
                    p : "We appreciate all of the time and input the community as a whole has provided gearing up for this season. Nevertheless, we would like to extend a special thanks to the following individuals and groups for working with us to help make S8 work smoothly:";
                    p {
                        strong : "Aksannyi";
                        : " for providing us with excellent artwork to make our qualifier schedule look amazing!";
                    }
                    p {
                        strong : "OoTR developers";
                        : ", who are consistently bringing us additional quality of life and security features. They have worked diligently to make the password feature work on our stable branch and bring us version 8.2 in time for this tournament.";
                    }
                    p {
                        strong : "Beta Testers";
                        : ", for assisting devs by reporting issues with new features and keeping everything on schedule for the latest release (8.2!).";
                    }
                    p {
                        strong : "Fenhl";
                        : ", who, in addition to helping organize the tournament, has ensured Hyrule Town Hall is prepared to make this tournament more convenient for yourselves and us as organizers while maintaining security features. They have also kept us informed on important developer updates, and guided the team on presets and special generator knowledge we might not have known otherwise!";
                    }
                    p {
                        strong : "Race Mods";
                        : ", also a newer team like us, they have worked with us on coordinating weeklies, and making sure the ruleset is in a stable place throughout the S8 tournament.";
                    }
                    p {
                        strong : "TreZ & Chimp";
                        : " have both offered us insight, context, and input to help us in our decision-making. They have also, of course, helped us with general organization within Discord so we can stay organized and get out important information.";
                    }
                    h2(id = "all-settings") : "Appendix 1: Settings List";
                    h3 : "Main Rules:";
                    ul {
                        li : "Randomize Main Rule Settings: Off";
                        li : "Logic Rules: Glitchless";
                        li {
                            : "Open:";
                            ul {
                                li : "Forest: Closed Deku";
                                li : "Kakariko Gate: Open Gate";
                                li : "Open Door of Time";
                                li : "Zora's Fountain: Default behavior (closed)";
                                li : "Gerudo's Fortress: Rescue one carpenter";
                                li : "Dungeon Boss Shortcuts Mode: Off";
                                li : "Rainbow Bridge Requirement: Vanilla Requirements";
                                li : "Ganon's Trials: 0";
                            }
                        }
                        li {
                            : "World:";
                            ul {
                                li : "Starting Age: Random";
                                li : "MQ Dungeon Mode: Vanilla Dungeons";
                                li : "Pre-completed Dungeons Mode: Off";
                                li : "Shuffle Interior Entrances: Off";
                                li : "Shuffle Thieves' Hideout Entrances: Off";
                                li : "Shuffle Grotto Entrances: Off";
                                li : "Shuffle Dungeon Entrances: Off";
                                li : "Shuffle Boss Entrances: Off";
                                li : "Shuffle Overworld Entrances: Off";
                                li : "Shuffle Gerudo Valley Exit: Off";
                                li : "Randomize Owl Drops: Off";
                                li : "Randomize Overworld Spawns: Child";
                                li : "Triforce Hunt: Off";
                                li : "Add Bombchu Bag and Drops: Off";
                                li : "Dungeons Have One Major Item: Off";
                            }
                        }
                        li {
                            : "Shuffle:";
                            ul {
                                li : "Shuffle Songs: Song locations";
                                li : "Shopsanity: Off";
                                li : "Tokensanity: Off";
                                li : "Scrub Shuffle: Off";
                                li : "Shuffled Child Trade Sequence Items: None";
                                li : "Shuffle All Adult Trade Items: Off";
                                li : "Adult Trade Sequence Items: 4 (Prescription, Eyeball Frog, Eyedrops, Claim Check)";
                                li : "Shuffle Rupees & Hearts: Off";
                                li : "Shuffle Pots: Off";
                                li : "Shuffle Crates: Off";
                                li : "Shuffle Cows: Off";
                                li : "Shuffle Beehives: Off";
                                li : "Shuffle Wonderitems: Off";
                                li : "Shuffle Kokiri Sword: On";
                                li : "Shuffle Ocarinas: Off";
                                li : "Shuffle Gerudo Card: Off";
                                li : "Shuffle Magic Beans: Off";
                                li : "Shuffle Expensive Merchants: Off";
                                li : "Shuffle Frog Song Rupees: Off";
                                li : "Shuffle Hyrule Loach Reward";
                                li : "Shuffle Individual Ocarina Notes: Off";
                            }
                        }
                        li {
                            : "Shuffle Dungeon Items:";
                            ul {
                                li : "Maps & Compasses: Start with";
                                li : "Small Keys: Own Dungeon";
                                li : "Thieves' Hideout Keys: Vanilla Locations";
                                li : "Treasure Chest Game Keys: Vanilla Locations";
                                li : "Key Rings Mode: Off";
                                li : "Boss Keys: Own Dungeon";
                                li : "Ganon's Boss Key: 6 Medallions";
                                li : "Shuffle Silver Rupees: Vanilla Locations";
                                li : "Maps & Compasses Give Information: Off";
                            }
                        }
                    }
                    h3 : "Detailed Logic:";
                    ul {
                        li : "Guarantee Reachable Locations: All";
                        li : "Nighttime Skulltulas Expect Sun's Song: Off";
                        li {
                            : "Exclude Locations:";
                            ul {
                                li : "Deku Theater Mask of Truth";
                            }
                        }
                        li {
                            : "Enable Tricks:";
                            ul {
                                li : "Fewer Tunic Requirements";
                                li : "Hidden Grottos without Stone of Agony";
                                li : "Child Dead Hand without Kokiri Sword";
                                li : "Hammer Rusted Switches and Boulders Through Walls";
                                li : "Forest Temple East Courtyard Vines with Hookshot";
                                li : "Bottom of the Well without Lens of Truth";
                                li : "Ganon's Castle without Lens of Truth";
                                li : "Gerudo Training Ground without Lens of Truth";
                                li : "Shadow Temple Invisible Moving Platform without Lens of Truth";
                                li : "Shadow Temple Bongo Bongo without Lens of Truth";
                                li : "Spirit Temple without Lens of Truth";
                                li : "Man on Roof without Hookshot";
                                li : "Windmill Piece of Heart (PoH) as Adult with Nothing";
                                li : "Crater's Bean PoH with Hover Boots";
                                li : "Dodongo's Cavern Spike Trap Room Jump without Hover Boots";
                            }
                        }
                    }
                    h3 : "Starting Inventory:";
                    ul {
                        li : "Starting Equipment: Deku Shield";
                        li : "Starting Items: Ocarina, Zelda's Letter";
                        li : "Start with Consumables: On";
                        li : "Start with Max Rupees: Off";
                    }
                    h3 : "Other:";
                    ul {
                        li {
                            : "Timesavers:";
                            ul {
                                li : "Skip Tower Escape Sequence: On";
                                li : "Skip Child Stealth: On";
                                li : "Skip Epona Race: On";
                                li : "Skip Some Minigame Phases: On";
                                li : "Complete Mask Quest: Off";
                                li : "Enable Specific Glitch-Useful Cutscenes: Off";
                                li : "Fast Chest Cutscenes: On";
                                li : "Free Scarecrow's Song: Off";
                                li : "Fast Bunny Hood: On";
                                li : "Maintain Mask Equips through Scene Changes: Off";
                                li : "Plant Magic Beans: Off";
                                li : "Random Cucco Count: Off";
                                li : "Cucco Count: 3";
                                li : "Random Big Poe Target Count: Off";
                                li : "Big Poe Target Count: 1";
                                li : "Easier Fire Arrow Entry: Off";
                                li : "Ruto Already at F1: Off";
                            }
                        }
                        li {
                            : "Misc:";
                            ul {
                                li : "Randomize Ocarina Melodies: Off";
                                li : "Chest Appearance Matches Contents: Both size and texture";
                                li : "Minor Items in Big/Gold chests: Off";
                                li : "Invisible Chests: Off";
                                li : "Pot, Crate & Beehive Appearance Matches Contents: Texture (match content)";
                                li : "Key Appearance Matches Dungeons: Off";
                                li : "Clearer Hints: On";
                                li : "Gossip Stones: Hints; need nothing";
                                li : "Hint Distribution: Tournament";
                                li : "Misc. Hints: Temple of Time Altar, Warp Songs and Owls, House of Skulltula: 30, 40 & 50";
                                li : "Text Shuffle: No";
                                li : "Damage Multiplier: Normal";
                                li : "Bonks Do Damage: No";
                                li : "Hero Mode: Off";
                                li : "Starting Time of Day: Default (10:00)";
                                li : "Blue Fire Arrows: On";
                                li : "Fix Broken Drops: Off";
                            }
                        }
                        li {
                            : "Item Pool:";
                            ul {
                                li : "Balanced Item Pool";
                                li : "No Ice Traps";
                                li : "Ice Trap Appearances: Junk Items only";
                            }
                        }
                    }
                    h2(id = "sometimes-hints") : "Appendix 2: Sometimes/Dual Hints";
                    h3 : "Sometimes Hints";
                    h4 : "Overworld:";
                    ul {
                        li : "20 Skulls";
                        li : "Big Poes";
                        li : "Chickens";
                        li : "Composer Torches";
                        li : "Darunia's Joy";
                        li : "Frogs 1";
                        li : "Goron Pot";
                        li : "King Zora";
                        li : "Lab Dive";
                        li : "Shoot the Sun";
                        li : "Skull Kid";
                        li : "Sun's Song Grave";
                        li : "Target in the Woods";
                        li : "Treasure Chest Game";
                        li : "Wasteland Torches";
                        li : "ZF Icy Waters";
                    }
                    h4 : "Dungeons:";
                    ul {
                        li : "Fire Temple Hammer Chest";
                        li : "Fire Temple Scarecrow";
                        li : "Ganon's Castle Shadow Trial 2";
                        li : "GTG Toilet";
                        li : "Ice Cavern Final Chest";
                        li : "Jabu Stingers";
                        li : "Shadow Temple Skull Pot";
                        li : "Water Temple BK Chest";
                        li : "Water Temple Central Pillar";
                    }
                    h3 : "Dual Hints";
                    h4 : "Overworld:";
                    ul {
                        li : "Adult Lake Bean Checks";
                        li : "Bombchu Bowling";
                        li : "Castle Great Fairies";
                        li : "Child Domain";
                        li : "Gerudo Valley PoH Ledges";
                        li : "Horseback Archery";
                    }
                    h4 : "Dungeons:";
                    ul {
                        li : "BotW Dead Hand";
                        li : "Fire Temple Lower Hammer Loop";
                        li : "Ganon's Castle Spirit Trial";
                        li : "Shadow Temple Invisible Blades";
                        li : "Shadow Temple Spiked Walls";
                        li : "Spirit Temple Child Loop";
                        li : "Spirit Temple Colossus Hands";
                        li : "Spirit Temple Early Adult";
                        li : "Water Temple Dark Link Loop";
                    }
                }
                div {
                    nav {
                        strong : "Contents";
                        ul {
                            li {
                                a(href = "#links") : "Important Links";
                            }
                            li {
                                a(href = "#format") : "Tournament Format";
                            }
                            li {
                                a(href = "#ruleset") : "Ruleset";
                            }
                            li {
                                a(href = "#settings") : "Settings";
                            }
                            li {
                                a(href = "#hints") : "Hint Distribution";
                            }
                            li {
                                a(href = "#password") : "Password Protection";
                            }
                            li {
                                a(href = "#qualifiers") : "Qualifiers";
                            }
                            li {
                                a(href = "#brackets") : "Brackets";
                                ul {
                                    li {
                                        a(href = "#top32") : "Main Tournament";
                                    }
                                    li {
                                        a(href = "#cc") : "Challenge Cup";
                                    }
                                    li {
                                        a(href = "#scheduling") : "Scheduling";
                                    }
                                    li {
                                        a(href = "#asyncs") : "Asynchronous Matches (asyncs)";
                                    }
                                }
                            }
                            li {
                                a(href = "#fpa") : "Fair Play Agreement (FPA)";
                            }
                            li {
                                a(href = "#rules") : "Tournament and Streaming Rules";
                            }
                            li {
                                a(href = "#streaming") : "Streaming Setup";
                            }
                            li {
                                a(href = "#coverage") : "Coverage";
                            }
                            li {
                                a(href = "#special-thanks") : "Special Thanks";
                            }
                            li {
                                a(href = "#all-settings") : "Appendix 1: Settings List";
                            }
                            li {
                                a(href = "#sometimes-hints") : "Appendix 2: Sometimes/Dual Hints";
                            }
                        }
                    }
                }
            }
        }),
        "8cc" => Some(html! {
            article {
                p {
                    : "This is the Challenge Cup portion of the 8th season of the main Ocarina of Time randomizer tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1TY4ZjOaT55bx5rEE9uua4H2YfGNTWFOcueAZ4TuJkb4/edit") : "the official document";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = uri!(event::info(Series::Standard, "8"))) : "main bracket";
                    }
                }
            }
        }),
        _ => None,
    })
}

pub(crate) fn weeklies_enter_form(me: Option<&User>) -> RawHtml<String> {
    html! {
        article {
            p {
                : "The room for each race will be opened 30 minutes before the scheduled starting time. ";
                @if me.as_ref().is_some_and(|me| me.racetime.is_some()) {
                    : "You don't need to sign up beforehand.";
                } else {
                    : "You will need a ";
                    a(href = format!("https://{}/", racetime_host())) : "racetime.gg";
                    : " account to participate.";
                }
            }
        }
    }
}

pub(crate) fn s8_settings() -> seed::Settings {
    collect![
        format!("password_lock") => json!(true),
        format!("user_message") => json!("S8 Tournament"),
        format!("bridge") => json!("vanilla"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("medallions"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("spawn_positions") => json!([
            "child",
        ]),
        format!("free_bombchu_drops") => json!(false),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
        ]),
        format!("allowed_tricks") => json!([
            "logic_grottos_without_agony",
            "logic_fewer_tunic_requirements",
            "logic_rusted_switches",
            "logic_man_on_roof",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_dc_jump",
            "logic_forest_vines",
            "logic_child_deadhand",
            "logic_lens_botw",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_bongo",
            "logic_lens_spirit",
            "logic_lens_gtg",
            "logic_lens_castle",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("skip_reward_from_rauru") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("correct_chest_appearances") => json!("both"),
        format!("correct_potcrate_appearances") => json!("off"),
        format!("potcrate_textures_specific") => json!([]),
        format!("hint_dist") => json!("tournament"),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "30_skulltulas",
            "40_skulltulas",
            "50_skulltulas",
        ]),
        format!("blue_fire_arrows") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("anything"),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
    ]
}
