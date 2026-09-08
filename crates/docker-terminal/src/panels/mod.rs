mod contexts;

use eframe::egui;

pub use contexts::contexts_panel;

pub fn placeholder(ui: &mut egui::Ui) {
    ui.label("No context selected");
}
