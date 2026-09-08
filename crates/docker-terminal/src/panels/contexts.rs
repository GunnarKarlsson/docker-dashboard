use eframe::egui;

use crate::app::{App, ContextId};
use crate::theme;
use crate::ui_elements;

const ROW_CORNER_RADIUS: u8 = 4;
const ROW_PAD_X: i8 = 6;
const ROW_PAD_Y: i8 = 4;

pub fn contexts_panel(ui: &mut egui::Ui, app: &mut App, icon: Option<egui::ImageSource<'static>>) {
    let mut refresh = false;
    let refreshed_at = app.contexts_refreshed_at;
    ui_elements::panel_with_custom_footer(
        ui,
        icon,
        "Contexts",
        |_| {},
        |ui| {
            show_contexts_body(ui, app);
        },
        |ui| {
            refresh = ui_elements::contexts_footer(ui, refreshed_at);
        },
    );
    if refresh {
        app.refresh_contexts();
    }
}

fn show_contexts_body(ui: &mut egui::Ui, app: &mut App) {
    egui::ScrollArea::vertical()
        .id_salt(egui::Id::new("context_list"))
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                show_local_row(ui, app);
            });
        });
}

fn show_local_row(ui: &mut egui::Ui, app: &mut App) {
    let selected = app.selected_context == Some(ContextId::Local);
    let reachable = app.local.reachable;
    let detail = if reachable {
        app.local
            .engine_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        app.local
            .error
            .clone()
            .unwrap_or_else(|| "unreachable".to_string())
    };

    if reachable {
        let response = context_row(ui, true, selected, theme::colors::REACHABLE, &detail);
        if response.clicked() && !selected {
            app.select_context(ContextId::Local);
        }
    } else {
        ui.add_enabled_ui(false, |ui| {
            context_row(ui, false, false, theme::colors::ERROR, &detail);
        });
    }
}

fn context_row(
    ui: &mut egui::Ui,
    clickable: bool,
    selected: bool,
    dot: egui::Color32,
    detail: &str,
) -> egui::Response {
    let fill = if selected {
        theme::colors::SELECTION
    } else {
        egui::Color32::TRANSPARENT
    };
    let text = if selected {
        theme::colors::OFF_WHITE
    } else {
        theme::colors::HEADER_ICON
    };

    let response = egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(ROW_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(ROW_PAD_X, ROW_PAD_Y))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.colored_label(dot, "●");
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Local").color(text));
                    ui.label(egui::RichText::new(detail).color(text));
                });
            });
        })
        .response;

    if clickable {
        response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}
