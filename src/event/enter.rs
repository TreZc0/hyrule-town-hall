use {
    lazy_regex::Regex,
    racetime::model::EntrantStatusValue,
    serde_with::DeserializeAs,
    crate::{
        discord_bot::ADMIN_USER,
        event::{
            Data,
            DataError,
            Role,
            SignupStatus,
            Tab,
            teams,
        },
        prelude::*,
    },
};

#[derive(Debug, Clone, Deserialize)]
pub(super) struct Flow {
    pub(crate) requirements: Vec<Requirement>,
    closes: Option<DateTime<Utc>>,
}

enum DeserializeRawHtml {}

impl<'de> DeserializeAs<'de, RawHtml<String>> for DeserializeRawHtml {
    fn deserialize_as<D: Deserializer<'de>>(deserializer: D) -> Result<RawHtml<String>, D::Error> {
        String::deserialize(deserializer).map(RawHtml)
    }
}

enum DeserializeRegex {}

impl<'de> DeserializeAs<'de, Regex> for DeserializeRegex {
    fn deserialize_as<D: Deserializer<'de>>(deserializer: D) -> Result<Regex, D::Error> {
        Regex::new(<&str>::deserialize(deserializer)?).map_err(|e| D::Error::custom(e.to_string()))
    }
}

fn make_true() -> bool { true }

/// Requirements to enter an event
#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum Requirement {
    /// Must have a racetime.gg account connected to their Mido's House account
    RaceTime,
    /// Must be on a list of invited racetime.gg users
    #[serde(rename_all = "camelCase")]
    RaceTimeInvite {
        invites: HashSet<String>,
        #[serde(default)]
        #[serde_as(as = "Option<DeserializeRawHtml>")]
        text: Option<RawHtml<String>>,
        error_text: Option<String>,
    },
    /// Must have a Twitch account connected to their racetime.gg account
    Twitch,
    /// Must have a Discord account connected to their Mido's House account
    Discord,
    /// Must be in the event's Discord guild
    DiscordGuild {
        name: String,
    },
    /// Must have a Challonge account connected to their Mido's House account
    Challonge,
    /// Must have a start.gg account connected to their Mido's House account
    StartGG {
        #[serde(default)]
        optional: bool,
    },
    /// Must fill a custom text field
    #[serde(rename_all = "camelCase")]
    TextField {
        #[serde_as(as = "DeserializeRawHtml")]
        label: RawHtml<String>,
        #[serde(default)]
        long: bool,
        #[serde_as(as = "DeserializeRegex")]
        regex: Regex,
        #[serde_as(as = "serde_with::Map<DeserializeRegex, _>")]
        regex_error_messages: Vec<(Regex, String)>,
        fallback_error_message: String,
    },
    /// Must fill a custom text field
    #[serde(rename_all = "camelCase")]
    TextField2 {
        #[serde_as(as = "DeserializeRawHtml")]
        label: RawHtml<String>,
        #[serde(default)]
        long: bool,
        #[serde_as(as = "DeserializeRegex")]
        regex: Regex,
        #[serde_as(as = "serde_with::Map<DeserializeRegex, _>")]
        regex_error_messages: Vec<(Regex, String)>,
        fallback_error_message: String,
    },
    /// Must answer a custom yes/no question
    YesNo {
        #[serde_as(as = "DeserializeRawHtml")]
        label: RawHtml<String>,
    },
    /// Must answer a custom yes/no question, stored in `custom_choices`
    #[serde(rename_all = "camelCase")]
    BooleanChoice {
        key: String,
        #[serde_as(as = "DeserializeRawHtml")]
        label: RawHtml<String>,
    },
    /// Must agree to the event rules
    Rules {
        document: Option<Url>,
    },
    /// Must submit a response to a custom poll
    Poll {
        document: Option<Url>,
    },
    /// For Crosskeys, opt in to or out of "All Dungeons" as a goal
    AllDungeonsOk,
    /// For Tournoi Francophone-style settings drafts, opt in to or out of hard settings
    HardSettingsOk,
    /// For Crosskeys, opt in to or out of hovering being allowed
    HoveringOk,
    /// For Crosskeys, opt in to or out of "Inverted" as a world state
    InvertedOk,
    /// For Crosskeys, opt in to or out of starting with an activate flute.
    FluteOk,
    /// For Crosskeys, opt in to or out of "Keydrop" as an item setting
    KeydropOk,
    /// For Crosskeys, opt in to or out of mirror scroll
    MirrorScrollOk,
    /// For Tournoi Francophone-style settings drafts, opt in to or out of Master Quest
    MqOk,
    /// For Crosskeys, opt in to or out of waiving the tournament required stream delay.
    NoDelayOk,
    /// For RSL-style weights drafts, opt into RSL-Lite weights
    LiteOk,
    /// For Crosskeys, opt in to or out of pseudoboots
    PbOk,
    /// For Crosskeys, opt in to or out of playing with ZW (formerly Zelgawoods).
    ZwOk,
    /// Must agree to be restreamed
    RestreamConsent {
        #[serde(default)]
        optional: bool,
        #[serde(default)]
        #[serde_as(as = "Option<DeserializeRawHtml>")]
        note: Option<RawHtml<String>>,
    },
    /// Must either request and submit the qualifier seed as an async, or participate in the live qualifier
    #[serde(rename_all = "camelCase")]
    Qualifier {
        async_start: DateTime<Utc>,
        async_end: DateTime<Utc>,
        live_start: DateTime<Utc>,
    },
    /// Must either request and submit at least one of the qualifier seeds as an async, or participate in at least one of the live qualifiers
    #[serde(rename_all = "camelCase")]
    TripleQualifier {
        async_starts: [DateTime<Utc>; 3],
        async_ends: [DateTime<Utc>; 3],
        live_starts: [DateTime<Utc>; 3],
    },
    /// Must place within the top n players after all races in the `Qualifier` phase
    #[serde(rename_all = "camelCase")]
    QualifierPlacement {
        num_players: usize,
        #[serde(default)]
        min_races: usize,
        /// If this is set, a DNF does not count towards the minimum required races.
        #[serde(default)]
        need_finish: bool,
        /// Check qualifiers for a different event in this series
        event: Option<String>,
        /// Must not place within this range, e.g. for Challenge Cup
        #[serde(default)]
        exclude_players: usize,
    },
    /// Must finish at least 3 races on the RSL leaderboard for this season
    RslLeaderboard,
    /// A signup requirement that cannot be checked automatically
    #[serde(rename_all = "camelCase")]
    External {
        #[serde(default)]
        #[serde_as(as = "Option<DeserializeRawHtml>")]
        html: Option<RawHtml<String>>,
        text: Option<String>,
        #[serde(default = "make_true")]
        blocks_submit: bool,
    },
}

struct RequirementStatus {
    blocks_submit: bool,
    html_content: Box<dyn FnOnce(&mut Vec<&form::Error<'_>>) -> RawHtml<String> + Send>,
}

impl Requirement {
    async fn is_checked(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, discord_ctx: &RwFuture<DiscordCtx>, me: &User, data: &Data<'_>) -> Result<Option<bool>, Error> {
        let mut cache = teams::Cache::new(http_client.clone());
        Ok(match self {
            Self::RaceTime => Some(me.racetime.is_some()),
            Self::RaceTimeInvite { invites, .. } => Some(me.racetime.as_ref().is_some_and(|racetime| invites.contains(&racetime.id))),
            Self::Twitch => Some(if let Some(Some(racetime_user_data)) = me.racetime_user_data(http_client).await? {
                racetime_user_data.twitch_channel.is_some()
            } else {
                false
            }),
            Self::Discord => Some(me.discord.is_some()),
            Self::DiscordGuild { .. } => Some({
                let discord_guild = data.discord_guild.ok_or(Error::DiscordGuild)?;
                if let Some(ref discord) = me.discord {
                    discord_guild.member(&*discord_ctx.read().await, discord.id).await.is_ok()
                } else {
                    false
                }
            }),
            Self::Challonge => Some(me.challonge_id.is_some()),
            Self::StartGG { .. } => Some(me.startgg_id.is_some()),
            Self::TextField { .. } => Some(false),
            Self::TextField2 { .. } => Some(false),
            Self::YesNo { .. } => Some(false),
            Self::BooleanChoice { .. } => Some(false),
            Self::Rules { .. } => Some(false),
            Self::Poll { .. } => Some(false),
            Self::AllDungeonsOk { .. } => Some(false),
            Self::FluteOk { .. } => Some(false),
            Self::HardSettingsOk { .. } => Some(false),
            Self::HoveringOk { .. } => Some(false),
            Self::InvertedOk { .. } => Some(false),
            Self::KeydropOk { .. } => Some(false),
            Self::LiteOk { .. } => Some(false),
            Self::MirrorScrollOk { .. } => Some(false),
            Self::MqOk { .. } => Some(false),
            Self::NoDelayOk { .. } => Some(false),
            Self::PbOk { .. } => Some(false),
            Self::ZwOk { .. } => Some(false),
            Self::RestreamConsent { .. } => Some(false),
            Self::Qualifier { .. } => Some(false),
            Self::TripleQualifier { .. } => Some('checked: {
                if let Some(racetime) = &me.racetime {
                    for race in Race::for_event(transaction, http_client, data).await? {
                        if race.phase.as_ref().is_some_and(|phase| phase == "Live Qualifier") {
                            if let Ok(room) = race.rooms().exactly_one() {
                                let room_data = cache.race_data(&room).await?;
                                if room_data.entrants.iter().any(|entrant| entrant.status.value == EntrantStatusValue::Done && entrant.user.as_ref().unwrap().id == racetime.id) {
                                    break 'checked true
                                }
                            }
                        }
                    }
                }
                false
            }),
            Self::QualifierPlacement { num_players, min_races, need_finish, event, exclude_players } => Some(if_chain! {
                let data = if let Some(event) = event {
                    &Data::new(&mut *transaction, data.series, event).await?.ok_or(Error::NoSuchEvent)?
                } else {
                    data
                };
                let qualifier_kind = data.qualifier_kind(&mut *transaction, None).await?;
                // call signups_sorted with worst_case_extrapolation = true to calculate whether the player has secured a spot ahead of time
                let teams = teams::signups_sorted(transaction, &mut cache, None, data, false, qualifier_kind, Some(&teams::MemberUser::MidosHouse(me.clone()))).await?;
                if let Some((placement, team)) = teams.iter().enumerate().find(|(_, team)| team.members.iter().any(|member| member.user == *me));
                if let teams::Qualification::Multiple { num_entered, num_finished, .. } = team.qualification;
                then {
                    teams.iter()
                        .take(*exclude_players)
                        .all(|team| team.members.iter().all(|member| member.is_confirmed))
                    && teams.iter()
                        .enumerate()
                        .find(|(_, team)| team.members.iter().any(|member| member.user == teams::MemberUser::Newcomer))
                        .is_none_or(|(newcomer_placement, _)| placement < newcomer_placement) // Newcomer can represent any number of teams
                    && placement < *num_players
                    && if *need_finish { num_finished } else { num_entered } >= *min_races
                } else {
                    false
                }
            }),
            Self::RslLeaderboard => Some(if let Some(racetime) = &me.racetime {
                let rsl::Leaderboard { metadata, qualified, .. } = rsl::Leaderboard::get(http_client).await?;
                if metadata.season != data.event { return Ok(None) }
                qualified.iter().any(|iter_player| iter_player.userid == racetime.id)
            } else {
                false
            }),
            Self::External { .. } => None,
        })
    }

    async fn check_get(&self, http_client: &reqwest::Client, data: &Data<'_>, is_checked: Option<bool>, redirect_uri: rocket::http::uri::Origin<'_>, defaults: &pic::EnterFormDefaults<'_>) -> Result<RequirementStatus, Error> {
        Ok(match self {
            Self::RaceTime => {
                let mut html_content = html! {
                    : "Connect a racetime.gg account to your Hyrule Town Hall account";
                };
                if !is_checked.unwrap() {
                    //TODO offer to merge accounts like on profile
                    html_content = html! {
                        a(href = uri!(crate::auth::racetime_login(Some(redirect_uri))), target = "_blank") : html_content;
                    };
                }
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html_content),
                }
            }
            Self::RaceTimeInvite { text, .. } => {
                let text = text.clone();
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html! {
                        @if let Some(text) = text {
                            : text;
                        } else {
                            : "You must be on a list of invited racetime.gg users";
                        }
                    }),
                }
            }
            Self::Twitch => {
                let mut html_content = html! {
                    : "Connect a Twitch account to your racetime.gg account";
                };
                if !is_checked.unwrap() {
                    html_content = html! {
                        a(href = format!("https://{}/account/connections", racetime_host()), target = "_blank") : html_content;
                    };
                }
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html_content),
                }
            }
            Self::Discord => {
                let mut html_content = html! {
                    : "Connect a Discord account to your Hyrule Town Hall account";
                };
                if !is_checked.unwrap() {
                    //TODO offer to merge accounts like on profile
                    html_content = html! {
                        a(href = uri!(crate::auth::discord_login(Some(redirect_uri))), target = "_blank") : html_content;
                    };
                }
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html_content),
                }
            }
            Self::DiscordGuild { name } => {
                let name = name.clone();
                let invite_url = data.discord_invite_url.as_ref().map(|url| url.to_string());
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html! {
                        @if let Some(invite_url) = invite_url {
                            a(href = invite_url, target = "_blank") {
                                : "Join the ";
                                bdi : name;
                                : " Discord server";
                            }
                        } else {
                            : "Join the ";
                            bdi : name;
                            : " Discord server";
                        }
                    }),
                }
            }
            Self::Challonge => {
                let mut html_content = html! {
                    : "Connect a Challonge account to your Hyrule Town Hall account";
                };
                if !is_checked.unwrap() {
                    html_content = html! {
                        a(href = uri!(crate::auth::challonge_login(Some(redirect_uri))), target = "_blank") : html_content;
                    };
                }
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html_content),
                }
            }
            Self::StartGG { optional: false } => {
                let mut html_content = html! {
                    : "Connect a start.gg account to your Hyrule Town Hall account";
                };
                if !is_checked.unwrap() {
                    html_content = html! {
                        a(href = uri!(crate::auth::startgg_login(Some(redirect_uri))), target = "_blank") : html_content;
                    };
                }
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html_content),
                }
            }
            Self::StartGG { optional: true } => {
                let yes_checked = defaults.field_value("startgg_radio").is_none_or(|value| value == "yes");
                let html_content = html! {
                    @if is_checked.unwrap() {
                        : "Enter with your connected start.gg account"; //TODO show name and link to profile
                    } else {
                        a(href = uri!(crate::auth::startgg_login(Some(redirect_uri)))) : "Connect a start.gg account to your Hyrule Town Hall account";
                    }
                };
                let no_checked = defaults.field_value("startgg_radio").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("startgg_radio", errors, html! {
                            input(id = "startgg_radio-yes", type = "radio", name = "startgg_radio", value = "yes", checked? = yes_checked);
                            label(for = "startgg_radio-yes") : html_content;
                            br;
                            input(id = "startgg_radio-no", type = "radio", name = "startgg_radio", value = "no", checked? = no_checked);
                            label(for = "startgg_radio-no") : "Enter without connecting a start.gg account";
                        });
                    }),
                }
            }
            &Self::TextField { ref label, long, .. } => {
                let label = label.clone();
                let value = defaults.field_value("text_field").map(|value| value.to_owned());
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : label;
                        : form_field("text_field", errors, html! {
                            @if long {
                                textarea(name = "text_field") : value;
                            } else {
                                input(type = "text", name = "text_field", value? = value);
                            }
                        });
                    }),
                }
            }
            &Self::TextField2 { ref label, long, .. } => {
                let label = label.clone();
                let value = defaults.field_value("text_field2").map(|value| value.to_owned());
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : label;
                        : form_field("text_field2", errors, html! {
                            @if long {
                                textarea(name = "text_field2") : value;
                            } else {
                                input(type = "text", name = "text_field2", value? = value);
                            }
                        });
                    }),
                }
            }
            Self::YesNo { label } => {
                let label = label.clone();
                let yes_checked = defaults.field_value("yes_no").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("yes_no").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("yes_no", errors, html! {
                            label(for = "yes_no") : label;
                            br;
                            input(id = "yes_no-yes", type = "radio", name = "yes_no", value = "yes", checked? = yes_checked);
                            label(for = "yes_no-yes") : "Yes";
                            input(id = "yes_no-no", type = "radio", name = "yes_no", value = "no", checked? = no_checked);
                            label(for = "yes_no-no") : "No";
                        });
                    }),
                }
            }
            Self::BooleanChoice { key, label } => {
                let key = key.clone();
                let label = label.clone();
                let yes_checked = defaults.field_value(&format!("custom_choices[{key}]")).is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value(&format!("custom_choices[{key}]")).is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field(&format!("custom_choices[{key}]"), errors, html! {
                            label(for = &format!("custom_choices[{key}]")) : label;
                            br;
                            input(id = &format!("custom_choices[{key}]-yes"), type = "radio", name = &format!("custom_choices[{key}]"), value = "yes", checked? = yes_checked);
                            label(for = &format!("custom_choices[{key}]-yes")) : "Yes";
                            input(id = &format!("custom_choices[{key}]-no"), type = "radio", name = &format!("custom_choices[{key}]"), value = "no", checked? = no_checked);
                            label(for = &format!("custom_choices[{key}]-no")) : "No";
                        });
                    }),
                }
            }
            Self::Rules { document } => {
                let checked = defaults.field_value("confirm").is_some_and(|value| value == "on");
                let team_config = data.team_config;
                let rules_url = if let Some(document) = document {
                    document.to_string()
                } else {
                    uri!(crate::event::info(data.series, &*data.event)).to_string()
                };
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("confirm", errors, html! {
                            input(type = "checkbox", id = "confirm", name = "confirm", checked? = checked);
                            label(for = "confirm") {
                                @if let TeamConfig::Solo = team_config {
                                    : "I have read and agree to ";
                                } else {
                                    : "We have read and agree to ";
                                }
                                a(href = rules_url, target = "_blank") : "the event rules";
                                : ".";
                            }
                        });
                    }),
                }
            }
            Self::Poll { document } => {
                let checked = defaults.field_value("confirm").is_some_and(|value| value == "on");
                let team_config = data.team_config;
                let poll_url = if let Some(document) = document {
                    document.to_string()
                } else {
                    uri!(crate::event::info(data.series, &*data.event)).to_string()
                };
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("confirm", errors, html! {
                            input(type = "checkbox", id = "confirm", name = "confirm", checked? = checked);
                            label(for = "confirm") {
                                @if let TeamConfig::Solo = team_config {
                                    : "I have submitted a response to ";
                                } else {
                                    : "We have submitted a response to  ";
                                }
                                a(href = poll_url, target = "_blank") : "the settings poll";
                                : ".";
                            }
                        });
                    }),
                }
            }
            Self::AllDungeonsOk => {
                let yes_checked = defaults.field_value("all_dungeons_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("all_dungeons_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("all_dungeons_ok", errors, html! {
                            label(for = "all_dungeons_ok") : "Opt-in for All Dungeons goal?";
                            br;
                            input(id = "all_dungeons_ok-yes", type = "radio", name = "all_dungeons_ok", value = "yes", checked? = yes_checked);
                            label(for = "all_dungeons_ok-yes") : "Yes";
                            input(id = "all_dungeons_ok-no", type = "radio", name = "all_dungeons_ok", value = "no", checked? = no_checked);
                            label(for = "all_dungeons_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::FluteOk => {
                let yes_checked = defaults.field_value("flute_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("flute_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("flute_ok", errors, html! {
                            label(for = "flute_ok") : "Opt-in for starting with an activated flute?";
                            br;
                            input(id = "flute_ok-yes", type = "radio", name = "flute_ok", value = "yes", checked? = yes_checked);
                            label(for = "flute_ok-yes") : "Yes";
                            input(id = "flute_ok-no", type = "radio", name = "flute_ok", value = "no", checked? = no_checked);
                            label(for = "flute_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::HardSettingsOk => {
                let yes_checked = defaults.field_value("hard_settings_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("hard_settings_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("hard_settings_ok", errors, html! {
                            label(for = "hard_settings_ok") : "Allow hardcore settings?";
                            br;
                            input(id = "hard_settings_ok-yes", type = "radio", name = "hard_settings_ok", value = "yes", checked? = yes_checked);
                            label(for = "hard_settings_ok-yes") : "Yes";
                            input(id = "hard_settings_ok-no", type = "radio", name = "hard_settings_ok", value = "no", checked? = no_checked);
                            label(for = "hard_settings_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::HoveringOk => {
                let yes_checked = defaults.field_value("hover_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("hover_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("hover_ok", errors, html! {
                            label(for = "hover_ok") : "Allow hovering and moldorm bouncing in your races (with opponent consent)?";
                            br;
                            input(id = "hover_ok-yes", type = "radio", name = "hover_ok", value = "yes", checked? = yes_checked);
                            label(for = "hover_ok-yes") : "Yes";
                            input(id = "hover_ok-no", type = "radio", name = "hover_ok", value = "no", checked? = no_checked);
                            label(for = "hover_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::InvertedOk => {
                let yes_checked = defaults.field_value("inverted_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("inverted_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("inverted_ok", errors, html! {
                            label(for = "inverted_ok") : "Opt-in for Inverted world state?";
                            br;
                            input(id = "inverted_ok-yes", type = "radio", name = "inverted_ok", value = "yes", checked? = yes_checked);
                            label(for = "inverted_ok-yes") : "Yes";
                            input(id = "inverted_ok-no", type = "radio", name = "inverted_ok", value = "no", checked? = no_checked);
                            label(for = "inverted_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::KeydropOk => {
                let yes_checked = defaults.field_value("keydrop_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("keydrop_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("keydrop_ok", errors, html! {
                            label(for = "keydrop_ok") : "Opt-in for keydrop?";
                            br;
                            input(id = "keydrop_ok-yes", type = "radio", name = "keydrop_ok", value = "yes", checked? = yes_checked);
                            label(for = "keydrop_ok-yes") : "Yes";
                            input(id = "keydrop_ok-no", type = "radio", name = "keydrop_ok", value = "no", checked? = no_checked);
                            label(for = "keydrop_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::LiteOk => {
                let yes_checked = defaults.field_value("lite_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("lite_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("lite_ok", errors, html! {
                            label(for = "lite_ok") : "Allow RSL-Lite?";
                            br;
                            input(id = "lite_ok-yes", type = "radio", name = "lite_ok", value = "yes", checked? = yes_checked);
                            label(for = "lite_ok-yes") : "Yes";
                            input(id = "lite_ok-no", type = "radio", name = "lite_ok", value = "no", checked? = no_checked);
                            label(for = "lite_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::MirrorScrollOk => {
                let yes_checked = defaults.field_value("mirror_scroll_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("mirror_scroll_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("mirror_scroll_ok", errors, html! {
                            label(for = "mirror_scroll_ok") : "Opt-in for mirror scroll?";
                            br;
                            input(id = "mirror_scroll_ok-yes", type = "radio", name = "mirror_scroll_ok", value = "yes", checked? = yes_checked);
                            label(for = "mirror_scroll_ok-yes") : "Yes";
                            input(id = "mirror_scroll_ok-no", type = "radio", name = "mirror_scroll_ok", value = "no", checked? = no_checked);
                            label(for = "mirror_scroll_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::MqOk => {
                let yes_checked = defaults.field_value("mq_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("mq_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("mq_ok", errors, html! {
                            label(for = "mq_ok") : "Allow Master Quest?";
                            br;
                            input(id = "mq_ok-yes", type = "radio", name = "mq_ok", value = "yes", checked? = yes_checked);
                            label(for = "mq_ok-yes") : "Yes";
                            input(id = "mq_ok-no", type = "radio", name = "mq_ok", value = "no", checked? = no_checked);
                            label(for = "mq_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::NoDelayOk => {
                let yes_checked = defaults.field_value("no_delay_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("no_delay_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("no_delay_ok", errors, html! {
                            label(for = "no_delay_ok") : "Opt-in for no stream delay?";
                            br;
                            input(id = "no_delay_ok-yes", type = "radio", name = "no_delay_ok", value = "yes", checked? = yes_checked);
                            label(for = "no_delay_ok-yes") : "Yes";
                            input(id = "no_delay_ok-no", type = "radio", name = "no_delay_ok", value = "no", checked? = no_checked);
                            label(for = "no_delay_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::PbOk => {
                let yes_checked = defaults.field_value("pb_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("pb_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("pb_ok", errors, html! {
                            label(for = "pb_ok") : "Opt-in for pseudoboots?";
                            br;
                            input(id = "pb_ok-yes", type = "radio", name = "pb_ok", value = "yes", checked? = yes_checked);
                            label(for = "pb_ok-yes") : "Yes";
                            input(id = "pb_ok-no", type = "radio", name = "pb_ok", value = "no", checked? = no_checked);
                            label(for = "pb_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::ZwOk => {
                let yes_checked = defaults.field_value("zw_ok").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("zw_ok").is_some_and(|value| value == "no");
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("zw_ok", errors, html! {
                            // TODO: Maybe add a link here explaining ZW?
                            label(for = "zw_ok") : "Opt-in for ZW?";
                            br;
                            input(id = "zw_ok-yes", type = "radio", name = "zw_ok", value = "yes", checked? = yes_checked);
                            label(for = "zw_ok-yes") : "Yes";
                            input(id = "zw_ok-no", type = "radio", name = "zw_ok", value = "no", checked? = no_checked);
                            label(for = "zw_ok-no") : "No";
                        });
                    }),
                }
            }
            Self::RestreamConsent { optional: false, note } => {
                let checked = defaults.field_value("restream_consent").is_some_and(|value| value == "on");
                let team_config = data.team_config;
                let note = note.clone();
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("restream_consent", errors, html! {
                            input(type = "checkbox", id = "restream_consent", name = "restream_consent", checked? = checked);
                            label(for = "restream_consent") {
                                @if let TeamConfig::Solo = team_config {
                                    : "I am okay with being restreamed.";
                                } else {
                                    : "We are okay with being restreamed.";
                                }
                                @if let Some(note) = note {
                                    br;
                                    : note;
                                }
                            }
                        });
                    }),
                }
            }
            Self::RestreamConsent { optional: true, note } => {
                let yes_checked = defaults.field_value("restream_consent_radio").is_some_and(|value| value == "yes");
                let no_checked = defaults.field_value("restream_consent_radio").is_some_and(|value| value == "no");
                let note = note.clone();
                RequirementStatus {
                    blocks_submit: false,
                    html_content: Box::new(move |errors| html! {
                        : form_field("restream_consent_radio", errors, html! {
                            label(for = "restream_consent_radio") {
                                : "Let us know whether you are okay with being restreamed:";
                            }
                            br;
                            input(id = "restream_consent_radio-yes", type = "radio", name = "restream_consent_radio", value = "yes", checked? = yes_checked);
                            label(for = "restream_consent_radio-yes") : "Yes";
                            input(id = "restream_consent_radio-no", type = "radio", name = "restream_consent_radio", value = "no", checked? = no_checked);
                            label(for = "restream_consent_radio-no") : "No";
                            @if let Some(note) = note {
                                br;
                                label(for = "restream_consent_radio") : note;
                            }
                        });
                    }),
                }
            }
            &Self::Qualifier { async_start, async_end, live_start } => {
                let now = Utc::now();
                let async_available = now >= async_start && now < async_end;
                let series = data.series;
                let checked = defaults.field_value("confirm").is_some_and(|value| value == "on");
                RequirementStatus {
                    blocks_submit: !async_available,
                    html_content: Box::new(move |errors| html! {
                        @if async_available {
                            : "Play the qualifier seed, either live on ";
                            : format_datetime(live_start, DateTimeFormat { long: true, running_text: true });
                            : " or request it as an async using this form by ";
                            : format_datetime(async_end, DateTimeFormat { long: true, running_text: true });
                            : ".";
                            @match series {
                                Series::TriforceBlitz => : tfb::qualifier_async_rules();
                                _ => @unimplemented
                            }
                            : form_field("confirm", errors, html! {
                                input(type = "checkbox", id = "confirm", name = "confirm", checked? = checked);
                                label(for = "confirm") : "I have read the above and am ready to play the seed";
                            });
                        } else {
                            : "Play the qualifier seed, either live on ";
                            : format_datetime(live_start, DateTimeFormat { long: true, running_text: true });
                            : " or async between ";
                            : format_datetime(async_start, DateTimeFormat { long: false, running_text: true });
                            : " and ";
                            : format_datetime(async_end, DateTimeFormat { long: false, running_text: true });
                            @if now < async_start {
                                : ". The form to request the async will appear on this page.";
                            }
                        }
                    }),
                }
            }
            &Self::TripleQualifier { async_starts, async_ends, live_starts } => {
                let now = Utc::now();
                let async_available = async_starts.into_iter().zip_eq(async_ends).any(|(async_start, async_end)| now >= async_start && now < async_end);
                let series = data.series;
                let checked = defaults.field_value("confirm").is_some_and(|value| value == "on");
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap() && !async_available,
                    html_content: Box::new(move |errors| html! {
                        @if is_checked.unwrap() {
                            : "Play at least one of the 3 qualifier seeds, either live or async.";
                            br;
                            : "If you would like to play additional asyncs, enter the event and request them from your status page.";
                        } else if async_available {
                            : "Play at least one of the 3 qualifier seeds, either live or by requesting as an async using this form: ";
                        } else {
                            : "Play at least one of the 3 qualifier seeds, either live or async. The form to request an async will appear on this page.";
                        }
                        ol {
                            @for ((async_start, async_end), live_start) in async_starts.into_iter().zip_eq(async_ends).zip(live_starts) {
                                li {
                                    : "Live at ";
                                    : format_datetime(live_start, DateTimeFormat { long: true, running_text: true });
                                    : " or async between ";
                                    : format_datetime(async_start, DateTimeFormat { long: false, running_text: true });
                                    : " and ";
                                    : format_datetime(async_end, DateTimeFormat { long: false, running_text: true });
                                }
                            }
                        }
                        @if !is_checked.unwrap() && async_available {
                            @match series {
                                Series::TriforceBlitz => : tfb::qualifier_async_rules();
                                _ => @unimplemented
                            }
                            : form_field("confirm", errors, html! {
                                input(type = "checkbox", id = "confirm", name = "confirm", checked? = checked);
                                label(for = "confirm") : "I have read the above and am ready to play the seed";
                            });
                        }
                    }),
                }
            }
            &Self::QualifierPlacement { num_players, min_races, need_finish, exclude_players, event: _ } => {
                RequirementStatus {
                    blocks_submit: !is_checked.unwrap(),
                    html_content: Box::new(move |_| html! {
                        @if min_races == 0 {
                            : "Place";
                        } else {
                            @if need_finish {
                                : "Finish at least ";
                            } else {
                                : "Enter at least ";
                            }
                            : min_races;
                            : " qualifier races and place";
                        }
                        @if exclude_players == 0 {
                            : " in the top ";
                            : num_players.to_string();
                        } else {
                            : " ";
                            : lang::english_ordinal(exclude_players + 1);
                            : " to ";
                            : lang::english_ordinal(num_players);
                        }
                        : " of qualifier scores.";
                        br;
                        : "Note: You may be eligible to enter even if you don't initially place ";
                        @if exclude_players == 0 {
                            : "in the top ";
                            : num_players.to_string();
                        } else {
                            : " in this range";
                        }
                        : " due to other players opting out. You will be notified by an organizer if this is the case.";
                    }),
                }
            }
            Self::RslLeaderboard => {
                let rsl::Leaderboard { metadata: rsl::LeaderboardMetadata { required_races, .. }, .. } = rsl::Leaderboard::get(http_client).await?;
                RequirementStatus {
                    blocks_submit: is_checked.is_none_or(|is_checked| !is_checked),
                    html_content: Box::new(move |_| html! {
                        : "Have ";
                        : required_races;
                        : " finishes on ";
                        a(href = "https://rsl.one/") : "the RSL leaderboard";
                    }),
                }
            }
            Self::External { html, text, blocks_submit } => {
                let html = html.clone();
                let text = text.clone();
                RequirementStatus {
                    blocks_submit: *blocks_submit,
                    html_content: Box::new(move |_| html! {
                        : html;
                        : text;
                    }),
                }
            }
        })
    }

    async fn check_form(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, discord_ctx: &RwFuture<DiscordCtx>, me: &User, data: &Data<'_>, form_ctx: &mut Context<'_>, value: &EnterForm) -> Result<(), Error> {
        match self {
            Self::StartGG { optional: false } => if !self.is_checked(transaction, http_client, discord_ctx, me, data).await?.unwrap_or(false) {
                form_ctx.push_error(form::Error::validation("A start.gg account is required to enter this event.")); //TODO link to /login/startgg
            },
            Self::StartGG { optional: true } => match value.startgg_radio {
                Some(BoolRadio::Yes) => if !self.is_checked(transaction, http_client, discord_ctx, me, data).await?.unwrap_or(false) {
                    form_ctx.push_error(form::Error::validation("Sign in with start.gg or opt out of start.gg integration.").with_name("startgg_radio")); //TODO link to /login/startgg
                },
                Some(BoolRadio::No) => {}
                None => form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("startgg_radio")),
            },
            Self::TextField { regex, regex_error_messages, fallback_error_message, .. } => if !regex.is_match(&value.text_field) {
                let error_message = if let Some((_, error_message)) = regex_error_messages.iter().find(|(regex, _)| regex.is_match(&value.text_field)) {
                    error_message.clone()
                } else {
                    fallback_error_message.clone()
                };
                form_ctx.push_error(form::Error::validation(error_message).with_name("text_field"));
            },
            Self::TextField2 { regex, regex_error_messages, fallback_error_message, .. } => if !regex.is_match(&value.text_field2) {
                let error_message = if let Some((_, error_message)) = regex_error_messages.iter().find(|(regex, _)| regex.is_match(&value.text_field2)) {
                    error_message.clone()
                } else {
                    fallback_error_message.clone()
                };
                form_ctx.push_error(form::Error::validation(error_message).with_name("text_field2"));
            },
            Self::YesNo { .. } => if value.yes_no.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("yes_no"));
            },
            Self::BooleanChoice { key, .. } => if !value.custom_choices.contains_key(key) {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name(format!("custom_choices[{key}]")));
            },
            Self::Rules { .. } => if !value.confirm {
                form_ctx.push_error(form::Error::validation("This field is required.").with_name("confirm"));
            },
            Self::Poll { .. } => if !value.confirm {
                form_ctx.push_error(form::Error::validation("This field is required.").with_name("confirm"));
            },
            Self::AllDungeonsOk => if value.all_dungeons_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("all_dungeons_ok"));
            },
            Self::FluteOk => if value.flute_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("flute_ok"));
            },
            Self::HardSettingsOk => if value.hard_settings_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("hard_settings_ok"));
            },
            Self::HoveringOk => if value.hover_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("hover_ok"));
            },
            Self::InvertedOk => if value.inverted_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("inverted_ok"));
            },
            Self::KeydropOk => if value.keydrop_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("keydrop_ok"));
            },
            Self::LiteOk => if value.lite_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("lite_ok"));
            },
            Self::MirrorScrollOk => if value.mirror_scroll_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("mirror_scroll_ok"));
            },
            Self::MqOk => if value.mq_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("mq_ok"));
            },
            Self::NoDelayOk => if value.no_delay_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("no_delay_ok"));
            },
            Self::PbOk => if value.pb_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("pb_ok"));
            }
            Self::ZwOk => if value.zw_ok.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("zw_ok"));
            }
            Self::RestreamConsent { optional: false, .. } => if !value.restream_consent {
                form_ctx.push_error(form::Error::validation("Restream consent is required to enter this event.").with_name("restream_consent"));
            },
            Self::RestreamConsent { optional: true, .. } => if value.restream_consent_radio.is_none() {
                form_ctx.push_error(form::Error::validation("Please select one of the options.").with_name("restream_consent_radio"));
            },
            Self::Qualifier { async_start, async_end, .. } => {
                let now = Utc::now();
                if now >= *async_start && now < *async_end {
                    if !value.confirm {
                        form_ctx.push_error(form::Error::validation("This field is required.").with_name("confirm"));
                    }
                } else {
                    form_ctx.push_error(form::Error::validation("The qualifier seed is not yet available."));
                }
            }
            Self::TripleQualifier { async_starts, async_ends, .. } => if !self.is_checked(transaction, http_client, discord_ctx, me, data).await?.unwrap_or(false) {
                let now = Utc::now();
                if (*async_starts).into_iter().zip_eq(*async_ends).any(|(async_start, async_end)| now >= async_start && now < async_end) {
                    if !value.confirm {
                        form_ctx.push_error(form::Error::validation("This field is required.").with_name("confirm"));
                    }
                } else {
                    form_ctx.push_error(form::Error::validation("No qualifier seed is currently available."));
                }
            },
            Self::External { blocks_submit, .. } => if *blocks_submit {
                form_ctx.push_error(form::Error::validation("Please complete event entry via the external method."));
            },
            _ => if !self.is_checked(transaction, http_client, discord_ctx, me, data).await?.unwrap_or(false) {
                form_ctx.push_error(form::Error::validation(match self {
                    Self::RaceTime => Cow::Borrowed("A racetime.gg account is required to enter this event. Go to your Hyrule Town Hall profile and select 'Connect a racetime.gg account'."), //TODO direct link?
                    Self::RaceTimeInvite { error_text, .. } => if me.racetime.is_some() {
                        if let Some(error_text) = error_text {
                            Cow::Owned(error_text.clone())
                        } else {
                            Cow::Borrowed("This is an invitational event and it looks like you're not invited.")
                        }
                    } else {
                        Cow::Borrowed("This event uses an invite list of racetime.gg users. Go to your Hyrule Town Hall profile and select 'Connect a racetime.gg account' to check whether you're invited.") //TODO direct link?
                    },
                    Self::Twitch => Cow::Borrowed("A Twitch account is required to enter this event. Go to the 'Twitch & connections' section of your racetime.gg settings to connect one."), //TODO direct link?
                    Self::Discord => Cow::Borrowed("A Discord account is required to enter this event. Go to your Hyrule Town Hall profile and select 'Connect a Discord account'."), //TODO direct link?
                    Self::DiscordGuild { .. } => Cow::Borrowed("You must join the event's Discord server to enter."), //TODO invite link?
                    Self::Challonge => Cow::Borrowed("A Challonge account is required to enter this event."), //TODO link to /login/challonge
                    Self::QualifierPlacement { .. } => Cow::Borrowed("You have not secured a qualifying placement."), //TODO different message if the player has overqualified or overqualifying due to opt-outs is still possible
                    Self::RslLeaderboard => Cow::Borrowed("You have not finished the required number of races on the RSL leaderboard."), //TODO link to rsl.one
                    | Self::StartGG { .. }
                    | Self::TextField { .. }
                    | Self::TextField2 { .. }
                    | Self::YesNo { .. }
                    | Self::Rules { .. }
                    | Self::Poll { .. }
                    | Self::AllDungeonsOk
                    | Self::FluteOk
                    | Self::HoveringOk
                    | Self::HardSettingsOk
                    | Self::InvertedOk
                    | Self::KeydropOk
                    | Self::LiteOk
                    | Self::MirrorScrollOk
                    | Self::MqOk
                    | Self::NoDelayOk
                    | Self::PbOk
                    | Self::ZwOk
                    | Self::RestreamConsent { .. }
                    | Self::Qualifier { .. }
                    | Self::TripleQualifier { .. }
                    | Self::BooleanChoice { .. }
                    | Self::External { .. }
                        => unreachable!(),
                }));
            }
        }
        Ok(())
    }

    async fn request_qualifier(&self, transaction: &mut Transaction<'_, Postgres>, http_client: &reqwest::Client, discord_ctx: &RwFuture<DiscordCtx>, me: &User, data: &Data<'_>) -> Result<Option<AsyncKind>, Error> {
        Ok(match self {
            Requirement::Qualifier { .. } => Some(AsyncKind::Qualifier1),
            Requirement::TripleQualifier { async_starts, async_ends, .. } => {
                let now = Utc::now();
                if self.is_checked(transaction, http_client, discord_ctx, me, data).await?.unwrap_or(false) {
                    None
                } else {
                    (*async_starts).into_iter()
                        .zip_eq(*async_ends)
                        .enumerate()
                        .find(|&(_, (async_start, async_end))| now >= async_start && now < async_end)
                        .map(|(idx, _)| match idx {
                            0 => AsyncKind::Qualifier1,
                            1 => AsyncKind::Qualifier2,
                            2 => AsyncKind::Qualifier3,
                            _ => unreachable!("more than 3 qualifiers in Requirement::TripleQualifier"),
                        })
                }
            }
            _ => None,
        })
    }
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum Error {
    #[error(transparent)] Cal(#[from] cal::Error),
    #[error(transparent)] Data(#[from] DataError),
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] Notification(#[from] crate::notification::Error),
    #[error(transparent)] Page(#[from] PageError),
    #[error(transparent)] Reqwest(#[from] reqwest::Error),
    #[error(transparent)] Serenity(#[from] serenity::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] StartGG(#[from] startgg::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[error("event has a discordGuild entry requirement but no Discord guild")]
    DiscordGuild,
    #[error("no such event")]
    NoSuchEvent,
}

impl<E: Into<Error>> From<E> for StatusOrError<Error> {
    fn from(e: E) -> Self {
        Self::Err(e.into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, FromFormField)]
enum BoolRadio {
    Yes,
    No,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct EnterForm {
    #[field(default = String::new())]
    csrf: String,
    confirm: bool,
    racetime_team: Option<String>,
    #[field(default = String::new())]
    team_name: String,
    my_role: Option<pic::Role>,
    teammate: Option<Id<Users>>,
    step2: bool,
    roles: HashMap<String, Role>,
    /// Mapping from racetime.gg user IDs to start.gg user slugs.
    /// Slugs are used in the profile URL and on the profile page itself; not to be confused with the start.gg user IDs returned by the GraphQL API.
    startgg_id: HashMap<String, String>,
    mw_impl: Option<mw::Impl>,
    startgg_radio: Option<BoolRadio>,
    restream_consent: bool,
    restream_consent_radio: Option<BoolRadio>,
    yes_no: Option<BoolRadio>,
    all_dungeons_ok: Option<BoolRadio>,
    flute_ok: Option<BoolRadio>,
    hard_settings_ok: Option<BoolRadio>,
    hover_ok: Option<BoolRadio>,
    inverted_ok: Option<BoolRadio>,
    keydrop_ok: Option<BoolRadio>,
    lite_ok: Option<BoolRadio>,
    mq_ok: Option<BoolRadio>,
    mirror_scroll_ok: Option<BoolRadio>,
    no_delay_ok: Option<BoolRadio>,
    pb_ok: Option<BoolRadio>,
    zw_ok: Option<BoolRadio>,
    #[field(default = String::new())]
    text_field: String,
    #[field(default = String::new())]
    text_field2: String,
    custom_choices: HashMap<String, String>,
}

pub(crate) async fn enter_form(mut transaction: Transaction<'_, Postgres>, http_client: &reqwest::Client, discord_ctx: &RwFuture<DiscordCtx>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, defaults: pic::EnterFormDefaults<'_>) -> Result<RawHtml<String>, Error> {
    //TODO if already entered, redirect to status page
    let my_invites = if let Some(ref me) = me {
        sqlx::query_scalar!(r#"SELECT team AS "team: Id<Teams>" FROM teams, team_members WHERE series = $1 AND event = $2 AND member = $3 AND status = 'unconfirmed'"#, data.series as _, &*data.event, me.id as _).fetch_all(&mut *transaction).await?
    } else {
        Vec::default()
    };
    let content = if data.is_started(&mut transaction).await? {
        html! {
            article {
                p : "You can no longer enter this event since it has already started.";
            }
        }
    } else {
        match (data.series, &*data.event) {
            (Series::BattleRoyale, "1") => ohko::enter_form(),
            (Series::Standard, "w") => s::weeklies_enter_form(me.as_ref()),
            _ => match data.team_config {
                TeamConfig::Solo => {
                    if let Some(Flow { ref requirements, closes }) = data.enter_flow {
                        let opted_out = if let Some(racetime) = me.as_ref().and_then(|me| me.racetime.as_ref()) {
                            sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM opt_outs WHERE series = $1 AND event = $2 AND racetime_id = $3) AS "exists!""#, data.series as _, &data.event, racetime.id).fetch_one(&mut *transaction).await?
                        } else {
                            false
                        };
                        if opted_out {
                            html! {
                                article {
                                    p : "You can no longer enter this event since you have already opted out.";
                                }
                            }
                        } else if closes.is_some_and(|closes| closes <= Utc::now()) {
                            html! {
                                article {
                                    p : "The deadline to enter this event has passed.";
                                }
                            }
                        } else if requirements.is_empty() {
                            if data.is_single_race() {
                                html! {
                                    article {
                                        p {
                                            @if let Some(ref url) = data.url {
                                                : "Enter ";
                                                a(href = url.to_string()) : "the race room";
                                                : " to participate in this race.";
                                            } else {
                                                : "The race room will be opened around 1 hour before the scheduled starting time. ";
                                                @if me.as_ref().is_some_and(|me| me.racetime.is_some()) {
                                                    : "You don't need to sign up beforehand.";
                                                } else {
                                                    : "You will need a ";
                                                    a(href = format!("https://{}/", racetime_host())) : "racetime.gg";
                                                    : " account to participate.";
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                full_form(uri!(post(data.series, &*data.event)), csrf, html! {}, defaults.errors(), "Enter")
                            }
                        } else if let Some(ref me) = me {
                            let mut can_submit = true;
                            let mut request_qualifier = false;
                            let mut requirements_display = Vec::with_capacity(requirements.len());
                            for requirement in requirements {
                                let is_checked = requirement.is_checked(&mut transaction, http_client, discord_ctx, me, &data).await?;
                                let status = requirement.check_get(http_client, &data, is_checked, uri!(get(data.series, &*data.event, defaults.my_role(), defaults.teammate())), &defaults).await?;
                                if status.blocks_submit { can_submit = false }
                                if requirement.request_qualifier(&mut transaction, http_client, discord_ctx, me, &data).await?.is_some() { request_qualifier = true }
                                requirements_display.push((is_checked, status.html_content));
                            }
                            let preface = html! {
                                @if data.show_opt_out {
                                    p {
                                        : "If you would like to enter this event, please fill out the form below. If not, please ";
                                        a(href = uri!(super::opt_out(data.series, &*data.event))) : "opt out";
                                        : ".";
                                    }
                                } else {
                                    p : "To enter this event:";
                                }
                            };
                            if can_submit {
                                let mut errors = defaults.errors();
                                full_form(uri!(post(data.series, &*data.event)), csrf, html! {
                                    : preface;
                                    @for (is_checked, html_content) in requirements_display {
                                        div(class = "check-item") {
                                            div(class = "checkmark") {
                                                @match is_checked {
                                                    Some(true) => : "✓";
                                                    Some(false) => {}
                                                    None => : "?";
                                                }
                                            }
                                            div : html_content(&mut errors);
                                        }
                                    }
                                }, errors, if request_qualifier { "Enter and Request Seed" } else { "Enter" })
                            } else {
                                html! {
                                    article {
                                        : preface;
                                        @for (is_checked, html_content) in requirements_display {
                                            div(class = "check-item") {
                                                div(class = "checkmark") {
                                                    @match is_checked {
                                                        Some(true) => : "✓";
                                                        Some(false) => {}
                                                        None => : "?";
                                                    }
                                                }
                                                div : html_content(&mut Vec::default());
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            html! {
                                article {
                                    p {
                                        a(href = uri!(auth::login(Some(uri!(get(data.series, &*data.event, defaults.my_role(), defaults.teammate())))))) : "Sign in or create a Hyrule Town Hall account";
                                        : " to enter";
                                        @if data.show_opt_out {
                                            : " or opt out of";
                                        }
                                        : " this event.";
                                    }
                                }
                            }
                        }
                    } else if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE series = $1 AND event = $2) AS "exists!""#, data.series as _, &*data.event).fetch_one(&mut *transaction).await? {
                        if me.is_none() {
                            html! {
                                article {
                                    p {
                                        : "This is an invitational event. ";
                                        a(href = uri!(auth::login(Some(uri!(get(data.series, &*data.event, defaults.my_role(), defaults.teammate())))))) : "Sign in or create a Hyrule Town Hall account";
                                        : " to see if you're invited.";
                                    }
                                }
                            }
                        } else if my_invites.is_empty() {
                            html! {
                                article {
                                    p : "This is an invitational event and it looks like you're not invited.";
                                }
                            }
                        } else {
                            html! {} // invite should be rendered above this content
                        }
                    } else {
                        html! {
                            article {
                                p : "Signups for this event aren't open yet."; //TODO option to be notified when signups open
                            }
                        }
                    }
                }
                TeamConfig::Pictionary => return Ok(pic::enter_form(transaction, me, uri, csrf, data, defaults).await?),
                TeamConfig::CoOp | TeamConfig::TfbCoOp | TeamConfig::Multiworld => return Ok(mw::enter_form(transaction, me, uri, csrf, data, defaults.into_context(), http_client).await?),
            },
        }
    };
    let header = data.header(&mut transaction, me.as_ref(), Tab::Enter, false).await?;
    let invites = html! {
        @for team_id in my_invites {
            : crate::notification::team_invite(&mut transaction, me.as_ref().expect("got a team invite while not logged in"), csrf, defaults.errors(), crate::notification::TeamInviteSource::Enter, team_id).await?;
        }
    };
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Enter — {}", data.display_name), html! {
        : header;
        : invites;
        : content;
    }).await?)
}

fn enter_form_step2<'a, 'b: 'a, 'c: 'a, 'd: 'a>(mut transaction: Transaction<'a, Postgres>, me: Option<User>, uri: Origin<'b>, http_client: &reqwest::Client, csrf: Option<&'a CsrfToken>, data: Data<'c>, defaults: mw::EnterFormStep2Defaults<'d>) -> Pin<Box<dyn Future<Output = Result<RawHtml<String>, Error>> + Send + 'a>> {
    let team_members = defaults.racetime_members(http_client);
    Box::pin(async move {
        let header = data.header(&mut transaction, me.as_ref(), Tab::Enter, true).await?;
        let page_content = {
            let team_config = data.team_config;
            let team_members = team_members.await?;
            let mut errors = defaults.errors();
            html! {
                : header;
                : full_form(uri!(post(data.series, &*data.event)), csrf, html! {
                    input(type = "hidden", name = "step2", value = "true");
                    : form_field("racetime_team", &mut errors, html! {
                        label(for = "racetime_team") {
                            : "racetime.gg Team: ";
                            a(href = format!("https://{}/team/{}", racetime_host(), defaults.racetime_team_slug().expect("missing racetime team slug"))) : defaults.racetime_team_name().expect("missing racetime team name");
                            : " • ";
                            a(href = uri!(get(data.series, &*data.event, _, _))) : "Change";
                        }
                        input(type = "hidden", name = "racetime_team", value = defaults.racetime_team_slug());
                        input(type = "hidden", name = "racetime_team_name", value = defaults.racetime_team_name());
                    });
                    @for (member_idx, team_member) in team_members.into_iter().enumerate() {
                        @if team_config.has_distinct_roles() {
                            : form_field(&format!("roles[{}]", team_member.id), &mut errors, html! {
                                label(for = &format!("roles[{}]", team_member.id)) : &team_member.name; //TODO Mido's House display name, falling back to racetime display name if no Mido's House account
                                @for (role, display_name) in team_config.roles() {
                                    @let css_class = role.css_class().expect("tried to render enter_form_step2 for a solo event");
                                    input(id = &format!("roles[{}]-{css_class}", team_member.id), class = css_class, type = "radio", name = &format!("roles[{}]", team_member.id), value = css_class, checked? = defaults.role(&team_member.id) == Some(*role));
                                    label(class = css_class, for = &format!("roles[{}]-{css_class}", team_member.id)) : display_name;
                                }
                            });
                        }
                        : form_field(&format!("startgg_id[{}]", team_member.id), &mut errors, html! {
                            label(for = &format!("startgg_id[{}]", team_member.id)) {
                                : "start.gg User ID (";
                                : &team_member.name; //TODO Mido's House display name, falling back to racetime display name if no Mido's House account
                                : "):";
                            }
                            input(type = "text", name = &format!("startgg_id[{}]", team_member.id), value? = defaults.startgg_id(&team_member.id));
                            label(class = "help") {
                                : "(Optional. Can be copied by going to your ";
                                a(href = "https://start.gg/") : "start.gg";
                                : " profile and clicking your name.)";
                            }
                        });
                        @if let Series::CoOp = data.series {
                            @let field_name = match member_idx {
                                0 => "text_field",
                                1 => "text_field2",
                                _ => unreachable!("co-op event with team size > 2"),
                            };
                            : form_field(field_name, &mut errors, html! {
                                label(for = field_name) {
                                    : "Nationality (";
                                    : &team_member.name; //TODO Mido's House display name, falling back to racetime display name if no Mido's House account
                                    : "):";
                                }
                                input(type = "text", name = field_name, value? = defaults.field_value(field_name));
                            });
                        }
                    }
                    @if let TeamConfig::Multiworld = team_config {
                        : form_field("mw_impl", &mut errors, html! {
                            label(for = "mw_impl") : "Multiworld plugin:";
                            input(id = "mw_impl-bizhawk_co_op", type = "radio", name = "mw_impl", value = "bizhawk_co_op", checked? = defaults.mw_impl() == Some(mw::Impl::BizHawkCoOp));
                            label(for = "mw_impl-bizhawk_co_op") : "bizhawk-co-op";
                            input(id = "mw_impl-midos_house", type = "radio", name = "mw_impl", value = "midos_house", checked? = defaults.mw_impl() == Some(mw::Impl::MidosHouse));
                            label(for = "mw_impl-midos_house") : "HTH Multiworld";
                        });
                    }
                    : form_field("restream_consent_radio", &mut errors, html! {
                        label(for = "restream_consent_radio") {
                            @match data.series {
                                Series::CoOp => {
                                    : "Do you consent to your matches being restreamed by our restream partners (see ";
                                    a(href = "https://zsr.link/coops3rules") : "rules document";
                                    : ")? If you change your mind later, please let organizers know.";
                                }
                                Series::Multiworld => {
                                    //TODO allow changing on Status page during Swiss, except revoking while a restream is planned
                                    //TODO change text depending on tournament structure
                                    : "We are okay with being restreamed. (Optional for Swiss, required for top 8. Can be changed later.)";
                                }
                                _ => : "We are okay with being restreamed. (Optional. Can be changed later.)";
                            }
                        }
                        br;
                        input(id = "restream_consent_radio-yes", type = "radio", name = "restream_consent_radio", value = "yes", checked? = defaults.restream_consent() == Some(true));
                        label(for = "restream_consent_radio-yes") : "Yes";
                        input(id = "restream_consent_radio-no", type = "radio", name = "restream_consent_radio", value = "no", checked? = defaults.restream_consent() == Some(false));
                        label(for = "restream_consent_radio-no") : "No";
                    });
                }, errors, "Enter");
            }
        };
        Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Enter — {}", data.display_name), page_content).await?)
    })
}

#[rocket::get("/event/<series>/<event>/enter?<my_role>&<teammate>")]
pub(crate) async fn get(pool: &State<PgPool>, http_client: &State<reqwest::Client>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: Option<User>, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, my_role: Option<pic::Role>, teammate: Option<Id<Users>>) -> Result<RawHtml<String>, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    Ok(enter_form(transaction, http_client, discord_ctx, me, uri, csrf.as_ref(), data, pic::EnterFormDefaults::Values { my_role, teammate }).await?)
}

#[rocket::post("/event/<series>/<event>/enter", data = "<form>")]
pub(crate) async fn post(config: &State<Config>, pool: &State<PgPool>, http_client: &State<reqwest::Client>, discord_ctx: &State<RwFuture<DiscordCtx>>, me: User, uri: Origin<'_>, csrf: Option<CsrfToken>, series: Series, event: &str, form: Form<Contextual<'_, EnterForm>>) -> Result<RedirectOrContent, StatusOrError<Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(ref value) = form.value {
        if data.is_started(&mut transaction).await? {
            form.context.push_error(form::Error::validation("You can no longer enter this event since it has already started."));
        }
        match data.team_config {
            TeamConfig::Solo => {
                let mut request_qualifier = None;
                if let Some(Flow { ref requirements, closes }) = data.enter_flow {
                    if closes.is_some_and(|closes| closes <= Utc::now()) {
                        form.context.push_error(form::Error::validation("The deadline to enter this event has passed."));
                    } else if requirements.is_empty() {
                        if data.is_single_race() {
                            form.context.push_error(form::Error::validation("Signups for this event are not handled by Hyrule Town Hall."));
                        }
                    } else {
                        for requirement in requirements {
                            requirement.check_form(&mut transaction, http_client, discord_ctx, &me, &data, &mut form.context, value).await?;
                            if let Some(async_kind) = requirement.request_qualifier(&mut transaction, http_client, discord_ctx, &me, &data).await? {
                                request_qualifier = Some(async_kind);
                            }
                        }
                    }
                } else {
                    form.context.push_error(form::Error::validation("Signups for this event aren't open yet."));
                }
                if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
                    id = team
                    AND series = $1
                    AND event = $2
                    AND member = $3
                    AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                ) AS "exists!""#, series as _, event, me.id as _).fetch_one(&mut *transaction).await? {
                    form.context.push_error(form::Error::validation("You are already signed up for this event."));
                }
                if form.context.errors().next().is_none() {
                    let id = Id::<Teams>::new(&mut transaction).await?;
                    // Merge old-style boolean choices into custom_choices
                    let mut custom_choices = value.custom_choices.clone();
                    if value.all_dungeons_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("all_dungeons")).or_insert(format!("yes")); }
                    if value.flute_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("flute")).or_insert(format!("yes")); }
                    if value.hard_settings_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("hard_settings")).or_insert(format!("yes")); }
                    if value.hover_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("hovering")).or_insert(format!("yes")); }
                    if value.inverted_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("inverted")).or_insert(format!("yes")); }
                    if value.keydrop_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("keydrop")).or_insert(format!("yes")); }
                    if value.lite_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("lite")).or_insert(format!("yes")); }
                    if value.mirror_scroll_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("mirror_scroll")).or_insert(format!("yes")); }
                    if value.mq_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("mq")).or_insert(format!("yes")); }
                    if value.no_delay_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("no_delay")).or_insert(format!("yes")); }
                    if value.pb_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("pseudoboots")).or_insert(format!("yes")); }
                    if value.zw_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("zw")).or_insert(format!("yes")); }
                    sqlx::query!(
                        "INSERT INTO teams (id, series, event, plural_name, restream_consent, text_field, text_field2, yes_no, mw_impl, custom_choices) VALUES ($1, $2, $3, FALSE, $4, $5, $6, $7, $8, $9)",
                        id as _,
                        series as _,
                        event,
                        value.restream_consent || value.restream_consent_radio == Some(BoolRadio::Yes),
                        value.text_field,
                        value.text_field2,
                        value.yes_no == Some(BoolRadio::Yes),
                        value.mw_impl as _,
                        sqlx::types::Json(&custom_choices) as _,
                    ).execute(&mut *transaction).await?;
                    sqlx::query!("INSERT INTO team_members (team, member, status, role) VALUES ($1, $2, 'created', 'none')", id as _, me.id as _).execute(&mut *transaction).await?;
                    if let Some(async_kind) = request_qualifier {
                        sqlx::query!("INSERT INTO async_teams (team, kind, requested) VALUES ($1, $2, NOW())", id as _, async_kind as _).execute(&mut *transaction).await?;
                    }
                    if let (Some(discord_user), Some(discord_guild)) = (me.discord.as_ref(), data.discord_guild) {
                        let discord_ctx = discord_ctx.read().await;
                        if let Some(PgSnowflake(participant_role)) = sqlx::query_scalar!(r#"SELECT id AS "id: PgSnowflake<RoleId>" FROM discord_roles WHERE guild = $1 AND series = $2 AND event = $3"#, PgSnowflake(discord_guild) as _, series as _, event).fetch_optional(&mut *transaction).await? {
                            if let Ok(member) = discord_guild.member(&*discord_ctx, discord_user.id).await {
                                member.add_role(&*discord_ctx, participant_role).await?;
                            }
                        }
                    }
                    let Flow { ref requirements, .. } = data.enter_flow.expect("checked above");
                    for requirement in requirements {
                        if let Requirement::StartGG { optional } = requirement {
                            let discord_ctx = discord_ctx.read().await;
                            if !optional || value.startgg_radio == Some(BoolRadio::Yes) {
                                // enter event on start.gg with user ID
                                // this is currently not possible to automate, see conversation ending at <https://discord.com/channels/339548254704369677/541015301618401301/1346621619787006083> for details
                                // temporary workaround until this is automated:
                                let startgg_id = me.startgg_id.as_ref().expect("checked by requirement");
                                let mut msg = MessageBuilder::default();
                                msg.mention_user(&me);
                                msg.push(" signed up for ");
                                msg.push_safe(&data.display_name);
                                let response = startgg::query_cached::<startgg::UserSlugQuery>(http_client, &config.startgg, startgg::user_slug_query::Variables { id: startgg_id.clone() }).await?;
                                if let startgg::user_slug_query::ResponseData { user: Some(startgg::user_slug_query::UserSlugQueryUser { discriminator: Some(slug) }) } = response {
                                    msg.push(" with start.gg user slug ");
                                    msg.push_mono_safe(slug);
                                    msg.push(". Please go to the start.gg tournament settings › Attendees › Add Attendee, paste the user slug into the search field, and select the first result.");
                                } else {
                                    msg.push(" with unknown start.gg user ID ");
                                    msg.push_mono_safe(&startgg_id.0);
                                    msg.push(". ");
                                    msg.mention(&FENHL);
                                    msg.push(" please investigate.");
                                }
                                if let Some(organizer_channel) = data.discord_organizer_channel {
                                    organizer_channel.say(&*discord_ctx, msg.build()).await?;
                                } else {
                                    ADMIN_USER.create_dm_channel(&*discord_ctx).await?.say(&*discord_ctx, msg.build()).await?;
                                }
                            } else {
                                // enter event on start.gg anonymously
                                // this is currently not possible to automate, see conversation ending at <https://discord.com/channels/339548254704369677/541015301618401301/1346621619787006083> for details
                                // temporary workaround until this is automated:
                                let msg = MessageBuilder::default()
                                    .mention_user(&me)
                                    .push(" signed up for ")
                                    .push_safe(&data.display_name)
                                    .push(" without start.gg account. Please go to the start.gg tournament settings › Attendees › Add Attendee, click the search field, then click “Or add someone without an account”, then enter ")
                                    .push_mono_safe(me.display_name())
                                    .push(" as the gamertag. Please notify Fenhl once this is done so they can connect the attendee to the Mido's House account.")
                                    .build();
                                if let Some(organizer_channel) = data.discord_organizer_channel {
                                    organizer_channel.say(&*discord_ctx, msg).await?;
                                } else {
                                    ADMIN_USER.create_dm_channel(&*discord_ctx).await?.say(&*discord_ctx, msg).await?;
                                }
                            }
                        }
                    }
                    transaction.commit().await?;
                    return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(super::status(series, event)))))
                }
            }
            TeamConfig::Pictionary => {
                let (my_role, teammate) = match (value.my_role, value.teammate) {
                    (Some(my_role), Some(teammate)) => {
                        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
                            id = team
                            AND series = $1
                            AND event = $2
                            AND member = $3
                            AND EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $4)
                        ) AS "exists!""#, series as _, event, me.id as _, teammate as _).fetch_one(&mut *transaction).await? {
                            form.context.push_error(form::Error::validation("A team with these members is already proposed for this race. Check your notifications to accept the invite, or ask your teammate to do so.")); //TODO linkify notifications? More specific message based on whether viewer has confirmed?
                        }
                        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
                            id = team
                            AND series = $1
                            AND event = $2
                            AND member = $3
                            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                        ) AS "exists!""#, series as _, event, me.id as _).fetch_one(&mut *transaction).await? {
                            form.context.push_error(form::Error::validation("You are already signed up for this race."));
                        }
                        if !value.team_name.is_empty() && sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE
                            series = $1
                            AND event = $2
                            AND name = $3
                            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                        ) AS "exists!""#, series as _, event, value.team_name).fetch_one(&mut *transaction).await? {
                            form.context.push_error(form::Error::validation("A team with this name is already signed up for this race.").with_name("team_name"));
                        }
                        if my_role == pic::Role::Sheikah && me.racetime.is_none() {
                            form.context.push_error(form::Error::validation("A racetime.gg account is required to enter as runner. Go to your profile and select 'Connect a racetime.gg account'.").with_name("my_role")); //TODO direct link?
                        }
                        if teammate == me.id {
                            form.context.push_error(form::Error::validation("You cannot be your own teammate.").with_name("teammate"));
                        }
                        if !sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM users WHERE id = $1) AS "exists!""#, teammate as _).fetch_one(&mut *transaction).await? {
                            form.context.push_error(form::Error::validation("There is no user with this ID.").with_name("teammate"));
                        }
                        if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
                            id = team
                            AND series = $1
                            AND event = $2
                            AND member = $3
                            AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                        ) AS "exists!""#, series as _, event, teammate as _).fetch_one(&mut *transaction).await? {
                            form.context.push_error(form::Error::validation("This user is already signed up for this race.").with_name("teammate"));
                        }
                        //TODO check to make sure the teammate hasn't blocked the user submitting the form (or vice versa) or the event
                        (Some(my_role), Some(teammate))
                    }
                    (Some(_), None) => {
                        form.context.push_error(form::Error::validation("This field is required.").with_name("teammate"));
                        (None, None)
                    }
                    (None, Some(_)) => {
                        form.context.push_error(form::Error::validation("This field is required.").with_name("my_role"));
                        (None, None)
                    }
                    (None, None) => {
                        form.context.push_error(form::Error::validation("This field is required.").with_name("my_role"));
                        form.context.push_error(form::Error::validation("This field is required.").with_name("teammate"));
                        (None, None)
                    }
                };
                if form.context.errors().next().is_none() {
                    let id = Id::<Teams>::new(&mut transaction).await?;
                    // Merge old-style boolean choices into custom_choices
                    let mut custom_choices = value.custom_choices.clone();
                    if value.all_dungeons_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("all_dungeons")).or_insert(format!("yes")); }
                    if value.flute_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("flute")).or_insert(format!("yes")); }
                    if value.hard_settings_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("hard_settings")).or_insert(format!("yes")); }
                    if value.hover_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("hovering")).or_insert(format!("yes")); }
                    if value.inverted_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("inverted")).or_insert(format!("yes")); }
                    if value.keydrop_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("keydrop")).or_insert(format!("yes")); }
                    if value.lite_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("lite")).or_insert(format!("yes")); }
                    if value.mirror_scroll_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("mirror_scroll")).or_insert(format!("yes")); }
                    if value.mq_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("mq")).or_insert(format!("yes")); }
                    if value.no_delay_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("no_delay")).or_insert(format!("yes")); }
                    if value.pb_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("pseudoboots")).or_insert(format!("yes")); }
                    if value.zw_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("zw")).or_insert(format!("yes")); }
                    sqlx::query!(
                    "INSERT INTO teams (id, series, event, name, restream_consent, text_field, text_field2, yes_no, mw_impl, custom_choices) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                        id as _,
                        series as _,
                        event,
                        (!value.team_name.is_empty()).then(|| &value.team_name),
                        value.restream_consent || value.restream_consent_radio == Some(BoolRadio::Yes),
                        value.text_field,
                        value.text_field2,
                        value.yes_no == Some(BoolRadio::Yes),
                        value.mw_impl as _,
                        sqlx::types::Json(&custom_choices) as _,
                    ).execute(&mut *transaction).await?;
                    sqlx::query!("INSERT INTO team_members (team, member, status, role) VALUES ($1, $2, 'created', $3)", id as _, me.id as _, Role::from(my_role.expect("validated")) as _).execute(&mut *transaction).await?;
                    sqlx::query!("INSERT INTO team_members (team, member, status, role) VALUES ($1, $2, 'unconfirmed', $3)", id as _, teammate.expect("validated") as _, match my_role.expect("validated") { pic::Role::Sheikah => Role::Gerudo, pic::Role::Gerudo => Role::Sheikah } as _).execute(&mut *transaction).await?;
                    transaction.commit().await?;
                    return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(super::status(series, event)))))
                }
            }
            team_config => {
                let racetime_team = if let Some(ref racetime_team) = value.racetime_team {
                    match me.racetime_user_data(http_client).await? {
                        Some(Some(user)) => if user.teams.iter().any(|team| team.slug == *racetime_team) {
                            let team = http_client.get(format!("https://{}/team/{racetime_team}/data", racetime_host()))
                                .send().await?
                                .detailed_error_for_status().await?
                                .json_with_text_in_error::<mw::RaceTimeTeamData>().await?;
                            let expected_size = team_config.roles().len();
                            if team.members.len() != expected_size {
                                form.context.push_error(form::Error::validation(format!("Teams for this event must have exactly {expected_size} members, but this team has {}", team.members.len())))
                            }
                            //TODO get each team member's Mido's House account for displaying in step 2
                            Some(team)
                        } else {
                            form.context.push_error(form::Error::validation("This racetime.gg team does not exist or you're not in it.").with_name("racetime_team"));
                            None
                        },
                        Some(None) => {
                            form.context.push_error(form::Error::validation("Your racetime.gg profile is not public. Please connect a Twitch or Patreon account to your racetime.gg account or participate in a recorded race."));
                            None
                        }
                        None => {
                            form.context.push_error(form::Error::validation("A racetime.gg account is required to enter this event. Go to your profile and select 'Connect a racetime.gg account'.")); //TODO direct link?
                            None
                        }
                    }
                } else {
                    form.context.push_error(form::Error::validation("This field is required.").with_name("racetime_team"));
                    None
                };
                let (team_slug, team_name, users, roles, startgg_ids) = if value.step2 {
                    if let Some(ref racetime_team) = racetime_team {
                        let mut all_accounts_exist = true;
                        let mut users = Vec::default();
                        let mut roles = if team_config.has_distinct_roles() {
                            Vec::default()
                        } else {
                            team_config.roles().iter().map(|&(role, _)| role).collect()
                        };
                        let mut startgg_ids = Vec::default();
                        for member in &racetime_team.members {
                            if let Some(user) = User::from_racetime(&mut *transaction, &member.id).await? {
                                if let Some(ref discord) = user.discord {
                                    if let Some(discord_guild) = data.discord_guild {
                                        if discord_guild.member(&*discord_ctx.read().await, discord.id).await.is_err() {
                                            //TODO only check if Requirement::DiscordGuild is present
                                            form.context.push_error(form::Error::validation("This user has not joined the tournament's Discord server.").with_name(format!("roles[{}]", member.id)));
                                        }
                                    }
                                } else {
                                    //TODO only check if Requirement::Discord is present
                                    form.context.push_error(form::Error::validation("This Hyrule Town Hall account is not associated with a Discord account.").with_name(format!("roles[{}]", member.id)));
                                }
                                if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams, team_members WHERE
                                    id = team
                                    AND series = $1
                                    AND event = $2
                                    AND member = $3
                                    AND NOT EXISTS (SELECT 1 FROM team_members WHERE team = id AND status = 'unconfirmed')
                                ) AS "exists!""#, series as _, event, user.id as _).fetch_one(&mut *transaction).await? {
                                    form.context.push_error(form::Error::validation("This user is already signed up for this tournament."));
                                }
                                users.push(user);
                            } else {
                                form.context.push_error(form::Error::validation("This racetime.gg account is not associated with a Hyrule Town Hall account.").with_name(format!("roles[{}]", member.id)));
                                all_accounts_exist = false;
                            }
                            if team_config.has_distinct_roles() {
                                if let Some(&role) = value.roles.get(&member.id) {
                                    roles.push(role);
                                } else {
                                    form.context.push_error(form::Error::validation("This field is required.").with_name(format!("roles[{}]", member.id)));
                                }
                            }
                            if let Some(id) = value.startgg_id.get(&member.id) {
                                if id.is_empty() {
                                    startgg_ids.push(None);
                                } else if !regex_is_match!("^[0-9a-f]{8}$", id) {
                                    form.context.push_error(form::Error::validation("User IDs on start.gg are exactly 8 characters in length and consist only of digits and the letters a–f. You can find a start.gg user's ID on their profile next to their name.").with_name(format!("startgg_id[{}]", member.id)));
                                } else {
                                    startgg_ids.push(Some(id.clone()));
                                }
                            } else {
                                startgg_ids.push(None);
                            }
                        }
                        if all_accounts_exist {
                            match &*users {
                                [u1, u2] => if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE
                                    series = $1
                                    AND event = $2
                                    AND EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $3)
                                    AND EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $4)
                                ) AS "exists!""#, series as _, event, u1.id as _, u2.id as _).fetch_one(&mut *transaction).await? {
                                    form.context.push_error(form::Error::validation("A team with these members is already proposed for this tournament. Check your notifications to accept the invite, or ask your teammate to do so.")); //TODO linkify notifications? More specific message based on whether viewer has confirmed?
                                },
                                [u1, u2, u3] => if sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE
                                    series = $1
                                    AND event = $2
                                    AND EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $3)
                                    AND EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $4)
                                    AND EXISTS (SELECT 1 FROM team_members WHERE team = id AND member = $5)
                                ) AS "exists!""#, series as _, event, u1.id as _, u2.id as _, u3.id as _).fetch_one(&mut *transaction).await? {
                                    form.context.push_error(form::Error::validation("A team with these members is already proposed for this tournament. Check your notifications to accept the invite, and/or ask your teammates to do so.")); //TODO linkify notifications? More specific message based on whether viewer has confirmed?
                                },
                                _ => unimplemented!("exact proposed team check for {} members", users.len()),
                            }
                        }
                        for (required_role, label) in team_config.roles() {
                            let mut found = false;
                            for (member_id, role) in &value.roles {
                                if role == required_role {
                                    if found {
                                        form.context.push_error(form::Error::validation("Each team member must have a different role.").with_name(format!("roles[{member_id}]")));
                                    } else {
                                        found = true;
                                    }
                                }
                            }
                            if !found {
                                form.context.push_error(form::Error::validation(format!("No team member is assigned as {label}.")));
                            }
                        }
                        match team_config {
                            TeamConfig::CoOp => {
                                if value.text_field.is_empty() {
                                    form.context.push_error(form::Error::validation("This field is required.").with_name("text_field"));
                                }
                                if value.text_field2.is_empty() {
                                    form.context.push_error(form::Error::validation("This field is required.").with_name("text_field2"));
                                }
                            }
                            TeamConfig::Multiworld => if value.mw_impl.is_none() {
                                form.context.push_error(form::Error::validation("This field is required.").with_name("mw_impl"));
                            },
                            _ => {}
                        }
                        (racetime_team.slug.clone(), racetime_team.name.clone(), users, roles, startgg_ids)
                    } else {
                        Default::default()
                    }
                } else {
                    Default::default()
                };
                if form.context.errors().next().is_none() {
                    return Ok(if value.step2 {
                        let id = Id::<Teams>::new(&mut transaction).await?;
                        // Merge old-style boolean choices into custom_choices
                        let mut custom_choices = value.custom_choices.clone();
                        if value.all_dungeons_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("all_dungeons")).or_insert(format!("yes")); }
                        if value.flute_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("flute")).or_insert(format!("yes")); }
                        if value.hard_settings_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("hard_settings")).or_insert(format!("yes")); }
                        if value.hover_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("hovering")).or_insert(format!("yes")); }
                        if value.inverted_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("inverted")).or_insert(format!("yes")); }
                        if value.keydrop_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("keydrop")).or_insert(format!("yes")); }
                        if value.lite_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("lite")).or_insert(format!("yes")); }
                        if value.mirror_scroll_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("mirror_scroll")).or_insert(format!("yes")); }
                        if value.mq_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("mq")).or_insert(format!("yes")); }
                        if value.no_delay_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("no_delay")).or_insert(format!("yes")); }
                        if value.pb_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("pseudoboots")).or_insert(format!("yes")); }
                        if value.zw_ok == Some(BoolRadio::Yes) { custom_choices.entry(format!("zw")).or_insert(format!("yes")); }
                        sqlx::query!(
                            "INSERT INTO teams (id, series, event, name, racetime_slug, restream_consent, text_field, text_field2, yes_no, mw_impl, custom_choices) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                            id as _,
                            series as _,
                            event,
                            (!team_name.is_empty()).then(|| team_name),
                            team_slug,
                            value.restream_consent || value.restream_consent_radio == Some(BoolRadio::Yes),
                            value.text_field,
                            value.text_field2,
                            value.yes_no == Some(BoolRadio::Yes),
                            value.mw_impl as _,
                        sqlx::types::Json(&custom_choices) as _,
                        ).execute(&mut *transaction).await?;
                        for ((user, role), startgg_id) in users.into_iter().zip_eq(roles).zip_eq(startgg_ids) {
                            sqlx::query!(
                                "INSERT INTO team_members (team, member, status, role, startgg_id) VALUES ($1, $2, $3, $4, $5)",
                                id as _, user.id as _, if user == me { SignupStatus::Created } else { SignupStatus::Unconfirmed } as _, role as _, startgg_id,
                            ).execute(&mut *transaction).await?;
                        }
                        transaction.commit().await?;
                        RedirectOrContent::Redirect(Redirect::to(uri!(super::status(series, event))))
                    } else {
                        RedirectOrContent::Content(enter_form_step2(transaction, Some(me), uri, http_client, csrf.as_ref(), data, mw::EnterFormStep2Defaults::Values { racetime_team: racetime_team.expect("validated") }).await?)
                    })
                }
            }
        }
        if value.step2 {
            return Ok(RedirectOrContent::Content(enter_form_step2(transaction, Some(me), uri, http_client, csrf.as_ref(), data, mw::EnterFormStep2Defaults::Context(form.context)).await?))
        }
    }
    Ok(RedirectOrContent::Content(enter_form(transaction, http_client, discord_ctx, Some(me), uri, csrf.as_ref(), data, pic::EnterFormDefaults::Context(form.context)).await?))
}
