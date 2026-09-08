use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

const STOP_POLL_STEP: Duration = Duration::from_millis(100);
const BACKOFF_STEP: Duration = Duration::from_secs(1);
const MAX_BACKOFF_INTERVAL: Duration = Duration::from_secs(5);

/// Signal a background worker to stop without blocking on `join`.
///
/// Dropping the join handle detaches the thread so app shutdown is not stalled
/// by in-flight docker commands.
pub(crate) fn signal_stop_and_detach(
    stop_tx: &Sender<()>,
    join_handle: &mut Option<JoinHandle<()>>,
) {
    let _ = stop_tx.send(());
    let _ = join_handle.take();
}

/// Sleeps `duration`, returning early when `stop_rx` receives.
pub(crate) fn sleep_until_stop(stop_rx: &Receiver<()>, duration: Duration) {
    let mut elapsed = Duration::ZERO;

    while elapsed < duration {
        if stop_rx.try_recv().is_ok() {
            return;
        }
        thread::sleep(STOP_POLL_STEP);
        elapsed += STOP_POLL_STEP;
    }
}

/// Increases a poll interval after an error, capped at [`MAX_BACKOFF_INTERVAL`].
pub(crate) fn next_backoff_interval(current: Duration) -> Duration {
    (current + BACKOFF_STEP).min(MAX_BACKOFF_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::{next_backoff_interval, MAX_BACKOFF_INTERVAL};
    use std::time::Duration;

    #[test]
    fn backoff_caps_at_max() {
        let stepped = next_backoff_interval(Duration::from_secs(2));
        assert_eq!(stepped, Duration::from_secs(3));
        assert_eq!(
            next_backoff_interval(MAX_BACKOFF_INTERVAL),
            MAX_BACKOFF_INTERVAL
        );
    }
}
