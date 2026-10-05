use {
    chrono::{DateTime, Utc},
    std::time::Duration,
    tokio::{
        select,
        time::{Instant, sleep_until},
    },
    tokio_util::sync::CancellationToken,
};

/// Generation and reveal share a signal that advances their scheduled waits.
/// Cancelling this token only skips waiting; it never cancels seed generation.
pub(crate) struct SeedRollTiming {
    deadline: Option<DateTime<Utc>>,
    force_roll: CancellationToken,
}

impl From<Option<DateTime<Utc>>> for SeedRollTiming {
    fn from(deadline: Option<DateTime<Utc>>) -> Self {
        Self::new(deadline, CancellationToken::new())
    }
}

impl SeedRollTiming {
    pub(super) fn new(deadline: Option<DateTime<Utc>>, force_roll: CancellationToken) -> Self {
        Self {
            deadline,
            force_roll,
        }
    }

    pub(super) fn deadline(&self) -> Option<DateTime<Utc>> {
        if self.force_roll.is_cancelled() {
            None
        } else {
            self.deadline
        }
    }

    pub(super) async fn wait(&self, duration: Duration) {
        self.wait_until(Instant::now() + duration).await;
    }

    pub(super) async fn wait_until(&self, deadline: Instant) {
        select! {
            () = sleep_until(deadline) => {}
            () = self.force_roll.cancelled() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn force_roll_wakes_generation_and_reveal() {
        let signal = CancellationToken::new();
        let at = Utc::now() + chrono::TimeDelta::hours(1);
        let generation = SeedRollTiming::new(Some(at), signal.clone());
        let reveal = SeedRollTiming::new(Some(at), signal.clone());
        let generation_wait = generation.wait(Duration::from_secs(3600));
        let reveal_wait = reveal.wait(Duration::from_secs(3600));
        tokio::pin!(generation_wait, reveal_wait);
        assert!(futures::poll!(&mut generation_wait).is_pending());
        assert!(futures::poll!(&mut reveal_wait).is_pending());
        signal.cancel();
        assert!(futures::poll!(&mut generation_wait).is_ready());
        assert!(futures::poll!(&mut reveal_wait).is_ready());
        assert_eq!(generation.deadline(), None);
        assert_eq!(reveal.deadline(), None);
    }

    #[tokio::test]
    async fn force_roll_is_remembered_for_later_waits_and_is_room_local() {
        let signal = CancellationToken::new();
        signal.cancel();
        signal.cancel(); // Repeated requests are harmless.
        let at = Utc::now() + chrono::TimeDelta::hours(1);
        let timing = SeedRollTiming::new(Some(at), signal);
        assert_eq!(timing.deadline(), None);
        assert!(futures::poll!(Box::pin(timing.wait(Duration::from_secs(3600)))).is_ready());

        let other_room = SeedRollTiming::from(Some(at));
        assert_eq!(other_room.deadline(), Some(at));
        assert!(futures::poll!(Box::pin(other_room.wait(Duration::from_secs(3600)))).is_pending());
    }

    #[tokio::test]
    async fn scheduled_wait_still_expires_without_force_roll() {
        let timing = SeedRollTiming::from(None);
        tokio::time::timeout(Duration::from_secs(1), timing.wait(Duration::ZERO))
            .await
            .unwrap();
    }
}
