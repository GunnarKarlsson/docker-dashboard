use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::client::complete;
use crate::config::InsightConfig;
use crate::reduce::InsightSnapshot;

/// Result of one background Chat Completions call.
#[derive(Debug, Clone)]
pub enum InsightUpdate {
    Started {
        generation: u64,
        context_id: String,
    },
    Reply {
        generation: u64,
        context_id: String,
        text: String,
    },
    Error {
        generation: u64,
        context_id: String,
        message: String,
    },
}

/// Starts a thread that POSTs `snapshot` and sends [`InsightUpdate`] values on a channel.
///
/// - `generation` — caller sequence id; echoed on every update
/// - `context_id` — dashboard context the snapshot was built for; echoed on every update
pub fn spawn_insight(
    snapshot: InsightSnapshot,
    generation: u64,
    context_id: String,
) -> Receiver<InsightUpdate> {
    let (tx, rx) = crossbeam_channel::unbounded();
    thread::spawn(move || run_insight(tx, snapshot, generation, context_id));
    rx
}

fn run_insight(
    tx: Sender<InsightUpdate>,
    snapshot: InsightSnapshot,
    generation: u64,
    context_id: String,
) {
    let _ = tx.send(InsightUpdate::Started {
        generation,
        context_id: context_id.clone(),
    });

    let config = InsightConfig::from_env();
    match complete(&config, &snapshot) {
        Ok(text) => {
            let _ = tx.send(InsightUpdate::Reply {
                generation,
                context_id,
                text,
            });
        }
        Err(err) => {
            let _ = tx.send(InsightUpdate::Error {
                generation,
                context_id,
                message: err.to_string(),
            });
        }
    }
}
