mod compose;
mod containers;
mod contexts;
mod disk;
mod donut;
mod host_df;
mod images;
mod inspect;
mod logs;
mod stats_events;

use eframe::egui;

use crate::app::App;
use crate::ui_elements;

pub use compose::compose_panel;
pub use containers::containers_panel;
pub use contexts::contexts_panel;
pub use disk::disk_panel;
pub use host_df::host_df_panel;
pub use images::local_images_panel;
pub use inspect::inspect_panel;
pub use logs::{log_errors_panel, logs_panel};
pub use stats_events::stats_events_panel;

pub fn placeholder(ui: &mut egui::Ui, app: &App) {
    if app.selected_context.is_none() {
        ui.label("No context selected");
        return;
    }
    ui_elements::panel_loading(ui);
}
