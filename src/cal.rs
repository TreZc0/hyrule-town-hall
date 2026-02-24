use {
    ics::{
        ICalendar,
        parameters::TzIDParam,
        properties::{
            Description,
            DtEnd,
            DtStart,
            RRule,
            Summary,
            URL,
        },
    },
    reqwest::StatusCode,
    rocket_util::Response,
    serenity::all::{
        CreateMessage,
        CreateSelectMenu,
        CreateSelectMenuKind,
        CreateSelectMenuOption,
        ScheduledEventId,
    },
    sqlx::types::Json,
    chrono::LocalResult,
    crate::{
        discord_bot,
        event::Tab,
        event::roles::{
            Signup,
            VolunteerSignupStatus
        },
        hash_icon::SpoilerLog,
        hash_icon_db::HashIconData,
        prelude::*,
        racetime_bot,
        sheets,
        weekly::{WeeklySchedule, WeeklySchedules},
    },
    crate::id::RoleBindings,
};
pub(crate) use mhstatus::EventKind;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Source {
    Manual,
    Challonge {
        id: String,
    },
    League {
        id: i32,
    },
    Sheet {
        timestamp: NaiveDateTime,
    },
    StartGG {
        event: String,
        set: startgg::ID,
    },
    SpeedGaming {
        id: i64,
    },
}

#[derive(Clone)]
pub(crate) enum Entrant {
    MidosHouseTeam(Team),
    Discord {
        id: UserId,
        racetime_id: Option<String>,
        twitch_username: Option<String>,
    },
    Named {
        name: String,
        racetime_id: Option<String>,
        twitch_username: Option<String>,
    },
}

impl Entrant {
    pub(crate) async fn name(&self, transaction: &mut Transaction<'_, Postgres>, discord_ctx: &DiscordCtx) -> Result<Option<Cow<'_, str>>, discord_bot::Error> {
        Ok(match self {
            Self::MidosHouseTeam(team) => team.name(transaction).await?,
            Self::Discord { id, .. } => if let Some(user) = User::from_discord(&mut **transaction, *id).await? {
                Some(Cow::Owned(user.discord.unwrap().display_name))
            } else {
                let user = id.to_user(discord_ctx).await?;
                Some(Cow::Owned(user.global_name.unwrap_or(user.name)))
            },
            Self::Named { name, .. } => Some(Cow::Borrowed(name)),
        })
    }

    pub(crate) fn name_is_plural(&self) -> bool {
        match self {
            Self::MidosHouseTeam(team) => team.name_is_plural(),
            Self::Discord { .. } => false,
            Self::Named { .. } => false, // assume solo (e.g. League)
        }
    }

    pub(crate) async fn to_html(&self, transaction: &mut Transaction<'_, Postgres>, discord_ctx: &DiscordCtx, running_text: bool) -> Result<RawHtml<String>, discord_bot::Error> {
        Ok(match self {
            Self::MidosHouseTeam(team) => team.to_html(transaction, running_text).await?,
            Self::Discord { id, racetime_id, .. } => {
                let url = if let Some(racetime_id) = racetime_id {
                    format!("https://{}/user/{racetime_id}", racetime_host())
                } else {
                    format!("https://discord.com/users/{id}")
                };
                if let Some(user) = User::from_discord(&mut **transaction, *id).await? {
                    html! {
                        a(href = url) {
                            bdi : user.discord.unwrap().display_name;
                        }
                    }
                } else {
                    let user = id.to_user(discord_ctx).await?;
                    html! {
                        a(href = url) {
                            bdi : user.global_name.unwrap_or(user.name);
                        }
                    }
                }
            }
            Self::Named { name, racetime_id: Some(racetime_id), .. } => html! {
                a(href = format!("https://{}/user/{racetime_id}", racetime_host())) {
                    bdi : name;
                }
            },
            Self::Named { name, racetime_id: None, .. } => html! {
                bdi : name;
            },
        })
    }
}

impl PartialEq for Entrant {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MidosHouseTeam(lhs), Self::MidosHouseTeam(rhs)) => lhs == rhs,
            (Self::Discord { id: lhs, .. }, Self::Discord { id: rhs, .. }) => lhs == rhs,
            (Self::Named { name: lhs, .. }, Self::Named { name: rhs, .. }) => lhs == rhs,
            (Self::MidosHouseTeam(_), Self::Discord { .. } | Self::Named { .. }) |
            (Self::Discord { .. }, Self::MidosHouseTeam(_) | Self::Named { .. }) |
            (Self::Named { .. }, Self::MidosHouseTeam(_) | Self::Discord { .. }) => false,
        }
    }
}

impl Eq for Entrant {}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Entrants {
    Open,
    Count {
        total: u32,
        finished: u32,
    },
    Named(String),
    Two([Entrant; 2]),
    Three([Entrant; 3]),
}

impl Entrants {
    fn to_db(&self) -> ([Option<Id<Teams>>; 3], [Option<&String>; 3], [Option<UserId>; 2], [Option<&String>; 2], [Option<&String>; 2], [Option<u32>; 2]) {
        match *self {
            Entrants::Open => ([None; 3], [None; 3], [None; 2], [None; 2], [None; 2], [None; 2]),
            Entrants::Count { total, finished } => ([None; 3], [None; 3], [None; 2], [None; 2], [None; 2], [Some(total), Some(finished)]),
            Entrants::Named(ref entrants) => ([None; 3], [Some(entrants), None, None], [None; 2], [None; 2], [None; 2], [None; 2]),
            Entrants::Two([ref p1, ref p2]) => {
                let (team1, p1, p1_discord, p1_racetime, p1_twitch) = match p1 {
                    Entrant::MidosHouseTeam(team) => (Some(team.id), None, None, None, None),
                    Entrant::Discord { id, racetime_id, twitch_username } => (None, None, Some(*id), racetime_id.as_ref(), twitch_username.as_ref()),
                    Entrant::Named { name, racetime_id, twitch_username } => (None, Some(name), None, racetime_id.as_ref(), twitch_username.as_ref()),
                };
                let (team2, p2, p2_discord, p2_racetime, p2_twitch) = match p2 {
                    Entrant::MidosHouseTeam(team) => (Some(team.id), None, None, None, None),
                    Entrant::Discord { id, racetime_id, twitch_username } => (None, None, Some(*id), racetime_id.as_ref(), twitch_username.as_ref()),
                    Entrant::Named { name, racetime_id, twitch_username } => (None, Some(name), None, racetime_id.as_ref(), twitch_username.as_ref()),
                };
                ([team1, team2, None], [p1, p2, None], [p1_discord, p2_discord], [p1_racetime, p2_racetime], [p1_twitch, p2_twitch], [None; 2])
            }
            Entrants::Three([ref p1, ref p2, ref p3]) => {
                let (team1, p1) = match p1 {
                    Entrant::MidosHouseTeam(team) => (Some(team.id), None),
                    Entrant::Named { name, racetime_id: None, twitch_username: None } => (None, Some(name)),
                    _ => unimplemented!(), //TODO
                };
                let (team2, p2) = match p2 {
                    Entrant::MidosHouseTeam(team) => (Some(team.id), None),
                    Entrant::Named { name, racetime_id: None, twitch_username: None } => (None, Some(name)),
                    _ => unimplemented!(), //TODO
                };
                let (team3, p3) = match p3 {
                    Entrant::MidosHouseTeam(team) => (Some(team.id), None),
                    Entrant::Named { name, racetime_id: None, twitch_username: None } => (None, Some(name)),
                    _ => unimplemented!(), //TODO
                };
                ([team1, team2, team3], [p1, p2, p3], [None; 2], [None; 2], [None; 2], [None; 2])
            }
        }
    }
}

#[derive(Default, Clone)]
pub(crate) enum RaceSchedule {
    #[default]
    Unscheduled,
    Live {
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        room: Option<Url>,
    },
    Async {
        start1: Option<DateTime<Utc>>,
        start2: Option<DateTime<Utc>>,
        start3: Option<DateTime<Utc>>,
        end1: Option<DateTime<Utc>>,
        end2: Option<DateTime<Utc>>,
        end3: Option<DateTime<Utc>>,
        room1: Option<Url>,
        room2: Option<Url>,
        room3: Option<Url>,
    },
}

impl RaceSchedule {
    fn new(
        live_start: Option<DateTime<Utc>>, async_start1: Option<DateTime<Utc>>, async_start2: Option<DateTime<Utc>>, async_start3: Option<DateTime<Utc>>,
        live_end: Option<DateTime<Utc>>, async_end1: Option<DateTime<Utc>>, async_end2: Option<DateTime<Utc>>, async_end3: Option<DateTime<Utc>>,
        live_room: Option<Url>, async_room1: Option<Url>, async_room2: Option<Url>, async_room3: Option<Url>,
    ) -> Self {
        match (live_start, async_start1, async_start2, async_start3) {
            (None, None, None, None) => Self::Unscheduled,
            (Some(start), None, None, None) => Self::Live {
                end: live_end,
                room: live_room,
                start,
            },
            (None, start1, start2, start3) => Self::Async {
                end1: async_end1,
                end2: async_end2,
                end3: async_end3,
                room1: async_room1,
                room2: async_room2,
                room3: async_room3,
                start1, start2, start3,
            },
            (Some(_), _, _, _) => unreachable!("both live and async starts included, should be prevented by SQL constraint"),
        }
    }

    fn end_time(&self, entrants: &Entrants) -> Option<DateTime<Utc>> {
        match *self {
            Self::Unscheduled => None,
            Self::Live { end, .. } => end,
            Self::Async { end1, end2, end3, .. } => Some(if let Entrants::Three(_) = entrants {
                end1?.max(end2?).max(end3?)
            } else {
                end1?.max(end2?)
            }),
        }
    }

    #[allow(dead_code)]
    fn start_matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unscheduled, Self::Unscheduled) => true,
            (Self::Live { start: start_a, .. }, Self::Live { start: start_b, .. }) => start_a == start_b,
            (Self::Async { start1: start_a1, start2: start_a2, start3: start_a3, .. }, Self::Async { start1: start_b1, start2: start_b2, start3: start_b3, .. }) => start_a1 == start_b1 && start_a2 == start_b2 && start_a3 == start_b3,
            (Self::Unscheduled, _) | (Self::Live { .. }, _) | (Self::Async { .. }, _) => false, // ensure compile error on missing variants by listing each left-hand side individually
        }
    }

    fn cmp(&self, entrants_a: &Entrants, other: &Self, entrants_b: &Entrants) -> Ordering {
        let (mut starts_a, end_a) = match *self {
            Self::Unscheduled => ([None; 3], None),
            Self::Live { start, end, .. } => ([Some(start); 3], end),
            Self::Async { start1, start2, start3, end1, end2, end3, .. } => ([start1, start2, start3], if let Entrants::Three(_) = entrants_a {
                end1.and_then(|end1| Some(end1.max(end2?).max(end3?)))
            } else {
                end1.and_then(|end1| Some(end1.max(end2?)))
            }),
        };
        let (mut starts_b, end_b) = match *other {
            Self::Unscheduled => ([None; 3], None),
            Self::Live { start, end, .. } => ([Some(start); 3], end),
            Self::Async { start1, start2, start3, end1, end2, end3, .. } => ([start1, start2, start3], if let Entrants::Three(_) = entrants_b {
                end1.and_then(|end1| Some(end1.max(end2?).max(end3?)))
            } else {
                end1.and_then(|end1| Some(end1.max(end2?)))
            }),
        };
        let mut ordering = end_a.is_none().cmp(&end_b.is_none()) // races that have ended first
            .then_with(|| end_a.cmp(&end_b)); // races that ended earlier first
        if ordering.is_eq() {
            starts_a.sort_unstable();
            starts_b.sort_unstable();
            for (start_a, start_b) in starts_a.into_iter().zip_eq(starts_b) {
                ordering = ordering.then_with(|| start_a.is_none().cmp(&start_b.is_none())) // races with more starting times first
                    .then_with(|| start_a.cmp(&start_b)); // races with parts starting earlier first
            }
        }
        ordering
    }

    pub(crate) fn set_live_start(&mut self, new_start: DateTime<Utc>) {
        match self {
            Self::Live { start, .. } => *start = new_start,
            _ => *self = Self::Live { start: new_start, end: None, room: None },
        }
    }

    pub(crate) fn set_async_start1(&mut self, new_start: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match *self {
            Self::Unscheduled => {
                *self = Self::Async { start1: Some(new_start), start2: None, start3: None, end1: None, end2: None, end3: None, room1: None, room2: None, room3: None };
                None
            }
            Self::Live { start, .. } => {
                *self = Self::Async { start1: Some(new_start), start2: None, start3: None, end1: None, end2: None, end3: None, room1: None, room2: None, room3: None };
                Some(start)
            }
            Self::Async { ref mut start1, .. } => start1.replace(new_start),
        }
    }

    pub(crate) fn set_async_start2(&mut self, new_start: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match *self {
            Self::Unscheduled => {
                *self = Self::Async { start1: None, start2: Some(new_start), start3: None, end1: None, end2: None, end3: None, room1: None, room2: None, room3: None };
                None
            }
            Self::Live { start, .. } => {
                *self = Self::Async { start1: None, start2: Some(new_start), start3: None, end1: None, end2: None, end3: None, room1: None, room2: None, room3: None };
                Some(start)
            }
            Self::Async { ref mut start2, .. } => start2.replace(new_start),
        }
    }

    pub(crate) fn set_async_start3(&mut self, new_start: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match *self {
            Self::Unscheduled => {
                *self = Self::Async { start1: None, start2: None, start3: Some(new_start), end1: None, end2: None, end3: None, room1: None, room2: None, room3: None };
                None
            }
            Self::Live { start, .. } => {
                *self = Self::Async { start1: None, start2: None, start3: Some(new_start), end1: None, end2: None, end3: None, room1: None, room2: None, room3: None };
                Some(start)
            }
            Self::Async { ref mut start3, .. } => start3.replace(new_start),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Race {
    pub(crate) id: Id<Races>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) source: Source,
    pub(crate) entrants: Entrants,
    pub(crate) phase: Option<String>,
    pub(crate) round: Option<String>,
    pub(crate) game: Option<i16>,
    pub(crate) scheduling_thread: Option<ChannelId>,
    pub(crate) schedule: RaceSchedule,
    pub(crate) schedule_updated_at: Option<DateTime<Utc>>,
    pub(crate) fpa_invoked: bool,
    pub(crate) breaks_used: bool,
    pub(crate) draft: Option<Draft>,
    pub(crate) seed: seed::Data,
    pub(crate) video_urls: HashMap<Language, Url>,
    pub(crate) restreamers: HashMap<Language, String>,
    pub(crate) last_edited_by: Option<Id<Users>>,
    pub(crate) last_edited_at: Option<DateTime<Utc>>,
    /// An ignored race is treated as if it didn't exist for most purposes, with the notable exception of auto-import.
    /// This allows a race to be “deleted” without being recreated automatically.
    pub(crate) ignored: bool,
    pub(crate) schedule_locked: bool,
    pub(crate) notified: bool,
    pub(crate) async_notified_1: bool,
    pub(crate) async_notified_2: bool,
    pub(crate) async_notified_3: bool,
    pub(crate) discord_scheduled_event_id: Option<PgSnowflake<ScheduledEventId>>,
    pub(crate) volunteer_request_sent: bool,
    pub(crate) volunteer_request_message_id: Option<PgSnowflake<MessageId>>,
}

impl Race {
    pub(crate) async fn from_id(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, id: Id<Races>) -> Result<Self, Error> {
        let row = sqlx::query!(r#"SELECT
            series AS "series: Series",
            event,
            challonge_match,
            league_id,
            sheet_timestamp,
            startgg_event,
            startgg_set AS "startgg_set: startgg::ID",
            speedgaming_id,
            game,
            team1 AS "team1: Id<Teams>",
            team2 AS "team2: Id<Teams>",
            team3 AS "team3: Id<Teams>",
            p1,
            p2,
            p3,
            p1_discord AS "p1_discord: PgSnowflake<UserId>",
            p2_discord AS "p2_discord: PgSnowflake<UserId>",
            p1_racetime,
            p2_racetime,
            p1_twitch,
            p2_twitch,
            total,
            finished,
            phase,
            round,
            scheduling_thread AS "scheduling_thread: PgSnowflake<ChannelId>",
            draft_state AS "draft_state: Json<Draft>",
            start,
            async_start1,
            async_start2,
            async_start3,
            end_time,
            async_end1,
            async_end2,
            async_end3,
            room,
            async_room1,
            async_room2,
            async_room3,
            schedule_updated_at,
            fpa_invoked,
            breaks_used,
            seed_data,
            file_stem,
            locked_spoiler_log_path,
            web_id,
            web_gen_time,
            is_tfb_dev,
            tfb_uuid,
            xkeys_uuid,
            hash1,
            hash2,
            hash3,
            hash4,
            hash5,
            seed_password,
            video_url,
            restreamer,
            video_url_fr,
            restreamer_fr,
            video_url_de,
            restreamer_de,
            video_url_pt,
            restreamer_pt,
            last_edited_by AS "last_edited_by: Id<Users>",
            last_edited_at,
            ignored,
            schedule_locked,
            notified,
            async_notified_1,
            async_notified_2,
            async_notified_3,
            discord_scheduled_event_id AS "discord_scheduled_event_id: PgSnowflake<ScheduledEventId>",
            volunteer_request_sent,
            volunteer_request_message_id AS "volunteer_request_message_id: PgSnowflake<MessageId>"
        FROM races WHERE id = $1"#, id as _).fetch_one(&mut **transaction).await?;
        let source = if let Some(id) = row.challonge_match {
            Source::Challonge { id }
        } else if let Some(id) = row.league_id {
            Source::League { id }
        } else if let Some(timestamp) = row.sheet_timestamp {
            Source::Sheet { timestamp }
        } else if let (Some(event), Some(set)) = (row.startgg_event, row.startgg_set) {
            Source::StartGG { event, set }
        } else if let Some(id) = row.speedgaming_id {
            Source::SpeedGaming { id }
        } else {
            Source::Manual
        };
        let entrants = {
            let p1 = if let Some(team1) = row.team1 {
                Some(Entrant::MidosHouseTeam(Team::from_id(&mut *transaction, team1).await?.ok_or(Error::UnknownTeam)?))
            } else if let Some(PgSnowflake(id)) = row.p1_discord {
                Some(Entrant::Discord {
                    racetime_id: row.p1_racetime,
                    twitch_username: row.p1_twitch,
                    id,
                })
            } else if let Some(name) = row.p1 {
                Some(Entrant::Named {
                    racetime_id: row.p1_racetime,
                    twitch_username: row.p1_twitch,
                    name,
                })
            } else {
                None
            };
            let p2 = if let Some(team2) = row.team2 {
                Some(Entrant::MidosHouseTeam(Team::from_id(&mut *transaction, team2).await?.ok_or(Error::UnknownTeam)?))
            } else if let Some(PgSnowflake(id)) = row.p2_discord {
                Some(Entrant::Discord {
                    racetime_id: row.p2_racetime,
                    twitch_username: row.p2_twitch,
                    id,
                })
            } else if let Some(name) = row.p2 {
                Some(Entrant::Named {
                    racetime_id: row.p2_racetime,
                    twitch_username: row.p2_twitch,
                    name,
                })
            } else {
                None
            };
            let p3 = if let Some(team3) = row.team3 {
                Some(Entrant::MidosHouseTeam(Team::from_id(&mut *transaction, team3).await?.ok_or(Error::UnknownTeam)?))
            } else if let Some(name) = row.p3 {
                Some(Entrant::Named { racetime_id: None, twitch_username: None, name })
            } else {
                None
            };
            match [p1, p2, p3] {
                [Some(p1), Some(p2), Some(p3)] => Entrants::Three([p1, p2, p3]),
                [Some(p1), Some(p2), None] => Entrants::Two([p1, p2]),
                [Some(Entrant::Named { name, .. }), None, None] => Entrants::Named(name),
                [None, None, None] => if let (Some(total), Some(finished)) = (row.total, row.finished) {
                    Entrants::Count {
                        total: total as u32,
                        finished: finished as u32,
                    }
                } else {
                    Entrants::Open
                },
                _ => panic!("unexpected configuration of entrants"),
            }
        };

        macro_rules! update_end {
            ($var:ident, $room:ident, $query:literal) => {
                let $var = if let Some(end) = row.$var {
                    Some(end)
                } else if let Some(ref room) = row.$room {
                    let end = http_client.get(format!("{room}/data"))
                        .send().await?
                        .detailed_error_for_status().await?
                        .json_with_text_in_error::<RaceData>().await?
                        .ended_at;
                    if let Some(end) = end {
                        sqlx::query!($query, end, id as _).execute(&mut **transaction).await?;
                    }
                    end
                } else {
                    None
                };
            };
        }

        update_end!(end_time, room, "UPDATE races SET end_time = $1 WHERE id = $2");
        update_end!(async_end1, async_room1, "UPDATE races SET async_end1 = $1 WHERE id = $2");
        update_end!(async_end2, async_room2, "UPDATE races SET async_end2 = $1 WHERE id = $2");
        update_end!(async_end3, async_room3, "UPDATE races SET async_end3 = $1 WHERE id = $2");
        Ok(Self {
            series: row.series,
            event: row.event,
            phase: row.phase,
            round: row.round,
            game: row.game,
            scheduling_thread: row.scheduling_thread.map(|PgSnowflake(id)| id),
            schedule: RaceSchedule::new(
                row.start, row.async_start1, row.async_start2, row.async_start3,
                end_time, async_end1, async_end2, async_end3,
                row.room.map(|room| room.parse()).transpose()?, row.async_room1.map(|room| room.parse()).transpose()?, row.async_room2.map(|room| room.parse()).transpose()?, row.async_room3.map(|room| room.parse()).transpose()?,
            ),
            schedule_updated_at: row.schedule_updated_at,
            fpa_invoked: row.fpa_invoked,
            breaks_used: row.breaks_used,
            draft: row.draft_state.map(|Json(draft)| draft),
            seed: seed::Data::from_db(
                row.start,
                row.async_start1,
                row.async_start2,
                row.async_start3,
                row.file_stem,
                row.locked_spoiler_log_path,
                row.web_id,
                row.web_gen_time,
                row.is_tfb_dev,
                row.tfb_uuid,
                row.xkeys_uuid,
                row.seed_data,
                row.hash1,
                row.hash2,
                row.hash3,
                row.hash4,
                row.hash5,
                row.seed_password.as_deref(),
                false, // no official races with progression spoilers so far
            ),
            video_urls: all().filter_map(|language| match language {
                English => row.video_url.clone(),
                French => row.video_url_fr.clone(),
                German => row.video_url_de.clone(),
                Portuguese => row.video_url_pt.clone(),
            }.map(|video_url| Ok::<_, Error>((language, video_url.parse()?)))).try_collect()?,
            restreamers: all().filter_map(|language| match language {
                English => row.restreamer.clone(),
                French => row.restreamer_fr.clone(),
                German => row.restreamer_de.clone(),
                Portuguese => row.restreamer_pt.clone(),
            }.map(|restreamer| (language, restreamer))).collect(),
            last_edited_by: row.last_edited_by,
            last_edited_at: row.last_edited_at,
            ignored: row.ignored,
            schedule_locked: row.schedule_locked,
            notified: row.notified,
            async_notified_1: row.async_notified_1,
            async_notified_2: row.async_notified_2,
            async_notified_3: row.async_notified_3,
            discord_scheduled_event_id: row.discord_scheduled_event_id.map(|PgSnowflake(id)| PgSnowflake(id)),
            volunteer_request_sent: row.volunteer_request_sent,
            volunteer_request_message_id: row.volunteer_request_message_id.map(|PgSnowflake(id)| PgSnowflake(id)),
            id, source, entrants,
        })
    }

    pub(crate) async fn for_event(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, event: &event::Data<'_>) -> Result<Vec<Self>, Error> {
        let mut races = Vec::default();
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE series = $1 AND event = $2"#, event.series as _, &event.event).fetch_all(&mut **transaction).await? {
            races.push(Self::from_id(&mut *transaction, http_client, id).await?);
        }
        match event.series {
            Series::BattleRoyale => match &*event.event {
                "1" => {}
                _ => unimplemented!(),
            },
            Series::League => {} // this series is scheduled via the League website, which is auto-imported
            Series::Multiworld => match &*event.event {
                "1" => {} // no match data available
                _ => {} // new events are scheduled via Mido's House
            },
            Series::NineDaysOfSaws | Series::Pictionary => if let Some(race) = races.iter_mut().find(|race| race.series == event.series && race.event == event.event) {
                race.schedule = if let Some(start) = event.start(&mut *transaction).await? {
                    RaceSchedule::Live {
                        end: event.end,
                        room: event.url.clone(),
                        start,
                    }
                } else {
                    RaceSchedule::Unscheduled
                };
                if let Some(english_video_url) = event.video_url.clone() {
                    race.video_urls.entry(English).or_insert(english_video_url);
                }
                race
            } else {
                races.push(Self {
                    id: Id::<Races>::new(&mut *transaction).await?,
                    series: event.series,
                    event: event.event.to_string(),
                    source: Source::Manual,
                    entrants: Entrants::Open,
                    phase: None,
                    round: None,
                    game: None,
                    scheduling_thread: None,
                    schedule: if let Some(start) = event.start(&mut *transaction).await? {
                        RaceSchedule::Live {
                            end: event.end,
                            room: event.url.clone(),
                            start,
                        }
                    } else {
                        RaceSchedule::Unscheduled
                    },
                    schedule_updated_at: None,
                    fpa_invoked: false,
                    breaks_used: false,
                    draft: None,
                    seed: seed::Data::default(),
                    video_urls: event.video_url.iter().map(|video_url| (English, video_url.clone())).collect(), //TODO sync between event and race? Video URL fields for other languages on event::Data?
                    restreamers: HashMap::default(),
                    last_edited_by: None,
                    last_edited_at: None,
                    ignored: false,
                    schedule_locked: false,
                    notified: false,
                    async_notified_1: false,
                    async_notified_2: false,
                    async_notified_3: false,
                    discord_scheduled_event_id: None,
                    volunteer_request_sent: false,
                    volunteer_request_message_id: None,
                });
                races.last_mut().expect("just pushed")
            }.save(&mut *transaction).await?,
            Series::Rsl => match &*event.event {
                "1" => {} // no match data available
                _ => {} // new events are scheduled via Mido's House
            },
            Series::Scrubs => match &*event.event {
                "5" => {}
                "6" => {}
                _ => unimplemented!(),
            },
            Series::Standard => match &*event.event {
                _ => {} // new events are scheduled via Mido's House
            },
            Series::TwwrMain => match &*event.event {
                _ => {} // other TWWR events are scheduled via Mido's House
            },
            | Series::AlttprDe
            | Series::CoOp //TODO add archives of seasons 1 and 2?
            | Series::CopaDoBrasil
            | Series::Crosskeys
            | Series::MixedPools
            | Series::Mq
            | Series::MysteryD
            | Series::SongsOfHope
            | Series::SpeedGaming
            | Series::TournoiFrancophone
            | Series::TriforceBlitz
            | Series::WeTryToBeBetter
                => {} // these series are now scheduled via Mido's House
        }
        races.retain(|race| !race.ignored);
        races.sort_unstable();
        Ok(races)
    }

    pub(crate) async fn for_scheduling_channel(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, channel_id: ChannelId, game: Option<i16>, include_started: bool) -> Result<Vec<Self>, Error> {
        let mut races = Vec::default();
        let rows = match (game, include_started) {
            (None, false) => sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND scheduling_thread = $1 AND (start IS NULL OR start > NOW())"#, PgSnowflake(channel_id) as _).fetch_all(&mut **transaction).await?,
            (None, true) => sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND scheduling_thread = $1"#, PgSnowflake(channel_id) as _).fetch_all(&mut **transaction).await?,
            (Some(game), false) => sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND scheduling_thread = $1 AND game = $2 AND (start IS NULL OR start > NOW())"#, PgSnowflake(channel_id) as _, game).fetch_all(&mut **transaction).await?,
            (Some(game), true) => sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND scheduling_thread = $1 AND game = $2"#, PgSnowflake(channel_id) as _, game).fetch_all(&mut **transaction).await?,
        };
        for id in rows {
            races.push(Self::from_id(&mut *transaction, http_client, id).await?);
        }
        races.retain(|race| !race.ignored);
        races.sort_unstable();
        Ok(races)
    }

    pub(crate) async fn game_count(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<i16, Error> {
        let ([team1, team2, team3], [p1, p2, p3], [p1_discord, p2_discord], [p1_racetime, p2_racetime], [p1_twitch, p2_twitch], [total, finished]) = self.entrants.to_db();
        Ok(sqlx::query_scalar!(r#"SELECT game AS "game!" FROM races WHERE
            NOT ignored
            AND series = $1
            AND event = $2
            AND phase IS NOT DISTINCT FROM $3
            AND round IS NOT DISTINCT FROM $4
            AND game IS NOT NULL
            AND team1 IS NOT DISTINCT FROM $5
            AND team2 IS NOT DISTINCT FROM $6
            AND team3 IS NOT DISTINCT FROM $7
            AND p1 IS NOT DISTINCT FROM $8
            AND p2 IS NOT DISTINCT FROM $9
            AND p3 IS NOT DISTINCT FROM $10
            AND p1_discord IS NOT DISTINCT FROM $11
            AND p2_discord IS NOT DISTINCT FROM $12
            AND p1_racetime IS NOT DISTINCT FROM $13
            AND p2_racetime IS NOT DISTINCT FROM $14
            AND p1_twitch IS NOT DISTINCT FROM $15
            AND p2_twitch IS NOT DISTINCT FROM $16
            AND total IS NOT DISTINCT FROM $17
            AND finished IS NOT DISTINCT FROM $18
            ORDER BY game DESC LIMIT 1
        "#,
            self.series as _,
            self.event,
            self.phase,
            self.round,
            team1 as _,
            team2 as _,
            team3 as _,
            p1,
            p2,
            p3,
            p1_discord.map(PgSnowflake) as _,
            p2_discord.map(PgSnowflake) as _,
            p1_racetime,
            p2_racetime,
            p1_twitch,
            p2_twitch,
            total.map(|total| total as i32),
            finished.map(|finished| finished as i32),
        ).fetch_optional(&mut **transaction).await?.unwrap_or(1))
    }

    pub(crate) async fn next_game(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client) -> Result<Option<Self>, Error> {
        Ok(if_chain! {
            if let Some(game) = self.game;
            let ([team1, team2, team3], [p1, p2, p3], [p1_discord, p2_discord], [p1_racetime, p2_racetime], [p1_twitch, p2_twitch], [total, finished]) = self.entrants.to_db();
            if let Some(id) = sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE
                NOT ignored
                AND series = $1
                AND event = $2
                AND phase IS NOT DISTINCT FROM $3
                AND round IS NOT DISTINCT FROM $4
                AND game = $5
                AND team1 IS NOT DISTINCT FROM $6
                AND team2 IS NOT DISTINCT FROM $7
                AND team3 IS NOT DISTINCT FROM $8
                AND p1 IS NOT DISTINCT FROM $9
                AND p2 IS NOT DISTINCT FROM $10
                AND p3 IS NOT DISTINCT FROM $11
                AND p1_discord IS NOT DISTINCT FROM $12
                AND p2_discord IS NOT DISTINCT FROM $13
                AND p1_racetime IS NOT DISTINCT FROM $14
                AND p2_racetime IS NOT DISTINCT FROM $15
                AND p1_twitch IS NOT DISTINCT FROM $16
                AND p2_twitch IS NOT DISTINCT FROM $17
                AND total IS NOT DISTINCT FROM $18
                AND finished IS NOT DISTINCT FROM $19
            "#,
                self.series as _,
                self.event,
                self.phase,
                self.round,
                game + 1,
                team1 as _,
                team2 as _,
                team3 as _,
                p1,
                p2,
                p3,
                p1_discord.map(PgSnowflake) as _,
                p2_discord.map(PgSnowflake) as _,
                p1_racetime,
                p2_racetime,
                p1_twitch,
                p2_twitch,
                total.map(|total| total as i32),
                finished.map(|finished| finished as i32),
            ).fetch_optional(&mut **transaction).await?;
            then {
                Some(Self::from_id(&mut *transaction, http_client, id).await?)
            } else {
                None
            }
        })
    }

    pub(crate) async fn event(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<event::Data<'static>, event::DataError> {
        event::Data::new(transaction, self.series, self.event.clone()).await?.ok_or(event::DataError::Missing)
    }

    pub(crate) fn startgg_set_url(&self) -> Result<Option<Url>, url::ParseError> {
        Ok(if let Source::StartGG { ref event, set: startgg::ID(ref set), .. } = self.source {
            Some(format!("https://start.gg/{event}/set/{set}").parse()?)
        } else {
            None
        })
    }

    pub(crate) fn cal_events(&self) -> impl Iterator<Item = Event> + Send + use<> {
        match self.schedule {
            RaceSchedule::Unscheduled => Box::new(iter::empty()) as Box<dyn Iterator<Item = Event> + Send>,
            RaceSchedule::Live { .. } => Box::new(iter::once(Event { race: self.clone(), kind: EventKind::Normal })),
            RaceSchedule::Async { .. } => if let Entrants::Three(_) = self.entrants {
                Box::new([
                    Event { race: self.clone(), kind: EventKind::Async1 },
                    Event { race: self.clone(), kind: EventKind::Async2 },
                    Event { race: self.clone(), kind: EventKind::Async3 },
                ].into_iter()) as Box<dyn Iterator<Item = Event> + Send>
            } else {
                Box::new([
                    Event { race: self.clone(), kind: EventKind::Async1 },
                    Event { race: self.clone(), kind: EventKind::Async2 },
                ].into_iter())
            },
        }
    }

    /// The seed remains hidden until it's posted in the last calendar event of this race.
    pub(crate) fn show_seed(&self) -> bool {
        if let RaceSchedule::Unscheduled = self.schedule { return false }
        let now = Utc::now();
        self.cal_events().all(|event| event.is_private_async_part() || event.start().is_some_and(|start| start <= now + TimeDelta::minutes(15)) || event.end().is_some())
    }

    pub(crate) fn is_ended(&self) -> bool {
        // Since the end time of a race isn't known in advance, we assume that if a race has an end time, that end time is in the past.
        self.schedule.end_time(&self.entrants).is_some()
    }

    pub(crate) fn rooms(&self) -> impl Iterator<Item = Url> + Send + use<> {
        // hide room of private async parts until public part finished
        //TODO show to the team that played the private async part
        let all_ended = self.cal_events().all(|event| event.end().is_some());
        self.cal_events().filter(move |event| all_ended || !event.is_private_async_part()).filter_map(|event| event.room().cloned())
    }

    /// Returns an iterator over all entrants that are Mido's House teams, skipping any that aren't.
    pub(crate) fn teams(&self) -> impl Iterator<Item = &Team> + Send {
        match self.entrants {
            Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => Box::new(iter::empty()) as Box<dyn Iterator<Item = &Team> + Send>,
            Entrants::Two([ref team1, ref team2]) => Box::new([team1, team2].into_iter().filter_map(as_variant!(Entrant::MidosHouseTeam))),
            Entrants::Three([ref team1, ref team2, ref team3]) => Box::new([team1, team2, team3].into_iter().filter_map(as_variant!(Entrant::MidosHouseTeam))),
        }
    }

    /// If all entrants are Mido's House teams, returns `Some` with an iterator over them.
    pub(crate) fn teams_opt(&self) -> Option<impl Iterator<Item = &Team> + Send> {
        match self.entrants {
            Entrants::Two([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2)]) => Some(Box::new([team1, team2].into_iter()) as Box<dyn Iterator<Item = &Team> + Send>),
            Entrants::Three([Entrant::MidosHouseTeam(ref team1), Entrant::MidosHouseTeam(ref team2), Entrant::MidosHouseTeam(ref team3)]) => Some(Box::new([team1, team2, team3].into_iter())),
            Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) | Entrants::Two(_) | Entrants::Three(_) => None,
        }
    }

    pub(crate) async fn multistream_url_prerace(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, event: &event::Data<'_>) -> Result<Option<Url>, Error> {
        async fn entrant_twitch_names<'a>(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, event: &event::Data<'_>, entrant: &'a Entrant) -> Result<Option<Vec<Cow<'a, str>>>, Error> {
            let mut channels = Vec::default();
            match entrant {
                Entrant::MidosHouseTeam(team) => for (member, role) in team.members_roles(&mut *transaction).await? {
                    if event.team_config.role_is_racing(role) {
                        if let Some(twitch_name) = member.racetime_user_data(http_client).await?.and_then(identity).and_then(|racetime_user_data| racetime_user_data.twitch_name) {
                            channels.push(Cow::Owned(twitch_name));
                        } else {
                            return Ok(None)
                        }
                    }
                },
                Entrant::Discord { twitch_username: Some(twitch_name), .. } | Entrant::Named { twitch_username: Some(twitch_name), .. } => channels.push(Cow::Borrowed(&**twitch_name)),
                Entrant::Discord { twitch_username: None, racetime_id: Some(racetime_id), .. } | Entrant::Named { twitch_username: None, racetime_id: Some(racetime_id), .. } => {
                    let racetime_user_data = racetime_bot::user_data(http_client, racetime_id).await?;
                    if let Some(twitch_name) = racetime_user_data.and_then(|racetime_user_data| racetime_user_data.twitch_name) {
                        channels.push(Cow::Owned(twitch_name));
                    } else {
                        return Ok(None)
                    }
                }
                Entrant::Discord { twitch_username: None, racetime_id: None, id } => if_chain! {
                    if let Some(user) = User::from_discord(&mut **transaction, *id).await?;
                    if let Some(Some(racetime_user_data)) = user.racetime_user_data(http_client).await?;
                    if let Some(twitch_name) = racetime_user_data.twitch_name;
                    then {
                        channels.push(Cow::Owned(twitch_name));
                    } else {
                        return Ok(None)
                    }
                },
                Entrant::Named { twitch_username: None, racetime_id: None, .. } => return Ok(None),
            }
            Ok(Some(channels))
        }

        Ok(match self.entrants {
            Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => None,
            Entrants::Two(ref entrants) => {
                let mut channels = Vec::default();
                for entrant in entrants {
                    if let Some(twitch_names) = entrant_twitch_names(&mut *transaction, http_client, event, entrant).await? {
                        channels.extend(twitch_names);
                    } else {
                        return Ok(None)
                    }
                }
                let mut url = Url::parse("https://multistre.am/").unwrap();
                url.path_segments_mut().unwrap().extend(&channels).push(match channels.len() {
                    0 => return Ok(None),
                    2 => "layout4",
                    4 => "layout12",
                    6 => "layout18",
                    _ => unimplemented!(),
                });
                Some(url)
            }
            Entrants::Three(ref entrants) => {
                let mut channels = Vec::default();
                for entrant in entrants {
                    if let Some(twitch_names) = entrant_twitch_names(&mut *transaction, http_client, event, entrant).await? {
                        channels.extend(twitch_names);
                    } else {
                        return Ok(None)
                    }
                }
                let mut url = Url::parse("https://multistre.am/").unwrap();
                url.path_segments_mut().unwrap().extend(&channels).push(match channels.len() {
                    0 => return Ok(None),
                    3 => "layout7",
                    6 => "layout17",
                    _ => unimplemented!(),
                });
                Some(url)
            }
        })
    }

    pub(crate) async fn multistream_url(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, event: &event::Data<'_>) -> Result<Option<Url>, Error> {
        async fn entrant_twitch_names<'a>(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, event: &event::Data<'_>, entrant: &'a Entrant) -> Result<Option<Vec<Cow<'a, str>>>, Error> {
            let mut channels = Vec::default();
            match entrant {
                Entrant::MidosHouseTeam(team) => for (member, role) in team.members_roles(&mut *transaction).await? {
                    if event.team_config.role_is_racing(role) {
                        if let Some(twitch_name) = member.racetime_user_data(http_client).await?.and_then(identity).and_then(|racetime_user_data| racetime_user_data.twitch_name) {
                            channels.push(Cow::Owned(twitch_name));
                        } else {
                            return Ok(None)
                        }
                    }
                },
                Entrant::Discord { twitch_username: Some(twitch_name), .. } | Entrant::Named { twitch_username: Some(twitch_name), .. } => channels.push(Cow::Borrowed(&**twitch_name)),
                Entrant::Discord { twitch_username: None, racetime_id: Some(racetime_id), .. } | Entrant::Named { twitch_username: None, racetime_id: Some(racetime_id), .. } => {
                    let racetime_user_data = racetime_bot::user_data(http_client, racetime_id).await?;
                    if let Some(twitch_name) = racetime_user_data.and_then(|racetime_user_data| racetime_user_data.twitch_name) {
                        channels.push(Cow::Owned(twitch_name));
                    } else {
                        return Ok(None)
                    }
                }
                Entrant::Discord { twitch_username: None, racetime_id: None, id } => if_chain! {
                    if let Some(user) = User::from_discord(&mut **transaction, *id).await?;
                    if let Some(Some(racetime_user_data)) = user.racetime_user_data(http_client).await?;
                    if let Some(twitch_name) = racetime_user_data.twitch_name;
                    then {
                        channels.push(Cow::Owned(twitch_name));
                    } else {
                        return Ok(None)
                    }
                },
                Entrant::Named { twitch_username: None, racetime_id: None, .. } => return Ok(None),
            }
            Ok(Some(channels))
        }

        Ok(if let RaceSchedule::Live { room: Some(_), .. } = self.schedule {
            match self.entrants {
                Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => None,
                Entrants::Two(ref entrants) => {
                    let mut channels = Vec::default();
                    for entrant in entrants {
                        if let Some(twitch_names) = entrant_twitch_names(&mut *transaction, http_client, event, entrant).await? {
                            channels.extend(twitch_names);
                        } else {
                            return Ok(None)
                        }
                    }
                    let mut url = Url::parse("https://multistre.am/").unwrap();
                    url.path_segments_mut().unwrap().extend(&channels).push(match channels.len() {
                        0 => return Ok(None),
                        2 => "layout4",
                        4 => "layout12",
                        6 => "layout18",
                        _ => unimplemented!(),
                    });
                    Some(url)
                }
                Entrants::Three(ref entrants) => {
                    let mut channels = Vec::default();
                    for entrant in entrants {
                        if let Some(twitch_names) = entrant_twitch_names(&mut *transaction, http_client, event, entrant).await? {
                            channels.extend(twitch_names);
                        } else {
                            return Ok(None)
                        }
                    }
                    let mut url = Url::parse("https://multistre.am/").unwrap();
                    url.path_segments_mut().unwrap().extend(&channels).push(match channels.len() {
                        0 => return Ok(None),
                        3 => "layout7",
                        6 => "layout17",
                        _ => unimplemented!(),
                    });
                    Some(url)
                }
            }
        } else {
            None
        })
    }

    pub(crate) async fn player_video_urls(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<(User, Url)>, Error> {
        let rows = sqlx::query!(r#"SELECT player AS "player: Id<Users>", video FROM race_player_videos WHERE race = $1"#, self.id as _).fetch_all(&mut **transaction).await?;
        let mut tuples = Vec::with_capacity(rows.len());
        for row in rows {
            tuples.push((User::from_id(&mut **transaction, row.player).await?.expect("foreign key constraint violated"), row.video.parse()?));
        }
        Ok(tuples)
    }

    pub(crate) fn has_any_room(&self) -> bool {
        match &self.schedule {
            RaceSchedule::Unscheduled => false,
            RaceSchedule::Live { room, .. } => room.is_some(),
            RaceSchedule::Async { room1, room2, room3, .. } => room1.is_some() || room2.is_some() || room3.is_some(),
        }
    }

    pub(crate) fn has_room_for(&self, team: &Team) -> bool {
        match &self.schedule {
            RaceSchedule::Unscheduled => false,
            RaceSchedule::Live { room, .. } => room.is_some(),
            RaceSchedule::Async { room1, room2, room3, .. } => match &self.entrants {
                Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => panic!("asynced race not with Entrants::Two or Entrants::Three"),
                Entrants::Two([team1, team2]) => {
                    if let Entrant::MidosHouseTeam(team1) = team1 {
                        if team == team1 {
                            return room1.is_some()
                        }
                    }
                    if let Entrant::MidosHouseTeam(team2) = team2 {
                        if team == team2 {
                            return room2.is_some()
                        }
                    }
                    false
                }
                Entrants::Three([team1, team2, team3]) => {
                    if let Entrant::MidosHouseTeam(team1) = team1 {
                        if team == team1 {
                            return room1.is_some()
                        }
                    }
                    if let Entrant::MidosHouseTeam(team2) = team2 {
                        if team == team2 {
                            return room2.is_some()
                        }
                    }
                    if let Entrant::MidosHouseTeam(team3) = team3 {
                        if team == team3 {
                            return room3.is_some()
                        }
                    }
                    false
                }
            },
        }
    }

    pub(crate) async fn single_settings(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<Option<seed::Settings>, Error> {
        let event = self.event(transaction).await?;
        Ok(if let Some(settings) = event.single_settings {
            Some(settings)
        } else if let Some(draft) = &self.draft {
            let Some(draft_kind) = event.draft_kind() else { return Ok(None) };
            match draft.next_step(draft_kind, None, &mut draft::MessageContext::None).await?.kind {
                draft::StepKind::Done(settings) => Some(settings),
                draft::StepKind::DoneRsl { .. } => None, //TODO
                draft::StepKind::GoFirst | draft::StepKind::Ban { .. } | draft::StepKind::Pick { .. } | draft::StepKind::BooleanChoice { .. } => None,
            }
        } else {
            None
        })
    }

    pub(crate) async fn save(&self, transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<()> {
        let (challonge_match, league_id, sheet_timestamp, startgg_event, startgg_set, speedgaming_id) = match self.source {
            Source::Manual => (None, None, None, None, None, None),
            Source::Challonge { ref id } => (Some(id), None, None, None, None, None),
            Source::League { id } => (None, Some(id), None, None, None, None),
            Source::Sheet { timestamp } => (None, None, Some(timestamp), None, None, None),
            Source::StartGG { ref event, ref set } => (None, None, None, Some(event), Some(set), None),
            Source::SpeedGaming { id } => (None, None, None, None, None, Some(id)),
        };
        let ([team1, team2, team3], [p1, p2, p3], [p1_discord, p2_discord], [p1_racetime, p2_racetime], [p1_twitch, p2_twitch], [total, finished]) = self.entrants.to_db();
        let (start, [async_start1, async_start2, async_start3], end, [async_end1, async_end2, async_end3], room, [async_room1, async_room2, async_room3]) = match self.schedule {
            RaceSchedule::Unscheduled => (None, [None; 3], None, [None; 3], None, [None; 3]),
            RaceSchedule::Live { start, end, ref room } => (Some(start), [None; 3], end, [None; 3], room.as_ref(), [None; 3]),
            RaceSchedule::Async { start1, start2, start3, end1, end2, end3, ref room1, ref room2, ref room3 } => (None, [start1, start2, start3], None, [end1, end2, end3], None, [room1.as_ref(), room2.as_ref(), room3.as_ref()]),
        };
        let (web_id, web_gen_time, file_stem, locked_spoiler_log_path, is_tfb_dev, tfb_uuid, xkeys_uuid) = match self.seed.files {
            Some(seed::Files::AlttprDoorRando { uuid}) => (None, None, None, None, false, None, Some(uuid)),
            Some(seed::Files::MidosHouse { ref file_stem, ref locked_spoiler_log_path }) => (None, None, Some(file_stem), locked_spoiler_log_path.as_ref(), false, None, None),
            Some(seed::Files::OotrWeb { id, gen_time, ref file_stem }) => (Some(id), Some(gen_time), Some(file_stem), None, false, None, None),
            Some(seed::Files::TriforceBlitz { is_dev, uuid }) => (None, None, None, None, is_dev, Some(uuid), None),
            Some(seed::Files::TfbSotd { .. }) => unimplemented!("Triforce Blitz seed of the day not supported for official races"),
            Some(seed::Files::TwwrPermalink { .. }) => (None, None, None, None, false, None, None),
            None => (None, None, None, None, false, None, None),
        };
        sqlx::query!("
            INSERT INTO races              (startgg_set, start, series, event, async_start2, async_start1, room, scheduling_thread, async_room1, async_room2, draft_state, async_end1, async_end2, end_time, team1, team2, web_id, web_gen_time, file_stem, hash1, hash2, hash3, hash4, hash5, game, id,  p1,  p2,  last_edited_by, last_edited_at, video_url, phase, round, ignored, p3,  startgg_event, total, finished, tfb_uuid, video_url_fr, restreamer, restreamer_fr, locked_spoiler_log_path, video_url_pt, restreamer_pt, p1_twitch, p2_twitch, p1_discord, p2_discord, schedule_locked, team3, schedule_updated_at, video_url_de, restreamer_de, sheet_timestamp, league_id, p1_racetime, p2_racetime, async_start3, async_room3, async_end3, challonge_match, seed_password, speedgaming_id, notified, is_tfb_dev, fpa_invoked, breaks_used, xkeys_uuid, async_notified_1, async_notified_2, async_notified_3, discord_scheduled_event_id)
            VALUES                         ($1,          $2,    $3,     $4,    $5,           $6,           $7,   $8,                $9,          $10,         $11,         $12,        $13,        $14,      $15,   $16,   $17,    $18,          $19,       $20,   $21,   $22,   $23,   $24,   $25,  $26, $27, $28, $29,            $30,            $31,       $32,   $33,   $34,     $35, $36,           $37,   $38,      $39,      $40,          $41,        $42,           $43,                     $44,          $45,           $46,       $47,       $48,        $49,        $50,             $51,   $52,                 $53,          $54,           $55,             $56,       $57,         $58,         $59,          $60,         $61,        $62,             $63,           $64,            $65,      $66,        $67,         $68,          $69,        $70,        $71,        $72,                       $73)
            ON CONFLICT (id) DO UPDATE SET (startgg_set, start, series, event, async_start2, async_start1, room, scheduling_thread, async_room1, async_room2, draft_state, async_end1, async_end2, end_time, team1, team2, web_id, web_gen_time, file_stem, hash1, hash2, hash3, hash4, hash5, game, id,  p1,  p2,  last_edited_by, last_edited_at, video_url, phase, round, ignored, p3,  startgg_event, total, finished, tfb_uuid, video_url_fr, restreamer, restreamer_fr, locked_spoiler_log_path, video_url_pt, restreamer_pt, p1_twitch, p2_twitch, p1_discord, p2_discord, schedule_locked, team3, schedule_updated_at, video_url_de, restreamer_de, sheet_timestamp, league_id, p1_racetime, p2_racetime, async_start3, async_room3, async_end3, challonge_match, seed_password, speedgaming_id, notified, is_tfb_dev, fpa_invoked, breaks_used, xkeys_uuid, async_notified_1, async_notified_2, async_notified_3, discord_scheduled_event_id)
            =                              ($1,          $2,    $3,     $4,    $5,           $6,           $7,   $8,                $9,          $10,         $11,         $12,        $13,        $14,      $15,   $16,   $17,    $18,          $19,       $20,   $21,   $22,   $23,   $24,   $25,  $26, $27, $28, $29,            $30,            $31,       $32,   $33,   $34,     $35, $36,           $37,   $38,      $39,      $40,          $41,        $42,           $43,                     $44,          $45,           $46,       $47,       $48,        $49,        $50,             $51,   $52,                 $53,          $54,           $55,             $56,       $57,         $58,         $59,          $60,         $61,        $62,             $63,           $64,            $65,      $66,        $67,         $68,          $69,        $70,        $71,        $72,                       $73)
        ",
            startgg_set as _,
            start,
            self.series as _,
            self.event,
            async_start2,
            async_start1,
            room.map(|url| url.to_string()),
            self.scheduling_thread.map(PgSnowflake) as _,
            async_room1.map(|url| url.to_string()),
            async_room2.map(|url| url.to_string()),
            self.draft.as_ref().map(Json) as _,
            async_end1,
            async_end2,
            end,
            team1 as _,
            team2 as _,
            web_id.map(|web_id| web_id as i64),
            web_gen_time,
            file_stem.map(|file_stem| &**file_stem),
            self.seed.file_hash.as_ref().map(|[hash1, _, _, _, _]| hash1),
            self.seed.file_hash.as_ref().map(|[_, hash2, _, _, _]| hash2),
            self.seed.file_hash.as_ref().map(|[_, _, hash3, _, _]| hash3),
            self.seed.file_hash.as_ref().map(|[_, _, _, hash4, _]| hash4),
            self.seed.file_hash.as_ref().map(|[_, _, _, _, hash5]| hash5),
            self.game,
            self.id as _,
            p1,
            p2,
            self.last_edited_by as _,
            self.last_edited_at,
            self.video_urls.get(&English).map(|url| url.to_string()),
            self.phase,
            self.round,
            self.ignored,
            p3,
            startgg_event,
            total.map(|total| total as i32),
            finished.map(|finished| finished as i32),
            tfb_uuid,
            self.video_urls.get(&French).map(|url| url.to_string()),
            self.restreamers.get(&English),
            self.restreamers.get(&French),
            locked_spoiler_log_path,
            self.video_urls.get(&Portuguese).map(|url| url.to_string()),
            self.restreamers.get(&Portuguese),
            p1_twitch,
            p2_twitch,
            p1_discord.map(PgSnowflake) as _,
            p2_discord.map(PgSnowflake) as _,
            self.schedule_locked,
            team3 as _,
            self.schedule_updated_at,
            self.video_urls.get(&German).map(|url| url.to_string()),
            self.restreamers.get(&German),
            sheet_timestamp,
            league_id,
            p1_racetime,
            p2_racetime,
            async_start3,
            async_room3.map(|url| url.to_string()),
            async_end3,
            challonge_match,
            self.seed.password.map(|password| password.into_iter().map(char::from).collect::<String>()),
            speedgaming_id,
            self.notified,
            is_tfb_dev,
            self.fpa_invoked,
            self.breaks_used,
            xkeys_uuid,
            self.async_notified_1,
            self.async_notified_2,
            self.async_notified_3,
            self.discord_scheduled_event_id as _
        ).execute(&mut **transaction).await?;
        Ok(())
    }
}

impl PartialEq for Race {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Equal
    }
}

impl Eq for Race {}

impl PartialOrd for Race {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Race {
    fn cmp(&self, other: &Self) -> Ordering {
        self.schedule.cmp(&self.entrants, &other.schedule, &other.entrants)
            .then_with(|| self.series.slug().cmp(other.series.slug()))
            .then_with(|| self.event.cmp(&other.event))
            .then_with(|| self.phase.cmp(&other.phase))
            .then_with(|| self.round.cmp(&other.round))
            .then_with(|| self.source.cmp(&other.source))
            .then_with(|| self.game.cmp(&other.game))
            .then_with(|| self.id.cmp(&other.id))
    }
}

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) race: Race,
    pub(crate) kind: EventKind,
}

impl Event {
    pub(crate) async fn from_room(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, room: Url) -> Result<Option<Self>, Error> {
        if let Some(id) = sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE room = $1 AND start IS NOT NULL"#, room.to_string()).fetch_optional(&mut **transaction).await? {
            return Ok(Some(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Normal,
            }))
        }
        if let Some(id) = sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE async_room1 = $1 AND async_start1 IS NOT NULL"#, room.to_string()).fetch_optional(&mut **transaction).await? {
            return Ok(Some(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Async1,
            }))
        }
        if let Some(id) = sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE async_room2 = $1 AND async_start2 IS NOT NULL"#, room.to_string()).fetch_optional(&mut **transaction).await? {
            return Ok(Some(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Async2,
            }))
        }
        if let Some(id) = sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE async_room3 = $1 AND async_start3 IS NOT NULL"#, room.to_string()).fetch_optional(&mut **transaction).await? {
            return Ok(Some(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Async3,
            }))
        }
        Ok(None)
    }

    pub(crate) async fn rooms_to_open(transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client) -> Result<Vec<Self>, Error> {
        let mut events = Vec::default();
        // Query with a generous window (60 minutes) to accommodate custom room_open_minutes_before
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND room IS NULL AND start IS NOT NULL AND start > NOW() AND (start <= NOW() + TIME '01:00:00' OR (team1 IS NULL AND p1_discord IS NULL AND p1 IS NULL AND (series != 's' OR event != 'w') AND start <= NOW() + TIME '01:00:00'))"#).fetch_all(&mut **transaction).await? {
            let race = Race::from_id(&mut *transaction, http_client, id).await?;

            // Check if this is a weekly race with custom room opening timing
            let room_open_minutes = if race.phase.is_none() {
                if let Some(round) = race.round.as_deref().and_then(|r| r.strip_suffix(" Weekly")) {
                    if let Ok(Some(schedule)) = WeeklySchedule::for_round(&mut *transaction, race.series, &race.event, round).await {
                        schedule.room_open_minutes_before as i64
                    } else {
                        30 // Default if weekly schedule not found
                    }
                } else {
                    30 // Default for non-weekly races
                }
            } else {
                30 // Default for races with phases (not weeklies)
            };

            // Only include the race if it's within the configured time window
            if let RaceSchedule::Live { start, .. } = race.schedule {
                let now = Utc::now();
                let minutes_until_start = (start - now).num_minutes();
                if minutes_until_start <= room_open_minutes && minutes_until_start > 0 {
                    events.push(Self {
                        race,
                        kind: EventKind::Normal,
                    });
                }
            }
        }
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND async_room1 IS NULL AND async_notified_1 IS NOT TRUE AND async_start1 IS NOT NULL AND async_start1 > NOW() AND async_start1 <= NOW() + TIME '00:30:00'"#).fetch_all(&mut **transaction).await? {
            events.push(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Async1,
            });
        }
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND async_room2 IS NULL AND async_notified_2 IS NOT TRUE AND async_start2 IS NOT NULL AND async_start2 > NOW() AND async_start2 <= NOW() + TIME '00:30:00'"#).fetch_all(&mut **transaction).await? {
            events.push(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Async2,
            });
        }
        for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE NOT ignored AND async_room3 IS NULL AND async_notified_3 IS NOT TRUE AND async_start3 IS NOT NULL AND async_start3 > NOW() AND async_start3 <= NOW() + TIME '00:30:00'"#).fetch_all(&mut **transaction).await? {
            events.push(Self {
                race: Race::from_id(&mut *transaction, http_client, id).await?,
                kind: EventKind::Async3,
            });
        }
        Ok(events)
    }

    pub(crate) fn active_teams(&self) -> impl Iterator<Item = &Team> + Send {
        match self.race.entrants {
            Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => Box::new(iter::empty()) as Box<dyn Iterator<Item = &Team> + Send>,
            Entrants::Two([ref team1, ref team2]) => Box::new([
                matches!(self.kind, EventKind::Normal | EventKind::Async1).then_some(team1),
                matches!(self.kind, EventKind::Normal | EventKind::Async2).then_some(team2),
            ].into_iter().filter_map(identity).filter_map(as_variant!(Entrant::MidosHouseTeam))),
            Entrants::Three([ref team1, ref team2, ref team3]) => Box::new([
                matches!(self.kind, EventKind::Normal | EventKind::Async1).then_some(team1),
                matches!(self.kind, EventKind::Normal | EventKind::Async2).then_some(team2),
                matches!(self.kind, EventKind::Normal | EventKind::Async3).then_some(team3),
            ].into_iter().filter_map(identity).filter_map(as_variant!(Entrant::MidosHouseTeam))),
        }
    }

    pub(crate) async fn racetime_users_to_invite(&self, transaction: &mut Transaction<'_, Postgres>, discord_ctx: &DiscordCtx, event: &event::Data<'_>) -> Result<Vec<Result<String, String>>, discord_bot::Error> {
        let mut buf = Vec::default();
        let entrants = match self.race.entrants {
            Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => Box::new(iter::empty()) as Box<dyn Iterator<Item = &Entrant> + Send>,
            Entrants::Two([ref team1, ref team2]) => Box::new([
                matches!(self.kind, EventKind::Normal | EventKind::Async1).then_some(team1),
                matches!(self.kind, EventKind::Normal | EventKind::Async2).then_some(team2),
            ].into_iter().filter_map(identity)),
            Entrants::Three([ref team1, ref team2, ref team3]) => Box::new([
                matches!(self.kind, EventKind::Normal | EventKind::Async1).then_some(team1),
                matches!(self.kind, EventKind::Normal | EventKind::Async2).then_some(team2),
                matches!(self.kind, EventKind::Normal | EventKind::Async3).then_some(team3),
            ].into_iter().filter_map(identity)),
        };
        for entrant in entrants {
            match entrant {
                Entrant::MidosHouseTeam(team) => for (member, role) in team.members_roles(&mut *transaction).await? {
                    if event.team_config.role_is_racing(role) {
                        buf.push(if let Some(member) = member.racetime {
                            Ok(member.id)
                        } else {
                            Err(format!(
                                "Warning: {member} could not be invited because {subj} {has_not} linked {poss} racetime.gg account to {poss} Hyrule Town Hall account. Please contact an organizer to invite {obj} manually for now.",
                                subj = member.subjective_pronoun(),
                                has_not = if member.subjective_pronoun_uses_plural_form() { "haven't" } else { "hasn't" },
                                poss = member.possessive_determiner(),
                                obj = member.objective_pronoun(),
                            ))
                        });
                    }
                },
                Entrant::Discord { racetime_id, .. } | Entrant::Named { racetime_id, .. } => {
                    assert!(matches!(event.team_config, TeamConfig::Solo));
                    buf.push(if let Some(racetime_id) = racetime_id {
                        Ok(racetime_id.clone())
                    } else {
                        Err(format!("Warning: {} could not be invited. Please contact an organizer to invite them manually.", entrant.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)"))))
                    });
                }
            }
        }
        Ok(buf)
    }

    pub(crate) fn room(&self) -> Option<&Url> {
        match self.race.schedule {
            RaceSchedule::Unscheduled => None,
            RaceSchedule::Live { ref room, .. } => room.as_ref(),
            RaceSchedule::Async { ref room1, ref room2, ref room3, .. } => match self.kind {
                EventKind::Normal => unreachable!(),
                EventKind::Async1 => room1.as_ref(),
                EventKind::Async2 => room2.as_ref(),
                EventKind::Async3 => room3.as_ref(),
            },
        }
    }

    pub(crate) fn start(&self) -> Option<DateTime<Utc>> {
        match self.race.schedule {
            RaceSchedule::Unscheduled => None,
            RaceSchedule::Live { start, .. } => Some(start),
            RaceSchedule::Async { start1, start2, start3, .. } => match self.kind {
                EventKind::Normal => unreachable!(),
                EventKind::Async1 => start1,
                EventKind::Async2 => start2,
                EventKind::Async3 => start3,
            },
        }
    }

    pub(crate) fn end(&self) -> Option<DateTime<Utc>> {
        match self.race.schedule {
            RaceSchedule::Unscheduled => None,
            RaceSchedule::Live { end, .. } => end,
            RaceSchedule::Async { end1, end2, end3, .. } => match self.kind {
                EventKind::Normal => unreachable!(),
                EventKind::Async1 => end1,
                EventKind::Async2 => end2,
                EventKind::Async3 => end3,
            },
        }
    }

    pub(crate) fn is_private_async_part(&self) -> bool {
        match self.race.schedule {
            RaceSchedule::Unscheduled | RaceSchedule::Live { .. } => false,
            RaceSchedule::Async { start1, start2, start3, .. } => match self.race.entrants {
                Entrants::Two(_) => match self.kind {
                    EventKind::Async1 => start1.is_some_and(|start1| start2.is_none_or(|start2| start1 <= start2)),
                    EventKind::Async2 => start2.is_some_and(|start2| start1.is_none_or(|start1| start2 < start1)),
                    EventKind::Normal | EventKind::Async3 => unreachable!(),
                },
                Entrants::Three(_) => match self.kind {
                    EventKind::Async1 => start1.is_some_and(|start1| start2.is_none_or(|start2| start1 <= start2) || start3.is_none_or(|start3| start1 <= start3)),
                    EventKind::Async2 => start2.is_some_and(|start2| start1.is_none_or(|start1| start2 < start1) || start3.is_none_or(|start3| start2 <= start3)),
                    EventKind::Async3 => start3.is_some_and(|start3| start1.is_none_or(|start1| start3 < start1) || start2.is_none_or(|start2| start3 < start2)),
                    EventKind::Normal => unreachable!(),
                },
                Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => unreachable!(),
            },
        }
    }

    pub(crate) fn is_public_async_part(&self) -> bool {
        match self.race.schedule {
            RaceSchedule::Unscheduled | RaceSchedule::Live { .. } => false,
            RaceSchedule::Async { .. } => !self.is_private_async_part(),
        }
    }

    pub(crate) async fn should_create_room(&self, transaction: &mut Transaction<'_, Postgres>, event: &event::Data<'_>) -> Result<RaceHandleMode, event::DataError> {
        Ok(if racetime_bot::Goal::for_event(self.race.series, &self.race.event).is_some() {
            if_chain! {
                if self.race.series == Series::SpeedGaming && self.race.event.ends_with("live");
                if let Some(race_start) = self.start();
                if event.start(transaction).await?.is_some_and(|event_start| event_start <= race_start);
                then {
                    // don't create racetime.gg rooms for in-person races
                    RaceHandleMode::Notify
                } else {
                    if matches!(self.kind, EventKind::Normal) || event.team_config.is_racetime_team_format() {
                        RaceHandleMode::RaceTime
                    } else {
                        // racetime.gg doesn't support single-entrant races
                        RaceHandleMode::Discord
                    }
                }
            }
        } else {
            // the organizers of this event didn't request for Mido to handle official races, so we ignore this race even if it would otherwise not be handled on racetime.gg
            RaceHandleMode::None
        })
    }
}

pub(crate) enum RaceHandleMode {
    None,
    Notify,
    RaceTime,
    Discord,
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)] ChronoParse(#[from] chrono::format::ParseError),
    #[error(transparent)] Discord(#[from] discord_bot::Error),
    #[error(transparent)] Draft(#[from] draft::Error),
    #[error(transparent)] Event(#[from] event::DataError),
    #[error(transparent)] OotrWeb(#[from] ootr_web::Error),
    #[error(transparent)] ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] SeedData(#[from] seed::ExtraDataError),
    #[error(transparent)] Sheets(#[from] sheets::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] StartGG(#[from] startgg::Error),
    #[error(transparent)] TimeFromLocal(#[from] wheel::traits::TimeFromLocalError<DateTime<Tz>>),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("anonymized entrant in race without hidden entrants")]
    AnonymizedEntrant,
    #[error("attempted to rewrite start.gg phase/round name with pool placeholder, but start.gg didn't report a pool name")]
    PoolPlaceholder,
    #[error("no team with this ID")]
    UnknownTeam,
    #[error("start.gg team ID {0} is not associated with a Hyrule Town Hall team")]
    UnknownTeamStartGG(startgg::ID),
}

impl From<racetime::model::AnonymousError> for Error {
    fn from(racetime::model::AnonymousError: racetime::model::AnonymousError) -> Self {
        Self::AnonymizedEntrant
    }
}

impl<E: Into<Error>> From<E> for StatusOrError<Error> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

impl IsNetworkError for Error {
    fn is_network_error(&self) -> bool {
        match self {
            Self::ChronoParse(_) => false,
            Self::Discord(_) => false,
            Self::Draft(e) => e.is_network_error(),
            Self::Event(_) => false,
            Self::OotrWeb(e) => e.is_network_error(),
            Self::ParseInt(_) => false,
            Self::Reqwest(e) => e.is_network_error(),
            Self::SeedData(e) => e.is_network_error(),
            Self::Sheets(e) => e.is_network_error(),
            Self::Sql(_) => false,
            Self::StartGG(e) => e.is_network_error(),
            Self::TimeFromLocal(_) => false,
            Self::Url(_) => false,
            Self::Wheel(e) => e.is_network_error(),
            Self::AnonymizedEntrant => false,
            Self::PoolPlaceholder => false,
            Self::UnknownTeam => false,
            Self::UnknownTeamStartGG(_) => false,
        }
    }
}

trait IntoIcsTzid {
    fn into_tzid(self) -> TzIDParam<'static>;
}

impl IntoIcsTzid for Utc {
    fn into_tzid(self) -> TzIDParam<'static> {
        TzIDParam::new("Etc/UTC")
    }
}

impl IntoIcsTzid for Tz {
    fn into_tzid(self) -> TzIDParam<'static> {
        TzIDParam::new(self.name())
    }
}

fn dtstamp(datetime: DateTime<Utc>) -> String {
    datetime.to_utc().format("%Y%m%dT%H%M%SZ").to_string()
}

fn dtstart<Z: TimeZone + IntoIcsTzid>(datetime: DateTime<Z>) -> DtStart<'static> {
    let mut dtstart = DtStart::new(datetime.naive_local().format("%Y%m%dT%H%M%S").to_string());
    dtstart.add(datetime.timezone().into_tzid());
    dtstart
}

fn dtend<Z: TimeZone + IntoIcsTzid>(datetime: DateTime<Z>) -> DtEnd<'static> {
    let mut dtend = DtEnd::new(datetime.naive_local().format("%Y%m%dT%H%M%S").to_string());
    dtend.add(datetime.timezone().into_tzid());
    dtend
}

async fn add_event_races(transaction: &mut Transaction<'_, Postgres>, discord_ctx: &DiscordCtx, http_client: &reqwest::Client, cal: &mut ICalendar<'_>, event: &event::Data<'_>) -> Result<(), Error> {
    let now = Utc::now();
    let mut latest_instantiated_weeklies: HashMap<Id<WeeklySchedules>, DateTime<Utc>> = HashMap::new();
    for race in Race::for_event(transaction, http_client, event).await?.into_iter() {
        for race_event in race.cal_events() {
            if let Some(start) = race_event.start() {
                let mut cal_event = ics::Event::new(format!("{}{}@midos.house",
                    race.id,
                    match race_event.kind {
                        EventKind::Normal => "",
                        EventKind::Async1 => "-1",
                        EventKind::Async2 => "-2",
                        EventKind::Async3 => "-3",
                    },
                ), dtstamp(now));
                let summary_prefix = match (&race.phase, &race.round) {
                    (Some(phase), Some(round)) => format!("{} {phase} {round}", event.short_name()),
                    (Some(phase), None) => format!("{} {phase}", event.short_name()),
                    (None, Some(round)) => format!("{} {round}", event.short_name()),
                    (None, None) => event.display_name.clone(),
                };
                let summary_prefix = match race.entrants {
                    Entrants::Open | Entrants::Count { .. } => summary_prefix,
                    Entrants::Named(ref entrants) => match race_event.kind {
                        EventKind::Normal => format!("{summary_prefix}: {entrants}"),
                        EventKind::Async1 | EventKind::Async2 | EventKind::Async3 => format!("{summary_prefix} (async): {entrants}"),
                    },
                    Entrants::Two([ref team1, ref team2]) => match race_event.kind {
                        EventKind::Normal => format!(
                            "{summary_prefix}: {} vs {}",
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                        EventKind::Async1 => format!(
                            "{summary_prefix} (async): {} vs {}",
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                        EventKind::Async2 => format!(
                            "{summary_prefix} (async): {} vs {}",
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                        EventKind::Async3 => unreachable!(),
                    },
                    Entrants::Three([ref team1, ref team2, ref team3]) => match race_event.kind {
                        EventKind::Normal => format!(
                            "{summary_prefix}: {} vs {} vs {}",
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team3.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                        EventKind::Async1 => format!(
                            "{summary_prefix} (async): {} vs {} vs {}",
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team3.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                        EventKind::Async2 => format!(
                            "{summary_prefix} (async): {} vs {} vs {}",
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team3.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                        EventKind::Async3 => format!(
                            "{summary_prefix} (async): {} vs {} vs {}",
                            team3.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team1.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                            team2.name(&mut *transaction, discord_ctx).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                        ),
                    },
                };
                cal_event.push(Summary::new(ics::escape_text(if let Some(game) = race.game {
                    format!("{summary_prefix}, game {game}")
                } else {
                    summary_prefix
                })));
                cal_event.push(dtstart(start));
                cal_event.push(dtend(race_event.end().filter(|_| !race_event.is_private_async_part() || race.cal_events().all(|event| event.end().is_some())).unwrap_or_else(|| start + event.series.default_race_duration())));
                let mut urls = Vec::default();
                for (language, video_url) in &race.video_urls {
                    urls.push((Cow::Owned(format!("{language} restream")), video_url.clone()));
                }
                if let Some(room) = race_event.room() {
                    urls.push((Cow::Borrowed("race room"), room.clone()));
                }
                if let Some(set_url) = race.startgg_set_url()? {
                    urls.push((Cow::Borrowed("start.gg set"), set_url));
                }
                if let Some((_, url)) = urls.get(0) {
                    cal_event.push(URL::new(url.to_string()));
                    urls.remove(0);
                    if !urls.is_empty() {
                        cal_event.push(Description::new(urls.into_iter().map(|(description, url)| format!("{description}: {url}")).join("\n")));
                    }
                } else {
                    cal_event.push(URL::new(uri!(base_uri(), event::info(event.series, &*event.event)).to_string()));
                }
                cal.add_event(cal_event);
                // Track latest instantiated weekly by schedule
                if let Some(round) = &race.round {
                    if round.ends_with(" Weekly") {
                        let weekly_name = round.trim_end_matches(" Weekly");
                        if let Some(schedule) = WeeklySchedule::for_round(&mut *transaction, race.series, &race.event, weekly_name).await? {
                            let entry = latest_instantiated_weeklies.entry(schedule.id).or_insert(start.to_utc());
                            if start.to_utc() > *entry {
                                *entry = start.to_utc();
                            }
                        }
                    }
                }
            }
        }
    }
    // Add recurring calendar events for active weekly schedules
    let weekly_schedules = WeeklySchedule::for_event(&mut *transaction, event.series, &event.event).await?;
    for schedule in weekly_schedules.into_iter().filter(|s| s.active) {
        // Find when to start the recurring event (after last instantiated race)
        let start = if let Some(&last) = latest_instantiated_weeklies.get(&schedule.id) {
            schedule.next_after(last)
        } else {
            schedule.next_after(now)
        };

        let mut cal_event = ics::Event::new(
            format!("weekly-{}@midos.house", schedule.id),
            dtstamp(now),
        );
        cal_event.push(Summary::new(ics::escape_text(format!("{} {} Weekly", event.short_name(), schedule.name))));
        cal_event.push(dtstart(start));
        cal_event.push(dtend(start + event.series.default_race_duration()));
        cal_event.push(RRule::new(format!("FREQ=DAILY;INTERVAL={}", schedule.frequency_days)));
        cal.add_event(cal_event);
    }
    Ok(())
}

#[rocket::get("/calendar")]
pub(crate) async fn index_help(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> PageResult {
    page(pool.begin().await?, &me, &uri, PageStyle::default(), "Calendar — Hyrule Town Hall", html! {
        p {
            : "A calendar of all races across all events can be found at ";
            code : uri!(base_uri(), index);
            : " — by pasting this link into most calendar apps' \"subscribe\" feature instead of downloading it, you can get automatic updates as races are scheduled:";
        }
        ul {
            li {
                : "In Google Calendar, select ";
                a(href = "https://calendar.google.com/calendar/u/0/r/settings/addbyurl") : "Add calendar → From URL";
            }
            li {
                : "In Apple Calendar, press ";
                kbd : "⌥";
                kbd : "⌘";
                kbd : "S";
                : " or select File → New Calendar Subscription";
            }
            li : "In Mozilla Thunderbird, select New Calendar → On the Network. Paste the link into the \"Location\" field and click \"Find Calendars\", then \"Properties\". Enable \"Read Only\" and click \"OK\", then \"Subscribe\".";
        }
        //p : "You can also find calendar links for individual events on their pages."; //TODO
    }).await
}

#[rocket::get("/calendar.ics")]
pub(crate) async fn index(discord_ctx: &State<RwFuture<DiscordCtx>>, pool: &State<PgPool>, http_client: &State<reqwest::Client>) -> Result<Response<ICalendar<'static>>, Error> {
    let mut transaction = pool.begin().await?;
    let mut cal = ICalendar::new("2.0", concat!("midos.house/", env!("CARGO_PKG_VERSION")));
    for row in sqlx::query!(r#"SELECT series AS "series: Series", event FROM events WHERE listed"#).fetch_all(&mut *transaction).await? {
        let event = event::Data::new(&mut transaction, row.series, row.event).await?.expect("event deleted during calendar load");
        add_event_races(&mut transaction, &*discord_ctx.read().await, http_client, &mut cal, &event).await?;
    }
    transaction.commit().await?;
    Ok(Response(cal))
}

#[rocket::get("/series/<series>/calendar.ics")]
pub(crate) async fn for_series(discord_ctx: &State<RwFuture<DiscordCtx>>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, series: Series) -> Result<Response<ICalendar<'static>>, Error> {
    let mut transaction = pool.begin().await?;
    let mut cal = ICalendar::new("2.0", concat!("midos.house/", env!("CARGO_PKG_VERSION")));
    for event in sqlx::query_scalar!(r#"SELECT event FROM events WHERE listed AND series = $1"#, series as _).fetch_all(&mut *transaction).await? {
        let event = event::Data::new(&mut transaction, series, event).await?.expect("event deleted during calendar load");
        add_event_races(&mut transaction, &*discord_ctx.read().await, http_client, &mut cal, &event).await?;
    }
    transaction.commit().await?;
    Ok(Response(cal))
}

#[rocket::get("/event/<series>/<event>/calendar.ics")]
pub(crate) async fn for_event(discord_ctx: &State<RwFuture<DiscordCtx>>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, series: Series, event: &str) -> Result<Response<ICalendar<'static>>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut cal = ICalendar::new("2.0", concat!("midos.house/", env!("CARGO_PKG_VERSION")));
    add_event_races(&mut transaction, &*discord_ctx.read().await, http_client, &mut cal, &event).await?;
    transaction.commit().await?;
    Ok(Response(cal))
}

pub(crate) async fn create_race_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: event::Data<'_>, ctx: Context<'_>, is_3p: bool) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Races, true).await?;
    let form = if me.is_some() {
        let teams = Team::for_event(&mut transaction, event.series, &event.event).await?;
        let mut team_data = Vec::with_capacity(teams.len());
        for team in teams {
            let name = if let Some(name) = team.name(&mut transaction).await? {
                name.into_owned()
            } else {
                format!("unnamed team ({})", English.join_str_opt(team.members(&mut transaction).await?).unwrap_or_else(|| format!("no members")))
            };
            team_data.push((team.id.to_string(), name));
        }
        team_data.sort_unstable_by(|(_, name1), (_, name2)| name1.cmp(name2));
        let phase_round_options = sqlx::query!("SELECT phase, round FROM phase_round_options WHERE series = $1 AND event = $2", event.series as _, &event.event).fetch_all(&mut *transaction).await?;
        let mut errors = ctx.errors().collect_vec();
        full_form(uri!(create_race_post(event.series, &*event.event)), csrf, html! {
            : form_field("team1", &mut errors, html! {
                label(for = "team1") {
                    @if let TeamConfig::Solo = event.team_config {
                        : "Player A:";
                    } else {
                        : "Team A:";
                    }
                }
                select(name = "team1") {
                    @for (id, name) in &team_data {
                        option(value = id, selected? = ctx.field_value("team1") == Some(id)) : name;
                    }
                }
            });
            : form_field("team2", &mut errors, html! {
                label(for = "team2") {
                    @if let TeamConfig::Solo = event.team_config {
                        : "Player B:";
                    } else {
                        : "Team B:";
                    }
                }
                select(name = "team2") {
                    @for (id, name) in &team_data {
                        option(value = id, selected? = ctx.field_value("team2") == Some(id)) : name;
                    }
                }
            });
            @if is_3p {
                : form_field("team3", &mut errors, html! {
                    label(for = "team3") {
                        @if let TeamConfig::Solo = event.team_config {
                            : "Player C:";
                        } else {
                            : "Team C:";
                        }
                    }
                    select(name = "team3") {
                        @for (id, name) in team_data {
                            option(value = id, selected? = ctx.field_value("team3") == Some(&id)) : name;
                        }
                    }
                });
            }
            @if phase_round_options.is_empty() {
                : form_field("phase", &mut errors, html! {
                    label(for = "phase") : "Phase:";
                    input(type = "text", name = "phase", value? = ctx.field_value("phase"));
                });
                : form_field("round", &mut errors, html! {
                    label(for = "round") : "Round:";
                    input(type = "text", name = "round", value? = ctx.field_value("round"));
                });
            } else {
                : form_field("phase_round", &mut errors, html! {
                    label(for = "phase_round") : "Round:";
                    select(name = "phase_round") {
                        @for row in phase_round_options {
                            @let option = format!("{} {}", row.phase, row.round);
                            option(value = &option, selected? = ctx.field_value("phase_round") == Some(&option)) : option;
                        }
                    }
                });
            }
            : form_field("game_count", &mut errors, html! {
                label(for = "game_count") : "Number of games in this match:";
                input(type = "number", min = "1", max = "255", name = "game_count", value = ctx.field_value("game_count").map_or_else(|| event.default_game_count.to_string(), |game_count| game_count.to_owned()));
                label(class = "help") {
                    : "(If some games end up not being necessary, use ";
                    code : "/delete-after";
                    : " in the scheduling thread to delete them.)";
                }
            });
        }, errors, "Create")
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(create_race(event.series, &*event.event, Some(NonZero::<u8>::new(if ctx.field_value("team3").is_some() { 3 } else { 2 }).unwrap()))))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to create a race.";
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("New Race — {}", event.display_name), html! {
        : header;
        h2 : "Create race";
        : form;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/races/new?<players>")]
pub(crate) async fn create_race(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String, players: Option<NonZero<u8>>) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let is_3p = match players.unwrap_or_else(|| NonZero::<u8>::new(2).unwrap()).get() {
        2 => false,
        3 => true,
        _ => return Err(StatusOrError::Status(Status::NotImplemented)),
    };
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(create_race_form(transaction, me, uri, csrf.as_ref(), event, Context::default(), is_3p).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct CreateRaceForm {
    #[field(default = String::new())]
    csrf: String,
    team1: Id<Teams>,
    team2: Id<Teams>,
    team3: Option<Id<Teams>>,
    #[field(default = String::new())]
    phase: String,
    #[field(default = String::new())]
    round: String,
    #[field(default = String::new())]
    phase_round: String,
    game_count: i16,
}

#[rocket::post("/event/<series>/<event>/races/new", data = "<form>")]
pub(crate) async fn create_race_post(pool: &State<PgPool>, discord_ctx: &State<RwFuture<DiscordCtx>>, http_client: &State<reqwest::Client>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, CreateRaceForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if !event.organizers(&mut transaction).await?.contains(&me) {
        form.context.push_error(form::Error::validation("You must be an organizer of this event to add a race."));
    }
    Ok(if let Some(ref value) = form.value {
        let team1 = Team::from_id(&mut transaction, value.team1).await?;
        if let Some(team1) = &team1 {
            if team1.series != event.series || team1.event != event.event {
                form.context.push_error(form::Error::validation("This team is for a different event.").with_name("team1"));
            }
        } else {
            form.context.push_error(form::Error::validation("There is no team with this ID.").with_name("team1"));
        }
        let team2 = Team::from_id(&mut transaction, value.team2).await?;
        if let Some(team2) = &team2 {
            if team2.series != event.series || team2.event != event.event {
                form.context.push_error(form::Error::validation("This team is for a different event.").with_name("team2"));
            }
        } else {
            form.context.push_error(form::Error::validation("There is no team with this ID.").with_name("team2"));
        }
        if team1 == team2 {
            form.context.push_error(form::Error::validation("Can't choose the same team twice.").with_name("team2"));
        }
        let team3 = if let Some(team3) = value.team3 {
            let team3 = Team::from_id(&mut transaction, team3).await?;
            if let Some(team3) = &team3 {
                if team3.series != event.series || team3.event != event.event {
                    form.context.push_error(form::Error::validation("This team is for a different event.").with_name("team3"));
                }
            } else {
                form.context.push_error(form::Error::validation("There is no team with this ID.").with_name("team3"));
            }
            if team1 == team3 || team2 == team3 {
                form.context.push_error(form::Error::validation("Can't choose the same team twice.").with_name("team3"));
            }
            team3
        } else {
            None
        };
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(create_race_form(transaction, Some(me), uri, csrf.as_ref(), event, form.context, team3.is_some()).await?)
        } else {
            let (phase, round) = if value.phase_round.is_empty() {
                (
                    (!value.phase.is_empty()).then(|| value.phase.clone()),
                    (!value.round.is_empty()).then(|| value.round.clone()),
                )
            } else {
                sqlx::query!("SELECT phase, round FROM phase_round_options WHERE series = $1 AND event = $2", event.series as _, &event.event).fetch_all(&mut *transaction).await?
                    .into_iter()
                    .find(|row| format!("{} {}", row.phase, row.round) == value.phase_round)
                    .map(|row| (Some(row.phase), Some(row.round)))
                    .unwrap_or_else(|| (None, Some(value.phase_round.clone())))
            };
            let [team1, team2] = [team1, team2].map(|team| team.expect("validated"));
            let draft = if team3.is_some() {
                None
            } else if let Some(draft_kind) = event.draft_kind() {
                Some(Draft::for_game1(&mut transaction, http_client, draft_kind, &event, phase.as_deref(), [&team1, &team2]).await?)
            } else {
                None
            };
            let mut scheduling_thread = None;
            for game in 1..=value.game_count {
                let mut race = Race {
                    id: Id::<Races>::new(&mut transaction).await?,
                    series: event.series,
                    event: event.event.to_string(),
                    source: Source::Manual,
                    entrants: if let Some(ref team3) = team3 {
                        Entrants::Three([
                            Entrant::MidosHouseTeam(team1.clone()),
                            Entrant::MidosHouseTeam(team2.clone()),
                            Entrant::MidosHouseTeam(team3.clone()),
                        ])
                    } else {
                        Entrants::Two([
                            Entrant::MidosHouseTeam(team1.clone()),
                            Entrant::MidosHouseTeam(team2.clone()),
                        ])
                    },
                    phase: phase.clone(),
                    round: round.clone(),
                    game: (value.game_count > 1).then_some(game),
                    schedule: RaceSchedule::Unscheduled,
                    schedule_updated_at: None,
                    fpa_invoked: false,
                    breaks_used: false,
                    draft: draft.clone(),
                    seed: seed::Data::default(),
                    video_urls: HashMap::default(),
                    restreamers: HashMap::default(),
                    last_edited_by: None,
                    last_edited_at: None,
                    ignored: false,
                    schedule_locked: false,
                    notified: false,
                    async_notified_1: false,
                    async_notified_2: false,
                    async_notified_3: false,
                    discord_scheduled_event_id: None,
                    volunteer_request_sent: false,
                    volunteer_request_message_id: None,
                    scheduling_thread,
                };
                if game == 1 {
                    transaction = discord_bot::create_scheduling_thread(&*discord_ctx.read().await, transaction, &mut race, value.game_count).await?;
                    scheduling_thread = race.scheduling_thread;
                }
                race.save(&mut transaction).await?;
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(event::races(event.series, &*event.event))))
        }
    } else {
        let is_3p = form.context.field_value("team3").is_some();
        RedirectOrContent::Content(create_race_form(transaction, Some(me), uri, csrf.as_ref(), event, form.context, is_3p).await?)
    })
}

pub(crate) struct RaceTableOptions<'a> {
    pub(crate) game_count: bool,
    pub(crate) show_multistreams: bool,
    pub(crate) can_create: bool,
    pub(crate) can_edit: bool,
    pub(crate) show_restream_consent: bool,
    pub(crate) challonge_import_ctx: Option<Context<'a>>,
}

pub(crate) async fn race_table(
    transaction: &mut Transaction<'_, Postgres>,
    discord_ctx: &DiscordCtx,
    http_client: &reqwest::Client,
    uri: &Origin<'_>,
    event: Option<&event::Data<'_>>,
    options: RaceTableOptions<'_>,
    races: &[Race],
    user: Option<&User>,
    approved_role_binding_ids: Option<&[Id<RoleBindings>]>,
) -> Result<RawHtml<String>, Error> {
    let mut event_cache = HashMap::new();
    if let Some(event) = event {
        event_cache.insert((event.series, &*event.event), event.clone());
    }
    let phase_round_options = if_chain! {
        if let Some(event) = event;
        if options.challonge_import_ctx.is_some();
        then {
            Some(sqlx::query!("SELECT phase, round FROM phase_round_options WHERE series = $1 AND event = $2", event.series as _, &event.event).fetch_all(&mut **transaction).await?)
        } else {
            None
        }
    };
    let has_games = options.game_count || races.iter().any(|race| race.game.is_some());
    let has_seeds = 'has_seeds: {
        for race in races {
            if race.show_seed() {
                if race.seed.file_hash.is_some() || race.seed.files.is_some() {
                    break 'has_seeds true
                }
            } else {
                let event = match event_cache.entry((race.series, &race.event)) {
                    hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    hash_map::Entry::Vacant(entry) => entry.insert(race.event(&mut *transaction).await?),
                };
                if event.single_settings.is_none() && race.single_settings(&mut *transaction).await?.is_some() {
                    break 'has_seeds true
                }
            }
        }
        false
    };
    
    // Check if any race has custom settings that would show in a settings column
    let has_settings = 'has_settings: {
        for race in races {
            if !race.show_seed() {
                // Check for Crosskeys2025 races
                if let Some(racetime_bot::Goal::Crosskeys2025) = racetime_bot::Goal::for_event(race.series, &race.event) {
                    break 'has_settings true
                }
            }
        }
        false
    };
    let has_buttons = options.can_create || options.can_edit;
    let now = Utc::now();
    Ok(html! {
        table {
            thead {
                tr {
                    th : "Start";
                    @if event.is_none() {
                        th : "Event";
                    }
                    @if phase_round_options.as_ref().is_some_and(|phase_round_options| phase_round_options.is_empty()) {
                        th : "Phase";
                    }
                    th : "Round";
                    @if has_games {
                        @if options.game_count {
                            th : "Best of";
                        } else {
                            th : "Game";
                        }
                    }
                    th(colspan = "6") : "Entrants";
                    th : "Links";
                    @if has_seeds {
                        th : "Seed";
                    }
                    @if !has_seeds && has_settings {
                        th : "Settings";
                    }
                    @if options.show_restream_consent {
                        th : "Restream Consent";
                    }
                    @if has_buttons {
                        th {
                            @if options.can_create {
                                @if let Some(event) = event {
                                    @match event.match_source() {
                                        MatchSource::Manual | MatchSource::Challonge { .. } => a(class = "clean_button", href = uri!(create_race(races[0].series, &*races[0].event, _)), target = "_blank") : "New Race";
                                        //MatchSource::Challonge { .. } => a(class = "clean_button", href = uri!(import_races(races[0].series, &*races[0].event))) : "Import"; // disabled due to Challonge pagination bug
                                        MatchSource::League => {}
                                        MatchSource::StartGG(_) => @if !event.auto_import {
                                            a(class = "clean_button", href = uri!(import_races(races[0].series, &*races[0].event))) : "Import";
                                        }
                                    }
                                }
                            }
                        }
                    }
                    th : "Volunteers";
                }
            }
            tbody {
                @for race in races {
                    tr {
                        @let (event, show_event) = if let Some(event) = event {
                            (event, false)
                        } else {
                            (&*match event_cache.entry((race.series, &race.event)) {
                                hash_map::Entry::Occupied(entry) => entry.into_mut(),
                                hash_map::Entry::Vacant(entry) => entry.insert(race.event(&mut *transaction).await?),
                            }, true)
                        };
                        td {
                            @match race.schedule {
                                RaceSchedule::Unscheduled => {}
                                RaceSchedule::Live { start, .. } => : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                RaceSchedule::Async { .. } => : "(async)";
                            }
                        }
                        @if show_event {
                            td(class = "small-table-content") {
                                a(href = uri!(event::info(event.series, &*event.event))) : event.short_name();
                            }
                            td(class = "large-table-content") : event;
                        }
                        @if let (Some(ctx), Some(phase_round_options), Source::Challonge { id: challonge_id }) = (&options.challonge_import_ctx, &phase_round_options, &race.source) {
                            @if phase_round_options.is_empty() {
                                : form_table_cell(&format!("phase[{challonge_id}]"), &mut Vec::default(), html! {
                                    input(type = "text", name = format!("phase[{challonge_id}]"), value? = ctx.field_value(&*format!("phase[{challonge_id}]")));
                                });
                                : form_table_cell(&format!("round[{challonge_id}]"), &mut Vec::default(), html! {
                                    input(type = "text", name = format!("round[{challonge_id}]"), value? = ctx.field_value(&*format!("round[{challonge_id}]")));
                                });
                            } else {
                                : form_table_cell(&format!("phase_round[{challonge_id}]"), &mut Vec::default(), html! {
                                    select(name = format!("phase_round[{challonge_id}]")) {
                                        @for row in phase_round_options {
                                            @let option = format!("{} {}", row.phase, row.round);
                                            option(value = &option, selected? = ctx.field_value(&*format!("phase_round[{challonge_id}]")) == Some(&option)) : option;
                                        }
                                    }
                                });
                            }
                        } else {
                            td {
                                : race.phase;
                                : " ";
                                : race.round;
                            }
                        }
                        @if has_games {
                            @if let (Some(ctx), Source::Challonge { id: challonge_id }) = (&options.challonge_import_ctx, &race.source) {
                                : form_table_cell(&format!("game_count[{challonge_id}]"), &mut Vec::default(), html! {
                                    input(type = "number", min = "1", max = "255", name = format!("game_count[{challonge_id}]"), value = ctx.field_value(&*format!("game_count[{challonge_id}]")).map_or_else(|| event.default_game_count.to_string(), |game_count| game_count.to_owned()));
                                });
                            } else {
                                td {
                                    @if let Some(game) = race.game {
                                        : game;
                                    }
                                }
                            }
                        }
                        @match race.entrants {
                            Entrants::Open => td(colspan = "6") : "(open)";
                            Entrants::Count { total, finished } => td(colspan = "6") {
                                : total;
                                : " (";
                                : finished;
                                : " finishers)";
                            }
                            Entrants::Named(ref entrants) => td(colspan = "6") {
                                bdi : entrants;
                            }
                            Entrants::Two([ref team1, ref team2]) => {
                                td(class = "vs1", colspan = "3") {
                                    : team1.to_html(&mut *transaction, discord_ctx, false).await?;
                                    @if let RaceSchedule::Async { start1: Some(start), .. } = race.schedule {
                                        br;
                                        small {
                                            : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                        }
                                    }
                                }
                                td(class = "vs2", colspan = "3") {
                                    : team2.to_html(&mut *transaction, discord_ctx, false).await?;
                                    @if let RaceSchedule::Async { start2: Some(start), .. } = race.schedule {
                                        br;
                                        small {
                                            : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                        }
                                    }
                                }
                            }
                            Entrants::Three([ref team1, ref team2, ref team3]) => {
                                td(colspan = "2") {
                                    : team1.to_html(&mut *transaction, discord_ctx, false).await?;
                                    @if let RaceSchedule::Async { start1: Some(start), .. } = race.schedule {
                                        br;
                                        small {
                                            : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                        }
                                    }
                                }
                                td(colspan = "2") {
                                    : team2.to_html(&mut *transaction, discord_ctx, false).await?;
                                    @if let RaceSchedule::Async { start2: Some(start), .. } = race.schedule {
                                        br;
                                        small {
                                            : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                        }
                                    }
                                }
                                td(colspan = "2") {
                                    : team3.to_html(&mut *transaction, discord_ctx, false).await?;
                                    @if let RaceSchedule::Async { start3: Some(start), .. } = race.schedule {
                                        br;
                                        small {
                                            : format_datetime(start, DateTimeFormat { long: false, running_text: false });
                                        }
                                    }
                                }
                            }
                        }
                        td {
                            div(class = "favicon-container") {
                                @for (language, video_url) in &race.video_urls {
                                    a(class = "favicon", title = format!("{language} restream"), href = video_url.to_string(), target = "_blank") : favicon(video_url);
                                }
                                @if options.show_multistreams && race.video_urls.is_empty() {
                                    @if let Some(multistream_url) = race.multistream_url(&mut *transaction, http_client, &event).await? {
                                        a(class = "favicon", title = "multistream", href = multistream_url.to_string(), target = "_blank") : favicon(&multistream_url);
                                    }
                                }
                                @for (user, video_url) in race.player_video_urls(&mut *transaction).await? {
                                    a(class = "favicon", title = format!("{user}'s vod"), href = video_url.to_string(), target = "_blank") : favicon(&video_url);
                                }
                                @if let Some(startgg_url) = race.startgg_set_url()? {
                                    a(class = "favicon", title = "start.gg set", href = startgg_url.to_string(), target = "_blank") : favicon(&startgg_url);
                                }
                                @for room in race.rooms() {
                                    a(class = "favicon", title = "race room", href = room.to_string(), target = "_blank") : favicon(&room);
                                }
                                // Volunteer button for upcoming live races
                                @let is_upcoming_live = match race.schedule {
                                    RaceSchedule::Live { end, .. } => end.is_none(),
                                    _ => false,
                                };
                                @if is_upcoming_live {
                                                                    @let scheduled = match race.schedule {
                                    RaceSchedule::Unscheduled => false,
                                    RaceSchedule::Live { end, .. } => end.is_none_or(|end_time| end_time > Utc::now()),
                                    RaceSchedule::Async { .. } => false, // asyncs not eligible
                                };
                                @let all_teams_consented = race.teams_opt().map_or(true, |mut teams| teams.all(|team| team.restream_consent));
                                @if scheduled && all_teams_consented {
                                        @if let Some(user) = user {
                                            @let is_organizer = event.organizers(&mut *transaction).await.ok().map_or(false, |orgs| orgs.contains(user));
                                            @let is_event_restreamer = event.restreamers(&mut *transaction).await.ok().map_or(false, |rest| rest.contains(user));
                                            @let is_game_restreamer = if is_event_restreamer { false } else {
                                                match crate::game::Game::from_series(&mut *transaction, race.series).await {
                                                    Ok(Some(game)) => game.is_restreamer_any_language(&mut *transaction, user).await.unwrap_or(false),
                                                    _ => false,
                                                }
                                            };
                                            @let is_restreamer = is_event_restreamer || is_game_restreamer;
                                            @if is_organizer || is_restreamer {
                                                a(class = "clean_button", href = uri!(crate::event::roles::match_signup_page_get(race.series, &race.event, race.id, _))) : "Manage Volunteers";
                                            } else if let Some(approved_roles) = approved_role_binding_ids {
                                                @if !approved_roles.is_empty() {
                                                    a(class = "clean_button", href = uri!(crate::event::roles::match_signup_page_get(race.series, &race.event, race.id, _))) : "Volunteer";
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        @if has_seeds {
                            td {
                                @if race.show_seed() {
                                    @let game_id = event.game(&mut *transaction).await?.map(|g| g.id).unwrap_or(1);
                                    // Only show "Add Hash" for games that support hashes (not TWWR)
                                    @let supports_hash = !matches!(event.rando_version, Some(racetime_bot::VersionedBranch::Tww { .. }));
                                    @let add_hash_url = options.can_edit.then(|| uri!(cal::add_file_hash(race.series, &*race.event, race.id))).filter(|_| supports_hash);
                                    : seed::table_cell(now, &race.seed, true, add_hash_url, &mut *transaction, game_id).await?;
                                } else {
                                    // hide seed if unfinished async
                                    //TODO show to the team that played the 1st async half
                                    @if event.game(&mut *transaction).await?.map(|g| g.name == "ootr").unwrap_or(false) && event.single_settings.is_none() && race.single_settings(&mut *transaction).await?.is_some() {
                                        a(class = "clean_button", href = uri!(practice_seed(event.series, &*event.event, race.id))) {
                                            : favicon(&Url::parse("https://ootrandomizer.com/").unwrap()); //TODO adjust based on seed host
                                            : "Practice";
                                        }
                                    }
                                }
                                
                                // Add Settings link for races with custom options
                                @if race.show_seed() || race.is_ended() || matches!(race.schedule, RaceSchedule::Unscheduled | RaceSchedule::Async { .. } | RaceSchedule::Live { .. }) {
                                    @if let Some(racetime_bot::Goal::Crosskeys2025) = racetime_bot::Goal::for_event(race.series, &race.event) {
                                        @if let Ok(crosskeys_options) = racetime_bot::CrosskeysRaceOptions::for_race_with_transaction(&mut *transaction, race).await {
                                            span(class = "settings-link", data_tooltip = format!("Seed Settings: {}\nRace Rules: {}", crosskeys_options.as_seed_options_str(), crosskeys_options.as_race_options_str_no_delay())) {
                                                : " -Hover for Settings- ";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        @if !has_seeds && has_settings {
                            td {
                                // Check for Crosskeys2025 races
                                @if let Some(racetime_bot::Goal::Crosskeys2025) = racetime_bot::Goal::for_event(race.series, &race.event) {
                                    @if let Ok(crosskeys_options) = racetime_bot::CrosskeysRaceOptions::for_race_with_transaction(&mut *transaction, race).await {
                                        span(class = "settings-link", data_tooltip = format!("Seed Settings: {}\nRace Rules: {}", crosskeys_options.as_seed_options_str(), crosskeys_options.as_race_options_str_no_delay())) {
                                            : "-Hover for Settings-";
                                        }
                                    }
                                }
                            }
                        }
                        @if options.show_restream_consent {
                            td {
                                @if let Some(mut teams) = race.teams_opt() {
                                    @if teams.all(|team| team.restream_consent) {
                                        : "✓";
                                    } else {
                                        : "x";
                                    }
                                }
                            }
                        }
                        @if has_buttons {
                            td {
                                @if let Some(user) = user {
                                    @let is_admin = u64::from(user.id) == User::GLOBAL_ADMIN_USER_IDS[0];
                                    @let is_organizer = event.organizers(&mut *transaction).await.ok().map_or(false, |orgs| orgs.contains(user));
                                    @if is_admin || is_organizer {
                                        a(class = "clean_button", href = uri!(crate::cal::edit_race(race.series, &race.event, race.id, Some(uri)))) : "Edit";
                                    } else if options.can_edit {
                                        @match race.schedule {
                                            RaceSchedule::Live { .. } => {
                                                @let all_teams_consented = race.teams_opt().map_or(true, |mut teams| teams.all(|team| team.restream_consent));
                                                @if all_teams_consented {
                                                    a(class = "clean_button", href = uri!(crate::cal::edit_race(race.series, &race.event, race.id, Some(uri)))) : "Edit Restreams";
                                                }
                                            }
                                            RaceSchedule::Async { .. } | RaceSchedule::Unscheduled => {
                                                a(class = "clean_button", href = uri!(crate::cal::edit_race(race.series, &race.event, race.id, Some(uri)))) : "Edit Restreams";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        td {
                            @if race.is_ended() {
                                @match race.schedule {
                                    RaceSchedule::Live { .. } => {
                                        @let all_teams_consented = race.teams_opt().map_or(true, |mut teams| teams.all(|team| team.restream_consent));
                                        @if all_teams_consented {
                                            @let signups = Signup::for_race(&mut *transaction, race.id).await?;
                                            @let confirmed_signups = signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Confirmed)).collect::<Vec<_>>();

                                            @if !confirmed_signups.is_empty() {
                                                @let role_bindings = event::roles::RoleBinding::for_event(&mut *transaction, race.series, &race.event).await?;

                                                // Group bindings by language and check which languages have volunteers
                                                @let languages_with_volunteers = role_bindings.iter()
                                                    .filter(|binding| confirmed_signups.iter().any(|s| s.role_binding_id == binding.id))
                                                    .map(|binding| binding.language)
                                                    .collect::<HashSet<_>>();

                                                // Pre-fetch all users into a HashMap for use in tooltips
                                                @let user_cache = {
                                                    let mut cache = HashMap::new();
                                                    let unique_user_ids = confirmed_signups.iter().map(|s| s.user_id).collect::<HashSet<_>>();
                                                    for user_id in unique_user_ids {
                                                        let user = User::from_id(&mut **transaction, user_id).await.ok().flatten();
                                                        cache.insert(user_id, user);
                                                    }
                                                    cache
                                                };

                                                @if languages_with_volunteers.len() == 1 {
                                                    // Single language: show volunteers with language abbreviation
                                                    @let language = *languages_with_volunteers.iter().next().unwrap();
                                                    @for binding in role_bindings {
                                                        @if binding.language == language {
                                                            @let binding_signups = confirmed_signups.iter().filter(|s| s.role_binding_id == binding.id).collect::<Vec<_>>();
                                                            @if !binding_signups.is_empty() {
                                                                : binding.role_type_name;
                                                                : " (";
                                                                : language.short_code();
                                                                : "): ";
                                                                @for (i, signup) in binding_signups.iter().enumerate() {
                                                                    @if i > 0 { : ", "; }
                                                                    : user_cache.get(&signup.user_id)
                                                                        .and_then(|opt| opt.as_ref())
                                                                        .map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                                                }
                                                                br;
                                                            }
                                                        }
                                                    }
                                                } else if languages_with_volunteers.len() > 1 {
                                                    // Multiple languages: show languages as hoverable links
                                                    @for (lang_idx, language) in languages_with_volunteers.iter().enumerate() {
                                                        @if lang_idx > 0 { : ", "; }

                                                        // Build tooltip for this language using pre-fetched users
                                                        @let tooltip_content = role_bindings.iter()
                                                            .filter(|b| b.language == *language)
                                                            .filter_map(|binding| {
                                                                let binding_signups = confirmed_signups.iter().filter(|s| s.role_binding_id == binding.id).collect::<Vec<_>>();
                                                                if binding_signups.is_empty() {
                                                                    None
                                                                } else {
                                                                    let users = binding_signups.iter()
                                                                        .map(|signup| user_cache.get(&signup.user_id)
                                                                            .and_then(|opt| opt.as_ref())
                                                                            .map_or_else(|| signup.user_id.to_string(), |u| u.to_string()))
                                                                        .collect::<Vec<_>>()
                                                                        .join(", ");
                                                                    Some(format!("{}: {}", binding.role_type_name, users))
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()
                                                            .join("\n");

                                                        span(class = "settings-link", data_tooltip = tooltip_content) {
                                                            : language.to_string();
                                                        }
                                                    }
                                                }
                                            }
                                            // If no confirmed volunteers, show nothing (empty)
                                        } else {
                                            : "no restream";
                                        }
                                    }
                                    RaceSchedule::Async { .. } => {
                                        : "no restream";
                                    }
                                    RaceSchedule::Unscheduled => {
                                        // Unscheduled live races: show nothing (empty)
                                    }
                                }
                            } else {
                                @match race.schedule {
                                    RaceSchedule::Live { .. } => {
                                        @let all_teams_consented = race.teams_opt().map_or(true, |mut teams| teams.all(|team| team.restream_consent));
                                        @if all_teams_consented {
                                            @let signups = Signup::for_race(&mut *transaction, race.id).await?;
                                            @let pending_signups = signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Pending)).collect::<Vec<_>>();
                                            @let confirmed_signups = signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Confirmed)).collect::<Vec<_>>();

                                            @if !pending_signups.is_empty() && confirmed_signups.is_empty() {
                                                : "pending";
                                            } else if !confirmed_signups.is_empty() {
                                                @let role_bindings = event::roles::RoleBinding::for_event(&mut *transaction, race.series, &race.event).await?;

                                                // Group bindings by language and check which languages have volunteers
                                                @let languages_with_volunteers = role_bindings.iter()
                                                    .filter(|binding| confirmed_signups.iter().any(|s| s.role_binding_id == binding.id))
                                                    .map(|binding| binding.language)
                                                    .collect::<HashSet<_>>();

                                                // Pre-fetch all users into a HashMap for use in tooltips
                                                @let user_cache = {
                                                    let mut cache = HashMap::new();
                                                    let unique_user_ids = confirmed_signups.iter().map(|s| s.user_id).collect::<HashSet<_>>();
                                                    for user_id in unique_user_ids {
                                                        let user = User::from_id(&mut **transaction, user_id).await.ok().flatten();
                                                        cache.insert(user_id, user);
                                                    }
                                                    cache
                                                };

                                                @if languages_with_volunteers.len() == 1 {
                                                    // Single language: show volunteers with language abbreviation
                                                    @let language = *languages_with_volunteers.iter().next().unwrap();
                                                    @for binding in role_bindings {
                                                        @if binding.language == language {
                                                            @let binding_signups = confirmed_signups.iter().filter(|s| s.role_binding_id == binding.id).collect::<Vec<_>>();
                                                            @if !binding_signups.is_empty() {
                                                                : binding.role_type_name;
                                                                : " (";
                                                                : language.short_code();
                                                                : "): ";
                                                                @for (i, signup) in binding_signups.iter().enumerate() {
                                                                    @if i > 0 { : ", "; }
                                                                    : user_cache.get(&signup.user_id)
                                                                        .and_then(|opt| opt.as_ref())
                                                                        .map_or_else(|| signup.user_id.to_string(), |u| u.to_string());
                                                                }
                                                                br;
                                                            }
                                                        }
                                                    }
                                                } else if languages_with_volunteers.len() > 1 {
                                                    // Multiple languages: show languages as hoverable links
                                                    @for (lang_idx, language) in languages_with_volunteers.iter().enumerate() {
                                                        @if lang_idx > 0 { : ", "; }

                                                        // Build tooltip for this language using pre-fetched users
                                                        @let tooltip_content = role_bindings.iter()
                                                            .filter(|b| b.language == *language)
                                                            .filter_map(|binding| {
                                                                let binding_signups = confirmed_signups.iter().filter(|s| s.role_binding_id == binding.id).collect::<Vec<_>>();
                                                                if binding_signups.is_empty() {
                                                                    None
                                                                } else {
                                                                    let users = binding_signups.iter()
                                                                        .map(|signup| user_cache.get(&signup.user_id)
                                                                            .and_then(|opt| opt.as_ref())
                                                                            .map_or_else(|| signup.user_id.to_string(), |u| u.to_string()))
                                                                        .collect::<Vec<_>>()
                                                                        .join(", ");
                                                                    Some(format!("{}: {}", binding.role_type_name, users))
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()
                                                            .join("\n");

                                                        span(class = "settings-link", data_tooltip = tooltip_content) {
                                                            : language.to_string();
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            : "no restream";
                                        }
                                    }
                                    RaceSchedule::Async { .. } => {
                                        : "no restream";
                                    }
                                    RaceSchedule::Unscheduled => {
                                        // Unscheduled live races: show nothing (empty)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

pub(crate) async fn import_races_form(mut transaction: Transaction<'_, Postgres>, http_client: &reqwest::Client, discord_ctx: &DiscordCtx, config: &Config, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: event::Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Races, true).await?;
    let form = match event.match_source() {
        MatchSource::Manual => html! {
            article {
                p : "This event has no source for importing races configured.";
            }
        },
        MatchSource::Challonge { community, tournament } => if me.is_some() {
            let (races, skips) = challonge::races_to_import(&mut transaction, http_client, config, &event, community, tournament).await?;
            if races.is_empty() {
                html! {
                    article {
                        @if skips.is_empty() {
                            p : "Challonge did not list any matches for this event.";
                        } else {
                            p : "There are no races to import. The following matches have been skipped:";
                            table {
                                thead {
                                    tr {
                                        th : "Challonge match ID";
                                        th : "Reason";
                                    }
                                }
                                tbody {
                                    @for (set_id, reason) in skips {
                                        tr {
                                            td : set_id;
                                            td : reason.to_string();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                let table = race_table(&mut transaction, discord_ctx, http_client, &uri, Some(&event), RaceTableOptions { game_count: true, show_multistreams: false, can_create: false, can_edit: false, show_restream_consent: false, challonge_import_ctx: Some(ctx.clone()) }, &races, None, None).await?;
                let errors = ctx.errors().collect_vec();
                full_form(uri!(import_races_post(event.series, &*event.event)), csrf, html! {
                    p : "The following races will be imported:";
                    : table;
                    p {
                        : "If some games of a multi-game match end up not being necessary, use ";
                        code : "/delete-after";
                        : " in the scheduling thread to delete them.";
                    }
                }, errors, "Import")
            }
        } else {
            html! {
                article {
                    p {
                        a(href = uri!(auth::login(Some(uri!(import_races(event.series, &*event.event)))))) : "Sign in or create a Hyrule Town Hall account";
                        : " to import races.";
                    }
                }
            }
        },
        MatchSource::League => html! {
            article {
                p {
                    : "Races for this event are automatically imported from ";
                    a(href = "https://league.ootrandomizer.com/") : "league.ootrandomizer.com";
                    : ".";
                }
            }
        },
        MatchSource::StartGG(event_slug) => if event.auto_import {
            html! {
                article {
                    p : "Races for this event are imported automatically every 5 minutes.";
                }
            }
        } else if me.is_some() {
            let (races, skips) = startgg::races_to_import(&mut transaction, http_client, config, &event, event_slug).await?;
            if races.is_empty() {
                html! {
                    article {
                        @if skips.is_empty() {
                            p : "start.gg did not list any matches for this event.";
                        } else {
                            p : "There are no races to import. The following matches have been skipped:";
                            table {
                                thead {
                                    tr {
                                        th : "start.gg match ID";
                                        th : "Reason";
                                    }
                                }
                                tbody {
                                    @for (set_id, reason) in skips {
                                        tr {
                                            td : set_id.0;
                                            td : reason.to_string();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                let table = race_table(&mut transaction, discord_ctx, http_client, &uri, Some(&event), RaceTableOptions { game_count: true, show_multistreams: false, can_create: false, can_edit: false, show_restream_consent: false, challonge_import_ctx: None }, &races, None, None).await?;
                let errors = ctx.errors().collect_vec();
                full_form(uri!(import_races_post(event.series, &*event.event)), csrf, html! {
                    p : "The following races will be imported:";
                    : table;
                }, errors, "Import")
            }
        } else {
            html! {
                article {
                    p {
                        a(href = uri!(auth::login(Some(uri!(import_races(event.series, &*event.event)))))) : "Sign in or create a Hyrule Town Hall account";
                        : " to import races.";
                    }
                }
            }
        },
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Import Races — {}", event.display_name), html! {
        : header;
        h2 : "Import races";
        : form;
    }).await?)
}

#[rocket::get("/event/<series>/<event>/races/import")]
pub(crate) async fn import_races(config: &State<Config>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: String) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(import_races_form(transaction, http_client, &*discord_ctx.read().await, config, me, uri, csrf.as_ref(), event, Context::default()).await?)
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct ImportRacesForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = HashMap::new())]
    phase: HashMap<String, String>,
    #[field(default = HashMap::new())]
    round: HashMap<String, String>,
    #[field(default = HashMap::new())]
    phase_round: HashMap<String, String>,
    #[field(default = HashMap::new())]
    game_count: HashMap<String, i16>,
}

#[rocket::post("/event/<series>/<event>/races/import", data = "<form>")]
pub(crate) async fn import_races_post(discord_ctx: &State<RwFuture<DiscordCtx>>, config: &State<Config>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, ImportRacesForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if !event.organizers(&mut transaction).await?.contains(&me) {
        form.context.push_error(form::Error::validation("You must be an organizer to import races."));
    }
    Ok(if let Some(ref value) = form.value {
        let races = match event.match_source() {
            MatchSource::Manual => {
                form.context.push_error(form::Error::validation("This event has no source for importing races configured."));
                Vec::default()
            }
            MatchSource::Challonge { community, tournament } => {
                let (mut races, skips) = challonge::races_to_import(&mut transaction, http_client, config, &event, community, tournament).await?;
                if races.is_empty() {
                    if skips.is_empty() {
                        form.context.push_error(form::Error::validation("Challonge did not list any matches for this event."));
                    } else {
                        form.context.push_error(form::Error::validation("There are no races to import. Some matches have been skipped."));
                    }
                }
                for race in &mut races {
                    let Source::Challonge { ref id } = race.source else { unreachable!("received non-Challonge race from challonge::races_to_import") };
                    (race.phase, race.round) = if value.phase_round.get(id).is_none_or(|phase_round| phase_round.is_empty()) {
                        (
                            value.phase.get(id).filter(|phase| !phase.is_empty()).map(|phase| phase.clone()),
                            value.round.get(id).filter(|round| !round.is_empty()).map(|round| round.clone()),
                        )
                    } else {
                        sqlx::query!("SELECT phase, round FROM phase_round_options WHERE series = $1 AND event = $2", event.series as _, &event.event).fetch_all(&mut *transaction).await?
                            .into_iter()
                            .find(|row| format!("{} {}", row.phase, row.round) == value.phase_round[id])
                            .map(|row| (Some(row.phase), Some(row.round)))
                            .unwrap_or_else(|| (None, Some(value.phase_round[id].clone())))
                    };
                    race.game = value.game_count.get(id).copied();
                }
                races
            }
            MatchSource::League => {
                form.context.push_error(form::Error::validation("Races for this event are automatically imported from league.ootrandomizer.com."));
                Vec::default()
            }
            MatchSource::StartGG(event_slug) => {
                let (races, skips) = startgg::races_to_import(&mut transaction, http_client, config, &event, event_slug).await?;
                if races.is_empty() {
                    if skips.is_empty() {
                        form.context.push_error(form::Error::validation("start.gg did not list any matches for this event."));
                    } else {
                        form.context.push_error(form::Error::validation("There are no races to import. Some matches have been skipped."));
                    }
                }
                races
            }
        };
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(import_races_form(transaction, http_client, &*discord_ctx.read().await, config, Some(me), uri, csrf.as_ref(), event, form.context).await?)
        } else {
            for race in races {
                transaction = import_race(transaction, &*discord_ctx.read().await, race).await?;
            }
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(event::races(event.series, &*event.event))))
        }
    } else {
        RedirectOrContent::Content(import_races_form(transaction, http_client, &*discord_ctx.read().await, config, Some(me), uri, csrf.as_ref(), event, form.context).await?)
    })
}

async fn import_race<'a>(mut transaction: Transaction<'a, Postgres>, discord_ctx: &DiscordCtx, race: Race) -> Result<Transaction<'a, Postgres>, event::Error> {
    let game_count = race.game.unwrap_or(1);
    let mut scheduling_thread = None;
    for game in 1..=game_count {
        let mut race = Race {
            id: Id::<Races>::new(&mut transaction).await?,
            game: (game_count > 1).then_some(game),
            draft: race.draft.as_ref().filter(|_| game == 1).cloned(),
            scheduling_thread,
            ..race.clone()
        };
        if game == 1 {
            transaction = discord_bot::create_scheduling_thread(discord_ctx, transaction, &mut race, game_count).await?;
            scheduling_thread = race.scheduling_thread;
        }
        race.save(&mut transaction).await?;
    }
    Ok(transaction)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AutoImportError {
    #[error(transparent)] Calendar(#[from] Error),
    #[error(transparent)] Discord(#[from] discord_bot::Error),
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] EventData(#[from] event::DataError),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] StartGG(#[from] startgg::Error),
    #[error(transparent)] Url(#[from] url::ParseError),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("HTTP error{}: {}", if let Some(url) = .0.url() { format!(" at {url}") } else { String::default() }, .0)]
    Http(#[from] reqwest::Error),
}

impl IsNetworkError for AutoImportError {
    fn is_network_error(&self) -> bool {
        match self {
            Self::Calendar(e) => e.is_network_error(),
            Self::Discord(_) => false,
            Self::Event(e) => e.is_network_error(),
            Self::EventData(_) => false,
            Self::Serenity(_) => false,
            Self::Sql(_) => false,
            Self::StartGG(e) => e.is_network_error(),
            Self::Url(_) => false,
            Self::Wheel(e) => e.is_network_error(),
            Self::Http(e) => e.is_network_error(),
        }
    }
}

async fn auto_import_races_inner(db_pool: PgPool, http_client: reqwest::Client, config: Config, mut shutdown: rocket::Shutdown, discord_ctx: RwFuture<DiscordCtx>, new_room_lock: Arc<Mutex<()>>) -> Result<(), AutoImportError> {
    loop {
        lock!(new_room_lock = new_room_lock; {
            let mut transaction = db_pool.begin().await?;
            for row in sqlx::query!(r#"SELECT series AS "series: Series", event FROM events WHERE end_time IS NULL OR end_time > NOW()"#).fetch_all(&mut *transaction).await? {
                let event = event::Data::new(&mut transaction, row.series, row.event).await?.expect("event deleted during transaction");
                if event.auto_import {
                    match event.match_source() {
                        MatchSource::Manual => {}
                        MatchSource::Challonge { .. } => {} // Challonge's API doesn't provide enough data to automate race imports
                        MatchSource::League => if event.is_started(&mut transaction).await? {
                            let mut races = Vec::default();
                            for id in sqlx::query_scalar!(r#"SELECT id AS "id: Id<Races>" FROM races WHERE series = $1 AND event = $2"#, event.series as _, &event.event).fetch_all(&mut *transaction).await? {
                                races.push(Race::from_id(&mut transaction, &http_client, id).await?);
                            }
                            let schedule = http_client.get("https://league.ootrandomizer.com/scheduleJson")
                                .send().await?
                                .detailed_error_for_status().await?
                                .json_with_text_in_error::<league::Schedule>().await?;
                            for match_data in schedule.matches {
                                if match_data.id <= 938 { continue } // seasons 5 to 8
                                let mut new_race = Race {
                                    id: Id::dummy(),
                                    series: event.series,
                                    event: event.event.to_string(),
                                    source: Source::League { id: match_data.id },
                                    entrants: Entrants::Two([
                                        match_data.player_a.into_entrant(&http_client).await?,
                                        match_data.player_b.into_entrant(&http_client).await?,
                                    ]),
                                    phase: None,
                                    round: Some(match_data.division),
                                    game: None,
                                    scheduling_thread: None,
                                    schedule: RaceSchedule::Live {
                                        start: match_data.time_utc,
                                        end: None,
                                        room: None,
                                    },
                                    schedule_updated_at: None,
                                    fpa_invoked: false,
                                    breaks_used: false,
                                    draft: None,
                                    seed: seed::Data::default(),
                                    video_urls: if let Ok(twitch_username) = match_data.restreamers.iter().filter_map(|restreamer| restreamer.twitch_username.as_ref()).exactly_one() { //TODO notify on multiple restreams
                                        iter::once((match_data.restream_language.unwrap_or(English), Url::parse(&format!("https://twitch.tv/{twitch_username}"))?)).collect()
                                    } else {
                                        HashMap::default()
                                    },
                                    restreamers: if_chain! {
                                        if let Ok(restreamer) = match_data.restreamers.into_iter().exactly_one(); //TODO notify on multiple restreams
                                        if let Some(racetime_id) = restreamer.racetime_id(&http_client).await?;
                                        then {
                                            iter::once((match_data.restream_language.unwrap_or(English), racetime_id)).collect()
                                        } else {
                                            HashMap::default()
                                        }
                                    },
                                    last_edited_by: None,
                                    last_edited_at: None,
                                    ignored: match match_data.status {
                                        league::MatchStatus::Canceled => true,
                                        league::MatchStatus::Confirmed => false,
                                    },
                                    schedule_locked: false,
                                    notified: false,
                                    async_notified_1: false,
                                    async_notified_2: false,
                                    async_notified_3: false,
                                    discord_scheduled_event_id: None,
                                    volunteer_request_sent: false,
                                    volunteer_request_message_id: None,
                                };
                                if let Some(race) = races.iter_mut().find(|race| if let Source::League { id } = race.source { id == match_data.id } else { false }) {
                                    if !race.schedule_locked {
                                        let is_upcoming = !race.has_any_room(); // stop automatically updating certain fields once a room is open
                                        *race = Race {
                                            id: race.id,
                                            schedule: if is_upcoming { new_race.schedule } else { mem::take(&mut race.schedule) },
                                            schedule_updated_at: race.schedule_updated_at,
                                            seed: mem::take(&mut race.seed),
                                            video_urls: if is_upcoming { new_race.video_urls } else { mem::take(&mut race.video_urls) },
                                            restreamers: if is_upcoming { new_race.restreamers } else { mem::take(&mut race.restreamers) },
                                            last_edited_at: race.last_edited_at,
                                            last_edited_by: race.last_edited_by,
                                            notified: race.notified,
                                            ..new_race
                                        };
                                    }
                                    race
                                } else {
                                    new_race.id = Id::<Races>::new(&mut transaction).await?;
                                    races.push(new_race);
                                    races.last_mut().expect("just pushed")
                                }.save(&mut transaction).await?;
                            }
                        },
                        MatchSource::StartGG(event_slug) => loop {
                            match startgg::races_to_import(&mut transaction, &http_client, &config, &event, event_slug).await {
                                Ok((races, _)) => {
                                    for race in races {
                                        transaction = import_race(transaction, &*discord_ctx.read().await, race).await?;
                                    }
                                    break
                                }
                                Err(Error::UnknownTeamStartGG(entrant)) => {
                                    let response = startgg::query_cached::<startgg::TeamMembersQuery>(&http_client, &config.startgg, startgg::team_members_query::Variables { entrant: entrant.clone() }).await?;
                                    let startgg::team_members_query::ResponseData {
                                        entrant: Some(startgg::team_members_query::TeamMembersQueryEntrant {
                                            participants: Some(participants),
                                        }),
                                    } = response else { return Err(Error::UnknownTeamStartGG(entrant).into()) };
                                    let Ok(startgg::team_members_query::TeamMembersQueryEntrantParticipants {
                                        user: Some(startgg::team_members_query::TeamMembersQueryEntrantParticipantsUser { //TODO if user is None, this is a participant without a start.gg account, match on display name or DM Fenhl about connecting manually, don't return error
                                            id: Some(user_id),
                                        }),
                                    }) = participants.into_iter().filter_map(identity).exactly_one() else { return Err(Error::UnknownTeamStartGG(entrant).into()) };
                                    let Some(user) = User::from_startgg(&mut *transaction, user_id).await? else { return Err(Error::UnknownTeamStartGG(entrant).into()) };
                                    let Some(team) = Team::from_event_and_member(&mut transaction, event.series, &event.event, user.id).await? else { return Err(Error::UnknownTeamStartGG(entrant).into()) };
                                    sqlx::query!("UPDATE teams SET startgg_id = $1 WHERE id = $2", entrant as _, team.id as _).execute(&mut *transaction).await?;
                                    transaction.commit().await?;
                                    transaction = db_pool.begin().await?;
                                }
                                Err(e) => return Err(e.into()),
                            }
                        }
                    }
                }
                if let Some(ref speedgaming_slug) = event.speedgaming_slug {
                    let schedule = sgl::schedule(&http_client, speedgaming_slug).await?;
                    let races = Race::for_event(&mut transaction, &http_client, &event).await?;
                    let (mut existing_races, mut unassigned_races) = races.into_iter().partition::<Vec<_>, _>(|race| matches!(race.source, Source::SpeedGaming { .. }));
                    existing_races.sort_unstable_by_key(|race| {
                        let Source::SpeedGaming { id } = race.source else { unreachable!("partitioned above") };
                        id
                    });
                    let disambiguation_messages = sqlx::query_scalar!(
                        "SELECT speedgaming_id FROM speedgaming_disambiguation_messages WHERE speedgaming_id = ANY($1) ORDER BY speedgaming_id ASC",
                        &schedule.iter().flat_map(|restream| restream.matches()).map(|restream_match| restream_match.id).collect_vec(),
                    ).fetch_all(&mut *transaction).await?;
                    for restream in schedule {
                        for restream_match in restream.matches() {
                            if let Ok(idx) = existing_races.binary_search_by_key(&restream_match.id, |race| {
                                let Source::SpeedGaming { id } = race.source else { unreachable!("partitioned above") };
                                id
                            }) {
                                // this match is already assigned to a race, update it in case it got rescheduled or its restream info got changed
                                let race = &mut existing_races[idx];
                                restream.update_race(race, restream_match.id)?;
                                race.save(&mut transaction).await?;
                            } else if disambiguation_messages.binary_search(&restream_match.id).is_ok() {
                                // this match is pending manual assignment, ignore it for now
                            } else {
                                let mut matching_races = Vec::default();
                                for (idx, race) in unassigned_races.iter().enumerate() {
                                    if restream_match.matches(&mut transaction, &http_client, race).await? {
                                        matching_races.push((idx, race));
                                    }
                                }
                                match matching_races.into_iter().at_most_one() {
                                    Ok(None) => {
                                        if let Some(organizer_channel) = event.discord_organizer_channel {
                                            let msg = MessageBuilder::default()
                                                .push("could not find any races matching SpeedGaming match ")
                                                .push_mono(restream_match.id.to_string())
                                                .push(" (")
                                                .push_safe(restream_match.to_string())
                                                .push(')')
                                                //TODO instructions for how to fix?
                                                .build();
                                            let notification = organizer_channel.say(&*discord_ctx.read().await, msg).await?;
                                            sqlx::query!(
                                                "INSERT INTO speedgaming_disambiguation_messages (speedgaming_id, message_id) VALUES ($1, $2)",
                                                restream_match.id, PgSnowflake(notification.id) as _,
                                            ).execute(&mut *transaction).await?;
                                        }
                                    }
                                    Ok(Some((idx, _))) => {
                                        let mut race = unassigned_races.swap_remove(idx);
                                        restream.update_race(&mut race, restream_match.id)?;
                                        race.save(&mut transaction).await?;
                                    }
                                    Err(races) => {
                                        if let Some(organizer_channel) = event.discord_organizer_channel {
                                            let msg = MessageBuilder::default()
                                                .push("found multiple races matching SpeedGaming match ")
                                                .push_mono(restream_match.id.to_string())
                                                .push(" (")
                                                .push_safe(restream_match.to_string())
                                                .push("), please select one to assign it to:")
                                                .build();
                                            let mut options = Vec::with_capacity(races.size_hint().0);
                                            for (_, race) in races {
                                                let info_prefix = format!("{}{}{}",
                                                    race.phase.as_deref().unwrap_or(""),
                                                    if race.phase.is_none() || race.round.is_none() { "" } else { " " },
                                                    race.round.as_deref().unwrap_or(""),
                                                );
                                                let summary = match race.entrants {
                                                    Entrants::Open | Entrants::Count { .. } => if info_prefix.is_empty() { format!("Untitled Race") } else { info_prefix },
                                                    Entrants::Named(ref entrants) => format!("{info_prefix}{}{entrants}", if info_prefix.is_empty() { "" } else { ": " }),
                                                    Entrants::Two([ref team1, ref team2]) => format!(
                                                        "{info_prefix}{}{} vs {}",
                                                        if info_prefix.is_empty() { "" } else { ": " },
                                                        team1.name(&mut transaction, &*discord_ctx.read().await).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                                        team2.name(&mut transaction, &*discord_ctx.read().await).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                                    ),
                                                    Entrants::Three([ref team1, ref team2, ref team3]) => format!(
                                                        "{info_prefix}{}{} vs {} vs {}",
                                                        if info_prefix.is_empty() { "" } else { ": " },
                                                        team1.name(&mut transaction, &*discord_ctx.read().await).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                                        team2.name(&mut transaction, &*discord_ctx.read().await).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                                        team3.name(&mut transaction, &*discord_ctx.read().await).await?.unwrap_or(Cow::Borrowed("(unnamed)")),
                                                    ),
                                                };
                                                options.push(CreateSelectMenuOption::new(if let Some(game) = race.game {
                                                    format!("{summary}, game {game}")
                                                } else {
                                                    summary
                                                }, race.id.to_string()));
                                            }
                                            let notification = organizer_channel.send_message(&*discord_ctx.read().await, CreateMessage::default()
                                                .content(msg)
                                                .select_menu(
                                                    CreateSelectMenu::new(format!("sgdisambig_{}", restream_match.id), CreateSelectMenuKind::String { options })
                                                        .placeholder("Select Race")
                                                )
                                            ).await?;
                                            sqlx::query!(
                                                "INSERT INTO speedgaming_disambiguation_messages (speedgaming_id, message_id) VALUES ($1, $2)",
                                                restream_match.id, PgSnowflake(notification.id) as _,
                                            ).execute(&mut *transaction).await?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            transaction.commit().await?;
        });
        select! {
            () = &mut shutdown => break,
            () = sleep(Duration::from_secs(60)) => {}
        }
    }
    Ok(())
}

/// Ensures that races exist for the next two occurrences of each active weekly schedule.
///
/// This is called both from the `weekly_race_manager` background task 
/// (to create races proactively before the room-opening window).
pub(crate) async fn ensure_weekly_races(
    transaction: &mut Transaction<'_, Postgres>,
    series: Series,
    event: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Race>, Error> {
    let weekly_schedules = WeeklySchedule::for_event(&mut *transaction, series, event).await?;
    let mut created = Vec::new();
    for ws in weekly_schedules.iter().filter(|s| s.active) {
        let next_weekly = ws.next_after(now);
        let weekly_after = ws.next_after(next_weekly);
        let frequency_label = match ws.frequency_days {
            7 => "Weekly",
            14 => "Biweekly",
            28 | 30 => "Monthly",
            _ => "Weekly",
        };
        let round = format!("{} {frequency_label}", ws.name);
        for weekly_time in [next_weekly, weekly_after] {
            let start = weekly_time.to_utc();
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM races WHERE series = $1 AND event = $2 AND start = $3 AND round = $4 AND NOT ignored) AS "exists!""#,
                series as _,
                event,
                start,
                round,
            )
            .fetch_one(&mut **transaction)
            .await?;
            if !exists {
                let schedule = RaceSchedule::Live { start, end: None, room: None };
                let race = Race {
                    id: Id::new(&mut *transaction).await?,
                    series,
                    event: event.to_owned(),
                    source: Source::Manual,
                    entrants: Entrants::Open,
                    phase: None,
                    round: Some(round.clone()),
                    game: None,
                    scheduling_thread: None,
                    schedule_updated_at: None,
                    fpa_invoked: false,
                    breaks_used: false,
                    draft: None,
                    seed: seed::Data::default(),
                    video_urls: HashMap::default(),
                    restreamers: HashMap::default(),
                    last_edited_by: None,
                    last_edited_at: None,
                    ignored: false,
                    schedule_locked: false,
                    notified: false,
                    async_notified_1: false,
                    async_notified_2: false,
                    async_notified_3: false,
                    discord_scheduled_event_id: None,
                    volunteer_request_sent: false,
                    volunteer_request_message_id: None,
                    schedule,
                };
                race.save(&mut *transaction).await?;
                created.push(race);
            }
        }
    }
    Ok(created)
}

pub(crate) async fn auto_import_races(db_pool: PgPool, http_client: reqwest::Client, config: Config, shutdown: rocket::Shutdown, discord_ctx: RwFuture<DiscordCtx>, new_room_lock: Arc<Mutex<()>>) -> Result<(), AutoImportError> {
    let mut last_crash = Instant::now();
    let mut wait_time = Duration::from_secs(1);
    loop {
        match auto_import_races_inner(db_pool.clone(), http_client.clone(), config.clone(), shutdown.clone(), discord_ctx.clone(), new_room_lock.clone()).await {
            Ok(()) => break Ok(()),
            Err(AutoImportError::Discord(discord_bot::Error::UninitializedDiscordGuild(guild_id))) => {
                let wait_time = Duration::from_secs(60);
                eprintln!("failed to auto-import races for uninitialized Discord guild {guild_id} (retrying in {})", English.format_duration(wait_time, true));
                sleep(wait_time).await;
            }
            Err(e) if e.is_network_error() => {
                if last_crash.elapsed() >= Duration::from_secs(60 * 60 * 24) {
                    wait_time = Duration::from_secs(1); // reset wait time after no crash for a day
                } else {
                    wait_time *= 2; // exponential backoff
                }
                if wait_time >= Duration::from_secs(2 * 60) {
                    eprintln!("failed to auto-import races (retrying in {}): {e} ({e:?})", English.format_duration(wait_time, true));
                    if wait_time >= Duration::from_secs(10 * 60) {
                        log::error!("failed to auto-import races (retrying in {}): {e} ({e:?})", English.format_duration(wait_time, true));
                    }
                }
                sleep(wait_time).await;
                last_crash = Instant::now();
            }
            Err(e) => {
                log::error!("failed to auto-import races: {e} ({e:?})");
                break Err(e)
            }
        }
    }
}

#[rocket::get("/event/<series>/<event>/races/<id>/practice")]
pub(crate) async fn practice_seed(pool: &State<PgPool>, http_client: &State<reqwest::Client>, ootr_api_client: &State<Arc<ootr_web::ApiClient>>, series: Series, event: &str, id: Id<Races>) -> Result<Redirect, StatusOrError<Error>> {
    let _ = (series, event);
    let mut transaction = pool.begin().await?;
    let race = Race::from_id(&mut transaction, http_client, id).await?;
    let rando_version = race.event(&mut transaction).await?.rando_version.ok_or(StatusOrError::Status(Status::NotFound))?;
    let settings = race.single_settings(&mut transaction).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    transaction.commit().await?;
    let world_count = settings.get("world_count").map_or(1, |world_count| world_count.as_u64().expect("world_count setting wasn't valid u64").try_into().expect("too many worlds"));
    let web_version = ootr_api_client.can_roll_on_web(None, &rando_version, world_count, UnlockSpoilerLog::Now).await.ok_or(StatusOrError::Status(Status::NotFound))?;
    let id = Arc::clone(ootr_api_client).roll_practice_seed(web_version, false, settings).await?;
    Ok(Redirect::to(format!("https://ootrandomizer.com/seed/get?id={id}")))
}

pub(crate) async fn edit_race_form(mut transaction: Transaction<'_, Postgres>, discord_ctx: &DiscordCtx, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: event::Data<'_>, race: Race, redirect_to: Option<Origin<'_>>, ctx: Option<Context<'_>>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Races, true).await?;
    let admin_user = User::primary_global_admin(&mut *transaction).await?.ok_or(PageError::AdminUserData(0))?;
    
    // Check if user is an organizer for start date editing
    let is_organizer = if let Some(ref me) = me {
        event.organizers(&mut transaction).await?.contains(me)
    } else {
        false
    };
    
    let mut errors = ctx.as_ref().map(|ctx| ctx.errors().collect()).unwrap_or_default();
    let form = if me.is_some() {
        full_form(uri!(edit_race_post(event.series, &*event.event, race.id, redirect_to)), csrf, html! {
            @match race.schedule {
                RaceSchedule::Unscheduled => {}
                RaceSchedule::Live { ref room, .. } => : form_field("room", &mut errors, html! {
                    label(for = "room") : "racetime.gg room:";
                    input(type = "text", name = "room", value? = if let Some(ref ctx) = ctx {
                        ctx.field_value("room").map(|room| room.to_string())
                    } else {
                        room.as_ref().map(|room| room.to_string())
                    });
                });
                RaceSchedule::Async { ref room1, ref room2, ref room3, .. } => {
                    : form_field("async_room1", &mut errors, html! {
                        label(for = "async_room1") : "racetime.gg room (team A):";
                        input(type = "text", name = "async_room1", value? = if let Some(ref ctx) = ctx {
                            ctx.field_value("async_room1").map(|room| room.to_string())
                        } else {
                            room1.as_ref().map(|room| room.to_string())
                        });
                    });
                    : form_field("async_room2", &mut errors, html! {
                        label(for = "async_room2") : "racetime.gg room (team B):";
                        input(type = "text", name = "async_room2", value? = if let Some(ref ctx) = ctx {
                            ctx.field_value("async_room2").map(|room| room.to_string())
                        } else {
                            room2.as_ref().map(|room| room.to_string())
                        });
                    });
                    @if let Entrants::Three(_) = race.entrants {
                        : form_field("async_room3", &mut errors, html! {
                            label(for = "async_room3") : "racetime.gg room (team C):";
                            input(type = "text", name = "async_room3", value? = if let Some(ref ctx) = ctx {
                                ctx.field_value("async_room3").map(|room| room.to_string())
                            } else {
                                room2.as_ref().map(|room| room.to_string())
                            });
                        });
                    }
                }
            }
            @if race.series == Series::League && !race.has_any_room() {
                // restream data entered here would be automatically overwritten
                fieldset {
                    label : "To edit restream data, please use the League website.";
                }
            } else {
                table {
                    thead {
                        tr {
                            th;
                            th {
                                : "Restream URL";
                                br;
                                small(style = "font-weight: normal;") : "Please use the first available out of the following: Permanent Twitch highlight, YouTube or other video, Twitch past broadcast, Twitch channel.";
                            }
                            //TODO hide restreamers column if the race room exists
                            th {
                                : "Restreamer";
                                br;
                                small(style = "font-weight: normal;") : "racetime.gg profile URL, racetime.gg user ID, or Hyrule Town Hall user ID. Enter \"me\" to assign yourself.";
                            }
                        }
                    }
                    tbody {
                        @for language in all::<Language>() {
                            tr {
                                th : language;
                                @let field_name = format!("video_urls.{}", language.short_code());
                                : form_table_cell(&field_name, &mut errors, html! {
                                    div(class = "autocomplete-container") {
                                        input(
                                            type = "text",
                                            name = &field_name,
                                            autocomplete = "off",
                                            value? = if let Some(ref ctx) = ctx {
                                                ctx.field_value(&*field_name).map(|room| room.to_string())
                                            } else {
                                                race.video_urls.get(&language).map(|video_url| video_url.to_string())
                                            }
                                        );
                                        div(
                                            id = format!("suggestions-{}", field_name.replace(".", "-")),
                                            class = "suggestions",
                                            style = "display: none;"
                                        ) {}
                                    }
                                });
                                //TODO hide restreamers column if the race room exists
                                @let field_name = format!("restreamers.{}", language.short_code());
                                : form_table_cell(&field_name, &mut errors, html! {
                                    div(class = "autocomplete-container") {
                                        input(
                                            type = "text",
                                            name = &field_name,
                                            autocomplete = "off",
                                            value? = if let Some(ref ctx) = ctx {
                                                ctx.field_value(&*field_name)
                                            } else if me.as_ref().and_then(|me| me.racetime.as_ref()).is_some_and(|racetime| race.restreamers.get(&language).is_some_and(|restreamer| *restreamer == racetime.id)) {
                                                Some("me")
                                            } else {
                                                race.restreamers.get(&language).map(|restreamer| restreamer.as_str()) //TODO display as racetime.gg profile URL
                                            }
                                        );
                                        div(
                                            id = format!("suggestions-{}", field_name.replace(".", "-")),
                                            class = "suggestions",
                                            style = "display: none;"
                                        ) {}
                                    }
                                });
                            }
                        }
                    }
                }
            }
            
            // Race management section
            fieldset {
                legend : "Race management";
                p(style = "font-size: 0.9em; color: #666; margin-bottom: 1em;") : "If a race will not take place at all, you can set it being canceled and hidden from view here.";
                : form_field("is_canceled", &mut errors, html! {
                    input(type = "checkbox", id = "is_canceled", name = "is_canceled", checked? = if let Some(ref ctx) = ctx {
                        ctx.field_value("is_canceled").map_or(false, |value| value == "on")
                    } else {
                        race.ignored
                    });
                    label(for = "is_canceled") : "Cancel race permanently";
                });
            }

        }, errors.clone(), "Save")
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(edit_race(event.series, &*event.event, race.id, redirect_to)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to edit this race.";
                }
            }
        }
    };
    let content = html! {
        : header;
        h2 : "Edit race";
        @match race.source {
            Source::Manual => p : "Source: Manually added";
            Source::Challonge { id } => p {
                : "Challonge match: ";
                : id;
            }
            Source::League { id } => p {
                : "league.ootrandomizer.com match ID: ";
                : id;
            }
            Source::Sheet { timestamp } => p {
                : "Google Form submission timestamp: ";
                : timestamp.format("%d/%m/%Y %H:%M:%S").to_string();
                : " (unknown time zone)";
            }
            Source::StartGG { event, set: startgg::ID(set) } => {
                p {
                    : "start.gg event: ";
                    : event;
                }
                p {
                    : "start.gg match: ";
                    : set;
                }
            }
            Source::SpeedGaming { id } => p {
                : "SpeedGaming match: ";
                : id;
            }
        }
        @match race.entrants {
            Entrants::Open => p : "Open entry";
            Entrants::Count { total, finished } => p {
                : total;
                : " entrants, ";
                : finished;
                : " finishers";
            }
            Entrants::Named(ref entrants) => p {
                : "Entrants: ";
                bdi : entrants;
            }
            Entrants::Two([ref p1, ref p2]) => {
                p : "Entrants:";
                ol {
                    li : p1.to_html(&mut transaction, discord_ctx, false).await?;
                    li : p2.to_html(&mut transaction, discord_ctx, false).await?;
                }
            }
            Entrants::Three([ref p1, ref p2, ref p3]) => {
                p : "Entrants:";
                ol {
                    li : p1.to_html(&mut transaction, discord_ctx, false).await?;
                    li : p2.to_html(&mut transaction, discord_ctx, false).await?;
                    li : p3.to_html(&mut transaction, discord_ctx, false).await?;
                }
            }
        }
        @if let Some(phase) = race.phase {
            p {
                : "Phase: ";
                : phase;
            }
        }
        @if let Some(round) = race.round {
            p {
                : "Round: ";
                : round;
            }
        }
        @if let Some(game) = race.game {
            p {
                : "Game: ";
                : game;
            }
        }
        @if let Some(ref me) = me {
            @if is_organizer {
                fieldset {
                    legend : "Race Schedule (Organizers Only)";
                    @match race.schedule {
                        RaceSchedule::Unscheduled => {
                            p : "Not yet scheduled";
                            : form_field("start_date", &mut errors, html! {
                                label(for = "start_date") : "Start date (YYYY-MM-DD HH:MM in your timezone):";
                                input(type = "text", name = "start_date", value? = if let Some(ref ctx) = ctx {
                                    ctx.field_value("start_date").map(|date| date.to_string())
                                } else {
                                    Some(String::new())
                                });
                                small : "input in your local time (name of timezone)";
                            });
                            input(type = "hidden", name = "timezone", id = "timezone-field");
                        }
                        RaceSchedule::Live { start, end, room: _ } => {
                            p {
                                : "Current start: ";
                                : format_datetime(start, DateTimeFormat { long: true, running_text: false });
                            }
                            : form_field("start_date", &mut errors, html! {
                                label(for = "start_date") : "New start date (YYYY-MM-DD HH:MM in your timezone):";
                                input(type = "text", name = "start_date", value? = if let Some(ref ctx) = ctx {
                                    ctx.field_value("start_date").map(|date| date.to_string())
                                } else {
                                    Some(start.format("%Y-%m-%d %H:%M").to_string())
                                });
                                small : "Leave empty to keep current time";
                            });
                            input(type = "hidden", name = "timezone", id = "timezone-field");

                        }
                        RaceSchedule::Async { start1, start2, start3, end1, end2, end3, room1: _, room2: _, room3: _ } => {
                            @if let Some(start1) = start1 {
                                p {
                                    : "Current start (team A): ";
                                    : format_datetime(start1, DateTimeFormat { long: true, running_text: false });
                                }
                            } else {
                                p : "Team A not yet started";
                            }
                            : form_field("async_start1_date", &mut errors, html! {
                                label(for = "async_start1_date") : "New start (team A) (YYYY-MM-DD HH:MM in your timezone):";
                                input(type = "text", name = "async_start1_date", value? = if let Some(ref ctx) = ctx {
                                    ctx.field_value("async_start1_date").map(|date| date.to_string())
                                } else {
                                    Some(start1.map(|s| s.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default())
                                });
                                small : "input in your local time (name of timezone)";
                            });
                            @if let Some(start2) = start2 {
                                p {
                                    : "Current start (team B): ";
                                    : format_datetime(start2, DateTimeFormat { long: true, running_text: false });
                                }
                            } else {
                                p : "Team B not yet started";
                            }
                            : form_field("async_start2_date", &mut errors, html! {
                                label(for = "async_start2_date") : "New start (team B) (YYYY-MM-DD HH:MM in your timezone):";
                                input(type = "text", name = "async_start2_date", value? = if let Some(ref ctx) = ctx {
                                    ctx.field_value("async_start2_date").map(|date| date.to_string())
                                } else {
                                    Some(start2.map(|s| s.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default())
                                });
                                small : "input in your local time (name of timezone)";
                            });
                            @if let Entrants::Three(_) = race.entrants {
                                @if let Some(start3) = start3 {
                                    p {
                                        : "Current start (team C): ";
                                        : format_datetime(start3, DateTimeFormat { long: true, running_text: false });
                                    }
                                } else {
                                    p : "Team C not yet started";
                                }
                                : form_field("async_start3_date", &mut errors, html! {
                                    label(for = "async_start3_date") : "New start (team C) (YYYY-MM-DD HH:MM in your timezone):";
                                    input(type = "text", name = "async_start3_date", value? = if let Some(ref ctx) = ctx {
                                        ctx.field_value("async_start3_date").map(|date| date.to_string())
                                    } else {
                                        Some(start3.map(|s| s.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default())
                                    });
                                    small : "input in your local time (name of timezone)";
                                });
                            }

                        }
                    }
                }
            } else {
                @match race.schedule {
                    RaceSchedule::Unscheduled => p : "Not yet scheduled";
                    RaceSchedule::Live { start, end: _, room: _ } => {
                        p {
                            : "Start: ";
                            : format_datetime(start, DateTimeFormat { long: true, running_text: false });
                        }
                    }
                    RaceSchedule::Async { start1, start2, start3, end1: _, end2: _, end3: _, room1: _, room2: _, room3: _ } => {
                        @if let Some(start1) = start1 {
                            p {
                                : "Start (team A): ";
                                : format_datetime(start1, DateTimeFormat { long: true, running_text: false });
                            }
                        } else {
                            p : "Team A not yet started";
                        }
                        @if let Some(start2) = start2 {
                            p {
                                : "Start (team B): ";
                                : format_datetime(start2, DateTimeFormat { long: true, running_text: false });
                            }
                        } else {
                            p : "Team B not yet started";
                        }
                        @if let Entrants::Three(_) = race.entrants {
                            @if let Some(start3) = start3 {
                                p {
                                    : "Start (team C): ";
                                    : format_datetime(start3, DateTimeFormat { long: true, running_text: false });
                                }
                            } else {
                                p : "Team C not yet started";
                            }
                        }
                    }
                }
                p {
                    : "The data above is currently not editable for technical reasons. Please contact ";
                    : admin_user;
                    : " if you've spotted an error in it.";
                }
            }
        } else {
            @match race.schedule {
                RaceSchedule::Unscheduled => p : "Not yet scheduled";
                RaceSchedule::Live { start, end, room: _ } => {
                    p {
                        : "Start: ";
                        : format_datetime(start, DateTimeFormat { long: true, running_text: false });
                    }
                    @if let Some(end) = end {
                        p {
                            : "End: ";
                            : format_datetime(end, DateTimeFormat { long: true, running_text: false });
                        }
                    } else {
                        p : "Not yet ended (will be updated automatically from the racetime.gg room, if any)";
                    }
                }
                RaceSchedule::Async { start1, start2, start3, end1, end2, end3, room1: _, room2: _, room3: _ } => {
                    @if let Some(start1) = start1 {
                        p {
                            : "Start (team A): ";
                            : format_datetime(start1, DateTimeFormat { long: true, running_text: false });
                        }
                    } else {
                        p : "Team A not yet started";
                    }
                    @if let Some(start2) = start2 {
                        p {
                            : "Start (team B): ";
                            : format_datetime(start2, DateTimeFormat { long: true, running_text: false });
                        }
                    } else {
                        p : "Team B not yet started";
                    }
                    @if let Entrants::Three(_) = race.entrants {
                        @if let Some(start3) = start3 {
                            p {
                                : "Start (team C): ";
                                : format_datetime(start3, DateTimeFormat { long: true, running_text: false });
                            }
                        } else {
                            p : "Team C not yet started";
                        }
                    }
                    @if let Some(end1) = end1 {
                        p {
                            : "End (team A): ";
                            : format_datetime(end1, DateTimeFormat { long: true, running_text: false });
                        }
                    } else {
                        p : "Team A not yet ended (will be updated automatically from the racetime.gg room, if any)";
                    }
                    @if let Some(end2) = end2 {
                        p {
                            : "End (team B): ";
                            : format_datetime(end2, DateTimeFormat { long: true, running_text: false });
                        }
                    } else {
                        p : "Team B not yet ended (will be updated automatically from the racetime.gg room, if any)";
                    }
                    @if let Entrants::Three(_) = race.entrants {
                        @if let Some(end3) = end3 {
                            p {
                                : "End (team C): ";
                                : format_datetime(end3, DateTimeFormat { long: true, running_text: false });
                            }
                        } else {
                            p : "Team C not yet ended (will be updated automatically from the racetime.gg room, if any)";
                        }
                    }
                }
            }
            p {
                : "The data above is currently not editable for technical reasons. Please contact ";
                : admin_user;
                : " if you've spotted an error in it.";
            }
        }
        : form;
        script(src = static_url!("restream-autocomplete.js")) {}
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Edit Race — {}", event.display_name), content).await?)
}

#[rocket::get("/event/<series>/<event>/races/<id>/edit?<redirect_to>")]
pub(crate) async fn edit_race(discord_ctx: &State<RwFuture<DiscordCtx>>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, id: Id<Races>, redirect_to: Option<Origin<'_>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let race = Race::from_id(&mut transaction, http_client, id).await?;
    if race.series != event.series || race.event != event.event {
        return Ok(RedirectOrContent::Redirect(Redirect::permanent(uri!(edit_race(race.series, race.event, id, redirect_to)))))
    }
    Ok(RedirectOrContent::Content(edit_race_form(transaction, &*discord_ctx.read().await, me, uri, csrf.as_ref(), event, race, redirect_to, None).await?))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EditRaceForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = String::new())]
    room: String,
    #[field(default = String::new())]
    async_room1: String,
    #[field(default = String::new())]
    async_room2: String,
    #[field(default = String::new())]
    async_room3: String,
    #[field(default = HashMap::new())]
    video_urls: HashMap<Language, String>,
    #[field(default = HashMap::new())]
    restreamers: HashMap<Language, String>,
    #[field(default = false)]
    is_canceled: bool,
    #[field(default = String::new())]
    start_date: String,
    #[field(default = String::new())]
    async_start1_date: String,
    #[field(default = String::new())]
    async_start2_date: String,
    #[field(default = String::new())]
    async_start3_date: String,
    #[field(default = String::new())]
    timezone: String,
}

#[rocket::post("/event/<series>/<event>/races/<id>/edit?<redirect_to>", data = "<form>")]
pub(crate) async fn edit_race_post(discord_ctx: &State<RwFuture<DiscordCtx>>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, id: Id<Races>, redirect_to: Option<Origin<'_>>, form: Form<Contextual<'_, EditRaceForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut race = Race::from_id(&mut transaction, http_client, id).await?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if race.series != event.series || race.event != event.event {
        form.context.push_error(form::Error::validation("This race is not part of this event."));
    }
    if !me.is_archivist && !event.organizers(&mut transaction).await?.contains(&me) && !event.restreamers(&mut transaction).await?.contains(&me) {
        form.context.push_error(form::Error::validation("You must be an organizer, restream coordinator, or archivist to edit this race. If you would like to be a restream coordinator for this event, please contact the organizers. If you would like to become an archivist, please contact TreZ on Discord."));
    }
    Ok(if let Some(ref value) = form.value {
        // Check if user is an organizer for start date editing
        let is_organizer = event.organizers(&mut transaction).await?.contains(&me);
        
        // Parse and validate start dates if user is an organizer
        let mut new_start_date = None;
        let mut new_async_start1_date = None;
        let mut new_async_start2_date = None;
        let mut new_async_start3_date = None;
        
        if is_organizer {
            // Parse live race start date
            if !value.start_date.is_empty() {
                if let Ok(naive_datetime) = NaiveDateTime::parse_from_str(&value.start_date, "%Y-%m-%d %H:%M") {
                    if value.timezone.is_empty() {
                        new_start_date = Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc));
                    } else {
                        match value.timezone.parse::<Tz>() {
                            Ok(tz) => {
                                match tz.from_local_datetime(&naive_datetime) {
                                    LocalResult::Single(dt) => new_start_date = Some(dt.with_timezone(&Utc)),
                                    LocalResult::None => {
                                        form.context.push_error(form::Error::validation(format!("Invalid datetime for timezone {}: {}", value.timezone, value.start_date)).with_name("start_date"));
                                    }
                                    LocalResult::Ambiguous(dt1, _) => {
                                        new_start_date = Some(dt1.with_timezone(&Utc));
                                    }
                                }
                            }
                            Err(_) => {
                                form.context.push_error(form::Error::validation(format!("Invalid timezone: {}. Use format like America/New_York or Europe/London", value.timezone)).with_name("timezone"));
                            }
                        }
                    }
                } else {
                    form.context.push_error(form::Error::validation("Start date must be in format YYYY-MM-DD HH:MM").with_name("start_date"));
                }
            }
            
            // Parse async race start dates
            if !value.async_start1_date.is_empty() {
                if let Ok(naive_datetime) = NaiveDateTime::parse_from_str(&value.async_start1_date, "%Y-%m-%d %H:%M") {
                    if value.timezone.is_empty() {
                        new_async_start1_date = Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc));
                    } else {
                        match value.timezone.parse::<Tz>() {
                            Ok(tz) => {
                                match tz.from_local_datetime(&naive_datetime) {
                                    LocalResult::Single(dt) => new_async_start1_date = Some(dt.with_timezone(&Utc)),
                                    LocalResult::None => {
                                        form.context.push_error(form::Error::validation(format!("Invalid datetime for timezone {}: {}", value.timezone, value.async_start1_date)).with_name("async_start1_date"));
                                    }
                                    LocalResult::Ambiguous(dt1, _) => {
                                        new_async_start1_date = Some(dt1.with_timezone(&Utc));
                                    }
                                }
                            }
                            Err(_) => {
                                form.context.push_error(form::Error::validation(format!("Invalid timezone: {}. Use format like America/New_York or Europe/London", value.timezone)).with_name("timezone"));
                            }
                        }
                    }
                } else {
                    form.context.push_error(form::Error::validation("Async start 1 date must be in format YYYY-MM-DD HH:MM").with_name("async_start1_date"));
                }
            }
            
            if !value.async_start2_date.is_empty() {
                if let Ok(naive_datetime) = NaiveDateTime::parse_from_str(&value.async_start2_date, "%Y-%m-%d %H:%M") {
                    if value.timezone.is_empty() {
                        new_async_start2_date = Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc));
                    } else {
                        match value.timezone.parse::<Tz>() {
                            Ok(tz) => {
                                match tz.from_local_datetime(&naive_datetime) {
                                    LocalResult::Single(dt) => new_async_start2_date = Some(dt.with_timezone(&Utc)),
                                    LocalResult::None => {
                                        form.context.push_error(form::Error::validation(format!("Invalid datetime for timezone {}: {}", value.timezone, value.async_start2_date)).with_name("async_start2_date"));
                                    }
                                    LocalResult::Ambiguous(dt1, _) => {
                                        new_async_start2_date = Some(dt1.with_timezone(&Utc));
                                    }
                                }
                            }
                            Err(_) => {
                                form.context.push_error(form::Error::validation(format!("Invalid timezone: {}. Use format like America/New_York or Europe/London", value.timezone)).with_name("timezone"));
                            }
                        }
                    }
                } else {
                    form.context.push_error(form::Error::validation("Async start 2 date must be in format YYYY-MM-DD HH:MM").with_name("async_start2_date"));
                }
            }
            
            if !value.async_start3_date.is_empty() {
                if let Ok(naive_datetime) = NaiveDateTime::parse_from_str(&value.async_start3_date, "%Y-%m-%d %H:%M") {
                    if value.timezone.is_empty() {
                        new_async_start3_date = Some(DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc));
                    } else {
                        match value.timezone.parse::<Tz>() {
                            Ok(tz) => {
                                match tz.from_local_datetime(&naive_datetime) {
                                    LocalResult::Single(dt) => new_async_start3_date = Some(dt.with_timezone(&Utc)),
                                    LocalResult::None => {
                                        form.context.push_error(form::Error::validation(format!("Invalid datetime for timezone {}: {}", value.timezone, value.async_start3_date)).with_name("async_start3_date"));
                                    }
                                    LocalResult::Ambiguous(dt1, _) => {
                                        new_async_start3_date = Some(dt1.with_timezone(&Utc));
                                    }
                                }
                            }
                            Err(_) => {
                                form.context.push_error(form::Error::validation(format!("Invalid timezone: {}. Use format like America/New_York or Europe/London", value.timezone)).with_name("timezone"));
                            }
                        }
                    }
                } else {
                    form.context.push_error(form::Error::validation("Async start 3 date must be in format YYYY-MM-DD HH:MM").with_name("async_start3_date"));
                }
            }
        }
        
        let mut valid_room_urls = HashMap::new();
        match race.schedule {
            RaceSchedule::Unscheduled => {
                if !value.room.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("room"));
                }
                if !value.async_room1.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("async_room1"));
                }
                if !value.async_room2.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("async_room2"));
                }
                if !value.async_room3.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("async_room3"));
                }
            }
            RaceSchedule::Live { .. } => {
                if !value.room.is_empty() {
                    match Url::parse(&value.room) {
                        Ok(room) => if let Some(host) = room.host_str() {
                            if host == racetime_host() {
                                valid_room_urls.insert("room", room);
                            } else {
                                form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("room"));
                            }
                        } else {
                            form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("room"));
                        }
                        Err(e) => form.context.push_error(form::Error::validation(format!("Failed to parse race room URL: {e}")).with_name("room")),
                    }
                }
                if !value.async_room1.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("async_room1"));
                }
                if !value.async_room2.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("async_room2"));
                }
                if !value.async_room3.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("async_room3"));
                }
            }
            RaceSchedule::Async { .. } => {
                if !value.room.is_empty() {
                    form.context.push_error(form::Error::validation("The race room can't be added yet because the race isn't scheduled.").with_name("room"));
                }
                if !value.async_room1.is_empty() {
                    match Url::parse(&value.async_room1) {
                        Ok(room) => if let Some(host) = room.host_str() {
                            if host == racetime_host() {
                                valid_room_urls.insert("async_room1", room);
                            } else {
                                form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("async_room1"));
                            }
                        } else {
                            form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("async_room1"));
                        }
                        Err(e) => form.context.push_error(form::Error::validation(format!("Failed to parse race room URL: {e}")).with_name("async_room1")),
                    }
                }
                if !value.async_room2.is_empty() {
                    match Url::parse(&value.async_room2) {
                        Ok(room) => if let Some(host) = room.host_str() {
                            if host == racetime_host() {
                                valid_room_urls.insert("async_room2", room);
                            } else {
                                form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("async_room2"));
                            }
                        } else {
                            form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("async_room2"));
                        }
                        Err(e) => form.context.push_error(form::Error::validation(format!("Failed to parse race room URL: {e}")).with_name("async_room2")),
                    }
                }
                if !value.async_room3.is_empty() {
                    match Url::parse(&value.async_room3) {
                        Ok(room) => if let Some(host) = room.host_str() {
                            if host == racetime_host() {
                                valid_room_urls.insert("async_room3", room);
                            } else {
                                form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("async_room3"));
                            }
                        } else {
                            form.context.push_error(form::Error::validation("Race room must be a racetime.gg URL.").with_name("async_room3"));
                        }
                        Err(e) => form.context.push_error(form::Error::validation(format!("Failed to parse race room URL: {e}")).with_name("async_room3")),
                    }
                }
            }
        }
        let mut file_hash: Option<[String; 5]> = None;
        let mut web_id = None;
        let mut web_gen_time = None;
        let mut file_stem = None;
        for (field_name, room) in valid_room_urls {
            if let Some(row) = sqlx::query!(r#"SELECT
                file_stem,
                web_id,
                web_gen_time,
                hash1,
                hash2,
                hash3,
                hash4,
                hash5
            FROM rsl_seeds WHERE room = $1"#, room.to_string()).fetch_optional(&mut *transaction).await? {
                file_hash = Some([row.hash1, row.hash2, row.hash3, row.hash4, row.hash5]);
                if let Some(new_web_id) = row.web_id {
                    web_id = Some(new_web_id);
                }
                if let Some(new_web_gen_time) = row.web_gen_time {
                    web_gen_time = Some(new_web_gen_time);
                }
                file_stem = Some(row.file_stem);
            } else {
                match http_client.get(format!("{room}/data")).send().await {
                    Ok(response) => match response.detailed_error_for_status().await {
                        Ok(response) => match response.json_with_text_in_error::<RaceData>().await {
                            Ok(race_data) => if let Some(info_bot) = race_data.info_bot {
                                if let Some((_, hash1, hash2, hash3, hash4, hash5, info_file_stem)) = regex_captures!("^(?:.+\n)?([^ ]+) ([^ ]+) ([^ ]+) ([^ ]+) ([^ ]+)(?: \\| (?:Password: )?[^ ]+ [^ ]+ [^ ]+ [^ ]+ [^ ]+ [^ ]+)?\nhttps://midos\\.house/seed/([0-9A-Za-z_-]+)(?:\\.zpfz?)?$", &info_bot) {
                                    let game_id = event.game(&mut transaction).await?.map(|g| g.id).unwrap_or(1);
                                    let Some(hash1) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash1).await? else { continue };
                                    let Some(hash2) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash2).await? else { continue };
                                    let Some(hash3) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash3).await? else { continue };
                                    let Some(hash4) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash4).await? else { continue };
                                    let Some(hash5) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash5).await? else { continue };
                                    file_hash = Some([hash1, hash2, hash3, hash4, hash5]);
                                    file_stem = Some(info_file_stem.to_owned());
                                    break
                                } else if let Some((_, hash1, hash2, hash3, hash4, hash5, web_id_str)) = regex_captures!("^(?:.+\n)?([^ ]+) ([^ ]+) ([^ ]+) ([^ ]+) ([^ ]+)(?: \\| (?:Password: )?[^ ]+ [^ ]+ [^ ]+ [^ ]+ [^ ]+ [^ ]+)?\nhttps://ootrandomizer\\.com/seed/get\\?id=([0-9]+)$", &info_bot) {
                                    let game_id = event.game(&mut transaction).await?.map(|g| g.id).unwrap_or(1);
                                    let Some(hash1) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash1).await? else { continue };
                                    let Some(hash2) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash2).await? else { continue };
                                    let Some(hash3) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash3).await? else { continue };
                                    let Some(hash4) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash4).await? else { continue };
                                    let Some(hash5) = racetime_emoji_to_hash_icon(&mut transaction, game_id, hash5).await? else { continue };
                                    file_hash = Some([hash1, hash2, hash3, hash4, hash5]);
                                    web_id = Some(web_id_str.parse().expect("found race room linking to out-of-range web seed ID"));
                                    match http_client.get("https://ootrandomizer.com/patch/get").query(&[("id", web_id)]).send().await {
                                        Ok(patch_response) => match patch_response.detailed_error_for_status().await {
                                            Ok(patch_response) => if let Some(content_disposition) = patch_response.headers().get(reqwest::header::CONTENT_DISPOSITION) {
                                                match content_disposition.to_str() {
                                                    Ok(content_disposition) => if let Some((_, patch_file_name)) = regex_captures!("^attachment; filename=(.+)$", content_disposition) {
                                                        let patch_file_name = patch_file_name.to_owned();
                                                        if let Some((_, patch_file_stem)) = regex_captures!(r"^(.+)\.zpfz?$", &patch_file_name) {
                                                            file_stem = Some(patch_file_stem.to_owned());
                                                            match File::create(Path::new(seed::DIR).join(&patch_file_name)).await {
                                                                Ok(mut file) => if let Err(e) = io::copy_buf(&mut StreamReader::new(patch_response.bytes_stream().map_err(io_error_from_reqwest)), &mut file).await {
                                                                    form.context.push_error(form::Error::validation(format!("Error saving patch file from room data: {e}")).with_name(field_name))
                                                                },
                                                                Err(e) => form.context.push_error(form::Error::validation(format!("Error saving patch file from room data: {e}")).with_name(field_name)),
                                                            }
                                                        } else {
                                                            form.context.push_error(form::Error::validation("Couldn't parse patch file name from room data").with_name(field_name));
                                                        }
                                                    } else {
                                                        form.context.push_error(form::Error::validation("Couldn't parse patch file name from room data").with_name(field_name));
                                                    },
                                                    Err(e) => form.context.push_error(form::Error::validation(format!("Couldn't parse patch file name from room data: {e}")).with_name(field_name)),
                                                }
                                            } else {
                                                form.context.push_error(form::Error::validation("Couldn't parse patch file name from room data").with_name(field_name));
                                            }
                                            Err(wheel::Error::ResponseStatus { inner, text, .. }) if inner.status() == Some(StatusCode::NOT_FOUND) && text.as_ref().is_ok_and(|text| text == "The indicated id is either invalid or has expired") => continue,
                                            Err(e) => form.context.push_error(form::Error::validation(format!("Error getting patch file from room data: {e}")).with_name(field_name)),
                                        },
                                        Err(e) => form.context.push_error(form::Error::validation(format!("Error getting patch file from room data: {e}")).with_name(field_name)),
                                    }
                                    if let Some(ref file_stem) = file_stem {
                                        match http_client.get("https://ootrandomizer.com/spoilers/get").query(&[("id", web_id)]).send().await {
                                            Ok(spoiler_response) => if spoiler_response.status() != StatusCode::BAD_REQUEST { // returns error 400 if no spoiler log has been generated
                                                match spoiler_response.detailed_error_for_status().await {
                                                    Ok(spoiler_response) => {
                                                        let spoiler_filename = format!("{file_stem}_Spoiler.json");
                                                        let spoiler_path = Path::new(seed::DIR).join(&spoiler_filename);
                                                        match File::create(&spoiler_path).await {
                                                            Ok(mut file) => match io::copy_buf(&mut StreamReader::new(spoiler_response.bytes_stream().map_err(io_error_from_reqwest)), &mut file).await {
                                                                Ok(_) => if file_hash.is_none() {
                                                                    match fs::read(spoiler_path).await {
                                                                        Ok(buf) => match serde_json::from_slice::<SpoilerLog>(&buf) {
                                                                            Ok(spoiler_log) => file_hash = Some(spoiler_log.file_hash),
                                                                            Err(e) => form.context.push_error(form::Error::validation(format!("Error reading spoiler log from room data: {e}")).with_name(field_name)),
                                                                        },
                                                                        Err(e) => form.context.push_error(form::Error::validation(format!("Error reading spoiler log from room data: {e}")).with_name(field_name)),
                                                                    }
                                                                },
                                                                Err(e) => form.context.push_error(form::Error::validation(format!("Error saving spoiler log from room data: {e}")).with_name(field_name)),
                                                            },
                                                            Err(e) => form.context.push_error(form::Error::validation(format!("Error saving spoiler log from room data: {e}")).with_name(field_name)),
                                                        }
                                                    }
                                                    Err(e) => form.context.push_error(form::Error::validation(format!("Error getting spoiler log from room data: {e}")).with_name(field_name)),
                                                }
                                            },
                                            Err(e) => form.context.push_error(form::Error::validation(format!("Error getting spoiler log from room data: {e}")).with_name(field_name)),
                                        }
                                    }
                                    break
                                }
                            },
                            Err(e) => form.context.push_error(form::Error::validation(format!("Error getting room data: {e}")).with_name(field_name)),
                        },
                        Err(e) => form.context.push_error(form::Error::validation(format!("Error getting room data: {e}")).with_name(field_name)),
                    },
                    Err(e) => form.context.push_error(form::Error::validation(format!("Error getting room data: {e}")).with_name(field_name)),
                }
            }
        }
        let mut restreamers = HashMap::new();
        for language in all() {
            if let Some(video_url) = value.video_urls.get(&language) {
                if !video_url.is_empty() {
                    if let Err(e) = Url::parse(video_url) {
                        form.context.push_error(form::Error::validation(format!("Failed to parse URL: {e}")).with_name(format!("video_urls.{}", language.short_code())));
                    }
                    if let Some(restreamer) = value.restreamers.get(&language) {
                        if !restreamer.is_empty() {
                            if restreamer == "me" {
                                if let Some(ref racetime) = me.racetime {
                                    restreamers.insert(language, racetime.id.clone());
                                } else {
                                    form.context.push_error(form::Error::validation("A racetime.gg account is required to restream races. Go to your profile and select \"Connect a racetime.gg account\".").with_name(format!("restreamers.{}", language.short_code()))); //TODO direct link
                                }
                            } else {
                                match racetime_bot::parse_user(&mut transaction, http_client, restreamer).await {
                                    Ok(racetime_id) => { restreamers.insert(language, racetime_id); }
                                    Err(e @ (racetime_bot::ParseUserError::Format | racetime_bot::ParseUserError::IdNotFound | racetime_bot::ParseUserError::InvalidUrl | racetime_bot::ParseUserError::MidosHouseId | racetime_bot::ParseUserError::MidosHouseUserNoRacetime | racetime_bot::ParseUserError::UrlNotFound)) => {
                                        form.context.push_error(form::Error::validation(e.to_string()).with_name(format!("restreamers.{}", language.short_code())));
                                    }
                                    Err(racetime_bot::ParseUserError::Reqwest(e)) => return Err(e.into()),
                                    Err(racetime_bot::ParseUserError::Sql(e)) => return Err(e.into()),
                                    Err(racetime_bot::ParseUserError::Wheel(e)) => return Err(e.into()),
                                }
                            }
                        }
                    }
                } else {
                    if value.restreamers.get(&language).is_some_and(|restreamer| !restreamer.is_empty()) {
                        form.context.push_error(form::Error::validation("Please either add a restream URL or remove the restreamer.").with_name(format!("restreamers.{}", language.short_code())));
                    }
                }
            }
        }
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(edit_race_form(transaction, &*discord_ctx.read().await, Some(me), uri, csrf.as_ref(), event, race, redirect_to, Some(form.context)).await?)
        } else {
            // Update race schedule with new start dates if organizer
            if is_organizer {
                match &mut race.schedule {
                    RaceSchedule::Unscheduled => {
                        if let Some(new_start) = new_start_date {
                            race.schedule = RaceSchedule::Live {
                                start: new_start,
                                end: None,
                                room: None,
                            };
                            race.schedule_updated_at = Some(Utc::now());
                        }
                    }
                    RaceSchedule::Live { start, end: _, room } => {
                        if let Some(new_start) = new_start_date {
                            *start = new_start;
                            race.schedule_updated_at = Some(Utc::now());
                        }
                        *room = (!value.room.is_empty()).then(|| Url::parse(&value.room).expect("validated"));
                    }
                    RaceSchedule::Async { start1, start2, start3, end1: _, end2: _, end3: _, room1, room2, room3 } => {
                        if let Some(new_start) = new_async_start1_date {
                            *start1 = Some(new_start);
                            race.schedule_updated_at = Some(Utc::now());
                        }
                        if let Some(new_start) = new_async_start2_date {
                            *start2 = Some(new_start);
                            race.schedule_updated_at = Some(Utc::now());
                        }
                        if let Some(new_start) = new_async_start3_date {
                            *start3 = Some(new_start);
                            race.schedule_updated_at = Some(Utc::now());
                        }
                        *room1 = (!value.async_room1.is_empty()).then(|| Url::parse(&value.async_room1).expect("validated"));
                        *room2 = (!value.async_room2.is_empty()).then(|| Url::parse(&value.async_room2).expect("validated"));
                        *room3 = (!value.async_room3.is_empty()).then(|| Url::parse(&value.async_room3).expect("validated"));
                    }
                }
            } else {
                // Non-organizers can only update room URLs
                match &mut race.schedule {
                    RaceSchedule::Unscheduled => {}
                    RaceSchedule::Live { room, .. } => *room = (!value.room.is_empty()).then(|| Url::parse(&value.room).expect("validated")),
                    RaceSchedule::Async { room1, room2, room3, .. } => {
                        *room1 = (!value.async_room1.is_empty()).then(|| Url::parse(&value.async_room1).expect("validated"));
                        *room2 = (!value.async_room2.is_empty()).then(|| Url::parse(&value.async_room2).expect("validated"));
                        *room3 = (!value.async_room3.is_empty()).then(|| Url::parse(&value.async_room3).expect("validated"));
                    }
                }
            }
            race.last_edited_by = Some(me.id);
            race.last_edited_at = Some(Utc::now());
            race.ignored = value.is_canceled;
            
            // Save original video URLs to check if they changed
            let original_video_urls = race.video_urls.clone();
            
            if race.series != Series::League || race.has_any_room() {
                race.video_urls = value.video_urls.iter().filter(|(_, video_url)| !video_url.is_empty()).map(|(language, video_url)| (*language, Url::parse(video_url).expect("validated"))).collect();
                race.restreamers = restreamers;
            }
            if let Some(file_hash) = file_hash {
                race.seed.file_hash = Some(file_hash);
            }
            if let (Some(id), Some(gen_time), Some(file_stem)) = (web_id, web_gen_time, file_stem) {
                race.seed.files = Some(seed::Files::OotrWeb { id, gen_time, file_stem: Cow::Owned(file_stem) });
            }
            race.save(&mut transaction).await?;

            // Update Discord scheduled event if restream URLs changed
            if race.video_urls != original_video_urls {
                if let Err(e) = crate::discord_scheduled_events::update_discord_scheduled_event(&*discord_ctx.read().await, &mut transaction, &race, &event, http_client.inner()).await {
                    eprintln!("Failed to update Discord scheduled event for race {}: {}", race.id, e);
                }
            }

            // Send DM notifications to confirmed volunteers when restream URLs are newly assigned
            if !race.video_urls.is_empty() && race.video_urls != original_video_urls {
                // Find newly added languages
                let new_languages: Vec<_> = race.video_urls.keys()
                    .filter(|lang| !original_video_urls.contains_key(*lang))
                    .collect();

                if !new_languages.is_empty() {
                    // Build race description
                    let race_description = if race.phase.as_ref().is_some_and(|p| p == "Qualifier") {
                        match (&race.round, &race.phase) {
                            (Some(round), _) => round.clone(),
                            (None, Some(phase)) => phase.clone(),
                            (None, None) => "Qualifier".to_string(),
                        }
                    } else {
                        match &race.entrants {
                            Entrants::Two([team1, team2]) => format!("{} vs {}",
                                match team1 {
                                    Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                    Entrant::Named { name, .. } => name.clone(),
                                    Entrant::Discord { .. } => "Discord User".to_string(),
                                },
                                match team2 {
                                    Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                    Entrant::Named { name, .. } => name.clone(),
                                    Entrant::Discord { .. } => "Discord User".to_string(),
                                }
                            ),
                            Entrants::Three([team1, team2, team3]) => format!("{} vs {} vs {}",
                                match team1 {
                                    Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                    Entrant::Named { name, .. } => name.clone(),
                                    Entrant::Discord { .. } => "Discord User".to_string(),
                                },
                                match team2 {
                                    Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                    Entrant::Named { name, .. } => name.clone(),
                                    Entrant::Discord { .. } => "Discord User".to_string(),
                                },
                                match team3 {
                                    Entrant::MidosHouseTeam(team) => team.name(&mut transaction).await?.unwrap_or_else(|| "Unknown Team".to_string().into()).into_owned(),
                                    Entrant::Named { name, .. } => name.clone(),
                                    Entrant::Discord { .. } => "Discord User".to_string(),
                                }
                            ),
                            _ => race.round.clone().or_else(|| race.phase.clone()).unwrap_or_else(|| "Race".to_string()),
                        }
                    };

                    // Get race start time for timestamp
                    if let RaceSchedule::Live { start: race_start_time, .. } = race.schedule {
                        // Get confirmed volunteers and their role bindings
                        let signups = Signup::for_race(&mut transaction, race.id).await?;
                        let role_bindings = event::roles::EffectiveRoleBinding::for_event(&mut transaction, event.series, &event.event).await?;

                        let discord_ctx = discord_ctx.read().await;

                        for signup in signups.iter().filter(|s| matches!(s.status, VolunteerSignupStatus::Confirmed)) {
                            // Get the language for this signup's role binding
                            if let Some(binding) = role_bindings.iter().find(|b| b.id == signup.role_binding_id) {
                                // Check if the restream for this language was newly added
                                if new_languages.contains(&&binding.language) {
                                    if let Some(video_url) = race.video_urls.get(&binding.language) {
                                        // Get user and send DM
                                        if let Ok(Some(user)) = User::from_id(&mut *transaction, signup.user_id).await {
                                            if let Some(discord) = user.discord {
                                                let discord_user_id = UserId::new(discord.id.get());

                                                let mut msg = MessageBuilder::default();
                                                msg.push("A restream channel has been assigned for ");
                                                msg.push_mono(&race_description);
                                                msg.push(" in ");
                                                msg.push(&event.display_name);
                                                msg.push("!\n\n");
                                                msg.push("**Restream (");
                                                msg.push(&binding.language.to_string());
                                                msg.push("):** <");
                                                msg.push(&video_url.to_string());
                                                msg.push(">\n");
                                                msg.push("**When:** ");
                                                msg.push_timestamp(race_start_time, serenity_utils::message::TimestampStyle::LongDateTime);

                                                if let Ok(dm_channel) = discord_user_id.create_dm_channel(&*discord_ctx).await {
                                                    if let Err(e) = dm_channel.say(&*discord_ctx, msg.build()).await {
                                                        eprintln!("Failed to send restream notification DM: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            transaction.commit().await?;

            // Update the volunteer info post to reflect restream changes
            // (Must be done after commit so the new transaction can see the changes)
            if race.video_urls != original_video_urls {
                if let Err(e) = crate::volunteer_requests::update_volunteer_post_for_race(
                    pool,
                    &*discord_ctx.read().await,
                    race.id,
                ).await {
                    eprintln!("Failed to update volunteer info post for race {} after restream URL change: {}", race.id, e);
                }
            }
            RedirectOrContent::Redirect(Redirect::to(redirect_to.map(|Origin(uri)| uri.into_owned()).unwrap_or_else(|| uri!(event::races(event.series, &*event.event)))))
        }
    } else {
        RedirectOrContent::Content(edit_race_form(transaction, &*discord_ctx.read().await, Some(me), uri, csrf.as_ref(), event, race, redirect_to, Some(form.context)).await?)
    })
}

pub(crate) async fn add_file_hash_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, event: event::Data<'_>, race: Race, ctx: Context<'_>) -> Result<RawHtml<String>, event::Error> {
    let header = event.header(&mut transaction, me.as_ref(), Tab::Races, true).await?;
    let form = if me.is_some() {
        let mut errors = ctx.errors().collect();
        full_form(uri!(add_file_hash_post(event.series, &*event.event, race.id)), csrf, html! {
            //TODO preview selected icons using CSS/JS?
            : form_field("hash1", &mut errors, html! {
                label(for = "hash1") : "Hash Icon 1:";
                select(name = "hash1") {
                    @for hash_icon_data in HashIconData::all_for_game(&mut transaction, 1).await? {
                        option(value = hash_icon_data.name, selected? = ctx.field_value("hash1") == Some(&hash_icon_data.name)) : hash_icon_data.name;
                    }
                }
            });
            : form_field("hash2", &mut errors, html! {
                label(for = "hash2") : "Hash Icon 2:";
                select(name = "hash2") {
                    @for hash_icon_data in HashIconData::all_for_game(&mut transaction, 1).await? {
                        option(value = hash_icon_data.name, selected? = ctx.field_value("hash2") == Some(&hash_icon_data.name)) : hash_icon_data.name;
                    }
                }
            });
            : form_field("hash3", &mut errors, html! {
                label(for = "hash3") : "Hash Icon 3:";
                select(name = "hash3") {
                    @for hash_icon_data in HashIconData::all_for_game(&mut transaction, 1).await? {
                        option(value = hash_icon_data.name, selected? = ctx.field_value("hash3") == Some(&hash_icon_data.name)) : hash_icon_data.name;
                    }
                }
            });
            : form_field("hash4", &mut errors, html! {
                label(for = "hash4") : "Hash Icon 4:";
                select(name = "hash4") {
                    @for hash_icon_data in HashIconData::all_for_game(&mut transaction, 1).await? {
                        option(value = hash_icon_data.name, selected? = ctx.field_value("hash4") == Some(&hash_icon_data.name)) : hash_icon_data.name;
                    }
                }
            });
            : form_field("hash5", &mut errors, html! {
                label(for = "hash5") : "Hash Icon 5:";
                select(name = "hash5") {
                    @for hash_icon_data in HashIconData::all_for_game(&mut transaction, 1).await? {
                        option(value = hash_icon_data.name, selected? = ctx.field_value("hash5") == Some(&hash_icon_data.name)) : hash_icon_data.name;
                    }
                }
            });
        }, errors, "Save")
    } else {
        html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(add_file_hash(event.series, &*event.event, race.id)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to edit this race.";
                }
            }
        }
    };
    let content = html! {
        : header;
        h2 : "Add file hash";
        @match race.schedule {
            RaceSchedule::Unscheduled => p : "Not yet scheduled";
            RaceSchedule::Live { room, .. } => @if let Some(room) = room {
                p {
                    a(href = room.to_string()) : "Race room";
                }
            } else {
                p : "Race room not yet assigned";
            }
            RaceSchedule::Async { room1, room2, room3, .. } => {
                @if let Some(room1) = room1 {
                    p {
                        a(href = room1.to_string()) : "Race room 1";
                    }
                } else {
                    p : "Race room 1 not yet assigned";
                }
                @if let Some(room2) = room2 {
                    p {
                        a(href = room2.to_string()) : "Race room 2";
                    }
                } else {
                    p : "Race room 2 not yet assigned";
                }
                @if let Entrants::Three(_) = race.entrants {
                    @if let Some(room3) = room3 {
                        p {
                            a(href = room3.to_string()) : "Race room 3";
                        }
                    } else {
                        p : "Race room 3 not yet assigned";
                    }
                }
            }
        }
        : form;
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: event.chests().await?, ..PageStyle::default() }, &format!("Edit Race — {}", event.display_name), content).await?)
}

#[rocket::get("/event/<series>/<event>/races/<id>/edit-hash")]
pub(crate) async fn add_file_hash(pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, id: Id<Races>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let race = Race::from_id(&mut transaction, http_client, id).await?;
    if race.series != event.series || race.event != event.event {
        return Ok(RedirectOrContent::Redirect(Redirect::permanent(uri!(add_file_hash(race.series, race.event, id)))))
    }
    Ok(RedirectOrContent::Content(add_file_hash_form(transaction, me, uri, csrf.as_ref(), event, race, Context::default()).await?))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct AddFileHashForm {
    #[field(default = String::new())]
    csrf: String,
    hash1: String,
    hash2: String,
    hash3: String,
    hash4: String,
    hash5: String,
}

#[rocket::post("/event/<series>/<event>/races/<id>/edit-hash", data = "<form>")]
pub(crate) async fn add_file_hash_post(pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, id: Id<Races>, form: Form<Contextual<'_, AddFileHashForm>>) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event = event::Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let race = Race::from_id(&mut transaction, http_client, id).await?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if race.series != event.series || race.event != event.event {
        form.context.push_error(form::Error::validation("This race is not part of this event."));
    }
    if !me.is_archivist && !event.organizers(&mut transaction).await?.contains(&me) {
        form.context.push_error(form::Error::validation("You must be an organizer or archivist to edit this race. If you would like to become an archivist, please contact TreZ on Discord."));
    }
    Ok(if let Some(ref value) = form.value {
        let hash1 = if !value.hash1.is_empty() {
            Some(&value.hash1)
        } else {
            form.context.push_error(form::Error::validation("Hash icon 1 is required.").with_name("hash1"));
            None
        };
        let hash2 = if !value.hash2.is_empty() {
            Some(&value.hash2)
        } else {
            form.context.push_error(form::Error::validation("Hash icon 2 is required.").with_name("hash2"));
            None
        };
        let hash3 = if !value.hash3.is_empty() {
            Some(&value.hash3)
        } else {
            form.context.push_error(form::Error::validation("Hash icon 3 is required.").with_name("hash3"));
            None
        };
        let hash4 = if !value.hash4.is_empty() {
            Some(&value.hash4)
        } else {
            form.context.push_error(form::Error::validation("Hash icon 4 is required.").with_name("hash4"));
            None
        };
        let hash5 = if !value.hash5.is_empty() {
            Some(&value.hash5)
        } else {
            form.context.push_error(form::Error::validation("Hash icon 5 is required.").with_name("hash5"));
            None
        };
        if form.context.errors().next().is_some() {
            RedirectOrContent::Content(add_file_hash_form(transaction, Some(me), uri, csrf.as_ref(), event, race, form.context).await?)
        } else {
            sqlx::query!(
                "UPDATE races SET hash1 = $1, hash2 = $2, hash3 = $3, hash4 = $4, hash5 = $5 WHERE id = $6",
                hash1.unwrap(), hash2.unwrap(), hash3.unwrap(), hash4.unwrap(), hash5.unwrap(), id as _,
            ).execute(&mut *transaction).await?;
            transaction.commit().await?;
            RedirectOrContent::Redirect(Redirect::to(uri!(event::races(event.series, &*event.event))))
        }
    } else {
        RedirectOrContent::Content(add_file_hash_form(transaction, Some(me), uri, csrf.as_ref(), event, race, form.context).await?)
    })
}

async fn racetime_emoji_to_hash_icon(_transaction: &mut Transaction<'_, Postgres>, _game_id: i32, emoji: &str) -> Result<Option<String>, sqlx::Error> {
    // This function converts racetime emoji to hash icon names
    // For now, we'll just return the emoji as-is since we're using strings
    // In the future, this could be enhanced to map emojis to specific hash icon names
    Ok(Some(emoji.to_string()))
}
