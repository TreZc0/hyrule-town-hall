use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

/// Rate limit once per minute according to DMs with tsigma6
const RATE_LIMIT: Duration = Duration::from_secs(60);

static CACHE: LazyLock<Mutex<(Instant, Schedule)>> = LazyLock::new(|| Mutex::new((Instant::now() + RATE_LIMIT, Schedule::default())));

#[derive(Clone, Deserialize)]
pub(crate) struct RestreamMatch {
    players: Vec<Player>,
    pub(crate) id: i64,
    pub(crate) title: String,
}

impl RestreamMatch {
    pub(crate) async fn matches(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, race: &Race) -> Result<bool, cal::Error> {
        Ok(if race.phase.as_ref().is_some_and(|phase| phase == "Qualifier") {
            let Some((_, match_round)) = regex_captures!("^Qualifier #([0-9]+)$", &self.title) else { return Ok(false) };
            race.round.as_ref().is_some_and(|race_round| race_round == match_round)
        } else {
            match &race.entrants {
                Entrants::Open | Entrants::Count { .. } | Entrants::Named(_) => false,
                Entrants::Two(entrants) => {
                    if self.players.len() == 2 {
                        for players in self.players.iter().permutations(2) {
                            for (entrant, player) in entrants.iter().zip_eq(players) {
                                if !player.matches(&mut *transaction, http_client, entrant).await? {
                                    return Ok(false)
                                }
                            }
                            return Ok(true)
                        }
                    }
                    false
                }
                Entrants::Three(entrants) => {
                    if self.players.len() == 2 {
                        for players in self.players.iter().permutations(3) {
                            for (entrant, player) in entrants.iter().zip_eq(players) {
                                if !player.matches(&mut *transaction, http_client, entrant).await? {
                                    return Ok(false)
                                }
                            }
                            return Ok(true)
                        }
                    }
                    false
                }
            }
        })
    }
}

impl fmt::Display for RestreamMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.title.is_empty() {
            write!(f, "{}", self.players.iter().map(|player| &player.streaming_from).format(" vs "))
        } else {
            self.title.fmt(f)
        }
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct RestreamChannel {
    pub(crate) language: Language,
    pub(crate) slug: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Player {
    streaming_from: String,
}

impl Player {
    async fn matches(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, entrant: &Entrant) -> Result<bool, cal::Error> {
        Ok(match entrant {
            Entrant::MidosHouseTeam(team) => if_chain! {
                if let Ok(member) = team.members(transaction).await?.into_iter().exactly_one();
                if let Some(Some(user_data)) = member.racetime_user_data(http_client).await?;
                if let Some(twitch_name) = user_data.twitch_name;
                then {
                    twitch_name.eq_ignore_ascii_case(&self.streaming_from)
                } else {
                    false
                }
            },
            Entrant::Discord { twitch_username: None, .. } | Entrant::Named { twitch_username: None, .. } => false,
            Entrant::Discord { twitch_username: Some(username), .. } | Entrant::Named { twitch_username: Some(username), .. } => username.eq_ignore_ascii_case(&self.streaming_from),
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Restream {
    pub(crate) match1: Option<RestreamMatch>,
    pub(crate) match2: Option<RestreamMatch>,
    pub(crate) channels: Vec<RestreamChannel>,
    pub(crate) when_countdown: DateTime<Utc>,
}

impl Restream {
    pub(crate) fn matches(&self) -> impl Iterator<Item = &RestreamMatch> {
        self.match1.iter().chain(&self.match2)
    }

    pub(crate) fn update_race(&self, race: &mut Race, id: i64) -> Result<(), url::ParseError> {
        if !race.cal_events().any(|cal_event| cal_event.room().is_some()) { // don't mess with starting time if room already open
            assert!(matches!(mem::replace(&mut race.source, cal::Source::SpeedGaming { id }), cal::Source::Manual | cal::Source::SpeedGaming { id: _ }));
            race.schedule.set_live_start(self.when_countdown);
            //TODO if schedule changed, post notice in scheduling thread, open room if short notice
        }
        if !race.schedule_locked {
            for channel in &self.channels {
                if let hash_map::Entry::Vacant(entry) = race.video_urls.entry(channel.language) {
                    let video_url = Url::parse(&format!("https://twitch.tv/{}", channel.slug))?;
                    entry.insert(video_url);
                }
                //TODO register restreamer, if any
            }
        }
        Ok(())
    }
}

pub(crate) type Schedule = Vec<Restream>;

pub(crate) async fn schedule(http_client: &reqwest::Client, event_slug: &str) -> wheel::Result<Schedule> {
    let now = Utc::now();
    lock!(cache = CACHE; {
        let (ref mut next_request, ref mut cache) = *cache;
        if *next_request <= Instant::now() {
            *cache = http_client.get("https://speedgaming.org/api/schedule")
                .query(&[
                    ("event", event_slug),
                    ("from", &now.to_rfc3339()), // no need to look for races created in the past minute since filters by start time with stream delay
                    ("to", &(now + TimeDelta::days(365)).to_rfc3339()), // required because the default is some very short interval (less than 1 week)
                ])
                .send().await?
                .detailed_error_for_status().await?
                .json_with_text_in_error().await?;
            *next_request = Instant::now() + RATE_LIMIT;
        }
        Ok(cache.clone())
    })
}

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "2023onl" => {
            Some(html! {
                article {
                    p {
                        : "Welcome to the 2023 SpeedGaming Live online OoTR tournament, organized by ";
                        : English.join_html_opt(data.organizers(&mut *transaction).await?);
                        : ".";
                        h2 : "See also";
                        ul {
                            li {
                                a(href = "https://docs.google.com/document/d/1EACqBl8ZOreD6xT5jQ2HrdLOnpBpKyjS3FUYK8XFeqg/edit") : "Rules document";
                            }
                            li {
                                a(href = "https://sglive.speedgaming.org/") : "Main SGL event page";
                            }
                            li {
                                a(href = "https://discord.gg/YGzQsUp") : "Main SGL Discord";
                            }
                        }
                    }
                }
            })
        }
        "2023live" => {
            Some(html! {
                article {
                    p {
                        : "Welcome to the 2023 SpeedGaming Live in-person OoTR tournament, organized by ";
                        : English.join_html_opt(data.organizers(&mut *transaction).await?);
                        : ".";
                        h2 : "See also";
                        ul {
                            li {
                                a(href = "https://docs.google.com/document/d/1EACqBl8ZOreD6xT5jQ2HrdLOnpBpKyjS3FUYK8XFeqg/edit") : "Rules document";
                            }
                            li {
                                a(href = "https://sglive.speedgaming.org/") : "Main SGL event page";
                            }
                            li {
                                a(href = "https://matcherino.com/t/sglive23") : "Matcherino";
                            }
                            li {
                                a(href = "https://discord.gg/YGzQsUp") : "Main SGL Discord";
                            }
                        }
                    }
                }
            })
        }
        "2024onl" => {
            Some(html! {
                article {
                    p {
                        : "Welcome to the 2024 SpeedGaming Live online OoTR tournament, organized by ";
                        : English.join_html_opt(data.organizers(&mut *transaction).await?);
                        : ".";
                        h2 : "See also";
                        ul {
                            li {
                                a(href = "https://docs.google.com/document/d/1I0IcnGMqKr3QaCgg923SR_SxVu0iytIA_lOhN2ybj9w/edit") : "Rules document";
                            }
                            li {
                                a(href = "https://sglive.speedgaming.org/") : "Main SGL event page";
                            }
                            li {
                                a(href = "https://discord.gg/YGzQsUp") : "Main SGL Discord";
                            }
                        }
                    }
                }
            })
        }
        "2024live" => {
            Some(html! {
                article {
                    p {
                        : "Welcome to the 2024 SpeedGaming Live in-person OoTR tournament, organized by ";
                        : English.join_html_opt(data.organizers(&mut *transaction).await?);
                        : ".";
                        h2 : "See also";
                        ul {
                            li {
                                a(href = "https://docs.google.com/document/d/1I0IcnGMqKr3QaCgg923SR_SxVu0iytIA_lOhN2ybj9w/edit") : "Rules document";
                            }
                            li {
                                a(href = "https://sglive.speedgaming.org/") : "Main SGL event page";
                            }
                            li {
                                a(href = "https://matcherino.com/t/sglive24") : "Matcherino";
                            }
                            li {
                                a(href = "https://discord.gg/YGzQsUp") : "Main SGL Discord";
                            }
                        }
                    }
                }
            })
        }
        "2025onl" => {
            Some(html! {
                article {
                    p {
                        : "Welcome to the 2025 SpeedGaming Live online OoTR tournament, organized by ";
                        : English.join_html_opt(data.organizers(&mut *transaction).await?);
                        : ".";
                        h2 : "See also";
                        ul {
                            li {
                                a(href = "https://sglive.speedgaming.org/") : "Main SGL event page";
                            }
                            li {
                                a(href = "https://discord.gg/YGzQsUp") : "Main SGL Discord";
                            }
                        }
                    }
                }
            })
        }
        "2025live" => {
            Some(html! {
                article {
                    p {
                        : "Welcome to the 2025 SpeedGaming Live in-person OoTR tournament, organized by ";
                        : English.join_html_opt(data.organizers(&mut *transaction).await?);
                        : ".";
                        h2 : "See also";
                        ul {
                            li {
                                a(href = "https://sglive.speedgaming.org/") : "Main SGL event page";
                            }
                            li {
                                a(href = "https://matcherino.com/t/sglive25") : "Matcherino";
                            }
                            li {
                                a(href = "https://discord.gg/YGzQsUp") : "Main SGL Discord";
                            }
                        }
                    }
                }
            })
        }
        _ => None,
    })
}
