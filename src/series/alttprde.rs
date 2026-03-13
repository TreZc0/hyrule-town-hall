use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

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
