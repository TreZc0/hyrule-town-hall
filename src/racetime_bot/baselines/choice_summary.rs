//! Group saved choice summaries for Racetime chat and Discord async threads.

#[derive(Default)]
struct Groups {
    headings: Vec<String>,
    applied: Vec<String>,
    rules: Vec<String>,
    superseded: Vec<String>,
    not_applied: Vec<String>,
}

impl Groups {
    fn finish(&mut self, messages: &mut Vec<String>, discord: bool) {
        let heading = self.headings.join(" ");
        let prefix = if discord {
            if !self.headings.is_empty() {
                messages.push(self.headings.join("\n"));
            }
            "**Applied:** ".to_owned()
        } else if heading.is_empty() {
            "Applied: ".to_owned()
        } else {
            format!("{heading} Applied: ")
        };
        push_group(
            messages,
            &prefix,
            &if self.applied.is_empty() {
                "none (base settings)".to_owned()
            } else {
                self.applied.join(", ")
            },
            discord,
        );
        for (label, items, separator) in [
            ("Rules: ", &self.rules, "; "),
            ("Superseded: ", &self.superseded, "; "),
            ("Not applied: ", &self.not_applied, ", "),
        ] {
            if !items.is_empty() {
                let label = if discord {
                    format!("**{}** ", label.trim())
                } else {
                    label.to_owned()
                };
                push_group(messages, &label, &items.join(separator), discord);
            }
        }
        *self = Self::default();
    }
}

fn push_group(messages: &mut Vec<String>, prefix: &str, text: &str, discord: bool) {
    if discord {
        messages.push(format!("{prefix}{text}"));
    } else {
        push_messages(messages, prefix, text);
    }
}

/// Repeat the category on continuations and prefer splitting at item/word boundaries.
fn push_messages(messages: &mut Vec<String>, prefix: &str, text: &str) {
    // Custom baseline names can make the heading itself longer than a message.
    if prefix.len() > 450 {
        push_messages(messages, "", prefix.trim());
        push_messages(messages, "", text);
        return;
    }
    let mut rest = text;
    let limit = 900 - prefix.len();
    while rest.len() > limit {
        let mut end = limit;
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        if let Some(boundary) = rest[..end]
            .rfind([',', ';'])
            .filter(|pos| *pos >= end / 2)
            .or_else(|| rest[..end].rfind(' ').filter(|pos| *pos >= end / 2))
        {
            end = boundary + 1;
        }
        messages.push(format!("{prefix}{}", rest[..end].trim_end()));
        rest = rest[end..].trim_start();
    }
    if !rest.is_empty() {
        messages.push(format!("{prefix}{rest}"));
    }
}

/// Read the persisted presentation, so old seeds use the same room formatting.
/// Only the fixed patch-status suffixes are interpreted; rule wording stays intact.
pub(crate) fn messages(text: &str) -> Vec<String> {
    format_summary(text, false)
}

/// Discord preserves line breaks, so keep the groups together as labelled sections.
pub(crate) fn discord_summary(text: &str) -> String {
    format_summary(text, true).join("\n\n")
}

fn format_summary(text: &str, discord: bool) -> Vec<String> {
    let mut messages = Vec::new();
    if !text
        .lines()
        .any(|line| line.starts_with("Choices resolved at ") || line.starts_with("Baseline: "))
    {
        // Older generators store free-form random-choice descriptions.
        if discord {
            return vec![text.to_owned()];
        }
        push_messages(
            &mut messages,
            "",
            &text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
        );
        return messages;
    }
    let mut groups = Groups::default();
    let mut has_outcomes = false;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("Choices resolved at ")
            || line.starts_with("Baseline: ")
            || line.starts_with("If ") && line.ends_with(" is selected:")
        {
            if has_outcomes {
                groups.finish(&mut messages, discord);
                has_outcomes = false;
            }
            groups.headings.push(line.to_owned());
            continue;
        }
        has_outcomes = true;
        if let Some((label, by)) = line
            .rsplit_once(": not applied (superseded by ")
            .and_then(|(label, by)| by.strip_suffix(')').map(|by| (label, by)))
        {
            groups
                .superseded
                .push(format!("{label} (replaced by {by})"));
        } else if let Some(label) = line.strip_suffix(": not applied") {
            groups.not_applied.push(label.to_owned());
        } else if let Some((label, detail)) = line.rsplit_once(": applied").filter(|(_, detail)| {
            detail.is_empty() || detail.starts_with(" (") && detail.ends_with(')')
        }) {
            groups.applied.push(format!("{label}{detail}"));
        } else if line != "No optional patches applied." {
            groups.rules.push(line.to_owned());
        }
    }
    if has_outcomes {
        groups.finish(&mut messages, discord);
    } else if !groups.headings.is_empty() {
        push_group(
            &mut messages,
            "",
            &groups.headings.join(if discord { "\n" } else { " " }),
            discord,
        );
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_groups_are_sections_in_one_summary() {
        assert_eq!(
            discord_summary(
                "Choices resolved at seed reveal:\nBaseline: Crosskeys\nHovering: banned\nBoots: not applied\nSmall Keys: not applied (superseded by Full Keysanity)\nFull Keysanity: applied\nSwordless: applied"
            ),
            "Choices resolved at seed reveal:\nBaseline: Crosskeys\n\n**Applied:** Full Keysanity, Swordless\n\n**Rules:** Hovering: banned\n\n**Superseded:** Small Keys (replaced by Full Keysanity)\n\n**Not applied:** Boots",
        );
    }

    #[test]
    fn discord_preserves_pending_modes_empty_groups_and_legacy_line_breaks() {
        assert_eq!(
            discord_summary(
                "Choices resolved at room opening (mode selection pending):\nIf a is selected:\nBoots: applied\nIf b is selected:\nFlute: not applied"
            ),
            "Choices resolved at room opening (mode selection pending):\nIf a is selected:\n\n**Applied:** Boots\n\nIf b is selected:\n\n**Applied:** none (base settings)\n\n**Not applied:** Flute",
        );
        let legacy = "Final settings - Boots: Enabled\nFlute: Disabled";
        assert_eq!(discord_summary(legacy), legacy);
        assert_eq!(
            discord_summary("Baseline: Standard\nNo optional patches applied."),
            "Baseline: Standard\n\n**Applied:** none (base settings)"
        );
    }

    #[test]
    fn groups_outcomes_with_rules_directly_after_applied() {
        assert_eq!(
            messages(
                "Choices resolved at seed reveal:\nHovering: banned\nBoots: not applied\nSmall Keys: not applied (superseded by Full Keysanity)\nFull Keysanity: applied\nSwordless: applied\nNo delay"
            ),
            [
                "Choices resolved at seed reveal: Applied: Full Keysanity, Swordless",
                "Rules: Hovering: banned; No delay",
                "Superseded: Small Keys (replaced by Full Keysanity)",
                "Not applied: Boots",
            ]
        );
    }

    #[test]
    fn keeps_baseline_and_partial_override_details() {
        assert_eq!(
            messages(
                "Choices resolved at room opening:\nBaseline: Crosskeys\nBoots: applied (goal by Other overridden)\nOther: applied"
            ),
            [
                "Choices resolved at room opening: Baseline: Crosskeys Applied: Boots (goal by Other overridden), Other",
            ]
        );
    }

    #[test]
    fn empty_applied_group_is_explicit_and_other_empty_groups_are_omitted() {
        assert_eq!(
            messages("Baseline: Standard\nNo optional patches applied."),
            ["Baseline: Standard Applied: none (base settings)"]
        );
        assert_eq!(
            messages("Choices resolved at seed reveal:\nFlute: not applied\nHovering: not allowed"),
            [
                "Choices resolved at seed reveal: Applied: none (base settings)",
                "Rules: Hovering: not allowed",
                "Not applied: Flute",
            ]
        );
    }

    #[test]
    fn pending_modes_are_grouped_separately() {
        assert_eq!(
            messages(
                "Baseline: awaiting mode draft\nChoices resolved at room opening (mode selection pending):\nIf a is selected:\nBoots: applied\nHovering: banned\nIf b is selected:\nFlute: not applied"
            ),
            [
                "Baseline: awaiting mode draft Choices resolved at room opening (mode selection pending): If a is selected: Applied: Boots",
                "Rules: Hovering: banned",
                "If b is selected: Applied: none (base settings)",
                "Not applied: Flute",
            ]
        );
    }

    #[test]
    fn legacy_free_form_summary_is_preserved() {
        assert_eq!(
            messages("Final settings - Boots: Enabled, Flute: Disabled"),
            ["Final settings - Boots: Enabled, Flute: Disabled"]
        );
    }

    #[test]
    fn long_unicode_groups_are_bounded_and_keep_the_category() {
        let names = (0..100)
            .map(|n| format!("Option {n} 🦉: not applied"))
            .collect::<Vec<_>>();
        let output = messages(&format!(
            "Choices resolved at seed reveal:\n{}",
            names.join("\n")
        ));
        assert!(output.len() > 2);
        assert!(
            output
                .iter()
                .all(|message| message.len() <= 900 && !message.contains('\n'))
        );
        assert!(
            output[1..]
                .iter()
                .all(|message| message.starts_with("Not applied: "))
        );
        for n in 0..100 {
            assert!(
                output
                    .iter()
                    .any(|message| message.contains(&format!("Option {n} 🦉")))
            );
        }
        let output = messages(&format!("Baseline: {}\nBoots: applied", "🦉".repeat(500)));
        assert!(output.iter().all(|message| message.len() <= 900));
        assert!(output.last().unwrap().contains("Boots"));
    }
}
