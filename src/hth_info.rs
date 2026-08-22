use crate::prelude::*;

#[rocket::get("/hth-info")]
pub(crate) async fn get(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> PageResult {
    page(pool.begin().await?, &me, &uri, PageStyle::default(), "HTH organizer and volunteer guide — Hyrule Town Hall", html! {
        div(class = "hth-info-page") {
            section(class = "hth-info-hero") {
                p(class = "hth-info-eyebrow") : "Help and reference";
                h1 : "HTH organizer and volunteer guide";
                p(class = "hth-info-lead") {
                    : "This guide explains where event work happens, how a race moves from bracket import to result reporting, and what organizers, coordinators, participants, and volunteers each need to do.";
                }
                p(class = "hth-info-guide-notice") : "Every event is configured differently. If a tab or option shown here is missing, it may be disabled for your event or your account may not have that role.";
            }

            section(class = "hth-info-audience") {
                h2 : "What are you here to do?";
                div(class = "hth-info-audience-grid") {
                    a(href = "#organizer-tools") {
                        span(class = "hth-info-audience-label") : "I'm organizing a tournament";
                        strong : "Set up and operate an event";
                        span : "Configuration, registration, races, automation, asyncs, and results.";
                    }
                    a(href = "#restream-coverage") {
                        span(class = "hth-info-audience-label") : "I'm a restream coordinator";
                        strong : "Prepare coverage and staff races";
                        span : "Restream assignments, race-room readiness, and volunteer roster decisions.";
                    }
                    a(href = "#volunteers") {
                        span(class = "hth-info-audience-label") : "I'm looking to volunteer";
                        strong : "Join a role and choose a race";
                        span : "Game-role approval, Discord requests, match signup, confirmation, and withdrawal.";
                    }
                }
            }

            nav(class = "hth-info-jump") {
                strong : "Contents";
                a(href = "#workspace") : "Where work happens";
                a(href = "#organizer-tools") : "Organizer tools";
                a(href = "#participants") : "Registration";
                a(href = "#live-match") : "Live match workflow";
                a(href = "#restream-coverage") : "Restream coordinators";
                a(href = "#volunteers") : "Volunteer workflow";
                a(href = "#asyncs") : "Async workflow";
                a(href = "#automation") : "Automation";
                a(href = "#access") : "Roles & access";
                a(href = "#faq") : "FAQ";
            }

            section(id = "workspace", class = "hth-info-section") {
                h2 : "Where HTH work happens";
                p : "HTH connects the services an event already uses. The website is the status and configuration layer; Discord handles day-to-day coordination; racetime.gg runs the race; bracket and broadcast services exchange match and coverage data.";
                div(class = "hth-info-grid") {
                    article(class = "hth-info-card") {
                        p(class = "hth-info-card-kicker") : "Website";
                        h3 : "The source of truth";
                        p : "Public event information, entrants, standings, schedules, race links, volunteer coverage, and results live alongside the organizer configuration pages.";
                    }
                    article(class = "hth-info-card") {
                        p(class = "hth-info-card-kicker") : "Discord";
                        h3 : "The day-to-day workspace";
                        p : "Players schedule matches in dedicated threads. Volunteers receive requests and updates. Private async threads guide runners and give staff result controls.";
                    }
                    article(class = "hth-info-card") {
                        p(class = "hth-info-card-kicker") : "racetime.gg";
                        h3 : "The race room";
                        p : "HTH can create official and practice rooms, invite entrants, distribute seeds, run settings drafts, announce race rules, and follow room results.";
                    }
                    article(class = "hth-info-card") {
                        p(class = "hth-info-card-kicker") : "Brackets and broadcasts";
                        h3 : "The external connections";
                        p : "start.gg and Challonge can provide matchups and receive results. Restream information can be shared with ZeldaSpeedRuns and SpeedGaming workflows when configured.";
                    }
                }
            }

            section(id = "organizer-tools", class = "hth-info-section") {
                h2 : "What organizers can do, and where";
                p : "Open an event from the HTH home page. Organizer-only tabs appear in its navigation once your HTH account has been added to the organizer team.";
                div(class = "hth-info-tool-list") {
                    article {
                        h3 : "Races";
                        p : "Import bracket matches or create races manually, review scheduling and room status, and open a race to edit its start time, room, restream URLs, and restream coordinator. Organizers can also cancel a race permanently or pair two nearby 1v1s into one shared restream room.";
                        p(class = "hth-info-note") : "Normal scheduling and rescheduling usually happens in the match's Discord thread. The web editor is also available for organizer corrections and exceptional cases.";
                    }
                    article {
                        h3 : "Configure";
                        p : "Control automatic start.gg imports, minimum scheduling notice, close-result and break handling, FPA announcements, async availability, force-start timing, and Discord scheduled events.";
                        p : "This area also links to restream coordinator management, recurring weekly schedules, the event's public info-page editor, start.gg round labels, and round-specific scheduling deadlines or restream-consent rules.";
                    }
                    article {
                        h3 : "Volunteer Setup";
                        p : "Choose the volunteer roles needed for each language, required minimum and maximum staffing, optional Discord roles, and whether role applications are approved automatically. Review the volunteer pool, approve applications, reuse volunteers from another event, and configure Discord request and ping workflows.";
                    }
                    article {
                        h3 : "Qualifiers";
                        p : "Create or edit live qualifier and seeding races, choose whether qualifier scores are hidden, configure an announcement role, and enable guided Discord threads for qualifier asyncs.";
                    }
                    article {
                        h3 : "Asyncs";
                        p : "Prepare qualifier, seeding, and tiebreaker async windows and their seed data. The page shows the configured seed or permalink, hash, start and end window, and lets organizers update or retire that async.";
                    }
                    article {
                        h3 : "Setup (global administrators)";
                        p : "Initial event identity and infrastructure live here: dates, visibility, format, external links, Discord server and channel IDs, participant role, randomizer version, signup requirements, and the organizer list. Ask a global administrator when one of these foundations needs to change.";
                    }
                }
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/real-casboots-configure.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/real-casboots-configure.png"), alt = "Configure page for Casual Boots Tournament 2026", loading = "lazy");
                    }
                    figcaption : "Configure controls imports, scheduling notice, result-review rules, async behavior, and Discord scheduled events. Open the image for the full-size interface.";
                }
            }

            section(id = "participants", class = "hth-info-section") {
                h2 : "Registration and teams";
                p : "Participants sign in with Discord or racetime.gg and connect any other account required by the event. Depending on the format, HTH can accept solo entries, create or confirm teams, collect event-specific answers, enforce eligibility requirements, and expose a personal status page.";
                div(class = "hth-info-columns hth-info-columns-two") {
                    div {
                        h3 : "Participants";
                        ul {
                            li : "Enter before the event deadline and answer the event's registration questions.";
                            li : "Invite or confirm teammates when the format uses teams.";
                            li : "Use My Status to review registration, qualification, and team state.";
                            li : "Use Find Teammates when the event makes that page available.";
                        }
                    }
                    div {
                        h3 : "Organizers";
                        ul {
                            li : "Review entrants, incomplete teams, and unconfirmed members.";
                            li : "Correct identities or team membership when self-service is no longer appropriate.";
                            li : "Sync participant identifiers with the configured bracket platform.";
                            li : "Handle resignations, opt-outs, eligibility, and qualification status.";
                        }
                    }
                }
            }

            section(class = "hth-info-section") {
                h2 : "A typical event lifecycle";
                ol(class = "hth-info-steps") {
                    li {
                        div(class = "hth-info-step-number") : "1";
                        div {
                            h3 : "Design and setup";
                            p : "Tell us the format, game, bracket source, ruleset, Discord structure, signup requirements, async needs, and broadcast workflow. HTH is configured around the event rather than forcing every event into the same template.";
                        }
                    }
                    li {
                        div(class = "hth-info-step-number") : "2";
                        div {
                            h3 : "Registration";
                            p : "Players sign in with Discord or racetime.gg and connect any other required accounts. HTH can collect solo or team entries, custom choices, eligibility requirements, and qualification status.";
                        }
                    }
                    li {
                        div(class = "hth-info-step-number") : "3";
                        div {
                            h3 : "Matches and scheduling";
                            p : "Bracket matches can be imported automatically. HTH creates scheduling threads where players agree on a live time or schedule their individual async parts using Discord commands.";
                        }
                    }
                    li {
                        div(class = "hth-info-step-number") : "4";
                        div {
                            h3 : "Coverage";
                            p : "Restream coordinators assign channels and coverage. Volunteer requests show the open roles for upcoming races, and staff confirm the final roster.";
                        }
                    }
                    li {
                        div(class = "hth-info-step-number") : "5";
                        div {
                            h3 : "Race day and results";
                            p : "Rooms, seeds, notifications, scheduled events, and result reporting run according to the event configuration. Organizers step in for close finishes, forfeits, FPA calls, retimes, or other cases that need judgment.";
                        }
                    }
                }
            }

            section(id = "live-match", class = "hth-info-section") {
                h2 : "A live match, end to end";
                p : "The Races tab is the organizer's status board. Player coordination happens mainly in Discord, while HTH keeps the schedule, race room, bracket, volunteers, and restream information connected.";

                div(class = "hth-info-guide-step") {
                    span(class = "hth-info-guide-step-number") : "1";
                    div {
                        h3 : "Import or create the matchup";
                        p : "HTH can import bracket matches from start.gg or Challonge, create races manually, or generate recurring races from a weekly schedule. The Races page shows start time, round, entrants, room and broadcast links, seed state, consent, and volunteer coverage.";
                    }
                }
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/real-casboots-races.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/real-casboots-races.png"), alt = "Races page for Casual Boots Tournament 2026", loading = "lazy");
                    }
                    figcaption : "The Races page is the authoritative overview for organizers. Click any screenshot in this guide to open it full size.";
                }

                div(class = "hth-info-guide-step") {
                    span(class = "hth-info-guide-step-number") : "2";
                    div {
                        h3 : "Players schedule in Discord";
                        p : "The Mayor opens a match thread for the entrants. The thread lists the commands for scheduling a live race, scheduling individual async parts, rescheduling, or removing a schedule. Natural-language times and profile timezones are supported, and multi-game matches can schedule later games separately.";
                    }
                }
                figure(class = "hth-info-screenshot hth-info-screenshot-wide") {
                    a(href = static_url!("hth-info/discord-scheduling-thread.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/discord-scheduling-thread.png"), alt = "Discord match scheduling thread created by The Mayor", loading = "lazy");
                    }
                    figcaption : "A newly created Discord scheduling thread explains the available commands and each runner's timezone.";
                }

                div(class = "hth-info-guide-step") {
                    span(class = "hth-info-guide-step-number") : "3";
                    div {
                        h3 : "Coverage is prepared";
                        p {
                            : "Restream coordinators add the broadcast and choose the production roster. See the ";
                            a(href = "#restream-coverage") : "restream coordinator workflow";
                            : " for the complete race-day process.";
                        }
                    }
                }

                div(class = "hth-info-media-split") {
                    div {
                        div(class = "hth-info-guide-step") {
                            span(class = "hth-info-guide-step-number") : "4";
                            div {
                                h3 : "HTH opens the room and distributes the seed";
                                p : "At the configured times, The Mayor can create the racetime.gg room, invite entrants, apply restream delay, promote the monitor, announce event information, roll or fetch the seed, and post the seed link and hash.";
                                p : "Exact timing depends on the event configuration. A room or seed may be created immediately when a race is scheduled at short notice.";
                            }
                        }
                        div(class = "hth-info-guide-step") {
                            span(class = "hth-info-guide-step-number") : "5";
                            div {
                                h3 : "Results are checked and reported";
                                p : "Straightforward results can be posted to Discord and reported back to the bracket automatically. Close finishes, breaks, FPA calls, forfeits, retimes, or incomplete external data wait for an organizer.";
                            }
                        }
                    }
                    figure(class = "hth-info-screenshot hth-info-screenshot-narrow") {
                        a(href = static_url!("hth-info/racetime-room-creating-seed-rolling.png"), target = "_blank", rel = "noopener noreferrer") {
                            img(src = static_url!("hth-info/racetime-room-creating-seed-rolling.png"), alt = "The Mayor creating a racetime.gg room, inviting entrants, and posting the seed", loading = "lazy");
                        }
                        figcaption : "A real room-opening and seed-distribution sequence.";
                    }
                }
            }

            section(id = "restream-coverage", class = "hth-info-section") {
                h2 : "Restream coordinator workflow";
                p : "A restream coordinator handles coverage for scheduled races: attach the broadcast destination, assign the racetime.gg restreamer, select the production volunteers, and ensure the assigned restreamer releases the room's auto-start when the broadcast is ready. This role does not grant access to event configuration, scheduling changes, seeds, or result decisions.";

                div(class = "hth-info-flow") {
                    div {
                        strong : "1. Add the restream";
                        span : "From Races, open the race editor. Enter the Restream URL and Restreamer for the correct language, then save. Use a racetime.gg profile URL, racetime.gg user ID, HTH user ID, or “me” when assigning yourself.";
                    }
                    div {
                        strong : "2. Select volunteers";
                        span : "Open Manage Volunteers from the race. Review each person's role, language, status, and note; confirm the people who are assigned, decline signups you will not use, or return a confirmed signup to Pending.";
                    }
                    div {
                        strong : "3. Prepare the room";
                        span : "HTH invites the assigned restreamer to racetime.gg and promotes them to race monitor. Verify runner feeds, delay, layouts, commentary, tracking, and the broadcast destination before releasing the start.";
                    }
                    div {
                        strong : "4. Signal broadcast readiness";
                        span : "The assigned restreamer types !ready in the racetime.gg room. This unlocks auto-start; with multiple language restreams, HTH waits until every assigned restreamer has used the command.";
                    }
                }

                div(class = "hth-info-callout") {
                    h3 : "What !ready means";
                    p {
                        code : "!ready";
                        : " is a broadcast gate, not the entrant Ready button. Only the racetime.gg account assigned as a restreamer for that race can use it. Wait until the stream is genuinely ready: the channel is live, the correct runner feeds are visible, and the production team is prepared.";
                    }
                    p : "If HTH says that only restreamers can use the command, check the Restreamer field—not just the Restream URL—and make sure it identifies the same racetime.gg account currently in the room. When no restreamer identity was assigned, an organizer must arrange race-monitor access before the start can be controlled.";
                }

                div(class = "hth-info-screenshot-grid") {
                    figure(class = "hth-info-screenshot") {
                        a(href = static_url!("hth-info/restream-setup.png"), target = "_blank", rel = "noopener noreferrer") {
                            img(src = static_url!("hth-info/restream-setup.png"), alt = "Race editor with restream URL and racetime.gg restreamer fields for each language", loading = "lazy");
                        }
                        figcaption : "Set the broadcast URL and the person operating it for the appropriate language. Shared-room races keep these settings on the primary race.";
                    }
                    figure(class = "hth-info-screenshot") {
                        a(href = static_url!("hth-info/real-casboots-signups-pending.png"), target = "_blank", rel = "noopener noreferrer") {
                            img(src = static_url!("hth-info/real-casboots-signups-pending.png"), alt = "Manage Volunteers page with a pending commentary signup and confirmation controls", loading = "lazy");
                        }
                        figcaption : "Pending means interested, not assigned. Confirm the final roster for each role and language after reviewing volunteer notes and staffing counts.";
                    }
                }

                div(class = "hth-info-columns hth-info-columns-two") {
                    div {
                        h3 : "Coordinator checklist";
                        ul {
                            li : "Confirm the URL points to the correct channel and language.";
                            li : "Assign the actual racetime.gg user who will operate the restream.";
                            li : "Choose commentary, tracking, and other configured roles in Manage Volunteers.";
                            li : "Tell replacements or declined volunteers promptly; HTH sends status updates for roster changes.";
                            li : "Use !ready only after every broadcast dependency is ready for the race to begin.";
                        }
                    }
                    div {
                        h3 : "Important limits";
                        ul {
                            li : "A restream coordinator cannot change the schedule, room URL, seed, cancellation state, or shared-room pairing.";
                            li : "Confirming a volunteer automatically declines that person's overlapping pending signups.";
                            li : "For a shared restream room, edit coverage and volunteers on the primary race.";
                            li : "If the room is already open and its restream identity is wrong, contact an organizer or race monitor before trying to start.";
                        }
                    }
                }
            }

            section(id = "volunteers", class = "hth-info-section") {
                h2 : "How volunteer signups work";
                p : "HTH has two distinct signup stages. A role request establishes that someone is eligible to do a kind of work in a language; a match signup says that an approved volunteer is available for one particular race. Approval for a role never assigns someone to a race by itself.";
                div(class = "hth-info-callout") {
                    h3 : "The usual setup: game-level roles";
                    p : "Most events use the game's shared volunteer roles. A volunteer normally applies once for a role such as Commentary (English) or Tracking (German), and that approval can be reused by every event for that game that uses the shared pool. The event Volunteer page identifies these as game roles and links to the game volunteer page where the application is managed.";
                    p : "An event may change its staffing counts, attach an event-specific Discord role, or disable a shared role without making approved volunteers apply again. Fully event-specific roles are the exception, used when a tournament needs a separate pool.";
                }

                h3 : "1. Request and receive a role";
                div(class = "hth-info-flow") {
                    div {
                        strong : "Open the Volunteer page";
                        span : "Sign in and connect Discord. If the event uses the normal game-level pool, follow its link to the game's volunteer page.";
                    }
                    div {
                        strong : "Choose role and language";
                        span : "Apply separately for each combination you can fill, such as English commentary or German tracking. Add an application note when useful.";
                    }
                    div {
                        strong : "Wait for approval";
                        span : "Auto-approved roles become available immediately. Otherwise the request stays Pending: game administrators or a restream coordinator for that language review game roles, while event organizers review event-specific roles.";
                    }
                    div {
                        strong : "Join the eligible pool";
                        span : "Approved means the volunteer may sign up for compatible races. It does not mean they are scheduled or confirmed for any of them.";
                    }
                }
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/real-casboots-volunteer.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/real-casboots-volunteer.png"), alt = "Signed-in Volunteer page showing game-level commentary and tracking roles by language", loading = "lazy");
                    }
                    figcaption : "The Volunteer page explicitly shows that these roles are managed at game level. Approval is tied to the role and language, not to one race.";
                }

                h3 : "2. Sign up for a specific race";
                p : "Once approved, a volunteer can express interest in individual live races. This second signup is deliberately temporary and still needs a roster decision.";
                div(class = "hth-info-flow") {
                    div {
                        strong : "Choose an open race";
                        span : "Use the event Volunteer page or a signup button in HTH's Discord request. Only compatible approved roles are offered.";
                    }
                    div {
                        strong : "Submit availability";
                        span : "Choose the role and add an optional note about language, co-commentators, timing, or other useful context.";
                    }
                    div {
                        strong : "Pending is not assigned";
                        span : "An organizer or restream coordinator reviews the interested volunteers, staffing target, notes, language, and possible overlaps.";
                    }
                    div {
                        strong : "Confirmed is assigned";
                        span : "HTH notifies the selected volunteer and keeps the web and Discord rosters current. The volunteer can withdraw before the race if plans change.";
                    }
                }
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/discord-volunteer-signup.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/discord-volunteer-signup.png"), alt = "Discord volunteer request listing open races, missing roles, and signup buttons", loading = "lazy");
                    }
                    figcaption : "Discord requests advertise individual races after role approval. Each button starts a match signup; it does not create or approve a volunteer role.";
                }
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/real-casboots-signups-pending.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/real-casboots-signups-pending.png"), alt = "Match volunteer signup page with a pending commentary signup and organizer controls", loading = "lazy");
                    }
                    figcaption : "A pending match signup means interested, not assigned. Organizers and authorized coordinators confirm or decline the final roster.";
                }
                h3 : "Finding volunteers automatically";
                p : "Organizers choose how far before a race HTH posts a volunteer request. The announcement groups upcoming races, shows confirmed and pending coverage, and offers signup buttons. Separate ping workflows can send a daily or weekly digest, or ping per race at chosen lead times. Event-level workflows override game defaults, and ping messages can be removed after the race starts.";
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/real-casboots-volunteer-setup.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/real-casboots-volunteer-setup.png"), alt = "Volunteer Setup page with request lead time and Discord ping workflows", loading = "lazy");
                    }
                    figcaption : "Volunteer Setup controls the initial request lead time and optional scheduled or per-race Discord ping workflows.";
                }
            }

            section(id = "asyncs", class = "hth-info-section") {
                h2 : "Guided async racing";
                p : "HTH supports both bracket matches whose runners complete separate parts and qualifier, seeding, or tiebreaker asyncs requested through the website. When Discord automation is enabled, each run gets a private thread shared with the runner and event staff.";
                div(class = "hth-info-async-grid") {
                    article {
                        span(class = "hth-info-sequence") : "Ready";
                        h3 : "Seed delivery";
                        p : "The runner clicks READY when prepared. HTH records the state and posts the correct seed, permalink, password, or hash information in the private thread.";
                    }
                    article {
                        span(class = "hth-info-sequence") : "Start";
                        h3 : "Controlled countdown";
                        p : "START COUNTDOWN runs a five-second countdown and records the start. Organizers can configure a delay that force-starts a distributed seed if the button is not pressed in time.";
                    }
                    article {
                        span(class = "hth-info-sequence") : "Finish";
                        h3 : "Runner submission";
                        p : "FINISH captures an estimated time and offers a short revert window. The runner then posts a VOD and the game-specific completion evidence requested in the thread.";
                    }
                    article {
                        span(class = "hth-info-sequence") : "Verify";
                        h3 : "Organizer confirmation";
                        p : "Staff use Confirm Result to record the official time and VOD, or confirm a requested forfeit. Bracket async results remain private until the relevant parts are complete and confirmed.";
                    }
                }
                p(class = "hth-info-note") : "Match volunteer signups are for live races; async runs use their private staff thread instead.";
                figure(class = "hth-info-screenshot") {
                    a(href = static_url!("hth-info/real-casboots-asyncs.png"), target = "_blank", rel = "noopener noreferrer") {
                        img(src = static_url!("hth-info/real-casboots-asyncs.png"), alt = "Asyncs organizer page with seed, start and end window, and edit form", loading = "lazy");
                    }
                    figcaption : "Organizers prepare the seed and availability window on the Asyncs page before runners use their private Discord workflow.";
                }
            }

            section(id = "automation", class = "hth-info-section") {
                h2 : "What HTH can automate";
                div(class = "hth-info-columns") {
                    div {
                        h3 : "Before a race";
                        ul {
                            li : "Import new matchups from start.gg or Challonge.";
                            li : "Create scheduling threads and enforce scheduling notice or deadlines.";
                            li : "Create Discord scheduled events, optionally only for restreamed races.";
                            li : "Open recurring weekly races from a schedule.";
                            li : "Request volunteers and ping the relevant Discord roles.";
                            li : "Publish room, multistream, and restream links on the public schedule.";
                        }
                    }
                    div {
                        h3 : "In the race room";
                        ul {
                            li : "Open and configure official racetime.gg rooms.";
                            li : "Invite the correct entrants and announce stream-delay or FPA rules.";
                            li : "Roll or distribute seeds and support event-specific settings drafts.";
                            li : "Track room state, FPA calls, breaks, and finish order.";
                        }
                    }
                    div {
                        h3 : "After a race";
                        ul {
                            li : "Post results to configured Discord channels.";
                            li : "Report eligible results to the bracket platform.";
                            li : "Export race and volunteer data to supported broadcast workflows.";
                            li : "Preserve past events, races, standings, seeds, and VOD links.";
                        }
                    }
                }
                p(class = "hth-info-note") : "Automation is intentionally conservative. Close finishes, races using breaks, FPA situations, retimes, forfeits, and incomplete external data may wait for an organizer instead of being reported automatically.";
            }

            section(id = "access", class = "hth-info-section") {
                h2 : "Roles and access";
                p : "These roles are additive: one person may be an organizer, a restream coordinator, and an approved volunteer at the same time. Permissions come from each role separately; organizer access does not automatically approve someone for a volunteer role.";
                div(class = "hth-info-role-grid") {
                    article {
                        p(class = "hth-info-card-kicker") : "Event-wide operations";
                        h3 : "Organizer";
                        p : "Responsible for the event as a whole and for decisions that affect participants, schedules, automation, and results.";
                        h4 : "Can";
                        ul {
                            li : "Configure event-level operating rules, rounds, deadlines, and public information.";
                            li : "Import, create, edit, cancel, and pair races; correct rooms and schedules.";
                            li : "Configure volunteer roles and requests, review applications, and manage every race roster.";
                            li : "Manage qualifiers, guided asyncs, restream coordinators, and exceptional results.";
                        }
                        h4 : "Cannot by default";
                        ul {
                            li : "Change foundational Setup fields reserved for global administrators.";
                            li : "Manage reusable game-wide roles or integrations without separate game-admin access.";
                        }
                    }
                    article {
                        p(class = "hth-info-card-kicker") : "Broadcast coverage";
                        h3 : "Restream coordinator";
                        p : "Coordinates restream coverage for an event or game, normally for one or more assigned languages.";
                        h4 : "Can";
                        ul {
                            li : "Open race editing pages and maintain restream URLs and assigned restreamers.";
                            li : "Review game-level volunteer role requests for their assigned languages.";
                            li : "Review match volunteer signups and confirm, decline, or reopen roster assignments.";
                            li : "Use !ready in racetime.gg when they are the assigned restreamer and the broadcast is prepared.";
                        }
                        h4 : "Cannot";
                        ul {
                            li : "Change event configuration, volunteer role definitions, request workflows, qualifiers, or async seeds.";
                            li : "Change race rooms, cancel races, or create shared race rooms unless they also have organizer access.";
                        }
                    }
                    article {
                        p(class = "hth-info-card-kicker") : "A specific production role";
                        h3 : "Volunteer";
                        p : "An approved commentator, tracker, or other helper. Approval is role- and language-specific and is separate from assignment to a race.";
                        h4 : "Can";
                        ul {
                            li : "Apply for available event-level or game-level volunteer roles.";
                            li : "Sign up for compatible live races, add notes, and see pending or confirmed status.";
                            li : "Withdraw a signup before the race when plans change, or forfeit the underlying role.";
                        }
                        h4 : "Cannot";
                        ul {
                            li : "Confirm their own assignment or manage another volunteer's signup.";
                            li : "Edit event, race, restream, qualifier, async, or automation settings.";
                        }
                    }
                }

                h3 : "Capability overview";
                div(class = "hth-info-permission-table-wrapper") {
                    table(class = "hth-info-permission-table") {
                        thead {
                            tr {
                                th : "Capability";
                                th : "Organizer";
                                th : "Restream coordinator";
                                th : "Volunteer";
                            }
                        }
                        tbody {
                            tr {
                                th : "View public event and race information";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-yes") : "Yes";
                            }
                            tr {
                                th : "Configure event rules, rounds, and deadlines";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-no") : "No";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Change race room, cancellation, or shared-room state";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-no") : "No";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Maintain restream URLs and assignments";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-limited") : "Coverage scope";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Define volunteer roles and request workflows";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-no") : "No";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Review volunteer role requests";
                                td(class = "hth-info-permission-limited") : "Event roles";
                                td(class = "hth-info-permission-limited") : "Game roles, assigned language";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Confirm or decline race volunteer signups";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-limited") : "Coverage scope";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Apply and sign up as a volunteer";
                                td(class = "hth-info-permission-limited") : "If approved";
                                td(class = "hth-info-permission-limited") : "If approved";
                                td(class = "hth-info-permission-yes") : "Yes";
                            }
                            tr {
                                th : "Manage qualifiers and async seeds";
                                td(class = "hth-info-permission-yes") : "Yes";
                                td(class = "hth-info-permission-no") : "No";
                                td(class = "hth-info-permission-no") : "No";
                            }
                            tr {
                                th : "Change foundational event or game setup";
                                td(class = "hth-info-permission-limited") : "Admin role required";
                                td(class = "hth-info-permission-no") : "No";
                                td(class = "hth-info-permission-no") : "No";
                            }
                        }
                    }
                }

                h3 : "Other account types";
                div(class = "hth-info-access-list") {
                    div {
                        strong : "Everyone";
                        span : "Can view public events, schedules, standings, race rooms, restreams, results, and confirmed volunteer coverage.";
                    }
                    div {
                        strong : "Participants";
                        span : "Enter events, manage their team or status, schedule matches in Discord, request asyncs, and access event-specific practice tools.";
                    }
                    div {
                        strong : "Game and global administrators";
                        span : "Maintain reusable game roles and coordinators, platform connections, and the event's foundational setup.";
                    }
                }
                p {
                    : "One HTH profile can connect ";
                    strong : "Discord";
                    : ", ";
                    strong : "racetime.gg";
                    : ", and ";
                    strong : "start.gg";
                    : ". Set a profile timezone so schedules and organizer date fields use your local time. Connect Discord before volunteering or using Discord-based organizer and async workflows.";
                }
            }

            section(id = "getting-started", class = "hth-info-section") {
                h2 : "Bring an event to HTH";
                p : "Because formats and community workflows vary, new events start with a short conversation. It helps to bring the following information:";
                ul(class = "hth-info-checklist") {
                    li : "The game, ruleset, format, dates, and expected number of entrants.";
                    li : "Whether matchups come from start.gg, Challonge, or are managed manually.";
                    li : "The Discord server and the channels intended for scheduling, rooms, results, volunteers, organizers, and asyncs.";
                    li : "The volunteer roles, languages, staffing targets, and restream destinations.";
                    li : "Signup requirements, qualifier or async plans, scheduling deadlines, and any special seed or draft workflow.";
                }
                div(class = "hth-info-actions") {
                    a(class = "button", href = uri!(crate::http::new_event)) : "See how to contact us";
                    a(class = "button", href = uri!(crate::http::index)) : "Browse current events";
                }
            }

            section(id = "faq", class = "hth-info-section hth-info-final hth-info-faq-section") {
                h2 : "FAQ: something went wrong—what now?";
                p : "These are the usual recovery paths. Commands must be used in the match's scheduling thread, and some are limited to runners or event organizers. If an event has stricter rules, follow those rules first.";
                div(class = "hth-info-faq") {
                    details {
                        summary : "We scheduled the wrong time. How do we change it?";
                        p {
                            : "Run ";
                            code : "/schedule";
                            : " again in the match thread with the corrected time. That reschedules a live race. For an async part, the affected runner uses ";
                            code : "/schedule-async";
                            : ". HTH updates the race and notifies pending and confirmed volunteers about the new time.";
                        }
                        p : "If the race should become unscheduled without otherwise resetting it, use /schedule-remove instead.";
                    }
                    details {
                        summary : "The race is postponed. Should I mark it as canceled?";
                        p {
                            : "No. Use ";
                            code : "/schedule-remove";
                            : " to remove the current scheduling, or reschedule directly with ";
                            code : "/schedule";
                            : ". The organizer-only “Cancel race permanently” checkbox is for a matchup that will not take place at all.";
                        }
                    }
                    details {
                        summary : "A room or seed was already created, but we need a clean reschedule.";
                        p {
                            : "An event organizer can run ";
                            code : "/reset-race schedule:true";
                            : " in the scheduling thread. This clears the schedule, room, seed, async state, and Discord scheduled event so the race can be scheduled again. Pending and confirmed volunteers are told that the race must be rescheduled.";
                        }
                        p {
                            : "For a multi-game match, include the ";
                            code : "game:";
                            : " option when Discord cannot identify one race. If a result had already been recorded, reset it manually on start.gg or Challonge as well.";
                        }
                        p {
                            : "If only one unplayed async part needs to be rescheduled, an organizer can use ";
                            code : "/reset-async participant: game:";
                            : ". This preserves the other participants' schedules and results. It cannot be used after the selected participant's seed has been released or their run has started.";
                        }
                    }
                    details {
                        summary : "The settings draft needs to start over.";
                        p {
                            : "An organizer uses ";
                            code : "/reset-race draft:true";
                            : ". Add the game number for a multi-game match when needed. This resets the draft without deleting the schedule unless ";
                            code : "schedule:true";
                            : " is also selected.";
                        }
                    }
                    details {
                        summary : "The racetime.gg room was canceled for inactivity.";
                        p {
                            : "A runner in that match or an event organizer can use ";
                            code : "/restart-room";
                            : " in the scheduling thread. It only works for a live-scheduled race whose start time has arrived and whose room was cleared after inactivity; it does not open an upcoming room early.";
                        }
                    }
                    details {
                        summary : "A volunteer can no longer make the race.";
                        p : "Before the race starts, the volunteer can withdraw on the match signup page or use the withdrawal button in HTH's Discord message. The request post and roster update automatically. After the race has started, contact the restream coordinator or an organizer directly—the self-service withdrawal is closed.";
                    }
                    details {
                        summary : "The wrong volunteer was confirmed, or coverage changed.";
                        p : "Open the race's Manage Volunteers or Match Volunteer Signups page. An organizer or restream coordinator can decline a pending signup, revert a confirmed signup to Pending, or confirm a replacement. Recheck the staffing count and the correct language before finishing.";
                    }
                    details {
                        summary : "The Discord volunteer request is missing or looks stale.";
                        p : "On Volunteer Setup, confirm that automatic request posts are enabled and that the race falls inside the configured lead-time window. Check the role bindings and scheduled start, then use Check Now to run the request check immediately. Existing posts normally refresh when schedules, role bindings, or signups change.";
                    }
                    details {
                        summary : "I clicked FINISH in an async by mistake, or I need to forfeit.";
                        p : "Use the short Finish Revert opportunity in the private async thread and continue the run. For a genuine forfeit, use the Forfeit button and confirm it; an organizer must then confirm the forfeit before it is recorded. If the controls are gone or the submitted data is wrong, ask an organizer in that private thread rather than starting another run.";
                    }
                    details {
                        summary : "The finish was close, FPA was invoked, or breaks were used.";
                        p : "Do not rush a manual bracket result. HTH may intentionally withhold automatic reporting for a close finish, FPA situation, configured break condition, or retime window. Preserve the VODs and room information, then let an organizer or race monitor review and record the official result. Runners should use !fpa only according to the event's race rules.";
                    }
                    details {
                        summary : "The race finished, but the bracket did not update.";
                        p : "First check whether HTH is deliberately waiting for review because of a close finish, breaks, FPA, a forfeit, a retime, or incomplete external identifiers. Confirm the result and participant mapping, then have an organizer correct the bracket manually if automatic reporting cannot proceed.";
                    }
                    details {
                        summary : "A tab, button, or command shown in this guide is missing.";
                        p : "Check that you are signed into the intended HTH account, that Discord is connected when required, and that your event role was assigned. Features such as asyncs, volunteer roles, scheduled events, and bracket integrations can be disabled per event. Editing tools are also hidden after an event ends. Ask an organizer or HTH administrator if the access still looks wrong.";
                    }
                }
            }
        }
    }).await
}
