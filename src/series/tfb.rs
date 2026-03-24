use {
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
