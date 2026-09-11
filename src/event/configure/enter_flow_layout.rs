use super::*;

const FORM_ID: &str = "enter-flow-layout";

pub(super) fn section_select(
    flow: &serde_json::Value,
    name: &str,
    selected: &str,
    form_id: Option<&str>,
) -> RawHtml<String> {
    html! {
        select(id = name, name = name, form? = form_id, style = "max-width: calc(100% - 10px);") {
            option(value = "", selected? = selected.is_empty().then_some("")) : "No section";
            @for section in flow.get("sections").and_then(|v| v.as_array()).into_iter().flatten() {
                @let id = section["id"].as_str().unwrap_or_default();
                option(value = id, selected? = (id == selected).then_some("")) {
                    : section["label"].as_str().unwrap_or(id);
                    : format!(" ({id})");
                }
            }
        }
    }
}

pub(super) fn section_fields(flow: &serde_json::Value, ctx: &Context<'_>) -> RawHtml<String> {
    html! {
        h3 : "Sections";
        p : "Edit headings, choose parent sections, and set order numbers. Lower numbers appear first; ties keep their current order. Removing a section leaves its requirements ungrouped and moves its child sections to the top level.";
        @for (i, section) in flow.get("sections").and_then(|v| v.as_array()).into_iter().flatten().enumerate() {
            @let label_name = format!("sections[{i}].label");
            @let parent_name = format!("sections[{i}].parent");
            @let order_name = format!("sections[{i}].order");
            @let remove_name = format!("sections[{i}].remove");
            fieldset(class = "enter-flow-section-editor") {
                legend : section["id"].as_str().unwrap_or_default();
                input(type = "hidden", name = format!("sections[{i}].id"), value = section["id"].as_str().unwrap_or_default(), form = FORM_ID);
                label(for = &label_name) : "Heading (HTML allowed):";
                input(type = "text", id = &label_name, name = &label_name, value = ctx.field_value(label_name.as_str()).unwrap_or(section["label"].as_str().unwrap_or_default()), form = FORM_ID);
                label(for = &parent_name) : "Parent section:";
                : section_select(flow, &parent_name, ctx.field_value(parent_name.as_str()).unwrap_or(section["parent"].as_str().unwrap_or_default()), Some(FORM_ID));
                label(for = &order_name) : "Order:";
                input(type = "number", min = "1", id = &order_name, name = &order_name, value = ctx.field_value(order_name.as_str()).map(str::to_owned).unwrap_or_else(|| (i + 1).to_string()), form = FORM_ID);
                div(class = "enter-flow-section-remove") {
                    input(type = "checkbox", id = &remove_name, name = &remove_name, checked? = ctx.field_value(remove_name.as_str()).is_some().then_some(""), form = FORM_ID);
                    label(for = &remove_name) : "Remove section";
                }
            }
        }
        fieldset(class = "enter-flow-section-editor") {
            legend : "Add section";
            label(for = "new_id") : "Section ID (unique; leave blank to skip):";
            input(type = "text", id = "new_id", name = "new_id", value = ctx.field_value("new_id").unwrap_or_default(), form = FORM_ID);
            label(for = "new_label") : "Heading (HTML allowed):";
            input(type = "text", id = "new_label", name = "new_label", value = ctx.field_value("new_label").unwrap_or_default(), form = FORM_ID);
            label(for = "new_parent") : "Parent section:";
            : section_select(flow, "new_parent", ctx.field_value("new_parent").unwrap_or_default(), Some(FORM_ID));
        }
    }
}

pub(super) fn requirement_fields(
    flow: &serde_json::Value,
    i: usize,
    req: &serde_json::Value,
    ctx: &Context<'_>,
) -> RawHtml<String> {
    let section_name = format!("requirements[{i}].section");
    let order_name = format!("requirements[{i}].order");
    html! {
        div(class = "enter-flow-requirement-position") {
            label(for = &section_name) : "Section:";
            : section_select(flow, &section_name, ctx.field_value(section_name.as_str()).unwrap_or(req["section"].as_str().unwrap_or_default()), Some(FORM_ID));
            label(for = &order_name) : "Order:";
            input(type = "number", min = "1", id = &order_name, name = &order_name, value = ctx.field_value(order_name.as_str()).map(str::to_owned).unwrap_or_else(|| (i + 1).to_string()), form = FORM_ID, style = "width: 5em;");
        }
    }
}

pub(super) fn save_button(
    series: Series,
    event: &str,
    csrf: Option<&CsrfToken>,
    ctx: &Context<'_>,
) -> RawHtml<String> {
    html! {
        form(id = FORM_ID, action = uri!(save(series, event)), method = "post") {
            : csrf;
            @for error in ctx.errors() { p(class = "error") : error; }
            input(type = "submit", value = "Save sections and order");
        }
    }
}

#[derive(FromForm)]
struct SectionForm {
    id: String,
    label: String,
    parent: String,
    #[field(validate = range(1..))]
    order: usize,
    remove: bool,
}

#[derive(FromForm)]
struct RequirementForm {
    section: String,
    #[field(validate = range(1..))]
    order: usize,
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct LayoutForm {
    #[field(default = String::new())]
    csrf: String,
    #[field(default = Vec::new())]
    sections: Vec<SectionForm>,
    #[field(default = Vec::new())]
    requirements: Vec<RequirementForm>,
    new_id: String,
    new_label: String,
    new_parent: String,
}

/// A missing field (e.g. an older edit page) preserves the assignment; an empty
/// selection explicitly removes it.
pub(super) fn assign_section(req: &mut serde_json::Value, section: Option<&str>) {
    if let Some(section) = section {
        if section.is_empty() {
            req.as_object_mut()
                .expect("requirement object")
                .remove("section");
        } else {
            req["section"] = json!(section);
        }
    }
}

pub(super) fn preserve_section(
    req: &serde_json::Value,
    replacement: &mut serde_json::Value,
    section: Option<&str>,
) {
    if let Some(current) = req.get("section") {
        replacement["section"] = current.clone();
    }
    assign_section(replacement, section);
}

fn apply_layout(
    mut flow: serde_json::Value,
    value: &LayoutForm,
) -> Result<serde_json::Value, String> {
    let old_sections = flow
        .get("sections")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut requirements = flow["requirements"].as_array().cloned().unwrap_or_default();
    if old_sections.len() != value.sections.len()
        || requirements.len() != value.requirements.len()
        || old_sections
            .iter()
            .zip(&value.sections)
            .any(|(old, new)| old["id"].as_str() != Some(new.id.as_str()))
    {
        return Err(
            "The flow changed while you were editing. Reload the page and try again.".into(),
        );
    }
    let removed: HashSet<&str> = value
        .sections
        .iter()
        .filter(|s| s.remove)
        .map(|s| s.id.as_str())
        .collect();
    let mut sections = Vec::new();
    for (old, section) in old_sections.into_iter().zip(&value.sections) {
        if !section.remove {
            let mut updated = old;
            updated["label"] = json!(section.label);
            updated
                .as_object_mut()
                .expect("section object")
                .remove("parent");
            if !section.parent.is_empty() && !removed.contains(section.parent.as_str()) {
                updated["parent"] = json!(section.parent);
            }
            sections.push((section.order, updated));
        }
    }
    sections.sort_by_key(|(order, _)| *order);
    let mut sections: Vec<_> = sections.into_iter().map(|(_, section)| section).collect();
    if !value.new_id.trim().is_empty() {
        if value.new_label.trim().is_empty() {
            return Err("Enter a heading for the new section.".into());
        }
        let mut section = json!({"id": value.new_id.trim(), "label": value.new_label});
        if !value.new_parent.is_empty() && !removed.contains(value.new_parent.as_str()) {
            section["parent"] = json!(value.new_parent);
        }
        sections.push(section);
    } else if !value.new_label.trim().is_empty() || !value.new_parent.is_empty() {
        return Err("Enter an ID for the new section.".into());
    }
    for (req, settings) in requirements.iter_mut().zip(&value.requirements) {
        assign_section(
            req,
            Some(if removed.contains(settings.section.as_str()) {
                ""
            } else {
                &settings.section
            }),
        );
    }
    let mut ordered: Vec<_> = requirements.into_iter().zip(&value.requirements).collect();
    ordered.sort_by_key(|(_, settings)| settings.order);
    flow["requirements"] = json!(ordered.into_iter().map(|(req, _)| req).collect::<Vec<_>>());
    flow["sections"] = json!(sections);
    serde_json::from_value::<enter::Flow>(flow.clone())
        .map_err(|e| format!("Invalid configuration: {e}"))?;
    Ok(flow)
}

#[rocket::post("/event/<series>/<event>/configure/enter-flow/layout", data = "<form>")]
pub(crate) async fn save(
    pool: &State<PgPool>,
    me: User,
    uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, LayoutForm>>,
) -> Result<RedirectOrContent, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;
    if data.is_ended()
        || (!data.organizers(&mut transaction).await?.contains(&me) && !me.is_global_admin())
    {
        return Err(StatusOrError::Status(Status::Forbidden));
    }
    let mut form = form.into_inner();
    form.verify(&csrf);
    if let Some(ref value) = form.value {
        let flow = load_flow_json(&mut transaction, series, event).await?;
        match apply_layout(flow, value) {
            Ok(flow) => {
                save_flow_json(&mut transaction, flow, series, event).await?;
                transaction.commit().await?;
                return Ok(RedirectOrContent::Redirect(Redirect::to(uri!(
                    enter_flow_get(series, event)
                ))));
            }
            Err(error) => form.context.push_error(form::Error::validation(error)),
        }
    }
    Ok(RedirectOrContent::Content(
        enter_flow_form(
            transaction,
            Some(me),
            uri,
            csrf.as_ref(),
            data,
            form.context,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow() -> serde_json::Value {
        json!({
            "closes": "2027-01-01T00:00:00Z",
            "sections": [
                {"id": "general", "label": "General"},
                {"id": "advanced", "label": "Advanced", "parent": "general"}
            ],
            "requirements": [
                {"type": "booleanChoice", "key": "one", "label": "One", "section": "general"},
                {"type": "radioChoice", "key": "two", "label": "Two", "section": "advanced"}
            ]
        })
    }

    fn layout() -> LayoutForm {
        Form::parse("sections[0].id=general&sections[0].label=General&sections[0].parent=&sections[0].order=1&sections[1].id=advanced&sections[1].label=Advanced&sections[1].parent=general&sections[1].order=2&requirements[0].section=general&requirements[0].order=1&requirements[1].section=advanced&requirements[1].order=2&new_id=&new_label=&new_parent=")
            .expect("layout form should parse")
    }

    #[test]
    fn option_edits_preserve_change_and_clear_sections() {
        let original = &flow()["requirements"][0];
        for (fields, expected) in [
            ("key=one&label=Edited", Some("general")),
            ("key=one&label=Edited&section=advanced", Some("advanced")),
            ("key=one&label=Edited&section=", None),
        ] {
            let form: EnterFlowEditForm = Form::parse(fields).expect("edit form should parse");
            for kind in ["booleanChoice", "radioChoice"] {
                let mut errors = Vec::new();
                let mut replacement = build_requirement_json(kind, &form, &mut errors);
                preserve_section(original, &mut replacement, form.section.as_deref());
                assert!(errors.is_empty());
                assert_eq!(replacement["label"], "Edited");
                assert_eq!(replacement["section"].as_str(), expected);
                let mut updated = flow();
                updated["requirements"][0] = replacement;
                assert!(serde_json::from_value::<enter::Flow>(updated).is_ok());
            }
        }
    }

    #[test]
    fn layout_round_trip_keeps_assignments_and_deadline() {
        assert_eq!(apply_layout(flow(), &layout()).unwrap(), flow());
    }

    #[test]
    fn layout_swaps_orders_updates_headings_and_assigns_sections() {
        let mut form = layout();
        form.sections[0].order = 2;
        form.sections[1].order = 1;
        form.sections[1].parent.clear();
        form.sections[1].label = "New heading".into();
        form.requirements[0].order = 2;
        form.requirements[1].order = 1;
        form.requirements[1].section = "general".into();
        let updated = apply_layout(flow(), &form).unwrap();
        assert_eq!(updated["sections"][0]["id"], "advanced");
        assert_eq!(updated["sections"][0]["label"], "New heading");
        assert!(updated["sections"][0].get("parent").is_none());
        assert_eq!(updated["requirements"][0]["key"], "two");
        assert_eq!(updated["requirements"][0]["section"], "general");
        assert_eq!(updated["requirements"][1]["section"], "general");
    }

    #[test]
    fn removing_section_ungroups_requirements_and_reparents_children() {
        let mut form = layout();
        form.sections[0].remove = true;
        let updated = apply_layout(flow(), &form).unwrap();
        assert_eq!(updated["sections"].as_array().unwrap().len(), 1);
        assert_eq!(updated["sections"][0]["id"], "advanced");
        assert!(updated["sections"][0].get("parent").is_none());
        assert!(updated["requirements"][0].get("section").is_none());
        assert_eq!(updated["requirements"][1]["section"], "advanced");
    }

    #[test]
    fn adds_nested_section_and_rejects_invalid_hierarchies() {
        let mut form = layout();
        form.new_id = "extra".into();
        form.new_label = "Extra".into();
        form.new_parent = "advanced".into();
        let updated = apply_layout(flow(), &form).unwrap();
        assert_eq!(updated["sections"][2]["parent"], "advanced");
        form.new_id = "general".into();
        assert!(
            apply_layout(flow(), &form)
                .unwrap_err()
                .contains("duplicate section")
        );
        form.new_id = "extra".into();
        form.sections[0].parent = "advanced".into();
        assert!(apply_layout(flow(), &form).unwrap_err().contains("cycle"));
        form.sections[0].parent.clear();
        form.requirements[0].section = "missing".into();
        assert!(
            apply_layout(flow(), &form)
                .unwrap_err()
                .contains("missing section")
        );
    }

    #[test]
    fn ties_are_stable_and_stale_layouts_are_rejected() {
        let mut form = layout();
        form.sections[1].order = 1;
        form.requirements[1].order = 1;
        assert_eq!(apply_layout(flow(), &form).unwrap(), flow());
        form.requirements.pop();
        assert!(apply_layout(flow(), &form).unwrap_err().contains("Reload"));
    }

    #[test]
    fn legacy_empty_flow_can_gain_sections() {
        let mut form: LayoutForm = Form::parse("new_id=first&new_label=First&new_parent=").unwrap();
        let updated = apply_layout(json!({"requirements": []}), &form).unwrap();
        assert_eq!(updated["sections"][0]["id"], "first");
        form.new_id.clear();
        assert!(
            apply_layout(json!({"requirements": []}), &form)
                .unwrap_err()
                .contains("ID")
        );
    }

    #[test]
    fn rejects_zero_order_numbers() {
        assert!(Form::<RequirementForm>::parse("section=&order=0").is_err());
        assert!(Form::<SectionForm>::parse("id=test&label=Test&parent=&order=0").is_err());
    }

    #[test]
    fn renders_layout_controls_with_form_association() {
        let flow = flow();
        let ctx = Context::default();
        let content = html! {
            @for (i, req) in flow["requirements"].as_array().unwrap().iter().enumerate() {
                : requirement_fields(&flow, i, req, &ctx);
            }
            : section_fields(&flow, &ctx);
            : save_button("ohko".parse().unwrap(), "test", None, &ctx);
        };
        assert!(content.0.contains("form=\"enter-flow-layout\""));
        assert!(content.0.contains("value=\"advanced\" selected"));
        if let Ok(path) = std::env::var("HTH_TEST_ENTER_LAYOUT_FIXTURE") {
            let css = std::fs::read_to_string("assets/static/common.css").unwrap();
            std::fs::write(path, format!(
                "<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><style>{css}</style></head><body><article>{}</article></body></html>",
                content.0,
            )).unwrap();
        }
    }
}
