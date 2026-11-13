//! Discord is an entry point to the same volunteer workflow as the website.

use super::*;
use crate::event::roles::{self, pairs};
type Error = Box<dyn std::error::Error + Send + Sync>;

fn ids(value: &str) -> Option<(Id<Races>, Id<RoleBindings>)> {
    let mut parts = value.split('_');
    Some((
        Id::from(parts.next()?.parse::<u64>().ok()?),
        Id::from(parts.next()?.parse::<u64>().ok()?),
    ))
}

pub(crate) fn modal_payload(
    race: Id<Races>,
    binding: Id<RoleBindings>,
    notes: bool,
    pair: bool,
) -> serde_json::Value {
    let mut fields = Vec::new();
    if notes {
        fields.push(json!({"type":18,"label":"Notes for organizers (optional)","component":{"type":4,"custom_id":"notes","style":1,"required":false,"max_length":60}}));
    }
    if pair {
        fields.push(json!({"type":18,"label":"Only sign up together with (optional)","description":"Leave empty for solo signup. Your partner must accept before you can be selected together.","component":{"type":5,"custom_id":"partner","required":false,"min_values":0,"max_values":1}}));
    }
    json!({"type":9,"data":{"custom_id":format!("volunteer_signup_form_{}_{}",race,binding),"title":"Volunteer signup","components":fields}})
}

pub(crate) async fn button(
    ctx: &DiscordCtx,
    interaction: &ComponentInteraction,
    params: &str,
) -> Result<(), Error> {
    let Some((race, binding)) = ids(params) else {
        return Ok(());
    };
    let flags = params.split('_').skip(2).collect_vec();
    if flags.len() != 2 {
        interaction.create_response(ctx,CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().ephemeral(true).content("Please click the race's Sign up button again to refresh these signup choices."))).await?;
        return Ok(());
    }
    let notes = flags[0] == "1";
    let pair = flags[1] == "1";
    if notes || pair {
        ctx.http
            .create_interaction_response(
                interaction.id,
                &interaction.token,
                &modal_payload(race, binding, notes, pair),
                Vec::new(),
            )
            .await?;
    } else {
        interaction.defer_ephemeral(ctx).await?;
        let message = submit(ctx, interaction.user.id, race, binding, "", None).await?;
        interaction
            .edit_response(
                ctx,
                EditInteractionResponse::new()
                    .content(message)
                    .components(Vec::new()),
            )
            .await?;
    }
    Ok(())
}

fn inputs(data: &ModalInteractionData) -> Result<(String, Option<UserId>), String> {
    let mut notes = String::new();
    let mut partner = None;
    let mut seen = HashSet::new();
    for row in &data.components {
        for component in row.components.iter().chain(row.component.iter()) {
            match component {
                ActionRowComponent::InputText(input)
                    if input.custom_id == "notes" && seen.insert("notes") =>
                {
                    notes = input.value.clone().unwrap_or_default()
                }
                ActionRowComponent::SelectMenu(input)
                    if input.custom_id.as_deref() == Some("partner") && seen.insert("partner") =>
                {
                    if input.values.len() > 1 {
                        return Err("Choose at most one partner.".into());
                    }
                    partner = input
                        .values
                        .first()
                        .map(|id| id.parse::<UserId>())
                        .transpose()
                        .map_err(|_| "Invalid partner selection.".to_owned())?;
                }
                _ => return Err("Please reopen the signup form and try again.".into()),
            }
        }
    }
    Ok((notes, partner))
}

pub(crate) async fn modal(
    ctx: &DiscordCtx,
    interaction: &ModalInteraction,
    params: &str,
) -> Result<(), Error> {
    interaction.defer_ephemeral(ctx).await?;
    let message = match (ids(params), inputs(&interaction.data)) {
        (Some((race, binding)), Ok((notes, partner))) => {
            submit(ctx, interaction.user.id, race, binding, &notes, partner).await?
        }
        (_, Err(message)) => message,
        _ => "This signup form is no longer valid. Please reopen it.".into(),
    };
    interaction
        .edit_response(ctx, EditInteractionResponse::new().content(message))
        .await?;
    Ok(())
}

async fn submit(
    ctx: &DiscordCtx,
    discord_user: UserId,
    race: Id<Races>,
    binding_id: Id<RoleBindings>,
    notes: &str,
    partner: Option<UserId>,
) -> Result<String, Error> {
    let (pool, http) = {
        let data = ctx.data.read().await;
        (
            data.get::<DbPool>().expect("database").clone(),
            data.get::<HttpClient>().expect("HTTP client").clone(),
        )
    };
    // Validate a selected Discord identity before holding the allocation lock.
    if let Some(partner) = partner {
        if partner == discord_user {
            return Ok("Choose another volunteer as your partner.".into());
        }
        if partner.to_user(ctx).await?.bot {
            return Ok("A bot cannot be a volunteer partner.".into());
        }
    }
    let mut tx = pool.begin().await?;
    pairs::lock(&mut tx).await?;
    let Some((series, event)) =
        sqlx::query_as::<_, (Series, String)>("SELECT series,event FROM races WHERE id=$1")
            .bind(race)
            .fetch_optional(&mut *tx)
            .await?
    else {
        return Ok("This race is no longer available.".into());
    };
    let Some(data) = event::Data::new(&mut tx, series, &event).await? else {
        return Ok("This event is no longer available.".into());
    };
    let Some(user) = User::from_discord(&mut *tx, discord_user).await? else {
        return Ok(format!(
            "Connect your Discord account on Hyrule Town Hall first: {}/",
            base_uri()
        ));
    };
    let url = format!(
        "{}/event/{}/{}/races/{}/signups",
        base_uri(),
        series.slug(),
        event,
        race
    );
    let result=async {
        pairs::check_race(&mut tx,&data,race,partner.is_some()).await?;
        let binding=pairs::binding(&mut tx,&data,binding_id).await?;
        let note=pairs::notes(notes,binding.allow_signup_notes).map_err(roles::Error::Signup)?;
        if let Some(partner)=partner {
            let partner=User::from_discord(&mut *tx,partner).await?.ok_or_else(||roles::Error::Signup("Your partner needs a linked Hyrule Town Hall account first.".into()))?;
            let (pair,created)=pairs::create(&mut tx,&data,race,binding_id,user.id,partner.id,notes).await?;
            Ok::<_,roles::Error>(Some((pair,created)))
        } else {
            if roles::Signup::active_for_user(&mut tx,race,binding_id,user.id).await? || pairs::outgoing(&mut tx,race,binding_id,user.id).await?.is_some() {
                return Err(roles::Error::Signup("You already have a signup or outgoing pair invitation for this role.".into()));
            }
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM signups WHERE race_id=$1 AND role_binding_id=$2 AND status='confirmed'")
                .bind(race).bind(i64::from(binding_id)).fetch_one(&mut *tx).await?;
            if count>=i64::from(binding.max_count) { return Err(roles::Error::Signup("This role is full.".into())); }
            pairs::eligible(&mut tx,&binding,user.id,true).await?;
            roles::Signup::create(&mut tx,race,binding_id,user.id,note).await?;
            Ok(None)
        }
    }.await;
    match result {
        Ok(Some((pair, created))) => {
            tx.commit().await?;
            let delivered = if created {
                roles::sync_signup_discord_roles(&pool, ctx, &data, binding_id, user.id).await;
                pairs::notify(&pool, ctx, pair.id).await
            } else {
                true
            };
            Ok(format!(
                "{}\n{}\nReview or share the invitation: {}{}",
                pair.label(),
                if delivered {
                    "You will only be considered together after your partner accepts."
                } else {
                    "The invitation was created, but I couldn't DM your partner. Please share the link."
                },
                base_uri(),
                pair.url(series, &event)
            ))
        }
        Ok(None) => {
            tx.commit().await?;
            roles::sync_signup_discord_roles(&pool, ctx, &data, binding_id, user.id).await;
            speedgaming_export::schedule_sync(pool.clone(), http.clone());
            let _ = volunteer_requests::update_volunteer_post_for_race(&pool, ctx, race).await;
            crate::zsr_export::schedule_volunteer_api_call(pool.clone(), http.clone(), race);
            Ok(format!(
                "Your signup is awaiting organizer approval.\n{url}"
            ))
        }
        Err(roles::Error::Signup(message)) => {
            tx.rollback().await?;
            Ok(format!("{message}\nReview your signup or try again: {url}"))
        }
        Err(error) => {
            tx.rollback().await?;
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_fields_follow_role_settings() {
        for (notes, pair, count) in [(true, true, 2), (true, false, 1), (false, true, 1)] {
            let payload = modal_payload(Id::from(1_i64), Id::from(2_i64), notes, pair);
            let fields = payload["data"]["components"].as_array().unwrap();
            assert_eq!(fields.len(), count);
            for field in fields {
                assert_eq!(field["type"], 18);
                assert_eq!(field["component"]["required"], false);
            }
        }
    }

    #[test]
    fn accepts_label_wrapped_and_empty_optional_inputs() {
        let data: ModalInteractionData =
            serde_json::from_value(json!({"custom_id":"test","components":[
                {"type":18,"component":{"type":4,"custom_id":"notes","value":"With Bob"}},
                {"type":18,"component":{"type":5,"custom_id":"partner","values":["42"]}}
            ]}))
            .unwrap();
        let (notes, partner) = inputs(&data).unwrap();
        assert_eq!(notes, "With Bob");
        assert_eq!(partner, Some(UserId::new(42)));
        let empty: ModalInteractionData =
            serde_json::from_value(json!({"custom_id":"test","components":[]})).unwrap();
        assert_eq!(inputs(&empty).unwrap(), (String::new(), None));
    }
}
