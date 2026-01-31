//! Database-driven weekly schedule configuration.

use {
    chrono::{
        DateTime,
        Days,
        NaiveDate,
        NaiveTime,
        TimeZone,
    },
    chrono_tz::Tz,
    crate::{
        id::Table,
        prelude::*,
    },
};

pub(crate) enum WeeklySchedules {}

impl Table for WeeklySchedules {
    fn query_exists(id: i64) -> sqlx::query::QueryScalar<'static, Postgres, bool, <Postgres as sqlx::Database>::Arguments<'static>> {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM weekly_schedules WHERE id = $1) AS "exists!""#, id)
    }
}

/// A configurable weekly schedule stored in the database.
#[derive(Debug, Clone)]
pub(crate) struct WeeklySchedule {
    pub(crate) id: Id<WeeklySchedules>,
    pub(crate) series: Series,
    pub(crate) event: String,
    pub(crate) name: String,
    pub(crate) frequency_days: i16,
    pub(crate) time_of_day: NaiveTime,
    pub(crate) timezone: Tz,
    pub(crate) anchor_date: NaiveDate,
    pub(crate) active: bool,
}

impl WeeklySchedule {
    /// Calculate the next occurrence after a given time.
    pub(crate) fn next_after(&self, min_time: DateTime<impl TimeZone>) -> DateTime<Tz> {
        // Start from the anchor date at the configured time
        let mut time = self.anchor_date
            .and_time(self.time_of_day)
            .and_local_timezone(self.timezone)
            .single()
            .expect("invalid anchor datetime for weekly schedule");

        // Advance by frequency_days until we're past min_time
        while time <= min_time {
            let date = time.date_naive()
                .checked_add_days(Days::new(self.frequency_days as u64))
                .expect("overflow calculating next weekly");
            time = date
                .and_time(self.time_of_day)
                .and_local_timezone(self.timezone)
                .single()
                .expect("error determining weekly time");
        }

        time
    }

    /// Load all schedules for an event.
    pub(crate) async fn for_event(
        transaction: &mut Transaction<'_, Postgres>,
        series: Series,
        event: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id,
                series,
                event,
                name,
                frequency_days,
                time_of_day,
                timezone,
                anchor_date,
                active
            FROM weekly_schedules
            WHERE series = $1 AND event = $2
            ORDER BY name
            "#,
            series as _,
            event
        )
        .fetch_all(&mut **transaction)
        .await?;

        let mut schedules = Vec::with_capacity(rows.len());
        for row in rows {
            schedules.push(Self {
                id: Id::from(row.id),
                series: row.series.parse().expect("invalid series in weekly_schedules"),
                event: row.event,
                name: row.name,
                frequency_days: row.frequency_days,
                time_of_day: row.time_of_day,
                timezone: row.timezone.parse().expect("invalid timezone in weekly_schedules"),
                anchor_date: row.anchor_date,
                active: row.active,
            });
        }

        Ok(schedules)
    }

    /// Load a specific schedule by ID.
    pub(crate) async fn from_id(
        transaction: &mut Transaction<'_, Postgres>,
        id: Id<WeeklySchedules>,
    ) -> Result<Option<Self>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT
                id,
                series,
                event,
                name,
                frequency_days,
                time_of_day,
                timezone,
                anchor_date,
                active
            FROM weekly_schedules
            WHERE id = $1
            "#,
            id as _
        )
        .fetch_optional(&mut **transaction)
        .await?;

        Ok(row.map(|row| Self {
            id: Id::from(row.id),
            series: row.series.parse().expect("invalid series in weekly_schedules"),
            event: row.event,
            name: row.name,
            frequency_days: row.frequency_days,
            time_of_day: row.time_of_day,
            timezone: row.timezone.parse().expect("invalid timezone in weekly_schedules"),
            anchor_date: row.anchor_date,
            active: row.active,
        }))
    }

    /// Save this schedule to the database (update if exists).
    pub(crate) async fn save(&self, transaction: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO weekly_schedules (id, series, event, name, frequency_days, time_of_day, timezone, anchor_date, active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                frequency_days = EXCLUDED.frequency_days,
                time_of_day = EXCLUDED.time_of_day,
                timezone = EXCLUDED.timezone,
                anchor_date = EXCLUDED.anchor_date,
                active = EXCLUDED.active
            "#,
            self.id as _,
            self.series as _,
            &self.event,
            &self.name,
            self.frequency_days,
            self.time_of_day,
            self.timezone.name(),
            self.anchor_date,
            self.active
        )
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    /// Delete this schedule from the database.
    pub(crate) async fn delete(transaction: &mut Transaction<'_, Postgres>, id: Id<WeeklySchedules>) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM weekly_schedules WHERE id = $1", id as _)
            .execute(&mut **transaction)
            .await?;

        Ok(())
    }
}
