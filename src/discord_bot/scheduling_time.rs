use chrono::{
    DateTime, Datelike as _, FixedOffset, NaiveDate, NaiveTime, Offset as _, TimeZone as _, Utc,
    Weekday,
};
use chrono_tz::{America, Europe, Tz};
use lazy_regex::{regex, regex_captures, regex_is_match};

pub(super) fn parse_timestamp(timestamp: &str) -> Option<DateTime<Utc>> {
    regex_captures!("^<t:(-?[0-9]+)(?::[stTdDfFR])?>$", timestamp)
        .and_then(|(_, timestamp)| timestamp.parse().ok())
        .and_then(|timestamp| Utc.timestamp_opt(timestamp, 0).single())
}

fn tz_from_abbr(abbr: &str) -> Option<Tz> {
    match abbr {
        "UTC" | "GMT" => Some(chrono_tz::UTC),
        "ET" | "EST" | "EDT" | "EASTERN" => Some(America::New_York),
        "CT" | "CST" | "CDT" | "CENTRAL" => Some(America::Chicago),
        "MT" | "MST" | "MDT" | "MOUNTAIN" => Some(America::Denver),
        "PT" | "PST" | "PDT" | "PACIFIC" => Some(America::Los_Angeles),
        "CET" | "CEST" => Some(Europe::Paris),
        "WET" | "WEST" => Some(Europe::Lisbon),
        "EET" | "EEST" => Some(Europe::Helsinki),
        "BST" => Some(Europe::London),
        "IST" => Some(chrono_tz::Asia::Kolkata),
        "JST" => Some(chrono_tz::Asia::Tokyo),
        "AEST" | "AEDT" | "AET" => Some(chrono_tz::Australia::Sydney),
        "NZST" | "NZDT" | "NZT" => Some(chrono_tz::Pacific::Auckland),
        _ => None,
    }
}

pub(super) fn timezone_utc_offset(tz: Tz, at: DateTime<Utc>) -> String {
    let seconds = at.with_timezone(&tz).offset().fix().local_minus_utc();
    let sign = if seconds < 0 { '-' } else { '+' };
    let hours = seconds.abs() / 3600;
    let minutes = (seconds.abs() % 3600) / 60;
    if minutes == 0 {
        format!("UTC{sign}{hours}")
    } else {
        format!("UTC{sign}{hours}:{minutes:02}")
    }
}

/// Parse a whole clock token. Never let interim interpret an unchecked hour:
/// it treats 930 PM as 942 hours and carries the overflow into the date.
fn parse_clock(s: &str) -> Option<NaiveTime> {
    let (_, hour, minute, second, fraction, meridiem) = regex_captures!(
        r"^([0-9]+)(?:[:.]([0-9]{1,2})(?::([0-9]{1,2})(?:\.([0-9]{1,9}))?)?)?\s*([ap]m)?$",
        s
    )?;
    let compact = minute.is_empty() && !meridiem.is_empty() && (3..=4).contains(&hour.len());
    if hour.len() > 2 && !compact {
        return None;
    }
    let mut hour = hour.parse::<u32>().ok()?;
    let minute = if compact {
        let minute = hour % 100;
        hour /= 100;
        minute
    } else if minute.is_empty() {
        0
    } else {
        minute.parse().ok()?
    };
    if !meridiem.is_empty() {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour = hour % 12 + if meridiem == "pm" { 12 } else { 0 };
    }
    let second = if second.is_empty() {
        0
    } else {
        second.parse().ok()?
    };
    let nanos = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()? * 10u32.pow(9 - fraction.len() as u32)
    };
    NaiveTime::from_hms_nano_opt(hour, minute, second, nanos)
}

fn weekday(s: &str) -> Option<Weekday> {
    match s {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturda" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Restrict the grammar before calling interim, which does not always consume
/// its entire input. Calendar dates are validated by chrono without truncation.
fn canonical_date(s: &str, year: i32) -> Option<String> {
    if let Some((_, year, month, day)) =
        regex_captures!(r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})$", s)
    {
        return Some(
            NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)?
                .to_string(),
        );
    }
    if let Some((_, month, day, explicit_year)) =
        regex_captures!(r"^([0-9]{1,2})/([0-9]{1,2})(?:/([0-9]{2}|[0-9]{4}))?$", s)
    {
        let year = if explicit_year.is_empty() {
            year
        } else {
            let year = explicit_year.parse::<i32>().ok()?;
            if explicit_year.len() == 2 {
                year + if year <= 40 { 2000 } else { 1900 }
            } else {
                year
            }
        };
        return Some(
            NaiveDate::from_ymd_opt(year, month.parse().ok()?, day.parse().ok()?)?.to_string(),
        );
    }
    // Named calendar dates: September 9th [2026], or 9 September [2026].
    let named_date = regex_captures!(r"^([a-z]+)(?: ([0-9]{1,2})(?: ([0-9]{4}))?)?$", s)
        .map(|(_, month, day, year)| (month, day, year))
        .or_else(|| {
            regex_captures!(r"^([0-9]{1,2}) ([a-z]+)(?: ([0-9]{4}))?$", s)
                .map(|(_, day, month, year)| (month, day, year))
        });
    if let Some((month, day, explicit_year)) = named_date {
        let month = match month {
            "jan" | "january" => Some(1),
            "feb" | "february" => Some(2),
            "mar" | "march" => Some(3),
            "apr" | "april" => Some(4),
            "may" => Some(5),
            "jun" | "june" => Some(6),
            "jul" | "july" => Some(7),
            "aug" | "august" => Some(8),
            "sep" | "sept" | "september" => Some(9),
            "oct" | "october" => Some(10),
            "nov" | "november" => Some(11),
            "dec" | "december" => Some(12),
            _ => None,
        };
        if let Some(month) = month {
            let day = if day.is_empty() { 1 } else { day.parse().ok()? };
            let year = if explicit_year.is_empty() {
                year
            } else {
                explicit_year.parse().ok()?
            };
            return Some(NaiveDate::from_ymd_opt(year, month, day)?.to_string());
        }
    }
    if matches!(s, "today" | "tomorrow" | "yesterday" | "now") {
        return Some(s.to_owned());
    }
    let (direction, name) = s
        .split_once(' ')
        .filter(|(direction, _)| matches!(*direction, "next" | "last" | "this"))
        .unwrap_or(("", s));
    if let Some(day) = weekday(name) {
        return Some(
            format!("{direction} {}", day.to_string().to_lowercase())
                .trim()
                .to_owned(),
        );
    }
    if !direction.is_empty()
        && regex_is_match!(
            r"^(?:[a-z]+(?: [0-9]{1,2})?|[0-9]{1,2} [a-z]+|[0-9]{1,2}/[0-9]{1,2})$",
            name
        )
    {
        // Keep interim's existing next/last semantics for recurring calendar
        // dates, after validating the month and day ourselves.
        if let Some(date) = canonical_date(name, year)
            .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
        {
            return Some(format!("{direction} {}/{}", date.month(), date.day()));
        }
    }
    if regex_is_match!(
        r"^(?:(?:next|last|this) |[-+]?[0-9]{1,4} ?)(?:s|sec(?:ond)?s?|m|min(?:ute)?s?|h|hours?|d|days?|w|weeks?|months?|y|years?)(?: ago)?$",
        s
    ) {
        return Some(s.to_owned());
    }
    None
}

fn parse_datetime_at(s: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    // Match complete numeric tokens, including invalid values, so malformed
    // clocks are rejected instead of partially normalized (e.g. 9:05 PM).
    let mut clocks =
        regex!(r"[0-9]+(?:[:.][0-9]+(?::[0-9]+(?:\.[0-9]+)?)?(?:\s*[ap]m)?|\s*[ap]m)").find_iter(s);
    let clock = clocks.next();
    if clocks.next().is_some() {
        return None;
    }
    let time = match clock {
        Some(clock) => Some(parse_clock(clock.as_str())?),
        None => None,
    };
    let date = if let Some(clock) = clock {
        let before = &s[..clock.start()];
        // ISO's T separator is otherwise indistinguishable from an unknown word.
        let before = if regex_is_match!(r"^[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}t$", before) {
            &before[..before.len() - 1]
        } else {
            before
        };
        format!("{before} {}", &s[clock.end()..])
    } else {
        s.to_owned()
    };
    let date = date.replace(',', " ");
    let date = lazy_regex::regex_replace_all!(r"\b([0-9]{1,2})(?:st|nd|rd|th)\b", &date, "$1");
    let date = date.split_whitespace().collect::<Vec<_>>().join(" ");
    let date = date.strip_prefix("on ").unwrap_or(&date);
    let date = date.strip_suffix(" at").unwrap_or(date);
    let date = date.strip_prefix("at ").unwrap_or(date);
    let date = if date.is_empty() || date == "at" {
        time?;
        "today"
    } else {
        date
    };
    let (expected_weekday, date) = match date.split_once(' ') {
        Some((day, date)) if weekday(day).is_some() => (weekday(day), date),
        _ => (None, date),
    };
    let date = canonical_date(date, now.year())?;
    if expected_weekday.is_some() && !regex_is_match!(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$", &date) {
        return None;
    }
    // Sub-day durations already specify a time; interim would silently ignore
    // an additional clock. Reject such contradictory expressions.
    if time.is_some()
        && interim::parse_duration(&date)
            .is_ok_and(|duration| matches!(duration, interim::Interval::Seconds(_)))
    {
        return None;
    }
    let input = match time {
        Some(time) => format!("{date} {}", time.format("%H:%M:%S")),
        None => date,
    };
    let mut result = interim::parse_date_string(&input, now, interim::Dialect::Us).ok()?;
    if let Some(time) = time {
        // interim reads fractional digits as microseconds without scaling.
        // Keep our validated clock, using interim only to resolve the date.
        result = result.date_naive().and_time(time).and_utc();
    }
    if expected_weekday.is_some_and(|day| day != result.weekday()) {
        return None;
    }
    Some(result)
}

pub(super) struct ParsedNaturalLanguageTimestamp {
    pub(super) start: DateTime<Utc>,
    pub(super) timezone: Option<Tz>,
    pub(super) used_default_timezone: bool,
}

pub(super) fn parse_natural_language_timestamp(
    s: &str,
    default_timezone: Option<Tz>,
) -> Option<ParsedNaturalLanguageTimestamp> {
    parse_natural_language_timestamp_at(s, default_timezone, Utc::now())
}

fn parse_natural_language_timestamp_at(
    s: &str,
    default_timezone: Option<Tz>,
    now: DateTime<Utc>,
) -> Option<ParsedNaturalLanguageTimestamp> {
    let input = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if input.is_empty() {
        return None;
    }
    let mut s = input.as_str();
    let mut explicit_timezone = None;
    if let Some((_, date, name)) = regex_captures!(
        r"^(.*\S)\s+(eastern|central|mountain|pacific)(?: (?:standard|daylight))?(?: time)?$"i,
        s
    ) {
        explicit_timezone = tz_from_abbr(&name.to_ascii_uppercase());
        s = date;
    } else if let Some((date, name)) = s.rsplit_once(' ') {
        explicit_timezone =
            tz_from_abbr(&name.to_ascii_uppercase()).or_else(|| name.parse::<Tz>().ok());
        if explicit_timezone.is_some() {
            s = date;
        }
    }
    let s = s.to_lowercase();
    // Explicit numeric offsets override the profile zone, just as named zones do.
    if let Some((_, date, offset)) =
        regex_captures!(r"^(.*[0-9](?:\s*[ap]m)?)\s*(z|[+-][0-9]{2}:?[0-9]{2})$", &s)
    {
        if explicit_timezone.is_some() {
            return None;
        }
        let offset = if offset == "z" {
            FixedOffset::east_opt(0)?
        } else {
            let digits = offset[1..].replace(':', "");
            let hours = digits[..2].parse::<i32>().ok()?;
            let minutes = digits[2..].parse::<i32>().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            FixedOffset::east_opt(
                (hours * 3600 + minutes * 60) * if offset.starts_with('-') { -1 } else { 1 },
            )?
        };
        let local_now = now.with_timezone(&offset).naive_local().and_utc();
        let parsed = parse_datetime_at(date.trim(), local_now)?;
        return Some(ParsedNaturalLanguageTimestamp {
            start: offset
                .from_local_datetime(&parsed.naive_utc())
                .single()?
                .to_utc(),
            timezone: None,
            used_default_timezone: false,
        });
    }
    let timezone = explicit_timezone.or(default_timezone);
    let local_now = timezone.map_or(now, |tz| now.with_timezone(&tz).naive_local().and_utc());
    let parsed = parse_datetime_at(&s, local_now)?;
    let start = if let Some(tz) = timezone {
        // Preserve the existing policy for repeated DST times; nonexistent local
        // times still fail instead of being shifted into a different hour.
        tz.from_local_datetime(&parsed.naive_utc())
            .latest()?
            .to_utc()
    } else {
        parsed
    };
    Some(ParsedNaturalLanguageTimestamp {
        start,
        timezone,
        used_default_timezone: explicit_timezone.is_none() && default_timezone.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0).unwrap()
    }

    #[test]
    fn discord_timestamp_accepts_hammertime_short_suffix() {
        assert_eq!(
            parse_timestamp("<t:1784005200:s>"),
            Some(Utc.with_ymd_and_hms(2026, 7, 14, 5, 0, 0).unwrap()),
        );
    }

    #[test]
    fn natural_language_normalizes_eastern_standard_abbreviation_in_summer() {
        let parsed =
            parse_natural_language_timestamp_at("friday 3pm EST", None, fixed_now()).unwrap();

        assert_eq!(parsed.timezone, Some(America::New_York));
        assert_eq!(
            parsed.start,
            Utc.with_ymd_and_hms(2026, 7, 10, 19, 0, 0).unwrap()
        );
        let tz = parsed.timezone.unwrap();
        assert_eq!(
            tz.from_utc_datetime(&parsed.start.naive_utc())
                .format("%Z")
                .to_string(),
            "EDT"
        );
        assert_eq!(timezone_utc_offset(tz, parsed.start), "UTC-4");
    }

    #[test]
    fn natural_language_handles_bare_noon_with_pacific_abbreviation() {
        let parsed =
            parse_natural_language_timestamp_at("friday 12pm PST", None, fixed_now()).unwrap();

        assert_eq!(parsed.timezone, Some(America::Los_Angeles));
        assert_eq!(
            parsed.start,
            Utc.with_ymd_and_hms(2026, 7, 10, 19, 0, 0).unwrap()
        );
        let tz = parsed.timezone.unwrap();
        assert_eq!(
            tz.from_utc_datetime(&parsed.start.naive_utc())
                .format("%Z")
                .to_string(),
            "PDT"
        );
        assert_eq!(timezone_utc_offset(tz, parsed.start), "UTC-7");
    }

    #[test]
    fn natural_language_handles_bare_noon_without_rolling_to_next_day() {
        let parsed =
            parse_natural_language_timestamp_at("Sunday 12pm EDT", None, fixed_now()).unwrap();

        assert_eq!(parsed.timezone, Some(America::New_York));
        assert_eq!(
            parsed.start,
            Utc.with_ymd_and_hms(2026, 7, 12, 16, 0, 0).unwrap()
        );
    }

    #[test]
    fn natural_language_handles_bare_midnight() {
        let parsed =
            parse_natural_language_timestamp_at("friday 12am PT", None, fixed_now()).unwrap();

        assert_eq!(parsed.timezone, Some(America::Los_Angeles));
        assert_eq!(
            parsed.start,
            Utc.with_ymd_and_hms(2026, 7, 10, 7, 0, 0).unwrap()
        );
    }

    #[test]
    fn natural_language_handles_default_timezone_with_bare_meridiem() {
        let parsed = parse_natural_language_timestamp_at(
            "friday 12 pm",
            Some(America::Los_Angeles),
            fixed_now(),
        )
        .unwrap();

        assert_eq!(parsed.timezone, Some(America::Los_Angeles));
        assert!(parsed.used_default_timezone);
        assert_eq!(
            parsed.start,
            Utc.with_ymd_and_hms(2026, 7, 10, 19, 0, 0).unwrap()
        );
    }

    fn september_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 11, 16, 0, 0).unwrap()
    }

    #[test]
    fn natural_language_reported_formats() {
        for (input, zone, expected) in [
            (
                "today 930 PM EDT",
                Some(America::New_York),
                "2026-09-12T01:30:00Z",
            ),
            ("7pm Saturday", None, "2026-09-12T19:00:00Z"),
            (
                "Saturday 7pm central time",
                Some(America::Chicago),
                "2026-09-13T00:00:00Z",
            ),
            (
                "20:30 Wednesday September 9th",
                None,
                "2026-09-09T20:30:00Z",
            ),
            (
                "Saturday, September 5, 9:00AM MDT",
                Some(America::Denver),
                "2026-09-05T15:00:00Z",
            ),
            (
                "Saturda 9 am mountain",
                Some(America::Denver),
                "2026-09-12T15:00:00Z",
            ),
            (
                "sunday 2pm eastern",
                Some(America::New_York),
                "2026-09-13T18:00:00Z",
            ),
            (
                "thursday at 7pm EDT",
                Some(America::New_York),
                "2026-09-17T23:00:00Z",
            ),
            (
                "Thursday 7pm Eastern",
                Some(America::New_York),
                "2026-09-17T23:00:00Z",
            ),
        ] {
            let parsed = parse_natural_language_timestamp_at(input, None, september_now())
                .unwrap_or_else(|| panic!("failed to parse {input:?}"));
            assert_eq!(
                parsed.start,
                expected.parse::<DateTime<Utc>>().unwrap(),
                "{input}"
            );
            assert_eq!(parsed.timezone, zone, "{input}");
            assert!(!parsed.used_default_timezone, "{input}");
        }
    }

    #[test]
    fn natural_language_preserves_minutes_and_meridiem() {
        for (clock, expected) in [
            ("9:05 PM", "21:05:00"),
            ("9:12pm", "21:12:00"),
            ("9:01 AM", "09:01:00"),
            ("9:30PM", "21:30:00"),
            ("9.30pm", "21:30:00"),
            ("9:05:12pm", "21:05:12"),
            ("12am", "00:00:00"),
            ("12pm", "12:00:00"),
            ("1200 AM", "00:00:00"),
            ("1205pm", "12:05:00"),
            ("930pm", "21:30:00"),
            ("0930 PM", "21:30:00"),
            ("00:05", "00:05:00"),
            ("23:59:59", "23:59:59"),
        ] {
            let input = format!("today {clock} UTC");
            let parsed =
                parse_natural_language_timestamp_at(&input, None, september_now()).unwrap();
            assert_eq!(
                parsed.start.date_naive(),
                september_now().date_naive(),
                "{input}"
            );
            assert_eq!(parsed.start.time().to_string(), expected, "{input}");
        }
    }

    #[test]
    fn natural_language_rejects_invalid_or_unconsumed_input() {
        for input in [
            "today 2500 PM EDT",
            "today 1360pm",
            "today 960pm",
            "today 0am",
            "today 13pm",
            "today 24:00",
            "today 942:00",
            "today 9:60 PM",
            "today 9:05:60 PM",
            "today 99999999999999999pm",
            "today 9:05 PM garbage",
            "today 9pm 10pm",
            "today 9am tomorrow",
            "today 9:05 PM EDT garbage",
            "today 9:05 PM unknown",
            "Friday September 5 9am MDT",
            "February 30 9am UTC",
            "2026-13-01 9am UTC",
            "September 257 9am",
            "Wednesday tomorrow 9am",
            "today 9pm +04:60",
            "today 9pm +24:00",
            "today 9pm +0400 EDT",
            "3 hours 9pm",
            "",
            "   ",
        ] {
            for default in [None, Some(America::New_York)] {
                assert!(
                    parse_natural_language_timestamp_at(input, default, september_now()).is_none(),
                    "accepted {input:?}"
                );
            }
        }
    }

    #[test]
    fn natural_language_normalizes_spaces_and_calendar_dates() {
        for input in [
            " Saturday,\u{a0}September\u{202f}5th,\t2026 at 9:00AM MDT ",
            "9am on Saturday September 5 2026 MDT",
            "September 5, 2026 9am MDT",
            "5 September 2026 9am MDT",
            "2026-09-05T09:00:00 America/Denver",
            "9/5/26 9am MDT",
            "9/5 9am MDT",
        ] {
            let parsed = parse_natural_language_timestamp_at(input, None, september_now())
                .unwrap_or_else(|| panic!("{input}"));
            assert_eq!(
                parsed.start,
                Utc.with_ymd_and_hms(2026, 9, 5, 15, 0, 0).unwrap(),
                "{input}"
            );
        }
    }

    #[test]
    fn natural_language_timezone_precedence_and_local_today() {
        let now = Utc.with_ymd_and_hms(2026, 9, 12, 1, 0, 0).unwrap();
        for input in [
            "today 930 PM eastern",
            "today 930 PM Eastern Time",
            "today 930 PM Eastern Standard Time",
            "today 930 PM eastern daylight time",
            "today 930 PM America/New_York",
        ] {
            let parsed =
                parse_natural_language_timestamp_at(input, Some(Europe::Paris), now).unwrap();
            assert_eq!(
                parsed.start,
                Utc.with_ymd_and_hms(2026, 9, 12, 1, 30, 0).unwrap()
            );
            assert_eq!(parsed.timezone, Some(America::New_York));
            assert!(!parsed.used_default_timezone);
        }
        let parsed =
            parse_natural_language_timestamp_at("930 PM today", Some(America::New_York), now)
                .unwrap();
        assert_eq!(
            parsed.start,
            Utc.with_ymd_and_hms(2026, 9, 12, 1, 30, 0).unwrap()
        );
        assert!(parsed.used_default_timezone);
    }

    #[test]
    fn natural_language_explicit_offsets_override_profile() {
        for (input, expected) in [
            ("today 9:30pm -0400", "2026-09-12T01:30:00Z"),
            ("today 9:30pm -04:00", "2026-09-12T01:30:00Z"),
            ("today 21:30Z", "2026-09-11T21:30:00Z"),
            ("2026-09-11T21:30:00+05:30", "2026-09-11T16:00:00Z"),
            ("2026-09-11T21:30:00.123Z", "2026-09-11T21:30:00.123Z"),
        ] {
            for default in [None, Some(America::Los_Angeles)] {
                let parsed = parse_natural_language_timestamp_at(input, default, september_now())
                    .unwrap_or_else(|| panic!("{input}"));
                assert_eq!(
                    parsed.start,
                    expected.parse::<DateTime<Utc>>().unwrap(),
                    "{input}"
                );
                assert!(!parsed.used_default_timezone);
            }
        }
    }

    #[test]
    fn natural_language_preserves_relative_dates_and_durations() {
        for (input, expected) in [
            ("tomorrow 15:00 UTC", "2026-09-12T15:00:00Z"),
            ("next friday 8pm UTC", "2026-09-11T20:00:00Z"),
            ("last friday 8pm UTC", "2026-09-04T20:00:00Z"),
            ("2 days 8pm UTC", "2026-09-13T20:00:00Z"),
            ("3 hours UTC", "2026-09-11T19:00:00Z"),
            ("15m UTC", "2026-09-11T16:15:00Z"),
            ("September 5 UTC", "2026-09-05T00:00:00Z"),
            ("next September 5 9am UTC", "2027-09-05T09:00:00Z"),
            ("last October 5 9am UTC", "2025-10-05T09:00:00Z"),
        ] {
            let parsed = parse_natural_language_timestamp_at(input, None, september_now())
                .unwrap_or_else(|| panic!("{input}"));
            assert_eq!(
                parsed.start,
                expected.parse::<DateTime<Utc>>().unwrap(),
                "{input}"
            );
        }
    }

    #[test]
    fn natural_language_handles_daylight_saving_boundaries() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        assert!(parse_natural_language_timestamp_at("March 8 2:30am eastern", None, now).is_none());
        let repeated =
            parse_natural_language_timestamp_at("November 1 1:30am eastern", None, now).unwrap();
        assert_eq!(
            repeated.start,
            Utc.with_ymd_and_hms(2026, 11, 1, 6, 30, 0).unwrap()
        );
        let winter =
            parse_natural_language_timestamp_at("January 5 7pm Eastern", None, now).unwrap();
        assert_eq!(
            winter.start,
            Utc.with_ymd_and_hms(2026, 1, 6, 0, 0, 0).unwrap()
        );
        assert_eq!(
            timezone_utc_offset(America::New_York, winter.start),
            "UTC-5"
        );
    }

    #[test]
    fn natural_language_every_meridiem_minute_stays_on_requested_day() {
        for hour in 1..=12 {
            for minute in 0..60 {
                for meridiem in ["AM", "PM"] {
                    let expected_hour = hour % 12 + if meridiem == "PM" { 12 } else { 0 };
                    let expected = Utc
                        .with_ymd_and_hms(2026, 9, 11, expected_hour, minute, 0)
                        .unwrap();
                    for clock in [
                        format!("{hour}:{minute:02} {meridiem}"),
                        format!("{hour}{minute:02}{meridiem}"),
                    ] {
                        let input = format!("today {clock} UTC");
                        let parsed =
                            parse_natural_language_timestamp_at(&input, None, september_now())
                                .unwrap();
                        assert_eq!(parsed.start, expected, "{input}");
                    }
                }
            }
        }
    }
}
