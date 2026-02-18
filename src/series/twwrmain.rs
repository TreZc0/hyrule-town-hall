use {
    chrono::Utc,
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
    },
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    let now = Utc::now();
    Ok(match &*data.event {
        "w" => {
            let weekly_schedules = WeeklySchedule::for_event(transaction, Series::TwwrMain, "w").await?;
            let active_schedules: Vec<_> = weekly_schedules.iter().filter(|s| s.active).collect();
            Some(html! {
                article {
                    @if active_schedules.is_empty() {
                        p : "Weekly races for The Wind Waker Randomizer are currently not scheduled.";
                    } else if active_schedules.len() == 1 {
                        @let schedule = active_schedules[0];
                        p {
                            : "Weekly races for The Wind Waker Randomizer run every ";
                            : format_recurring_time(schedule.next_after(now));
                            : " (next: ";
                            : format_datetime(schedule.next_after(now), DateTimeFormat { long: true, running_text: false });
                            : ").";
                        }
                    } else {
                        p : "Weekly races for The Wind Waker Randomizer:";
                        ul {
                            @for schedule in active_schedules {
                                li {
                                    : format!("{}: ", schedule.name);
                                    : format_recurring_time(schedule.next_after(now));
                                    : " (next: ";
                                    : format_datetime(schedule.next_after(now), DateTimeFormat { long: true, running_text: false });
                                    : ")";
                                }
                            }
                        }
                    }
                }
            })
        },
        "miniblins26" => Some(html! {
            article {
                p : "Hello, and welcome to the Miniblins Tournament for Wind Waker Randomizer. Every hero starts their journey somewhere. This tournament is here to help newcomers and less-experienced runners grow. You will move up from training against Miniblins to facing the main boss, Ganondorf. Runners of all skill levels are encouraged to participate.";
                p : "This document covers everything you need to know before joining the tournament. If you have any further questions, please ask on the WWR Racing Discord server or contact a tournament organizer (anyone with the Organizer role in Discord). There is also an FAQ section at the end that may answer your questions.";
            }
            article {
                h2 : "Settings";
                p {
                    : "The tournament uses the ";
                    code : "dev_tanjo3.1.10.5";
                    : " build of the randomizer (";
                    a(href = "https://github.com/tanjo3/wwrando/releases/tag/dev_tanjo3.1.10.5") : "download";
                    : ") with the following settings:";
                }
                h3 : "Dungeons";
                ul {
                    li : "2 Required Bosses";
                    li : "Add Inter-Dungeon Shortcuts";
                    li : "Open DRC is off";
                    li : "Prioritize Required Bosses";
                }
                h3 : "Included Locations";
                ul {
                    li : "Dungeon Secrets";
                    li : "Puzzle Secret Caves";
                    li {
                        : "Great Fairies (only the following):";
                        ul {
                            li : "Eastern, Southern, and Thorned Fairies";
                        }
                    }
                    li : "Short Sidequests";
                    li : "Free Gifts";
                    li : "Mail";
                    li {
                        : "Submarines (only the following):";
                        ul {
                            li : "Bomb Island and Crescent Moon Island";
                        }
                    }
                    li {
                        : "Island Puzzles";
                        ul {
                            li : "Horseshoe Island - Play Golf and Outset Island - Jabun's Cave are excluded";
                        }
                    }
                }
                h3 : "Miscellaneous";
                ul {
                    li : "Tingle Island - Ankle - Reward for all Tingle Statues is excluded";
                    li : "Sword Mode: Start with Hero's Sword";
                    li : "Chest Type Matches Contents (CTMC)";
                    li : "Randomize Starting Island";
                    li : "Start with six hearts; Double Magic; Hero's Shield; a Bomb Bag and Quiver Upgrade (capacity upgrades, but you still need to find Bombs/Hero's Bow to use them); both Din's and Nayru's Pearls; Telescope; and all Songs, Dungeon Maps, and Compasses";
                    li : "Extra Random Starting Items: 1";
                    li : "Logic Difficulty settings set to None";
                    li : "Hints on King of Red Lions: 2 Path, 2 Barren, 3 Location";
                    li : "Prioritize Remote Location Hints";
                    li : "Hint Importance";
                    li : "Split Warp Pots by Required (1, 2, 3 DRM)";
                }
                h3 : "Excluded Windfall Island Locations";
                p : "Locations behind freeing Tingle:";
                ul {
                    li : "Jail - Maze Chest";
                    li : "Jail - Tingle - First Gift";
                    li : "Jail - Tingle - Second Gift";
                }
                p : "Locations behind the Delivery Bag:";
                ul {
                    li : "Cafe Bar - Postman";
                    li : "Maggie - Delivery Reward";
                    li : "Zunari - Stock Exotic Flower in Zunari's Shop";
                    li : "Mila - Follow the Thief";
                }
                h3 : "Convenience Tweaks (always on)";
                ul {
                    li : "Swift Sail";
                    li : "Instant Text Boxes";
                    li : "Reveal Full Sea Chart";
                    li : "Add Shortcut Warps Between Dungeons";
                    li : "Skip Boss Rematches";
                    li : "Remove Title and Ending Videos";
                }
                h3 : "Optional Tweaks (your preference)";
                ul {
                    li : "Randomize Enemy Palettes";
                    li : "Invert Camera X-Axis";
                    li : "Invert Sea Compass X-Axis";
                }
                p {
                    : "Example permalink: ";
                    code : "eJxLSS2LL0nMy8o31jPUMzTQM423NLIwtUwzYHBkOLshQYKdgY2DiQEOGOEsBRkGhhYFBgUIT/MMQwOHctID4SYBBgYOFQcGJkWgUi6oUiYBAOCvDuY=";
                }
                p {
                    a(href = "https://github.com/tanjo3/wwrando/releases/tag/dev_tanjo3.1.10.5") : "Download the randomizer build";
                    : " | ";
                    a(href = "https://wooferzfg.me/tww-rando-tracker/miniblins") : "Tracker";
                }
                h3 : "Changes from Last Season";
                ul {
                    li {
                        : "Using the ";
                        code : "dev_tanjo3.1.10.5";
                        : " build";
                    }
                    li : "Dungeon Secrets and Great Fairies have been turned back on (half the Great Fairies are excluded)";
                    li : "Mila - Follow the Thief has been turned off";
                    li : "Submarines have been turned on (only 2 out of 7 are included)";
                    li : "You start with one extra random starting item";
                    li : "Required bosses are guaranteed to drop a progress item (that may or may not be required)";
                    li : "There is one fewer location hint than last season";
                }
            }
            article {
                h2 : "Tournament Structure";
                p : "The tournament consists of a qualifying stage and a main stage.";
                h3 : "Qualifying Stage";
                p : "The qualifying stage lasts for two weeks. During this stage, three qualifier asyncs and three live qualifiers will be released. You may complete these asyncs at any time and in any order during the qualifying stage.";
                p : "Each race or async is scored based on your time compared to the top three times for that seed. The best two of your first four races or asyncs will make up your final score. You must finish at least 2 races or asyncs to qualify for the main stage. A forfeit does not count as seed completion. If you do not want to be eligible for the main stage, contact a tournament organizer as soon as possible.";
                h4 : "Live Qualifiers";
                p : "There will be three live qualifiers (all times in Eastern Time):";
                ul {
                    li : "Saturday, March 7th, 2026 at 2:00 PM";
                    li : "Sunday, March 8th, 2026 at 11:00 AM";
                    li : "Friday, March 13th, 2026 at 7:00 PM";
                }
                h3 : "Main Stage";
                p : "All runners who qualify can move on to the main stage. Runners will be divided into groups determined by seeding, with group sizes set by tournament organizers to ensure a level and competitive playing field. The names and number of groups will be determined once the qualifying stage is complete.";
                p : "Each group will follow a seeded, best-of-one, double-elimination bracket. The Grand Finals of each group will use a bracket reset (a second match is played if the runner from the losers bracket wins the first game). Each group's winners will be crowned as the Miniblins Tournament 2026 winners.";
            }
            article {
                h2 : "Timeline";
                p : "The qualifying stage starts March 1st at noon ET. Complete and submit qualifiers by March 14th, 11:59 PM ET.";
                p : "There is a one-week break between qualifying and the main stage for run verification and group assignments. The main stage starts on March 21st. If the groups end up being different sizes, the timeline will be adjusted accordingly.";
                table {
                    thead {
                        tr {
                            th : "Round";
                            th : "Start Date";
                            th : "Deadline (11:59 PM ET)";
                        }
                    }
                    tbody {
                        tr { td : "WB Round 1"; td : "March 21st"; td : "March 28th"; }
                        tr { td : "WB Semis"; td : "March 29th"; td : "April 4th"; }
                        tr { td : "LB Round 1"; td : "March 29th"; td : "April 4th"; }
                        tr { td : "WB Finals"; td : "April 5th"; td : "April 18th"; }
                        tr { td : "LB Round 2"; td : "April 5th"; td : "April 11th"; }
                        tr { td : "LB Semis"; td : "April 12th"; td : "April 18th"; }
                        tr { td : "LB Finals"; td : "April 19th"; td : "April 25th"; }
                        tr { td : "Grand Finals"; td : "April 26th"; td : "May 2nd"; }
                    }
                }
            }
            article {
                h2 : "Rules";
                p : "You must follow the rules when participating in the tournament. Breaking rules can result in penalties, disqualification, or other disciplinary actions.";
                h3 : "Randomizer Setup Rules";
                ul {
                    li : "Both console and the Dolphin emulator are allowed.";
                    li {
                        : "We recommend that you use the latest Dolphin release. It is recommended that you unlock disc read speed:";
                        ul {
                            li {
                                b : "Dolphin 5.0-18639 and newer:";
                                : " Disable \"Emulate Disc Speed\".";
                            }
                            li {
                                b : "Older than Dolphin 5.0-18639:";
                                : " Enable \"Speed up Disc Transfer Rate\". (Note that you must set this to a check mark, not a black box.)";
                            }
                            li : "To find this setting, right-click the ISO file in Dolphin and click Properties. Once you set it, it should apply to all instances of WWR seeds.";
                            li {
                                b : "Console (Nintendont):";
                                : " Enable \"Unlock Read Speed\". Don't forget to enable \"Memcard Emulation\" to save without needing a memory card.";
                            }
                        }
                    }
                    li {
                        : "All custom player models in the official model pack are allowed. The following models from GameBanana are also allowed:";
                        ul {
                            li : "Toad";
                            li : "Among Us Crewmate";
                            li : "Skyward Ascent (Skyward Sword Link)";
                            li : "Twilight Princess Link";
                        }
                    }
                    li : "Swapping item models with other allowed models is permitted with the permission of the model author(s). Any other custom player model is only allowed if the tournament organizers approve it.";
                    li : "All hacks and cheats (such as AR or Gecko codes) are banned.";
                    li : "Graphical game modifications (e.g., HD texture packs, widescreen, upscaling/upscalers) are allowed, except that \"Disable Fog\" in Dolphin must remain unchecked. However, the internal resolution must be set to Native (640x528).";
                    li : "Save states, modified emulation speeds, and TAS inputs are not allowed.";
                }
                h3 : "Racing Rules";
                ul {
                    li : "All runners must only use the provided seed in a tournament qualifier async or match. The use of other save files or game ISOs is not allowed. Runners are responsible for ensuring the seed they use is correct.";
                    li : "For main-stage matches, streaming to Twitch with VOD recording is required. Ensure game audio is included, no overlays or text cover gameplay, and use local recording as a backup if possible.";
                    li : "While streaming any tournament seed, you are not allowed to read your Twitch chat. Additionally, watching, lurking, or engaging in any other interactions with your opponent's or the restream's channels while the race is in progress is grounds for disqualification.";
                    li : "Receiving any help or voice chatting during the race is not allowed. However, you may look up strats or review your own VOD on your own.";
                    li : "It is not permitted to send a message in the racetime.gg chat after the race has started and before the race outcome has been decided.";
                    li : "Runners are encouraged to enable notifications in the racetime.gg chat and to occasionally check the race room or Discord throughout the race in case a tournament organizer needs to contact them.";
                    li : "Using a tracker operated manually is permitted. No automated tools are allowed.";
                    li : "Tardiness of more than 15 minutes past the scheduled start time of a race results in a forfeit, unless both runners agree to reschedule.";
                    li : "If both runners forfeit the race, they will be required to schedule a rematch on a different seed.";
                    li : "If the runners finish within 5 seconds of each other, tournament organizers will retime the race to determine the outcome. If the outcome cannot be determined, the race is considered a tie, and the runners must schedule a rematch on a different seed.";
                    li : "If a runner wishes to force a rematch of a race they would otherwise have won, they must notify the tournament organizers within 24 hours of the race's end. A rematch requires the tournament organizers' approval.";
                    li : "A runner may cancel and reschedule a race if the race is canceled at least 12 hours before the scheduled time. If less than 12 hours remain, the runner who canceled the race must forfeit unless both runners agree to reschedule.";
                }
                h3 : "In-Game Rules";
                ul {
                    li : "Timing starts upon hitting End on the Name Entry screen and ends as Link deals the final attack on Ganondorf.";
                    li {
                        : "All glitches and tricks are allowed except:";
                        ul {
                            li : "Barrier Skip";
                            li : "Puppet Ganon Skip";
                            li : "Salvage Item Manipulation";
                            li : "Fairy Item Duplication";
                            li : "Back in Time (BiT)";
                            li : "Stone Tablet Item Duplication";
                            li : "Dungeon Chest Reload";
                            li : "Text Stacking and Arbitrary Code Execution (ACE)";
                        }
                    }
                    li : "These glitches and tricks are challenging or, realistically, impossible to pull off, and are here mainly for completeness. Practically speaking, all glitches and tricks are allowed.";
                    li : "Using the Tingle Tuner is banned. If no Game Boy Advance is connected, you may still use the Tingle Tuner item.";
                    li : "Using the Sploosh Kaboom solver/tool is forbidden.";
                }
                p {
                    a(href = "https://docs.google.com/document/d/1LRNFT3RUoeI2gegYwCmTaqQJsABBa7f5DzHAG7BkP5A/edit?tab=t.0#heading=h.q7sppx7kjlq") : "View the full detailed document";
                }
            }
        }),
        _ => None,
    })
}
