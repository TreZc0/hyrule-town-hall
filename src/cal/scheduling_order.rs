use super::*;

#[derive(sqlx::FromRow)]
struct GameSchedule {
    game: i16,
    first_start: Option<DateTime<Utc>>,
    last_start: Option<DateTime<Utc>>,
}

fn ordering_error(game: i16, start: DateTime<Utc>, siblings: &[GameSchedule]) -> Option<String> {
    for previous in 1..game {
        match siblings
            .iter()
            .find(|sibling| sibling.game == previous)
            .and_then(|sibling| sibling.last_start)
        {
            None => {
                return Some(format!(
                    "Please schedule game {previous} before scheduling game {game}. Games must be played in game-number order."
                ));
            }
            Some(previous_start) if previous_start >= start => {
                return Some(format!(
                    "Game {game} must start after game {previous}. To use an earlier slot, reschedule game {previous} first."
                ));
            }
            Some(_) => {}
        }
    }
    for sibling in siblings {
        if sibling.game > game
            && sibling
                .first_start
                .is_some_and(|next_start| next_start <= start)
        {
            return Some(format!(
                "Game {game} must start before game {}. Reschedule the later game first.",
                sibling.game
            ));
        }
    }
    None
}

impl Race {
    /// Validate live scheduling in both Discord and the organizer editor. Lock the
    /// whole match so concurrent schedule requests cannot both pass a stale check.
    pub(crate) async fn live_schedule_order_error(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        start: DateTime<Utc>,
    ) -> Result<Option<String>, sqlx::Error> {
        let Some(game) = self.game else {
            return Ok(None);
        };
        let siblings = sqlx::query_as::<_, GameSchedule>(r#"
            SELECT sibling.game,
                COALESCE(sibling.start, LEAST(sibling.async_start1, sibling.async_start2, sibling.async_start3)) AS first_start,
                CASE
                    WHEN sibling.start IS NOT NULL THEN sibling.start
                    WHEN sibling.async_start1 IS NOT NULL AND sibling.async_start2 IS NOT NULL
                        AND ((sibling.team3 IS NULL AND sibling.p3 IS NULL) OR sibling.async_start3 IS NOT NULL)
                    THEN GREATEST(sibling.async_start1, sibling.async_start2, sibling.async_start3)
                END AS last_start
            FROM races sibling JOIN races current ON current.id = $1
            WHERE NOT sibling.ignored AND sibling.game IS NOT NULL
                AND sibling.series = current.series AND sibling.event = current.event
                AND CASE
                    WHEN current.startgg_set IS NOT NULL THEN sibling.startgg_set = current.startgg_set
                    WHEN current.challonge_match IS NOT NULL THEN sibling.challonge_match = current.challonge_match
                    WHEN current.scheduling_thread IS NOT NULL THEN sibling.scheduling_thread = current.scheduling_thread
                    ELSE ROW(sibling.phase, sibling.round, sibling.team1, sibling.team2, sibling.team3,
                        sibling.p1, sibling.p2, sibling.p3, sibling.p1_discord, sibling.p2_discord,
                        sibling.p1_racetime, sibling.p2_racetime, sibling.p1_twitch, sibling.p2_twitch,
                        sibling.total, sibling.finished)
                    IS NOT DISTINCT FROM ROW(current.phase, current.round, current.team1, current.team2, current.team3,
                        current.p1, current.p2, current.p3, current.p1_discord, current.p2_discord,
                        current.p1_racetime, current.p2_racetime, current.p1_twitch, current.p2_twitch,
                        current.total, current.finished)
                END
            ORDER BY sibling.id
            FOR UPDATE OF sibling
        "#)
        .bind(self.id)
        .fetch_all(&mut **transaction)
        .await?;
        Ok(ordering_error(game, start, &siblings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + hour * 3600, 0).unwrap()
    }

    fn scheduled(game: i16, hour: Option<i64>) -> GameSchedule {
        GameSchedule {
            game,
            first_start: hour.map(at),
            last_start: hour.map(at),
        }
    }

    #[test]
    fn live_games_must_stay_in_number_order() {
        let games = [
            scheduled(1, Some(24)),
            scheduled(2, None),
            scheduled(3, None),
        ];
        assert!(
            ordering_error(2, at(12), &games)
                .unwrap()
                .contains("after game 1")
        );
        assert!(ordering_error(2, at(24), &games).is_some());
        assert!(ordering_error(2, at(48), &games).is_none());
        assert!(ordering_error(1, at(12), &games).is_none());
        assert!(
            ordering_error(3, at(72), &games)
                .unwrap()
                .contains("schedule game 2")
        );

        let games = [
            scheduled(1, Some(24)),
            scheduled(2, Some(48)),
            scheduled(3, None),
        ];
        assert!(ordering_error(1, at(48), &games).is_some());
        assert!(
            ordering_error(1, at(72), &games)
                .unwrap()
                .contains("before game 2")
        );
        assert!(ordering_error(1, at(36), &games).is_none());
        assert!(ordering_error(3, at(72), &games).is_none());
    }

    #[test]
    fn earlier_games_need_a_complete_schedule() {
        assert!(ordering_error(2, at(24), &[]).is_some());
        assert!(ordering_error(1, at(24), &[]).is_none());
        let mut earlier_async = scheduled(1, Some(12));
        earlier_async.last_start = None;
        assert!(ordering_error(2, at(24), &[earlier_async]).is_some());
        let earlier_async = GameSchedule {
            game: 1,
            first_start: Some(at(12)),
            last_start: Some(at(36)),
        };
        assert!(ordering_error(2, at(24), &[earlier_async]).is_some());
    }

    #[tokio::test]
    #[ignore = "requires HTH_TEST_DATABASE_URL pointing to a migrated production-copy *_test database"]
    async fn database_live_schedule_order() {
        let pool = event::configuration::test_pool().await;
        let mut tx = pool.begin().await.unwrap();
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM races ORDER BY id LIMIT 3")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        assert_eq!(ids.len(), 3);
        let client = reqwest::Client::new();
        let base = Race::from_id(&mut tx, &client, ids[0].into())
            .await
            .unwrap();
        let set = startgg::ID(format!("schedule-order-test-{}", Uuid::new_v4()));
        let mut games = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            let mut race = base.clone();
            race.id = (*id).into();
            race.game = Some(index as i16 + 1);
            race.source = Source::StartGG {
                event: "test/event".into(),
                set: set.clone(),
            };
            race.schedule = RaceSchedule::Unscheduled;
            race.ignored = false;
            race.save(&mut tx).await.unwrap();
            games.push(race);
        }
        assert!(
            games[1]
                .live_schedule_order_error(&mut tx, at(12))
                .await
                .unwrap()
                .is_some()
        );
        games[0].schedule.set_live_start(at(24));
        games[0].save(&mut tx).await.unwrap();
        assert!(
            games[1]
                .live_schedule_order_error(&mut tx, at(12))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            games[1]
                .live_schedule_order_error(&mut tx, at(24))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            games[1]
                .live_schedule_order_error(&mut tx, at(48))
                .await
                .unwrap()
                .is_none()
        );
        games[1].schedule.set_live_start(at(48));
        games[1].save(&mut tx).await.unwrap();
        assert!(
            games[0]
                .live_schedule_order_error(&mut tx, at(72))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            games[0]
                .live_schedule_order_error(&mut tx, at(12))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            games[2]
                .live_schedule_order_error(&mut tx, at(72))
                .await
                .unwrap()
                .is_none()
        );
        // Other start.gg sets in the same scheduling thread must not interfere.
        games[2].source = Source::StartGG {
            event: "test/event".into(),
            set: startgg::ID("different-set".into()),
        };
        games[2].schedule.set_live_start(at(36));
        games[2].save(&mut tx).await.unwrap();
        assert!(
            games[1]
                .live_schedule_order_error(&mut tx, at(48))
                .await
                .unwrap()
                .is_none()
        );
        tx.rollback().await.unwrap();
    }
}
