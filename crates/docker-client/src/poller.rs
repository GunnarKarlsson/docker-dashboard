use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use crate::background::{next_backoff_interval, signal_stop_and_detach, sleep_until_stop};
use crate::error::DockerError;
use crate::transport::Transport;

/// Background worker that re-runs `fetch` until [`Poller::stop`].
pub struct Poller {
    stop_tx: Sender<()>,
    join_handle: Option<JoinHandle<()>>,
}

impl Poller {
    pub fn spawn<T, F>(
        transport: Transport,
        interval: Duration,
        fetch: F,
    ) -> Result<(Receiver<Result<T, String>>, Self), DockerError>
    where
        T: Send + 'static,
        F: Fn(&Transport) -> Result<T, DockerError> + Send + 'static,
    {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (stop_tx, stop_rx) = crossbeam_channel::unbounded();

        let join_handle = thread::spawn(move || {
            let mut poll_interval = interval;
            while stop_rx.try_recv().is_err() {
                match fetch(&transport) {
                    Ok(value) => {
                        poll_interval = interval;
                        if tx.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        poll_interval = next_backoff_interval(poll_interval);
                        if tx.send(Err(err.user_message())).is_err() {
                            break;
                        }
                    }
                }

                sleep_until_stop(&stop_rx, poll_interval);
            }
        });

        Ok((
            rx,
            Poller {
                stop_tx,
                join_handle: Some(join_handle),
            },
        ))
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        signal_stop_and_detach(&self.stop_tx, &mut self.join_handle);
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.shutdown();
    }
}
