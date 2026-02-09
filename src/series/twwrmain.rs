use {
    chrono::Utc,
    crate::{
        event::{
            Data,
            InfoError,
        },
        prelude::*,
    },
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    let now = Utc::now();
    Ok(match &*data.event {
        "w" => {
            let weekly_schedules = WeeklySchedule::for_event(transaction, Series::TwwrMain, "w").await?;
            let active_schedules: Vec<_> = weekly_schedules.iter().filter(|s| s.active).collect();
            Some(html! {
                article {
                    @if active_schedules.is_empty() {
                        p : "Weekly races for The Wind Waker Randomizer are currently not scheduled.";
                    } else if active_schedules.len() == 1 {
                        @let schedule = active_schedules[0];
                        p {
                            : format!("Weekly races for The Wind Waker Randomizer run every {} ", schedule.name);
                            : format_recurring_time(schedule.next_after(now));
                            : " (next: ";
                            : format_datetime(schedule.next_after(now), DateTimeFormat { long: true, running_text: false });
                            : ").";
                        }
                    } else {
                        p : "Weekly races for The Wind Waker Randomizer:";
                        ul {
                            @for schedule in active_schedules {
                                li {
                                    : format!("{} ", schedule.name);
                                    : format_recurring_time(schedule.next_after(now));
                                    : " (next: ";
                                    : format_datetime(schedule.next_after(now), DateTimeFormat { long: true, running_text: false });
                                    : ")";
                                }
                            }
                        }
                    }
                }
            })
        },
        "miniblins26" => Some(html! {
            article {
                p : "Miniblins 2026 is a tournament for The Wind Waker Randomizer.";
            }
        }),
        _ => None,
    })
}
