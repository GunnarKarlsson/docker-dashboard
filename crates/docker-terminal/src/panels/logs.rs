use std::collections::{HashSet, VecDeque};

use docker_client::{LogLevel, LogLine, LogStream};
use eframe::egui;

use crate::app::App;
use crate::theme;
use crate::ui_elements;

pub fn logs_panel(ui: &mut egui::Ui, app: &mut App, auto_scroll: bool) -> usize {
    show_container_toggles(ui, app);
    ui_elements::filter_row(ui, |ui| {
        ui.label(filter_text("Filter:"));
        ui.add(
            egui::TextEdit::singleline(&mut app.logs_filter)
                .desired_width(ui.available_width())
                .id_salt("logs_filter"),
        );
    });

    if app.selected_context.is_none() {
        ui.label("No context selected");
        return 0;
    }

    if app.containers.is_none() {
        ui_elements::panel_loading(ui);
        return 0;
    }

    if app.log_targets().is_empty() {
        ui.label("No running containers");
        return 0;
    }

    if app.log_lines.is_empty() {
        ui.label("Waiting for log output…");
        return 0;
    }

    let matching = filtered_line_indices(
        &app.log_lines,
        &app.logs_excluded,
        &app.logs_filter,
        app.logs_show_timestamps,
    );

    if matching.is_empty() {
        ui.label("No log lines match filters");
        return 0;
    }

    show_log_scroll(
        ui,
        &app.log_lines,
        &matching,
        auto_scroll,
        app.logs_show_timestamps,
        egui::Id::new("logs_scroll"),
        false,
    );
    matching.len()
}

pub fn log_errors_panel(ui: &mut egui::Ui, app: &mut App, auto_scroll: bool) -> usize {
    show_container_toggles(ui, app);
    ui_elements::filter_row(ui, |ui| {
        ui.label(filter_text("Filter:"));
        ui.add(
            egui::TextEdit::singleline(&mut app.error_logs_filter)
                .desired_width(ui.available_width())
                .id_salt("error_logs_filter"),
        );
    });

    if app.selected_context.is_none() {
        ui.label("No context selected");
        return 0;
    }

    if app.containers.is_none() {
        ui_elements::panel_loading(ui);
        return 0;
    }

    if app.log_targets().is_empty() {
        ui.label("No running containers");
        return 0;
    }

    if app.error_lines.is_empty() {
        ui.label("Waiting for error log output…");
        return 0;
    }

    let matching = filtered_line_indices(
        &app.error_lines,
        &app.logs_excluded,
        &app.error_logs_filter,
        app.error_show_timestamps,
    );

    if matching.is_empty() {
        ui.label("No log lines match filters");
        return 0;
    }

    show_log_scroll(
        ui,
        &app.error_lines,
        &matching,
        auto_scroll,
        app.error_show_timestamps,
        egui::Id::new("log_errors_scroll"),
        true,
    );
    matching.len()
}

fn show_container_toggles(ui: &mut egui::Ui, app: &mut App) {
    let targets = app.log_targets();
    let mut toggled = None;
    if targets.is_empty() {
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label(filter_text("Containers:"));
        for (id, name) in &targets {
            let included = !app.logs_excluded.contains(id);
            if ui.selectable_label(included, filter_text(name)).clicked() {
                toggled = Some(id.clone());
            }
        }
    });
    ui_elements::section_gap(ui);

    if let Some(id) = toggled {
        app.toggle_log_container(&id);
    }
}

fn filter_text(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(theme::FONT_BODY)
}

fn filtered_line_indices(
    lines: &VecDeque<LogLine>,
    excluded: &HashSet<String>,
    text_filter: &str,
    show_timestamps: bool,
) -> Vec<usize> {
    let query = text_filter.trim().to_ascii_lowercase();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            if !line.container_id.is_empty() && excluded.contains(&line.container_id) {
                return false;
            }
            if query.is_empty() {
                return true;
            }
            line.format_line(show_timestamps)
                .to_ascii_lowercase()
                .contains(&query)
        })
        .map(|(index, _)| index)
        .collect()
}

fn show_log_scroll(
    ui: &mut egui::Ui,
    lines: &VecDeque<LogLine>,
    matching: &[usize],
    stick_to_bottom: bool,
    show_timestamps: bool,
    scroll_id: egui::Id,
    errors_only: bool,
) {
    ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let total_rows = matching.len();

    egui::ScrollArea::both()
        .id_salt(scroll_id)
        .stick_to_bottom(stick_to_bottom)
        .animated(false)
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show_rows(ui, row_height, total_rows, |ui, row_range| {
            for row in row_range {
                let line = &lines[matching[row]];
                let color = if errors_only {
                    error_line_color(line.level)
                } else {
                    log_line_color(line)
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(line.format_line(show_timestamps)).color(color),
                    )
                    .extend(),
                );
            }
        });
}

fn log_line_color(line: &LogLine) -> egui::Color32 {
    match line.level {
        LogLevel::Fatal => theme::colors::LOG_FATAL,
        LogLevel::Error => theme::colors::LOG_ERROR,
        LogLevel::Warn => theme::colors::LOG_WARNING,
        LogLevel::Info => theme::colors::LOG_INFO,
        LogLevel::Debug => theme::colors::LOG_DEBUG,
        LogLevel::None => match line.stream {
            LogStream::Stderr => theme::colors::LOG_WARNING,
            LogStream::Stdout => theme::colors::LOG_DEFAULT,
        },
    }
}

fn error_line_color(level: LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Fatal => theme::colors::LOG_FATAL,
        _ => theme::colors::LOG_ERROR,
    }
}
