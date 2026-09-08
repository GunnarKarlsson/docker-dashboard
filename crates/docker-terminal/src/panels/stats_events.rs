use docker_client::{ContainerStats, DockerEvent};
use eframe::egui;

use crate::app::App;
use crate::theme;
use crate::ui_elements;

const CELL_PAD_X: i8 = 4;
const CELL_PAD_Y: i8 = 2;

pub fn stats_events_panel(
    ui: &mut egui::Ui,
    app: &mut App,
    icon: Option<egui::ImageSource<'static>>,
) {
    let stats_count = app.stats.as_ref().map(|stats| stats.len());
    let mut clicked_id = None;

    ui_elements::panel_with_header_actions(
        ui,
        icon,
        "Stats / Events",
        |ui| {
            if let Some(count) = stats_count {
                ui.label(
                    egui::RichText::new(format!("{count} running"))
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

            ui_elements::panel_body(ui, theme::colors::STATS_BODY, |ui| {
                let stats_height = (ui.available_height() * 0.55).max(72.0);
                ui.allocate_ui(egui::vec2(ui.available_width(), stats_height), |ui| {
                    clicked_id = show_stats(ui, app);
                });
                ui.separator();
                show_events(ui, app);
            });
        },
    );

    if let Some(id) = clicked_id {
        app.select_container(id);
    }
}

fn show_stats(ui: &mut egui::Ui, app: &App) -> Option<String> {
    ui.label("Stats");

    if let Some(error) = &app.stats_error {
        ui_elements::error_label(ui, error);
    }

    let Some(stats) = &app.stats else {
        ui_elements::panel_loading(ui);
        return None;
    };

    if stats.is_empty() {
        ui.label("No running containers");
        return None;
    }

    let mut rows: Vec<&ContainerStats> = stats.iter().collect();
    rows.sort_by(|a, b| {
        app.stats_display_name(a)
            .to_ascii_lowercase()
            .cmp(&app.stats_display_name(b).to_ascii_lowercase())
    });

    let selected = app.selected_container.clone();
    let mut clicked = None;

    egui::ScrollArea::both()
        .id_salt(egui::Id::new("stats_scroll"))
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, |ui| {
            egui::Grid::new("container_stats")
                .num_columns(7)
                .spacing(theme::GRID_SPACING)
                .min_col_width(16.0)
                .striped(true)
                .show(ui, |ui| {
                    header_cell(ui, "Name");
                    header_cell(ui, "CPU");
                    header_cell(ui, "Mem");
                    header_cell(ui, "Mem %");
                    header_cell(ui, "Net I/O");
                    header_cell(ui, "Block I/O");
                    header_cell(ui, "PIDs");
                    ui.end_row();

                    for row in rows {
                        let name = app.stats_display_name(row);
                        let is_selected = selected
                            .as_ref()
                            .is_some_and(|id| row.matches_container_id(id));
                        if stats_row(ui, row, &name, is_selected) {
                            if let Some(container) = app.container_matching_id(&row.id) {
                                clicked = Some(container.id.clone());
                            }
                        }
                    }
                });
        });

    clicked
}

fn show_events(ui: &mut egui::Ui, app: &App) {
    ui.label("Events");

    if let Some(error) = &app.events_error {
        ui_elements::error_label(ui, error);
    }

    let Some(events) = &app.events else {
        ui_elements::panel_loading(ui);
        return;
    };

    if events.is_empty() {
        ui.label("No events in the last 30m");
        return;
    }

    egui::ScrollArea::both()
        .id_salt(egui::Id::new("events_scroll"))
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .max_height(ui.available_height())
        .show(ui, |ui| {
            egui::Grid::new("docker_events")
                .num_columns(4)
                .spacing(theme::GRID_SPACING)
                .min_col_width(16.0)
                .striped(true)
                .show(ui, |ui| {
                    header_cell(ui, "Time");
                    header_cell(ui, "Action");
                    header_cell(ui, "Target");
                    header_cell(ui, "Detail");
                    ui.end_row();

                    for event in events {
                        event_row(ui, event);
                    }
                });
        });
}

fn stats_row(ui: &mut egui::Ui, stats: &ContainerStats, name: &str, selected: bool) -> bool {
    let text = if selected {
        theme::colors::OFF_WHITE
    } else {
        theme::colors::HEADER_ICON
    };

    let mut clicked = false;
    clicked |= text_cell(ui, selected, name, text).clicked();
    clicked |= text_cell(ui, selected, dash(&stats.cpu_perc), text).clicked();
    clicked |= text_cell(ui, selected, dash(&stats.mem_usage), text).clicked();
    clicked |= text_cell(ui, selected, dash(&stats.mem_perc), text).clicked();
    clicked |= text_cell(ui, selected, dash(&stats.net_io), text).clicked();
    clicked |= text_cell(ui, selected, dash(&stats.block_io), text).clicked();
    clicked |= text_cell(ui, selected, dash(&stats.pids), text).clicked();
    ui.end_row();
    clicked
}

fn event_row(ui: &mut egui::Ui, event: &DockerEvent) {
    let color = if event.is_failure() {
        theme::colors::ERROR
    } else {
        theme::colors::STATS_BODY
    };
    extend_colored(ui, &event.time_utc(), color);
    extend_colored(ui, event.action_kind(), color);
    extend_colored(ui, &event.target_name(), color);
    extend_colored(ui, event.detail().as_deref().unwrap_or("—"), color);
    ui.end_row();
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

fn extend_colored(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.add(
        egui::Label::new(egui::RichText::new(text).color(color))
            .wrap_mode(egui::TextWrapMode::Extend),
    );
}

fn dash(value: &str) -> &str {
    if value.is_empty() {
        "—"
    } else {
        value
    }
}
