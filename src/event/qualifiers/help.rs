use crate::prelude::*;

pub(super) fn label(id: &str, field: &str, title: &str) -> RawHtml<String> {
    let paragraphs: &[&str] = match field {
        "required_mode_count" => &[
            "The number of distinct qualifier modes each entrant must complete to receive an overall average. A mode is a complete ruleset, such as Open or Inverted, with its own private seed pool and linked live qualifiers.",
            "This must exactly match the number of enabled mode cards below. For three required modes, create and enable three modes. Each contributes one counted result; the overall score is their arithmetic mean, including zero-point outcomes. Unfinished or pending modes prevent a complete average.",
            "Changing this count does not create or remove mode cards. It is a structural setting and cannot change after the qualifier settings lock.",
        ],
        "pool_seed_count" => &[
            "How many private async seed slots each enabled mode must have ready before requests can be activated. With three modes and three private seeds per mode, prepare nine private seeds in total.",
            "A slot is shared by a group of entrants. Each entrant is assigned one seed for that mode; they do not play every slot. Assignment balancing distributes requests among seeds. Each physical seed has its own par time.",
            "Save the count, then use Generate missing slots in each mode’s seed pool. This field does not generate anything by itself. The count is locked when the first seed reveal or live eligibility cutoff/start locks the configuration.",
        ],
        "live_races_per_mode" => &[
            "The exact number of linked live qualifier races required for each enabled mode. Zero permits an entirely async qualifier. Live races are separate from the private seed slots.",
            "Create and link the races in Live Qualifier Races below. Each needs a valid scheduled start within the qualifier window; before first activation, linked starts must also be in the future. Readiness checks compare the linked race count with this value.",
            "Live and async attempts feed the same mode result workflow. Each physical live seed has its own cohort and par. This count is a structural setting.",
        ],
        "requests_open_at" => &[
            "The earliest time an entrant can request a private async qualifier. Requests also require an unpaused configuration, enabled mode, ready pool, valid entrant and no conflicting active attempt.",
            "All six date/time fields are interpreted as UTC. The browser’s date picker does not convert from your local timezone. For example, enter 18:00 for an intended 18:00 UTC opening.",
            "The opening must be strictly earlier than the last-request time and the retry deadline. Saving dates alone does not unpause requests.",
        ],
        "requests_close_at" => &[
            "The cutoff for new private seed requests, including retry requests. At the timestamp itself, the request window is closed. An entrant who already received a seed has separate GO and submission deadlines.",
            "Must be later than requests open, no later than Last start (GO), and at least as late as the retry deadline. For example: stop requests at 18:00 UTC, allow GO until 18:30, then allow submissions until 23:00.",
        ],
        "starts_close_at" => &[
            "The last permitted start boundary for private async attempts. GO is the recorded start of the timed run after the READY/countdown workflow. It is not the time the seed was requested.",
            "The request cutoff cannot be later than this time, and this time cannot be later than the submission deadline. Linked live qualifier start times must be earlier than this boundary.",
            "Leave enough time between GO and submissions for entrants to finish. The global submission deadline can shorten an individual run’s allowed duration.",
        ],
        "submissions_close_at" => &[
            "The event-wide deadline for private async run submissions. An attempt’s deadline is the earlier of GO plus its async time limit and this global cutoff.",
            "For example, with a 12-hour run limit and a 23:00 UTC submission deadline, an entrant starting at 20:00 has only three hours. Overdue active attempts are handled by the expiry workflow.",
            "This must be at or after the last GO and no later than standings publication. Verification and organizer corrections are separate from a participant’s submission window.",
        ],
        "retries_close_at" => &[
            "The extra deadline for requesting a replacement attempt. A retry must also fit within the ordinary request, start and submission windows; this timestamp does not extend any of them.",
            "It must be after requests open and no later than the last-request time. Set it even when retries are disabled, because activation requires all six timestamps.",
            "The event-wide retry allowance is shared across modes and live/async routes. Replacing an attempt changes which result counts; the earlier result is not retained as a best-of score.",
        ],
        "results_release_at" => &[
            "The earliest publication time for pooled standings and the pooled workflow’s public result/source disclosure. Organizer views on this page remain available before publication.",
            "Must be at or after the submission deadline. Allow time for VOD checks and result verification if you want standings to be complete when publication begins. Pending par calculations can still leave scores pending.",
            "The event’s score-hiding settings also affect public presentation. Reaching this timestamp does not manufacture missing results or complete organizer reviews.",
        ],
        "async_run_limit_hours" => &[
            "Maximum duration of a private async run, in whole hours, measured from recorded GO. Enter a positive integer; the default configuration uses 12 hours.",
            "The actual deadline is the earlier of GO plus this limit and the global submission deadline. Seed-request time is not the start of this clock. Changing this structural limit is blocked after settings lock.",
        ],
        "live_entry_close_minutes" => &[
            "How many minutes before the scheduled live race start the eligible entrant list is frozen. For a 19:00 UTC race and a value of 10, the cutoff is 18:50 UTC.",
            "The live eligibility ledger records the cutoff decision and whether the entrant was present at GO. Joining the room is not sufficient by itself to guarantee a counted qualifier attempt.",
            "Zero places the cutoff at the scheduled start. This is distinct from the private async request deadline and is locked with the structural configuration.",
        ],
        "retry_limit" => &[
            "Choose 0 to disable replacement attempts or 1 to allow one retry per entrant across the entire event. It is not one retry for each mode, seed or source. Larger values are not supported.",
            "A retry replaces the counted attempt in the same mode; it is not a best-of-two result. An async retry uses a different physical seed. The original attempt remains in the ledger and an eligible earlier finish can still contribute to its seed’s par.",
            "Live retry reservations are recorded separately as reserved, committed or released. The retry deadline, eligibility checks and disclosure sanctions can prevent a retry even when the allowance is otherwise unused.",
        ],
        "allocation_spread" => &[
            "Controls how far apart the most-used and least-used private pool seeds may be when assigning entrants. A smaller number favors more even cohort sizes. Enter an integer of at least 1; the initial configuration uses 2.",
            "Loads count non-void assignments, including replaced attempts. This is not a maximum number of entrants per seed or a cap on total registrations.",
            "Retries must avoid the original seed. The allocator can record an explicit retry exception when the normal spread would prevent a replacement; this setting is not an absolute guarantee that all cohort counts stay within that spread.",
        ],
        "par_finishers" => &[
            "Par is the arithmetic mean of the fastest N eligible, finalized finish times on the same physical seed. With N = 5, the five fastest eligible finishes define the comparison time for that seed.",
            "Each seed needs at least N eligible finishes before finish scores can be calculated. Until then they remain pending. A forfeit, disqualification or invalid result scores zero and does not supply an eligible finish.",
            "Replaced attempts may remain eligible for par even though they no longer count toward that entrant’s average. Staff result corrections can change par and consequently other scores on that seed.",
        ],
        "score_scale" | "score_offset" | "score_minimum" | "score_maximum" => &[
            "Finished-run points = scale × (offset − finish time ÷ seed par), clamped between the minimum and maximum. Times use the same units. Par is calculated separately for each physical seed from its fastest eligible finishes.",
            "With scale 100, offset 2, minimum 0 and maximum 105: a finish at par earns 100 points; a finish at 1.2 times par earns 80; a finish at 0.9 times par would earn 110 and is capped at 105.",
            "Scale must be positive; all four values must be finite and maximum must be at least minimum. Finished runs remain pending until par exists. Forfeits, disqualifications and invalid results receive zero, independently of the minimum for finished runs.",
            "The final average uses one counted score per required mode, including zeroes. These scoring settings are locked once the qualifier configuration locks.",
        ],
        "requests_paused" => &[
            "Keep this checked while preparing modes, seed pools, schedules and deadlines. Saving with it unchecked attempts to activate requests and runs the readiness checks shown above the form.",
            "If any readiness check fails, the configuration save is rolled back. Resolve the listed issues, save the preparation settings while paused, and then try activation again. Unpausing does not bypass the configured opening time.",
            "Pausing stops new requests; it does not cancel existing attempts or reset the settings lock. Once locked, structural settings cannot be changed by pausing again. Date/time fields and the pause control remain separately editable.",
        ],
        "position" => &[
            "Display order of this mode in the qualifier workflow. Use positive integers such as 1, 2 and 3. This is not a seed count, mode identifier or score multiplier.",
            "Reordering and renaming are presentation changes. They can be saved without changing the mode’s generator configuration or existing seed assignments.",
        ],
        "slug" => &[
            "A stable identifier for this mode within the event, for example inverted or mc-boss. Use a short, recognizable slug and keep it distinct from the other modes. Entrants see the display name.",
            "Changing the slug is a material mode change. Material changes are blocked while requests are unpaused, after settings lock, or once the mode already has generated/failed seed material, releases, eligibility cutoffs or attempts. Finalize it before generating seeds.",
        ],
        "display_name" => &[
            "The human-readable title shown for this qualifier mode, for example Inverted + Keysanity. Include enough information that entrants can distinguish the rulesets.",
            "Each card represents one complete qualifier ruleset and one required mode result, not a Yes/No preference or a private seed slot. A mode has its own pool and can also have linked live races.",
            "Renaming affects presentation and does not change generated seeds. Use the baseline settings JSON to define what the mode actually generates.",
        ],
        "seed_gen_type" => &[
            "Selects the generator for this mode’s private seeds and linked live qualifier races. It is independent of the main event generator in Setup.",
            "For pooled qualifiers, both OWR identifiers use the deployed OWR tournament build. Door Rando supports mutual_choices with one baseline or mystery_pool with a weights URL; its boothisman preset source is not supported here. Avianart needs a default preset, and TWWR needs a settings permalink.",
            "Every selection needs matching Baseline settings JSON. Choose the generator first, use the examples in that field’s help, and verify your intended settings before generating the pool. The default profile selects the configured installation; a branch field in JSON cannot select another build.",
        ],
        "seed_config" => &[
            "This is the generator configuration for one complete qualifier mode. Enter a JSON object with double-quoted keys and strings; comments and trailing commas are not valid JSON.",
            "For OWR and Door Rando mutual choices, define one base_settings object plus optional base_placements and start_inventory. Named baselines maps are not supported for pooled modes: create a separate mode card for each ruleset instead. Entrant choice patches and match drafts are not used when rolling pooled qualifiers.",
            "An empty object is not a description of your intended ruleset. Expand a matching example below and replace the sample settings with values supported by the deployed generator. The examples explain the structure and do not validate your tournament rules.",
            "Finish this configuration before generating seed material. Generator, JSON, profile, slug and enabled-state changes are protected once the mode is in use. Display name and order can still be edited.",
        ],
        "generator_profile" => &[
            "Selects the deployment’s configured generation profile. This installation currently supports only default. For pooled OWR, that means the tournament build; other generators use their respective configured installation.",
            "This is not a settings preset or a branch selector. Rules belong in Baseline settings JSON. Readiness rejects unsupported profile names, and existing seeds must match the mode’s recorded profile and settings fingerprint.",
        ],
        "enabled" => &[
            "An enabled mode participates in qualifier readiness and new assignments. The number of enabled cards must exactly equal Required modes. Disabled modes retain their historical assignments and results for organizer review.",
            "Save each mode separately. Adding or enabling a mode does not generate its pool or link live races. Continue to Seed pools and Live Qualifier Races after saving.",
            "Changing enabled state is a material change: it is blocked while requests are active, after settings lock, or once seed material or attempts exist for the mode. It is not a way to discard a mode’s history during an event.",
        ],
        "pool_position" => &[
            "The numbered private seed slot within this mode, starting at 1 and ending at Private seeds per mode. Slots in different modes are independent.",
            "Generate missing slots queues absent slots. Generating a specific slot creates it if missing or retries a failed, unused, unreleased slot. It does not reroll a ready seed or replace an entrant’s assignment. Reload the page to see worker progress.",
            "For manual import, choose an unused slot while requests are paused. Existing seed assignments and released seed material cannot be silently replaced.",
        ],
        "seed_data" => &[
            "Canonical delivery data for an already generated seed. This is different from Baseline settings JSON: settings describe how to roll a seed; seed data identifies the actual output and includes the information needed to deliver it.",
            "The payload must match the selected generator. OWR and Door Rando require a valid UUID, five hash icons and an existing nonempty patch on this deployment; Avianart and TWWR require their supported delivery identifiers and hash data. A generator settings object or a pasted download URL alone is insufficient.",
            "Prefer Generate missing slots for normal setup. Import requires requests to be paused, an enabled mode, an in-range unused slot, matching generator payload and your settings/build attestation. Duplicate physical seeds are protected against reuse. Do not invent a payload to bypass validation.",
        ],
        "attest_settings" => &[
            "Confirms that you checked this imported seed against the mode’s baseline settings and the actual generator build deployed for this event.",
            "Payload validation can check delivery data and patch availability; it cannot prove that every gameplay setting is correct. Your attestation is recorded with the imported seed and checked by readiness. Leave this unchecked until that review is complete.",
        ],
        "notification_role_id" => &[
            "Optional Discord role to mention when a qualifier room opens. Enter the numeric role ID, copied with Discord Developer Mode enabled, rather than a role name or channel ID.",
            "The role belongs to the event’s Discord server. Saving it selects the announcement audience; it does not assign the role to entrants or configure the announcement channel. Disable Ping removes this qualifier-specific mention.",
        ],
        "qualifier_score_hiding" => &[
            "Chooses how much qualifier information is withheld from public presentation. None shows scores, Async only hides async scores, and the Full options progressively withhold points, counts and additional qualifier information.",
            "This organizer page remains available for verification and troubleshooting. Pooled qualifiers also apply their publication timestamp, so selecting None does not publish pooled results before that release boundary. Review the public entrants view when setting your event’s disclosure policy.",
        ],
        "automated_asyncs" => &[
            "Enables bot-managed private Discord threads for qualifier asyncs. Entrants use the supported READY/countdown/GO and finish controls to receive and report their attempt.",
            "Pooled activation requires automated asyncs and a configured Discord async channel. Configure the event’s server and channel in Setup and make sure the bot can manage private threads. Enabling this checkbox does not fill seed pools or open requests by itself.",
        ],
        "recovery_action" => &[
            "Repairs a failed Discord delivery for this attempt. Retrying READY or seed delivery keeps the original seed and preparation deadline. It does not create a new attempt or grant a retry.",
            "Use connection actions only after reviewing the existing private thread or bot GO message. Supply its numeric Discord ID and explain the evidence. GO recovery uses an existing bot message; it does not let an organizer invent a replacement start time.",
        ],
        "discord_id" => &[
            "For Connect existing private thread, enter that thread’s numeric Discord ID. For Connect existing GO message, enter the existing bot GO message ID. It is not a URL or entrant account ID.",
            "Retry READY delivery and Retry same seed delivery do not need a connection ID. Inspect the private thread before reconnecting anything, and record what you checked in the reason field.",
        ],
        "reason" => &[
            "Required explanation of the organizer action and the evidence reviewed. Include enough context for another organizer to understand the decision, such as the relevant VOD timestamp or the Discord delivery issue.",
            "Result corrections and sanctions retain before/after history. Saving a result can change a seed’s par and other entrants’ scores, so describe both the issue and the corrected outcome. Reload before acting if another organizer has changed this attempt.",
        ],
        "result_action" => &[
            "Verify or correct result records the official outcome for this attempt. Select a finish time and VOD for a valid finish, or the appropriate zero-point outcome. Changes are retained in the review history.",
            "Apply mode disclosure sanction is a distinct disciplinary action for disclosure of that mode’s seed information. It affects the entrant’s mode attempts and retry eligibility. Reverse mode disclosure sanction restores the saved state for that sanction; it is not a general undo for arbitrary edits.",
            "Use the event’s published rules and reviewed evidence to choose the action. The reason field is mandatory, and stale controls are rejected if the attempt has changed since this page loaded.",
        ],
        "outcome" => &[
            "Finished is a verified completed run and requires an official duration and VOD. Its points depend on the par for that physical seed and may remain pending until enough eligible finishes exist.",
            "Forfeit / missing evidence, Disqualified and Invalid receive zero points and do not supply a finished time for par. The separate result action selects whether you are recording a result or applying/reversing a disclosure sanction.",
        ],
        "finish_time" => &[
            "Official elapsed run duration, not a clock time or a UTC timestamp. Use HH:MM:SS, for example 01:23:45 for one hour, 23 minutes and 45 seconds.",
            "Required when recording a Finished outcome. Verify the duration against the run evidence. Updating an eligible finish can change the fastest-finish par and all derived scores for the same physical seed.",
        ],
        "vod" => &[
            "The full URL of the recording used to verify the finish. A finished result requires VOD evidence. Prefer a durable link that organizers can revisit when reviewing corrections.",
            "This page is an organizer view. Public result and source disclosure follows the event’s visibility and pooled publication rules. Supplying a VOD does not by itself verify the run or mark the result as finalized.",
        ],
        "race_phase" => &[
            "The phase title shown for this live race, typically Qualifier. This is presentation text; the race is explicitly recorded as a qualifier regardless of its title.",
            "Changing a display name does not assign the race to a pooled mode. Select Qualifier mode separately so the race uses the correct baseline and counts toward that mode’s required live races.",
        ],
        "qualifier_number" => &[
            "Positive qualifier sequence number used to identify this race, for example 1, 2 or 3. It is separate from a private pool slot and from a mode’s display order.",
            "The optional round text can provide a readable name such as Live 1. For pooled qualification, the selected mode links this live race to the appropriate ruleset and readiness count.",
        ],
        "race_round" => &[
            "Optional round label for the race, for example Live 1 or Friday evening. Use it to distinguish multiple races with the same phase.",
            "This is separate from the numeric qualifier number, scheduled start and pooled mode selection. A round label alone does not link a race to its qualifier mode.",
        ],
        "race_start" => &[
            "Scheduled race start, entered directly in UTC. The date picker does not convert your local time. The live entry cutoff is calculated by subtracting the configured cutoff lead from this scheduled start.",
            "Pooled live races need a start within the qualifier window: at or after requests open and before the last GO boundary. Before the initial activation, linked starts must also be in the future. The seeding race is a separate event workflow.",
        ],
        "race_room" => &[
            "Optional full racetime.gg room URL for an existing race room. Leave empty when the normal event workflow should create or connect the room later.",
            "A room URL does not set the race’s scheduled time, pooled mode, eligibility cutoff or official result. Those fields and workflows remain separate.",
        ],
        "qualifier_mode_id" => &[
            "The enabled mode whose ruleset this live qualifier uses. Linking the race here makes it count toward Live races per mode for that specific mode.",
            "Live qualifiers use the mode’s fixed baseline generation settings, just like its private pool. A live seed has a separate cohort and par from the private seeds. Save and enable a mode above before creating its linked live races.",
        ],
        _ => unreachable!("qualifier help must have authored content: {field}"),
    };
    crate::http::setting_label(
        id,
        title,
        html! {
            @for paragraph in paragraphs { p : *paragraph; }
            @if field == "seed_config" { : examples(); }
        },
    )
}

fn examples() -> RawHtml<String> {
    html! {
        @for (title, description, example) in [
            ("OWR — one qualifier baseline", "Choose OWR. Replace the example goal and other settings with your ruleset; the deployed build determines the supported values.", json!({"base_settings": {"goal": "ganon"}, "base_placements": {}, "start_inventory": [], "choices": {}})),
            ("Door Rando — fixed baseline", "Choose ALTTPR Door Rando. The mutual_choices source uses one fixed baseline here; pooled qualifiers do not apply signup preferences.", json!({"source": "mutual_choices", "base_settings": {"shuffle": "crossed"}, "base_placements": {}, "start_inventory": [], "choices": {}})),
            ("Door Rando — mystery weights", "Choose ALTTPR Door Rando and replace this placeholder with your accessible weights YAML URL.", json!({"source": "mystery_pool", "mystery_weights_url": "https://example.com/qualifier-weights.yaml"})),
            ("Avianart — preset", "Choose ALTTPR Avianart and supply a preset supported by that deployment.", json!({"preset": "casualboots"})),
            ("The Wind Waker Randomizer — permalink", "Choose The Wind Waker Randomizer and replace the placeholder with the exported settings permalink.", json!({"permalink": "PASTE_YOUR_SETTINGS_PERMALINK_HERE"})),
        ] {
            details {
                summary : title;
                p : description;
                pre { code : serde_json::to_string_pretty(&example).expect("static JSON example"); }
            }
        }
    }
}
