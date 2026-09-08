use eframe::egui;

use crate::app::App;
use crate::format::format_bytes;
use crate::theme;
use crate::ui_elements;

use super::donut::show_usage_donut;

pub fn disk_panel(ui: &mut egui::Ui, app: &App) {
    if app.selected_context.is_none() {
        ui.label("No context selected");
        return;
    }

    if let Some(error) = &app.system_df_error {
        ui_elements::error_label(ui, error);
    }

    let Some(df) = &app.system_df else {
        ui_elements::panel_loading(ui);
        return;
    };

    let reclaimable = format!("reclaimable {}", format_bytes(df.reclaimable_bytes()));
    let (fraction, used, total) = match &app.host_stats {
        Some(stats) if stats.root_total_bytes > 0 => (
            (df.total_bytes() as f32 / stats.root_total_bytes as f32).clamp(0.0, 1.0),
            format_bytes(df.total_bytes()),
            format_bytes(stats.root_total_bytes),
        ),
        _ => (
            df.used_fraction(),
            format_bytes(df.in_use_bytes()),
            format_bytes(df.total_bytes()),
        ),
    };

    show_usage_donut(
        ui,
        fraction,
        used,
        total,
        theme::colors::STORAGE_TRACK,
        theme::colors::STORAGE_USED,
        &[&reclaimable],
    );
}
