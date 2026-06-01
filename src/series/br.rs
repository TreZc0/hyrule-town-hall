use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "1" => {
            let organizers = data.organizers(&mut *transaction).await?;
            Some(html! {
                article {
                    p(lang = "pt") {
                        : "Bem-vindo à primeira temporada da Copa do Brasil de Ocarina of Time Randomizer, idealizado por ";
                        : Portuguese.join_html_opt(&organizers);
                        : ". Verifique o documento de regras para mais detalhes e ";
                        a(href = "https://discord.gg/hJcttRqFGA") : "entre em nosso Discord";
                        : " para ser atualizado acerca do andamento do torneio!";
                    }
                    p(lang = "en") {
                        : "Welcome to the first season of Copa do Brasil, created by ";
                        : English.join_html_opt(organizers);
                        : ". See the rules document for details and ";
                        a(href = "https://discord.gg/hJcttRqFGA") : "join our Discord";
                        : " to stay tuned about the tournament!";
                    }
                }
            })
        }
        _ => None,
    })
}
