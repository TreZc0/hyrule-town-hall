use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "3" => Some(html! {
            article {
                p {
                    : "This is the 3rd co-op tournament, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/1hzTrwpKKfgCxtMnRC32xaF390zkAnT01Fr-jS5ummR0/edit") : "the official document";
                    : " for details.";
                }
            }
        }),
        _ => None,
    })
}

pub(crate) fn async_rules(async_kind: AsyncKind) -> RawHtml<String> {
    html! {
        p : "Rules:";
        ol {
            @match async_kind {
                AsyncKind::Qualifier1 | AsyncKind::Qualifier2 | AsyncKind::Qualifier3 | AsyncKind::Seeding => @unimplemented
                AsyncKind::Tiebreaker1 => li : "In order to qualify for a brackets wildcard, your team must be among the first two to finish this seed, either as an async or live race.";
                AsyncKind::Tiebreaker2 => @unimplemented
            }
            li : "You must start the seed within 30 minutes of obtaining it and submit your time within 30 minutes of the last finish. Any additional time taken will be added to your final time. If anything prevents you from obtaining the seed/submitting your time, please DM an admin (or ping the Discord role) to get it sorted out.";
            li : "You must not stream your run, but you must have video proof of it. Please simply record it and upload it to YouTube. You will be asked to provide a link to that video after you finish.";
            li {
                : "This should be run like an actual race. In the event of a technical issue, teams are allowed to invoke the ";
                a(href = "https://docs.google.com/document/d/e/2PACX-1vQd3S28r8SOBy-4C5Lxeu6nFAYpWgQqN9lCEKhLGTT3zcaXDSKj0iUnZv6UPo_GargUVQx5F-wOPUtJ/pub") : "Fair Play Agreement";
                : " and have up to a 15 minute time where the affected runner can try to catch back up. If you do this, you must fill out the appropriate field when submitting your time so it can be authenticated.";
            }
        }
    }
}
