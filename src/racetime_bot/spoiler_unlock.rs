use {
    super::{Error, format_error_chain},
    futures::FutureExt as _,
    std::{future::Future, panic::AssertUnwindSafe, time::Duration},
    tokio::time::timeout,
};

/// Spoiler release is optional: failures must not prevent race finalization.
pub(super) async fn best_effort(room_url: &str, unlock: impl Future<Output = Result<bool, Error>>) {
    // The unlock future is dropped after a panic or timeout, releasing its state
    // lock. Completion does not depend on the spoiler state or partial unlock work.
    let result = match timeout(
        Duration::from_secs(30),
        AssertUnwindSafe(unlock).catch_unwind(),
    )
    .await
    {
        Ok(Ok(Ok(_))) => Ok(()),
        Ok(Ok(Err(error))) => Err(format_error_chain(&error)),
        Ok(Err(panic)) => {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic payload");
            Err(format!("unlock panicked: {message}"))
        }
        Err(_) => Err("unlock timed out after 30 seconds".into()),
    };
    if let Err(error) = result {
        log::error!(
            "failed to unlock spoiler log for {room_url}: {error}; continuing race finalization"
        );
    }
}
