use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "2025" => Some(html! {
            article {
                p {
                    : "Welcome back to ALttPR, welcome back Crosskeys! The 2025 tournament is organised by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://zsr.link/xkeys2025") : "the official document";
                    : " for details.";
                }
            }
        }),
        _ => None,
    })
}
