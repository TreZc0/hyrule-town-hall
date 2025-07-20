use {
    sqlx::postgres::types::PgInterval,
    crate::prelude::*,
};

const NANOS_PER_SEC: u32 = 1_000_000_000;

pub(crate) trait TimeDeltaExt: Sized {
    fn as_secs_f64(&self) -> f64;
    fn from_secs_f64(secs: f64) -> Self;
    fn abs_diff(self, other: Self) -> Self;

    fn div_duration_f64(self, rhs: Self) -> f64 {
        self.as_secs_f64() / rhs.as_secs_f64()
    }

    fn mul_f64(self, rhs: f64) -> Self {
        Self::from_secs_f64(rhs * self.as_secs_f64())
    }
}

impl TimeDeltaExt for TimeDelta {
    fn as_secs_f64(&self) -> f64 {
        (self.num_seconds() as f64) + (self.subsec_nanos() as f64) / (NANOS_PER_SEC as f64)
    }

    fn from_secs_f64(secs: f64) -> Self {
        Self::seconds(secs.trunc() as i64) + Self::nanoseconds((secs.fract() * (NANOS_PER_SEC as f64)) as i64)
    }

    fn abs_diff(self, other: Self) -> Self {
        (self - other).abs()
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PgIntervalDecodeError {
    #[error(transparent)] TryFromInt(#[from] std::num::TryFromIntError),
    #[error("found PgInterval with nonzero months in database")]
    Months,
    #[error("PgInterval too long")]
    Range,
}

pub(crate) fn decode_pginterval(PgInterval { months, days, microseconds }: PgInterval) -> Result<Duration, PgIntervalDecodeError> {
    if months == 0 {
        Duration::from_secs(u64::try_from(days)? * 60 * 60 * 24)
            .checked_add(Duration::from_micros(microseconds.try_into()?))
            .ok_or(PgIntervalDecodeError::Range)
    } else {
        Err(PgIntervalDecodeError::Months)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum DurationUnit {
    Hours,
    Minutes,
    Seconds,
}

impl DurationUnit {
    fn with_magnitude(&self, magnitude: u64) -> Duration {
        Duration::from_secs(match self {
            Self::Hours => 60 * 60 * magnitude,
            Self::Minutes => 60 * magnitude,
            Self::Seconds => magnitude,
        })
    }
}

pub(crate) fn parse_duration(mut s: &str, default_unit: DurationUnit) -> Option<Duration> {
    let mut duration = Duration::default();
    let mut default_unit = Some(default_unit);
    let mut last_magnitude = None;
    loop {
        match s.chars().next() {
            None => break,
            Some(' ') => s = &s[1..],
            Some('0'..='9') => {
                let (_, magnitude, rest) = regex_captures!("^([0-9]+)(.*)$", s)?; //TODO allow fractional magnitudes? (e.g. 2.5h = 2h30m)
                if last_magnitude.replace(magnitude.parse().ok()?).is_some() {
                    return None // multiple whitespace-separated numbers
                }
                s = rest;
            }
            Some(':') => {
                let magnitude = last_magnitude.take()?;
                duration += default_unit?.with_magnitude(magnitude);
                default_unit = match default_unit? {
                    DurationUnit::Hours => Some(DurationUnit::Minutes),
                    DurationUnit::Minutes => Some(DurationUnit::Seconds),
                    DurationUnit::Seconds => None,
                };
                s = &s[1..];
            }
            Some('H' | 'h') => {
                let magnitude = last_magnitude.take()?;
                duration += Duration::from_secs(60 * 60 * magnitude);
                default_unit = Some(DurationUnit::Minutes);
                (_, s) = regex_captures!("^h(?:(?:ou)?r)?s?(.*)$"i, s)?;
            }
            Some('M' | 'm') => {
                let magnitude = last_magnitude.take()?;
                duration += Duration::from_secs(60 * magnitude);
                default_unit = Some(DurationUnit::Seconds);
                (_, s) = regex_captures!("^m(?:n|in(?:ute)?)?s?(.*)$"i, s)?;
            }
            Some('S' | 's') => {
                let magnitude = last_magnitude.take()?;
                duration += Duration::from_secs(magnitude);
                default_unit = None;
                (_, s) = regex_captures!("^s(?:ec(?:ond)?)?s?(.*)$"i, s)?;
            }
            _ => return None,
        }
    }
    if let Some(magnitude) = last_magnitude {
        duration += default_unit?.with_magnitude(magnitude);
    }
    Some(duration)
}

pub(crate) fn unparse_duration(duration: Duration) -> String {
    let mut buf = String::default();
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    if hours > 0 {
        buf.push_str(&format!("{hours}h"));
        if mins > 0 || secs > 0 {
            buf.push_str(&format!("{mins:02}m"));
        }
        if secs > 0 {
            buf.push_str(&format!("{secs:02}s"));
        }
    } else if mins > 0 {
        buf.push_str(&format!("{mins}m"));
        if secs > 0 {
            buf.push_str(&format!("{secs:02}s"));
        }
    } else {
        buf.push_str(&format!("{secs}s"));
    }
    buf
}

pub(crate) struct DateTimeFormat {
    pub(crate) long: bool,
    pub(crate) running_text: bool,
}

pub(crate) fn format_datetime<Z: TimeZone>(datetime: DateTime<Z>, format: DateTimeFormat) -> RawHtml<String> {
    let utc = datetime.to_utc();
    let paris = datetime.with_timezone(&Europe::Paris);
    let new_york = datetime.with_timezone(&America::New_York);
    let paris_same_date = paris.date_naive() == utc.date_naive();
    let new_york_same_date = new_york.date_naive() == utc.date_naive();
    let paris = paris.format(if paris_same_date { "%H:%M %Z" } else { "%A %H:%M %Z" }).to_string();
    let new_york = new_york.format(match (new_york_same_date, new_york.minute() == 0) {
        (false, false) => "%A %-I:%M %p %Z",
        (false, true) => "%A %-I%p %Z",
        (true, false) => "%-I:%M %p %Z",
        (true, true) => "%-I%p %Z",
    }).to_string();
    html! {
        //TODO once https://github.com/WentTheFox/SledgeHammerTime is out of beta and https://github.com/WentTheFox/SledgeHammerTime/issues/2 is fixed, format as a link, e.g. https://hammertime.cyou/?t=1723402800.000
        span(class = "datetime", data_timestamp = datetime.timestamp_millis(), data_long = format.long.to_string()) {
            : utc.format("%A, %B %-d, %Y, %H:%M UTC").to_string();
            @if format.running_text {
                : " (";
            } else {
                : " • ";
            }
            @if new_york_same_date && !paris_same_date {
                : new_york;
            } else {
                : paris;
            }
            @if format.running_text {
                : ", ";
            } else {
                : " • ";
            }
            @if new_york_same_date && !paris_same_date {
                : paris;
            } else {
                : new_york;
            }
            @if format.running_text {
                : ")";
            }
        }
    }
}

pub(crate) fn format_date_range<Z: TimeZone>(start: DateTime<Z>, end: DateTime<Z>) -> RawHtml<String>
where Z::Offset: fmt::Display {
    html! {
        span(class = "daterange", data_start = start.timestamp_millis(), data_end = end.timestamp_millis()) {
            @if start.year() != end.year() {
                : start.format("%B %-d, %Y").to_string();
                : "–";
                : end.format("%B %-d, %Y").to_string();
            } else if start.month() != end.month() {
                : start.format("%B %-d").to_string();
                : "–";
                : end.format("%B %-d, %Y").to_string();
            } else if start.day() != end.day() {
                : start.format("%B %-d").to_string();
                : "–";
                : end.format("%-d, %Y").to_string();
            } else {
                : start.format("%B %-d, %Y").to_string();
            }
        }
    }
}

pub(crate) fn timezone_info_html() -> RawHtml<String> {
    html! {
        span(class = "timezone-wrapper") {
            : "Times shown in your timezone (detected as ";
            span(class = "timezone") : "[unknown]";
            : ")";
        }
    }
}
