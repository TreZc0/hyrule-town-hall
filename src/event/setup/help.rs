use crate::prelude::*;

pub(super) fn label(field: &str, title: &str) -> RawHtml<String> {
    let paragraphs: &[&str] = match field {
        "series" => &[
            "Groups related events and determines their URL prefix. For example, series alttprmain with event slug 2026 produces /event/alttprmain/2026. Choose the community or tournament family, rather than the seed generator.",
            "A series must be connected to the correct game in game administration for category lookup and race-room creation. The two OWR builds are selected independently under Seed Gen Type. Existing events in the same series do not automatically share settings.",
        ],
        "event" => &[
            "The event slug is the stable identifier within a series and forms part of event links. Use a short, readable value such as 2026 or spring26. The combination of series and event slug must be unique.",
            "The display name can contain the full public tournament title. Treat the slug as permanent once you share links or create races; changing the public title does not require changing the slug.",
        ],
        "start" => &[
            "The start of the event as a whole, not the start time of every race. Event pages and workflows use it to decide whether the event has started; individual races have their own schedules.",
            "This form currently reads and stores this value as UTC, despite the browser showing a date/time picker without a timezone. Convert your intended local time to UTC before entering it. Qualification request, retry and submission windows are configured separately.",
        ],
        "end" => &[
            "The end of the event as a whole. This marks its lifecycle boundary for event listings and background workflows; it is separate from the scheduled finish of a race.",
            "Enter UTC in this form. A pooled qualifier's submission deadline and result-publication time belong on the Qualifiers page, and should not be represented only by the event end time.",
        ],
        "url" => &[
            "The external tournament page, such as your start.gg bracket or Challonge tournament. Use the full https:// URL for the specific event or bracket you intend to reference.",
            "This is separate from the signup link and participants-list link. Importing or connecting races is performed through the race/import workflows; entering a URL alone does not create a complete schedule.",
        ],
        "discord_invite_url" => &[
            "The public Discord invitation shown to participants so they can join the event community. Paste an invite URL, rather than a server ID or channel link.",
            "The bot's server and channel destinations are configured by the numeric Discord fields below. An invitation does not add the bot, grant it permissions, or select where it posts.",
        ],
        "discord_guild" => &[
            "The numeric ID of the Discord server hosting this event. Discord calls a server a guild. Copy its ID with Discord Developer Mode enabled; do not paste the server name or invite URL.",
            "The event's channel and role IDs should belong to this server, and the configured bot must already be a member. Channels select specific workflows; the server ID alone is not a posting destination.",
        ],
        "discord_race_room_channel" => &[
            "The channel used for event race-room announcements. Enter its numeric Discord channel ID, copied using Developer Mode.",
            "This is distinct from scheduling threads and recorded race results. The bot needs access and permission to send messages in the chosen channel. Leave optional destinations empty when that announcement workflow is not needed.",
        ],
        "discord_race_results_channel" => &[
            "The destination for event race-result announcements. Enter a numeric Discord channel ID, not a channel name or URL.",
            "Keep this separate from the scheduling channel if you want completed results to be easy to find. Qualifier visibility and pooled result-release rules still govern disclosure; this setting only selects the destination.",
        ],
        "discord_volunteer_info_channel" => &[
            "The Discord channel associated with volunteer information for this event. Use the channel's numeric ID and ensure the bot can access it.",
            "Volunteer roles, signup requirements and notification workflows are configured separately. Choosing this channel does not itself assign restreamers, commentators, trackers or coordinators to a race.",
        ],
        "discord_organizer_channel" => &[
            "A Discord destination for organizer-facing event messages. Use a staff channel that the bot can access, and enter its numeric channel ID.",
            "Organizers may receive workflow issues and actions requiring follow-up. This field does not grant organizer access to the website; manage website organizers in the organizer section below.",
        ],
        "discord_scheduling_channel" => &[
            "The Discord channel used for race scheduling threads. Enter the channel's numeric ID and ensure the bot can create and manage the required threads there.",
            "A race still needs to be imported or created with its participants. Scheduling threads coordinate an individual match; the event start date is not a substitute for each race's scheduled time.",
        ],
        "discord_async_channel" => &[
            "The Discord channel used for automated async race threads. Enter a channel ID in the event's server and grant the bot the thread permissions required by the workflow.",
            "Private qualifier delivery also depends on the qualifier configuration and thread-membership rules. Select a suitable channel before enabling requests; a public scheduling channel is not automatically a private async delivery setup.",
        ],
        "discord_participant_role" => &[
            "The participant role assigned through this event's role configuration. Copy the numeric role ID from the event's Discord server.",
            "The bot needs Manage Roles and its own role must be above the participant role. This is distinct from volunteer roles and website organizer permissions; assigning it does not make somebody a restreamer or organizer.",
        ],
        "listed" => &[
            "Controls whether the event appears in the site's public event listings. Leave it off while preparing a test event or an unfinished configuration.",
            "Unlisting is a discovery setting, not an access-control mechanism. People with a direct link may still be able to open the event. Use the actual participant, organizer and result-visibility controls for restricted information.",
        ],
        "emulator_settings_reminder" => &[
            "Enables the bot's reminder asking entrants to show their emulator settings in supported official race-room workflows. It is useful when your rules require stream verification of emulator configuration.",
            "The reminder follows the race's stream-delay handling. It does not inspect an emulator or validate settings automatically; staff still review compliance with the event's rules.",
        ],
        "prevent_late_joins" => &[
            "In the supported official room workflow, the bot switches an open room to invitational after the stream-delay waiting period. This prevents additional entrants from freely joining at that point.",
            "This is a race-room entry control, not a website signup deadline. Pooled live qualifiers have their own explicit entry cutoff and eligibility rules on the Qualifiers page.",
        ],
        "fpa_enabled" => &[
            "Makes the Fair Play Agreement workflow available for eligible official races. Entrants can use !fpa to notify race monitors of a technical problem such as a crash.",
            "The bot distinguishes invitational matches from large open races, so enabling this flag does not force FPA on every room. Set out the event's resolution rules separately; FPA notification does not itself decide a result or grant a rerun.",
        ],
        "auto_start_with_restream" => &[
            "Allows a restreamed race to keep automatic start behavior rather than always waiting for the restream readiness gate. Use it when player readiness should normally be enough to begin.",
            "When disabled, the normal restream gate remains active. With this option enabled, the race controls can still activate the gate before countdown when production needs extra preparation. Coordinate this policy with your restream team.",
        ],
        "rando_version_json" => &[
            "Version metadata for generators that use it, including TWWR build and tracker information. Use the tagged JSON structure shown in the examples below this field; a plain branch name is not a complete version object.",
            "This field does not select the OWR installation. Choose ALTTPR OWR (regular build) or ALTTPR OWR (tournament build) under Seed Gen Type. The tournament option runs the installation at /opt/owr_tourney; installing the intended build is a deployment step.",
        ],
        "enter_url" => &[
            "An external registration destination for this event. Use the full URL of the signup form or tournament registration page when entrants should register outside this site.",
            "For on-site registration, configure Enter Flow instead. External registration does not automatically provide the site with entrants' accounts, choice answers or qualifier results; those still require the corresponding integration or organizer workflow.",
        ],
        "teams_url" => &[
            "An external participants or teams page associated with the event. Use this when the canonical entrant list is maintained by another tournament platform.",
            "The site's own teams and races remain separate database records. A link does not import teams, grant signup approval or configure qualification. Hide Teams Tab controls whether the on-site tab is shown.",
        ],
        "challonge_community" => &[
            "The Challonge community identifier used for community-hosted tournaments. Use the community value expected by the Challonge integration, rather than the full tournament URL.",
            "The specific tournament URL belongs in Event URL. Check both when importing a bracket so the import refers to the intended tournament and community.",
        ],
        "language" => &[
            "Selects the event language used by supported participant-facing text and bot messages. Choose the language you want entrants to encounter in the event's workflows.",
            "This does not translate text you enter yourself, such as the event description, draft labels, rules or signup prompts. Write those in the intended language as well. Some messages have fewer translations and may use a fallback.",
        ],
        "team_config" => &[
            "Defines how an entrant is structured: Solo is one racer; the team variants use their predefined membership and role arrangements. Choose the structure before opening signup or importing a bracket.",
            "This affects team membership, role expectations and how races identify participants. It is separate from the generator's settings and from how many games a match contains. Changing it after participants have registered may require reconciling their existing teams.",
        ],
        "default_game_count" => &[
            "The default number of games offered by race creation and import workflows for a match. For example, use 3 as the usual count for a best-of-three setup when that workflow asks how many game records to create.",
            "Individual race/import forms may override the default. Changing it does not retroactively rebuild existing matches. For start.gg double round-robin best-of-one imports, the dedicated double round-robin setting has its own two-game behavior.",
        ],
        "open_stream_delay" => &[
            "The event's required stream delay for open races, including races represented by an entrant count rather than a fixed invitational matchup. Enter a duration such as 5m, not a timestamp.",
            "The bot can communicate and use this delay in its race preparation workflow. It does not configure OBS or your streaming service. The racetime countdown and seed-preroll policy are separate settings.",
        ],
        "invitational_stream_delay" => &[
            "The required stream delay for races with known invited entrants. Enter a duration such as 5m; choose it according to your event's spoiler and restream policy.",
            "This is independent of Open Stream Delay and the racetime countdown. Entrants still need to configure their streaming software themselves; the site cannot apply their broadcast delay.",
        ],
        "hide_teams_tab" => &[
            "Removes the event's teams/entrants tab from its navigation. Useful when registration and the canonical participant list live on an external page.",
            "It does not delete registered teams or make their information private. The separate Hide Entrants option controls entrant visibility in generated racetime rooms, and qualifier visibility has its own rules.",
        ],
        "hide_races_tab" => &[
            "Removes the event's races tab from its navigation. Existing race records and scheduled workflows are retained.",
            "Use this for event presentation when another page is the primary schedule. It is not a way to stop race creation, cancel races, or restrict access to private results.",
        ],
        "show_qualifier_times" => &[
            "Controls whether qualifier times are shown by the relevant event qualifier presentation. Use it when the format permits entrants to see those times.",
            "Other qualifier privacy rules still apply. In pooled-by-mode events, the configured publication time governs public result/source disclosure. This checkbox does not publish private seeds or replace the qualification method.",
        ],
        "swiss_standings" => &[
            "Shows the Swiss standings tab for the event's supported standings workflow. Use it for a Swiss stage that has the necessary race and entrant data.",
            "It is a presentation switch, not a pairing generator. It does not turn a bracket into Swiss, create rounds or replace the standings configuration supplied by the tournament format.",
        ],
        "automated_asyncs" => &[
            "Uses automated Discord threads for the supported qualifier-async workflow, so entrants receive the seed and start/report their run through the bot.",
            "Configure the event's Discord server and async channel as well as the async seed and timing settings. The legacy force-start delay is a separate field. Pooled-by-mode qualifiers use their dedicated configuration and request lifecycle on the Qualifiers page.",
        ],
        "async_start_delay" => &[
            "For the automated async workflow, this is the number of minutes after seed distribution before the bot forces the start if the entrant has not started manually. Leave it empty to disable that automatic force-start.",
            "This is measured in minutes, unlike Start Delay for racetime rooms, which is measured in seconds. It is also separate from a qualifier's request or submission deadline. Pooled qualifiers have their own READY/countdown/GO lifecycle.",
        ],
        "show_opt_out" => &[
            "Shows the opt-out control where the event supports it: before the event starts, for score-based qualification. It allows a qualified player to indicate they do not intend to participate.",
            "It does not automatically remove race results or change the scoring formula. Other qualification methods do not gain an opt-out workflow merely by enabling this checkbox.",
        ],
        "force_custom_role_binding" => &[
            "Uses event-specific volunteer-role bindings instead of the game's shared volunteer-role configuration. Enable it when this event needs a different set of roles or signup requirements.",
            "Configure the event's bindings before asking volunteers to sign up. This is separate from the participant Discord role and from website organizer permissions. Using game-level bindings makes sense when your events share the same volunteer setup.",
        ],
        "racetime_goal_slug" => &[
            "The exact goal text used when the bot creates racetime rooms. Despite the label, enter the displayed goal name, such as Beat the game - Tournament (Solo), rather than an internal generator slug.",
            "Pair it with Is Custom Goal: standard goals must match the selected game's category, while a custom goal uses your event-specific text. The series-to-game mapping supplies the category. A goal name does not choose a seed generator; configure Seed Gen Type separately.",
        ],
        "is_custom_goal" => &[
            "Tells racetime whether Goal Slug is custom event text or a standard goal in the game's category. Enable it for a custom tournament goal; disable it when using an existing standard goal.",
            "It affects room creation, not the seed settings. For a standard goal, copy the exact category goal name. For a custom goal, use a clear public event-specific name and configure the event's generator independently.",
        ],
        "draft_kind" => &[
            "Selects the workflow used for pre-race bans or picks. None means there is no configured draft. Generic Ban/Pick, Ban Only and Pick Only use the options and turn order supplied by Draft Config JSON.",
            "Preset drafts support Avianart, Boothisman and named OWR/Door Rando mutual-choice baselines. Draft preset keys must match the baselines map; configure default_baseline when a named event has no draft. Existing season-specific choices are predefined workflows. Pooled qualifier modes use baseline settings and do not run entrant drafts.",
        ],
        "draft_config" => &[
            "Parameters for the selected generic draft: the available options, display names and preset identifiers, plus the turn order or starting side required by that workflow. A ban_pick draft with three options and two pick steps (high_seed, then low_seed) assigns the remaining option to game 3. Use the same draft for two-game RR and BO3: an RR match simply has no third game. Use valid JSON, with double-quoted property names and no comments.",
            "For example, a pick_only draft can use options [{\"display_name\":\"Open\",\"preset\":\"open\"}], who_starts \"high_seed\", picks_per_player 1 and unique true. Add enough distinct options for every pick when unique is true. Preset identifiers must be understood by your seed generator; display names are only labels.",
        ],
        "qualifier_mode" => &[
            "Selects what determines qualification. None ignores qualification; Stored qualifier ranks uses organizer-assigned ranks entered on the event’s Qualifiers page (1 is highest; leave blank for unranked); Single async qualifier uses that async workflow; Configured scoring combines eligible live and async results using the selected score kind.",
            "Pooled by mode uses the dedicated per-mode seed pools, live links, attempts and scoring configuration on the Qualifiers page. After creating the event, configure those modes and windows and pass readiness checks before allowing requests.",
            "The presence of old async submissions or qualifier ranks does not automatically select a qualification method. Choose the method explicitly. Event-level scoring parameters do not replace the pooled-mode configuration.",
        ],
        "qualifier_score_kind" => &[
            "Selects the scoring strategy for Configured scoring. Time relative to par exposes reusable parameters for counts, the points formula and result aggregation. The TWWR strategies start from their existing event-specific formula defaults.",
            "Standard scores finishes on a 100–1100 scale and sums up to four results after dropping the best. SGL scores finishes on a 10–110 scale and averages results using season-specific discard rules. Expand the strategy explanations below for formulas, attempt limits and minimum requirements. These fixed strategies accept empty JSON or {}, rather than time-relative parameters.",
            "This field matters when Qualification method is Configured scoring. Pooled-by-mode scoring is configured separately on the Qualifiers page.",
        ],
        "qualifier_score_config" => &[
            "For Standard and SGL, leave this field empty or use {}. Their scoring formulas and result-selection rules are fixed and explained below; the configurable fields in the following paragraphs do not apply to them.",
            "Overrides the selected time-relative strategy's defaults. Use a JSON object; omitted properties inherit their defaults. Empty input keeps the defaults. On event creation, choosing another score kind replaces this field with that kind's defaults, so copy any custom values you want to keep.",
            "par_finishers defines the cohort used to establish par. required_finishes is the required finish count; counted_attempts limits the chronological attempts considered; best_results selects the best scores among those attempts. Counts must be positive, and required_finishes and best_results cannot exceed counted_attempts.",
            "The formula starts with (1 - (finish time - par) / par) × scale. rounding applies none, floor or nearest before offset is added, then minimum and optional maximum bound the score. A forfeit/DNF scores zero; a finish awaiting sufficient par data remains pending.",
            "aggregation is sum or average. Average divides by best_results, counting missing results as zero. extrapolation_score controls the score used for extrapolation. Keep this at the strategy default unless your qualification rules call for a change. These parameters do not configure pooled-by-mode qualifiers.",
        ],
        "is_single_race" => &[
            "Marks the event as one overall race for event presentation and workflows that distinguish single-race events from multi-match tournaments. Use it for an event built around one race rather than a bracket or series of matches.",
            "This is not the number of games in a match, and it does not itself create a race or define its entrants. Default Game Count is the separate match-length setting.",
        ],
        "hide_entrants" => &[
            "Requests hidden entrants when the bot creates racetime rooms for this event. This is useful for formats where the room should not expose the entrant list normally.",
            "It does not hide the website's teams tab or alter eligibility. Use Hide Teams Tab for navigation and the qualifier publication controls for result privacy. Existing rooms may need their room settings updated separately.",
        ],
        "start_delay" => &[
            "The racetime countdown duration, in seconds, configured when the bot creates a room. For example, 15 gives entrants a 15-second countdown after the room is ready to start.",
            "Open races may use Start Delay Open instead. This is distinct from stream delay, seed generation timing, and the automated async force-start delay. It does not change the race's scheduled start timestamp.",
        ],
        "start_delay_open" => &[
            "An optional racetime countdown duration, in seconds, specifically for open races. Leave it empty to inherit Start Delay; enter a value when open qualifier rooms need a different countdown.",
            "This does not close entry or reserve a place for a player. Pooled live entry cutoffs are configured separately. The value takes effect when the bot creates the room.",
        ],
        "restrict_chat_in_qualifiers" => &[
            "Disables pre-race and mid-race chat in racetime rooms explicitly marked as qualifiers. Other races retain their usual room chat settings.",
            "The race's qualifier flag controls this behavior; naming a phase Qualifier is not the configuration mechanism. It does not affect Discord thread chat or the public release time of qualifier results.",
        ],
        "preroll_mode" => &[
            "Choose when generation may begin ahead of seed delivery. It does not decide when a seed becomes visible to entrants. The available behavior depends on the selected generator; the details below distinguish ordinary generator timing from legacy web/reserve workflows.",
        ],
        "spoiler_unlock" => &[
            "The spoiler-release policy requested by supported generator workflows. Never keeps spoilers locked; After race requests release after completion; Immediately allows release as part of seed delivery when the generator supports it.",
            "A generator may not support every policy. In particular, the OWR/locally generated ALTTPR patch flow retains its private spoiler handling, and pooled qualifier generation uses Never. Selecting a policy does not make an unavailable spoiler file appear or bypass pooled privacy rules.",
        ],
        "startgg_double_rr" => &[
            "Enables the special handling for start.gg double round-robin stages represented as best-of-one sets. The import creates two game records for each matchup and uses the workflow that force-closes the start.gg set after both results are played.",
            "start.gg has no native double-RR format. Enable this when one RR/BO1 set represents two meetings managed by this site. Only matching RR sets receive the special handling; elimination BO3 sets still finish at two wins, even with this event option enabled. Use it only when the external stage is intended to represent those two meetings. It is not a general best-of-two match setting and does not change a single-elimination or ordinary best-of-one stage into double round-robin.",
        ],
        "is_live_event" => &[
            "Means an in-person event. For scheduled races after the event starts, the workflow sends notifications instead of creating racetime rooms as it would for an online event.",
            "Leave it disabled for an online tournament, including online races played live rather than asynchronously. The word live here describes an in-person venue, not the live-versus-async qualifier distinction.",
        ],
        "seed_gen_type" => &[
            "Chooses how the site generates event seeds. None means manual/external delivery. ALTTPR Door Rando selects one of the sources in Seed Config JSON; Avianart uses a preset; TWWR uses a settings permalink.",
            "OWR (regular build), stored as owr, uses /opt/owr. OWR (tournament build), stored as owr_tourney, uses /opt/owr_tourney. Both use the same baseline/choice JSON structure and support live, async and practice rolling. The installed build must support your supplied settings; a branch name in JSON does not switch installations.",
            "Pooled qualifier modes have their own generator and baseline configuration on the Qualifiers page. Existing pooled OWR modes also use the tournament installation. Changing the event generator does not replace already generated seeds. MMR generation is not implemented for official or async events.",
        ],
        "seed_config" => &[
            "The JSON object consumed by the selected generator. Choose a matching example below, then replace the sample settings with your event's intended configuration. JSON requires double quotes and does not support comments or trailing commas.",
            "For OWR and Door Rando mutual choices, use either a single base_settings object or a baselines map of named configurations; base_placements and start_inventory supplement each baseline. choices maps signup/practice choice keys to labels and patches. Pooled qualifiers use the mode's baseline only. For Avianart use preset and optional practice_presets; for TWWR use permalink; for mystery generation use mystery_weights_url.",
            "The generator build is selected above, not by adding a branch or executable path to this object. Settings from another build may not be compatible. Generation and a patch test with your actual settings are the way to verify the combination.",
        ],
        "enter_flow_json" => &[
            "Defines the on-site registration steps and requirements. Each entry has a type and its required fields, such as a rules acknowledgement, external registration instruction, Discord membership requirement or player choice.",
            "Use the documented examples below for the exact object shape. booleanChoice and radioChoice store answers by key; generator choice patches must use the same key to affect seeds. A display label alone does not connect an answer to a seed setting.",
            "Signup requirements and qualification scoring are separate. Configure Qualification method and any qualifier windows explicitly rather than relying on a signup step's name to control the event. Preview the full signup flow with a test entrant before opening registration.",
        ],
        "organizer" => &[
            "Adds an existing site user as an organizer for this event. Search for the intended account and verify the selection before submitting.",
            "Organizer access allows event-management actions; it is separate from having a Discord staff role or receiving messages in the organizer channel. Use the participant and volunteer role settings for Discord roles.",
        ],
        "copy_source_event" => &[
            "Copies organizer assignments from another event as a shortcut when the same staff team manages both. Select the source event deliberately and review the resulting organizer list.",
            "This copies organizer access, not seed settings, entrant registrations, schedules or qualifier results. Use the event's separate configuration controls for those settings.",
        ],
        _ => unreachable!("setting help must have authored content"),
    };
    crate::http::setting_label(field, title, html! {
        @for paragraph in paragraphs { p : *paragraph; }
        @if field == "preroll_mode" { : super::preroll_help(); }
        @if matches!(field, "seed_config" | "enter_flow_json") { : seed_choice_help(); }
        @if field == "seed_config" { : super::seed_config_help(); }
        @if matches!(field, "qualifier_score_kind" | "qualifier_score_config") { : fixed_scoring_help(); }
    })
}

pub(super) fn seed_choice_help() -> RawHtml<String> {
    html! {
        h4 : "Player choices and their effect on seeds";
        p : "These rules apply to OWR (both builds) and Door Rando with source mutual_choices. Enter Flow defines the question and allowed answers; Seed Config defines what an enabled answer changes. Connect them using exactly the same choice key. A choices entry is a patch object, not a true/false value or a list of answers.";
        details {
            summary : "Named baselines and a shared mode draft";
            p : "Define baselines as an object keyed by stable identifiers such as mode_a. Each entry contains label, base_settings, optional base_placements and start_inventory. Each is a complete baseline: there is no inheritance from a root baseline. Put choices alongside baselines. By default a choice applies to every mode. Add baselines: [\"mode_a\"] inside a choice to restrict it to that mode, or list several mode keys. The list must be non-empty and reference existing baselines. Other modes ignore that patch, including its starting items and suppression rules.";
            p : "The signup form can collect all mode preferences. Practice shows only options for its selected baseline; seed generation and final room summaries use the same restriction. Choices resolved before the draft remain saved, but only the selected mode's applicable choices affect its seed. Omit the per-choice baselines field for shared options and existing single-baseline events.";
            p : "Use either named baselines or a root baseline, not both. Keys must be 1–64 letters, digits, underscores or hyphens. Each draft option's preset must match a baseline key. With a draft, generation waits for the completed picks and uses the current game's selection; an invalid or missing selection never falls back to another mode.";
            p : "For an event without a preset draft, set default_baseline to one of the configured keys. Practice offers a baseline dropdown and optional-choice checkboxes. Pooled qualifier modes still take a single explicit baseline in their own configuration, not a baselines collection.";
            pre : serde_json::to_string_pretty(&json!({
                "label": "mode",
                "options": [
                    {"display_name": "Mode A", "preset": "mode_a"},
                    {"display_name": "Mode B", "preset": "mode_b"},
                    {"display_name": "Mode C", "preset": "mode_c"}
                ],
                "order": [{"phase": "pick", "team": "high_seed"}, {"phase": "pick", "team": "low_seed"}]
            })).expect("example JSON serializes");
            p : "Put this object in Draft Config JSON and select Generic Ban/Pick. It uses stored qualifier ranks: higher seed picks game 1, lower seed picks game 2, and the remaining mode is the possible game 3. Create two games for double RR, or three for BO3. Keep round_modes unset, since fixed round modes disable drafting.";
            p : "The selected baseline and final choice outcomes are saved with each generated seed and shown in Racetime and async delivery. Later edits do not change the description of an existing seed. Always + Random and Random + Random both remain one 50/50 decision for each game; both async participants receive the same saved result.";
        }
        details {
            summary : "Yes/No versus Never/Random/Always";
            p : "Use booleanChoice for Yes/No, or radioChoice for Never/Random/Always. Yes is equivalent to Always; No is equivalent to Never. Both use the same seed patch. The three-option question additionally lets a player consent to a random decision about whether that patch is applied.";
            p : "For an official match, the site combines the stored choices of its participating teams (a solo entrant is a team of one). It uses the most restrictive answer for each key: Never before Random before Always. Always means consent, not an override of an opponent's refusal.";
            ul {
                li : "Any No/Never, missing answer or unrecognized answer: the choice is disabled.";
                li : "Everyone Yes/Always: the choice is enabled.";
                li : "Everyone permits it, and at least one answer is Random: one 50/50 decision enables or disables the choice for that game.";
            }
            p : "Choose when random player choices resolve in event setup: On race creation / import, On room opening, or On seed rolling (the default). The Seed Config JSON field is choice_resolution, with values race_creation, room_opening and seed_rolling. Creation waits until all participants are registered. Room opening means the first live room or async part, and falls back to seed generation when no room opens. Practice rolls remain independent; pooled qualifiers use only baseline settings.";
            p : "Resolved results appear in the scheduling thread and race settings, and are reused after rescheduling, room recreation, restarts and generation failures. Player preference edits only affect unresolved races. Once resolved, participant replacements and choice-definition changes require restoring the original configuration or creating a replacement race. Existing seeds keep their saved descriptions.";
            p : "For example, Always + Random and Random + Random both give a 50% chance, not 25%. Never + Always and Never + Random are disabled. Each configured choice is resolved once per game; seed generation and all announcements use that saved decision, including seed rerolls.";
        }
        details {
            summary : "Matching Enter Flow and Seed Config examples";
            p : "This example changes the goal to All Dungeons and sets aga_randomness to false when the choice is enabled. Select an OWR build that supports these settings. For Door Rando mutual choices, also add source: mutual_choices as a JSON string property at the top level of Seed Config.";
            h5 : "Seed Config JSON — identical for either question type";
            pre : serde_json::to_string_pretty(&json!({
                "base_settings": {"shuffle": "crossed", "goal": "crystals"},
                "base_placements": {},
                "start_inventory": [],
                "choices": {
                    "all_dungeons": {
                        "label": "All Dungeons",
                        "settings": {"goal": "dungeons", "aga_randomness": false}
                    }
                }
            })).expect("example JSON serializes");
            p : "Choose ONE of the following Enter Flow examples. When editing an existing flow, add the requirement to its existing requirements array instead of replacing your other signup steps. The label is display text; all_dungeons is the key connecting the answer to the patch.";
            @for (title, question_type) in [("Yes/No question", "booleanChoice"), ("Never/Random/Always question", "radioChoice")] {
                h5 : title;
                pre : serde_json::to_string_pretty(&json!({
                    "requirements": [{
                        "type": question_type,
                        "key": "all_dungeons",
                        "label": "All Dungeons",
                        "prompt": "Allow All Dungeons when your opponent also permits it?"
                    }]
                })).expect("example JSON serializes");
            }
            p : "Enabling a choice can set a generator setting to false, as aga_randomness demonstrates. The player's Yes/Always enables the whole patch; it is not copied as a boolean value into each setting. An answer without a matching Seed Config choice has no seed effect.";
        }
        details {
            summary : "Baseline, patches, conflicts and starting items";
            p : "Generation starts with base_settings, base_placements and start_inventory. Enabled, unsuppressed choices then patch that baseline. A disabled choice does nothing: it does not reset settings or force a feature off. Put your intended default/off configuration in the baseline. Put mandatory settings in the baseline without a player choice.";
            ul {
                li : "settings replaces individual generator settings; placements replaces individual fixed placements. Values are shallow replacements, so a nested object replaces that whole field rather than merging recursively. A null value removes the key from the generated configuration; it does not explicitly set it to false.";
                li : "start_inventory appends item names to the baseline inventory. It does not replace the inventory, remove items or deduplicate repeated items.";
                li : "priority is an integer, default 0. Patches apply from lower to higher priority; at equal priority, they apply in alphabetical choice-key order. The later patch wins for a shared setting or placement; unrelated changes remain.";
                li : "supercedes (spelled exactly this way) is an array of other choice keys. When a choice with a seed patch is enabled, those other patches are suppressed entirely. For example, \"supercedes\": [\"all_dungeons\"] skips the All Dungeons patch, including its aga_randomness change. Suppression is collected from all enabled seed choices before patches are applied; avoid circular suppression, which can skip both patches.";
            }
            p : "Use the settings, placements and start_inventory sections for new patches. Legacy flat patches are also accepted, with non-metadata keys treated as generator settings, but do not mix flat settings with these sections: when any section is present, only the sections supply the patch.";
        }
        details {
            summary : "Display labels, rules without seed changes, practice and pooled qualifiers";
            p : "label names the choice in practice and race displays. Optional value_labels maps never, random and always to display text; these labels do not change the decision or generator settings, and do not rename the signup answer buttons. An empty display label hides that value where these labels are used.";
            p : "A choice containing only metadata, with no settings, placements, start_inventory or legacy setting fields, is displayed as a player rule and does not modify the seed. It can still use the same mutual agreement and random decision rules. Describing a rule does not automatically enforce it.";
            pre : serde_json::to_string_pretty(&json!({
                "no_delay": {
                    "label": "No stream delay",
                    "hidden_for_async": true,
                    "value_labels": {
                        "never": "Use the event's required stream delay",
                        "random": "Random stream-delay agreement",
                        "always": "No stream delay"
                    }
                }
            })).expect("example JSON serializes");
            p : "Add that entry inside choices and a matching question inside Enter Flow. hidden_for_async omits a choice from async scheduling/rule displays; it is not a switch to disable a seed patch for async races.";
            p : "Practice rolling exposes configured choices as checkboxes: checked means enabled, unchecked means disabled. It does not combine opponents' signup answers or offer the signup question's Random option. Pooled qualifier seeds use their mode's baseline only, without entrant choice patches or drafts; these examples do not make a pooled seed vary by requester.";
        }
    }
}

fn fixed_scoring_help() -> RawHtml<String> {
    html! {
        h4 : "Fixed scoring strategies";
        p : "Higher scores are better. These strategies combine scored live qualifiers and submitted async finish times, ordered by their start timestamps. The limits below refer to those recorded results, not every room a player has joined. Meeting a strategy's minimum requirement does not itself specify how many players advance to your bracket.";
        details {
            summary : "Standard — adjusted par points, first 8 results";
            p : "Each finish earns 100–1100 points. Par is the average of the fastest seven finishes; smaller live fields use the entrant count, and smaller async cohorts use the available finish count. The formula includes short-seed and finish-spread adjustments, which can reduce the effective gap behind par by at most 10 minutes.";
            ul {
                li : "Use only the first 8 recorded results, ordered chronologically.";
                li : "Live forfeits, disqualifications and declined results score 0 and use a result slot. Remove zero scores before selecting the scores to sum.";
                li : "Once at least 2 results are recorded, discard the single highest score. Then sum the highest 4 remaining scores, or all remaining scores if fewer than 4 exist.";
                li : "At least 5 finishes within those first 8 results are required to meet the leaderboard's minimum qualification requirement. Points can still appear before that requirement is met.";
            }
            p : "Example: results 1100, 1050, 1000, 950, 900, 850, 0, 0 produce 3900 points: discard 1100, then add 1050 + 1000 + 950 + 900.";
            details {
                summary : "Exact Standard formula";
                p : "Use minutes for all times. T is the player's finish time, A is the par average, and S is the sample standard deviation of the finish times used for par. clamp(x, low, high) limits x to that range.";
                pre : "short_seed_factor = 8 × clamp((150 − A) / 50, 0, 1)\njet = min(8, short_seed_factor × max(0, (T − A) / 8 × 0.35))\ngamble = min(5, S × max(0, (T − A) / S × max(0, (S / A) / 0.035 − 1) × 0.3))\nadjustment = min(10, jet + gamble)\npoints = clamp(1000 × (1 − (T − A − adjustment) / A), 100, 1100)";
                p : "The adjustment is zero for a finish at or ahead of par. The spread adjustment is also zero when the par finish times have no spread. The final score is not rounded to a whole number.";
            }
        }
        details {
            summary : "SGL — shared 10–110 point formula";
            p : "Par is the mean of the fastest 3 finishes when the field has fewer than 20 entrants, or the fastest 4 finishes when it has 20 or more. For asyncs, the threshold uses the number of submitted finish times in that async cohort. The required par finishes must be available to establish the score.";
            pre : "points = clamp(100 × (2 − finish_time / par_time), 10, 110)";
            p : "A finish exactly at par earns 100 points; a finish 20% slower earns 80. A finish 20% faster would yield 120, capped at 110. There is no whole-number rounding. Live forfeits, disqualifications and declined results earn 0, rather than the 10-point finish minimum.";
            p : "All three seasons average their retained scores. The divisor is at least 3, so fewer than 3 retained results effectively include missing results as zero. Higher final averages rank ahead of lower averages.";
        }
        details {
            summary : "SGL 2023 Online — first 5, discard best and worst";
            ul {
                li : "Use only the first 5 recorded results.";
                li : "With 1–3 results, keep them all and divide their sum by 3.";
                li : "With 4 results, discard the highest score and average the other 3.";
                li : "With 5 results, discard both the highest and lowest scores, then average the middle 3.";
                li : "The leaderboard's minimum requirement is 3 recorded results; live forfeits count toward this entry requirement even though they score 0.";
            }
            p : "Example: 110, 100, 90, 80, 0 becomes (100 + 90 + 80) / 3 = 90 after dropping 110 and 0.";
        }
        details {
            summary : "SGL 2024 Online — first 6, discard worst";
            ul {
                li : "Use only the first 6 recorded results.";
                li : "With 1–3 results, keep them all and divide their sum by 3.";
                li : "With 4–6 results, discard the single lowest score and average every remaining score. The highest score is retained.";
                li : "The leaderboard's minimum requirement is 3 recorded results; live forfeits count toward this entry requirement.";
            }
            p : "Example: 110, 100, 90, 80, 70, 0 becomes (110 + 100 + 90 + 80 + 70) / 5 = 90. It is an average of 5 retained results, not just the best 3.";
        }
        details {
            summary : "SGL 2025 Online — SGL 2024 scoring, 3 finishes required";
            p : "Uses the same first-6 limit, discard-worst rule and averaging as SGL 2024. The difference is the minimum requirement: at least 3 finishes are needed. Three recorded results that include a forfeit are not sufficient unless at least three of the counted results are finishes.";
        }
        p : "Async handling: these fixed strategies currently include submitted async results with a recorded finish time. An async submission without a finish time is omitted; it does not contribute the zero-point attempt that a live forfeit does. Select and test the strategy against your intended mix of live and async qualifiers.";
    }
}
