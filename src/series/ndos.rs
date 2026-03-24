use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<RawHtml<String>, InfoError> {
    Ok(html! {
        article {
            p {
                : "Day ";
                : data.event;
                : " of the ";
                a(href = "https://docs.google.com/document/d/1xELThZtIctwN-vYtYhUqtd88JigNzabk8OZHANa0gqY/edit") : "9 Days of SAWS";
                : " event, organized by ";
                : English.join_html_opt(data.organizers(transaction).await?);
                : ", will be a ";
                a(href = "https://docs.google.com/document/d/1sbL6Zju943F5qyx4QbTLUsqZqOTMmvqKVbDwJl08SGc/edit") : "Standard Anti-Weekly Settings";
                @match &*data.event {
                    "1" | "9" => : " (S6)";
                    "2" | "6" | "7" => : " (Beginner)";
                    "3" => : " (Advanced)";
                    "4" => : " (S5) + one bonk KO";
                    "5" => : " (Beginner) + mixed pools";
                    "8" => : " (S6) + dungeon ER";
                    _ => @unimplemented
                }
                : " race";
                @match &*data.event {
                    "1" | "3" | "4" | "5" | "7" | "9" => {}
                    "2" | "8" => : " with 2-player co-op teams";
                    "6" => : " with 3-player multiworld teams";
                    _ => @unimplemented
                }
                : ".";
            }
            h2 : "Rules";
            p {
                : "Follow the ";
                a(href = "https://wiki.ootrandomizer.com/index.php?title=Rules#Universal_Rules") : "Universal Rules";
                : " and the ";
                a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "Standard";
                @if data.event == "6" {
                    : " ruleset, with a few exceptions listed below:";
                } else {
                    : " ruleset, with the exception that streaming is not required.";
                }
            }
            @if data.event == "6" {
                ul {
                    li : "Fire Arrow Entry is allowed";
                    li : "Streaming is not required";
                }
            }
            @if let TeamConfig::CoOp | TeamConfig::Multiworld = data.team_config {
                p {
                    : "Each team will be ranked by the average of the finish times of its members. Timing for an individual player ends on the first frame of the cutscene that plays upon killing Ganon. Players are allowed to kill Ganon to stop their timer and then reset their game, allowing them to continue ";
                    @match data.team_config {
                        TeamConfig::CoOp => : "exploring the seed";
                        TeamConfig::Multiworld => : "collecting items for their team";
                        _ => @unimplemented
                    }
                    : " if necessary.";
                }
            }
            h2 : "Settings";
            p {
                : "The seed will be rolled on ";
                a(href = "https://ootrandomizer.com/generatorDev?version=devFenhl_6.9.14") : "version 6.9.14 Fenhl-2";
                : " of the randomizer using the ";
                @match &*data.event {
                    "1" | "4" | "8" | "9" => : "Standard Anti-Weekly Settings (S6)";
                    "2" | "5" | "6" | "7" => : "Standard Anti-Weekly Settings (Beginner)";
                    "3" => : "Standard Anti-Weekly Settings (Advanced)";
                    _ => @unimplemented
                }
                @match &*data.event {
                    "1" | "2" | "3" | "6" | "7" | "9" => : " preset.";
                    "4" | "5" | "8" => : " preset, with the following changes:";
                    _ => @unimplemented
                }
            }
            @match &*data.event {
                "1" | "2" | "3" | "6" | "7" | "9" => {}
                "4" => ul {
                    li : "No dungeon boss shortcuts";
                    li : "Spawn shuffled for both ages";
                    li : "“Fix broken drops” off";
                    li : "Minimal item pool";
                    li : "Blue Fire Arrows off";
                    li : "No ice traps";
                    li : "One Bonk KO";
                    li : "Standard S5 Tournament hint distribution";
                }
                "5" => {
                    ul {
                        li : "All interior entrances shuffled";
                        li : "Grotto entrances shuffled";
                        li : "Dungeon entrances shuffled (including Ganon's Castle)";
                        li : "Overworld entrances shuffled";
                        li : "Mixed entrance pools (interiors, grottos, dungeons, and overworld)";
                        li : "Full spawn shuffle";
                        li : "Gerudo Valley exit to Lake Hylia shuffled (full)";
                        li : "Owl drops shuffled (full)";
                        li : "Warp song destinations shuffled (full)";
                        li : "Blue warps lead to the shuffled entrances of the dungeons they're in";
                    }
                    p : "“Full” one-ways can lead to additional entrances, such as dungeons, bosses, or grottos.";
                }
                "8" => ul {
                    li : "Dungeon entrances shuffled (except Ganon's Castle)";
                    li : "Blue warps lead to the shuffled entrances of the dungeons they're in";
                }
                _ => @unimplemented
            }
        }
    })
}
