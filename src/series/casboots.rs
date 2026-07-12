use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "2026" => Some(html! {
            article {
                p {
                    : "Welcome to Casboots 2026! The tournament is organised by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
            }
        }),
        _ => None,
    })
}
