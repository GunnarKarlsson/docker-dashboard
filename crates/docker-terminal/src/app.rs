//! Application state. Pollers land in later steps.

use std::time::Instant;

use docker_client::Docker;

/// Which context is driving the rest of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextId {
    Local,
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
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            local: LocalContext::unreachable("Not checked"),
            contexts_refreshed_at: None,
            selected_context: None,
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
                    self.clear_local_if_selected();
                }
            },
            Err(err) => {
                self.local = LocalContext::unreachable(err.user_message());
                self.clear_local_if_selected();
            }
        }
    }

    pub fn select_context(&mut self, id: ContextId) {
        if !self.is_selectable(id) {
            return;
        }
        self.selected_context = Some(id);
    }

    pub fn is_selectable(&self, id: ContextId) -> bool {
        match id {
            ContextId::Local => self.local.reachable,
        }
    }

    pub fn tick(&mut self, _ctx: &eframe::egui::Context) {}

    pub fn shutdown(&mut self) {}

    fn clear_local_if_selected(&mut self) {
        if self.selected_context == Some(ContextId::Local) {
            self.selected_context = None;
        }
    }
}
