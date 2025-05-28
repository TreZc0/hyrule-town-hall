use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "3" => Some(html! {
            article {
                p(lang = "fr") {
                    : "Voici la 3e saison du tournoi francophone, organisée par ";
                    : French.join_html_opt(data.organizers(transaction).await?);
                    : ". Rejoignez ";
                    a(href = "https://discord.gg/wyhPVmquZC") : "le serveur Discord";
                    : " pour plus de détails.";
                }
                ul {
                    li {
                        a(href = "https://docs.google.com/document/d/1sQ8HgX1swX0PulCd85195z5eu7hIC73SYNVjUJY8Tw0/edit") : "Règlements";
                    }
                }
            }
        }),
        "4" => {
            let organizers = data.organizers(transaction).await?;
            Some(html! {
                article {
                    p(lang = "en") {
                        : "This is the 4th season of the Francophone tournament, organized by ";
                        : English.join_html_opt(&organizers);
                        : ". Join ";
                        a(href = "https://discord.gg/wyhPVmquZC") : "the Discord server";
                        : " for details.";
                    }
                    p(lang = "fr") {
                        : "Voici la 4e saison du tournoi francophone, organisée par ";
                        : French.join_html_opt(organizers);
                        : ". Rejoignez ";
                        a(href = "https://discord.gg/wyhPVmquZC") : "le serveur Discord";
                        : " pour plus de détails.";
                    }
                }
            })
        }
        "5" => {
            let organizers = data.organizers(transaction).await?;
            Some(html! {
                article {
                    p(lang = "en") {
                        : "This is the 5th season of the Francophone tournament, organized by ";
                        : English.join_html_opt(&organizers);
                        : ". Join ";
                        a(href = "https://discord.gg/wyhPVmquZC") : "the Discord server";
                        : " for details.";
                    }
                    p(lang = "fr") {
                        : "Voici la 5e saison du tournoi francophone, organisée par ";
                        : French.join_html_opt(organizers);
                        : ". Rejoignez ";
                        a(href = "https://discord.gg/wyhPVmquZC") : "le serveur Discord";
                        : " pour plus de détails.";
                    }
                }
            })
        }
        _ => None,
    })
}

pub(crate) struct Setting {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
    pub(crate) default: &'static str,
    pub(crate) default_display: &'static str,
    pub(crate) other: &'static [(&'static str, bool, &'static str)],
    pub(crate) description: &'static str,
}

pub(crate) const S3_SETTINGS: [Setting; 28] = [
    Setting { name: "weirdegg", display: "weird egg", default: "skip", default_display: "Skip Child Zelda", other: &[("shuffle", false, "shuffled weird egg")], description: "weirdegg: skip (défaut: Skip Child Zelda) ou shuffle" },
    Setting { name: "start", display: "starting items/spawns", default: "random", default_display: "random start", other: &[("vanilla", false, "vanilla start")], description: "start: random (défaut) ou vanilla (vanilla spawns, pas de consommables, pas de Deku Shield)" },
    Setting { name: "keysy", display: "keysy", default: "off", default_display: "dungeon small keys not removed", other: &[("on", false, "small keysy")], description: "keysy: off (défaut) ou on" },
    Setting { name: "camc", display: "CAMC", default: "on", default_display: "CAMC", other: &[("off", false, "no CAMC")], description: "camc: on (défaut) ou off" },
    Setting { name: "deku", display: "open Deku", default: "closed", default_display: "closed Deku", other: &[("open", false, "open Deku")], description: "deku: closed (défaut) ou open" },
    Setting { name: "card", display: "Gerudo card", default: "vanilla", default_display: "vanilla Gerudo card", other: &[("shuffle", false, "shuffled Gerudo card")], description: "card: vanilla (défaut) ou shuffle" },
    Setting { name: "merchants", display: "merchants", default: "off", default_display: "vanilla merchants", other: &[("shuffle", false, "shuffled merchants")], description: "merchants: off (défaut) ou shuffle" },
    Setting { name: "ocarina", display: "ocarina", default: "startwith", default_display: "start with ocarina", other: &[("shuffle", false, "shuffled ocarinas")], description: "ocarina: startwith (défaut) ou shuffle" },
    Setting { name: "chubags", display: "bombchu drops", default: "off", default_display: "no bombchu bag", other: &[("on", false, "bombchu bag")], description: "chubags: off (défaut) ou on" },
    Setting { name: "dungeon-er", display: "dungeon ER", default: "off", default_display: "no dungeon ER", other: &[("on", false, "dungeon ER")], description: "dungeon-er: off (défaut), on ou mixed" },
    Setting { name: "songs", display: "songs", default: "songs", default_display: "songs on songs", other: &[("anywhere", false, "songsanity anywhere"), ("dungeon", true, "songsanity dungeon rewards")], description: "songs: songs (défaut), anywhere ou dungeon (difficile)" },
    Setting { name: "cows", display: "cows", default: "off", default_display: "no cowsanity", other: &[("on", false, "cowsanity")], description: "cows: off (défaut) ou on" },
    Setting { name: "shops", display: "shops", default: "off", default_display: "no shopsanity", other: &[("random", false, "shopsanity random")], description: "shops: off (défaut) ou random" },
    Setting { name: "scrubs", display: "scrubs", default: "off", default_display: "no scrubsanity", other: &[("affordable", false, "scrubsanity affordable")], description: "scrubs: off (défaut) ou affordable" },
    Setting { name: "skulls", display: "tokens", default: "off", default_display: "no tokensanity", other: &[("dungeons", false, "tokensanity dungeon"), ("overworld", true, "tokensanity overworld"), ("all", true, "tokensanity all")], description: "skulls: off (défaut), dungeons, overworld (difficile) ou all (difficile)" },
    Setting { name: "bosskeys", display: "boss keys", default: "dungeon", default_display: "own dungeon boss keys", other: &[("anywhere", false, "boss keys anywhere")], description: "bosskeys: dungeon (défaut) ou anywhere" },
    Setting { name: "warps", display: "warps/owls", default: "off", default_display: "vanilla warps", other: &[("on", false, "shuffled warps")], description: "warps: off (défaut) ou on" },
    Setting { name: "dot", display: "Door of Time", default: "open", default_display: "open Door of Time", other: &[("closed", false, "closed Door of Time")], description: "dot: open (défaut) ou closed" },
    Setting { name: "fountain", display: "fountain", default: "closed", default_display: "closed fountain", other: &[("open", false, "open fountain")], description: "fountain: closed (défaut) ou open" },
    Setting { name: "boss-er", display: "boss ER", default: "off", default_display: "no boss ER", other: &[("on", false, "boss ER")], description: "boss-er: off (défaut) ou on" },
    Setting { name: "1major", display: "1 major item per dungeon", default: "off", default_display: "no major items per dungeon restriction", other: &[("on", false, "1 major item per dungeon")], description: "1major: off (défaut) ou on" },
    Setting { name: "bridge", display: "rainbow bridge", default: "6meds", default_display: "6 medallions bridge", other: &[("4meds", false, "4 medallions bridge"), ("5meds", false, "5 medallions bridge"), ("stones", false, "3 stones bridge"), ("vanilla", false, "vanilla bridge"), ("5dungeons", false, "5 dungeons bridge"), ("6dungeons", false, "6 dungeons bridge"), ("7dungeons", false, "7 dungeons bridge"), ("8dungeons", false, "8 dungeons bridge"), ("9dungeons", false, "9 dungeons bridge"), ("precompleted", false, "2 pre-completed dungeons")], description: "bridge: <4–6>meds (GBK 6 meds, défaut: 6), stones (3 stones, GBK 6 rewards), vanilla (GBK 6 meds), <5–9>dungeons, precompleted (9 rewards, 2 pre-completed dungeons, map/compass gives info)" },
    Setting { name: "shortcuts", display: "shortcuts", default: "off", default_display: "no shortcuts", other: &[("random", true, "random shortcuts")], description: "shortcuts: off (défaut) ou on (difficile)" },
    Setting { name: "mixed-er", display: "mixed ER", default: "off", default_display: "no mixed ER", other: &[("on", true, "mixed ER")], description: "mixed-er: off (défaut) ou on (difficile: intérieurs et grottos mixés)" },
    Setting { name: "keysanity", display: "keysanity", default: "off", default_display: "own dungeon small keys", other: &[("on", true, "small keys anywhere"), ("keyrings", true, "keyrings anywhere")], description: "keysanity: off (défaut), on (difficile) ou keyrings (difficile)" },
    Setting { name: "trials", display: "trials", default: "0", default_display: "0 trials", other: &[("random", true, "random trials")], description: "trials: 0 (défaut) ou random (difficile)" },
    Setting { name: "itempool", display: "item pool", default: "balanced", default_display: "balanced item pool", other: &[("minimal", true, "minimal item pool"), ("scarce", true, "scarce item pool")], description: "itempool: balanced (défaut), minimal (difficile) ou scarce (difficile)" },
    Setting { name: "reachable", display: "reachable locations", default: "all", default_display: "all locations reachable", other: &[("required", true, "required only")], description: "reachable: all (défaut) ou required (difficile)" },
];

pub(crate) const S4_SETTINGS: [Setting; 27] = [
    Setting { name: "camc", display: "CAMC", default: "on", default_display: "CAMC", other: &[("off", false, "no CAMC")], description: "camc: on (default) or off" },
    Setting { name: "start-weirdegg", display: "start & weird egg", default: "random-skip", default_display: "random start & Skip Child Zelda", other: &[("vanilla-shuffle", false, "vanilla start & shuffled weird egg")], description: "start-weirdegg: random-skip (default: random start & Skip Child Zelda) or vanilla-shuffle (vanilla start & shuffled weird egg)" },
    Setting { name: "keysy", display: "keysy", default: "off", default_display: "dungeon small keys not removed", other: &[("on", false, "small keysy")], description: "keysy: off (default) or on" },
    Setting { name: "deku", display: "open Deku", default: "closed", default_display: "closed Deku", other: &[("open", false, "open Deku")], description: "deku: closed (default) or open" },
    Setting { name: "card", display: "Gerudo card", default: "vanilla", default_display: "vanilla Gerudo card", other: &[("shuffle", false, "shuffled Gerudo card")], description: "card: vanilla (default) or shuffle" },
    Setting { name: "ocarina", display: "ocarina", default: "startwith", default_display: "start with ocarina", other: &[("shuffle", false, "shuffled ocarinas & free scarecrow")], description: "ocarina: startwith (default) or shuffle (shuffled ocarinas & free scarecrow)" },
    Setting { name: "chubags", display: "bombchu drops", default: "off", default_display: "no bombchu bag", other: &[("on", false, "bombchu bag")], description: "chubags: off (default) or on" },
    Setting { name: "cows", display: "cows", default: "off", default_display: "no cowsanity", other: &[("on", false, "cowsanity")], description: "cows: off (default) or on" },
    Setting { name: "shops", display: "shops", default: "off", default_display: "no shopsanity", other: &[("random", false, "shopsanity random & wallet full")], description: "shops: off (default) or random (shopsanity random & wallet full)" },
    Setting { name: "scrubs", display: "scrubs", default: "off", default_display: "no scrubsanity", other: &[("affordable", false, "scrubsanity affordable")], description: "scrubs: off (default) or affordable" },
    Setting { name: "skulls", display: "tokens", default: "off", default_display: "no tokensanity", other: &[("dungeons", false, "tokensanity dungeon"), ("overworld", true, "tokensanity overworld"), ("all", true, "tokensanity all")], description: "skulls: off (default), dungeons, overworld (hardcore), or all (hardcore)" },
    Setting { name: "boss-er", display: "boss ER", default: "off", default_display: "no boss ER", other: &[("on", false, "boss ER")], description: "boss-er: off (default) or on" },
    Setting { name: "bridge", display: "rainbow bridge", default: "6meds", default_display: "6 medallions bridge", other: &[("4meds-meds", false, "4 medallions bridge (GBK 6 meds)"), ("4meds-dungeons", false, "4 medallions bridge (GBK 6 dungeons)"), ("5meds-meds", false, "5 medallions bridge (GBK 6 meds)"), ("5meds-dungeons", false, "5 medallions bridge (GBK 6 dungeons)"), ("1stones", false, "1 stone bridge"), ("2stones", false, "2 stones bridge"), ("3stones", false, "3 stones bridge"), ("vanilla", false, "vanilla bridge"), ("5dungeons", false, "5 dungeons bridge"), ("6dungeons", false, "6 dungeons bridge"), ("7dungeons", false, "7 dungeons bridge"), ("8dungeons", false, "8 dungeons bridge"), ("9dungeons", false, "9 dungeons bridge"), ("1precompleted", false, "1 pre-completed dungeon"), ("2precompleted", false, "2 pre-completed dungeons"), ("3precompleted", false, "3 pre-completed dungeons")], description: "bridge: <4–6>meds (GBK 6 meds, default: 6), <1–3>stones (3 stones, GBK 6 rewards), vanilla (GBK 6 meds), <5–9>dungeons, <1-3>precompleted (9 rewards, map/compass gives info)" },
    Setting { name: "bosskeys", display: "boss keys", default: "dungeon", default_display: "own dungeon boss keys", other: &[("anywhere", false, "boss keys anywhere")], description: "bosskeys: dungeon (default) or anywhere" },
    Setting { name: "warps", display: "warps/owls", default: "off", default_display: "vanilla warps", other: &[("on", false, "shuffled warps")], description: "warps: off (default) or on" },
    Setting { name: "dot", display: "Door of Time", default: "open", default_display: "open Door of Time", other: &[("closed", false, "closed Door of Time")], description: "dot: open (default) or closed" },
    Setting { name: "fountain", display: "fountain", default: "closed", default_display: "closed fountain", other: &[("open", false, "open fountain")], description: "fountain: closed (default) or open" },
    Setting { name: "1major", display: "1 major item per dungeon", default: "off", default_display: "no major items per dungeon restriction", other: &[("on", false, "1 major item per dungeon")], description: "1major: off (default) or on" },
    Setting { name: "dungeon-er", display: "dungeon ER", default: "off", default_display: "no dungeon ER", other: &[("on", false, "dungeon ER")], description: "dungeon-er: off (default), on, or mixed" },
    Setting { name: "songs", display: "songs", default: "songs", default_display: "songs on songs", other: &[("anywhere", false, "songsanity anywhere"), ("dungeon", true, "songsanity dungeon rewards")], description: "songs: songs (default), anywhere, or dungeon (hardcore)" },
    Setting { name: "souls", display: "enemy souls", default: "off", default_display: "no enemy souls", other: &[("bosses", false, "boss souls"), ("all-anywhere", true, "all enemy souls (anywhere)"), ("all-regional", true, "all enemy souls (regional)")], description: "souls: off (default), bosses, all-anywhere (hardcore), or all-regional (hardcore)" },
    Setting { name: "itempool", display: "item pool", default: "balanced", default_display: "balanced item pool", other: &[("minimal", true, "minimal item pool"), ("scarce", true, "scarce item pool")], description: "itempool: balanced (default), minimal (hardcore), or scarce (hardcore)" },
    Setting { name: "shortcuts", display: "shortcuts", default: "off", default_display: "no shortcuts", other: &[("random", true, "random shortcuts")], description: "shortcuts: off (default) or on (hardcore)" },
    Setting { name: "keysanity", display: "keysanity", default: "off", default_display: "own dungeon small keys", other: &[("on", true, "small keys anywhere"), ("keyrings-anywhere", true, "keyrings anywhere"), ("keyrings-regional", true, "keyrings regional")], description: "keysanity: off (default), on (hardcore), keyrings-anywhere (hardcore), or keyrings-regional (hardcore)" },
    Setting { name: "trials", display: "trials", default: "0", default_display: "0 trials", other: &[("random", true, "random trials")], description: "trials: 0 (default) or random (hardcore)" },
    Setting { name: "mixed-er", display: "mixed ER", default: "off", default_display: "no mixed ER", other: &[("on", true, "mixed ER")], description: "mixed-er: off (default) or on (hardcore: intérieurs et grottos mixés)" },
    Setting { name: "reachable", display: "reachable locations", default: "all", default_display: "all locations reachable", other: &[("required", true, "required only")], description: "reachable: all (default) or required (hardcore)" },
];

pub(crate) const S5_SETTINGS: [Setting; 30] = [
    Setting { name: "camc", display: "CAMC", default: "on", default_display: "CAMC", other: &[("off", false, "no CAMC")], description: "camc: on (default) or off" },
    Setting { name: "start-weirdegg", display: "start & weird egg", default: "random-skip", default_display: "random start & Skip Child Zelda", other: &[("vanilla-shuffle", false, "vanilla start & shuffled weird egg")], description: "start-weirdegg: random-skip (default: random start & Skip Child Zelda) or vanilla-shuffle (vanilla start & shuffled weird egg)" },
    Setting { name: "keysy", display: "keysy", default: "off", default_display: "dungeon small keys not removed", other: &[("on", false, "small keysy")], description: "keysy: off (default) or on" },
    Setting { name: "deku", display: "open Deku", default: "closed", default_display: "closed Deku", other: &[("open", false, "open Deku")], description: "deku: closed (default) or open" },
    Setting { name: "card", display: "Gerudo card", default: "vanilla", default_display: "vanilla Gerudo card", other: &[("shuffle", false, "shuffled Gerudo card")], description: "card: vanilla (default) or shuffle" },
    Setting { name: "ocarina", display: "ocarina", default: "startwith", default_display: "start with ocarina", other: &[("shuffle", false, "shuffled ocarinas & free scarecrow")], description: "ocarina: startwith (default) or shuffle (shuffled ocarinas & free scarecrow)" },
    Setting { name: "chubags", display: "bombchu drops", default: "off", default_display: "no bombchu bag", other: &[("on", false, "bombchu bag")], description: "chubags: off (default) or on" },
    Setting { name: "dungeon-er", display: "dungeon ER", default: "off", default_display: "no dungeon ER", other: &[("on", false, "dungeon ER")], description: "dungeon-er: off (default), on, or mixed" },
    Setting { name: "songs", display: "songs", default: "songs", default_display: "songs on songs", other: &[("anywhere", false, "songsanity anywhere"), ("dungeon", false, "songsanity dungeon rewards")], description: "songs: songs (default), anywhere, or dungeon" },
    Setting { name: "cows", display: "cows", default: "off", default_display: "no cowsanity", other: &[("on", false, "cowsanity")], description: "cows: off (default) or on" },
    Setting { name: "shops", display: "shops", default: "off", default_display: "no shopsanity", other: &[("4", false, "shopsanity 4 & wallet full")], description: "shops: off (default) or 4 (shopsanity 4 & wallet full)" },
    Setting { name: "scrubs", display: "scrubs", default: "off", default_display: "no scrubsanity", other: &[("affordable", false, "scrubsanity affordable")], description: "scrubs: off (default) or affordable" },
    Setting { name: "skulls", display: "tokens", default: "off", default_display: "no tokensanity", other: &[("dungeons", false, "tokensanity dungeon"), ("overworld", true, "tokensanity overworld"), ("all", true, "tokensanity all")], description: "skulls: off (default), dungeons, overworld (hardcore), or all (hardcore)" },
    Setting { name: "bosskeys", display: "boss keys", default: "dungeon", default_display: "own dungeon boss keys", other: &[("anywhere", false, "boss keys anywhere")], description: "bosskeys: dungeon (default) or anywhere" },
    Setting { name: "warps", display: "warps/owls", default: "off", default_display: "vanilla warps", other: &[("on", false, "shuffled warps")], description: "warps: off (default) or on" },
    Setting { name: "dot", display: "Door of Time", default: "open", default_display: "open Door of Time", other: &[("closed", false, "closed Door of Time")], description: "dot: open (default) or closed" },
    Setting { name: "fountain", display: "fountain", default: "closed", default_display: "closed fountain", other: &[("open", false, "open fountain")], description: "fountain: closed (default) or open" },
    Setting { name: "1major", display: "1 major item per dungeon", default: "off", default_display: "no major items per dungeon restriction", other: &[("on", false, "1 major item per dungeon")], description: "1major: off (default) or on" },
    Setting { name: "souls", display: "enemy souls", default: "off", default_display: "no enemy souls", other: &[("bosses", false, "boss souls"), ("all-anywhere", true, "all enemy souls (anywhere)"), ("all-regional", true, "all enemy souls (regional)")], description: "souls: off (default), bosses, all-anywhere (hardcore), or all-regional (hardcore)" },
    Setting { name: "th", display: "Triforce Hunt", default: "off", default_display: "no Triforce Hunt", other: &[("on", false, "Triforce Hunt (between 50 and 100)")], description: "th (Triforce Hunt): off (default) or on (entre 50 et 100)" },
    Setting { name: "boss-er", display: "boss ER", default: "off", default_display: "no boss ER", other: &[("on", false, "boss ER")], description: "boss-er: off (default) or on" },
    Setting { name: "frogs", display: "frog song shuffle", default: "off", default_display: "no frog song shuffle", other: &[("on", false, "frog song shuffle")], description: "frogs: off (default) or on" },
    Setting { name: "bridge", display: "rainbow bridge", default: "6meds", default_display: "6 medallions bridge", other: &[("4meds-meds", false, "4 medallions bridge (GBK 6 meds)"), ("4meds-dungeons", false, "4 medallions bridge (GBK 6 dungeons)"), ("5meds-meds", false, "5 medallions bridge (GBK 6 meds)"), ("5meds-dungeons", false, "5 medallions bridge (GBK 6 dungeons)"), ("1stones", false, "1 stone bridge"), ("2stones", false, "2 stones bridge"), ("3stones", false, "3 stones bridge"), ("vanilla", false, "vanilla bridge"), ("5dungeons", false, "5 dungeons bridge"), ("6dungeons", false, "6 dungeons bridge"), ("7dungeons", false, "7 dungeons bridge"), ("8dungeons", false, "8 dungeons bridge"), ("9dungeons", false, "9 dungeons bridge"), ("1precompleted", false, "1 pre-completed dungeon"), ("2precompleted", false, "2 pre-completed dungeons"), ("3precompleted", false, "3 pre-completed dungeons")], description: "bridge: <4–6>meds (GBK 6 meds, default: 6), <1–3>stones (3 stones, GBK 6 rewards), vanilla (GBK 6 meds), <5–9>dungeons, <1-3>precompleted (9 rewards, map/compass gives info)" },
    Setting { name: "mixed-er", display: "mixed ER", default: "off", default_display: "no mixed ER", other: &[("on", true, "mixed ER")], description: "mixed-er: off (default) or on (hardcore: intérieurs et grottos mixés)" },
    Setting { name: "keysanity", display: "keysanity", default: "off", default_display: "own dungeon small keys", other: &[("on", true, "small keys anywhere"), ("keyrings-anywhere", true, "keyrings anywhere"), ("keyrings-regional", true, "keyrings regional")], description: "keysanity: off (default), on (hardcore), keyrings-anywhere (hardcore), or keyrings-regional (hardcore)" },
    Setting { name: "trials", display: "trials", default: "0", default_display: "0 trials", other: &[("random", true, "random trials")], description: "trials: 0 (default) or random (hardcore)" },
    Setting { name: "itempool", display: "item pool", default: "balanced", default_display: "balanced item pool", other: &[("minimal", true, "minimal item pool"), ("scarce", true, "scarce item pool")], description: "itempool: balanced (default), minimal (hardcore), or scarce (hardcore)" },
    Setting { name: "pots", display: "pot shuffle", default: "off", default_display: "no pot shuffle", other: &[("all", true, "pot shuffle")], description: "pots: off (default) or all (hardcore)" },
    Setting { name: "crates", display: "crate shuffle", default: "off", default_display: "no crate shuffle", other: &[("all", true, "crate shuffle")], description: "crates: off (default) or all (hardcore)" },
    Setting { name: "reachable", display: "reachable locations", default: "all", default_display: "all locations reachable", other: &[("required", true, "required only")], description: "reachable: all (default) or required (hardcore)" },
];

pub(crate) fn display_draft_picks(language: Language, all_settings: &[Setting], picks: &draft::Picks) -> String {
    let mut picks_display = Vec::default();
    if picks.get("mq_ok").map(|mq_ok| &**mq_ok).unwrap_or("no") == "ok" || picks.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0") != "0" {
        let mq_dungeons_count = picks.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0");
        picks_display.push(if let French = language {
            if mq_dungeons_count == "1" {
                Cow::Borrowed("1 donjon MQ")
            } else {
                Cow::Owned(format!("{mq_dungeons_count} donjons MQ"))
            }
        } else {
            if mq_dungeons_count == "1" {
                Cow::Borrowed("1 MQ dungeon")
            } else {
                Cow::Owned(format!("{mq_dungeons_count} MQ dungeons"))
            }
        });
    }
    picks_display.extend(all_settings.iter()
        .filter_map(|&Setting { name, other, .. }| picks.get(name).and_then(|pick| other.iter().find(|(other, _, _)| pick == other)).map(|&(value, _, display)| match (name, value) {
            ("mixed-er", "on") => if picks.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off") == "off" {
                Cow::Borrowed(display)
            } else if picks.get("mixed-dungeons").map(|mixed_dungeons| &**mixed_dungeons).unwrap_or("separate") == "mixed" {
                if let French = language {
                    Cow::Borrowed("mixed ER (donjons inclus)")
                } else {
                    Cow::Borrowed("mixed ER (including dungeons)")
                }
            } else {
                if let French = language {
                    Cow::Borrowed("mixed ER (donjons non inclus)")
                } else {
                    Cow::Borrowed("mixed ER (not including dungeons)")
                }
            },
            (_, _) => Cow::Borrowed(display),
        })));
    language.join_str_opt(picks_display).unwrap_or_else(|| if let French = language {
        format!("settings de base")
    } else {
        format!("base settings")
    })
}

pub(crate) fn resolve_s3_draft_settings(picks: &draft::Picks) -> seed::Settings {
    // selected settings
    let weirdegg = picks.get("weirdegg").map(|weirdegg| &**weirdegg).unwrap_or("skip");
    let start = picks.get("start").map(|start| &**start).unwrap_or("random");
    let keysy = picks.get("keysy").map(|keysy| &**keysy).unwrap_or("off");
    let camc = picks.get("camc").map(|camc| &**camc).unwrap_or("on");
    let deku = picks.get("deku").map(|deku| &**deku).unwrap_or("closed");
    let card = picks.get("card").map(|card| &**card).unwrap_or("vanilla");
    let merchants = picks.get("merchants").map(|merchants| &**merchants).unwrap_or("off");
    let ocarina = picks.get("ocarina").map(|ocarina| &**ocarina).unwrap_or("startwith");
    let chubags = picks.get("chubags").map(|chubags| &**chubags).unwrap_or("off");
    let dungeon_er = picks.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off");
    let songs = picks.get("songs").map(|songs| &**songs).unwrap_or("songs");
    let cows = picks.get("cows").map(|cows| &**cows).unwrap_or("off");
    let shops = picks.get("shops").map(|shops| &**shops).unwrap_or("off");
    let scrubs = picks.get("scrubs").map(|scrubs| &**scrubs).unwrap_or("off");
    let skulls = picks.get("skulls").map(|skulls| &**skulls).unwrap_or("off");
    let bosskeys = picks.get("bosskeys").map(|bosskeys| &**bosskeys).unwrap_or("dungeon");
    let warps = picks.get("warps").map(|warps| &**warps).unwrap_or("off");
    let dot = picks.get("dot").map(|dot| &**dot).unwrap_or("open");
    let fountain = picks.get("fountain").map(|fountain| &**fountain).unwrap_or("closed");
    let boss_er = picks.get("boss-er").map(|boss_er| &**boss_er).unwrap_or("off");
    let one_major = picks.get("1major").map(|one_major| &**one_major).unwrap_or("off");
    let bridge = picks.get("bridge").map(|bridge| &**bridge).unwrap_or("6meds");
    let shortcuts = picks.get("shortcuts").map(|shortcuts| &**shortcuts).unwrap_or("off");
    let mixed_er = picks.get("mixed-er").map(|mixed_er| &**mixed_er).unwrap_or("off");
    let keysanity = picks.get("keysanity").map(|keysanity| &**keysanity).unwrap_or("off");
    let trials = picks.get("trials").map(|trials| &**trials).unwrap_or("0");
    let itempool = picks.get("itempool").map(|itempool| &**itempool).unwrap_or("balanced");
    let reachable = picks.get("reachable").map(|reachable| &**reachable).unwrap_or("all");
    // special picks
    let mixed_dungeons = picks.get("mixed-dungeons").map(|mixed_dungeons| &**mixed_dungeons).unwrap_or("separate");
    let mq_dungeons_count = picks.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0");
    // convert to settings JSON
    let mut starting_inventory = vec![
        "farores_wind",
        "lens",
    ];
    if weirdegg != "shuffle" {
        starting_inventory.push("zeldas_letter");
    }
    if ocarina != "shuffle" {
        starting_inventory.push("ocarina");
    }
    let mut mix_entrance_pools = vec![
        "Interior",
        "GrottoGrave",
    ];
    if mixed_dungeons == "mixed" {
        mix_entrance_pools.push("Dungeon");
    }
    collect![
        format!("user_message") => json!("Tournoi Francophone Saison 3"),
        format!("reachable_locations") => match reachable {
            "all" => json!("all"),
            "required" => json!("beatable"),
            _ => unreachable!(),
        },
        format!("bridge") => match bridge {
            "4meds" | "5meds" | "6meds" => json!("medallions"),
            "stones" => json!("stones"),
            "5dungeons" | "6dungeons" | "7dungeons" | "8dungeons" | "9dungeons" | "precompleted" => json!("dungeons"),
            "vanilla" => json!("vanilla"),
            _ => unreachable!(),
        },
        format!("bridge_medallions") => match bridge {
            "4meds" => json!(4),
            "5meds" => json!(5),
            _ => json!(6),
        },
        format!("bridge_rewards") => match bridge {
            "5dungeons" => json!(5),
            "6dungeons" => json!(6),
            "7dungeons" => json!(7),
            "8dungeons" => json!(8),
            _ => json!(9),
        },
        format!("trials_random") => json!(trials == "random"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => match bridge {
            "4meds" | "5meds" | "6meds" | "vanilla" => json!("medallions"),
            "stones" | "5dungeons" | "6dungeons" | "7dungeons" | "8dungeons" | "9dungeons" | "precompleted" => json!("dungeons"),
            _ => unreachable!(),
        },
        format!("ganon_bosskey_rewards") => match bridge {
            "5dungeons" => json!(5),
            "stones" | "6dungeons" => json!(6),
            "7dungeons" => json!(7),
            "8dungeons" => json!(8),
            _ => json!(9),
        },
        format!("shuffle_bosskeys") => if bosskeys == "anywhere" {
            json!("keysanity")
        } else {
            json!("dungeon")
        },
        format!("shuffle_smallkeys") => if keysy == "on" {
            json!("remove")
        } else {
            match keysanity {
                "off" => json!("dungeon"),
                "on" | "keyrings" => json!("keysanity"),
                _ => unreachable!(),
            }
        },
        format!("key_rings_choice") => if keysanity == "keyrings" {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(bridge == "precompleted"),
        format!("open_forest") => if deku == "open" {
            json!("open")
        } else {
            json!("closed_deku")
        },
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(dot == "open"),
        format!("zora_fountain") => json!(fountain),
        format!("gerudo_fortress") => json!("fast"),
        format!("dungeon_shortcuts_choice") => json!(shortcuts),
        format!("starting_age") => json!("random"),
        format!("mq_dungeons_mode") => json!("count"),
        format!("mq_dungeons_count") => json!(mq_dungeons_count.parse::<u8>().unwrap()),
        format!("empty_dungeons_mode") => if bridge == "precompleted" {
            json!("count")
        } else {
            json!("none")
        },
        format!("empty_dungeons_count") => json!(2),
        format!("shuffle_interior_entrances") => if mixed_er == "on" {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_grotto_entrances") => json!(mixed_er == "on"),
        format!("shuffle_dungeon_entrances") => if dungeon_er == "on" {
            json!("simple")
        } else {
            json!("off")
        },
        format!("shuffle_bosses") => if boss_er == "on" {
            json!("full")
        } else {
            json!("off")
        },
        format!("mix_entrance_pools") => json!(mix_entrance_pools),
        format!("owl_drops") => json!(warps == "on"),
        format!("warp_songs") => json!(warps == "on"),
        format!("spawn_positions") => if start == "vanilla" {
            json!([])
        } else {
            json!(["child", "adult"])
        },
        format!("free_bombchu_drops") => json!(chubags == "on"),
        format!("one_item_per_dungeon") => json!(one_major == "on"),
        format!("shuffle_song_items") => match songs {
            "songs" => json!("song"),
            "anywhere" => json!("any"),
            "dungeon" => json!("dungeon"),
            _ => unreachable!(),
        },
        format!("shopsanity") => json!(shops),
        format!("tokensanity") => json!(skulls),
        format!("shuffle_scrubs") => if scrubs == "affordable" {
            json!("low")
        } else {
            json!("off")
        },
        format!("shuffle_child_trade") => if weirdegg == "shuffle" {
            json!(["Weird Egg"])
        } else {
            json!([])
        },
        format!("shuffle_cows") => json!(cows == "on"),
        format!("shuffle_ocarinas") => json!(ocarina == "shuffle"),
        format!("shuffle_gerudo_card") => json!(card == "shuffle"),
        format!("shuffle_beans") => json!(merchants == "shuffle"),
        format!("shuffle_expensive_merchants") => json!(merchants == "shuffle"),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
        ]),
        format!("allowed_tricks") => json!([
            "logic_visible_collisions",
            "logic_grottos_without_agony",
            "logic_fewer_tunic_requirements",
            "logic_rusted_switches",
            "logic_man_on_roof",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_deku_b1_webs_with_bow",
            "logic_dc_scarecrow_gs",
            "logic_dc_jump",
            "logic_lens_botw",
            "logic_child_deadhand",
            "logic_forest_vines",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_bongo",
            "logic_lens_spirit",
            "logic_lens_gtg",
            "logic_lens_castle",
        ]),
        format!("starting_equipment") => if start == "vanilla" {
            json!([])
        } else {
            json!(["deku_shield"])
        },
        format!("starting_inventory") => json!(starting_inventory),
        format!("start_with_consumables") => json!(start != "vanilla"),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(ocarina == "shuffle"),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count_random") => json!(true),
        format!("big_poe_count") => json!(1),
        format!("correct_chest_appearances") => if camc == "on" {
            json!("both")
        } else {
            json!("off")
        },
        format!("hint_dist_user") => json!({
            "name":                  "weekly",
            "gui_name":              "Weekly",
            "description":           "Hint distribution for weekly races. 5 Goal hints, 3 Barren hints, 5 Sometimes hints, 7 Always hints (including 30 Skulltula tokens, Skull Mask, Sheik in Kakariko, and Death Mountain Crater Scrub).",
            "add_locations":         [
                { "location": "Deku Theater Skull Mask", "types": ["always"] },
                { "location": "Sheik in Kakariko", "types": ["always"] },
                { "location": "DMC Deku Scrub", "types": ["always"] },
            ],
            "remove_locations":      [
                { "location": "Ganons Castle Shadow Trial Golden Gauntlets Chest", "types": ["sometimes"] },
                { "location": "Sheik in Forest", "types": ["sometimes"] },
                { "location": "Sheik at Temple", "types": ["sometimes"] },
                { "location": "Sheik in Crater", "types": ["sometimes"] },
                { "location": "Sheik at Colossus", "types": ["sometimes"] },
                { "location": "Song from Royal Familys Tomb", "types": ["sometimes"] },
            ],
            "add_items":             [],
            "remove_items":          [
                { "item": "Zeldas Lullaby", "types": ["goal"] },
            ],
            "dungeons_barren_limit": 1,
            "named_items_required":  true,
            "vague_named_items":     false,
            "use_default_goals":     true,
            "distribution":          {
                "trial":           {"order": 1, "weight": 0.0, "fixed":   0, "copies": 2},
                "entrance_always": {"order": 2, "weight": 0.0, "fixed":   0, "copies": 2},
                "always":          {"order": 3, "weight": 0.0, "fixed":   0, "copies": 2},
                "goal":            {"order": 4, "weight": 0.0, "fixed":   5, "copies": 2},
                "barren":          {"order": 5, "weight": 0.0, "fixed":   3, "copies": 2},
                "entrance":        {"order": 6, "weight": 0.0, "fixed":   4, "copies": 2},
                "sometimes":       {"order": 7, "weight": 0.0, "fixed": 100, "copies": 2},
                "random":          {"order": 8, "weight": 9.0, "fixed":   0, "copies": 2},
                "named-item":      {"order": 9, "weight": 0.0, "fixed":   0, "copies": 2},
                "item":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                "song":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                "overworld":       {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                "dungeon":         {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                "junk":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                "woth":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 2},
                "dual_always":     {"order": 0, "weight": 0.0, "fixed":   0, "copies": 0},
                "dual":            {"order": 0, "weight": 0.0, "fixed":   0, "copies": 0},
                "important_check": {"order": 0, "weight": 0.0, "fixed":   0, "copies": 0},
            },
        }),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "30_skulltulas",
            "40_skulltulas",
            "50_skulltulas",
        ]),
        format!("blue_fire_arrows") => json!(true),
        format!("item_pool_value") => json!(itempool),
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

pub(crate) fn resolve_s4_draft_settings(picks: &draft::Picks) -> seed::Settings {
    // selected settings
    let camc = picks.get("camc").map(|camc| &**camc).unwrap_or("on");
    let start_weirdegg = picks.get("start-weirdegg").map(|start_weirdegg| &**start_weirdegg).unwrap_or("random-skip");
    let keysy = picks.get("keysy").map(|keysy| &**keysy).unwrap_or("off");
    let deku = picks.get("deku").map(|deku| &**deku).unwrap_or("closed");
    let card = picks.get("card").map(|card| &**card).unwrap_or("vanilla");
    let ocarina = picks.get("ocarina").map(|ocarina| &**ocarina).unwrap_or("startwith");
    let chubags = picks.get("chubags").map(|chubags| &**chubags).unwrap_or("off");
    let cows = picks.get("cows").map(|cows| &**cows).unwrap_or("off");
    let shops = picks.get("shops").map(|shops| &**shops).unwrap_or("off");
    let scrubs = picks.get("scrubs").map(|scrubs| &**scrubs).unwrap_or("off");
    let skulls = picks.get("skulls").map(|skulls| &**skulls).unwrap_or("off");
    let boss_er = picks.get("boss-er").map(|boss_er| &**boss_er).unwrap_or("off");
    let bridge = picks.get("bridge").map(|bridge| &**bridge).unwrap_or("6meds");
    let bosskeys = picks.get("bosskeys").map(|bosskeys| &**bosskeys).unwrap_or("dungeon");
    let warps = picks.get("warps").map(|warps| &**warps).unwrap_or("off");
    let dot = picks.get("dot").map(|dot| &**dot).unwrap_or("open");
    let fountain = picks.get("fountain").map(|fountain| &**fountain).unwrap_or("closed");
    let one_major = picks.get("1major").map(|one_major| &**one_major).unwrap_or("off");
    let dungeon_er = picks.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off");
    let songs = picks.get("songs").map(|songs| &**songs).unwrap_or("songs");
    let souls = picks.get("souls").map(|souls| &**souls).unwrap_or("off");
    let itempool = picks.get("itempool").map(|itempool| &**itempool).unwrap_or("balanced");
    let shortcuts = picks.get("shortcuts").map(|shortcuts| &**shortcuts).unwrap_or("off");
    let keysanity = picks.get("keysanity").map(|keysanity| &**keysanity).unwrap_or("off");
    let trials = picks.get("trials").map(|trials| &**trials).unwrap_or("0");
    let mixed_er = picks.get("mixed-er").map(|mixed_er| &**mixed_er).unwrap_or("off");
    let reachable = picks.get("reachable").map(|reachable| &**reachable).unwrap_or("all");
    // special picks
    let mixed_dungeons = picks.get("mixed-dungeons").map(|mixed_dungeons| &**mixed_dungeons).unwrap_or("separate");
    let mq_dungeons_count = picks.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0");
    // convert to settings JSON
    let mut starting_inventory = vec![
        "farores_wind",
        "lens",
    ];
    if start_weirdegg != "vanilla-shuffle" {
        starting_inventory.push("zeldas_letter");
    }
    if ocarina != "shuffle" {
        starting_inventory.push("ocarina");
    }
    let mut mix_entrance_pools = vec![
        "Interior",
        "GrottoGrave",
    ];
    if mixed_dungeons == "mixed" {
        mix_entrance_pools.push("Dungeon");
    }
    collect![
        format!("user_message") => json!("Tournoi Francophone Saison 4"),
        format!("reachable_locations") => match reachable {
            "all" => json!("all"),
            "required" => json!("beatable"),
            _ => unreachable!(),
        },
        format!("bridge") => match bridge {
            "4meds-meds" | "4meds-dungeons" | "5meds-meds" | "5meds-dungeons" | "6meds" => json!("medallions"),
            "1stones" | "2stones" | "3stones" => json!("stones"),
            "5dungeons" | "6dungeons" | "7dungeons" | "8dungeons" | "9dungeons" | "1precompleted" | "2precompleted" | "3precompleted" => json!("dungeons"),
            "vanilla" => json!("vanilla"),
            _ => unreachable!(),
        },
        format!("bridge_medallions") => match bridge {
            "4meds-meds" | "4meds-dungeons" => json!(4),
            "5meds-meds" | "5meds-dungeons" => json!(5),
            _ => json!(6),
        },
        format!("bridge_stones") => match bridge {
            "1stones" => json!(1),
            "2stones" => json!(2),
            _ => json!(3),
        },
        format!("bridge_rewards") => match bridge {
            "5dungeons" => json!(5),
            "6dungeons" => json!(6),
            "7dungeons" => json!(7),
            "8dungeons" => json!(8),
            _ => json!(9),
        },
        format!("trials_random") => json!(trials == "random"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => match bridge {
            "4meds-meds" | "5meds-meds" | "6meds" | "vanilla" => json!("medallions"),
            "4meds-dungeons" | "5meds-dungeons" | "1stones" | "2stones" | "3stones" | "5dungeons" | "6dungeons" | "7dungeons" | "8dungeons" | "9dungeons" | "1precompleted" | "2precompleted" | "3precompleted" => json!("dungeons"),
            _ => unreachable!(),
        },
        format!("ganon_bosskey_rewards") => match bridge {
            "5dungeons" => json!(5),
            "4meds-dungeons" | "5meds-dungeons" | "1stones" | "2stones" | "3stones" | "6dungeons" => json!(6),
            "7dungeons" => json!(7),
            "8dungeons" => json!(8),
            _ => json!(9),
        },
        format!("shuffle_bosskeys") => if bosskeys == "anywhere" {
            json!("keysanity")
        } else {
            json!("dungeon")
        },
        format!("shuffle_smallkeys") => if keysy == "on" {
            json!("remove")
        } else {
            match keysanity {
                "off" => json!("dungeon"),
                "keyrings-regional" => json!("regional"),
                "on" | "keyrings-anywhere" => json!("keysanity"),
                _ => unreachable!(),
            }
        },
        format!("key_rings_choice") => if let "keyrings-regional" | "keyrings-anywhere" = keysanity {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("enhance_map_compass") => json!(matches!(bridge, "1precompleted" | "2precompleted" | "3precompleted")),
        format!("open_forest") => if deku == "open" {
            json!("open")
        } else {
            json!("closed_deku")
        },
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(dot == "open"),
        format!("zora_fountain") => json!(fountain),
        format!("gerudo_fortress") => json!("fast"),
        format!("dungeon_shortcuts_choice") => json!(shortcuts),
        format!("starting_age") => json!("random"),
        format!("mq_dungeons_mode") => json!("count"),
        format!("mq_dungeons_count") => json!(mq_dungeons_count.parse::<u8>().unwrap()),
        format!("empty_dungeons_mode") => if let "1precompleted" | "2precompleted" | "3precompleted" = bridge {
            json!("count")
        } else {
            json!("none")
        },
        format!("empty_dungeons_count") => match bridge {
            "1precompleted" => json!(1),
            "3precompleted" => json!(3),
            _ => json!(2),
        },
        format!("shuffle_interior_entrances") => if mixed_er == "on" {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_grotto_entrances") => json!(mixed_er == "on"),
        format!("shuffle_dungeon_entrances") => if dungeon_er == "on" {
            json!("simple")
        } else {
            json!("off")
        },
        format!("shuffle_bosses") => if boss_er == "on" {
            json!("full")
        } else {
            json!("off")
        },
        format!("mix_entrance_pools") => json!(mix_entrance_pools),
        format!("owl_drops") => json!(warps == "on"),
        format!("warp_songs") => json!(warps == "on"),
        format!("spawn_positions") => if start_weirdegg == "vanilla-shuffle" {
            json!([])
        } else {
            json!(["child", "adult"])
        },
        format!("free_bombchu_drops") => json!(chubags == "on"),
        format!("one_item_per_dungeon") => json!(one_major == "on"),
        format!("shuffle_song_items") => match songs {
            "songs" => json!("song"),
            "anywhere" => json!("any"),
            "dungeon" => json!("dungeon"),
            _ => unreachable!(),
        },
        format!("shopsanity") => json!(shops),
        format!("tokensanity") => json!(skulls),
        format!("shuffle_scrubs") => if scrubs == "affordable" {
            json!("low")
        } else {
            json!("off")
        },
        format!("shuffle_child_trade") => if start_weirdegg == "vanilla-shuffle" {
            json!(["Weird Egg"])
        } else {
            json!([])
        },
        format!("shuffle_cows") => json!(cows == "on"),
        format!("shuffle_ocarinas") => json!(ocarina == "shuffle"),
        format!("shuffle_gerudo_card") => json!(card == "shuffle"),
        format!("shuffle_enemy_spawns") => match souls {
            "off" => json!("off"),
            "bosses" => json!("bosses"),
            "all-anywhere" => json!("all"),
            "all-regional" => json!("regional"),
            _ => unreachable!(),
        },
        format!("disabled_locations") => json!([
            "Deku Theater Skull Mask",
            "Deku Theater Mask of Truth",
        ]),
        format!("allowed_tricks") => json!([
            "logic_visible_collisions",
            "logic_grottos_without_agony",
            "logic_fewer_tunic_requirements",
            "logic_rusted_switches",
            "logic_man_on_roof",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_deku_b1_webs_with_bow",
            "logic_dc_scarecrow_gs",
            "logic_dc_jump",
            "logic_lens_botw",
            "logic_child_deadhand",
            "logic_forest_vines",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_bongo",
            "logic_lens_spirit",
            "logic_lens_gtg",
            "logic_lens_castle",
        ]),
        format!("starting_equipment") => if start_weirdegg == "vanilla-shuffle" {
            json!([])
        } else {
            json!(["deku_shield"])
        },
        format!("starting_inventory") => json!(starting_inventory),
        format!("start_with_consumables") => json!(start_weirdegg != "vanilla-shuffle"),
        format!("start_with_rupees") => json!(shops == "random"),
        format!("skip_reward_from_rauru") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(ocarina == "shuffle"),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(3),
        format!("big_poe_count") => json!(1),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("correct_chest_appearances") => if camc == "on" {
            json!("both")
        } else {
            json!("off")
        },
        format!("correct_potcrate_appearances") => json!("textures_content"),
        format!("hint_dist") => json!("tournament"),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "10_skulltulas",
            "20_skulltulas",
            "30_skulltulas",
            "40_skulltulas",
            "50_skulltulas",
            "unique_merchants",
        ]),
        format!("blue_fire_arrows") => json!(true),
        format!("item_pool_value") => json!(itempool),
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

pub(crate) fn resolve_s5_draft_settings(picks: &draft::Picks) -> seed::Settings {
    // selected settings
    let camc = picks.get("camc").map(|camc| &**camc).unwrap_or("on");
    let start_weirdegg = picks.get("start-weirdegg").map(|start_weirdegg| &**start_weirdegg).unwrap_or("random-skip");
    let keysy = picks.get("keysy").map(|keysy| &**keysy).unwrap_or("off");
    let deku = picks.get("deku").map(|deku| &**deku).unwrap_or("closed");
    let card = picks.get("card").map(|card| &**card).unwrap_or("vanilla");
    let ocarina = picks.get("ocarina").map(|ocarina| &**ocarina).unwrap_or("startwith");
    let chubags = picks.get("chubags").map(|chubags| &**chubags).unwrap_or("off");
    let dungeon_er = picks.get("dungeon-er").map(|dungeon_er| &**dungeon_er).unwrap_or("off");
    let songs = picks.get("songs").map(|songs| &**songs).unwrap_or("songs");
    let cows = picks.get("cows").map(|cows| &**cows).unwrap_or("off");
    let shops = picks.get("shops").map(|shops| &**shops).unwrap_or("off");
    let scrubs = picks.get("scrubs").map(|scrubs| &**scrubs).unwrap_or("off");
    let skulls = picks.get("skulls").map(|skulls| &**skulls).unwrap_or("off");
    let bosskeys = picks.get("bosskeys").map(|bosskeys| &**bosskeys).unwrap_or("dungeon");
    let warps = picks.get("warps").map(|warps| &**warps).unwrap_or("off");
    let dot = picks.get("dot").map(|dot| &**dot).unwrap_or("open");
    let fountain = picks.get("fountain").map(|fountain| &**fountain).unwrap_or("closed");
    let one_major = picks.get("1major").map(|one_major| &**one_major).unwrap_or("off");
    let souls = picks.get("souls").map(|souls| &**souls).unwrap_or("off");
    let th = picks.get("th").map(|th| &**th).unwrap_or("off");
    let boss_er = picks.get("boss-er").map(|boss_er| &**boss_er).unwrap_or("off");
    let frogs = picks.get("frogs").map(|frogs| &**frogs).unwrap_or("off");
    let bridge = picks.get("bridge").map(|bridge| &**bridge).unwrap_or("6meds");
    let mixed_er = picks.get("mixed-er").map(|mixed_er| &**mixed_er).unwrap_or("off");
    let keysanity = picks.get("keysanity").map(|keysanity| &**keysanity).unwrap_or("off");
    let trials = picks.get("trials").map(|trials| &**trials).unwrap_or("0");
    let itempool = picks.get("itempool").map(|itempool| &**itempool).unwrap_or("balanced");
    let pots = picks.get("pots").map(|pots| &**pots).unwrap_or("off");
    let crates = picks.get("crates").map(|crates| &**crates).unwrap_or("off");
    let reachable = picks.get("reachable").map(|reachable| &**reachable).unwrap_or("all");
    // special picks
    let mixed_dungeons = picks.get("mixed-dungeons").map(|mixed_dungeons| &**mixed_dungeons).unwrap_or("separate");
    let mq_dungeons_count = picks.get("mq_dungeons_count").map(|mq_dungeons_count| &**mq_dungeons_count).unwrap_or("0");
    // convert to settings JSON
    let mut starting_inventory = vec![
        "farores_wind",
        "lens",
    ];
    if start_weirdegg != "vanilla-shuffle" {
        starting_inventory.push("zeldas_letter");
    }
    if ocarina != "shuffle" {
        starting_inventory.push("ocarina");
    }
    let mut mix_entrance_pools = vec![
        "Interior",
        "GrottoGrave",
    ];
    if mixed_dungeons == "mixed" {
        mix_entrance_pools.push("Dungeon");
    }
    let triforce_count = rng().random_range(50..=100);
    collect![
        format!("user_message") => json!("Tournoi Francophone Saison 5"),
        format!("password_lock") => json!(true),
        format!("reachable_locations") => match reachable {
            "all" => json!("all"),
            "required" => json!("beatable"),
            _ => unreachable!(),
        },
        format!("triforce_hunt") => match th {
            "on" => json!(true),
            "off" => json!(false),
            _ => unreachable!(),
        },
        format!("triforce_count_per_world") => json!((triforce_count as f32 * match itempool {
            "balanced" => 1.5,
            "scarce" => 1.25,
            "minimal" => 1.0,
            _ => unreachable!(),
        }).round() as u8),
        format!("triforce_goal_per_world") => json!(triforce_count),
        format!("bridge") => match bridge {
            "4meds-meds" | "4meds-dungeons" | "5meds-meds" | "5meds-dungeons" | "6meds" => json!("medallions"),
            "1stones" | "2stones" | "3stones" => json!("stones"),
            "5dungeons" | "6dungeons" | "7dungeons" | "8dungeons" | "9dungeons" | "1precompleted" | "2precompleted" | "3precompleted" => json!("dungeons"),
            "vanilla" => json!("vanilla"),
            _ => unreachable!(),
        },
        format!("bridge_medallions") => match bridge {
            "4meds-meds" | "4meds-dungeons" => json!(4),
            "5meds-meds" | "5meds-dungeons" => json!(5),
            _ => json!(6),
        },
        format!("bridge_stones") => match bridge {
            "1stones" => json!(1),
            "2stones" => json!(2),
            _ => json!(3),
        },
        format!("bridge_rewards") => match bridge {
            "5dungeons" => json!(5),
            "6dungeons" => json!(6),
            "7dungeons" => json!(7),
            "8dungeons" => json!(8),
            _ => json!(9),
        },
        format!("trials_random") => json!(trials == "random"),
        format!("trials") => json!(0),
        format!("shuffle_ganon_bosskey") => match bridge {
            "4meds-meds" | "5meds-meds" | "6meds" | "vanilla" => json!("medallions"),
            "4meds-dungeons" | "5meds-dungeons" | "1stones" | "2stones" | "3stones" | "5dungeons" | "6dungeons" | "7dungeons" | "8dungeons" | "9dungeons" | "1precompleted" | "2precompleted" | "3precompleted" => json!("dungeons"),
            _ => unreachable!(),
        },
        format!("ganon_bosskey_rewards") => match bridge {
            "5dungeons" => json!(5),
            "4meds-dungeons" | "5meds-dungeons" | "1stones" | "2stones" | "3stones" | "6dungeons" => json!(6),
            "7dungeons" => json!(7),
            "8dungeons" => json!(8),
            _ => json!(9),
        },
        format!("open_forest") => if deku == "open" {
            json!("open")
        } else {
            json!("closed_deku")
        },
        format!("open_kakariko") => json!("open"),
        format!("open_door_of_time") => json!(dot == "open"),
        format!("zora_fountain") => json!(fountain),
        format!("gerudo_fortress") => json!("fast"),
        format!("starting_age") => json!("random"),
        format!("mq_dungeons_mode") => json!("count"),
        format!("mq_dungeons_count") => json!(mq_dungeons_count.parse::<u8>().unwrap()),
        format!("empty_dungeons_mode") => if let "1precompleted" | "2precompleted" | "3precompleted" = bridge {
            json!("count")
        } else {
            json!("none")
        },
        format!("empty_dungeons_count") => match bridge {
            "1precompleted" => json!(1),
            "3precompleted" => json!(3),
            _ => json!(2),
        },
        format!("shuffle_interior_entrances") => if mixed_er == "on" {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_grotto_entrances") => json!(mixed_er == "on"),
        format!("shuffle_dungeon_entrances") => if dungeon_er == "on" {
            json!("simple")
        } else {
            json!("off")
        },
        format!("shuffle_bosses") => if boss_er == "on" {
            json!("full")
        } else {
            json!("off")
        },
        format!("mix_entrance_pools") => json!(mix_entrance_pools),
        format!("owl_drops") => json!(warps == "on"),
        format!("warp_songs") => json!(warps == "on"),
        format!("spawn_positions") => if start_weirdegg == "vanilla-shuffle" {
            json!([])
        } else {
            json!(["child", "adult"])
        },
        format!("free_bombchu_drops") => json!(chubags == "on"),
        format!("shuffle_song_items") => match songs {
            "songs" => json!("song"),
            "anywhere" => json!("any"),
            "dungeon" => json!("dungeon"),
            _ => unreachable!(),
        },
        format!("shopsanity") => json!(shops),
        format!("tokensanity") => json!(skulls),
        format!("shuffle_scrubs") => if scrubs == "affordable" {
            json!("low")
        } else {
            json!("off")
        },
        format!("shuffle_child_trade") => if start_weirdegg == "vanilla-shuffle" {
            json!(["Weird Egg"])
        } else {
            json!([])
        },
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
        format!("shuffle_pots") => json!(pots),
        format!("shuffle_crates") => json!(crates),
        format!("shuffle_cows") => json!(cows == "on"),
        format!("shuffle_ocarinas") => json!(ocarina == "shuffle"),
        format!("shuffle_gerudo_card") => json!(card == "shuffle"),
        format!("shuffle_frog_song_rupees") => json!(frogs == "on"),
        format!("shuffle_enemy_spawns") => match souls {
            "off" => json!("off"),
            "bosses" => json!("bosses"),
            "all-anywhere" => json!("all"),
            "all-regional" => json!("regional"),
            _ => unreachable!(),
        },
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("shuffle_smallkeys") => if keysy == "on" {
            json!("remove")
        } else {
            match keysanity {
                "off" => json!("dungeon"),
                "keyrings-regional" => json!("regional"),
                "on" | "keyrings-anywhere" => json!("keysanity"),
                _ => unreachable!(),
            }
        },
        format!("key_rings_choice") => if let "keyrings-regional" | "keyrings-anywhere" = keysanity {
            json!("all")
        } else {
            json!("off")
        },
        format!("shuffle_bosskeys") => if bosskeys == "anywhere" {
            json!("keysanity")
        } else {
            json!("dungeon")
        },
        format!("enhance_map_compass") => json!(matches!(bridge, "1precompleted" | "2precompleted" | "3precompleted")),
        format!("disabled_locations") => json!([
            "Deku Theater Skull Mask",
            "Deku Theater Mask of Truth",
        ]),
        format!("allowed_tricks") => json!([
            "logic_grottos_without_agony",
            "logic_fewer_tunic_requirements",
            "logic_rusted_switches",
            "logic_man_on_roof",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_deku_b1_webs_with_bow",
            "logic_dc_scarecrow_gs",
            "logic_dc_jump",
            "logic_lens_botw",
            "logic_child_deadhand",
            "logic_forest_vines",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_bongo",
            "logic_lens_spirit",
            "logic_lens_gtg",
            "logic_lens_castle",
        ]),
        format!("starting_equipment") => if start_weirdegg == "vanilla-shuffle" {
            json!([])
        } else {
            json!(["deku_shield"])
        },
        format!("starting_inventory") => json!(starting_inventory),
        format!("start_with_consumables") => json!(start_weirdegg != "vanilla-shuffle"),
        format!("start_with_rupees") => json!(shops == "4"),
        format!("skip_reward_from_rauru") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("free_scarecrow") => json!(ocarina == "shuffle"),
        format!("fast_bunny_hood") => json!(true),
        format!("ruto_already_f1_jabu") => json!(true),
        format!("chicken_count") => json!(0),
        format!("big_poe_count") => json!(1),
        format!("hint_dist_user") => json!({
            "name":                  "tournoi_fr",
            "gui_name":              "Tournoi FR",
            "description":           "Hint Distribution for the 5th season of the Francophone Tournament.",
            "add_locations":         [
                { "location": "Sheik in Kakariko", "types": ["always"] },
                { "location": "Deku Tree GS Basement Back Room", "types": ["always"] },
                { "location": "Deku Theater Skull Mask", "types": ["always"] },
            ],
            "remove_locations":      [
                { "location": "Song from Royal Familys Tomb", "types": ["sometimes"] },
                { "location": "Sheik in Forest", "types": ["sometimes"] },
                { "location": "Sheik at Temple", "types": ["sometimes"] },
                { "location": "Sheik in Crater", "types": ["sometimes"] },
                { "location": "Sheik at Colossus", "types": ["sometimes"] },
                { "location": "Ice Cavern Iron Boots Chest", "types": ["sometimes"] },
                { "location": "GF HBA 1500 Points", "types": ["sometimes"] },
                { "location": "GC Maze Left Chest", "types": ["sometimes"] },
                { "location": "GV Chest", "types": ["sometimes"] },
                { "location": "HC Great Fairy Reward", "types": ["sometimes"] },
                { "location": "OGC Great Fairy Reward", "types": ["sometimes"] },
                { "location": "Water Temple River Chest", "types": ["sometimes"] },
                { "location": "Gerudo Training Ground Maze Path Final Chest", "types": ["sometimes"] },
                { "location": "Spirit Temple Silver Gauntlets Chest", "types": ["sometimes"] },
                { "location": "Spirit Temple Mirror Shield Chest", "types": ["sometimes"] },
            ],
            "add_items":             [],
            "remove_items":          [
                { "item": "Zeldas Lullaby", "types": ["goal"] },
                { "item": "Light Arrows", "types": ["goal"] },
            ],
            "dungeons_barren_limit": 1,
            "one_hint_per_goal":     true,
            "named_items_required":  true,
            "vague_named_items":     false,
            "use_default_goals":     true,
            "distribution":          {
                "trial":      {"order":  1, "weight": 0.0, "fixed":   0, "copies": 2},
                "entrance":   {"order":  2, "weight": 0.0, "fixed":   0, "copies": 2},
                "always":     {"order":  3, "weight": 0.0, "fixed":   0, "copies": 2},
                "barren":     {"order":  4, "weight": 0.0, "fixed":   3, "copies": 2},
                "goal":       {"order":  5, "weight": 0.0, "fixed":   5, "copies": 2},
                "sometimes":  {"order":  6, "weight": 0.0, "fixed": 100, "copies": 2},
                "dual":       {"order":  7, "weight": 0.0, "fixed":   0, "copies": 2},
                "random":     {"order":  8, "weight": 9.0, "fixed":   0, "copies": 2},
                "woth":       {"order":  9, "weight": 0.0, "fixed":   0, "copies": 2},
                "item":       {"order":  0, "weight": 0.0, "fixed":   0, "copies": 2},
                "song":       {"order":  0, "weight": 0.0, "fixed":   0, "copies": 2},
                "overworld":  {"order":  0, "weight": 0.0, "fixed":   0, "copies": 2},
                "dungeon":    {"order":  0, "weight": 0.0, "fixed":   0, "copies": 2},
                "junk":       {"order":  0, "weight": 0.0, "fixed":   0, "copies": 2},
                "named-item": {"order":  0, "weight": 0.0, "fixed":   0, "copies": 2},
            },
        }),
        format!("misc_hints") => json!([
            "altar",
            "ganondorf",
            "warp_songs_and_owls",
            "10_skulltulas",
            "20_skulltulas",
            "30_skulltulas",
            "40_skulltulas",
            "50_skulltulas",
        ]),
        format!("correct_chest_appearances") => if camc == "on" {
            json!("both")
        } else {
            json!("off")
        },
        format!("correct_potcrate_appearances") => json!("textures_unchecked"),
        format!("key_appearance_match_dungeon") => json!(true),
        format!("potcrate_textures_specific") => json!([]),
        format!("blue_fire_arrows") => json!(true),
        format!("one_item_per_dungeon") => json!(one_major == "on"),
        format!("item_pool_value") => json!(itempool),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("anything"),
    ]
}
