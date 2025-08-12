use crate::{
    event::Role,
    prelude::*,
};

#[derive(Debug, Clone)]
pub(crate) struct Team {
    pub(crate) id: Id<Teams>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) name: Option<String>,
    pub(crate) racetime_slug: Option<String>,
    /// The event-specific entrant ID, not to be confused with the tournament-specific participant ID.
    pub(crate) startgg_id: Option<startgg::ID>,
    pub(crate) challonge_id: Option<String>,
    pub(crate) plural_name: Option<bool>,
    pub(crate) restream_consent: bool,
    pub(crate) mw_impl: Option<mw::Impl>,
    pub(crate) qualifier_rank: Option<i16>,
}

impl Team {
    pub(crate) async fn from_id(transaction: &mut Transaction<'_, Postgres>, id: Id<Teams>) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams WHERE id = $1"#, id as _).fetch_optional(&mut **transaction).await
    }

    pub(crate) async fn from_racetime(transaction: &mut Transaction<'_, Postgres>, series: Series, event: &str, racetime_slug: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams WHERE series = $1 AND event = $2 AND racetime_slug = $3"#, series as _, event, racetime_slug).fetch_optional(&mut **transaction).await
    }

    pub(crate) async fn from_challonge(transaction: &mut Transaction<'_, Postgres>, challonge_id: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams WHERE challonge_id = $1"#, challonge_id).fetch_optional(&mut **transaction).await
    }

    pub(crate) async fn from_startgg(transaction: &mut Transaction<'_, Postgres>, startgg_id: &startgg::ID) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams WHERE startgg_id = $1"#, startgg_id as _).fetch_optional(&mut **transaction).await
    }

    pub(crate) async fn for_event(transaction: &mut Transaction<'_, Postgres>, series: Series, event: &str) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams WHERE series = $1 AND event = $2 AND NOT resigned"#, series as _, event).fetch_all(&mut **transaction).await
    }

    pub(crate) async fn for_member(transaction: &mut Transaction<'_, Postgres>, member_id: Id<Users>) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, teams.startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams, team_members WHERE NOT resigned AND id = team AND member = $1"#, member_id as _).fetch_all(&mut **transaction).await
    }

    pub(crate) async fn from_event_and_member(transaction: &mut Transaction<'_, Postgres>, series: Series, event: &str, member_id: Id<Users>) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(Self, r#"SELECT id AS "id: Id<Teams>", series AS "series: Series", event, name, racetime_slug, teams.startgg_id AS "startgg_id: startgg::ID", challonge_id, plural_name, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams, team_members WHERE series = $1 AND event = $2 AND NOT resigned AND id = team AND member = $3"#, series as _, event, member_id as _).fetch_optional(&mut **transaction).await
    }

    pub(crate) fn dummy() -> Self {
        Self {
            id: Id::dummy(),
            series: Series::Standard,
            event: format!("1"),
            name: None,
            racetime_slug: None,
            startgg_id: None,
            challonge_id: None,
            plural_name: None,
            restream_consent: false,
            mw_impl: None,
            qualifier_rank: None,
        }
    }

    pub(crate) async fn name(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Option<Cow<'_, str>>> {
        Ok(if let Some(ref name) = self.name {
            Some(Cow::Borrowed(name))
        } else if let Ok(member) = self.members(transaction).await?.into_iter().exactly_one() {
            Some(Cow::Owned(member.display_name().to_owned()))
        } else {
            None
        })
    }

    pub(crate) fn name_is_plural(&self) -> bool {
        self.plural_name.unwrap_or(false)
    }

    pub(crate) async fn possessive_determiner(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<&'static str> {
        Ok(if let Ok(member) = self.members(transaction).await?.into_iter().exactly_one() {
            member.possessive_determiner()
        } else {
            "their"
        })
    }

    pub(crate) async fn to_html(&self, transaction: &mut Transaction<'_, Postgres>, running_text: bool) -> sqlx::Result<RawHtml<String>> {
        Ok(if let Ok(member) = self.members(transaction).await?.into_iter().exactly_one() {
            member.to_html()
        } else {
            let inner = html! {
                @if let Some(ref name) = self.name {
                    @if running_text {
                        i {
                            bdi : name;
                        }
                    } else {
                        bdi : name;
                    }
                } else {
                    @if running_text {
                        : "an unnamed team";
                    } else {
                        i : "(unnamed)";
                    }
                }
            };
            html! {
                @if let Some(ref racetime_slug) = self.racetime_slug {
                    a(href = format!("https://{}/team/{racetime_slug}", racetime_host())) : inner;
                } else {
                    : inner;
                }
            }
        })
    }

    pub(crate) async fn into_event(self, transaction: &mut Transaction<'_, Postgres>) -> Result<event::Data<'static>, event::DataError> {
        Ok(event::Data::new(transaction, self.series, self.event).await?.expect("team for nonexistent event"))
    }

    pub(crate) async fn member_ids(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Vec<Id<Users>>> {
        sqlx::query_scalar!(r#"SELECT member AS "member: Id<Users>" FROM team_members WHERE team = $1 ORDER BY role ASC"#, self.id as _).fetch_all(&mut **transaction).await
    }

    async fn member_ids_roles(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Vec<(Id<Users>, Role)>> {
        Ok(
            sqlx::query!(r#"SELECT member AS "member: Id<Users>", role AS "role: Role" FROM team_members WHERE team = $1 ORDER BY role ASC"#, self.id as _).fetch_all(&mut **transaction).await?
                .into_iter()
                .map(|row| (row.member, row.role))
                .collect()
        )
    }

    pub(crate) async fn members(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Vec<User>> {
        let user_ids = self.member_ids(&mut *transaction).await?;
        let mut members = Vec::with_capacity(user_ids.len());
        for user_id in user_ids {
            members.push(User::from_id(&mut **transaction, user_id).await?.expect("database constraint violated: nonexistent team member"));
        }
        Ok(members)
    }

    pub(crate) async fn members_roles(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Vec<(User, Role)>> {
        let rows = self.member_ids_roles(&mut *transaction).await?;
        let mut members = Vec::with_capacity(rows.len());
        for (user_id, role) in rows {
            members.push((User::from_id(&mut **transaction, user_id).await?.expect("database constraint violated: nonexistent team member"), role));
        }
        Ok(members)
    }
}

impl PartialEq for Team {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Team {}

impl PartialOrd for Team {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Team {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}
