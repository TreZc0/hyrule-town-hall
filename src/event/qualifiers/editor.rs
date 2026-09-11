use super::{help, pooled_qualifiers};
use crate::{event::Series, prelude::*};

pub(super) fn field(
    id: &str,
    name: &str,
    title: &str,
    hint: &str,
    control: RawHtml<String>,
) -> RawHtml<String> {
    html! {
        div(class = "qualifier-field") {
            : help::label(id, name, title);
            : control;
            p(id = format!("{id}-hint"), class = "qualifier-hint") : hint;
        }
    }
}

pub(super) fn configuration(
    series: Series,
    event: &str,
    csrf: Option<&CsrfToken>,
    config: &pooled_qualifiers::Config,
    readiness: &[String],
    ctx: &Context<'_>,
) -> RawHtml<String> {
    let locked = config.settings_locked_at.is_some();
    html! {
        section(id = "pooled-configuration", class = "qualifier-section") {
            p(class = "qualifier-eyebrow") : "1 · Configure the format";
            h2 : "Pooled qualifiers";
            p(class = "qualifier-intro") : "Each entrant completes one result per mode, using a private seed or a linked live race. Configure the format here, define each mode below, then prepare its seeds and live races before opening requests.";
            div(class = if readiness.is_empty() { "qualifier-status qualifier-status-ready" } else { "qualifier-status" }) {
                strong : if readiness.is_empty() { "Readiness checks passed" } else { "Finish setup before opening requests" };
                @if readiness.is_empty() {
                    p : if config.requests_paused { "Requests are paused. Review the schedule, then uncheck Pause new requests and save to activate." } else { "Requests are enabled during the configured request window." };
                } else {
                    ul { @for error in readiness { li : error; } }
                }
                @if locked { p : "Format and scoring settings are locked. Deadlines and the pause control can still be updated. Pausing does not unlock the format."; }
            }
            : full_form(uri!(super::post_pooled_config(series, event)), csrf, html! {
                @for (heading, description, fields) in [
                    ("Pool size & participation", "Define how many modes entrants complete and how many seeds and live races each mode needs.", vec![
                        ("required_mode_count", "Required modes", "Must match the number of enabled mode cards.", config.required_mode_count.to_string(), "1", None, "1"),
                        ("pool_seed_count", "Private seeds per mode", "Shared pool slots, not runs required from each entrant.", config.pool_seed_count.to_string(), "1", None, "1"),
                        ("live_races_per_mode", "Live races per mode", "Linked live qualifiers. Use 0 for async only.", config.live_races_per_mode.to_string(), "0", None, "1"),
                    ]),
                    ("Run limits & retries", "These limits apply across the qualifier workflow, regardless of the mode an entrant selects.", vec![
                        ("async_run_limit_hours", "Async time limit (hours)", "Measured from GO; the submission deadline can shorten it.", config.run_limit().num_hours().to_string(), "1", None, "1"),
                        ("live_entry_close_minutes", "Live entry cutoff (minutes before start)", "Freezes the eligible entrant list before each live race.", (i64::from(config.live_entry_close_lead.days) * 1440 + config.live_entry_close_lead.microseconds / 60_000_000).to_string(), "0", None, "1"),
                        ("retry_limit", "Retries per entrant, across all modes", "0 disables retries. 1 allows one replacement in the event.", config.retry_limit.to_string(), "0", Some("1"), "1"),
                        ("allocation_spread", "Maximum assignment count difference", "Balances the number of entrants assigned to each seed.", config.allocation_spread.to_string(), "1", None, "1"),
                    ]),
                    ("Scoring", "Each physical seed has its own par. The overall qualifier score averages one counted result per required mode.", vec![
                        ("par_finishers", "Fastest finishes used for par", "Scores remain pending until the seed has this many eligible finishes.", config.par_finishers.to_string(), "1", None, "1"),
                        ("score_scale", "Score scale", "Point multiplier. With the standard formula, use 100.", config.score_scale.to_string(), "", None, "any"),
                        ("score_offset", "Score offset", "The starting factor before subtracting time ÷ par.", config.score_offset.to_string(), "", None, "any"),
                        ("score_minimum", "Minimum finish score", "Lower bound for finished runs; other outcomes score zero.", config.score_minimum.to_string(), "", None, "any"),
                        ("score_maximum", "Maximum finish score", "Upper bound for finished runs, including faster-than-par times.", config.score_maximum.to_string(), "", None, "any"),
                    ]),
                ] {
                    section(class = "qualifier-form-section") {
                        h3 : heading;
                        p(class = "qualifier-hint") : description;
                        @if heading == "Scoring" {
                            p(class = "qualifier-formula") { code : "points = scale × (offset − finish time ÷ par)"; br; : "Clamped to the minimum and maximum finish score."; }
                        }
                        div(class = "qualifier-grid") {
                            @for (name, title, hint, value, min, max, step) in fields {
                                : field(name, name, title, hint, html! {
                                    input(id = name, type = "number", name = name, min? = (!min.is_empty()).then_some(min), max? = max, step = step,
                                        value = ctx.field_value(name).unwrap_or(&value), required? = true, readonly? = locked, aria_describedby = format!("{name}-hint"));
                                });
                            }
                        }
                    }
                }
                section(class = "qualifier-form-section") {
                    h3 : "Schedule · all times in UTC";
                    p(class = "qualifier-hint") : "Enter UTC directly; the date picker does not convert local time. Complete all six dates before activation.";
                    p(class = "qualifier-formula") : "Requests open → last request → last GO → submissions close → standings published. Retries close after opening and no later than the last request.";
                    div(class = "qualifier-grid") {
                        @for (name, title, hint, date) in [
                            ("requests_open_at", "Requests open (UTC)", "Entrants can begin requesting private seeds.", config.requests_open_at),
                            ("requests_close_at", "Last request (UTC)", "No new private seeds after this time, including retries.", config.requests_close_at),
                            ("starts_close_at", "Last start / GO (UTC)", "Latest boundary for starting a requested run.", config.starts_close_at),
                            ("submissions_close_at", "Submission deadline (UTC)", "Global cutoff, even if a run’s time limit has not elapsed.", config.submissions_close_at),
                            ("retries_close_at", "Retry request deadline (UTC)", "Set this even when the retry allowance is zero.", config.retries_close_at),
                            ("results_release_at", "Publish standings (UTC)", "At or after submissions close; allow time for verification.", config.results_release_at),
                        ] {
                            @let value = date.map(|date| date.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default();
                            : field(name, name, title, hint, html! {
                                input(id = name, type = "datetime-local", name = name, value = ctx.field_value(name).unwrap_or(&value), aria_describedby = format!("{name}-hint"));
                            });
                        }
                    }
                }
                div(class = "qualifier-activation") {
                    div(class = "qualifier-checkbox") {
                        input(type = "checkbox", name = "requests_paused", id = "requests_paused", checked? = if ctx.field_value("required_mode_count").is_some() { ctx.field_value("requests_paused").is_some_and(|value| value == "on") } else { config.requests_paused });
                        : help::label("requests_paused", "requests_paused", "Pause new requests");
                    }
                    p(class = "qualifier-hint") : "Keep paused while preparing the event. Uncheck and save when all readiness checks pass and you want requests to follow the schedule.";
                }
            }, ctx.errors().collect_vec(), "Save qualifier configuration");
        }
    }
}

pub(super) fn modes(
    series: Series,
    event: &str,
    csrf: Option<&CsrfToken>,
    config: &pooled_qualifiers::Config,
    modes: &[pooled_qualifiers::Mode],
) -> RawHtml<String> {
    let new_mode = pooled_qualifiers::Mode {
        id: 0,
        position: i16::try_from(modes.len() + 1).unwrap_or(1),
        slug: String::new(),
        display_name: String::new(),
        seed_gen_type: String::new(),
        seed_config: json!({}),
        generator_profile: "default".into(),
        settings_fingerprint: String::new(),
        enabled: true,
    };
    html! {
        section(id = "pooled-modes", class = "qualifier-section") {
            p(class = "qualifier-eyebrow") : "2 · Define the modes";
            h2 : "Qualifier modes";
            p(class = "qualifier-intro") : "One card = one complete ruleset, one private seed pool, and one result required from each entrant. For example, Open and Inverted belong in separate cards. These are not signup preferences or individual seed slots.";
            p { strong : format!("{} of {} required modes enabled", modes.iter().filter(|mode| mode.enabled).count(), config.required_mode_count); }
            @if modes.is_empty() { p(class = "qualifier-empty") : "No modes yet. Start by naming your first mode and selecting its generator below. Save it, then add the remaining rulesets."; }
            @for mode in modes.iter().chain(iter::once(&new_mode)) {
                @let is_new = mode.id == 0;
                details(class = "qualifier-mode-card", open? = is_new) {
                    summary {
                        span(class = "qualifier-mode-title") : if is_new { "Add a qualifier mode".to_owned() } else { format!("{}. {}", mode.position, mode.display_name) };
                        @if !is_new { span(class = "qualifier-badge") : if mode.enabled { "Enabled" } else { "Disabled" }; }
                    }
                    p(class = "qualifier-hint") : if is_new { "Give the ruleset a recognizable name, choose its generator, and enter its baseline settings. Each card saves separately." } else { "Edit this mode’s name, order and generation settings. Save this card to apply its changes." };
                    @if config.settings_locked_at.is_some() || !config.requests_paused {
                        p(class = "qualifier-notice") : "Mode settings are protected while requests are enabled or after settings lock. Existing modes can still be renamed or reordered; adding modes or changing their rulesets is blocked.";
                    } else if !is_new {
                        p(class = "qualifier-hint") : "If seed material already exists, only the name and display order can change. Finalize the ruleset before generating its pool.";
                    }
                    : full_form(uri!(super::post_pooled_mode(series, event)), csrf, html! {
                        input(type = "hidden", name = "mode_id", value = mode.id);
                        div(class = "qualifier-grid") {
                            @for (name, title, hint, value, placeholder) in [
                                ("display_name", "Mode name", "Shown to entrants, e.g. Inverted + Keysanity.", mode.display_name.as_str(), "Inverted + Keysanity"),
                                ("slug", "Stable mode ID", "A distinct identifier, e.g. inverted-keysanity.", mode.slug.as_str(), "inverted-keysanity"),
                            ] {
                                @let id = format!("mode-{}-{name}", mode.id);
                                : field(&id, name, title, hint, html! {
                                    input(id = &id, type = "text", name = name, value = value, placeholder = placeholder, required? = true, aria_describedby = format!("{id}-hint"));
                                });
                            }
                            @let id = format!("mode-{}-position", mode.id);
                            : field(&id, "position", "Display order", "1 appears first, followed by 2, 3, and so on.", html! {
                                input(id = &id, type = "number", name = "position", min = "1", value = mode.position, required? = true, aria_describedby = format!("{id}-hint"));
                            });
                            @let id = format!("mode-{}-seed_gen_type", mode.id);
                            : field(&id, "seed_gen_type", "Seed generator", "This mode’s generator, independent of the event’s main settings.", html! {
                                select(id = &id, name = "seed_gen_type", required? = true, aria_describedby = format!("{id}-hint")) {
                                    option(value = "", selected? = mode.seed_gen_type.is_empty(), disabled? = true) : "Choose a generator…";
                                    @for (value, title) in [(if mode.seed_gen_type == "owr" { "owr" } else { "owr_tourney" }, "OWR · tournament build"), ("alttpr_dr", "ALTTPR Door Rando"), ("alttpr_avianart", "ALTTPR Avianart"), ("twwr", "The Wind Waker Randomizer")] {
                                        option(value = value, selected? = mode.seed_gen_type == value) : title;
                                    }
                                    @if !mode.seed_gen_type.is_empty() && !["owr", "owr_tourney", "alttpr_dr", "alttpr_avianart", "twwr"].contains(&mode.seed_gen_type.as_str()) {
                                        option(value = &mode.seed_gen_type, selected? = true) : format!("{} (check pooled support)", mode.seed_gen_type);
                                    }
                                }
                            });
                        }
                        @let id = format!("mode-{}-seed_config", mode.id);
                        : field(&id, "seed_config", "Baseline settings JSON", "One fixed ruleset for this mode. Open (?) for generator-specific examples and restrictions.", html! {
                            textarea(id = &id, name = "seed_config", rows = "10", class = "qualifier-code", required? = true, spellcheck = "false", aria_describedby = format!("{id}-hint")) : serde_json::to_string_pretty(&mode.seed_config).expect("stored JSON value");
                        });
                        @let id = format!("mode-{}-generator_profile", mode.id);
                        : field(&id, "generator_profile", "Generator profile", "Automatic deployment profile; OWR pools use the tournament installation.", html! {
                            select(id = &id, name = "generator_profile", aria_describedby = format!("{id}-hint")) {
                                option(value = "default", selected? = mode.generator_profile == "default") : "Default (automatic)";
                                @if mode.generator_profile != "default" { option(value = &mode.generator_profile, selected? = true) : format!("{} (unsupported profile)", mode.generator_profile); }
                            }
                        });
                        div(class = "qualifier-checkbox") {
                            @let id = format!("mode-{}-enabled", mode.id);
                            input(id = &id, type = "checkbox", name = "enabled", checked? = mode.enabled);
                            : help::label(&id, "enabled", "Enable this mode");
                        }
                        p(class = "qualifier-hint") : "Enabled modes count toward the required total. Saving a mode does not generate its seeds.";
                    }, Vec::new(), if is_new { "Add qualifier mode" } else { "Save mode changes" });
                }
            }
        }
    }
}

pub(super) fn seed_controls(
    series: Series,
    event: &str,
    csrf: Option<&CsrfToken>,
    mode: &pooled_qualifiers::Mode,
    config: &pooled_qualifiers::Config,
) -> RawHtml<String> {
    html! {
        div(class = "qualifier-pool-actions") {
            : full_form(uri!(super::post_pooled_generate(series, event)), csrf, html! {
                input(type = "hidden", name = "mode_id", value = mode.id);
                input(type = "hidden", name = "retry_failed", value = "false");
                h4 : "Prepare the private pool";
                p(class = "qualifier-hint") : format!("Queue missing slots for this mode’s {}-seed pool. Generation runs in the background; reload to check progress.", config.pool_seed_count);
            }, Vec::new(), "Generate missing slots");
            details(class = "qualifier-disclosure") {
                summary : "Generate or retry a specific slot";
                : full_form(uri!(super::post_pooled_generate(series, event)), csrf, html! {
                    input(type = "hidden", name = "mode_id", value = mode.id);
                    input(type = "hidden", name = "retry_failed", value = "true");
                    @let id = format!("generate-{}-pool_position", mode.id);
                    : field(&id, "pool_position", "Private pool slot", "Creates a missing slot or retries a failed, unused slot. Ready seeds are preserved.", html! {
                        input(id = &id, type = "number", name = "pool_position", min = "1", max = config.pool_seed_count, required? = true, aria_describedby = format!("{id}-hint"));
                    });
                }, Vec::new(), "Generate / retry this slot");
            }
        }
    }
}

pub(super) fn import_form(
    series: Series,
    event: &str,
    csrf: Option<&CsrfToken>,
    mode: &pooled_qualifiers::Mode,
    config: &pooled_qualifiers::Config,
) -> RawHtml<String> {
    html! {
        details(class = "qualifier-disclosure") {
            summary : "Import an existing seed · advanced";
            p(class = "qualifier-hint") : "Use this only for an already generated seed with canonical delivery data. Requests must be paused and the target slot unused. Normal setup can use Generate missing slots above.";
            : full_form(uri!(super::post_pooled_seed(series, event)), csrf, html! {
                input(type = "hidden", name = "mode_id", value = mode.id);
                @let id = format!("import-{}-pool_position", mode.id);
                : field(&id, "pool_position", "Destination pool slot", "The unused private slot that will hold this seed.", html! {
                    input(id = &id, type = "number", name = "pool_position", min = "1", max = config.pool_seed_count, required? = true, aria_describedby = format!("{id}-hint"));
                });
                @let id = format!("import-{}-seed_data", mode.id);
                : field(&id, "seed_data", "Canonical seed delivery JSON", "Actual generated output, including delivery identifiers. This is not the mode’s settings JSON.", html! {
                    textarea(id = &id, name = "seed_data", rows = "8", class = "qualifier-code", required? = true, spellcheck = "false", aria_describedby = format!("{id}-hint"));
                });
                div(class = "qualifier-checkbox") {
                    @let id = format!("import-{}-attest_settings", mode.id);
                    input(id = &id, type = "checkbox", name = "attest_settings", required? = true);
                    : help::label(&id, "attest_settings", "I verified this seed’s baseline settings and deployed generator build");
                }
            }, Vec::new(), "Import into unused slot");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuchiki::traits::TendrilSink as _;

    #[test]
    fn qualifier_editor_preserves_fields_and_unique_help_targets() {
        let mut config = pooled_qualifiers::tests::config();
        config.requests_paused = true;
        let modes_data = [
            pooled_qualifiers::Mode {
                id: 7,
                position: 1,
                slug: "inverted".into(),
                display_name: "Inverted <test>".into(),
                seed_gen_type: "owr_tourney".into(),
                seed_config: json!({"base_settings": {"mode": "inverted"}}),
                generator_profile: "default".into(),
                settings_fingerprint: String::new(),
                enabled: true,
            },
            pooled_qualifiers::Mode {
                id: 8,
                position: 2,
                slug: "legacy".into(),
                display_name: "Legacy mode".into(),
                seed_gen_type: "legacy_generator".into(),
                seed_config: json!({}),
                generator_profile: "legacy_profile".into(),
                settings_fingerprint: String::new(),
                enabled: false,
            },
        ];
        let rendered = html! {
            article(class = "qualifier-admin") {
                : configuration(Series::Standard, "test", None, &config, &["Expected 3 enabled modes, found 1.".into()], &Context::default());
                : modes(Series::Standard, "test", None, &config, &modes_data);
                section(class = "qualifier-pool-card") {
                    h2 : "Inverted seed pool";
                    : seed_controls(Series::Standard, "test", None, &modes_data[0], &config);
                    : import_form(Series::Standard, "test", None, &modes_data[0], &config);
                }
            }
        }.0;
        let document = kuchiki::parse_html().one(rendered.clone());
        let mut ids = HashSet::new();
        for node in document.select("[id]").unwrap() {
            let attributes = node.attributes.borrow();
            assert!(
                ids.insert(attributes.get("id").unwrap().to_owned()),
                "duplicate ID"
            );
        }
        for node in document.select("label[for]").unwrap() {
            assert!(ids.contains(node.attributes.borrow().get("for").unwrap()));
        }
        for node in document.select(".setting-help-trigger").unwrap() {
            let attributes = node.attributes.borrow();
            assert_eq!(attributes.get("type"), Some("button"));
            assert!(ids.contains(attributes.get("popovertarget").unwrap()));
        }
        assert_eq!(document.select("form").unwrap().count(), 7);
        assert!(rendered.contains("Inverted &lt;test&gt;"));
        assert!(rendered.contains("Named baselines maps are not supported"));
        for (selector, expected) in [
            ("#required_mode_count", "3"),
            ("#mode-7-position", "1"),
            ("#mode-7-slug", "inverted"),
            ("#mode-7-seed_gen_type option[selected]", "owr_tourney"),
            ("#mode-8-seed_gen_type option[selected]", "legacy_generator"),
            (
                "#mode-8-generator_profile option[selected]",
                "legacy_profile",
            ),
        ] {
            assert_eq!(
                document
                    .select_first(selector)
                    .unwrap()
                    .attributes
                    .borrow()
                    .get("value"),
                Some(expected)
            );
        }
        config.settings_locked_at = Some(Utc::now());
        let locked = configuration(
            Series::Standard,
            "test",
            None,
            &config,
            &[],
            &Context::default(),
        )
        .0;
        let locked_document = kuchiki::parse_html().one(locked);
        assert_eq!(
            locked_document
                .select("input[type=number][readonly]")
                .unwrap()
                .count(),
            12
        );
        assert_eq!(
            locked_document
                .select("input[type=datetime-local][readonly]")
                .unwrap()
                .count(),
            0
        );
        if let Ok(path) = std::env::var("HTH_TEST_BROWSER_FIXTURE") {
            let css = std::fs::read_to_string("assets/static/common.css").unwrap();
            let script = std::fs::read_to_string("assets/static/setting-help.js").unwrap();
            std::fs::write(path, format!("<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>{css}</style></head><body><main>{rendered}</main><script>{script}</script></body></html>")).unwrap();
        }
    }
}
