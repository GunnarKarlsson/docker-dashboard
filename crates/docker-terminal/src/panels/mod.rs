use eframe::egui;

use crate::app::App;
use crate::ui_elements;

pub fn placeholder(ui: &mut egui::Ui) {
    ui.label("No context selected");
}

pub fn contexts_placeholder(ui: &mut egui::Ui, app: &App) {
    if let Some(error) = &app.docker_error {
        ui_elements::error_label(ui, error);
        return;
    }
    ui.label("No context selected");
}
