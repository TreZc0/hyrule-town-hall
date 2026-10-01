use super::*;

pub(crate) struct ScheduledSettings {
    pub(crate) interval: PingInterval,
    pub(crate) time: NaiveTime,
    pub(crate) day: Option<i16>,
    pub(crate) timezone: Tz,
    pub(crate) cutoff_hours: Option<i32>,
}

impl ScheduledSettings {
    pub(crate) fn parse(
        interval: &str,
        time: &str,
        day: &str,
        timezone: &str,
        cutoff_hours: &str,
    ) -> Result<Self, &'static str> {
        let interval = match interval {
            "daily" => PingInterval::Daily,
            "weekly" => PingInterval::Weekly,
            _ => return Err("Choose a daily or weekly interval."),
        };
        let time = NaiveTime::parse_from_str(time, "%H:%M")
            .map_err(|_| "Enter the ping time as HH:MM.")?;
        let day = if interval == PingInterval::Weekly {
            Some(
                day.parse::<i16>()
                    .ok()
                    .filter(|d| (0..=6).contains(d))
                    .ok_or("Choose a day from 0 (Monday) to 6 (Sunday).")?,
            )
        } else {
            None
        };
        let timezone = timezone
            .parse::<Tz>()
            .map_err(|_| "Choose a valid timezone.")?;
        let cutoff_hours = if cutoff_hours.trim().is_empty() {
            None // Preserve the event window for existing workflows.
        } else {
            Some(
                cutoff_hours
                    .parse::<i32>()
                    .ok()
                    .filter(|h| (1..=168).contains(h))
                    .ok_or("The race window must be between 1 and 168 hours.")?,
            )
        };
        Ok(Self {
            interval,
            time,
            day,
            timezone,
            cutoff_hours,
        })
    }

    pub(crate) fn interval_name(&self) -> &'static str {
        match self.interval {
            PingInterval::Daily => "daily",
            PingInterval::Weekly => "weekly",
        }
    }

    /// Resolve wall times deterministically: first occurrence on a fall-back day,
    /// first valid minute after a skipped time on a spring-forward day.
    fn local_instant(&self, date: NaiveDate, time: NaiveTime) -> Option<DateTime<Utc>> {
        let mut local = date.and_time(time);
        for _ in 0..=24 * 60 {
            if let Some(instant) = self.timezone.from_local_datetime(&local).earliest() {
                return Some(instant.with_timezone(&Utc));
            }
            local = local.checked_add_signed(Duration::minutes(1))?;
        }
        None
    }

    pub(crate) fn should_fire(&self, now: DateTime<Utc>, last_sent: Option<DateTime<Utc>>) -> bool {
        let date = now.with_timezone(&self.timezone).date_naive();
        if self.interval == PingInterval::Weekly
            && self.day != Some(date.weekday().num_days_from_monday() as i16)
        {
            return false;
        }
        let Some(target) = self.local_instant(date, self.time) else {
            return false;
        };
        now >= target
            && now - target < Duration::minutes(3)
            && last_sent.is_none_or(|last| last.with_timezone(&self.timezone).date_naive() < date)
    }

    pub(crate) fn cutoff(&self, now: DateTime<Utc>, event_hours: i32) -> DateTime<Utc> {
        now + Duration::hours(i64::from(self.cutoff_hours.unwrap_or(event_hours)))
    }
}

pub(crate) fn cutoff_label(hours: Option<i32>) -> String {
    hours.map_or_else(
        || "Uses event request lead time".to_owned(),
        |hours| format!("Next {hours} hours"),
    )
}

pub(crate) fn scheduled_fields(prefix: &str) -> RawHtml<String> {
    let type_id = format!("{prefix}_type");
    let interval_id = format!("{prefix}_interval");
    let timezone_id = format!("{prefix}_timezone");
    let time_id = format!("{prefix}_time");
    let day_id = format!("{prefix}_dow");
    let hours_id = format!("{prefix}_cutoff_hours");
    let mut timezones = chrono_tz::TZ_VARIANTS
        .iter()
        .copied()
        .map(Tz::name)
        .collect::<Vec<_>>();
    timezones.sort_unstable();
    html! {
        div(data_ping_form_scheduled = &type_id) {
            fieldset {
                label(for = &interval_id) : "Interval:";
                select(name = "ping_interval", id = &interval_id) {
                    option(value = "daily") : "Daily";
                    option(value = "weekly") : "Weekly";
                }
            }
            fieldset {
                label(for = &time_id) : "Ping time (in the selected timezone):";
                input(type = "time", name = "schedule_time", id = &time_id, required, value = "18:00");
            }
            fieldset {
                label(for = &timezone_id) : "Timezone:";
                select(name = "schedule_timezone", id = &timezone_id, class = "ping-timezone-picker", data_ping_timezone) {
                    @for timezone in timezones {
                        option(value = timezone, selected? = timezone == "UTC") : timezone;
                    }
                }
                p(class = "help") : "Starts with your browser's timezone. Choose America/New_York for Eastern time. Ping times follow daylight-saving changes.";
            }
            div(data_ping_form_weekly = &interval_id) {
                label(for = &day_id) : "Day of week (0=Mon..6=Sun):";
                input(type = "number", name = "schedule_day_of_week", id = &day_id, min = "0", max = "6", value = "0", required);
            }
            fieldset {
                label(for = &hours_id) : "Race window (hours after the ping):";
                input(type = "number", name = "cutoff_hours", id = &hours_id, min = "1", max = "168", value = "24", required);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }
    fn settings(time: &str, hours: &str) -> ScheduledSettings {
        ScheduledSettings::parse("daily", time, "", "America/New_York", hours).unwrap()
    }

    #[test]
    fn eastern_evening_tracks_daylight_saving_and_has_own_window() {
        let schedule = settings("21:00", "24");
        for now in [
            instant("2026-07-02T01:00:00Z"),
            instant("2026-12-02T02:00:00Z"),
        ] {
            assert!(schedule.should_fire(now, None));
            assert_eq!(schedule.cutoff(now, 72), now + Duration::hours(24));
            assert!(!schedule.should_fire(now + Duration::minutes(3), None));
            assert!(!schedule.should_fire(now, Some(now)));
        }
        let morning = settings("09:00", "15");
        let now = instant("2026-07-01T13:00:00Z");
        assert!(morning.should_fire(now, None));
        assert_eq!(morning.cutoff(now, 72), now + Duration::hours(15));
    }

    #[test]
    fn local_date_and_weekday_can_differ_from_utc() {
        let mut schedule = settings("21:00", "24");
        schedule.interval = PingInterval::Weekly;
        schedule.day = Some(2); // Wednesday evening, Thursday UTC.
        assert!(schedule.should_fire(instant("2026-07-02T01:00:00Z"), None));
        assert!(!schedule.should_fire(instant("2026-07-03T01:00:00Z"), None));
    }

    #[test]
    fn daily_dedup_handles_short_and_repeated_dst_days() {
        let schedule = settings("09:00", "24");
        assert!(schedule.should_fire(
            instant("2026-03-08T13:00:00Z"),
            Some(instant("2026-03-07T14:02:00Z"))
        ));
        let repeated = settings("01:30", "24");
        assert!(repeated.should_fire(instant("2026-11-01T05:30:00Z"), None));
        assert!(!repeated.should_fire(instant("2026-11-01T06:30:00Z"), None));
        let skipped = settings("02:30", "24");
        assert!(skipped.should_fire(instant("2026-03-08T07:00:00Z"), None));
    }

    #[test]
    fn legacy_window_and_validation() {
        let schedule = settings("21:00", "");
        let now = instant("2026-07-02T01:00:00Z");
        assert_eq!(schedule.cutoff(now, 72), now + Duration::hours(72));
        for (timezone, hours) in [
            ("invalid", "24"),
            ("UTC", "0"),
            ("UTC", "169"),
            ("UTC", "bad"),
        ] {
            assert!(ScheduledSettings::parse("daily", "09:00", "", timezone, hours).is_err());
        }
        assert!(ScheduledSettings::parse("weekly", "09:00", "7", "UTC", "24").is_err());
        assert!(ScheduledSettings::parse("daily", "bad", "", "UTC", "24").is_err());
    }
}
