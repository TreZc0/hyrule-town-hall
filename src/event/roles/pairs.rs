//! Together-only volunteer applications. Invitations are not signups until accepted.

use super::*;

#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Pair {
    pub id: i64,
    pub race_id: Id<Races>,
    pub role_binding_id: i32,
    pub inviter_user_id: Id<Users>,
    pub invitee_user_id: Id<Users>,
    pub state: String,
    pub inviter_notes: Option<String>,
    pub accepted_at: Option<DateTime<Utc>>,
}

impl Pair {
    pub fn binding_id(&self) -> Id<RoleBindings> {
        Id::from(i64::from(self.role_binding_id))
    }
    pub fn contains(&self, user: Id<Users>) -> bool {
        user == self.inviter_user_id || user == self.invitee_user_id
    }
    pub fn url(&self, series: Series, event: &str) -> String {
        format!(
            "/event/{}/{}/races/{}/volunteer-pairs/{}",
            series.slug(),
            event,
            self.race_id,
            self.id
        )
    }
    pub fn label(&self) -> &str {
        match self.state.as_str() {
            "awaiting_partner" => "Awaiting partner acceptance",
            "pending" => "Awaiting organizer decision",
            "confirmed" => "Both confirmed",
            "declined" => "Declined",
            "expired" => "Invitation expired",
            _ => "Cancelled",
        }
    }
}

pub(crate) async fn lock(tx: &mut Transaction<'_, Postgres>) -> sqlx::Result<()> {
    // Shared by solo and pair allocation writes. No network calls under this lock.
    sqlx::query("SELECT pg_advisory_xact_lock(724930, 1)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn expire(tx: &mut Transaction<'_, Postgres>) -> sqlx::Result<()> {
    sqlx::query("UPDATE volunteer_signup_pairs p SET state='expired',updated_at=NOW() FROM races r WHERE p.race_id=r.id AND p.state='awaiting_partner' AND (r.start<=NOW() OR r.ignored)")
        .execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn set_binding_options(
    tx: &mut Transaction<'_, Postgres>,
    id: Id<RoleBindings>,
    notes: bool,
    pairs: bool,
) -> sqlx::Result<()> {
    lock(tx).await?;
    sqlx::query(
        "UPDATE role_bindings SET allow_signup_notes=$2,allow_paired_signups=$3 WHERE id=$1",
    )
    .bind(i64::from(id))
    .bind(notes)
    .bind(pairs)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) fn notes(value: &str, allowed: bool) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !allowed {
        return Err(
            "Race signup notes are disabled for this role. Please refresh the signup form.".into(),
        );
    }
    if value.chars().count() > 60 {
        return Err("Signup notes must be at most 60 characters.".into());
    }
    Ok(Some(value.to_owned()))
}

pub(crate) async fn signup_fields(
    tx: &mut Transaction<'_, Postgres>,
    binding: &EffectiveRoleBinding,
    me: Id<Users>,
) -> Result<RawHtml<String>, Error> {
    let candidates: Vec<(Id<Users>, String, Option<String>)> = if binding.allow_paired_signups {
        sqlx::query_as("SELECT u.id,COALESCE(u.discord_display_name,u.discord_username,u.racetime_display_name,'Volunteer'),u.discord_username FROM users u WHERE u.id<>$1 AND u.discord_id IS NOT NULL AND ($3 OR EXISTS(SELECT 1 FROM role_requests r WHERE r.user_id=u.id AND r.role_binding_id=$2 AND r.status='approved')) ORDER BY 2,u.id")
            .bind(me).bind(i64::from(binding.id)).bind(binding.auto_approve).fetch_all(&mut **tx).await?
    } else {
        Vec::new()
    };
    Ok(html! {
        @if binding.allow_signup_notes {
            label(for=format!("signup-notes-{}",binding.id)) : "Notes for organizers (optional)";
            input(type="text",id=format!("signup-notes-{}",binding.id),name="notes",maxlength="60",size="30");
        }
        @if binding.allow_paired_signups {
            label(for=format!("signup-partner-{}",binding.id)) : "Only sign up together with (optional)";
            select(id=format!("signup-partner-{}",binding.id),name="partner") {
                option(value="") : "Sign up individually";
                @for (id,name,discord) in candidates {
                    option(value=id.to_string()) : format!("{}{}",name,discord.map_or(String::new(),|d|format!(" (@{d})")));
                }
            }
            p : "Choosing a partner sends them an invitation. You will only be considered together after they accept. Withdrawing cancels both signups.";
        }
    })
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Signup(message.into())
}

pub(crate) async fn for_race(
    tx: &mut Transaction<'_, Postgres>,
    race: Id<Races>,
) -> sqlx::Result<Vec<Pair>> {
    sqlx::query_as("SELECT * FROM volunteer_signup_pairs WHERE race_id=$1 ORDER BY created_at,id")
        .bind(race)
        .fetch_all(&mut **tx)
        .await
}

pub(crate) async fn load(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> sqlx::Result<Option<Pair>> {
    sqlx::query_as("SELECT * FROM volunteer_signup_pairs WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
}

pub(crate) async fn signup_pair(
    tx: &mut Transaction<'_, Postgres>,
    id: Id<Signups>,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>("SELECT pair_id FROM signups WHERE id=$1")
        .bind(i64::from(id))
        .fetch_optional(&mut **tx)
        .await
        .map(Option::flatten)
}

pub(crate) async fn outgoing(
    tx: &mut Transaction<'_, Postgres>,
    race: Id<Races>,
    binding: Id<RoleBindings>,
    user: Id<Users>,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT id FROM volunteer_signup_pairs WHERE race_id=$1 AND role_binding_id=$2 AND inviter_user_id=$3 AND state='awaiting_partner'")
        .bind(race).bind(i64::from(binding)).bind(user).fetch_optional(&mut **tx).await
}

pub(crate) async fn binding(
    tx: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    id: Id<RoleBindings>,
) -> Result<EffectiveRoleBinding, Error> {
    EffectiveRoleBinding::for_event(tx, data.series, &data.event)
        .await?
        .into_iter()
        .find(|b| b.id == id && !b.is_disabled)
        .ok_or_else(|| invalid("This role is unavailable for this event."))
}

pub(crate) async fn check_race(
    tx: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    race: Id<Races>,
    require_schedule: bool,
) -> Result<(), Error> {
    let available: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM races WHERE id=$1 AND series=$2 AND event=$3 AND (start>NOW() OR (start IS NULL AND NOT $4)) AND NOT ignored AND async_start1 IS NULL AND async_start2 IS NULL AND async_start3 IS NULL AND NOT EXISTS(SELECT 1 FROM races p WHERE p.companion_race_id=$1))")
        .bind(race).bind(data.series.to_string()).bind(&*data.event).bind(require_schedule).fetch_one(&mut **tx).await?;
    if data.is_ended() || !available {
        return Err(invalid(
            "This race is unavailable for signup. Paired signups require an upcoming scheduled live race. Open the primary race's signup page.",
        ));
    }
    Ok(())
}

pub(crate) async fn check_export(
    tx: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    binding: &EffectiveRoleBinding,
) -> Result<(), Error> {
    let enabled: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM speedgaming_exports e JOIN speedgaming_export_languages l ON l.export_id=e.id WHERE e.series=$1 AND e.event=$2 AND e.enabled AND e.archived_at IS NULL AND e.export_volunteers AND l.language=$3)")
        .bind(data.series.to_string()).bind(&*data.event).bind(binding.language).fetch_one(&mut **tx).await?;
    if enabled && matches!(binding.role_type_name.as_str(), "Commentary" | "Tracking") {
        return Err(invalid(
            "Together-only signup is unavailable while automatic SpeedGaming volunteer signup is enabled for this role. Contact the organizers.",
        ));
    }
    Ok(())
}

pub(crate) async fn eligible(
    tx: &mut Transaction<'_, Postgres>,
    binding: &EffectiveRoleBinding,
    user: Id<Users>,
    consenting: bool,
) -> Result<(), Error> {
    let account = User::from_id(&mut **tx, user)
        .await?
        .ok_or_else(|| invalid("The selected volunteer needs a Hyrule Town Hall account."))?;
    if account.discord.is_none() {
        return Err(invalid("Both volunteers must link their Discord accounts."));
    }
    if !RoleRequest::approved_for_user(tx, binding.id, user).await? {
        if !binding.auto_approve {
            return Err(invalid(
                "Both volunteers must be approved for this role first.",
            ));
        }
        if consenting {
            RoleRequest::ensure_approved_for_auto_approve(tx, binding.id, user).await?;
        }
    }
    Ok(())
}

async fn capacity(
    tx: &mut Transaction<'_, Postgres>,
    race: Id<Races>,
    binding: &EffectiveRoleBinding,
    needed: i64,
) -> Result<(), Error> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM signups WHERE race_id=$1 AND role_binding_id=$2 AND status='confirmed'")
        .bind(race).bind(i64::from(binding.id)).fetch_one(&mut **tx).await?;
    if count + needed > i64::from(binding.max_count) {
        return Err(invalid(format!(
            "This signup needs {needed} available slots."
        )));
    }
    Ok(())
}

pub(crate) async fn create(
    tx: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    race: Id<Races>,
    binding_id: Id<RoleBindings>,
    inviter: Id<Users>,
    invitee: Id<Users>,
    note: &str,
) -> Result<(Pair, bool), Error> {
    lock(tx).await?;
    expire(tx).await?;
    check_race(tx, data, race, true).await?;
    let binding = binding(tx, data, binding_id).await?;
    if !binding.allow_paired_signups {
        return Err(invalid(
            "Paired signups are disabled for this role. Please refresh the form.",
        ));
    }
    if inviter == invitee {
        return Err(invalid("Choose another volunteer as your partner."));
    }
    let note = notes(note, binding.allow_signup_notes).map_err(invalid)?;
    check_export(tx, data, &binding).await?;
    eligible(tx, &binding, inviter, false).await?;
    eligible(tx, &binding, invitee, false).await?;
    if let Some(existing) = sqlx::query_as::<_,Pair>("SELECT * FROM volunteer_signup_pairs WHERE race_id=$1 AND role_binding_id=$2 AND state IN ('awaiting_partner','pending','confirmed') AND ((inviter_user_id=$3 AND invitee_user_id=$4) OR (inviter_user_id=$4 AND invitee_user_id=$3))")
        .bind(race).bind(i64::from(binding_id)).bind(inviter).bind(invitee).fetch_optional(&mut **tx).await? {
        return Ok((existing,false));
    }
    if Signup::active_for_user(tx, race, binding_id, inviter).await?
        || outgoing(tx, race, binding_id, inviter).await?.is_some()
    {
        return Err(invalid(
            "You already have a signup or outgoing invitation for this role. Withdraw it before changing to a pair.",
        ));
    }
    let unavailable: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM signups WHERE race_id=$1 AND role_binding_id=$2 AND user_id=$3 AND (status='confirmed' OR (pair_id IS NOT NULL AND status='pending')))")
        .bind(race).bind(i64::from(binding_id)).bind(invitee).fetch_one(&mut **tx).await?;
    if unavailable {
        return Err(invalid(
            "Your partner already has a confirmed or paired signup for this role.",
        ));
    }
    capacity(tx, race, &binding, 2).await?;
    eligible(tx, &binding, inviter, true).await?;
    let pair = sqlx::query_as("INSERT INTO volunteer_signup_pairs(race_id,role_binding_id,inviter_user_id,invitee_user_id,inviter_notes) VALUES($1,$2,$3,$4,$5) RETURNING *")
        .bind(race).bind(i64::from(binding_id)).bind(inviter).bind(invitee).bind(note).fetch_one(&mut **tx).await?;
    Ok((pair, true))
}

async fn set_state(
    tx: &mut Transaction<'_, Postgres>,
    pair: &Pair,
    state: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE volunteer_signup_pairs SET state=$2,updated_at=NOW() WHERE id=$1")
        .bind(pair.id)
        .bind(state)
        .execute(&mut **tx)
        .await?;
    if pair.accepted_at.is_some() {
        let status = match state {
            "pending" => "pending",
            "confirmed" => "confirmed",
            "declined" => "declined",
            _ => "aborted",
        };
        sqlx::query("UPDATE signups SET status=$2::volunteer_signup_status,updated_at=NOW() WHERE pair_id=$1")
            .bind(pair.id).bind(status).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn accept(
    tx: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    pair: &Pair,
    note: &str,
    replace_solo: bool,
) -> Result<Vec<i64>, Error> {
    check_race(tx, data, pair.race_id, true).await?;
    let binding = binding(tx, data, pair.binding_id()).await?;
    // Turning the setting off prevents new invitations, not existing consent.
    check_export(tx, data, &binding).await?;
    let note = notes(note, binding.allow_signup_notes).map_err(invalid)?;
    eligible(tx, &binding, pair.inviter_user_id, false).await?;
    eligible(tx, &binding, pair.invitee_user_id, false).await?;
    capacity(tx, pair.race_id, &binding, 2).await?;
    if Signup::active_for_user(tx, pair.race_id, binding.id, pair.inviter_user_id).await? {
        return Err(invalid(
            "The inviter already has an active signup. Ask them to resolve it first.",
        ));
    }
    let existing: Vec<(i32,String,Option<i64>)> = sqlx::query_as("SELECT id,status::text,pair_id FROM signups WHERE race_id=$1 AND role_binding_id=$2 AND user_id=$3 AND status IN ('pending','confirmed')")
        .bind(pair.race_id).bind(i64::from(binding.id)).bind(pair.invitee_user_id).fetch_all(&mut **tx).await?;
    if existing.len() > 1
        || existing
            .iter()
            .any(|(_, state, group)| state != "pending" || group.is_some())
    {
        return Err(invalid(
            "Your existing booking must be resolved before joining this pair.",
        ));
    }
    if !existing.is_empty() && !replace_solo {
        return Err(invalid(
            "Confirm that your pending individual signup should be replaced by this pair.",
        ));
    }
    // Avoid replacing an application already forwarded to an external service.
    if let Some((id, _, _)) = existing.first() {
        let exported: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM speedgaming_volunteer_exports WHERE signup_id=$1 AND state NOT IN ('pending','ignored'))")
            .bind(id).fetch_one(&mut **tx).await?;
        if exported {
            return Err(invalid(
                "Your individual signup was already forwarded to SpeedGaming. Contact the organizers before changing it.",
            ));
        }
    }
    eligible(tx, &binding, pair.invitee_user_id, true).await?;
    sqlx::query(
        "INSERT INTO signups(race_id,role_binding_id,user_id,notes,pair_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(pair.race_id)
    .bind(i64::from(binding.id))
    .bind(pair.inviter_user_id)
    .bind(&pair.inviter_notes)
    .bind(pair.id)
    .execute(&mut **tx)
    .await?;
    if let Some((id, _, _)) = existing.first() {
        sqlx::query(
            "UPDATE signups SET pair_id=$2,notes=COALESCE($3,notes),updated_at=NOW() WHERE id=$1",
        )
        .bind(id)
        .bind(pair.id)
        .bind(note)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query("INSERT INTO signups(race_id,role_binding_id,user_id,notes,pair_id) VALUES($1,$2,$3,$4,$5)")
            .bind(pair.race_id).bind(i64::from(binding.id)).bind(pair.invitee_user_id).bind(note).bind(pair.id).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE volunteer_signup_pairs SET accepted_at=NOW(),state='pending',updated_at=NOW() WHERE id=$1")
        .bind(pair.id).execute(&mut **tx).await?;
    Ok(sqlx::query_scalar("UPDATE volunteer_signup_pairs SET state='cancelled',updated_at=NOW() WHERE id<>$1 AND race_id=$2 AND role_binding_id=$3 AND state='awaiting_partner' AND (inviter_user_id IN ($4,$5) OR invitee_user_id IN ($4,$5)) RETURNING id")
        .bind(pair.id).bind(pair.race_id).bind(i64::from(binding.id)).bind(pair.inviter_user_id).bind(pair.invitee_user_id).fetch_all(&mut **tx).await?)
}

pub(crate) async fn confirmed_conflict(
    tx: &mut Transaction<'_, Postgres>,
    race: Id<Races>,
    user: Id<Users>,
) -> sqlx::Result<bool> {
    let current: (Series, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT series,start FROM races WHERE id=$1")
            .bind(race)
            .fetch_one(&mut **tx)
            .await?;
    let Some(start) = current.1 else {
        return Ok(false);
    };
    let end = start + current.0.default_race_duration();
    let others: Vec<(Series,DateTime<Utc>)> = sqlx::query_as("SELECT r.series,r.start FROM signups s JOIN races r ON r.id=s.race_id WHERE s.user_id=$1 AND s.status='confirmed' AND r.id<>$2 AND r.start IS NOT NULL")
        .bind(user).bind(race).fetch_all(&mut **tx).await?;
    Ok(others
        .into_iter()
        .any(|(series, other)| start < other + series.default_race_duration() && other < end))
}

/// Called by the existing overlap workflow; an accepted pair is declined whole.
pub(crate) async fn decline_overlap(
    tx: &mut Transaction<'_, Postgres>,
    signup: Id<Signups>,
) -> sqlx::Result<Option<i64>> {
    if let Some(id) = signup_pair(tx, signup).await? {
        if let Some(pair) = load(tx, id).await? {
            if pair.state == "pending" {
                set_state(tx, &pair, "declined").await?;
                return Ok(Some(id));
            }
        }
    } else {
        sqlx::query("UPDATE signups SET status='declined',updated_at=NOW() WHERE id=$1 AND status='pending'").bind(i64::from(signup)).execute(&mut **tx).await?;
    }
    Ok(None)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ActionForm {
    #[field(default = String::new())]
    csrf: String,
    action: String,
    #[field(default = String::new())]
    notes: String,
    replace_solo: bool,
}

async fn act(
    tx: &mut Transaction<'_, Postgres>,
    data: &Data<'_>,
    pair: &Pair,
    me: &User,
    form: &ActionForm,
) -> Result<Vec<i64>, Error> {
    let permissions = Permissions::load(tx, data, me).await?;
    let mut affected = vec![pair.id];
    match form.action.as_str() {
        "accept" if pair.invitee_user_id == me.id && pair.state == "awaiting_partner" => {
            affected.extend(accept(tx, data, pair, &form.notes, form.replace_solo).await?)
        }
        "decline" if pair.invitee_user_id == me.id && pair.state == "awaiting_partner" => {
            set_state(tx, pair, "declined").await?
        }
        "cancel" if pair.inviter_user_id == me.id && pair.state == "awaiting_partner" => {
            set_state(tx, pair, "cancelled").await?
        }
        "withdraw"
            if pair.contains(me.id) && matches!(pair.state.as_str(), "pending" | "confirmed") =>
        {
            if pair.state == "confirmed" {
                let before_start: bool =
                    sqlx::query_scalar("SELECT COALESCE(start>NOW(),false) FROM races WHERE id=$1")
                        .bind(pair.race_id)
                        .fetch_one(&mut **tx)
                        .await?;
                if !before_start {
                    return Err(invalid(
                        "You cannot withdraw a confirmed pair after the race has started. Contact the organizers.",
                    ));
                }
            }
            set_state(tx, pair, "cancelled").await?;
        }
        "confirm" if permissions.decide_signups && pair.state == "pending" => {
            check_race(tx, data, pair.race_id, true).await?;
            let binding = binding(tx, data, pair.binding_id()).await?;
            check_export(tx, data, &binding).await?;
            capacity(tx, pair.race_id, &binding, 2).await?;
            for user in [pair.inviter_user_id, pair.invitee_user_id] {
                eligible(tx, &binding, user, false).await?;
                if confirmed_conflict(tx, pair.race_id, user).await? {
                    return Err(invalid(
                        "A member is already confirmed for an overlapping race.",
                    ));
                }
            }
            set_state(tx, pair, "confirmed").await?;
            let members: Vec<(i32, Id<Users>)> =
                sqlx::query_as("SELECT id,user_id FROM signups WHERE pair_id=$1")
                    .bind(pair.id)
                    .fetch_all(&mut **tx)
                    .await?;
            for (signup, user) in members {
                affected.extend(
                    Signup::auto_reject_overlapping_signups(tx, Id::from(i64::from(signup)), user)
                        .await?,
                );
            }
        }
        "reject"
            if permissions.decide_signups
                && !data.is_ended()
                && matches!(pair.state.as_str(), "pending" | "awaiting_partner") =>
        {
            set_state(tx, pair, "declined").await?;
        }
        "revert"
            if permissions.revert_signups
                && pair.accepted_at.is_some()
                && matches!(pair.state.as_str(), "confirmed" | "declined") =>
        {
            check_race(tx, data, pair.race_id, true).await?;
            let binding = binding(tx, data, pair.binding_id()).await?;
            for user in [pair.inviter_user_id, pair.invitee_user_id] {
                eligible(tx, &binding, user, false).await?;
                let duplicate: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM signups WHERE race_id=$1 AND role_binding_id=$2 AND user_id=$3 AND status IN ('pending','confirmed') AND pair_id IS DISTINCT FROM $4)")
                    .bind(pair.race_id).bind(i64::from(binding.id)).bind(user).bind(pair.id).fetch_one(&mut **tx).await?;
                if duplicate {
                    return Err(invalid(
                        "A member has another active signup; resolve it before reverting this pair.",
                    ));
                }
            }
            set_state(tx, pair, "pending").await?;
        }
        _ => {
            return Err(invalid(
                "This action is unavailable for your account or the pair's current state.",
            ));
        }
    }
    Ok(affected)
}

pub(crate) async fn notify(pool: &PgPool, ctx: &DiscordCtx, id: i64) -> bool {
    async fn send(pool: &PgPool, ctx: &DiscordCtx, id: i64) -> Result<bool, Error> {
        let mut tx = pool.begin().await?;
        let Some(pair) = load(&mut tx, id).await? else {
            return Ok(false);
        };
        let (series, event, start): (Series, String, Option<DateTime<Utc>>) =
            sqlx::query_as("SELECT series,event,start FROM races WHERE id=$1")
                .bind(pair.race_id)
                .fetch_one(&mut *tx)
                .await?;
        let inviter = User::from_id(&mut *tx, pair.inviter_user_id).await?;
        let invitee = User::from_id(&mut *tx, pair.invitee_user_id).await?;
        let data = Data::new(&mut tx, series, &event)
            .await?
            .ok_or_else(|| invalid("Unknown event"))?;
        let (role_name, role_language): (String, Language) = sqlx::query_as("SELECT rt.name,rb.language FROM role_bindings rb JOIN role_types rt ON rt.id=rb.role_type_id WHERE rb.id=$1")
            .bind(i64::from(pair.binding_id())).fetch_one(&mut *tx).await?;
        let mut recipients = vec![];
        if let Some(user) = invitee.as_ref().and_then(|u| u.discord.as_ref()) {
            recipients.push(user.id);
        }
        if pair.state != "awaiting_partner" {
            if let Some(user) = inviter.as_ref().and_then(|u| u.discord.as_ref()) {
                recipients.push(user.id);
            }
        }
        if pair.accepted_at.is_some() && pair.state == "cancelled" {
            for organizer in data.organizers(&mut tx).await? {
                if let Some(discord) = organizer.discord {
                    recipients.push(discord.id);
                }
            }
        }
        recipients.sort();
        recipients.dedup();
        let url = format!("{}{}", base_uri(), pair.url(series, &event));
        let text = format!(
            "{} + {} · {} ({})\nEvent: {}\nRace: {}\nWhen: {}\n{}\n{}\nReview on Hyrule Town Hall: {}",
            inviter.as_ref().map_or("Volunteer", User::display_name),
            invitee.as_ref().map_or("Volunteer", User::display_name),
            role_name,
            role_language,
            data.display_name,
            pair.race_id,
            start.map_or_else(
                || "Unscheduled".into(),
                |t| format!("<t:{}:F>", t.timestamp())
            ),
            pair.label(),
            if pair.state == "awaiting_partner" {
                "You will only be selected together. Accepting submits the pair for organizer review. If either member withdraws, both signups are cancelled."
            } else {
                "You will only be selected together."
            },
            url
        );
        tx.commit().await?;
        let mut delivered = !recipients.is_empty();
        for recipient in recipients {
            match recipient.create_dm_channel(ctx).await {
                Ok(channel) => {
                    if let Err(error) = channel
                        .send_message(
                            ctx,
                            serenity::all::CreateMessage::new()
                                .content(&text)
                                .components(vec![serenity::all::CreateActionRow::Buttons(vec![
                                    serenity::all::CreateButton::new_link(&url)
                                        .label("Review on Hyrule Town Hall"),
                                ])])
                                .allowed_mentions(serenity::all::CreateAllowedMentions::new()),
                        )
                        .await
                    {
                        eprintln!("Pair {id} DM failed: {error}");
                        delivered = false;
                    }
                }
                Err(error) => {
                    eprintln!("Pair {id} DM channel failed: {error}");
                    delivered = false;
                }
            }
        }
        Ok(delivered)
    }
    match send(pool, ctx, id).await {
        Ok(delivered) => delivered,
        Err(error) => {
            eprintln!("Pair {id} notification failed: {error}");
            false
        }
    }
}

pub(crate) async fn refresh(pool: &PgPool, http: &reqwest::Client, ctx: &DiscordCtx, ids: &[i64]) {
    let mut seen = HashSet::new();
    for &id in ids {
        if seen.insert(id) {
            notify(pool, ctx, id).await;
            if let Ok(Some(race)) = sqlx::query_scalar::<_, Id<Races>>(
                "SELECT race_id FROM volunteer_signup_pairs WHERE id=$1",
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            {
                let _ = volunteer_requests::update_volunteer_post_for_race(pool, ctx, race).await;
                crate::zsr_export::schedule_volunteer_api_call(pool.clone(), http.clone(), race);
            }
        }
    }
    if !ids.is_empty() {
        speedgaming_export::schedule_sync(pool.clone(), http.clone());
    }
}

#[rocket::get("/event/<series>/<event>/races/<race_id>/volunteer-pairs/<id>?<dm_failed>")]
pub(crate) async fn review(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    id: i64,
    csrf: Option<CsrfToken>,
    dm_failed: Option<bool>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    render(pool, me, &uri, series, event, race_id, id, csrf,
        dm_failed.filter(|failed| *failed).map(|_| "The invitation was created, but the DM could not be delivered. Share the invitation link with your partner.".into())).await
}

async fn render(
    pool: &PgPool,
    me: Option<User>,
    uri: &Origin<'_>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    id: i64,
    csrf: Option<CsrfToken>,
    error: Option<String>,
) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut tx = pool.begin().await?;
    let data = Data::new(&mut tx, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    ensure_race_event(&mut tx, &data, race_id).await?;
    let pair = load(&mut tx, id)
        .await?
        .filter(|p| p.race_id == race_id)
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let header = data.header(&mut tx, me.as_ref(), Tab::Races, true).await?;
    let content = if let Some(ref me) = me {
        let permissions = Permissions::load(&mut tx, &data, me).await?;
        if !pair.contains(me.id) && !permissions.view_signups {
            html! { p : "This invitation is for a different account. Sign in as the invited partner to review it."; }
        } else {
            let inviter = User::from_id(&mut *tx, pair.inviter_user_id).await?;
            let invitee = User::from_id(&mut *tx, pair.invitee_user_id).await?;
            let role = binding(&mut tx, &data, pair.binding_id()).await.ok();
            let start: Option<DateTime<Utc>> =
                sqlx::query_scalar("SELECT start FROM races WHERE id=$1")
                    .bind(race_id)
                    .fetch_one(&mut *tx)
                    .await?;
            let available =
                start.is_some_and(|s| s > Utc::now()) && !data.is_ended() && role.is_some();
            let existing =
                Signup::active_for_user(&mut tx, race_id, pair.binding_id(), me.id).await?;
            let member_notes: Vec<(Id<Users>, Option<String>)> =
                sqlx::query_as("SELECT user_id,notes FROM signups WHERE pair_id=$1")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            let button = |action: &str, label: &str| {
                html! {
                    form(method="post",action=pair.url(series,event)) {
                        @if let Some(ref csrf)=csrf { input(type="hidden",name="csrf",value=csrf.authenticity_token()); }
                        input(type="hidden",name="action",value=action);
                        button(type="submit") : label;
                    }
                }
            };
            html! {
                h2 : "Together-only volunteer signup";
                @if let Some(error)=error { p(class="error") : error; }
                h3 : format!("{} + {}",inviter.as_ref().map_or("Volunteer",User::display_name),invitee.as_ref().map_or("Volunteer",User::display_name));
                p : format!("{} · Race {}",data.display_name,race_id);
                @if let Some(ref role)=role { p : format!("{} · {}",role.role_type_name,role.language); }
                @if let Some(start)=start { p : format_datetime(start,DateTimeFormat {long:true,running_text:false}); }
                p { strong : if !available && pair.state=="awaiting_partner" { "Invitation no longer available" } else { pair.label() }; }
                p : "You will only be selected together. Accepting submits the pair for organizer review. If either member withdraws, both signups are cancelled.";
                @if permissions.view_signups {
                    @for (user,notes) in member_notes {
                        @if let Some(notes)=notes {
                            p : format!("{}: {}",if user==pair.inviter_user_id { inviter.as_ref().map_or("Volunteer",User::display_name) } else { invitee.as_ref().map_or("Volunteer",User::display_name) },notes);
                        }
                    }
                }
                @if pair.state=="awaiting_partner" {
                    @if me.id==pair.invitee_user_id && available {
                        form(method="post",action=pair.url(series,event)) {
                            @if let Some(ref csrf)=csrf { input(type="hidden",name="csrf",value=csrf.authenticity_token()); }
                            input(type="hidden",name="action",value="accept");
                            @if role.as_ref().is_some_and(|r|r.allow_signup_notes) {
                                label(for="pair-notes") : "Your notes for organizers (optional)";
                                input(id="pair-notes",name="notes",maxlength="60");
                            }
                            @if existing { label { input(type="checkbox",name="replace_solo",required); : " Replace my pending individual signup with this pair signup."; } }
                            button(type="submit") : "Accept invitation";
                        }
                    }
                    @if me.id==pair.invitee_user_id { : button("decline","Decline invitation"); }
                    @if me.id==pair.inviter_user_id {
                        p : "Share this link with your partner if they did not receive the DM:";
                        input(type="text",readonly,value=format!("{}{}",base_uri(),pair.url(series,event)),aria_label="Invitation link");
                        : button("cancel","Cancel invitation");
                    }
                }
                @if pair.state=="pending" && permissions.decide_signups && available {
                    @let free=capacity(&mut tx,race_id,role.as_ref().expect("available"),2).await;
                    @if free.is_ok() { : button("confirm","Confirm both"); } else { p : "This pair needs 2 available slots."; }
                }
                @if matches!(pair.state.as_str(),"pending"|"awaiting_partner") && permissions.decide_signups && !data.is_ended() { : button("reject","Decline pair"); }
                @if pair.accepted_at.is_some() && matches!(pair.state.as_str(),"confirmed"|"declined") && permissions.revert_signups && available { : button("revert","Revert both to pending"); }
                @if pair.contains(me.id) && (pair.state=="pending" || (pair.state=="confirmed" && start.is_some_and(|s|s>Utc::now()))) {
                    p : "Withdrawing cancels both signups. Your partner and the organizers will be notified.";
                    : button("withdraw","Withdraw and cancel pair");
                }
            }
        }
    } else {
        html! { p : "Sign in as the invited partner to review this invitation."; a(class="button",href=uri!(crate::auth::login(Some(uri!(review(series,event,race_id,id,_)))))) : "Sign in"; }
    };
    Ok(page(tx,&me,uri,PageStyle::default(),"Volunteer pair",html! { : header; article { : content; p { a(href=uri!(super::match_signup_page_get(series,event,race_id,_))) : "Back to race signups"; } } }).await?)
}

#[rocket::post(
    "/event/<series>/<event>/races/<race_id>/volunteer-pairs/<id>",
    data = "<form>"
)]
pub(crate) async fn action(
    pool: &State<PgPool>,
    http: &State<reqwest::Client>,
    discord: &State<RwFuture<DiscordCtx>>,
    me: User,
    uri: Origin<'_>,
    series: Series,
    event: &str,
    race_id: Id<Races>,
    id: i64,
    csrf: Option<CsrfToken>,
    form: Form<Contextual<'_, ActionForm>>,
) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut form = form.into_inner();
    form.verify(&csrf);
    let mut tx = pool.begin().await?;
    lock(&mut tx).await?;
    let data = Data::new(&mut tx, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    ensure_race_event(&mut tx, &data, race_id).await?;
    let pair = load(&mut tx, id)
        .await?
        .filter(|p| p.race_id == race_id)
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    let result = if form.context.errors().next().is_some() {
        Err(invalid(
            "Please refresh the page and submit the form again.",
        ))
    } else if let Some(ref value) = form.value {
        act(&mut tx, &data, &pair, &me, value).await
    } else {
        Err(invalid("Invalid form submission."))
    };
    match result {
        Ok(ids) => {
            tx.commit().await?;
            if form
                .value
                .as_ref()
                .is_some_and(|value| value.action == "accept")
            {
                sync_signup_discord_roles(
                    pool,
                    &*discord.read().await,
                    &data,
                    pair.binding_id(),
                    me.id,
                )
                .await;
            }
            refresh(pool, http, &*discord.read().await, &ids).await;
            Ok(RedirectOrContent::Redirect(Redirect::to(
                pair.url(series, event),
            )))
        }
        Err(Error::Signup(message)) => {
            tx.rollback().await?;
            Ok(RedirectOrContent::Content(
                render(
                    pool,
                    Some(me),
                    &uri,
                    series,
                    event,
                    race_id,
                    id,
                    csrf,
                    Some(message),
                )
                .await?,
            ))
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod notes_tests {
    use super::*;

    #[test]
    fn notes_enforce_settings_and_unicode_length() {
        assert_eq!(notes(" \n ", false).unwrap(), None);
        assert_eq!(notes(" hello ", true).unwrap(), Some("hello".into()));
        assert!(notes("hello", false).is_err());
        assert!(notes(&"é".repeat(60), true).is_ok());
        assert!(notes(&"é".repeat(61), true).is_err());
    }
}
