use {
    std::hash::Hasher,
    noisy_float::prelude::*,
    racetime::model::{
        EntrantStatusValue,
        RaceStatusValue,
    },
    crate::{
        event::{
            Data,
            DataError,
            Role,
            SignupStatus,
            Tab,
            enter,
        },
        prelude::*,
    },
};

#[derive(Clone, Copy)]
pub(crate) enum QualifierKind {
    None,
    Rank,
    Single {
        show_times: bool,
    },
    Score(QualifierScoreKind),
    SongsOfHope,
}

#[derive(Clone, Copy)]
pub(crate) enum QualifierScoreKind {
    Standard,
    Sgl2023Online,
    Sgl2024Online,
}

pub(crate) enum MemberUser {
    MidosHouse(User),
    RaceTime {
        id: String,
        url: String,
        name: String,
    },
    /// A user who represents someone new joining future qualifiers, for worst-case placement calculation.
    Newcomer,
}

impl PartialEq for MemberUser {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            | (Self::MidosHouse(user1), Self::MidosHouse(user2))
                => user1.id == user2.id,
            | (Self::RaceTime { id: id1, .. }, Self::RaceTime { id: id2, .. })
                => id1 == id2,
            | (Self::Newcomer, Self::Newcomer)
                => true,
            | (Self::MidosHouse(user), Self::RaceTime { id, .. })
            | (Self::RaceTime { id, .. }, Self::MidosHouse(user))
                => user.racetime.as_ref().is_some_and(|racetime| racetime.id == *id),
            | (Self::MidosHouse(_), Self::Newcomer)
            | (Self::RaceTime { .. }, Self::Newcomer)
            | (Self::Newcomer, Self::MidosHouse(_))
            | (Self::Newcomer, Self::RaceTime { .. })
                => false,
        }
    }
}

impl Eq for MemberUser {}

impl Hash for MemberUser {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::MidosHouse(user) => if let Some(racetime) = user.racetime.as_ref() {
                0u8.hash(state);
                racetime.id.hash(state);
            } else {
                1u8.hash(state);
                user.id.hash(state);
            },
            Self::RaceTime { id, .. } => {
                0u8.hash(state);
                id.hash(state);
            }
            Self::Newcomer => {
                2u8.hash(state);
            }
        }
    }
}

impl PartialOrd for MemberUser {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MemberUser {
    fn cmp(&self, other: &Self) -> Ordering {
        let racetime_id1 = match self {
            Self::MidosHouse(user) => user.racetime.as_ref().map(|racetime| &racetime.id),
            Self::RaceTime { id, .. } => Some(id),
            Self::Newcomer => None,
        };
        let racetime_id2 = match other {
            Self::MidosHouse(user) => user.racetime.as_ref().map(|racetime| &racetime.id),
            Self::RaceTime { id, .. } => Some(id),
            Self::Newcomer => None,
        };
        if let (Some(id1), Some(id2)) = (racetime_id1, racetime_id2) {
            id1.cmp(id2)
        } else {
            match (self, other) {
                (Self::MidosHouse(user1), Self::MidosHouse(user2)) => user1.racetime.is_some().cmp(&user2.racetime.is_some())
                    .then_with(|| user1.id.cmp(&user2.id)),
                (Self::MidosHouse(_), Self::RaceTime { .. }) => Less,
                (Self::MidosHouse(_), Self::Newcomer) => Less,
                (Self::RaceTime { .. }, Self::MidosHouse(_)) => Greater,
                (Self::RaceTime { id: id1, .. }, Self::RaceTime { id: id2, .. }) => id1.cmp(id2),
                (Self::RaceTime { .. }, Self::Newcomer) => Less,
                (Self::Newcomer, Self::MidosHouse(_)) => Greater,
                (Self::Newcomer, Self::RaceTime { .. }) => Greater,
                (Self::Newcomer, Self::Newcomer) => Equal,
            }
        }
    }
}

impl PartialEq<User> for MemberUser {
    fn eq(&self, other: &User) -> bool {
        match self {
            Self::MidosHouse(user) => user == other,
            Self::RaceTime { id, .. } => other.racetime.as_ref().is_some_and(|racetime| racetime.id == *id),
            Self::Newcomer => false,
        }
    }
}

pub(crate) struct SignupsMember {
    role: Role,
    pub(crate) user: MemberUser,
    pub(crate) is_confirmed: bool,
    pub(crate) qualifier_time: Option<Duration>,
    qualifier_vod: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum Qualification {
    Single {
        qualified: bool,
    },
    TriforceBlitz {
        qualified: bool,
        pieces: i16,
    },
    Multiple {
        num_entered: usize,
        num_finished: usize,
        score: R64,
    },
}

pub(crate) struct SignupsTeam {
    pub(crate) team: Option<Team>,
    pub(crate) members: Vec<SignupsMember>,
    pub(crate) qualification: Qualification,
    hard_settings_ok: bool,
    mq_ok: bool,
    lite_ok: bool,
    all_dungeons_ok: bool,
    flute_ok: bool,
    hover_ok: bool,
    inverted_ok: bool,
    keydrop_ok: bool,
    mirror_scroll_ok: bool,
    no_delay_ok: bool,
    pb_ok: bool,
    zw_ok: bool,
}

pub(crate) struct Cache {
    http_client: reqwest::Client,
    race_data: HashMap<Url, RaceData>,
}

impl Cache {
    pub(crate) fn new(http_client: reqwest::Client) -> Self {
        Self {
            race_data: HashMap::default(),
            http_client,
        }
    }

    async fn race_data(&mut self, room: &Url) -> Result<&RaceData, cal::Error> {
        Ok(match self.race_data.entry(room.clone()) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => entry.insert(self.http_client.get(format!("{room}/data"))
                .send().await?
                .detailed_error_for_status().await?
                .json_with_text_in_error::<RaceData>().await?
            ),
        })
    }
}

pub(crate) async fn signups_sorted(transaction: &mut Transaction<'_, Postgres>, cache: &mut Cache, me: Option<&User>, data: &Data<'_>, is_organizer: bool, qualifier_kind: QualifierKind, worst_case_extrapolation: Option<&MemberUser>) -> Result<Vec<SignupsTeam>, cal::Error> {
    let now = Utc::now();
    let mut signups = match qualifier_kind {
        QualifierKind::Score(score_kind) => {
            let mut scores = HashMap::<_, Vec<_>>::default();
            for race in Race::for_event(transaction, &cache.http_client, data).await? {
                if race.phase.as_ref().is_none_or(|phase| phase != "Qualifier") { continue }
                let Ok(room) = race.rooms().exactly_one() else {
                    if let Some(extrapolate_for) = worst_case_extrapolation {
                        scores.entry(MemberUser::Newcomer).or_default();
                        for (user, score) in &mut scores {
                            score.push(r64(if user == extrapolate_for {
                                0.0
                            } else {
                                match score_kind {
                                    QualifierScoreKind::Standard => 1100.0,
                                    QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => 110.0,
                                }
                            }));
                        }
                    }
                    continue
                };
                let room_data = cache.race_data(&room).await?;
                match room_data.status.value {
                    RaceStatusValue::Open => if let Some(extrapolate_for) = worst_case_extrapolation {
                        scores.entry(MemberUser::Newcomer).or_default();
                        for (user, score) in &mut scores {
                            score.push(r64(if user == extrapolate_for {
                                0.0
                            } else {
                                match score_kind {
                                    QualifierScoreKind::Standard => 1100.0,
                                    QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => 110.0,
                                }
                            }));
                        }
                    },
                    RaceStatusValue::Cancelled => {}
                    RaceStatusValue::Invitational | RaceStatusValue::Pending | RaceStatusValue::InProgress | RaceStatusValue::Finished => {
                        let mut entrants = room_data.entrants.clone();
                        if let QualifierScoreKind::Sgl2023Online = score_kind {
                            entrants.retain(|entrant| entrant.user.id != "yMewn83Vj3405Jv7"); // user was banned
                            entrants.iter_mut().for_each(|entrant| if entrant.user.id == "raP6yoaGaNBlV4zN" { entrant.user.id = format!("JrM6PoY8Pd3Rdm5v") }); // racetime.gg account change
                            if race.id == Id::from(17171498007470059483_u64) {
                                entrants.retain(|entrant| entrant.user.id != "JrM6PoY6LQWRdm5v"); // result was annulled
                            }
                        }
                        for entrant in &mut entrants {
                            match entrant.status.value {
                                | EntrantStatusValue::Requested
                                    => {}
                                | EntrantStatusValue::Invited
                                | EntrantStatusValue::NotReady
                                | EntrantStatusValue::Ready
                                | EntrantStatusValue::InProgress
                                    => if let Some(extrapolate_for) = worst_case_extrapolation {
                                        let user = MemberUser::RaceTime {
                                            id: entrant.user.id.clone(),
                                            url: entrant.user.url.clone(),
                                            name: entrant.user.name.clone(),
                                        };
                                        if user == *extrapolate_for {
                                            entrant.status.value = EntrantStatusValue::Dnf;
                                        } else {
                                            entrant.status.value = EntrantStatusValue::Done;
                                            entrant.finish_time = Some(room_data.started_at.and_then(|started_at| (now - started_at).to_std().ok()).unwrap_or_default());
                                        }
                                    },
                                | EntrantStatusValue::Done
                                    => {}
                                | EntrantStatusValue::Declined
                                | EntrantStatusValue::Dnf
                                | EntrantStatusValue::Dq
                                    => entrant.finish_time = None,
                            }
                        }
                        entrants.sort_unstable_by_key(|entrant| (entrant.finish_time.is_none(), entrant.finish_time));
                        let num_entrants = entrants.len();
                        let finish_times = entrants.iter().filter_map(|entrant| entrant.finish_time).collect_vec();
                        let num_finishers = finish_times.len();
                        let par_cutoff = match score_kind {
                            QualifierScoreKind::Standard => 7u8,
                            QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => if num_entrants < 20 { 3 } else { 4 },
                        };
                        if worst_case_extrapolation.is_none() && room_data.status.value != RaceStatusValue::Finished && num_finishers < usize::from(par_cutoff) {
                            continue // scores are not yet accurate
                        }
                        for entrant in entrants {
                            match entrant.status.value {
                                | EntrantStatusValue::Requested
                                | EntrantStatusValue::Invited
                                | EntrantStatusValue::NotReady
                                | EntrantStatusValue::Ready
                                | EntrantStatusValue::InProgress
                                    => continue, // score not yet determined
                                | EntrantStatusValue::Done
                                | EntrantStatusValue::Declined
                                | EntrantStatusValue::Dnf
                                | EntrantStatusValue::Dq
                                    => {}
                            }
                            scores.entry(MemberUser::RaceTime {
                                id: entrant.user.id,
                                url: entrant.user.url,
                                name: entrant.user.name,
                            }).or_default().push(r64(if let Some(finish_time) = entrant.finish_time {
                                match score_kind {
                                    QualifierScoreKind::Standard => {
                                        // https://docs.google.com/document/d/1IHrOGxFQpt3HpQ-9kQ6AVAARc04x6c96N1aHnHfHaKM/edit
                                        let finish_time = TimeDelta::from_std(finish_time).expect("finish time out of range");
                                        let par_cutoff = usize::from(par_cutoff).min(num_entrants);
                                        let par_times = finish_times[0..par_cutoff].iter().map(|&finish_time| TimeDelta::from_std(finish_time).expect("finish time out of range")).collect_vec();
                                        let t_average = par_times.iter().sum::<TimeDelta>() / i32::try_from(par_cutoff).expect("too many entrants");
                                        let t_j_h = TimeDelta::minutes(8).mul_f64(1.0.min(0.0.max((TimeDelta::hours(2) + TimeDelta::minutes(30) - t_average).div_duration_f64(TimeDelta::hours(2) + TimeDelta::minutes(30) - (TimeDelta::hours(1) + TimeDelta::minutes(40))))));
                                        let t_jet = TimeDelta::minutes(8).min(t_j_h.mul_f64(0.0.max((finish_time - t_average).div_duration_f64(TimeDelta::minutes(8)) * 0.35)));
                                        let t_g_h = TimeDelta::from_secs_f64((par_times.iter().map(|finish_time| finish_time.abs_diff(t_average).as_secs_f64().powi(2)).sum::<f64>() / 1.max(par_cutoff - 1) as f64).sqrt());
                                        let sigma_finish = t_g_h.div_duration_f64(t_average);
                                        let t_gamble = TimeDelta::minutes(5).min(t_g_h.mul_f64(0.0.max((finish_time - t_average).div_duration_f64(t_g_h) * 0.0.max(sigma_finish / 0.035 - 1.0) * 0.3)));
                                        ((1.0 - (finish_time - t_average - TimeDelta::minutes(10).min(t_jet + t_gamble)).div_duration_f64(t_average)) * 1000.0).clamp(100.0, 1100.0)
                                    }
                                    QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => {
                                        let par_time = finish_times[0..usize::from(par_cutoff)].iter().sum::<Duration>() / u32::from(par_cutoff);
                                        (100.0 * (2.0 - (finish_time.as_secs_f64() / par_time.as_secs_f64()))).clamp(10.0, 110.0)
                                    }
                                }
                            } else {
                                0.0
                            }));
                        }
                    }
                }
            }
            let teams = Team::for_event(&mut *transaction, data.series, &data.event).await?;
            let scores = if data.is_started(&mut *transaction).await? {
                let mut entrant_scores = Vec::with_capacity(teams.len());
                for team in &teams {
                    let user = team.members(&mut *transaction).await?.into_iter().exactly_one().expect("SGL-style qualifiers in team-based event");
                    let racetime_id = user.racetime.as_ref().expect("SGL-style qualifiers with entrant without racetime.gg account").id.clone();
                    let Some(score) = scores.remove(&MemberUser::RaceTime { id: racetime_id.clone(), url: String::default(), name: String::default() }) else {
                        return Err(cal::Error::UnqualifiedEntrant {
                            series: data.series,
                            event: data.event.to_string(),
                            racetime_id,
                        })
                    };
                    entrant_scores.push((MemberUser::MidosHouse(user), score));
                }
                Either::Left(entrant_scores.into_iter())
            } else {
                let opt_outs = sqlx::query_scalar!("SELECT racetime_id FROM opt_outs WHERE series = $1 AND event = $2", data.series as _, &data.event).fetch_all(&mut **transaction).await?;
                Either::Right(
                    scores.into_iter()
                        .filter(move |(user, _)| match user {
                            MemberUser::RaceTime { id, .. } => !opt_outs.contains(id),
                            MemberUser::MidosHouse(_) | MemberUser::Newcomer => true,
                        })
                )
            };
            let mut signups = Vec::with_capacity(scores.size_hint().0);
            for (user, mut scores) in scores {
                signups.push(SignupsTeam {
                    team: None, //TODO
                    members: vec![SignupsMember {
                        role: Role::None,
                        is_confirmed: match &user {
                            MemberUser::MidosHouse(user) => 'is_confirmed: {
                                for team in &teams {
                                    if team.member_ids(&mut *transaction).await?.contains(&user.id) {
                                        break 'is_confirmed true
                                    }
                                }
                                false
                            }
                            MemberUser::RaceTime { id, .. } => 'is_confirmed: {
                                for team in &teams {
                                    if team.members(&mut *transaction).await?.iter().any(|member| member.racetime.as_ref().is_some_and(|racetime| racetime.id == *id)) {
                                        break 'is_confirmed true
                                    }
                                }
                                false
                            }
                            MemberUser::Newcomer => false, // unused
                        },
                        qualifier_time: None,
                        qualifier_vod: None,
                        user,
                    }],
                    qualification: match score_kind {
                        QualifierScoreKind::Standard => {
                            scores.truncate(8); // only count the first 8 qualifiers chronologically
                            let num_entered = scores.len();
                            scores.retain(|&score| score != 0.0); // only count finished races
                            let num_finished = scores.len();
                            scores.sort_unstable();
                            if num_entered >= 2 {
                                scores.pop(); // remove best score
                            }
                            scores.reverse();
                            scores.truncate(4); // remove up to 3 worst scores
                            Qualification::Multiple {
                                score: scores.iter().copied().sum::<R64>(), // overall score is sum of remaining scores
                                num_entered, num_finished,
                            }
                        }
                        QualifierScoreKind::Sgl2023Online => {
                            scores.truncate(5); // only count the first 5 qualifiers chronologically
                            let num_entered = scores.len();
                            scores.sort_unstable();
                            if num_entered >= 4 {
                                scores.pop(); // remove best score
                            }
                            if num_entered >= 5 {
                                scores.swap_remove(0); // remove worst score
                            }
                            Qualification::Multiple {
                                num_finished: scores.iter().filter(|score| **score != 0.0).count(),
                                score: scores.iter().copied().sum::<R64>() / r64(scores.len().max(3) as f64), // overall score is average of remaining scores
                                num_entered,
                            }
                        }
                        QualifierScoreKind::Sgl2024Online => {
                            scores.truncate(6); // only count the first 6 qualifiers chronologically
                            let num_entered = scores.len();
                            scores.sort_unstable();
                            if num_entered >= 4 {
                                scores.swap_remove(0); // remove worst score
                            }
                            Qualification::Multiple {
                                num_finished: scores.iter().filter(|score| **score != 0.0).count(),
                                score: scores.iter().copied().sum::<R64>() / r64(scores.len().max(3) as f64), // overall score is average of remaining scores
                                num_entered,
                            }
                        }
                    },
                    hard_settings_ok: false,
                    mq_ok: false,
                    lite_ok: false,
                    all_dungeons_ok: false,
                    flute_ok: false,
                    hover_ok: false,
                    inverted_ok: false,
                    keydrop_ok: false,
                    mirror_scroll_ok: false,
                    no_delay_ok: false,
                    pb_ok: false,
                    zw_ok: false,
                });
            }
            signups
        }
        QualifierKind::SongsOfHope => {
            #[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
            enum QualificationLevel {
                Qualified,
                ChoppinBlock,
                #[default]
                None,
            }

            if worst_case_extrapolation.is_some() { unimplemented!("worst-case extrapolation for QualifierKind::SongsOfHope") } //TODO
            let mut entrant_data = HashMap::<_, (u8, _)>::default();
            for race in Race::for_event(transaction, &cache.http_client, data).await? {
                if race.phase.as_ref().is_none_or(|phase| phase != "Qualifier") { continue }
                let Ok(room) = race.rooms().exactly_one() else { continue };
                let room_data = cache.race_data(&room).await?;
                if room_data.status.value != RaceStatusValue::Finished { continue }
                let mut entrants = room_data.entrants.clone();
                entrants.retain(|entrant| entrant_data.entry(MemberUser::RaceTime {
                    id: entrant.user.id.clone(),
                    url: entrant.user.url.clone(),
                    name: entrant.user.name.clone(),
                }).or_default().0 < 2);
                let num_entrants = entrants.len();
                entrants.sort_unstable_by_key(|entrant| (entrant.finish_time.is_none(), entrant.finish_time));
                for (placement, entrant) in entrants.into_iter().enumerate() {
                    let (num_qualifiers, qualification_level) = entrant_data.entry(MemberUser::RaceTime {
                        id: entrant.user.id,
                        url: entrant.user.url,
                        name: entrant.user.name,
                    }).or_default();
                    if *num_qualifiers < 2 {
                        *num_qualifiers += 1;
                        *qualification_level = if placement < 3 {
                            QualificationLevel::Qualified
                        } else if placement < num_entrants / 2 {
                            QualificationLevel::ChoppinBlock
                        } else {
                            QualificationLevel::None
                        }.min(*qualification_level);
                    }
                }
            }
            let num_qualified = entrant_data.values().filter(|(_, qualification_level)| *qualification_level == QualificationLevel::Qualified).count();
            let choppin_block_finished = if_chain! {
                if let Ok(race) = Race::for_event(transaction, &cache.http_client, data).await?.into_iter()
                    .filter(|race| race.phase.as_ref().is_some_and(|phase| phase == "Choppin Block"))
                    .exactly_one();
                if let Ok(room) = race.rooms().exactly_one();
                let room_data = cache.race_data(&room).await?;
                if room_data.status.value == RaceStatusValue::Finished;
                then {
                    let mut entrants = room_data.entrants.clone();
                    entrants.retain(|entrant| entrant_data.entry(MemberUser::RaceTime {
                        id: entrant.user.id.clone(),
                        url: entrant.user.url.clone(),
                        name: entrant.user.name.clone(),
                    }).or_default().1 == QualificationLevel::ChoppinBlock);
                    entrants.sort_unstable_by_key(|entrant| (entrant.finish_time.is_none(), entrant.finish_time));
                    for entrant in entrants.drain(..entrants.len().min(32 - num_qualified)) {
                        entrant_data.entry(MemberUser::RaceTime {
                            id: entrant.user.id,
                            url: entrant.user.url,
                            name: entrant.user.name,
                        }).or_default().1 = QualificationLevel::Qualified;
                    }
                    true
                } else {
                    false
                }
            };
            entrant_data.into_iter()
                .filter(|(_, (_, qualification_level))|
                    *qualification_level == QualificationLevel::Qualified
                    || !choppin_block_finished && *qualification_level == QualificationLevel::ChoppinBlock
                )
                .map(|(user, (_, qualification_level))| SignupsTeam {
                    team: None, //TODO
                    members: vec![SignupsMember {
                        role: Role::None,
                        is_confirmed: false, //TODO
                        qualifier_time: None,
                        qualifier_vod: None,
                        user,
                    }],
                    qualification: Qualification::Single { qualified: qualification_level == QualificationLevel::Qualified },
                    hard_settings_ok: false,
                    mq_ok: false,
                    lite_ok: false,
                    all_dungeons_ok: false,
                    flute_ok: false,
                    hover_ok: false,
                    inverted_ok: false,
                    keydrop_ok: false,
                    mirror_scroll_ok: false,
                    no_delay_ok: false,
                    pb_ok: false,
                    zw_ok: false,
                }).collect()
        }
        QualifierKind::None | QualifierKind::Rank | QualifierKind::Single { .. } => {
            struct TeamRow {
                team: Team,
                hard_settings_ok: bool,
                mq_ok: bool,
                lite_ok: bool,
                pieces: Option<i16>,
                qualified: bool,
                all_dungeons_ok: bool,
                flute_ok: bool,
                hover_ok: bool,
                inverted_ok: bool,
                keydrop_ok: bool,
                mirror_scroll_ok: bool,
                no_delay_ok: bool,
                pb_ok: bool,
                zw_ok: bool,
            }

            let teams = if let QualifierKind::Rank = qualifier_kind {
                // teams are manually ranked so include ones that haven't submitted qualifier asyncs
                sqlx::query!(r#"SELECT id AS "id: Id<Teams>", name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", plural_name, hard_settings_ok, mq_ok, lite_ok, all_dungeons_ok, flute_ok, hover_ok, inverted_ok, keydrop_ok, mirror_scroll_ok, no_delay_ok, pb_ok, zw_ok, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams WHERE
                    series = $1
                    AND event = $2
                    AND NOT resigned
                    AND (
                        EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $3)
                        OR NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                    )
                "#, data.series as _, &data.event, me.as_ref().map(|me| PgSnowflake(me.id)) as _).fetch(&mut **transaction)
                    .map_ok(|row| TeamRow {
                        team: Team {
                            id: row.id,
                            series: data.series,
                            event: data.event.to_string(),
                            name: row.name,
                            racetime_slug: row.racetime_slug,
                            startgg_id: row.startgg_id,
                            plural_name: row.plural_name,
                            restream_consent: row.restream_consent,
                            mw_impl: row.mw_impl,
                            qualifier_rank: row.qualifier_rank,
                        },
                        hard_settings_ok: row.hard_settings_ok,
                        mq_ok: row.mq_ok,
                        lite_ok: row.lite_ok,
                        all_dungeons_ok: row.all_dungeons_ok,
                        flute_ok: row.flute_ok,
                        hover_ok: row.hover_ok,
                        inverted_ok: row.inverted_ok,
                        keydrop_ok: row.keydrop_ok,
                        mirror_scroll_ok: row.mirror_scroll_ok,
                        no_delay_ok: row.no_delay_ok,
                        pb_ok: row.pb_ok,
                        zw_ok: row.zw_ok,
                        pieces: None,
                        qualified: false,
                    })
                    .try_collect::<Vec<_>>().await?
            } else {
                sqlx::query!(r#"SELECT id AS "id: Id<Teams>", name, racetime_slug, startgg_id AS "startgg_id: startgg::ID", plural_name, submitted IS NOT NULL AS "qualified!", pieces, hard_settings_ok, mq_ok, lite_ok, all_dungeons_ok, flute_ok, hover_ok, inverted_ok, keydrop_ok, mirror_scroll_ok, no_delay_ok, pb_ok, zw_ok, restream_consent, mw_impl AS "mw_impl: mw::Impl", qualifier_rank FROM teams LEFT OUTER JOIN async_teams ON (id = team) WHERE
                    series = $1
                    AND event = $2
                    AND NOT resigned
                    AND (
                        $3
                        OR EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $4)
                        OR NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                    )
                    AND (kind = 'qualifier' OR kind IS NULL)
                "#, data.series as _, &data.event, is_organizer && !data.is_started(&mut *transaction).await?, me.as_ref().map(|me| PgSnowflake(me.id)) as _).fetch(&mut **transaction)
                    .map_ok(|row| TeamRow {
                        team: Team {
                            id: row.id,
                            series: data.series,
                            event: data.event.to_string(),
                            name: row.name,
                            racetime_slug: row.racetime_slug,
                            startgg_id: row.startgg_id,
                            plural_name: row.plural_name,
                            restream_consent: row.restream_consent,
                            mw_impl: row.mw_impl,
                            qualifier_rank: row.qualifier_rank,
                        },
                        hard_settings_ok: row.hard_settings_ok,
                        mq_ok: row.mq_ok,
                        lite_ok: row.lite_ok,
                        all_dungeons_ok: row.all_dungeons_ok,
                        flute_ok: row.flute_ok,
                        hover_ok: row.hover_ok,
                        inverted_ok: row.inverted_ok,
                        keydrop_ok: row.keydrop_ok,
                        mirror_scroll_ok: row.mirror_scroll_ok,
                        no_delay_ok: row.no_delay_ok,
                        pb_ok: row.pb_ok,
                        zw_ok: row.zw_ok,
                        pieces: row.pieces,
                        qualified: row.qualified,
                    })
                    .try_collect().await?
            };
            let roles = data.team_config.roles();
            let mut signups = Vec::with_capacity(teams.len());
            for team in teams {
                let mut members = Vec::with_capacity(roles.len());
                for &(role, _) in roles {
                    let row = sqlx::query!(r#"
                        SELECT member AS "id: Id<Users>", status AS "status: SignupStatus", time, vod
                        FROM team_members LEFT OUTER JOIN async_players ON (member = player AND series = $1 AND event = $2 AND kind = 'qualifier')
                        WHERE team = $3 AND role = $4
                    "#, data.series as _, &data.event, team.team.id as _, role as _).fetch_one(&mut **transaction).await?;
                    let is_confirmed = row.status.is_confirmed();
                    let user = User::from_id(&mut **transaction, row.id).await?.ok_or(DataError::NonexistentUser)?;
                    members.push(SignupsMember {
                        user: MemberUser::MidosHouse(user),
                        qualifier_time: row.time.map(decode_pginterval).transpose().map_err(DataError::PgInterval)?,
                        qualifier_vod: row.vod,
                        role, is_confirmed,
                    });
                }
                signups.push(SignupsTeam {
                    team: Some(team.team),
                    qualification: if let Some(pieces) = team.pieces {
                        Qualification::TriforceBlitz { qualified: team.qualified, pieces }
                    } else {
                        Qualification::Single { qualified: team.qualified }
                    },
                    hard_settings_ok: team.hard_settings_ok,
                    mq_ok: team.mq_ok,
                    lite_ok: team.lite_ok,
                    all_dungeons_ok: team.all_dungeons_ok,
                    flute_ok: team.flute_ok,
                    hover_ok: team.hover_ok,
                    inverted_ok: team.inverted_ok,
                    keydrop_ok: team.keydrop_ok,
                    mirror_scroll_ok: team.mirror_scroll_ok,
                    no_delay_ok: team.no_delay_ok,
                    pb_ok: team.pb_ok,
                    zw_ok: team.zw_ok,
                    members,
                });
            }
            signups
        }
    };
    signups.sort_unstable_by(|SignupsTeam { team: team1, members: members1, qualification: qualification1, .. }, SignupsTeam { team: team2, members: members2, qualification: qualification2, .. }| {
        match qualifier_kind {
            QualifierKind::None | QualifierKind::Single { show_times: false } | QualifierKind::SongsOfHope => {
                let qualified1 = match qualification1 {
                    Qualification::Single { qualified } | Qualification::TriforceBlitz { qualified, .. } => qualified,
                    Qualification::Multiple { .. } => unreachable!("Qualification::Multiple in QualifierKind::{{None, Single}}"),
                };
                let qualified2 = match qualification2 {
                    Qualification::Single { qualified } | Qualification::TriforceBlitz { qualified, .. } => qualified,
                    Qualification::Multiple { .. } => unreachable!("Qualification::Multiple in QualifierKind::{{None, Single}}"),
                };
                qualified2.cmp(&qualified1) // reversed to list qualified teams first
                .then_with(|| team1.cmp(&team2))
            }
            QualifierKind::Rank => {
                team1.as_ref().is_none_or(|team1| team1.qualifier_rank.is_none()).cmp(&team2.as_ref().is_none_or(|team2| team2.qualifier_rank.is_none())) // list qualified teams first
                .then_with(|| team1.as_ref().and_then(|team1| team1.qualifier_rank).cmp(&team2.as_ref().and_then(|team2| team2.qualifier_rank)))
                .then_with(|| team1.cmp(&team2))
            }
            QualifierKind::Single { show_times: true } => {
                #[derive(PartialEq, Eq, PartialOrd, Ord)]
                enum QualificationOrder {
                    Finished(Option<i16>, Duration),
                    DidNotFinish,
                    NotYetQualified,
                }

                impl QualificationOrder {
                    fn new(qualification: Qualification, members: &[SignupsMember]) -> Self {
                        match qualification {
                            Qualification::Single { qualified: false } | Qualification::TriforceBlitz { qualified: false, .. } => Self::NotYetQualified,
                            Qualification::Single { qualified: true } => if let Some(time) = members.iter().try_fold(Duration::default(), |acc, member| Some(acc + member.qualifier_time?)) {
                                Self::Finished(None, time)
                            } else {
                                Self::DidNotFinish
                            },
                            Qualification::TriforceBlitz { qualified: true, pieces } => if let Some(time) = members.iter().try_fold(Duration::default(), |acc, member| Some(acc + member.qualifier_time?)) {
                                Self::Finished(
                                    Some(-pieces), // list teams with more pieces first
                                    time,
                                )
                            } else {
                                Self::DidNotFinish
                            },
                            Qualification::Multiple { .. } => unreachable!("Qualification::Multiple in QualifierKind::Single"),
                        }
                    }
                }

                QualificationOrder::new(*qualification1, members1).cmp(&QualificationOrder::new(*qualification2, members2))
                .then_with(|| team1.cmp(&team2))
            }
            QualifierKind::Score(score_kind) => {
                let (num1, score1) = match *qualification1 {
                    Qualification::Multiple { num_entered, num_finished, score } => match score_kind {
                        QualifierScoreKind::Standard => (num_finished, score),
                        QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => (num_entered, score),
                    },
                    _ => unreachable!("QualifierKind::Multiple must use Qualification::Multiple"),
                };
                let (num2, score2) = match *qualification2 {
                    Qualification::Multiple { num_entered, num_finished, score } => match score_kind {
                        QualifierScoreKind::Standard => (num_finished, score),
                        QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => (num_entered, score),
                    },
                    _ => unreachable!("QualifierKind::Multiple must use Qualification::Multiple"),
                };
                let required_qualifiers = match score_kind {
                    QualifierScoreKind::Standard => 5,
                    QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => 3,
                };
                num2.min(required_qualifiers).cmp(&num1.min(required_qualifiers)) // list racers closer to reaching the required number of qualifiers first
                .then_with(|| score2.cmp(&score1)) // list racers with higher scores first
                .then_with(|| members1.iter().map(|member| &member.user).cmp(members2.iter().map(|member| &member.user)))
            }
        }
    });
    Ok(signups)
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] PgInterval(#[from] PgIntervalDecodeError),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("no such event")]
    NoSuchEvent,
}

impl<E: Into<Error>> From<E> for StatusOrError<Error> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

pub(crate) async fn list(pool: &PgPool, http_client: &reqwest::Client, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, ctx: Context<'_>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<Error>> {
    enum ShowStatus {
        Detailed,
        Confirmed,
        None,
    }

    let mut transaction = pool.begin().await?;
    let mut cache = Cache::new(http_client.clone());
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let header = data.header(&mut transaction, me.as_ref(), Tab::Teams, false).await?;
    let mut show_status = ShowStatus::None;
    let is_organizer = if let Some(ref me) = me {
        data.organizers(&mut transaction).await?.contains(me)
    } else {
        false
    };
    let qualifier_kind = match (data.series, &*data.event) {
        (Series::SongsOfHope, "1") => QualifierKind::SongsOfHope,
        (Series::SpeedGaming, "2023onl" | "2024onl") | (Series::Standard, "8") => {
            if is_organizer {
                show_status = ShowStatus::Detailed;
            } else if !data.is_started(&mut transaction).await? && Race::for_event(&mut transaction, http_client, &data).await?.into_iter().all(|race| race.phase.as_ref().is_none_or(|phase| phase != "Qualifier") || race.is_ended()) {
                show_status = ShowStatus::Confirmed;
            }
            QualifierKind::Score(match (data.series, &*data.event) {
                (Series::SpeedGaming, "2023onl") => QualifierScoreKind::Sgl2023Online,
                (Series::SpeedGaming, "2024onl") => QualifierScoreKind::Sgl2024Online,
                (Series::Standard, "8") => QualifierScoreKind::Standard,
                _ => unreachable!("checked by outer match"),
            })
        }
        (_, _) => if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE series = $1 AND event = $2 AND qualifier_rank IS NOT NULL) AS "exists!""#, series as _, event).fetch_one(&mut *transaction).await? {
            QualifierKind::Rank
        } else if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM asyncs WHERE series = $1 AND event = $2 AND kind = 'qualifier') AS "exists!""#, series as _, event).fetch_one(&mut *transaction).await? {
            QualifierKind::Single {
                show_times: data.show_qualifier_times && (
                    sqlx::query_scalar!(r#"SELECT submitted IS NOT NULL AS "qualified!" FROM teams, async_teams, team_members WHERE async_teams.team = teams.id AND teams.series = $1 AND teams.event = $2 AND async_teams.team = team_members.team AND member = $3 AND kind = 'qualifier'"#, series as _, event, me.as_ref().map(|me| PgSnowflake(me.id)) as _).fetch_optional(&mut *transaction).await?.unwrap_or(false)
                    || data.is_started(&mut transaction).await?
                ),
            }
        } else {
            QualifierKind::None
        },
    };
    let show_restream_consent = is_organizer || if let Some(ref me) = me {
        data.restreamers(&mut transaction).await?.contains(me)
    } else {
        false
    };
    let roles = data.team_config.roles();
    let signups = signups_sorted(&mut transaction, &mut Cache::new(http_client.clone()), me.as_ref(), &data, is_organizer, qualifier_kind, None).await?;
    let mut footnotes = Vec::default();
    let teams_label = if let TeamConfig::Solo = data.team_config { "Entrants" } else { "Teams" };
    let mut column_headers = Vec::default();
    if let QualifierKind::Rank | QualifierKind::Score(_) = qualifier_kind {
        column_headers.push(html! {
            th : "Qualifier Rank";
        });
    }
    if !matches!(data.team_config, TeamConfig::Solo) {
        column_headers.push(html! {
            th : "Team Name";
        });
    }
    for &(role, display_name) in roles {
        column_headers.push(html! {
            th(class? = role.css_class().filter(|_| data.team_config.has_distinct_roles())) : display_name;
        });
    }
    match qualifier_kind {
        QualifierKind::None | QualifierKind::Rank => {}
        QualifierKind::Single { show_times: false } | QualifierKind::SongsOfHope => column_headers.push(html! {
            th : "Qualified";
        }),
        QualifierKind::Single { show_times: true } => if series == Series::TriforceBlitz {
            column_headers.push(html! {
                th : "Pieces Found";
            });
        }
        QualifierKind::Score(QualifierScoreKind::Standard) => {
            column_headers.push(html! {
                th : "Qualifiers Entered";
            });
            column_headers.push(html! {
                th : "Qualifiers Finished";
            });
            column_headers.push(html! {
                th : "Qualifier Points";
            });
        }
        QualifierKind::Score(QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online) => {
            column_headers.push(html! {
                th : "Qualifiers Entered";
            });
            column_headers.push(html! {
                th : "Qualifier Points";
            });
        }
    }
    match show_status {
        ShowStatus::Detailed => column_headers.push(html! {
            th : "Status";
        }),
        ShowStatus::Confirmed => column_headers.push(html! {
            th : "Confirmed";
        }),
        ShowStatus::None => {}
    }
    match data.draft_kind() {
        None | Some(draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5) => {}
        Some(draft::Kind::RslS7) => column_headers.push(html! {
            th : "RSL-Lite OK";
        }),
        Some(draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5) => {
            column_headers.push(html! {
                th : "Advanced Settings OK";
            });
            column_headers.push(html! {
                th : "MQ OK";
            });
        }
    }
    if data.series == Series::Crosskeys {
        column_headers.push(html! {
            th : "All Dungeons";
        });
        column_headers.push(html! {
            th : "Flute";
        });
        column_headers.push(html! {
            th : "Inverted";
        });
        column_headers.push(html! {
            th : "Keydrop";
        });
        column_headers.push(html! {
            th : "Mirror Scroll";
        });
        column_headers.push(html! {
            th : "Pseudoboots";
        });
        column_headers.push(html! {
            th : "ZW";
        });
        column_headers.push(html! {
            th : "Hovering/Moldorm Bouncing";
        });
        column_headers.push(html! {
            th : "No Delay";
        });
    }
    if show_restream_consent {
        column_headers.push(html! {
            th : "Restream Consent";
        });
        if let TeamConfig::Multiworld = data.team_config {
            column_headers.push(html! {
                th : "Multiworld Plugin";
            });
        }
    }
    let content = html! {
        : header;
        table {
            thead {
                tr {
                    @for header in &column_headers {
                        : header;
                    }
                }
            }
            tbody {
                @if signups.is_empty() {
                    tr {
                        td(colspan = column_headers.len()) {
                            i : "(no signups yet)";
                        }
                    }
                } else {
                    @for (signup_idx, SignupsTeam { team, members, qualification, hard_settings_ok, mq_ok, lite_ok, all_dungeons_ok, flute_ok, hover_ok, inverted_ok, keydrop_ok, mirror_scroll_ok, no_delay_ok, pb_ok, zw_ok }) in signups.into_iter().enumerate() {
                        @let is_dimmed = match qualifier_kind {
                            QualifierKind::None => false,
                            QualifierKind::Rank => false, // unknown cutoff
                            QualifierKind::Single { .. } => false, // need to be qualified to be listed
                            QualifierKind::Score(QualifierScoreKind::Standard) => {
                                let Qualification::Multiple { num_finished, .. } = qualification else { unreachable!("qualification kind mismatch") };
                                num_finished < 5
                            }
                            QualifierKind::Score(QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online) => {
                                let Qualification::Multiple { num_entered, .. } = qualification else { unreachable!("qualification kind mismatch") };
                                num_entered < 3
                            }
                            QualifierKind::SongsOfHope => false, //TODO
                        };
                        tr(class? = is_dimmed.then_some("dimmed")) {
                            @match qualifier_kind {
                                QualifierKind::Rank => td : team.as_ref().and_then(|team| team.qualifier_rank);
                                QualifierKind::Score(_) => td : signup_idx + 1;
                                _ => {}
                            }
                            @if !matches!(data.team_config, TeamConfig::Solo) {
                                td {
                                    @if let Some(ref team) = team {
                                        : team.to_html(&mut transaction, false).await?;
                                    }
                                    @if let (QualifierKind::Single { show_times: true }, Qualification::Single { qualified: true } | Qualification::TriforceBlitz { qualified: true, .. }) = (qualifier_kind, qualification) {
                                        br;
                                        small {
                                            @if let Some(time) = members.iter().try_fold(Duration::default(), |acc, member| Some(acc + member.qualifier_time?)) {
                                                : English.format_duration(time / u32::try_from(members.len()).expect("too many team members"), false);
                                            } else {
                                                : "DNF";
                                            }
                                        }
                                    }
                                }
                            }
                            @for SignupsMember { role, user, is_confirmed, qualifier_time, qualifier_vod } in &members {
                                td(class? = role.css_class().filter(|_| data.team_config.has_distinct_roles())) {
                                    @match user {
                                        MemberUser::MidosHouse(user) => {
                                            : user;
                                            @if let Some(ref team) = team {
                                                @if *is_confirmed {
                                                    @if me.as_ref().is_some_and(|me| me == user) && members.iter().any(|member| !member.is_confirmed) {
                                                        : " ";
                                                        @let (errors, button) = button_form_ext(uri!(crate::event::resign_post(series, event, team.id)), csrf.as_ref(), ctx.errors().collect(), event::ResignFormSource::Teams, "Retract");
                                                        : errors;
                                                        span(class = "button-row") : button;
                                                    }
                                                } else {
                                                    : " ";
                                                    @if me.as_ref().is_some_and(|me| me == user) {
                                                        @let (errors, accept_button) = button_form_ext(uri!(crate::event::confirm_signup(series, event, team.id)), csrf.as_ref(), ctx.errors().collect(), event::AcceptFormSource::Teams, "Accept");
                                                        : errors;
                                                        @let (errors, decline_button) = button_form_ext(uri!(crate::event::resign_post(series, event, team.id)), csrf.as_ref(), Vec::default(), event::ResignFormSource::Teams, "Decline");
                                                        : errors;
                                                        span(class = "button-row") {
                                                            : accept_button;
                                                            : decline_button;
                                                            //TODO options to block sender or event
                                                        }
                                                    } else {
                                                        : "(unconfirmed)";
                                                    }
                                                }
                                            }
                                            @if let (QualifierKind::Single { show_times: true }, Qualification::Single { qualified: true } | Qualification::TriforceBlitz { qualified: true, .. }) = (qualifier_kind, qualification) {
                                                br;
                                                small {
                                                    @let time = if let Some(time) = qualifier_time { English.format_duration(*time, false) } else { format!("DNF") };
                                                    @if let Some(vod) = qualifier_vod {
                                                        @if let Some(Ok(vod_url)) = (!vod.contains(' ')).then(|| Url::parse(vod)) {
                                                            a(href = vod_url.to_string()) : time;
                                                        } else {
                                                            : time;
                                                            sup {
                                                                @let footnote_id = { footnotes.push(vod.clone()); footnotes.len() };
                                                                a(href = format!("#footnote{footnote_id}")) {
                                                                    : "[";
                                                                    : footnote_id;
                                                                    : "]";
                                                                }
                                                            };
                                                        }
                                                    } else {
                                                        : time;
                                                    }
                                                }
                                            }
                                        }
                                        MemberUser::RaceTime { url, name, .. } => a(href = format!("https://{}{url}", racetime_host())) : name;
                                        MemberUser::Newcomer => @unreachable // only returned if signups_sorted is called with worst_case_extrapolation = true, which it isn't above
                                    }
                                }
                            }
                            @match (qualifier_kind, qualification) {
                                (QualifierKind::None, _) | (QualifierKind::Rank, _) | (QualifierKind::Single { show_times: true }, Qualification::Single { .. }) => {}
                                (QualifierKind::Single { show_times: false } | QualifierKind::SongsOfHope, Qualification::Single { qualified } | Qualification::TriforceBlitz { qualified, .. }) => td {
                                    @if qualified {
                                        : "✓";
                                    }
                                }
                                (QualifierKind::Single { show_times: true }, Qualification::TriforceBlitz { pieces, .. }) => td : pieces;
                                (QualifierKind::Score(QualifierScoreKind::Standard), Qualification::Multiple { num_entered, num_finished, score }) => {
                                    td(style = "text-align: right;") : num_entered;
                                    td(style = "text-align: right;") : num_finished;
                                    td(style = "text-align: right;") : format!("{score:.2}");
                                }
                                (QualifierKind::Score(QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online), Qualification::Multiple { num_entered, num_finished: _, score }) => {
                                    td(style = "text-align: right;") : num_entered;
                                    td(style = "text-align: right;") : format!("{score:.2}");
                                }
                                (_, _) => @unreachable
                            }
                            @match show_status {
                                ShowStatus::Detailed => td {
                                    @if let Some(unconfirmed) = members.iter().filter(|member| !member.is_confirmed).try_into_nonempty_iter() {
                                        @if let Ok(entrant) = members.iter().exactly_one() {
                                            @if let Some(flow) = &data.enter_flow {
                                                @for requirement in &flow.requirements {
                                                    @match requirement {
                                                        enter::Requirement::RaceTime => {}
                                                        enter::Requirement::RaceTimeInvite { .. } => {}
                                                        enter::Requirement::Twitch => {}
                                                        enter::Requirement::Discord => {}
                                                        enter::Requirement::DiscordGuild { .. } => {}
                                                        enter::Requirement::Challonge => {}
                                                        enter::Requirement::StartGG { .. } => {}
                                                        enter::Requirement::TextField { .. } => {}
                                                        enter::Requirement::TextField2 { .. } => {}
                                                        enter::Requirement::YesNo { .. } => {}
                                                        enter::Requirement::Rules { .. } => {}
                                                        enter::Requirement::AllDungeonsOk => {}
                                                        enter::Requirement::FluteOk => {}
                                                        enter::Requirement::HardSettingsOk => {}
                                                        enter::Requirement::HoveringOk => {}
                                                        enter::Requirement::InvertedOk => {}
                                                        enter::Requirement::KeydropOk => {}
                                                        enter::Requirement::LiteOk => {}
                                                        enter::Requirement::MirrorScrollOk => {}
                                                        enter::Requirement::MqOk => {}
                                                        enter::Requirement::NoDelayOk => {}
                                                        enter::Requirement::PbOk => {}
                                                        enter::Requirement::ZwOk => {}
                                                        enter::Requirement::RestreamConsent { .. } => {}
                                                        enter::Requirement::Qualifier { .. } => {} //TODO
                                                        enter::Requirement::TripleQualifier { .. } => {} //TODO
                                                        enter::Requirement::QualifierPlacement { num_players, min_races, event, exclude_players } => : if_chain! {
                                                            let data = if let Some(event) = event {
                                                                &Data::new(&mut transaction, data.series, event).await?.ok_or(Error::NoSuchEvent)?
                                                            } else {
                                                                &data
                                                            };
                                                            let qualifier_kind = match (data.series, &*data.event) {
                                                                (Series::SpeedGaming, "2023onl") => QualifierScoreKind::Sgl2023Online,
                                                                (Series::SpeedGaming, "2024onl") => QualifierScoreKind::Sgl2024Online,
                                                                (Series::Standard, "8") => QualifierScoreKind::Standard,
                                                                (_, _) => unimplemented!("enter::Requirement::QualifierPlacement for event {}/{}", data.series.slug(), data.event),
                                                            };
                                                            let teams = signups_sorted(&mut transaction, &mut cache, None, data, is_organizer, QualifierKind::Score(qualifier_kind), Some(&entrant.user)).await?;
                                                            if let Some((placement, team)) = teams.iter().enumerate().find(|(_, team)| team.members.iter().any(|member| member.user == entrant.user));
                                                            if let Qualification::Multiple { num_entered, num_finished, .. } = team.qualification;
                                                            let num_qualifiers = match qualifier_kind {
                                                                QualifierScoreKind::Standard => num_finished,
                                                                QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => num_entered,
                                                            };
                                                            then {
                                                                if num_qualifiers < *min_races {
                                                                    html! {
                                                                        : "Not eligible (needs ";
                                                                        : min_races - num_qualifiers;
                                                                        @match qualifier_kind {
                                                                            QualifierScoreKind::Standard => {
                                                                                : " more finish";
                                                                                @if min_races - num_qualifiers != 1 {
                                                                                    : "es";
                                                                                }
                                                                            }
                                                                            QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online => {
                                                                                : " more race";
                                                                                @if min_races - num_qualifiers != 1 {
                                                                                    : "s";
                                                                                }
                                                                            }
                                                                        }
                                                                        : ")";
                                                                    }
                                                                } else if teams.iter()
                                                                    .enumerate()
                                                                    .find(|(_, team)| team.members.iter().any(|member| member.user == MemberUser::Newcomer))
                                                                    .is_some_and(|(newcomer_placement, _)| placement >= newcomer_placement)
                                                                {
                                                                    html! {
                                                                        : "Not eligible (can still be beaten by newcomers)";
                                                                    }
                                                                } else if placement >= *num_players {
                                                                    html! {
                                                                        : "Not eligible (worst-case placement: ";
                                                                        : placement + 1;
                                                                        : ")";
                                                                    }
                                                                } else if teams.iter()
                                                                    .take(*exclude_players)
                                                                    .any(|team| team.members.iter().any(|member| !member.is_confirmed))
                                                                {
                                                                    html! {
                                                                        : "Not eligible (may still overqualify due to opt-outs)";
                                                                    }
                                                                } else {
                                                                    html! {
                                                                        : "Eligible (worst-case placement: ";
                                                                        : placement + 1;
                                                                        : ")";
                                                                    }
                                                                }
                                                            } else {
                                                                html! {
                                                                    : "Error validating qualifier placement";
                                                                }
                                                            }
                                                        };
                                                        enter::Requirement::RslLeaderboard => {}
                                                        enter::Requirement::External { .. } => {}
                                                    }
                                                }
                                            } else {
                                                : "Signups not open";
                                            }
                                        } else {
                                            : "Missing confirmation from ";
                                            : English.join_html(unconfirmed.map(|member| html! {
                                                @match &member.user {
                                                    MemberUser::MidosHouse(user) => : user;
                                                    MemberUser::RaceTime { url, name, .. } => a(href = format!("https://{}{url}", racetime_host())) : name;
                                                    MemberUser::Newcomer => @unreachable // only returned if signups_sorted is called with worst_case_extrapolation = true, which it isn't above
                                                }
                                            }));
                                        }
                                    } else {
                                        : "Confirmed";
                                    }
                                }
                                ShowStatus::Confirmed => td {
                                    @if members.iter().all(|member| member.is_confirmed) {
                                        : "✓";
                                    }
                                }
                                ShowStatus::None => {}
                            }
                            @match data.draft_kind() {
                                None | Some(draft::Kind::S7 | draft::Kind::MultiworldS3 | draft::Kind::MultiworldS4 | draft::Kind::MultiworldS5) => {}
                                Some(draft::Kind::RslS7) => td {
                                    @if lite_ok {
                                        : "✓";
                                    }
                                }
                                Some(draft::Kind::TournoiFrancoS3 | draft::Kind::TournoiFrancoS4 | draft::Kind::TournoiFrancoS5) => {
                                    td {
                                        @if hard_settings_ok {
                                            : "✓";
                                        }
                                    }
                                    td {
                                        @if mq_ok {
                                            : "✓";
                                        }
                                    }
                                }
                            }
                            @if data.series == Series::Crosskeys {
                                td {
                                    @if (all_dungeons_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (flute_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (inverted_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (keydrop_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (mirror_scroll_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (pb_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (zw_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (hover_ok) {
                                        : "✓";
                                    }
                                }
                                td {
                                    @if (no_delay_ok) {
                                        : "✓";
                                    }
                                }

                            }
                            @if show_restream_consent {
                                td {
                                    @if team.as_ref().is_some_and(|team| team.restream_consent) {
                                        : "✓";
                                    }
                                }
                                @if let TeamConfig::Multiworld = data.team_config {
                                    td {
                                        @if let Some(team) = team {
                                            @match team.mw_impl {
                                                None => : "?";
                                                Some(mw::Impl::BizHawkCoOp) => : "bizhawk-co-op";
                                                Some(mw::Impl::MidosHouse) => : "MH MW";
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
        @for (i, footnote) in footnotes.into_iter().enumerate() {
            p(id = format!("footnote{}", i + 1)) {
                : "[";
                : i + 1;
                : "]";
                @for word in footnote.split(' ') {
                    : " ";
                    @if let Ok(word_url) = Url::parse(word) {
                        a(href = word_url.to_string()) : word;
                    } else {
                        : word;
                    }
                }
            }
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("{teams_label} — {}", data.display_name), content).await?)
}

#[rocket::get("/event/<series>/<event>/teams")]
pub(crate) async fn get(pool: &State<PgPool>, http_client: &State<reqwest::Client>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str) -> Result<RawHtml<String>, StatusOrError<Error>> {
    list(pool, http_client, me, uri, csrf, Context::default(), series, event).await
}
