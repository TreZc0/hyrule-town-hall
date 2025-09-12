use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "20" => Some(html! {
            article {
                p {
                    : "Welcome to the german ALTTPR mystery tournament series, version 2.0. This is organized by";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". Details will be revealed as the event progresses.";
                }
            }
        }),
        _ => None,
    })
}

