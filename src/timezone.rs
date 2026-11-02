//! Jiff time-zone rules behind the Chrono timestamps used by our integrations.
//! Named zones are loaded from Jiff's database (system tzdata on Unix). 

use {
    chrono::{
        Datelike as _, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone,
        Timelike as _,
    },
    jiff::{
        Timestamp, civil,
        tz::{AmbiguousOffset, TimeZone as JiffTimeZone},
    },
    std::{collections::BTreeMap, fmt, str::FromStr, sync::LazyLock},
};

static ZONES: LazyLock<BTreeMap<String, JiffTimeZone>> = LazyLock::new(|| {
    let database = jiff::tz::db();
    database
        .available()
        .map(|name| {
            let zone = database
                .get(name.as_str())
                .expect("could not load an installed time zone; check tzdata");
            (name.to_string(), zone)
        })
        .collect()
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tz {
    name: &'static str,
    rules: &'static JiffTimeZone,
}

impl Tz {
    /// Resolve a built-in zone. Parse user input with `FromStr` instead.
    pub(crate) fn get(name: &str) -> Self {
        name.parse()
            .expect("required IANA time zone is missing; install current tzdata")
    }

    pub(crate) fn name(self) -> &'static str {
        self.name
    }

    pub(crate) fn all() -> impl Iterator<Item = Self> {
        ZONES.iter().map(|(name, rules)| Self { name, rules })
    }

    fn at(self, timestamp: Timestamp) -> TzOffset {
        let info = self.rules.to_offset_info(timestamp);
        TzOffset {
            timezone: self,
            fixed: FixedOffset::east_opt(info.offset().seconds())
                .expect("IANA UTC offset exceeds Chrono's range"),
            timestamp,
        }
    }
}

impl FromStr for Tz {
    type Err = &'static str;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        ZONES
            .get_key_value(name)
            .map(|(name, rules)| Self { name, rules })
            .ok_or("unknown IANA time zone")
    }
}

impl fmt::Display for Tz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TzOffset {
    timezone: Tz,
    fixed: FixedOffset,
    timestamp: Timestamp,
}

impl Offset for TzOffset {
    fn fix(&self) -> FixedOffset {
        self.fixed
    }
}

impl fmt::Display for TzOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            self.timezone
                .rules
                .to_offset_info(self.timestamp)
                .abbreviation(),
        )
    }
}

impl TimeZone for Tz {
    type Offset = TzOffset;

    fn from_offset(offset: &Self::Offset) -> Self {
        offset.timezone
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<Self::Offset> {
        self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).expect("midnight"))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
        let Ok(year) = i16::try_from(local.year()) else {
            return LocalResult::None;
        };
        // The offset is constant within a second. Preserve Chrono's fractional
        // seconds (including its leap-second representation) in the caller.
        let Ok(local) = civil::DateTime::new(
            year,
            local.month() as i8,
            local.day() as i8,
            local.hour() as i8,
            local.minute() as i8,
            local.second() as i8,
            0,
        ) else {
            return LocalResult::None;
        };
        let ambiguous = self.rules.to_ambiguous_timestamp(local);
        match ambiguous.offset() {
            AmbiguousOffset::Unambiguous { .. } => ambiguous
                .unambiguous()
                .map(|timestamp| LocalResult::Single(self.at(timestamp)))
                .unwrap_or(LocalResult::None),
            // Do not apply Jiff's default gap adjustment: callers decide how
            // to handle nonexistent local times, just as they did previously.
            AmbiguousOffset::Gap { .. } => LocalResult::None,
            AmbiguousOffset::Fold { .. } => match (ambiguous.earlier(), ambiguous.later()) {
                (Ok(earlier), Ok(later)) => {
                    LocalResult::Ambiguous(self.at(earlier), self.at(later))
                }
                _ => LocalResult::None,
            },
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
        self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).expect("midnight"))
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
        self.at(Timestamp::from_second(utc.and_utc().timestamp())
            .expect("named-zone conversion outside Jiff's supported years (-9999..=9999)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    #[test]
    fn vancouver_keeps_daylight_offset_from_2026_onward() {
        let zone = Tz::get("America/Vancouver");
        for (date, hours) in [
            ("2025-12-01T12:00:00Z", -8),
            ("2026-01-01T12:00:00Z", -8),
            ("2026-07-01T12:00:00Z", -7),
            ("2026-11-01T08:59:59Z", -7),
            ("2026-11-01T09:00:00Z", -7),
            ("2026-12-01T12:00:00Z", -7),
            ("2027-01-01T12:00:00Z", -7),
        ] {
            let utc: DateTime<Utc> = date.parse().unwrap();
            assert_eq!(
                utc.with_timezone(&zone).offset().fix().local_minus_utc(),
                hours * 3600,
                "{date}; check installed tzdata"
            );
        }
        assert!(
            zone.with_ymd_and_hms(2026, 11, 1, 1, 30, 0)
                .single()
                .is_some()
        );
    }

    #[test]
    fn local_gaps_and_folds_keep_existing_scheduling_semantics() {
        let zone = Tz::get("America/New_York");
        assert!(matches!(
            zone.with_ymd_and_hms(2026, 3, 8, 2, 30, 0),
            LocalResult::None
        ));
        let LocalResult::Ambiguous(first, second) = zone.with_ymd_and_hms(2026, 11, 1, 1, 30, 0)
        else {
            panic!("expected a repeated local time");
        };
        assert_eq!(first.to_utc().to_rfc3339(), "2026-11-01T05:30:00+00:00");
        assert_eq!(second.to_utc().to_rfc3339(), "2026-11-01T06:30:00+00:00");
        assert_eq!(first.format("%Z").to_string(), "EDT");
        assert_eq!(second.format("%Z").to_string(), "EST");
    }

    #[test]
    fn zone_names_round_trip_without_changing_stored_aliases() {
        for zone in Tz::all() {
            assert_eq!(zone.name().parse::<Tz>().unwrap(), zone);
        }
        assert_eq!(Tz::get("US/Eastern").to_string(), "US/Eastern");
        assert!("not/a/zone".parse::<Tz>().is_err());
        assert!("../etc/passwd".parse::<Tz>().is_err());
    }

    #[test]
    fn fractional_seconds_and_non_hour_offsets_survive_conversion() {
        let utc: DateTime<Utc> = "2026-07-01T12:00:00.123456789Z".parse().unwrap();
        let local = utc.with_timezone(&Tz::get("Asia/Kathmandu"));
        assert_eq!(
            local.format("%H:%M:%S%.9f %:z").to_string(),
            "17:45:00.123456789 +05:45"
        );
        assert_eq!(
            local
                .timezone()
                .from_local_datetime(&local.naive_local())
                .single()
                .unwrap()
                .to_utc(),
            utc
        );
    }
}
