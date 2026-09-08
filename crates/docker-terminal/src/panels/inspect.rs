use docker_client::{InspectKind, InspectReport};
use eframe::egui;

use crate::app::App;
use crate::theme;
use crate::ui_elements;

pub fn inspect_panel(ui: &mut egui::Ui, app: &App, icon: Option<egui::ImageSource<'static>>) {
    ui_elements::panel_with_header_actions(
        ui,
        icon,
        "Inspect",
        |ui| {
            if let Some(report) = &app.inspect {
                ui.label(
                    egui::RichText::new(kind_label(report.kind))
                        .color(theme::colors::FOOTER_TEXT)
                        .small(),
                );
            }
        },
        |ui| {
            if app.selected_context.is_none() {
                ui.label("No context selected");
                return;
            }

            if let Some(error) = &app.inspect_error {
                ui_elements::error_label(ui, error);
            }

            if app.inspect_target.is_none() {
                ui.label("Select a container or image");
                return;
            }

            let Some(report) = &app.inspect else {
                ui_elements::panel_loading(ui);
                return;
            };

            egui::ScrollArea::both()
                .id_salt(egui::Id::new("inspect_scroll"))
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    show_report(ui, report);
                });
        },
    );
}

fn show_report(ui: &mut egui::Ui, report: &InspectReport) {
    egui::Grid::new("inspect_grid")
        .num_columns(2)
        .spacing(theme::GRID_SPACING)
        .min_col_width(16.0)
        .striped(true)
        .show(ui, |ui| {
            kv(ui, "Name", &report.title);
            kv(ui, "ID", &report.id);
            kv(ui, "Digest", &report.image_digest);
            kv(ui, "State", &report.state);
            kv(ui, "Command", &report.command);
            kv(ui, "Entrypoint", &report.entrypoint);
            kv(ui, "Restart", &report.restart_policy);
            kv(ui, "Health", &report.health_status);
            kv(ui, "Health output", &report.health_output);
            kv(ui, "Started", timestamp(&report.started_at));
            kv(ui, "Finished", timestamp(&report.finished_at));
            if let Some(code) = report.exit_code {
                kv(ui, "Exit", &code.to_string());
            }
            kv(ui, "Compose project", &report.compose_project);
            kv(ui, "Compose service", &report.compose_service);
            kv(ui, "Working dir", &report.compose_workdir);

            for (key, value) in &report.env {
                kv(ui, key, value);
            }
            for mount in &report.mounts {
                kv(ui, "Mount", mount);
            }
            for port in &report.ports {
                kv(ui, "Port", port);
            }
            for network in &report.networks {
                kv(ui, "Network", network);
            }
        });
}

fn kv(ui: &mut egui::Ui, key: &str, value: &str) {
    let value = if value.is_empty() { "—" } else { value };
    ui.add(
        egui::Label::new(
            egui::RichText::new(key)
                .color(theme::colors::FOOTER_TEXT)
                .small(),
        )
        .wrap_mode(egui::TextWrapMode::Extend),
    );
    ui.add(
        egui::Label::new(egui::RichText::new(value).color(theme::colors::OFF_WHITE))
            .wrap_mode(egui::TextWrapMode::Extend)
            .selectable(true),
    );
    ui.end_row();
}

fn timestamp(value: &str) -> &str {
    if value.is_empty() || value.starts_with("0001-01-01") {
        ""
    } else {
        value
    }
}

fn kind_label(kind: InspectKind) -> &'static str {
    match kind {
        InspectKind::Container => "container",
        InspectKind::Image => "image",
    }
}
