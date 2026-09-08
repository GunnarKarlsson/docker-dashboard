//! Application state: context selection drives pollers.

use std::time::Duration;

use crossbeam_channel::Receiver;
use docker_client::{Docker, HeartbeatPoller, HeartbeatUpdate, Transport};
use eframe::egui;

use std::time::Instant;

const REPAINT_INTERVAL: Duration = Duration::from_millis(200);

/// Which context is driving the rest of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextId {
    Local,
}

impl ContextId {
    fn transport(self) -> Transport {
        match self {
            ContextId::Local => Transport::Local,
        }
    }
}

/// Probe result for the local Docker engine.
pub struct LocalContext {
    pub reachable: bool,
    pub engine_version: Option<String>,
    pub error: Option<String>,
}

impl LocalContext {
    fn unreachable(error: impl Into<String>) -> Self {
        Self {
            reachable: false,
            engine_version: None,
            error: Some(error.into()),
        }
    }
}

pub struct App {
    pub local: LocalContext,
    pub contexts_refreshed_at: Option<Instant>,
    pub selected_context: Option<ContextId>,
    pub heartbeat_error: Option<String>,
    heartbeat_rx: Option<Receiver<HeartbeatUpdate>>,
    heartbeat_poller: Option<HeartbeatPoller>,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            local: LocalContext::unreachable("Not checked"),
            contexts_refreshed_at: None,
            selected_context: None,
            heartbeat_error: None,
            heartbeat_rx: None,
            heartbeat_poller: None,
        };
        app.refresh_contexts();
        app
    }

    pub fn refresh_contexts(&mut self) {
        self.contexts_refreshed_at = Some(Instant::now());
        match Docker::version() {
            Ok(version) => match version.engine_version() {
                Some(engine) => {
                    self.local = LocalContext {
                        reachable: true,
                        engine_version: Some(engine.to_string()),
                        error: None,
                    };
                    if self.selected_context.is_none() {
                        self.select_context(ContextId::Local);
                    }
                }
                None => {
                    self.local = LocalContext::unreachable("Docker daemon is not running");
                    self.deselect_context();
                }
            },
            Err(err) => {
                self.local = LocalContext::unreachable(err.user_message());
                self.deselect_context();
            }
        }
    }

    pub fn select_context(&mut self, id: ContextId) {
        if self.selected_context == Some(id) {
            return;
        }
        if !self.is_selectable(id) {
            return;
        }

        self.stop_streams();
        self.clear_context_data();
        self.selected_context = Some(id);
        self.start_streams(id);
    }

    pub fn deselect_context(&mut self) {
        if self.selected_context.is_none() {
            return;
        }
        self.stop_streams();
        self.clear_context_data();
        self.selected_context = None;
    }

    pub fn is_selectable(&self, id: ContextId) -> bool {
        match id {
            ContextId::Local => self.local.reachable,
        }
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        let mut needs_repaint = false;
        if self.drain_heartbeat() {
            needs_repaint = true;
        }
        if needs_repaint {
            ctx.request_repaint();
        }
        if self.selected_context.is_some() {
            ctx.request_repaint_after(REPAINT_INTERVAL);
        }
    }

    pub fn shutdown(&mut self) {
        self.stop_streams();
    }

    fn start_streams(&mut self, id: ContextId) {
        match HeartbeatPoller::spawn(id.transport()) {
            Ok((rx, poller)) => {
                self.heartbeat_rx = Some(rx);
                self.heartbeat_poller = Some(poller);
            }
            Err(err) => self.heartbeat_error = Some(err.user_message()),
        }
    }

    fn stop_streams(&mut self) {
        if let Some(poller) = self.heartbeat_poller.take() {
            poller.stop();
        }
        self.heartbeat_rx = None;
    }

    fn clear_context_data(&mut self) {
        self.heartbeat_error = None;
    }

    fn drain_heartbeat(&mut self) -> bool {
        let Some(rx) = &self.heartbeat_rx else {
            return false;
        };
        let mut changed = false;
        while let Ok(update) = rx.try_recv() {
            changed = true;
            match update {
                HeartbeatUpdate::Alive { engine_version } => {
                    self.heartbeat_error = None;
                    if self.local.reachable {
                        self.local.engine_version = Some(engine_version);
                    }
                }
                HeartbeatUpdate::Error(message) => {
                    self.heartbeat_error = Some(message);
                }
            }
        }
        changed
    }
}
