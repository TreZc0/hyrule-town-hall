//! Shared inline references for the raw JSON editors and organizer form builder.
use crate::prelude::*;

fn fields(rows: &[(&str, &str, &str)]) -> RawHtml<String> {
    html! {
        dl {
            @for (key, shape, description) in rows {
                dt { code : *key; : " — "; : *shape; }
                dd : *description;
            }
        }
    }
}

fn example(value: &serde_json::Value) -> RawHtml<String> {
    html! { pre { code : serde_json::to_string_pretty(value).expect("guide example serializes"); } }
}

// Keep examples as JSON values so the same objects can be checked against the real parsers.
fn requirements() -> Vec<(&'static str, &'static str, serde_json::Value)> {
    vec![
        ("raceTime", "No extra fields. Requires a racetime.gg account connected to the entrant's site account. This checks the account link, not participation in a particular race.", json!({"type": "raceTime"})),
        ("raceTimeInvite", "invites (required array of strings) contains racetime.gg user IDs, not display names or site IDs. text (optional HTML string) replaces the displayed instruction; errorText (optional plain string) replaces the rejection message. Missing text fields use the built-in wording. The linked racetime account must be in the list.", json!({"type": "raceTimeInvite", "invites": ["REPLACE_WITH_RACETIME_USER_ID"], "text": "This event is invitation only.", "errorText": "Your racetime account is not on the invite list."})),
        ("twitch", "No extra fields. Requires a Twitch channel connected through the entrant's racetime.gg profile. A written Twitch name in a text question does not satisfy this check.", json!({"type": "twitch"})),
        ("discord", "No extra fields. Requires a connected Discord account. Use discordGuild when membership in the event server is also required.", json!({"type": "discord"})),
        ("discordGuild", "name (required string) is the server's display name in the instruction; it does not select a server. Membership is checked against the event's configured Discord Guild ID. roleId (optional integer, omitted/null means any member) additionally requires that role in the event server. Supply the numeric role ID as a JSON number, not a quoted string. Configure the event's Discord server and bot access separately.", json!({"type": "discordGuild", "name": "Tournament community", "roleId": 123456789012345678_i64})),
        ("challonge", "No extra fields. Requires a connected Challonge account. This is an account check, not proof of registration in a specific bracket.", json!({"type": "challonge"})),
        ("startGG", "optional (boolean, default false): false requires a connected start.gg account; true allows signup without it. The exact type spelling is startGG. This requirement alone does not check registration in a particular start.gg event.", json!({"type": "startGG", "optional": false})),
        ("startGGEventSignup", "eventSlug (required string) is the start.gg API slug, such as tournament/example/event/main, not the full browser URL. Requires a linked start.gg account found among that event's entrants. text (optional HTML string) customizes the instruction; errorText (optional plain string) customizes the error. Omit these for built-in wording.", json!({"type": "startGGEventSignup", "eventSlug": "tournament/example/event/main", "text": "Register for the main bracket on start.gg first.", "errorText": "Your linked account is not registered in that bracket."})),
        ("textField", "label (required HTML string) describes the input. long (boolean, default false) chooses single-line input or multiline textarea. regex (required string) is the pattern accepted by the Rust regex engine. regexErrorMessages (required JSON object, use {} when unused) maps patterns matching invalid answers to plain error messages. These are examined only if regex fails; the first matching error pattern supplies the message, otherwise fallbackErrorMessage (required string) is used. There is one storage slot for this type; do not add several textField requirements expecting separate answers.", json!({"type": "textField", "label": "Name for the broadcast", "long": false, "regex": "^.{1,40}$", "regexErrorMessages": {"^$": "Please enter a name."}, "fallbackErrorMessage": "Use at most 40 characters."})),
        ("textField2", "A second independent text-answer slot. Accepts exactly the same fields and defaults as textField. Use at most one of each type. There is no arbitrary textField3 or key field for creating further text slots. For free multiline text, long: true with regex: (?s)^.+$ requires at least one character including line breaks; (?s)^.*$ also accepts an empty answer.", json!({"type": "textField2", "label": "Additional information", "long": true, "regex": "(?s)^.*$", "regexErrorMessages": {}, "fallbackErrorMessage": "Please check your answer."})),
        ("yesNo", "label (required HTML string). Collects Yes or No in the single legacy yes/no slot. No is a valid answer; this does not require agreement. It has no key and does not connect to seed choices. Use booleanChoice for independently keyed seed preferences.", json!({"type": "yesNo", "label": "Have you played this tournament before?"})),
        ("booleanChoice", "key and label are required strings; label accepts HTML. key identifies the stored answer and must exactly match the corresponding Seed Config choices key to affect seeds. prompt (optional HTML string) supplies question wording; omitted/null uses label. locked (boolean, default false) prevents player edits after the event has started, not immediately after signup. Answers are the fixed Yes/No pair, stored as yes/no. Each question needs a distinct stable key. There is no configurable default answer or custom answer list.", json!({"type": "booleanChoice", "key": "full_keysanity", "label": "Full Keysanity", "prompt": "Allow Full Keysanity when your opponent also agrees?", "locked": false})),
        ("radioChoice", "Uses the same key, label, prompt and locked fields/defaults as booleanChoice. Answers are the fixed Never/Random/Always set, stored as never/random/always. Random permits a 50/50 choice for the match when neither player vetoes it. This is not an arbitrary radio-group schema: options, values, default and value_labels here do not define alternative answers. Use several keyed patches and priorities for ordered feature preferences; use a mode draft for baseline selection.", json!({"type": "radioChoice", "key": "flute", "label": "Starting Flute", "prompt": "Allow an activated starting flute?", "locked": true})),
        ("rules", "document (optional URL string, omitted/null uses the event information page) links to the rules. Requires the entrant to check an acknowledgement. Use one rules/poll acknowledgement per flow: rules and poll share the confirmation field.", json!({"type": "rules", "document": "https://example.com/rules"})),
        ("poll", "document (optional URL string, omitted/null uses the event information page) links to the poll. The entrant confirms having responded using a checkbox; the site does not query the poll provider to verify their response. Shares the confirmation field with rules, so do not use them as two independent acknowledgements.", json!({"type": "poll", "document": "https://example.com/poll"})),
        ("restreamConsent", "optional (boolean, default false): false requires the consent checkbox; true presents Yes/No and permits refusal. note (optional HTML string) adds explanatory wording. Collects consent, not a streaming schedule or automatic restream assignment. Use at most one such requirement.", json!({"type": "restreamConsent", "optional": true, "note": "Please tell us whether your races may be restreamed."})),
        ("qualifier", "asyncStart, asyncEnd and liveStart are all required date/time strings with offsets. Describes the existing single qualifier async/live entry workflow. Configure its actual async seed and live race separately. This is not the pooled-by-mode qualifier configuration and does not create a pool, define a score formula or set a redo allowance.", json!({"type": "qualifier", "asyncStart": "2027-01-01T00:00:00Z", "asyncEnd": "2027-01-07T23:59:59Z", "liveStart": "2027-01-08T18:00:00Z"})),
        ("tripleQualifier", "asyncStarts, asyncEnds and liveStarts are required arrays of exactly three date/time strings each. Corresponding array positions describe the same qualifier. Uses the existing three-qualifier entry workflow, including completion of at least one qualifier; this does not mean three configurable pooled modes. Pools, attempts and scoring belong on the Qualifiers page.", json!({"type": "tripleQualifier", "asyncStarts": ["2027-01-01T00:00:00Z", "2027-01-08T00:00:00Z", "2027-01-15T00:00:00Z"], "asyncEnds": ["2027-01-07T23:59:59Z", "2027-01-14T23:59:59Z", "2027-01-21T23:59:59Z"], "liveStarts": ["2027-01-08T18:00:00Z", "2027-01-15T18:00:00Z", "2027-01-22T18:00:00Z"]})),
        ("qualifierPlacement", "numPlayers (required non-negative integer) sets the upper placement cutoff. minRaces (non-negative integer, default 0) requires that many qualifier results; needFinish (boolean, default false) makes that minimum count finishes instead of entries. event (optional string, omitted/null uses this event) checks another event in the same series. excludePlayers (non-negative integer, default 0) supports the existing lower-bracket invitation workflow, waiting for the leading players' confirmations. This is tied to the multiple-results qualifier leaderboard and its secured-placement calculation; it does not assign stored ranks and is not a general filter for every qualification method.", json!({"type": "qualifierPlacement", "numPlayers": 16, "minRaces": 3, "needFinish": true, "excludePlayers": 0, "event": null})),
        ("external", "html (optional HTML string) and text (optional plain string) are both displayed when present. blocksSubmit (boolean, default true) unconditionally blocks on-site signup when enabled: there is no per-entrant approval checkbox created by this setting. Use false for an informational instruction that permits signup, or true when entry takes place elsewhere. The site does not check completion of a linked external form.", json!({"type": "external", "html": "See <a href=\"https://example.com/registration\">registration information</a>.", "text": "Contact the organizers if you need help.", "blocksSubmit": false})),
        ("rslLeaderboard (legacy)", "No extra fields. Retained for existing RSL events; checks the configured season's external RSL leaderboard qualification. It is specific to that integration and is not a generic leaderboard URL or an ALTTPR qualifier mode.", json!({"type": "rslLeaderboard"})),
    ]
}

fn sectioned_flow() -> serde_json::Value {
    json!({
        "closes": "2027-01-31T23:59:59Z",
        "sections": [
            {"id": "accounts", "label": "Accounts and rules"},
            {"id": "modes", "label": "Mode preferences"},
            {"id": "mcboss", "label": "McBoss", "parent": "modes"},
            {"id": "inverted", "label": "Inverted", "parent": "modes"}
        ],
        "requirements": [
            {"type": "raceTime", "section": "accounts"},
            {"type": "rules", "section": "accounts", "document": "https://example.com/rules"},
            {"type": "radioChoice", "section": "mcboss", "key": "mcboss_boots", "label": "Starting Boots", "locked": false},
            {"type": "booleanChoice", "section": "inverted", "key": "inverted_keysanity", "label": "Full Keysanity", "locked": false}
        ]
    })
}

pub(crate) fn enter_flow() -> RawHtml<String> {
    html! {
        details(class = "configuration-guide") {
            summary : "Enter Flow guide — every requirement, key, default and example";
            p : "Enter Flow defines registration checks and questions. The visual editor and Setup's Enter Flow JSON edit the same configuration. Examples inside a requirement reference are single requirements: place them inside the requirements array when using the JSON editor. The complete example below can be used as a structural starting point; replace dates, links and event-specific questions.";
            details {
                summary : "1. JSON structure, optional fields and signup deadline";
                : fields(&[
                    ("requirements", "required array of objects", "Each object needs a case-sensitive type and the fields documented for that type. An empty array means no additional requirements. The raw Setup editor also accepts a completely empty field to clear the flow; {} by itself is not a complete flow."),
                    ("sections", "optional array; default []", "Groups requirements under headings, optionally nested. Sections change presentation only; a heading named McBoss or Qualifier does not select a baseline or enable scoring."),
                    ("closes", "optional date/time string; default no separate deadline", "Use an explicit offset: 2027-01-31T23:59:59Z is UTC, or use +01:00 for that offset. At or after this instant signup is closed. The event's end and other eligibility checks still apply. The visual editor's deadline input is UTC."),
                    ("type", "required string on every requirement", "Use the exact camelCase spelling in the reference, including raceTime and startGG. Only the listed variants exist; a descriptive name cannot introduce new behavior."),
                    ("section", "optional string on any requirement", "Names an existing section id. Omitted/null leaves it ungrouped. This is a sibling of type, key and label, not a nested requirement object."),
                ]);
                p : "JSON uses double quotes, true/false without quotes, arrays in [] and objects in {}. Comments and trailing commas are invalid. Optional means a field can be omitted; null is only appropriate for nullable fields, not as a substitute for false or an empty array. Fields and choice IDs are case-sensitive. Unrecognized keys may be ignored by a parser, so saving successfully is not evidence that a misspelled option has an effect.";
                : example(&json!({"requirements": []}));
            }
            details {
                summary : "2. Sections, ordering and a complete mode-preference example";
                : fields(&[
                    ("sections[].id", "required non-empty, unique string", "A stable reference for requirements and child sections. Renaming it requires updating section and parent references."),
                    ("sections[].label", "required HTML string", "The heading visible to entrants. Plain text is fine; simple trusted markup is supported."),
                    ("sections[].parent", "optional string; default no parent", "References another section id. References must exist and the hierarchy must not contain cycles."),
                ]);
                p : "Ungrouped requirements appear first. Top-level sections follow sections-array order; within a section its own requirements appear in requirements-array order, then its child sections. Empty sections are hidden. Moving a requirement up/down changes array order, not its section assignment. Edit the raw flow in Setup to define sections and parent relationships. The same grouping is used where player choices are shown for editing.";
                : example(&sectioned_flow());
                p : "Here, the sections mcboss and inverted only group questions. To restrict mcboss_boots to its seed mode, add baselines: [\"mcboss\"] to choices.mcboss_boots in Seed Config. A section ID and a baseline ID may share a name, but the site does not infer a connection.";
            }
            details {
                summary : "3. Every available requirement type and its fields";
                p : "Fields described as required must be present even when an empty object is appropriate, such as regexErrorMessages: {}. Optional display strings can normally be omitted to use built-in wording. These examples are valid JSON structures; placeholder account IDs, event slugs, URLs and dates need replacement.";
                @for (name, description, config) in requirements() {
                    details {
                        summary : name;
                        p : description;
                        : example(&config);
                    }
                }
            }
            details {
                summary : "4. Questions, answer defaults, editing and HTML";
                p : "booleanChoice and radioChoice require an answer on signup. Neither schema has an options list, default selection, answer weights or a configurable ordering of answer buttons. The baseline's off/default behavior belongs in Seed Config. A Yes/Always answer enables the defined patch; it does not set every patched generator field to true.";
                p : "Use a unique key for every independently stored preference, including options with the same label in different modes. Keep keys stable once entrants have answered: renaming a key does not migrate their saved answers. A missing answer is treated as a veto during mutual seed resolution. Changing only a label keeps the key connection.";
                p : "locked: true restricts player changes after the event starts. It does not freeze a question at the initial signup instant. Other event editing restrictions still apply when locked is false. Once a race's choices have been resolved and saved, editing signup answers does not change that race's saved decision. See the Seed Config timing reference.";
                p : "Labels, prompts, section labels and the explicitly documented HTML fields are rendered as trusted HTML. Use simple text, emphasis and links supplied by organizers. Plain-string fields such as errorText and external.text are escaped. Example JSON shown in this guide is escaped text, so copying an HTML example does not execute it here.";
                p : "Text validation uses regular expressions, not JavaScript or an expression language. Use ^ and $ to match the whole answer and (?s) if dot should include line breaks. JSON needs escaped backslashes: the regex \\d+ is written as \"\\\\d+\". regexErrorMessages is an object mapping patterns to messages, not an array; avoid overlapping error patterns when message ordering matters.";
                p : "Account connections, rules/poll acknowledgement, consent and text/yes-no slots are specific predefined form controls. Repeating them does not create new independent storage. Use distinct keyed booleanChoice/radioChoice requirements for multiple seed preferences. Team composition and qualification scoring are configured separately from these requirements.";
            }
            details {
                summary : "5. Connecting answers to seeds and keeping qualifier setup separate";
                : super::help::seed_choice_help();
                p : "For pooled qualifiers, configure modes, seed pools, availability, attempts/redo rules and reveal/scoring on the Qualifiers page. Enter Flow does not allocate a seed or create a pool. The event's match choices do not customize a pooled seed for an individual requester: everyone assigned to that seed must race the same configuration.";
            }
        }
    }
}

fn scoped_seed() -> serde_json::Value {
    json!({
        "choice_resolution": "seed_rolling",
        "default_baseline": "mcboss",
        "baselines": {
            "mcboss": {"label": "McBoss", "base_settings": {"goal": "ganon", "mode": "open", "boss_shuffle": "full"}, "base_placements": {}, "start_inventory": []},
            "inverted": {"label": "Inverted", "base_settings": {"goal": "ganon", "mode": "inverted", "keyshuffle": "none", "bigkeyshuffle": "wild"}, "base_placements": {}, "start_inventory": []}
        },
        "choices": {
            "mcboss_boots": {"label": "Starting Boots", "baselines": ["mcboss"], "start_inventory": ["Pegasus Boots"]},
            "inverted_keysanity": {"label": "Full Keysanity", "baselines": ["inverted"], "settings": {"keyshuffle": "wild", "bigkeyshuffle": "wild", "mapshuffle": "wild", "compassshuffle": "wild"}},
            "no_delay": {"label": "No stream delay", "hidden_for_async": true, "value_labels": {"never": "Stream delay required", "random": "Stream delay decision pending", "always": "No stream delay required"}}
        }
    })
}

pub(crate) fn seed_config() -> RawHtml<String> {
    html! {
        details(class = "configuration-guide") {
            summary : "Seed Config guide — generators, every app key, baselines, choices and examples";
            p : "Seed Config supplies the selected generator's event settings. This reference covers the application's configuration keys and how they affect live, async and practice seeds. Generator settings inside base_settings and settings are passed to the installed randomizer: their full accepted key/value set belongs to that exact build, and is not a fixed list defined by this website. Use a settings export approved for your event as the baseline.";
            details {
                summary : "1. Select the generator and the correct configuration shape";
                : fields(&[
                    ("owr", "regular Overworld Randomizer installation", "Uses /opt/owr. Accepts the single-baseline or named-baseline structure below, with optional mutual choices. It does not use source, preset or a branch property to select its build."),
                    ("owr_tourney", "tournament Overworld Randomizer installation", "Uses /opt/owr_tourney for official live, async and practice generation. Same app schema as owr; the installed build must support the supplied generator settings. Seed generation and storage follow the OWR workflow; changing this selection does not convert previously generated seeds."),
                    ("alttpr_dr", "Door Randomizer with source selection", "source is optional and defaults to boothisman. Allowed source values: boothisman, mutual_choices, mystery_pool. Only mutual_choices uses the baseline/choice schema below and the local Door Randomizer installation; it does not select the OWR tournament build."),
                    ("alttpr_avianart", "Avianart presets", "preset is an optional non-empty string used when no draft supplies a preset. practice_presets is an optional list of {value, label} entries. A pooled mode needs an explicit preset. This provider does not consume the OWR choices patch schema."),
                    ("twwr", "Wind Waker settings permalink", "permalink is a required non-empty string exported from the intended randomizer version. The app uses this permalink, not an OWR-style base_settings object. Configure the version through the event's version setting."),
                    ("None", "manual/external seeds", "Leave Seed Config empty or use {}. This does not select an automatic generator."),
                    ("mmr / legacy OoTR generators", "existing compatibility options", "MMR official/async generation is not implemented. OoTR generators (stored as ootr, ootr_web, ootr_tfb and ootr_rsl) retain their separate legacy workflows; they do not consume the ALTTPR wrapper described here."),
                ]);
                p : "Select the generator in Seed Gen Type above this field. Do not put seed_gen_type, a git branch, a checkout revision, an executable path, race/job IDs, output paths or a selected_baseline into this JSON. Draft Kind, Draft Config, randomizer version, spoiler policy and preroll mode have their own event settings.";
                : super::seed_config_help();
            }
            details {
                summary : "2. OWR and Door Rando mutual choices — every root key";
                : fields(&[
                    ("source", "alttpr_dr only: string", "Use mutual_choices to enable the local Door Rando baseline/choice workflow. Omit this field for owr and owr_tourney."),
                    ("base_settings", "required object for a single baseline", "Full generator settings, including intended default/off behavior. A disabled choice leaves these settings unchanged. Preserve JSON types from the generator export: a string such as wild is not interchangeable with true, and a numeric 0 is not necessarily the same as false."),
                    ("base_placements", "optional object; default no fixed placements", "Maps exact randomizer location names to fixed item values, for example Skull Woods - Pinball Room to Small Key (Skull Woods). A choice can remove a fixed placement with a null patch value. Use the installed generator's location/item names."),
                    ("start_inventory", "optional array of strings; default []", "Exact item names granted initially. These are item names such as Pegasus Boots, not generator option keys. The app appends enabled choices' inventory; it does not deduplicate copies or remove baseline items."),
                    ("choices", "optional object; default no choices", "Maps stable signup keys to choice objects described below. The key must match booleanChoice/radioChoice.key. A choice without a question can still be exposed for practice; missing official answers veto it. Do not put true/false or answer arrays directly here."),
                    ("baselines", "alternative to root baseline fields: non-empty object", "Maps stable mode IDs to complete named baselines. When present, omit root base_settings, base_placements and start_inventory entirely. Even an empty root object/array alongside named baselines is invalid. There is no inheritance from a common root baseline."),
                    ("default_baseline", "optional string naming an existing baseline", "Required for named baselines without a preset draft. Used for the undrafted selection and practice default. It does not bypass an unfinished required draft or rescue an invalid mode selection."),
                    ("choice_resolution", "optional string; default seed_rolling", "race_creation, room_opening or seed_rolling. Defines when participant preferences and random decisions become fixed for an official race. The Resolve random player choices selector in this form writes this property and takes precedence over the value you paste here."),
                ]);
                p : "For each baselines entry, label (required non-blank string, at most 80 characters) names the mode; base_settings (required object), base_placements (optional object) and start_inventory (optional string array) define its seed. Baseline IDs must be 1–64 ASCII letters, digits, underscores or hyphens. Different named baselines may have completely different defaults. Put choices alongside baselines, not inside each baseline.";
            }
            details {
                summary : "3. Every choice property, patch ordering and mode scope";
                : fields(&[
                    ("label", "optional display string; defaults to the choice key", "Names the option in practice and seed/rule descriptions. It does not rename the stored signup key. Signup wording is separately defined by Enter Flow label/prompt."),
                    ("settings", "optional object", "Shallow patch of generator settings when enabled. Each field replaces the baseline value; a null field removes that setting. Nested objects are replaced whole, not recursively merged. To explicitly disable a generator feature, use that generator's actual off value, not null."),
                    ("placements", "optional object", "Shallow patch of fixed placements. A location mapped to null removes its fixed placement, allowing the generator to place it normally. This differs from granting a starting item."),
                    ("start_inventory", "optional string array", "Appends these items when enabled. For an activated flute, the build may require both flute_mode: active in settings and Ocarina (Activated) in this array. Merely changing a display label grants nothing."),
                    ("baselines", "optional non-empty array of unique baseline IDs", "Restricts this choice to the listed modes. Requires named baselines and existing IDs. Omitted means shared across every baseline. Other modes ignore this choice's settings, inventory, placements and suppression effects. Empty arrays, duplicates, null and unknown IDs are invalid."),
                    ("priority", "optional integer; default 0", "Enabled patches are ordered from lower to higher priority, then lexicographically by choice key. Later patches win only on fields they also change; other fields from earlier patches remain. Use higher priority when one goal should override another."),
                    ("supercedes", "optional array of other choice keys", "Exact spelling includes supercedes. An enabled seed-affecting choice can suppress the entire patch of each named choice, including inventory. References must exist and cannot refer to the same choice. Suppressors are collected before patching, so avoid chains/cycles with competing suppressors. Prefer priority for simple field precedence."),
                    ("value_labels", "optional object of display strings", "Supported entries: never, random, always. Overrides rule/choice display wording for those states; an empty string hides that wording. This does not change signup button labels, allowed answers, probabilities or generator settings. Metadata-only choices can describe rules such as stream delay without changing the seed."),
                    ("hidden_for_async", "optional boolean; default false", "Hides the choice in async scheduling/rule displays where applicable. It is not a switch to disable seed patches for async races; applied seed changes still belong to the generated seed and its settings summary."),
                ]);
                p : "Use the explicit settings/placements/start_inventory form for new entries. Legacy flat entries with generator keys directly inside a choice remain supported, but mixing flat generator fields with these explicit patch sections does not combine both forms. Misspelled metadata in a flat entry can be treated as a generator setting. Unknown configuration keys are not new supported features.";
                p : "Example of precedence: base goal is crystals; all_dungeons at priority 0 sets goal to dungeons and aga_randomness to false; completionist at priority 10 sets goal to completionist. If both enable, the final goal is completionist and aga_randomness remains false. Add supercedes: [\"all_dungeons\"] to completionist only if its entire patch must be omitted instead. A disabled completionist never undoes the other patch.";
                p : "A rules-only entry containing label/value_labels has no settings effect. The room can announce the rule; the app does not police game behavior or enforce stream delay through the randomizer. Keep seed changes inside patch fields and describe any player obligations clearly.";
            }
            details {
                summary : "4. Mutual answers, random timing, saved decisions and room display";
                : super::help::seed_choice_help();
                : fields(&[
                    ("race_creation", "resolve on creation/import", "Saves the participating players' current answers and decisions as early as the race is created/imported. A pending baseline draft can complete later; only the selected mode's choices affect the seed. Ensure preferences are collected before using this timing."),
                    ("room_opening", "resolve when opening a room", "Saves the decision as the room opens. Workflows without a room fall back to seed rolling. Subsequent seed generation uses that same saved decision."),
                    ("seed_rolling", "resolve on seed generation; default", "Collects and resolves preferences when the seed is first rolled. Once saved, rerolling does not reroll the random option decisions. Each game has its own saved result; both async participants receive the same result for that game."),
                ]);
                p : "Changing the timing affects unresolved races only. Editing a participant's preference later does not reroll an existing saved decision. Changing incompatible choice definitions or an already selected baseline can be rejected for a race with saved choices. Existing generated seeds retain their stored mode/settings presentation; changing event JSON does not regenerate them.";
            }
            details {
                summary : "5. Complete per-baseline example and how it connects to Enter Flow";
                p : "Structure example for owr_tourney, not a full tournament preset: replace each base_settings object with the complete approved export. It corresponds to mcboss_boots and inverted_keysanity in the Enter Flow example. no_delay demonstrates a shared rules-only choice; add its own radioChoice question if you want players to opt into it.";
                : example(&scoped_seed());
                p : "There is no draft in this two-mode example, so default_baseline selects McBoss for official races. In practice the user can select either baseline. Starting Boots is offered/applied only for McBoss; Full Keysanity only for Inverted. Removing a choice's baselines property makes it shared, even if its signup question remains under a mode heading.";
                p : "For the three-mode tournament draft, provide all three full baselines, set Draft Kind to Generic Ban/Pick and use the three-option Draft Config example in the mutual-choices section. Every draft option's preset must equal a baseline ID; display_name is the draft button label. First pick is high_seed, second low_seed, remaining mode is game 3. Stored qualifier ranks currently determine seeding. Keep round_modes unset when using this draft. Match scheduling creates two games for double RR or up to three for BO3; Seed Config does not decide the match format.";
            }
            details {
                summary : "6. Practice, provider-specific keys and pooled qualifiers";
                : fields(&[
                    ("practice_modes", "alttpr_dr: optional array of {value, label}", "Defines preset choices for the Boothisman practice dropdown. value is the provider's preset identifier, label is its visible name. Use modes accepted by that provider; creating a label does not create a preset."),
                    ("practice_choices", "alttpr_dr: optional array of {value, label}", "Defines practice checkboxes. With mutual_choices, each value must identify a configured choices entry and only choices applicable to the selected baseline can be submitted. With Boothisman these are provider option identifiers. This list does not collect entrants' official preferences."),
                    ("practice_presets", "alttpr_avianart: optional array of {value, label}", "Defines practice preset options. If absent/empty, practice uses the configured preset when available. Entries need non-empty labels and unique non-empty values, as do practice_modes and practice_choices."),
                    ("preset", "alttpr_avianart: optional non-empty string", "Default provider preset when the race draft does not supply one. The JSON key is preset, not default_preset. Pooled mode configurations need a concrete preset."),
                    ("mystery_weights_url", "alttpr_dr with source mystery_pool: required URL string", "HTTP(S) URL with a host, pointing to accessible mystery weights YAML. This is its own generation path, not an additional layer of OWR mutual-choice patches. Hosting and the validity of the weights are the organizer's responsibility."),
                    ("permalink", "twwr: required non-empty string", "Settings permalink from the intended TWWR version. Copy the exported settings string, not a tournament webpage URL."),
                ]);
                p : "OWR practice derives its checkboxes directly from choices; it does not need practice_choices. Practice shows only shared options and options scoped to the selected baseline. Checked means enabled; unchecked means disabled. Practice does not combine two players' saved answers and has no Never/Random/Always question or opponent veto. Server-side validation also rejects options from another mode.";
                p : "Pooled qualifiers have their own generator and single-baseline configuration per mode. Copy the chosen mode's base_settings, base_placements and start_inventory into that mode's configuration; do not paste the event's baselines collection or expect the match draft to select it. Entrant mutual choices do not alter pooled seeds. Pool generation, allocation, attempts, redo allowance and score reveal are configured separately on the Qualifiers page.";
                p : "Preroll, spoiler release, game counts and qualifier scoring are separate controls. Editing Seed Config does not regenerate already prepared pool seeds, redistribute assignments, alter submitted results or change a scoring formula. Review existing pools before changing a mode's generator settings.";
            }
            details {
                summary : "7. Generator settings, defaults and troubleshooting";
                p : "The app deliberately accepts generator settings as an object rather than maintaining a second list of every randomizer option. Fields such as goal, mode, shuffle, keyshuffle, mapshuffle, boss_shuffle, enemy_shuffle, flute_mode and aga_randomness are generator settings, not additional root-level app keys. Their accepted strings, booleans and numbers depend on the installed OWR/Doors build. Use that build's settings export and option documentation; this guide's minimal examples are not complete race presets.";
                p : "When translating a YAML export, copy its generator settings into base_settings and preserve their types. Do not paste the entire YAML document, metadata, output instructions or a YAML filename into this JSON editor. Keep fixed placements and starting inventory in their dedicated fields. A successful JSON save verifies the app's supported structure, not whether the generator can roll or patch every combination.";
                : fields(&[
                    ("Option appears at signup but has no seed effect", "check the join key and scope", "Enter Flow key must match a choices entry exactly. Verify neither player vetoed it, its baselines includes the selected mode, it contains a patch, and it was not suppressed or overwritten."),
                    ("Default/off result is wrong", "inspect the baseline", "Never/No means do not apply that optional patch. It does not force the feature off. Put the intended off/default values in every applicable baseline; mandatory features belong there too."),
                    ("Practice option is missing", "inspect provider and mode", "OWR derives options from choices; Door Rando uses practice_choices. A scoped option is hidden outside its modes. A named collection also needs a valid selected practice baseline."),
                    ("A new answer did not change an existing race", "inspect resolution timing", "That race may already have saved choices or a generated seed. Preference edits do not alter its saved decisions or an existing patch file."),
                    ("Named-baseline configuration will not save/roll", "check references and draft", "Remove root baseline fields; use existing IDs in choice scopes, default_baseline and draft presets; ensure the draft assigns every scheduled game and round_modes does not disable it."),
                    ("Generator rejects settings", "check the selected installation", "Verify Seed Gen Type and compare settings and item/location names with that build's exports. Selecting owr_tourney is what selects the tournament installation; a branch field in JSON has no effect."),
                ]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_examples_match_configuration_parsers() {
        for (name, _, requirement) in requirements() {
            serde_json::from_str::<event::enter::Flow>(&json!({"requirements": [requirement]}).to_string())
                .unwrap_or_else(|error| panic!("invalid {name} example: {error}"));
        }
        serde_json::from_value::<event::enter::Flow>(sectioned_flow()).unwrap();
        event::configuration::validate_seed(Some("owr_tourney"), Some(&scoped_seed())).unwrap();
        event::configuration::validate_draft(None, None, Some("owr_tourney"), Some(&scoped_seed()), None).unwrap();
        if let Ok(path) = std::env::var("HTH_TEST_GUIDE_FIXTURE") {
            let css = std::fs::read_to_string("assets/static/common.css").unwrap();
            std::fs::write(path, format!(
                "<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><style>{css}</style></head><body><article><form>{}{}<textarea name=\"seed_config\">unchanged</textarea></form></article></body></html>",
                enter_flow().0, seed_config().0,
            )).unwrap();
        }
    }
}
