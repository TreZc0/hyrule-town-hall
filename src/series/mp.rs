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
                    : "This is an archive of the 1st season of the Mixed Pools tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/1EoRh37QOKbTT86Jdo97KnvdJ66Y7oKjbIplwYY8qRYs/edit#gid=130670252") : "Swiss pairings and results, and tiebreaker results";
                    }
                }
            }
        }),
        "2" => Some(html! {
            article {
                p {
                    : "This is the 2nd season of the Mixed Pools tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". Join ";
                    a(href = "https://discord.gg/cpvPMTPZtP") : "the Discord server";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://docs.google.com/document/d/1OBzytLL3YFNKpa2ly6U7UEh4irOmhnPHaGI8zGfSrsA/edit") : "Tournament format, rules, and settings";
                    }
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/1nz43jWsDrTgsnMzdLdXI13l9J6b8xHx9Ycpp8PAv9E8/edit?resourcekey#gid=148749353") : "Swiss pairings and results";
                    }
                }
            }
        }),
        "3" => Some(html! {
            article {
                p {
                    : "This is the 3rd season of the Mixed Pools tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". Join ";
                    a(href = "https://discord.gg/cpvPMTPZtP") : "the Discord server";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://docs.google.com/document/d/1OBzytLL3YFNKpa2ly6U7UEh4irOmhnPHaGI8zGfSrsA/edit") : "Tournament format, rules, and settings";
                    }
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/1DqKrmcLhWwfIRpTMZ-NqGGFma_jgCSkyHyP8kaJg6OA/edit?resourcekey#gid=1885752948") : "Swiss pairings and results";
                    }
                }
            }
        }),
        "4" => Some(html! {
            div(class = "toc") {
                article {
                    p {
                        : "Welcome to the 4th Season of the Mixed Pools Tournament, organized by ";
                        : English.join_html_opt(data.organizers(transaction).await?);
                        : "!";
                    }
                    p {
                        a(href = "https://docs.google.com/spreadsheets/d/1xjU_tv8pbMpfDBeCbm6SQ7HtbOPS4ZCsOLySkDX296o/edit") : "Swiss and tiebreaker matchups and results";
                    }
                    h2(id = "signing-up") : "Signing Up";
                    p {
                        strong {
                            : "Deadline is ";
                            : format_datetime(Utc.with_ymd_and_hms(2025, 5, 13, 23, 59, 00).single().expect("wrong hardcoded datetime"), DateTimeFormat { long: true, running_text: true });
                            : ".";
                            br;
                            : "Start of the first round is May 14th after the wheel spin.";
                        }
                    }
                    h2(id = "settings") : "Settings";
                    ul {
                        li {
                            : "Mixed Pools Entrance Randomizer: dungeons, indoors, overworld, grottos, owls, warps, spawns, boss entrances, Ganon's Castle ";
                            em : "and";
                            : " Ganon's Tower (";
                            em : "no";
                            : " Gerudo's Fortress shuffle!)";
                        }
                        li : "Start with consumables, Zelda's letter and Farore's Wind";
                        li : "Blue fire arrows enabled, refilling wallet, free scarecrow song";
                        li : "Cheap scrubs, shopsanity 4";
                        li : "Ganon's boss key on 9 dungeon rewards";
                        li : "Open bridge, open Zora's fountain, open Deku Tree, open Gerudo's Fortress";
                        li : "CAMC (size and texture)";
                        li : "30 / 40 / 50 skulls are off";
                        li : "3 cuccos";
                        li : "Ruto at F1";
                        li : "Mask of Truth is enabled";
                        li : "Complete Mask Shop: Off";
                        li : "Maps/Compasses give info";
                        li : "No start with Deku Shield";
                        li : "Shields in big chests";
                        li : "The owls in Lake Hylia and Death Mountain Trail as well as the songs tell you where you go before deciding to warp.";
                    }
                    p {
                        : "You can find ";
                        a(href = "#complete-settings-list") : "a full list of the settings";
                        : " in the appendix below.";
                    }
                    h2(id = "hint-distribution") : "Hint Distribution";
                    ul {
                        li : "5x Way of the hero hints";
                        li : "3x foolish hints: 2 dungeons and 1 overworld area";
                        li : "2x Always hints: Biggoron and Frogs 2";
                        li : "1x Always dual hint: Skull Mask & Mask of Truth";
                        li : "2x Always entrance hints: Forest and Shadow boss door entrances";
                        li : "2x Dual sometimes hints";
                        li : "2x Entrance sometimes hints";
                        li : "3x Sometimes hints";
                    }
                    h2(id = "good-to-know") : "Good to Know";
                    ul {
                        li {
                            : "Zelda's Lullaby ";
                            em : "can";
                            : " be hinted way of the hero.";
                        }
                        li : "There is no limit for hinted dungeons as way of the hero.";
                        li : "If there are 2 overworld areas hinted foolish, you know that only 1 dungeon is foolish.";
                        li : "If you find less than 3 foolish hints, the hint stones will be filled up with additional dual sometimes hints.";
                        li : "The other boss door entrances are in the sometimes pool.";
                        li : "It is not possible to have an overworld entrance at the end of Shadow and Spirit Temple.";
                        li : "Ganon's Tower is considered part of the hint area it's connected to, like interiors, other boss rooms, etc";
                        li : "Warp songs: there will not be multiple warp songs that lead to the same region.";
                        li {
                            strong : "FW logic: You might be logically required to set FW in a dungeon to change time of day to gain access to nearby entrances.";
                        }
                        li {
                            strong : "Blue warp logic: Every time you enter a blue warp in a boss room, the time of day will be set to its vanilla behaviour. This is now repeatable and therefore in logic to change time of day to access certain areas. All warps set it to noon EXCEPT Morpha which sets it to morning.";
                            br;
                            : "The reason for the change is that accessing the Spirit temple via non-repeatable entrance access can lead to softlocks, and non-repeatable time-of-day access was one of the ways to do so.";
                        }
                    }
                    h2(id = "entrances-information") : "Entrances Information";
                    p {
                        : "Here is a list of entrances that ";
                        em : "most likely";
                        : " lead to an overworld area:";
                    }
                    ul {
                        li : "DMC Lower Nearby → GC Darunias Chamber";
                        li : "DMC Upper Nearby → Death Mountain Summit";
                        li : "Death Mountain → Kak Behind Gate Cutscene Entrance";
                        li : "Desert Colossus → Wasteland Near Colossus";
                        li : "Gerudo Valley → Hyrule Field";
                        li : "Gerudo Fortress → GV Fortress Side";
                        li : "GC Woods Warp → Lost Woods";
                        li : "Goron City → Death Mountain";
                        li : "Graveyard → Kak Cutscene Entrance";
                        li : "Wasteland Near Fortress → GF Outside Gate";
                        li : "Castle Grounds → Market";
                        li : "Hyrule Field → LW Bridge";
                        li : "Kakariko Village → Hyrule Field";
                        li : "Lake Hylia → Hyrule Field";
                        li : "Lon Lon Ranch → Hyrule Field";
                        li : "LW Bridge → Kokiri Forest";
                        li : "LW Forest Exit → Kokiri Forest";
                        li : "Market → Market Entrance";
                        li : "Market Entrance → Hyrule Field";
                        li : "SFMeadow Entryway → LW Behind Mido";
                        li : "ToT Entrance → Market";
                        li : "Zoras Domain → Lake Hylia";
                        li : "Zoras Domain → ZR Behind Waterfall";
                        li : "Zoras Fountain → ZD Behind King Zora";
                        li : "Zora's River Front → Hyrule Field";
                        li : "Zora River → LW Underwater Entrance";
                    }
                    p : "Other entrances will obviously also lead to overworld areas.";
                    h2(id = "gameplay-rules") : "Gameplay Rules";
                    p {
                        : "All races will be completed abiding by the Standard ruleset. You can find the Universal Racing rules, Standard rules and Emulator rules here: ";
                        a(href = "https://wiki.ootrandomizer.com/index.php?title=Main_Page") : "https://wiki.ootrandomizer.com/index.php?title=Main_Page";
                    }
                    p : "Note: Crossing the Gerudo Valley bridge as a child shall be banned unless it is from back to front.";
                    p : "Standard timing rules apply: .done is on the first frame of the cutscene that plays after beating Ganon.";
                    p {
                        : "The Fair Play Agreement is mandatory for all runners (please reach out if you have concerns). The details of the FPA can be found here: ";
                        a(href = "https://docs.google.com/document/d/1BbvHJF8vtyrte76jpoCVQBTy9MYStpN3vr2PLdiCIMk/edit") : "https://zsr.link/YadkP";
                    }
                    h2(id = "tournament-structure") : "Tournament Structure";
                    p : "The tournament structure will begin with Swiss rounds followed by a Single Elimination Top 16 Bracket.";
                    p {
                        : "The number of Swiss rounds is to be determined and will depend on the number of entrants for the tournament. It will be announced shortly after opt-ins close on May 13th. It will be between 4 to 6 rounds, with the first round being paired randomly (";
                        strong : "no seeding async";
                        : "). For Rounds 2 and after, players will race others with the same number of Swiss phase wins. In those individual brackets, we will continue to draw randomly (0-1 faces 0-1, 2-1 faces 2-1, etc.). ";
                        em : "In the event of an odd number of players, a random player in the lowest score group who has not yet received a bye will receive a bye.";
                    }
                    h2(id = "scheduling-and-racing") : "Scheduling and Racing";
                    p : "Scheduling and seed rolling will be handled through Hyrule Town Hall.";
                    p : "Prerolling a seed by Mido is set up to reduce waiting time on generating a seed. Matches that are scheduled on short notice might have to wait a bit, please be patient.";
                    p {
                        : "Seeds can also be generated on Fenhl's branch v8.2.76-10 with the 4th Mixed Pools Tournament preset: ";
                        a(href = "https://ootrandomizer.com/generatorDev?version=devFenhl_8.2.76-10") : "https://ootrandomizer.com/generatorDev?version=devFenhl_8.2.76-10";
                    }
                    p : "We are looking to get through this tournament quickly and efficiently. Thus, all matchmaking should honor the following time constraints:";
                    ul {
                        li : "Matches should be scheduled within 48 hours of the matchup being known.";
                        li : "If no contact has been made within the first 24 hours, both players will be notified of their inactivity.";
                        li : "If the match is not scheduled after 48 hours, a final warning will be issued.";
                        li : "Matches must be played within 7 days of the matchup being known.";
                    }
                    h2(id = "asyncing-matches") : "Asyncing Matches";
                    p : "To be able to run a smooth tournament, we are going to allow asyncing matches. The guidelines are taken from the RSL tournament. Here are the main points:";
                    ul {
                        li : "For the first person to play: No streaming allowed.";
                        li : "Unlisted upload on youtube required.";
                        li : "No breaks (it's a big hassle to enforce breaks and make sure people don't forget them).";
                        li : "15 min FPA where needed.";
                        li {
                            strong : "If an async is required, let the tournament organizers know ahead of time. We'll set up a form for you to request your seed. You'll only be able to request the seed once, so make sure you are ready!";
                        }
                        li : "You must start the race within 15 minutes of obtaining the seed and submit your time within 10 minutes of finishing.";
                        li : "Fill out the provided results form immediately after you're done.";
                        li : "If you obtain a seed but do not submit a finish time, it will count as a forfeit.";
                    }
                    h2(id = "streaming-guidelines") : "Streaming Guidelines";
                    p : "Streaming is required for all tournament matches. A stream delay is not required for any tournament matches. There will be no additional streaming rules, but we encourage all racers to take precautions in order to protect themselves against malicious behavior (such as spoilers).";
                    h2(id = "breaks") : "Breaks";
                    p : "While breaks are not required, you are welcome to schedule breaks in the racetime chat. If both players agree to a break, both need to pause their game when their timer reaches the agreed upon time and must remain paused until the break is over.";
                    p {
                        : "If a player misses the start of the break, they have to pause for the same duration as the agreed upon break, as soon as it becomes apparent.";
                        br;
                        : "In the case you realize that you forgot a break, the time will be added to your final time. Do not intentionally do this and repeated instances may result in disciplinary action or removing your break privileges.";
                    }
                    p : "Please note that a player who forfeits cannot be awarded a win. We recommend that when your opponent finishes you at least play out your break time (so if you took 10 minutes total of breaks, play an extra 10 minutes) just in case your opponent forgot to pause.";
                    p {
                        : "Breaks can be setup in the racetime room with the following command (example):";
                        br;
                        code : "!breaks 5 every 2";
                        : " meaning a 5 minute break every 2 hours";
                    }
                    h2(id = "restreaming") : "Restreaming";
                    p : "We encourage everyone who wants to restream a match to do so. If so, please get in touch with the tournament organizers for approval as well as notify and ask the runners for permission at least 24 hours before the match is scheduled to happen (an exception to this rule may be made if a match is scheduled less than 24 hours in advance, the runners are happy to be featured, and volunteers are readily available).";
                    h2(id = "complete-settings-list") : "Complete Settings List";
                    h3 : "Main Rules";
                    ul {
                        li : "Randomize Main Rule Settings: Off";
                        li : "Logic Rules: Glitchless";
                        li {
                            : "Open:";
                            ul {
                                li : "Open Deku Tree";
                                li : "Open Forest";
                                li : "Closed Forest Requires Gohma: Off";
                                li : "Kakariko Gate: Open Gate";
                                li : "Open Door of Time";
                                li : "Zora's Fountain: Always open";
                                li : "Gerudo's Fortress: Open Gerudo's Fortress";
                                li : "Dungeon Boss Shortcuts Mode: Off";
                                li : "Rainbow Bridge Requirement: Always open";
                                li : "Random Number of Trials: Off";
                                li : "Ganon's Trials: 0";
                            }
                        }
                        li {
                            : "Various:";
                            ul {
                                li : "Triforce Hunt: Off";
                                li : "Add Bombchu Bag and Drops: Off";
                            }
                        }
                        li {
                            : "World:";
                            ul {
                                li : "Starting Age: Random";
                                li : "MQ Dungeon Mode: Vanilla";
                                li : "Pre-completed Dungeons Mode: Off";
                                li : "Shuffle Interior Entrances: All interiors";
                                li : "Shuffle Thieves' Hideout Entrances: Off";
                                li : "Shuffle Grotto Entrances: On";
                                li : "Shuffle Dungeon Entrances: Dungeon and Ganon";
                                li : "Shuffle Boss Entrances: Full";
                                li : "Shuffle Ganon's Tower Entrance: On";
                                li : "Shuffle Overworld Entrances: On";
                                li : "Mix Entrance Pools: All";
                                li : "Decouple Entrances: Off";
                                li : "Shuffle Gerudo Valley Exit: Balanced";
                                li : "Randomize Owl Drops: Balanced";
                                li : "Randomize Warp Song Destinations: Balanced";
                                li : "Randomize Child Overworld Spawn: Balanced";
                                li : "Randomize Adult Overworld Spawn: Balanced";
                                li : "Randomize Blue Warps: Dungeon Entrance";
                                li : "Mutually Exclusive One-Ways: On";
                                li : "[EXPERIMENTAL] Allow Access to Shadow and Spirit Temples From Boss Doors: Off";
                            }
                        }
                        li {
                            : "Shuffle:";
                            ul {
                                li : "Shuffle Items: On";
                                li : "Shuffle Songs: Song locations";
                                li : "Shuffle Shops: 4 items per Shop";
                                li : "Special Deal Prices: Weighted";
                                li : "Minimum Special Deal Price: 0";
                                li : "Maximum Special Deal Price: 300";
                                li : "Shuffle Gold Skulltula Tokens: Off";
                                li : "Scrub Shuffle: On (Affordable)";
                                li : "Shuffle Child Trade Sequence Items: None";
                                li : "Shuffle All Selected Adult Trade Items: Off";
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
                                li : "Shuffle 100 Skulltula Reward: Off";
                                li : "Shuffle Hyrule Loach Reward: Off";
                                li : "Shuffle Individual Ocarina Notes: Off";
                                li : "Shuffle Other items: On";
                            }
                        }
                        li {
                            : "Shuffle Dungeon Items:";
                            ul {
                                li : "Shuffle Dungeon Rewards: Dungeon Reward Locations";
                                li : "Maps & Compasses: Start with";
                                li : "Small Keys: Own Dungeon";
                                li : "Thieves' Hideout Keys: Vanilla Locations";
                                li : "Treasure Chest Game Keys: Vanilla Locations";
                                li : "Key Rings Mode: Off";
                                li : "Boss Keys: Own Dungeon";
                                li : "Ganon's Boss Key: Dungeon Rewards";
                                li : "Dungeon Rewards Required for Ganon's BK: 9";
                                li : "Shuffle Silver Rupees: Vanilla Locations";
                                li : "Maps & Compasses Give Information: On";
                            }
                        }
                    }
                    h3 : "Detailed Logic";
                    ul {
                        li : "Guarantee Reachable Locations: All";
                        li : "Nighttime Skulltulas Expect Sun's Song: Off";
                        li : "Water Temple Disable Entry With Gold Scale: Off";
                        li {
                            : "Exclude Locations:";
                            ul {
                                li : "Kak 30 Gold Skulltula Reward";
                                li : "Kak 40 Gold Skulltula Reward";
                                li : "Kak 50 Gold Skulltula Reward";
                            }
                        }
                        li {
                            : "Enable Tricks:";
                            ul {
                                li : "Fewer Tunic Requirements";
                                li : "Hidden Grottos without Stone of Agony";
                                li : "Child Dead Hand without Kokiri Sword";
                                li : "Man on Roof without Hookshot";
                                li : "Dodongo's Cavern Spike Trap Room Jump without Hover Boots";
                                li : "Hammer Rusted Switches and Boulders Through Walls";
                                li : "Windmill Piece of Heart (PoH) as Adult with Nothing";
                                li : "Crater's Bean PoH with Hover Boots";
                                li : "Forest Temple East Courtyard Vines with Hookshot";
                                li : "Bottom of the Well without Lens of Truth";
                                li : "Ganon's Castle without Lens of Truth";
                                li : "Gerudo Training Ground without Lens of Truth";
                                li : "Shadow Temple Stationary Objects without Lens of Truth";
                                li : "Shadow Temple Invisible Moving Platform without Lens of Truth";
                                li : "Shadow Temple Bongo Bongo without Lens of Truth";
                                li : "Spirit Temple without Lens of Truth";
                                li : "Deku Tree Basement Web to Gohma with Bow";
                                li : "Pass Through Visible One-Way Collisions";
                            }
                        }
                    }
                    h3 : "Starting Inventory";
                    ul {
                        li : "Starting Items: Ocarina, Farore's Wind, Zelda's Letter";
                        li : "Start with Consumables: On";
                        li : "Start with Max Rupees: On";
                        li : "Starting Hearts: 3";
                    }
                    h3 : "Other";
                    ul {
                        li {
                            : "Timesavers:";
                            ul {
                                li : "Free Reward from Rauru: On";
                                li : "Skip Tower Escape Sequence: On";
                                li : "Skip Child Stealth: On";
                                li : "Skip Epona Race: On";
                                li : "Skip Some Minigame Phases: On";
                                li : "Complete Mask Quest: Off";
                                li : "Glitch-Useful Behaviours: Off";
                                li : "Fast Chest Cutscenes: On";
                                li : "Free Scarecrow's Song: On";
                                li : "Fast Bunny Hood: Off";
                                li : "Maintain Mask Equips through Scene Changes: Off";
                                li : "Plant Magic Beans: Off";
                                li : "Easier Fire Arrow Entry: Off";
                                li : "Ruto Already at F1: On";
                                li : "Fast Shadow Boat: Off";
                                li : "Random Cucco Count: Off";
                                li : "Cucco Count: 3";
                                li : "Random Big Poe Target Count: Off";
                                li : "Big Poe Target Count: 1";
                            }
                        }
                        li {
                            : "Hints and Information:";
                            ul {
                                li : "Clearer Hints: On";
                                li : "Gossip Stones: Hints; Need Nothing";
                                li : "Hint Distribution: Mixed Pools Tournament";
                                li : "Misc Hints: Temple of Time Altar, Ganondorf (Light Arrows), Warp Songs and Owls, House of Skulltula: 20";
                                li : "Chest Appearance Matches Contents: Box Size and Textures";
                                li : "Chest Textures: All";
                                li : "Minor Items in Big/Gold Chests: Deku & Hylian Shields";
                                li : "Invisible Chests: Off";
                                li : "Pot, Crate & Beehive Appearance Matches Contents: Off";
                                li : "Distinct Item Models: None";
                            }
                        }
                        li {
                            : "Item Pool:";
                            ul {
                                li : "Item Pool: Balanced";
                                li : "Ice Traps: No Ice Traps";
                                li : "Ice Traps Appearance: Junk Items Only";
                            }
                        }
                        li {
                            : "Gameplay Changes:";
                            ul {
                                li : "Randomize Ocarina Melodies: Off";
                                li : "Text Shuffle: No Text Shuffle";
                                li : "Damage Multiplier: Normal";
                                li : "Bonks Do Damage: No Damage";
                                li : "Starting Time of Day: Default (10:00)";
                                li : "Blue Fire Arrows: On";
                                li : "Fix Broken Drops: Off";
                                li : "Require Lens of Truth for Treasure Chest Game: On";
                                li : "Hero Mode: Off";
                                li : "Dungeons Have One Major Item: Off";
                            }
                        }
                    }
                }
                div {
                    nav {
                        strong : "Contents";
                        ul {
                            li {
                                a(href = "#signing-up") : "Signing Up";
                            }
                            li {
                                a(href = "#settings") : "Settings";
                            }
                            li {
                                a(href = "#hint-distribution") : "Hint Distribution";
                            }
                            li {
                                a(href = "#good-to-know") : "Good to Know";
                            }
                            li {
                                a(href = "#entrances-information") : "Entrances Information";
                            }
                            li {
                                a(href = "#gameplay-rules") : "Gameplay Rules";
                            }
                            li {
                                a(href = "#tournament-structure") : "Tournament Structure";
                            }
                            li {
                                a(href = "#scheduling-and-racing") : "Scheduling and Racing";
                            }
                            li {
                                a(href = "#asyncing-matches") : "Asyncing Matches";
                            }
                            li {
                                a(href = "#streaming-guidelines") : "Streaming Guidelines";
                            }
                            li {
                                a(href = "#breaks") : "Breaks";
                            }
                            li {
                                a(href = "#restreaming") : "Restreaming";
                            }
                            li {
                                a(href = "#complete-settings-list") : "Complete Settings List";
                            }
                        }
                    }
                }
            }
        }),
        _ => None,
    })
}

pub(crate) fn s2_settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("2nd Mixed Pools Tournament"),
        format!("bridge") => json!("open"),
        format!("bridge_medallions") => json!(2),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("dungeons"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("open_deku") => json!(true),
        format!("open_forest") => json!(true),
        format!("require_gohma") => json!(false),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("zora_fountain") => json!("open"),
        format!("gerudo_fortress") => json!("open"),
        format!("starting_age") => json!("random"),
        format!("shuffle_interior_entrances") => json!("all"),
        format!("shuffle_grotto_entrances") => json!(true),
        format!("shuffle_dungeon_entrances") => json!("all"),
        format!("shuffle_bosses") => json!("full"),
        format!("shuffle_overworld_entrances") => json!(true),
        format!("mix_entrance_pools") => json!([
            "Interior",
            "GrottoGrave",
            "Dungeon",
            "Overworld",
            "Boss",
        ]),
        format!("shuffle_gerudo_valley_river_exit") => json!("balanced"),
        format!("owl_drops") => json!("balanced"),
        format!("warp_songs") => json!("balanced"),
        format!("shuffle_child_spawn") => json!("balanced"),
        format!("shuffle_adult_spawn") => json!("balanced"),
        format!("exclusive_one_ways") => json!(true),
        format!("free_bombchu_drops") => json!(false),
        format!("shopsanity") => json!("4"),
        format!("shuffle_scrubs") => json!("low"),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
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
            "logic_deku_b1_webs_with_bow",
            "logic_visible_collisions",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_items") => json!([
            "ocarina",
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("start_with_rupees") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("complete_mask_quest") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("correct_chest_appearances") => json!("both"),
        format!("correct_potcrate_appearances") => json!("off"),
        format!("clearer_item_models") => json!(false),
        format!("hint_dist") => json!("mixed_pools"),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
    ]
}

pub(crate) fn s3_settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("3rd Mixed Pools Tournament"),
        format!("bridge") => json!("open"),
        format!("bridge_medallions") => json!(2),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("dungeons"),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("open_deku") => json!(true),
        format!("open_forest") => json!(true),
        format!("require_gohma") => json!(false),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("zora_fountain") => json!("open"),
        format!("gerudo_fortress") => json!("open"),
        format!("dungeon_shortcuts_choice") => json!("choice"),
        format!("dungeon_shortcuts") => json!([
            "Jabu Jabus Belly",
        ]),
        format!("starting_age") => json!("random"),
        format!("shuffle_interior_entrances") => json!("all"),
        format!("shuffle_grotto_entrances") => json!(true),
        format!("shuffle_dungeon_entrances") => json!("all"),
        format!("shuffle_bosses") => json!("full"),
        format!("shuffle_ganon_tower") => json!(true),
        format!("shuffle_overworld_entrances") => json!(true),
        format!("mix_entrance_pools") => json!([
            "Interior",
            "GrottoGrave",
            "Dungeon",
            "Overworld",
            "Boss",
        ]),
        format!("shuffle_gerudo_valley_river_exit") => json!("balanced"),
        format!("owl_drops") => json!("balanced"),
        format!("warp_songs") => json!("balanced"),
        format!("shuffle_child_spawn") => json!("balanced"),
        format!("shuffle_adult_spawn") => json!("balanced"),
        format!("exclusive_one_ways") => json!(true),
        format!("free_bombchu_drops") => json!(false),
        format!("shopsanity") => json!("4"),
        format!("shuffle_scrubs") => json!("low"),
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
            "logic_deku_b1_webs_with_bow",
            "logic_visible_collisions",
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
        format!("start_with_rupees") => json!(true),
        format!("skip_reward_from_rauru") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("complete_mask_quest") => json!(true),
        format!("free_scarecrow") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("correct_chest_appearances") => json!("both"),
        format!("correct_potcrate_appearances") => json!("off"),
        format!("clearer_item_models") => json!([]),
        format!("hint_dist") => json!("mixed_pools"),
        format!("blue_fire_arrows") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
    ]
}

pub(crate) fn s4_settings() -> seed::Settings {
    collect![
        format!("password_lock") => json!(true),
        format!("user_message") => json!("4th Mixed Pools Tournament"),
        format!("bridge") => json!("open"),
        format!("bridge_medallions") => json!(2),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => json!("dungeons"),
        format!("open_deku") => json!(true),
        format!("open_forest") => json!(true),
        format!("require_gohma") => json!(false),
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(true),
        format!("zora_fountain") => json!("open"),
        format!("gerudo_fortress") => json!("open"),
        format!("starting_age") => json!("random"),
        format!("shuffle_interior_entrances") => json!("all"),
        format!("shuffle_grotto_entrances") => json!(true),
        format!("shuffle_dungeon_entrances") => json!("all"),
        format!("shuffle_bosses") => json!("full"),
        format!("shuffle_ganon_tower") => json!(true),
        format!("shuffle_overworld_entrances") => json!(true),
        format!("mix_entrance_pools") => json!([
            "Interior",
            "GrottoGrave",
            "Dungeon",
            "Overworld",
            "Boss",
        ]),
        format!("shuffle_gerudo_valley_river_exit") => json!("balanced"),
        format!("owl_drops") => json!("balanced"),
        format!("warp_songs") => json!("balanced"),
        format!("shuffle_child_spawn") => json!("balanced"),
        format!("shuffle_adult_spawn") => json!("balanced"),
        format!("exclusive_one_ways") => json!(true),
        format!("shopsanity") => json!("4"),
        format!("shuffle_scrubs") => json!("low"),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(true),
        format!("free_bombchu_drops") => json!(false),
        format!("disabled_locations") => json!([
            "Kak 30 Gold Skulltula Reward",
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
            "logic_deku_b1_webs_with_bow",
            "logic_visible_collisions",
        ]),
        format!("starting_inventory") => json!([
            "ocarina",
            "farores_wind",
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
        format!("ruto_already_f1_jabu") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("hint_dist") => json!("mixed_pools"),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "20_skulltulas",
        ]),
        format!("correct_chest_appearances") => json!("both"),
        format!("minor_items_as_major_chest") => json!([
            "shields",
        ]),
        format!("correct_potcrate_appearances") => json!("off"),
        format!("clearer_item_models") => json!([]),
        format!("blue_fire_arrows") => json!(true),
        format!("tcg_requires_lens") => json!(true),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
    ]
}

pub(crate) fn async_rules(data: &Data<'_>) -> RawHtml<String> {
    html! {
        p : "Rules:";
        p {
            : "Welcome to the tiebreaker round of the ";
            : data;
            : "!";
        }
        p : "Here you can request the seed to play your async. Please follow the instructions correctly as well as make sure to know the process of asyncs:";
        ul {
            li : "No streaming allowed.";
            li : "Unlisted upload on youtube required.";
            li : "No breaks (it's a big hassle to enforce breaks and make sure people don't forget them).";
            li : "15 min FPA where needed.";
            li : "You must start the race within 15 minutes of obtaining the seed and submit your time within 10 minutes of finishing.";
            li : "If you obtain a seed but do not submit a finish time, it will count as a forfeit.";
        }
        p : "To submit the result, please also use the Mido's House website.";
        p : "Good luck and have fun!";
    }
}
