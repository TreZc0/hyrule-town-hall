use crate::{
    event::{AsyncKind, Data, Series, Tab, teams::{QualifierKind, QualifierScoreKind}},
    prelude::*,
    time::decode_pginterval,
};

struct AsyncResultRow {
    place: usize,
    display_name: String,
    time: Duration,
    points: Option<f64>,
    vod: Option<String>,
}

struct AsyncResultSection {
    title: &'static str,
    rows: Vec<AsyncResultRow>,
    has_vod: bool,
}

#[rocket::get("/event/<series>/<event>/async-results")]
pub(crate) async fn get(
    pool: &State<PgPool>,
    me: Option<User>,
    uri: Origin<'_>,
    series: Series,
    event: &str,
) -> Result<RawHtml<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let data = Data::new(&mut transaction, series, event)
        .await?
        .ok_or(StatusOrError::Status(Status::NotFound))?;

    let qualifier_kind = data.qualifier_kind(&mut transaction, me.as_ref()).await?;
    let score_kind = match qualifier_kind {
        QualifierKind::Score(k) => k,
        _ => return Err(StatusOrError::Status(Status::NotFound)),
    };

    let header = data.header(&mut transaction, me.as_ref(), Tab::Teams, true).await?;

    // Find which qualifier asyncs exist for this event
    let async_kinds = sqlx::query_scalar!(
        r#"SELECT kind AS "kind: AsyncKind" FROM asyncs WHERE series = $1 AND event = $2 AND kind IN ('qualifier', 'qualifier2', 'qualifier3') ORDER BY kind"#,
        data.series as _,
        &data.event
    )
    .fetch_all(&mut *transaction)
    .await?;

    let mut sections = Vec::new();

    for async_kind in async_kinds {
        struct PlayerRow {
            player: Id<Users>,
            time: sqlx::postgres::types::PgInterval,
            vod: Option<String>,
        }

        let rows = sqlx::query_as!(
            PlayerRow,
            r#"SELECT ap.player AS "player: Id<Users>", ap.time AS "time!", ap.vod FROM async_players ap WHERE ap.series = $1 AND ap.event = $2 AND ap.kind = $3 AND ap.time IS NOT NULL ORDER BY ap.time ASC"#,
            data.series as _,
            &data.event,
            async_kind as _
        )
        .fetch_all(&mut *transaction)
        .await?;

        if rows.is_empty() {
            continue;
        }

        // Decode all finish times
        let finish_times: Vec<Duration> = rows
            .iter()
            .filter_map(|r| decode_pginterval(r.time.clone()).ok())
            .collect();

        let num_entrants = finish_times.len();
        let par_cutoff = match score_kind {
            QualifierScoreKind::Standard => 7usize.min(num_entrants),
            QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online | QualifierScoreKind::Sgl2025Online => {
                if num_entrants < 20 { 3 } else { 4 }
            }
            QualifierScoreKind::TwwrMiniblins26 => 3,
        };

        let par_time_opt = if finish_times.len() >= par_cutoff {
            Some(finish_times[..par_cutoff].iter().sum::<Duration>() / par_cutoff as u32)
        } else {
            None
        };

        let title = match async_kind {
            AsyncKind::Qualifier1 => "Qualifier 1",
            AsyncKind::Qualifier2 => "Qualifier 2",
            AsyncKind::Qualifier3 => "Qualifier 3",
            _ => continue,
        };

        let mut result_rows = Vec::new();
        let mut has_vod = false;

        for (place, (row, time)) in rows.iter().zip(finish_times.iter()).enumerate() {
            let user = User::from_id(&mut *transaction, row.player)
                .await?
                .expect("async player not found");

            let points = par_time_opt.and_then(|par_time| match score_kind {
                QualifierScoreKind::TwwrMiniblins26 => {
                    Some((2000.0 + ((1.0 - (time.as_secs_f64() - par_time.as_secs_f64()) / par_time.as_secs_f64()) * 1000.0).floor()).max(100.0))
                }
                QualifierScoreKind::Sgl2023Online | QualifierScoreKind::Sgl2024Online | QualifierScoreKind::Sgl2025Online => {
                    Some((100.0 * (2.0 - (time.as_secs_f64() / par_time.as_secs_f64()))).clamp(10.0, 110.0))
                }
                QualifierScoreKind::Standard => {
                    // Standard formula requires per-qualifier par calculation with complex adjustments;
                    // points are shown on the standings page instead
                    None
                }
            });

            if row.vod.is_some() {
                has_vod = true;
            }

            result_rows.push(AsyncResultRow {
                place: place + 1,
                display_name: user.display_name().to_owned(),
                time: *time,
                points,
                vod: row.vod.clone(),
            });
        }

        sections.push(AsyncResultSection { title, rows: result_rows, has_vod });
    }

    let content = html! {
        : header;
        @if sections.is_empty() {
            p : "No async qualifier results are available yet.";
        } else {
            @for section in &sections {
                h2 : section.title;
                table(style = "table-layout: fixed; width: 100%;") {
                    colgroup {
                        col(style = "width: 4rem;");
                        col(style = "min-width: 10rem;");
                        col(style = "width: 7rem;");
                        @if section.rows.iter().any(|r| r.points.is_some()) {
                            col(style = "width: 6rem;");
                        }
                        @if section.has_vod {
                            col(style = "width: 5rem;");
                        }
                    }
                    thead {
                        tr {
                            th : "Place";
                            th : "Name";
                            th : "Time";
                            @if section.rows.iter().any(|r| r.points.is_some()) {
                                th : "Points";
                            }
                            @if section.has_vod {
                                th : "VoD";
                            }
                        }
                    }
                    tbody {
                        @for row in &section.rows {
                            tr {
                                td : row.place;
                                td : &row.display_name;
                                td : English.format_duration(row.time, false);
                                @if section.rows.iter().any(|r| r.points.is_some()) {
                                    td {
                                        @if let Some(pts) = row.points {
                                            : format!("{:.0}", pts);
                                        } else {
                                            : "—";
                                        }
                                    }
                                }
                                @if section.has_vod {
                                    td {
                                        @if let Some(ref vod) = row.vod {
                                            @if let Some(Ok(vod_url)) = (!vod.contains(' ')).then(|| Url::parse(vod)) {
                                                a(href = vod_url.to_string()) : "VoD";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Ok(page(transaction, &me, &uri, PageStyle { chests: data.chests().await?, ..PageStyle::default() }, &format!("Async Results — {}", data.display_name), content).await?)
}
