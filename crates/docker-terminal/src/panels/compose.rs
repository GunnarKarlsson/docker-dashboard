use compose_client::{ComposeProject, ComposeService, ProjectStatus};
use eframe::egui;

use crate::app::App;
use crate::theme;
use crate::ui_elements;

const CELL_PAD_X: i8 = 4;
const CELL_PAD_Y: i8 = 2;

pub fn compose_panel(ui: &mut egui::Ui, app: &mut App, icon: Option<egui::ImageSource<'static>>) {
    let status = app.compose.as_ref().map(|project| project.status);
    let mut clicked = None;
    ui_elements::panel_with_header_actions(
        ui,
        icon,
        "Compose",
        |ui| {
            if let Some(status) = status {
                ui.label(
                    egui::RichText::new(status.as_str())
                        .color(status_color(status))
                        .small(),
                );
            }
        },
        |ui| {
            if app.selected_context.is_none() {
                ui.label("No context selected");
                return;
            }

            if let Some(error) = &app.compose_error {
                ui_elements::error_label(ui, error);
            }

            if app.compose_missing {
                ui.label("No Compose projects");
                return;
            }

            let Some(project) = app.compose.clone() else {
                ui_elements::panel_loading(ui);
                return;
            };

            show_header(ui, &project);

            if !project.resolved_services.is_empty() {
                egui::CollapsingHeader::new(
                    egui::RichText::new("Resolved config").size(theme::FONT_BODY),
                )
                .id_salt("compose_resolved_config")
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(project.resolved_services.join(", "))
                            .color(theme::colors::HEADER_ICON),
                    );
                });
                ui_elements::section_gap(ui);
            }

            if project.services.is_empty() {
                ui.label("No services");
                return;
            }

            let selected_service = app.selected_compose_service.clone();
            egui::ScrollArea::both()
                .id_salt(egui::Id::new("compose_scroll"))
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    egui::Grid::new("compose_table")
                        .num_columns(8)
                        .spacing(theme::GRID_SPACING)
                        .min_col_width(16.0)
                        .striped(true)
                        .show(ui, |ui| {
                            header_cell(ui, "Service");
                            header_cell(ui, "Image");
                            header_cell(ui, "Desired");
                            header_cell(ui, "Current");
                            header_cell(ui, "Health");
                            header_cell(ui, "Restarts");
                            header_cell(ui, "Ports");
                            header_cell(ui, "Depends on");
                            ui.end_row();

                            for service in &project.services {
                                let selected =
                                    selected_service.as_deref() == Some(service.name.as_str());
                                if service_row(ui, service, selected) {
                                    clicked = Some(service.name.clone());
                                }
                            }
                        });
                });
        },
    );
    if let Some(name) = clicked {
        app.toggle_compose_service(&name);
    }
}

fn show_header(ui: &mut egui::Ui, project: &ComposeProject) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(&project.name).strong());
        if !project.status_label.is_empty() {
            ui.label(
                egui::RichText::new(&project.status_label)
                    .color(theme::colors::FOOTER_TEXT)
                    .small(),
            );
        }
    });
    if !project.config_file.is_empty() {
        ui.label(
            egui::RichText::new(&project.config_file)
                .color(theme::colors::FOOTER_TEXT)
                .small(),
        );
    }
    if !project.working_dir.is_empty() {
        ui.label(
            egui::RichText::new(&project.working_dir)
                .color(theme::colors::FOOTER_TEXT)
                .small(),
        );
    }
    ui_elements::section_gap(ui);
}

fn header_cell(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .color(theme::colors::FOOTER_TEXT)
                .small(),
        )
        .wrap_mode(egui::TextWrapMode::Extend),
    );
}

fn service_row(ui: &mut egui::Ui, service: &ComposeService, selected: bool) -> bool {
    let text = if selected {
        theme::colors::OFF_WHITE
    } else {
        theme::colors::HEADER_ICON
    };

    let mut clicked = false;
    clicked |= text_cell(ui, selected, &service.name, text).clicked();
    clicked |= text_cell(ui, selected, dash(&service.image), text).clicked();
    clicked |= text_cell(ui, selected, service.desired.to_string(), text).clicked();
    clicked |= text_cell(ui, selected, &service.current, current_color(service)).clicked();
    clicked |= text_cell(ui, selected, dash(&service.health), health_color(service)).clicked();
    clicked |= text_cell(
        ui,
        selected,
        service
            .restarts
            .map(|count| count.to_string())
            .unwrap_or_else(|| "—".to_string()),
        text,
    )
    .clicked();
    clicked |= text_cell(ui, selected, dash(&service.ports), text).clicked();
    clicked |= text_cell(ui, selected, dash(&service.depends_on), text).clicked();
    ui.end_row();
    clicked
}

fn text_cell(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl AsRef<str>,
    color: egui::Color32,
) -> egui::Response {
    let fill = if selected {
        theme::colors::SELECTION
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(CELL_PAD_X, CELL_PAD_Y))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(text.as_ref()).color(color))
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .selectable(false),
            );
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn dash(value: &str) -> &str {
    if value.is_empty() {
        "—"
    } else {
        value
    }
}

fn status_color(status: ProjectStatus) -> egui::Color32 {
    match status {
        ProjectStatus::Running => theme::colors::REACHABLE,
        ProjectStatus::Partial => theme::colors::LOG_WARNING,
        ProjectStatus::Stopped => theme::colors::ERROR,
    }
}

fn current_color(service: &ComposeService) -> egui::Color32 {
    match service.current.as_str() {
        "running" => theme::colors::REACHABLE,
        "restarting" => theme::colors::LOG_WARNING,
        "paused" => theme::colors::LOG_INFO,
        "exited" | "dead" | "missing" => theme::colors::ERROR,
        _ => theme::colors::HEADER_ICON,
    }
}

fn health_color(service: &ComposeService) -> egui::Color32 {
    match service.health.to_ascii_lowercase().as_str() {
        "healthy" => theme::colors::REACHABLE,
        "unhealthy" => theme::colors::ERROR,
        "starting" => theme::colors::LOG_WARNING,
        _ => theme::colors::HEADER_ICON,
    }
}
