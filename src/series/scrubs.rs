use crate::{
    event::{Data, InfoError},
    prelude::*,
};

pub(crate) async fn info(
    transaction: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "5" => Some(html! {
            article {
                p {
                    : "Season 5 of the Scrubs tournament is organized by Froppy, picks, ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". ";
                    a(href = "https://discord.gg/hpHngEY") : "Join the Discord server";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/e/2PACX-1vRtASXFkNaSzqJoFSmjDpU2XfClRdogkRAgTsJ7RSCiZwUwkrXNcjF06fO_I8vMWfchkUKCrACXPmyE/pubhtml?gid=566134238") : "Qualifier scores & offline qualifier times";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/ootrs5minuet") : "Minuet brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/ootrs5bolero") : "Bolero brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/ootrs5serenade") : "Serenade brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/ootrs5nocturne") : "Nocturne brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/ootrs5requiem") : "Requiem brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/ootrs5prelude") : "Prelude brackets";
                    }
                }
            }
        }),
        "6" => Some(html! {
            article {
                p {
                    : "Season 6 of the Scrubs tournament is organized by Froppy, picks, ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". ";
                    a(href = "https://discord.gg/hpHngEY") : "Join the Discord server";
                    : " for details.";
                }
                h2 : "See also";
                ul {
                    li {
                        a(href = "https://docs.google.com/spreadsheets/d/e/2PACX-1vQ9W-LpqwoWzIIxMZZyFWDl7-MYQ7v_0z2Ntu8aebGxOJRQ4r5LpCap8jjSuyeeVl0Z_SCCGIZn28b5/pubhtml?gid=566134238") : "Qualifier scores & offline qualifier times";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/s6Minuet") : "Minuet brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/s6Bolero") : "Bolero brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/s6Serenade") : "Serenade brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/s6Nocturne") : "Nocturne brackets";
                    }
                    li {
                        a(href = "https://scrubscentral.challonge.com/s6Requiem") : "Requiem brackets";
                    }
                }
            }
        }),
        _ => None,
    })
}
