use {
    rocket::http::uri::Origin,
    crate::prelude::*,
};

/// A form that only holds a CSRF token
#[derive(FromForm, CsrfForm)]
pub(crate) struct EmptyForm {
    #[field(default = String::new())]
    csrf: String,
}

fn render_form_error(error: &form::Error<'_>) -> RawHtml<String> {
    html! {
        p(class = "error") : error;
    }
}

pub(crate) fn form_field(name: &str, errors: &mut Vec<&form::Error<'_>>, content: impl ToHtml) -> RawHtml<String> {
    let field_errors;
    (field_errors, *errors) = mem::take(errors).into_iter().partition(|error| error.is_for(name));
    html! {
        fieldset(class? = (!field_errors.is_empty()).then_some("error")) {
            @for error in field_errors {
                : render_form_error(error);
            }
            : content;
        }
    }
}

pub(crate) fn form_table_cell(name: &str, errors: &mut Vec<&form::Error<'_>>, content: impl ToHtml) -> RawHtml<String> {
    let field_errors;
    (field_errors, *errors) = mem::take(errors).into_iter().partition(|error| error.is_for(name));
    html! {
        td {
            @for error in field_errors {
                : render_form_error(error);
            }
            : content;
        }
    }
}

/// Returns:
///
/// * Errors to display above the button row
/// * The button itself
pub(crate) fn button_form(uri: Origin<'_>, csrf: Option<&CsrfToken>, errors: Vec<&form::Error<'_>>, submit_text: &str) -> (RawHtml<String>, RawHtml<String>) {
    button_form_ext(uri, csrf, errors, RawHtml(""), submit_text)
}

pub(crate) fn button_form_ext(uri: Origin<'_>, csrf: Option<&CsrfToken>, errors: Vec<&form::Error<'_>>, extra_fields: impl ToHtml, submit_text: &str) -> (RawHtml<String>, RawHtml<String>) {
    button_form_ext_disabled(uri, csrf, errors, extra_fields, submit_text, false)
}

pub(crate) fn button_form_ext_disabled(uri: Origin<'_>, csrf: Option<&CsrfToken>, errors: Vec<&form::Error<'_>>, extra_fields: impl ToHtml, submit_text: &str, disabled: bool) -> (RawHtml<String>, RawHtml<String>) {
    (
        html! {
            @for error in errors {
                : render_form_error(error);
            }
        },
        html! {
            form(action = uri.to_string(), method = "post") {
                : csrf;
                : extra_fields;
                input(type = "submit", value = submit_text, disabled? = disabled.then_some("disabled"));
            }
        },
    )
}

pub(crate) fn full_form(uri: Origin<'_>, csrf: Option<&CsrfToken>, content: impl ToHtml, errors: Vec<&form::Error<'_>>, submit_text: &str) -> RawHtml<String> {
    html! {
        form(action = uri.to_string(), method = "post") {
            : csrf;
            @for error in errors {
                : render_form_error(error);
            }
            : content;
            fieldset {
                input(type = "submit", value = submit_text);
            }
        }
    }
}

pub(crate) fn full_form_confirm(uri: Origin<'_>, csrf: Option<&CsrfToken>, content: impl ToHtml, errors: Vec<&form::Error<'_>>, submit_text: &str, confirm_message: &str) -> RawHtml<String> {
    let onsubmit = format!("return confirm('{}')", confirm_message.replace('\'', "\\'"));
    html! {
        form(action = uri.to_string(), method = "post", onsubmit = onsubmit) {
            : csrf;
            @for error in errors {
                : render_form_error(error);
            }
            : content;
            fieldset {
                input(type = "submit", value = submit_text);
            }
        }
    }
}

pub(crate) fn button_form_confirm(uri: Origin<'_>, csrf: Option<&CsrfToken>, errors: Vec<&form::Error<'_>>, submit_text: &str, confirm_message: &str) -> (RawHtml<String>, RawHtml<String>) {
    let onsubmit = format!("return confirm('{}')", confirm_message.replace('\'', "\\'"));
    (
        html! {
            @for error in errors {
                : render_form_error(error);
            }
        },
        html! {
            form(action = uri.to_string(), method = "post", onsubmit = onsubmit) {
                : csrf;
                input(type = "submit", value = submit_text);
            }
        },
    )
}

