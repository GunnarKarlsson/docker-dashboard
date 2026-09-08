use docker_client::Container;
use eframe::egui;

use crate::app::{App, ContainerFilters};
use crate::theme;
use crate::ui_elements;

const CELL_PAD_X: i8 = 4;
const CELL_PAD_Y: i8 = 2;

pub fn containers_panel(
    ui: &mut egui::Ui,
    app: &mut App,
    icon: Option<egui::ImageSource<'static>>,
) {
    ui_elements::panel_with_header_actions(
        ui,
        icon,
        "Containers",
        |ui| {
            if let Some(containers) = &app.containers {
                ui.label(
                    egui::RichText::new(format!("{}", containers.len()))
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

            if let Some(error) = &app.containers_error {
                ui_elements::error_label(ui, error);
            }

            show_filters(ui, &mut app.container_filters);

            let Some(containers) = &app.containers else {
                ui_elements::panel_loading(ui);
                return;
            };

            let visible: Vec<&Container> = containers
                .iter()
                .filter(|container| app.container_filters.matches(container))
                .collect();

            if visible.is_empty() {
                ui.label("No containers match filters");
                return;
            }

            let selected_id = app.selected_container.clone();
            let mut clicked_id = None;
            egui::ScrollArea::both()
                .id_salt(egui::Id::new("containers_scroll"))
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    egui::Grid::new("containers_table")
                        .num_columns(8)
                        .spacing(theme::GRID_SPACING)
                        .min_col_width(16.0)
                        .striped(true)
                        .show(ui, |ui| {
                            header_cell(ui, "");
                            header_cell(ui, "Name");
                            header_cell(ui, "Image");
                            header_cell(ui, "Status");
                            header_cell(ui, "Ports");
                            header_cell(ui, "Created");
                            header_cell(ui, "ID");
                            header_cell(ui, "Compose");
                            ui.end_row();

                            for container in visible {
                                let selected =
                                    selected_id.as_deref() == Some(container.id.as_str());
                                if container_row(ui, container, selected) {
                                    clicked_id = Some(container.id.clone());
                                }
                            }
                        });
                });

            if let Some(id) = clicked_id {
                app.selected_container = Some(id);
            }
        },
    );
}

fn show_filters(ui: &mut egui::Ui, filters: &mut ContainerFilters) {
    ui_elements::filter_row(ui, |ui| {
        state_toggle(ui, "Running", &mut filters.running);
        state_toggle(ui, "Paused", &mut filters.paused);
        state_toggle(ui, "Exited", &mut filters.exited);
        state_toggle(ui, "Created", &mut filters.created);
        state_toggle(ui, "Restarting", &mut filters.restarting);
    });
    ui_elements::filter_row(ui, |ui| {
        ui.label(filter_text("Name or Image:"));
        ui.add(egui::TextEdit::singleline(&mut filters.query).desired_width(140.0));
    });
}

fn filter_text(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(theme::FONT_BODY)
}

fn state_toggle(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    if ui.selectable_label(*on, filter_text(label)).clicked() {
        *on = !*on;
    }
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

fn container_row(ui: &mut egui::Ui, container: &Container, selected: bool) -> bool {
    let text = if selected {
        theme::colors::OFF_WHITE
    } else {
        theme::colors::HEADER_ICON
    };

    let mut clicked = false;
    clicked |= state_cell(ui, selected, state_color(container)).clicked();
    clicked |= text_cell(ui, selected, &container.names, text).clicked();
    clicked |= text_cell(ui, selected, &container.image, text).clicked();
    clicked |= text_cell(ui, selected, &container.status, text).clicked();
    clicked |= text_cell(ui, selected, display_or_dash(&container.ports), text).clicked();
    clicked |= text_cell(ui, selected, display_or_dash(&container.running_for), text).clicked();
    clicked |= text_cell(ui, selected, &container.id, text).clicked();
    clicked |= text_cell(
        ui,
        selected,
        container.compose_label().as_deref().unwrap_or("—"),
        text,
    )
    .clicked();
    ui.end_row();
    clicked
}

fn state_cell(ui: &mut egui::Ui, selected: bool, color: egui::Color32) -> egui::Response {
    cell_frame(ui, selected, |ui| {
        ui.colored_label(color, "●");
    })
}

fn text_cell(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl AsRef<str>,
    color: egui::Color32,
) -> egui::Response {
    cell_frame(ui, selected, |ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(text.as_ref()).color(color))
                .wrap_mode(egui::TextWrapMode::Extend)
                .selectable(false),
        );
    })
}

fn cell_frame(
    ui: &mut egui::Ui,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let fill = if selected {
        theme::colors::SELECTION
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(CELL_PAD_X, CELL_PAD_Y))
        .show(ui, add_contents)
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn state_color(container: &Container) -> egui::Color32 {
    let state = container.state.to_ascii_lowercase();
    if state == "running" && container.status.to_ascii_lowercase().contains("unhealthy") {
        return theme::colors::LOG_WARNING;
    }
    match state.as_str() {
        "running" => theme::colors::REACHABLE,
        "restarting" => theme::colors::LOG_WARNING,
        "paused" => theme::colors::LOG_INFO,
        "exited" | "dead" | "removing" => theme::colors::ERROR,
        "created" => theme::colors::LOG_INFO,
        _ => theme::colors::HEADER_ICON,
    }
}

fn display_or_dash(value: &str) -> &str {
    if value.is_empty() {
        "—"
    } else {
        value
    }
}
