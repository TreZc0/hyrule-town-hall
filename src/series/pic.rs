use crate::{
    event::{
        Data,
        Error,
        FindTeamError,
        InfoError,
        Tab,
        enter,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    let is_random_settings = data.event.starts_with("rs");
    let settings = match &*data.event {
        "5" => html! {
            ul {
                li : "S5 base";
                li : "CSMC off";
                li : "no hints (including altar)";
                li : "Ganon boss key on 20 hearts";
                li : "vanilla bridge (Shadow and Spirit medallions + light arrows)";
                li : "cowsanity";
                li : "dungeon skulls";
                li : "30/40/50 skulls disabled";
                li : "shops 4 (reminder: no numbers allowed)";
            }
            p {
                : "The seed will be rolled on ";
                a(href = "https://github.com/fenhl/OoT-Randomizer/tree/valentine-pictionary") : "a custom branch";
                : " to support the heart wincon. The branch is based on Dev 6.2.1 and contains these settings as a preset called “5th Pictionary Spoiler Log Race”.";
            }
        },
        "6" => html! {
            p {
                : "The seed will be rolled on ";
                a(href = "https://ootrandomizer.com/generatorDev?version=devFenhl_6.2.206") : "version 6.2.206 Fenhl-5";
                : " of the randomizer. That version contains these settings as a preset called “6th Pictionary Spoiler Log Race”.";
            }
            ul {
                li : "weekly base";
                li : "CAMC off";
                li : "no hints (including altar)";
                li : "Ganon boss key on LACS";
                li : "vanilla dungeon rewards (i.e. you'll need to beat Shadow and Spirit temple)";
                li : "full dungeon ER (including Ganon's castle)";
                li : "13 token bridge";
                li : "all skulls shuffled";
                li : "40 and 50 skulls disabled";
                li : "closed Deku";
                li : "keyrings shuffled in their own dungeons";
                li : "3 big Poes";
                li : "midnight start";
            }
        },
        "7" => html! {
            p {
                : "The seed will be rolled on ";
                a(href = "https://github.com/OoTRandomizer/OoT-Randomizer/pull/2064") : "pull request #2064";
                : " which is based on version 7.1.166 of the randomizer.";
            }
            ul {
                li : "S6 base";
                li : "CAMC off";
                li : "no hints (including altar)";
                li : "shuffle songs anywhere";
                li : "shuffle ocarinas (no ocarina start)";
                li : "shuffle ocarina note buttons";
                li : "randomize song melodies (including frogs 2)";
                li : "randomize warp song destinations";
                li : "shuffle frog song rupees";
                li : "shuffle cows (house cow disabled)";
                li : "child start, closed Door of Time";
            }
        },
        "rs1" => html! {
            p {
                : "The seed will be rolled on ";
                a(href = "https://github.com/fenhl/plando-random-settings/tree/a08223927138c6f039c1aa3603130d8bd900fb48") : "version 2.2.10 Fenhl-5";
                : " of the random settings script. We will be using ";
                a(href = "https://github.com/fenhl/plando-random-settings/blob/a08223927138c6f039c1aa3603130d8bd900fb48/weights/pictionary_override.json") : "a special weights override";
                : " for Pictionary spoiler log races. Changes include:";
            }
            ul {
                li : "To reduce complexity for the pilot, overworld ER is disabled.";
                li : "Master Quest dungeons are disabled due to a lack of documentation for spoiler log location names.";
                li {
                    : "Some of the settings and combinations of settings that are disabled in RSL for information-related reasons are turned back on, since they're not an issue if you have the spoiler log:";
                    ul {
                        li : "Triforce hunt + minimal item pool";
                        li : "Ice trap mayhem/onslaught + quad damage/OHKO";
                        li : "Separate keysanity setting for the Thieves' Hideout";
                        li : "Random scrub prices without a starting wallet";
                        li : "All goals reachable (33% chance)";
                    }
                }
                li {
                    : "The seed will be rolled on ";
                    a(href = "https://github.com/fenhl/OoT-Randomizer/tree/d7d16553252b96bd0f50ef96c2af250b7bfbba58") : "Fenhl's branch";
                    : ", so some settings that aren't in Dev-R are added:";
                    ul {
                        li : "Heart container requirements for rainbow bridge and/or Ganon boss key (50% chance each to replace a skulltula token requirement)";
                        li : "Full one-way entrance randomization (owls, warp songs, and spawns can lead to more destinations; 25% chance each)";
                        li : "One bonk KO (5% chance)";
                        li : "Closed Kokiri Forest exit (50% chance, independent of Closed/Open Deku)";
                    }
                }
                li {
                    : "Some newer settings that are not yet included in RSL due to the ongoing tournament are enabled:";
                    ul {
                        li : "Planted magic beans (50% chance)";
                        li : "Key rings for all dungeons (20% chance)";
                    }
                }
                li {
                    : "The following settings that would give the runner hints or similar information are disabled:";
                    ul {
                        li : "Maps & compasses give info";
                        li : "Chest appearance matches contents";
                        li : "Gossip stone hints";
                        li : "Temple of Time altar hints";
                        li : "Ganondorf light arrows hint";
                        li : "Warp song text boxes hinting destinations";
                    }
                }
            }
            p {
                : "Everything else is the same as ";
                a(href = "https://rsl-leaderboard.web.app/weights") : "the usual RSL weights";
                : ".";
            }
        },
        "rs2" => html! {
            p {
                : "The seed will be rolled on ";
                a(href = "https://github.com/fenhl/plando-random-settings/tree/e15d97185093ae7dafa7a4e5ee9bf7fe7ced42dc") : "version 2.3.8 Fenhl-14";
                : " of the random settings script. We will be using ";
                a(href = "https://github.com/fenhl/plando-random-settings/blob/e15d97185093ae7dafa7a4e5ee9bf7fe7ced42dc/weights/pictionary_override.json") : "a special weights override";
                : " for Pictionary spoiler log races. Changes include:";
            }
            ul {
                li : "Overworld ER is disabled to reduce complexity for the pilot.";
                li : "Master Quest dungeons are disabled due to a lack of documentation for spoiler log location names.";
                li {
                    : "Some of the settings and combinations of settings that are disabled in RSL for information-related reasons are turned back on, since they're not an issue if you have the spoiler log:";
                    ul {
                        li : "Ice trap mayhem/onslaught + quad damage/OHKO";
                        li : "Separate key shuffle setting for the Thieves' Hideout";
                        li : "Random scrub prices without a starting wallet";
                        li : "All goals reachable (33% chance)";
                        li : "Boss keys in overworld, any dungeon, or regional";
                    }
                }
                li {
                    : "The seed will be rolled on ";
                    a(href = "https://github.com/fenhl/OoT-Randomizer/tree/ff5ba67fc1e66304332b0e8e5d43ba95c0231b4e") : "Fenhl's branch";
                    : ", so some settings that aren't in Dev-R are added:";
                    ul {
                        li : "Boss rooms included in mixed entrance pools (50% chance if mixed pools is on)";
                        li : "Triforce Hunt variants: Ice% (single piece in the iron boots chest) and Triforce Blitz (3 pieces found in dungeons), 5% chance each";
                        li : "Shuffled dungeon rewards (vanilla, own dungeon, regional, overworld, any dungeon, or anywhere; 5% chance each)";
                        li : "Shuffled silver rupees (same weights as small key shuffle) with silver rupee pouches (20% chance)";
                        li : "Closed Kokiri Forest exit (50% chance, independent of Closed/Open Deku) with a 5% chance of Require Gohma";
                        li : "Shuffled Thieves' Hideout entrances (50% chance if interiors are shuffled)";
                        li : "Shuffled blue warps (vanilla, dungeon entrance, or shuffled)";
                        li : "Full one-way entrance randomization (owls, warp songs, spawns, blue warps, and the Gerudo Valley river exit can lead to more destinations; 25% chance each)";
                        li : "Only one one-way entrance of any type goes to a given hint area (50% chance)";
                        li : "Vanilla song locations (5% chance)";
                        li : "Vanilla base item pool (5% chance)";
                    }
                }
                li {
                    : "Some newer settings that are not yet included in RSL due to the ongoing tournament are enabled:";
                    ul {
                        li : "Shuffled Ganon's Castle entrance (25% chance)";
                        li : "Shuffled beehives (50% chance)";
                        li : "Keyrings give boss keys (50% chance)";
                        li : "Shuffled Gerudo Valley river exit (50% chance)";
                    }
                }
                li {
                    : "The following settings that would give the runner hints or similar information are disabled:";
                    ul {
                        li : "Maps & compasses give info";
                        li : "Chest appearance matches contents";
                        li : "Gossip stone hints";
                        li : "Temple of Time altar hints";
                        li : "Ganondorf & Dampé diary light arrow hints";
                        li : "Warp song text boxes hinting destinations";
                    }
                }
            }
            p {
                : "Everything else is the same as ";
                a(href = "https://rsl-leaderboard.web.app/weights") : "the usual RSL weights";
                : ".";
            }
        },
        _ => return Ok(None),
    };
    let sample_seeds = match &*data.event {
        "5" => Some(seed::table(stream::iter(vec![
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_F35CF_7F1NK3FEGY"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_F35CF_XULLQE310I"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_F35CF_3PT90NK69D"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_F35CF_I7BN7K3S2Z"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_F35CF_99YI7I0K6O"), locked_spoiler_log_path: None }), progression_spoiler: false },
        ]), true, transaction, 1).await?),
        "rs1" => Some(seed::table(stream::iter(vec![
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoTR_1079630_V6516H22IW"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoTR_1079637_HAH75EOAHQ"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoTR_1079645_6XZJOSDCRW"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoTR_1079646_AJZWAB1X3U"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoTR_1079648_1DHCCQB5AC"), locked_spoiler_log_path: None }), progression_spoiler: false },
        ]), true, transaction, 1).await?),
        "rs2" => Some(seed::table(stream::iter(vec![
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_5ADE7_1S6GBQNP8R"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_5ADE7_IIPBIQ4XAB"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_5ADE7_LBZIZMD75C"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_5ADE7_3OBW74243M"), locked_spoiler_log_path: None }), progression_spoiler: false },
            seed::Data { file_hash: None, password: None, files: Some(seed::Files::MidosHouse { file_stem: Cow::Borrowed("OoT_5ADE7_E18HE17UKF"), locked_spoiler_log_path: None }), progression_spoiler: false },
        ]), true, transaction, 1).await?),
        _ => None,
    };
    let winniedemon = User::from_id(&mut **transaction, Id::from(1807131022132982871_u64)).await?.ok_or(Error::OrganizerUserData)?;
    Ok(Some(html! {
        article {
            h2 : "What is a Pictionary Spoiler Log Race?";
            p : "Each team consists of one Runner and one Spoiler Log Pilot who is drawing. The pilot has to figure out a way through the seed and how to tell their runner in drawing what checks they need to do. Hints are obviously disabled.";
            @if is_random_settings {
                p : "This time, we are doing something slightly different: The settings will be random, with weights based on the Random Settings League but adjusted for Pictionary. To compensate for the additional complexity, the preparation time for the pilot will be 30 minutes instead of the usual 15.";
            }
            p {
                : "Before the race we will provide a room on ";
                @match data.base_start.map(|base_start| base_start.year()) {
                    Some(..=2022) => a(href = "https://aggie.io/") : "aggie.io";
                    Some(2023) => {
                        a(href = "https://magma.com/") : "magma.com";
                        : " (formerly known as aggie.io)";
                    }
                    Some(2024..) | None => a(href = "https://magma.com/") : "magma.com";
                }
                : " to each team. The canvas will be set to 660×460 for restream purposes.";
            }
            p {
                strong : "At the ±0 minute mark:";
                : " The pilot is now allowed to look at the spoiler log and can start figuring out the route.";
            }
            p {
                strong {
                    : "At the ";
                    @if is_random_settings {
                        : "+30";
                    } else {
                        : "+15";
                    }
                    : " minute mark:";
                }
                : " The pilot is allowed to start drawing and the runner is allowed to start the file.";
            }
            h2 : "Rules";
            p {
                : "The race uses the ";
                @if is_random_settings {
                    a(href = "https://rsl-leaderboard.web.app/rules") : "Random Settings League";
                } else {
                    a(href = "https://wiki.ootrandomizer.com/index.php?title=Standard") : "Standard";
                }
                : " ruleset.";
            }
            p : "The pilot is allowed to communicate to their partner only via drawing and may watch and hear the stream of the runner. Runners may talk to their pilot. We would prefer if the pilot did not directly respond to questions, as figuring things out is supposed to be part of the challenge, but in the end it's up to the individual teams.";
            p {
                strong : "Allowed:";
                : " Arrows, question marks, ingame symbols, check marks, “X” for crossing out stuff.";
            }
            p {
                strong : "Not allowed:";
                : " Any kind of numbers or letters.";
            }
            h3 : "Examples";
            p : "For having a better idea what we mean in regards with the rules / communication, here are some examples:";
            ol {
                li {
                    : "The pilot draws 3 spiders and a bow. The runner then asks if there is a bow on 30 skulls. The pilot then draws a smiley or a checkmark for confirmation or a sad face for “no” — that is ";
                    strong : "allowed";
                    : ".";
                }
                li {
                    : "The runner just asks without a drawing if it's AD or if a specific check is required — that is ";
                    strong : "not allowed";
                    : ".";
                }
                li {
                    : "The team has prepared a language for specific checks to avoid the requirement to draw the check (like morse code etc.) — that is ";
                    strong : "not allowed";
                    : ".";
                }
                li {
                    : "The runner says “if I need to do the toilet check, draw a heart” — that is ";
                    strong : "not allowed";
                    : ".";
                }
                li {
                    : "The runner says: “since you didn't draw anything in the Lost Woods, I'm gonna skip all the checks there and go immediately to the Sacred Forest Meadow” — that is ";
                    strong : "allowed";
                    : ".";
                }
            }
            h2 : "Settings";
            : settings;
            @if let Some(sample_seeds) = sample_seeds {
                h2 : "Sample seeds";
                p {
                    @if is_random_settings {
                        : "Since the random settings script isn't available online";
                    } else {
                        : "Since the branch we're using isn't available on the website";
                    }
                    : ", we've prepared some sample seeds:";
                }
                : sample_seeds;
                @if data.event == "5" {
                    p {
                        a(href = "https://ootr.fenhl.net/static/pictionary5-sample-seeds-batch2.zip") : "Download all";
                    }
                    p {
                        : "You can apply these patch files using ";
                        a(href = "https://ootrandomizer.com/generator") : "the regular web patcher";
                        : ".";
                    }
                    p {
                        strong : "Note:";
                        : " These sample seeds were posted on February 11, replacing ";
                        a(href = "https://ootr.fenhl.net/static/pictionary5-sample-seeds.zip") : "the original batch";
                        : " which had a bug where the spoiler log would show the wrong prices for most right-side shop items. Special thanks to ShadowShine57 who found ";
                        a(href = "https://github.com/OoTRandomizer/OoT-Randomizer/pull/1505") : "the fix";
                        : " for that bug.";
                    }
                }
            }
            h2 : "Further information";
            p {
                : "The race is organized by ";
                : English.join_html_opt(data.organizers(transaction).await?);
                : ". We will answer questions and inform about recent events on The Silver Gauntlets Discord in the #pictionary-spoiler-log channel (";
                a(href = "https://discord.gg/m8z8ZqtN8H") : "invite link";
                : " • ";
                a(href = "https://discord.com/channels/663207960432082944/865206020015128586") : "direct channel link";
                : "). If you have any questions, feel free to ask there!";
            }
            p {
                : "Special thanks to ";
                : winniedemon;
                : " who will be helping us keep important posts from getting lost in the Discord!";
            }
        }
    }))
}

#[derive(Clone, Copy, PartialEq, Eq, FromFormField, UriDisplayQuery)]
pub(crate) enum Role {
    #[field(value = "sheikah")]
    Sheikah,
    #[field(value = "gerudo")]
    Gerudo,
}

impl ToHtml for Role {
    fn to_html(&self) -> RawHtml<String> {
        match self {
            Self::Sheikah => html! {
                span(class = "sheikah") : "runner";
            },
            Self::Gerudo => html! {
                span(class = "gerudo") : "pilot";
            },
        }
    }
}

impl TryFrom<event::Role> for Role {
    type Error = ();

    fn try_from(role: event::Role) -> Result<Self, ()> {
        match role {
            event::Role::Sheikah => Ok(Self::Sheikah),
            event::Role::Gerudo => Ok(Self::Gerudo),
            _ => Err(()),
        }
    }
}

impl From<Role> for event::Role {
    fn from(role: Role) -> Self {
        match role {
            Role::Sheikah => Self::Sheikah,
            Role::Gerudo => Self::Gerudo,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, sqlx::Type, FromFormField)]
#[sqlx(type_name = "role_preference", rename_all = "snake_case")]
pub(crate) enum RolePreference {
    #[field(value = "sheikah_only")]
    SheikahOnly,
    #[field(value = "sheikah_preferred")]
    SheikahPreferred,
    #[default]
    #[field(value = "no_preference")]
    NoPreference,
    #[field(value = "gerudo_preferred")]
    GerudoPreferred,
    #[field(value = "gerudo_only")]
    GerudoOnly,
}

impl ToHtml for RolePreference {
    fn to_html(&self) -> RawHtml<String> {
        match self {
            Self::SheikahOnly => html! {
                span(class = "sheikah") : "runner only";
            },
            Self::SheikahPreferred => html! {
                span(class = "sheikah") : "runner preferred";
            },
            Self::NoPreference => html! {
                : "no preference";
            },
            Self::GerudoPreferred => html! {
                span(class = "gerudo") : "pilot preferred";
            },
            Self::GerudoOnly => html! {
                span(class = "gerudo") : "pilot only";
            },
        }
    }
}

pub(crate) enum EnterFormDefaults<'v> { //TODO move to crate::event::enter
    Context(Context<'v>),
    Values {
        my_role: Option<Role>,
        teammate: Option<Id<Users>>,
    },
}

impl<'v> EnterFormDefaults<'v> {
    pub(crate) fn into_context(self) -> Context<'v> {
        as_variant!(self, Self::Context).unwrap_or_default()
    }

    pub(crate) fn errors(&self) -> Vec<&form::Error<'v>> {
        match self {
            Self::Context(ctx) => ctx.errors().collect(),
            Self::Values { .. } => Vec::default(),
        }
    }

    fn team_name(&self) -> Option<&str> {
        match self {
            Self::Context(ctx) => ctx.field_value("team_name"),
            Self::Values { .. } => None,
        }
    }

    pub(crate) fn my_role(&self) -> Option<Role> {
        match self {
            Self::Context(ctx) => match ctx.field_value("my_role") {
                Some("sheikah") => Some(Role::Sheikah),
                Some("gerudo") => Some(Role::Gerudo),
                _ => None,
            },
            &Self::Values { my_role, .. } => my_role,
        }
    }

    pub(crate) fn teammate(&self) -> Option<Id<Users>> {
        self.teammate_text().and_then(|text| text.parse().ok())
    }

    fn teammate_text(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Context(ctx) => ctx.field_value("teammate").map(Cow::Borrowed),
            &Self::Values { teammate, .. } => teammate.map(|id| Cow::Owned(id.to_string())),
        }
    }

    /// # Panics
    ///
    /// If used for fields that are present in `Self::Values`. Use the dedicated methods for those.
    pub(crate) fn field_value(&self, name: &str) -> Option<&str> {
        assert!(!matches!(name, "my_role" | "teammate"));
        match self {
            Self::Context(ctx) => ctx.field_value(name),
            Self::Values { .. } => None,
        }
    }
}

#[allow(unused_qualifications)] // rocket endpoint and uri macros don't work with relative module paths
pub(crate) async fn enter_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, defaults: EnterFormDefaults<'_>) -> Result<RawHtml<String>, Error> {
    let header = data.header(&mut transaction, me.as_ref(), Tab::Enter, false).await?;
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Enter — {}", data.display_name), if me.is_some() {
        let mut errors = defaults.errors();
        html! {
            : header;
            : full_form(uri!(enter::post(data.series, &*data.event)), csrf, html! {
                legend {
                    : "Fill out this form to enter the race as a team. Your teammate will receive an invitation they have to accept to confirm the signup. If you don't have a team yet, you can ";
                    a(href = uri!(event::find_team(data.series, &*data.event))) : "look for a teammate";
                    : " instead.";
                }
                : form_field("team_name", &mut errors, html! {
                    label(for = "team_name") : "Team Name:";
                    input(type = "text", name = "team_name", value? = defaults.team_name());
                    label(class = "help") : "(Optional unless you want to be on restream. Can be changed later. Organizers may remove inappropriate team names.)";
                });
                : form_field("my_role", &mut errors, html! {
                    label(for = "my_role") : "My Role:";
                    input(id = "my_role-sheikah", class = "sheikah", type = "radio", name = "my_role", value = "sheikah", checked? = defaults.my_role() == Some(Role::Sheikah));
                    label(class = "sheikah", for = "my_role-sheikah") : "Runner";
                    input(id = "my_role-gerudo", class = "gerudo", type = "radio", name = "my_role", value = "gerudo", checked? = defaults.my_role() == Some(Role::Gerudo));
                    label(class = "gerudo", for = "my_role-gerudo") : "Pilot";
                });
                : form_field("teammate", &mut errors, html! {
                    label(for = "teammate") : "Teammate:";
                    input(type = "text", name = "teammate", value? = defaults.teammate_text().as_deref());
                    label(class = "help") : "(Enter your teammate's Hyrule Town Hall user ID. It can be found on their profile page.)"; //TODO add JS-based user search?
                });
            }, errors, "Enter");
        }
    } else {
        html! {
            : header;
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(enter::get(data.series, &*data.event, defaults.my_role(), defaults.teammate())))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to enter this race.";
                }
            }
        }
    }).await?)
}

#[allow(unused_qualifications)] // rocket endpoint and uri macros don't work with relative module paths
pub(crate) async fn find_team_form(mut transaction: Transaction<'_, Postgres>, me: Option<User>, uri: Origin<'_>, csrf: Option<&CsrfToken>, data: Data<'_>, ctx: Context<'_>) -> Result<RawHtml<String>, FindTeamError> {
    let header = data.header(&mut transaction, me.as_ref(), Tab::FindTeam, false).await?;
    let mut my_role = None;
    let mut looking_for_team = Vec::default();
    for row in sqlx::query!(r#"SELECT user_id AS "user: Id<Users>", role AS "role: RolePreference" FROM looking_for_team WHERE series = $1 AND event = $2"#, data.series as _, &data.event).fetch_all(&mut *transaction).await? {
        let user = User::from_id(&mut *transaction, row.user).await?.ok_or(FindTeamError::UnknownUser)?;
        if me.as_ref().is_some_and(|me| user.id == me.id) { my_role = Some(row.role) }
        let can_invite = me.as_ref().is_none_or(|me| user.id != me.id) && true /*TODO not already in a team with that user */;
        looking_for_team.push((user, row.role, can_invite));
    }
    let form = if me.is_some() {
        let mut errors = ctx.errors().collect_vec();
        if my_role.is_none() {
            Some(full_form(uri!(event::find_team_post(data.series, &*data.event)), csrf, html! {
                legend {
                    : "Fill out this form to add yourself to the list below.";
                }
                : form_field("role", &mut errors, html! {
                    label(for = "role") : "Role:";
                    input(id = "role-sheikah_only", class = "sheikah", type = "radio", name = "role", value = "sheikah_only", checked? = ctx.field_value("role") == Some("sheikah_only"));
                    label(class = "sheikah", for = "role-sheikah_only") : "Runner only";
                    input(id = "role-sheikah_preferred", class = "sheikah", type = "radio", name = "role", value = "sheikah_preferred", checked? = ctx.field_value("role") == Some("sheikah_preferred"));
                    label(class = "sheikah", for = "role-sheikah_preferred") : "Runner preferred";
                    input(id = "role-no_preference", type = "radio", name = "role", value = "no_preference", checked? = ctx.field_value("role").is_none_or(|role| role == "no_preference"));
                    label(for = "role-no_preference") : "No preference";
                    input(id = "role-gerudo_preferred", class = "gerudo", type = "radio", name = "role", value = "gerudo_preferred", checked? = ctx.field_value("role") == Some("gerudo_preferred"));
                    label(class = "gerudo", for = "role-gerudo_preferred") : "Pilot preferred";
                    input(id = "role-gerudo_only", class = "gerudo", type = "radio", name = "role", value = "gerudo_only", checked? = ctx.field_value("role") == Some("gerudo_only"));
                    label(class = "gerudo", for = "role-gerudo_only") : "Pilot only";
                });
            }, errors, "Submit"))
        } else {
            None
        }
    } else {
        Some(html! {
            article {
                p {
                    a(href = uri!(auth::login(Some(uri!(event::find_team(data.series, &*data.event)))))) : "Sign in or create a Hyrule Town Hall account";
                    : " to add yourself to this list.";
                }
            }
        })
    };
    let can_invite_any = looking_for_team.iter().any(|&(_, _, can_invite)| can_invite);
    let looking_for_team = looking_for_team.into_iter()
        .map(|(user, role, can_invite)| (user, role, can_invite.then(|| match (my_role, role) {
            // if I haven't signed up looking for team, default to the role opposite the invitee's preference
            (None, RolePreference::SheikahOnly | RolePreference::SheikahPreferred) => Some(Role::Gerudo),
            (None, RolePreference::GerudoOnly | RolePreference::GerudoPreferred) => Some(Role::Sheikah),
            (None, RolePreference::NoPreference) => None,
            // if I have signed up looking for team, take the role that's more preferred by me than by the invitee
            (Some(my_role), _) => match my_role.cmp(&role) {
                Less => Some(Role::Sheikah),
                Equal => None,
                Greater => Some(Role::Gerudo),
            },
        })))
        .collect_vec();
    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Find Teammates — {}", data.display_name), html! {
        : header;
        : form;
        table {
            thead {
                tr {
                    th : "User";
                    th : "Role";
                    @if can_invite_any {
                        th;
                    }
                }
            }
            tbody {
                @if looking_for_team.is_empty() {
                    tr {
                        td(colspan = "3") {
                            i : "(no one currently looking for teammates)";
                        }
                    }
                } else {
                    @for (user, role, invite) in looking_for_team {
                        tr {
                            td : user;
                            td : role;
                            @if can_invite_any {
                                td {
                                    @if let Some(my_role) = invite {
                                        a(class = "button", href = uri!(enter::get(data.series, &*data.event, my_role, Some(user.id)))) : "Invite";
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }).await?)
}

pub(crate) fn race7_settings() -> seed::Settings {
    collect![
        format!("user_message") => json!("7th Pictionary Spoiler Log Race"),
        format!("open_forest") => json!("closed_deku"),
        format!("open_kakariko") => json!("open"),
        format!("gerudo_fortress") => json!("fast"),
        format!("trials") => json!(0),
        format!("warp_songs") => json!(true),
        format!("spawn_positions") => json!([
            "child",
        ]),
        format!("free_bombchu_drops") => json!(false),
        format!("shuffle_song_items") => json!("any"),
        format!("shuffle_cows") => json!(true),
        format!("shuffle_ocarinas") => json!(true),
        format!("shuffle_frog_song_rupees") => json!(true),
        format!("shuffle_individual_ocarina_notes") => json!(true),
        format!("shuffle_mapcompass") => json!("startwith"),
        format!("shuffle_ganon_bosskey") => json!("remove"),
        format!("disabled_locations") => json!([
            "Deku Theater Mask of Truth",
            "KF Links House Cow",
        ]),
        format!("allowed_tricks") => json!([
            "logic_fewer_tunic_requirements",
            "logic_grottos_without_agony",
            "logic_child_deadhand",
            "logic_man_on_roof",
            "logic_dc_jump",
            "logic_rusted_switches",
            "logic_windmill_poh",
            "logic_crater_bean_poh_with_hovers",
            "logic_forest_vines",
            "logic_lens_botw",
            "logic_lens_castle",
            "logic_lens_gtg",
            "logic_lens_shadow",
            "logic_lens_shadow_platform",
            "logic_lens_bongo",
            "logic_lens_spirit",
        ]),
        format!("starting_equipment") => json!([
            "deku_shield",
        ]),
        format!("starting_inventory") => json!([
            "zeldas_letter",
        ]),
        format!("start_with_consumables") => json!(true),
        format!("no_escape_sequence") => json!(true),
        format!("no_guard_stealth") => json!(true),
        format!("no_epona_race") => json!(true),
        format!("skip_some_minigame_phases") => json!(true),
        format!("fast_bunny_hood") => json!(true),
        format!("chicken_count") => json!(4),
        format!("big_poe_count") => json!(1),
        format!("ocarina_songs") => json!([
            "frog",
            "warp",
            "frogs2",
        ]),
        format!("correct_potcrate_appearances") => json!("textures_content"),
        format!("hints") => json!("none"),
        format!("hint_dist") => json!("useless"),
        format!("misc_hints") => json!([]),
        format!("junk_ice_traps") => json!("off"),
        format!("ice_trap_appearance") => json!("junk_only"),
        format!("adult_trade_start") => json!([
            "Prescription",
            "Eyeball Frog",
            "Eyedrops",
            "Claim Check",
        ]),
    ]
}
