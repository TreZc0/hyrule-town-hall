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
                p(lang = "en") {
                    : "This is a friendly invitational tournament organised by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". The tournament is mainly aimed at players with an intermediate level. It allows players to play against each other in a friendly and fun environment and get their first taste of restreaming.";
                }
                p(lang = "fr") {
                    : "Voici la 1ère saison du tournoi. Rejoignez ";
                    a(href = "https://discord.gg/YKvbQSBT5") : "le serveur Discord";
                    : " pour plus de détails.";
                }
                p(lang = "fr") {
                    : "Voir le ";
                    a(href = "https://docs.google.com/document/d/1qXnZTj-2voLKHB0D8Yv9_les7GRInoOwvMW6qMcJkwk/edit") : "règlement du tournoi";
                }
            }
        }),
        "2" => Some(html! {
            article {
                p(lang = "en") {
                    : "WeTryToBeBetter is a friendly invitational tournament organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". The tournament is entering its 2nd season after a successful first edition. It's aimed at players with an intermediate level and give them a chance to compete in a friendly, fun environment and get a taste about restreaming.";
                }
                p(lang = "fr") {
                    : "WeTryToBeBetter est un tournoi amical organisé par ";
                    : French.join_html_opt(data.organizers(transaction).await?);
                    : ". Le tournoi entame sa 2e saison après une première édition réussie. Il s'adresse principalement aux joueurs de niveau intermédiaire et leur donne l'opportunité de s'affronter dans un environnement fun et décontracté et de s'initier au restreaming.";
                }
                p(lang = "fr") {
                    : "Rejoignez ";
                    a(href = "https://discord.gg/ZmNKqrvfcR") : "le serveur Discord";
                    : " pour plus de détails";
                }
                p(lang = "fr") {
                    : "Voir le ";
                    a(href = "https://docs.google.com/document/d/1B6KWh_VK2udpkLOARNVOP4-CTULzX7Z46CiKbG2hcQU/edit") : "règlement du tournoi";
                }
            }
        }),
        _ => None,
    })
}
