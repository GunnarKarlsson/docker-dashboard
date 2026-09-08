//! Application state. Pollers and live data land in later steps.

pub struct App {
    pub docker_error: Option<String>,
}

impl App {
    pub fn new(docker_error: Option<String>) -> Self {
        Self { docker_error }
    }

    pub fn tick(&mut self, _ctx: &eframe::egui::Context) {}

    pub fn shutdown(&mut self) {}
}
