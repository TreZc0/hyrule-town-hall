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
                    : "This is the first charity beginner tournament for the ";
                    a(href = "https://autismsociety.org/") : "Autism of Society of America";
                    : ", organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ".";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://github.com/Queenhelena/Zootr-Charity") : "Tournament format, rules, and settings";
                    }
                }
            }
        }),
        _ => None,
    })
}
