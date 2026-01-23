use crate::{
    draft,
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

/// The 5 available modes for the German ALTTPR tournament season 9.
/// Each mode corresponds to a different PHP file on boothisman.de.
pub(crate) const MODES: [Mode; 5] = [
    Mode { name: "ambroz1a", display: "Ambroz1a" },
    Mode { name: "crosskeys", display: "Crosskeys" },
    Mode { name: "enemizer", display: "Enemizer" },
    Mode { name: "inverted", display: "Inverted" },
    Mode { name: "open", display: "Open" },
];

#[derive(Clone, Copy)]
pub(crate) struct Mode {
    pub(crate) name: &'static str,
    pub(crate) display: &'static str,
}

/// Given the draft picks, returns which mode should be used for the given game number.
pub(crate) fn mode_for_game(picks: &draft::Picks, game: i16) -> Option<&'static Mode> {
    let mode_name = match game {
        1 => picks.get("game1_mode")?,
        2 => picks.get("game2_mode")?,
        3 => picks.get("game3_mode")?,
        _ => return None,
    };
    MODES.iter().find(|m| m.name == mode_name.as_ref())
}

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "9bracket" | "9swissa" | "9swissb" => Some(html! {
            article {
                p {
                    : "Willkommen zum 9. deutschen ALTTPR Turnier! Organisiert von ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
            }
        }),
        _ => None,
    })
}
