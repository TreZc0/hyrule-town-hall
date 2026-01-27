use {
    chrono::{
        DateTime,
        Days,
        TimeZone,
        Utc,
    },
    chrono_tz::{
        Tz,
        US::Eastern,
    },
    derive_more::{
        Display,
        FromStr,
    },
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Sequence, FromStr, Display)]
pub(crate) enum WeeklyKind {
    Saturday,
}

impl WeeklyKind {
    pub(crate) fn next_weekly_after(&self, min_time: DateTime<impl TimeZone>) -> DateTime<Tz> {
        let mut time = match self {
            Self::Saturday => Utc.with_ymd_and_hms(2026, 1, 24, 18, 0, 0).single().expect("wrong hardcoded datetime"), // 6PM ET
        }.with_timezone(&Eastern);
        while time <= min_time {
            let date = time.date_naive().checked_add_days(Days::new(7)).unwrap();
            time = date.and_hms_opt(match self {
                Self::Saturday => 18,
            }, 0, 0).unwrap().and_local_timezone(Eastern).single_ok().expect("error determining weekly time");
        }
        time
    }
}

pub(crate) async fn info(_transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    let now = Utc::now();
    Ok(match &*data.event {
        "w" => Some(html! {
            article {
                p {
                    : "Weekly races for The Wind Waker Randomizer run every Saturday at 6:00 PM Eastern Time (next: ";
                    : format_datetime(WeeklyKind::Saturday.next_weekly_after(now), DateTimeFormat { long: true, running_text: false });
                    : ").";
                }
            }
        }),
        "miniblins26" => Some(html! {
            article {
                p : "Miniblins 2026 is a tournament for The Wind Waker Randomizer.";
            }
        }),
        _ => None,
    })
}
