use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use crate::background::{next_backoff_interval, signal_stop_and_detach, sleep_until_stop};
use crate::docker::version_for;
use crate::error::DockerError;
use crate::transport::Transport;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Reachability ping from a selected context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatUpdate {
    Alive { engine_version: String },
    Error(String),
}

/// Background poller that re-runs `docker version` on the selected transport.
pub struct HeartbeatPoller {
    stop_tx: Sender<()>,
    join_handle: Option<JoinHandle<()>>,
}

impl HeartbeatPoller {
    pub fn spawn(transport: Transport) -> Result<(Receiver<HeartbeatUpdate>, Self), DockerError> {
        Self::spawn_with_interval(transport, DEFAULT_POLL_INTERVAL)
    }

    pub fn spawn_with_interval(
        transport: Transport,
        interval: Duration,
    ) -> Result<(Receiver<HeartbeatUpdate>, Self), DockerError> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (stop_tx, stop_rx) = crossbeam_channel::unbounded();

        let join_handle = thread::spawn(move || {
            let mut poll_interval = interval;
            while stop_rx.try_recv().is_err() {
                match version_for(&transport) {
                    Ok(version) => match version.engine_version() {
                        Some(engine) => {
                            poll_interval = interval;
                            if tx
                                .send(HeartbeatUpdate::Alive {
                                    engine_version: engine.to_string(),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        None => {
                            poll_interval = next_backoff_interval(poll_interval);
                            if tx
                                .send(HeartbeatUpdate::Error(
                                    "Docker daemon is not running".to_string(),
                                ))
                                .is_err()
                            {
                                break;
                            }
                        }
                    },
                    Err(err) => {
                        poll_interval = next_backoff_interval(poll_interval);
                        if tx.send(HeartbeatUpdate::Error(err.user_message())).is_err() {
                            break;
                        }
                    }
                }

                sleep_until_stop(&stop_rx, poll_interval);
            }
        });

        Ok((
            rx,
            HeartbeatPoller {
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

impl Drop for HeartbeatPoller {
    fn drop(&mut self) {
        self.shutdown();
    }
}
