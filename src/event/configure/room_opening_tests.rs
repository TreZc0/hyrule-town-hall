use super::*;
use crate::event::setup::SetupForm;

#[test]
fn room_opening_form_defaults_are_independent() {
    let defaults = Form::<ConfigureForm>::parse("async_start_delay=0").unwrap();
    assert_eq!(defaults.live_room_open_minutes_before, 30);
    assert_eq!(defaults.async_room_open_minutes_before, 30);

    let changed_live =
        Form::<ConfigureForm>::parse("async_start_delay=0&live_room_open_minutes_before=60")
            .unwrap();
    assert_eq!(changed_live.live_room_open_minutes_before, 60);
    assert_eq!(changed_live.async_room_open_minutes_before, 30);
}

#[test]
fn room_opening_both_forms_validate_the_inclusive_range() {
    for field in [
        "live_room_open_minutes_before",
        "async_room_open_minutes_before",
    ] {
        for (value, valid) in [
            ("-1", false),
            ("0", false),
            ("14", false),
            ("15", true),
            ("30", true),
            ("60", true),
            ("61", false),
            ("abc", false),
        ] {
            let input = format!("{field}={value}");
            let configure = Form::<Contextual<'_, ConfigureForm>>::parse(&input).unwrap();
            assert_eq!(
                configure.context.field_errors(field).next().is_none(),
                valid,
                "Configure: {input}"
            );
            let setup = Form::<Contextual<'_, SetupForm>>::parse(&input).unwrap();
            assert_eq!(
                setup.context.field_errors(field).next().is_none(),
                valid,
                "Setup: {input}"
            );
        }
    }
}
