use docker_client::LocalImage;
use eframe::egui;

use crate::app::App;
use crate::theme;
use crate::ui_elements;

const CELL_PAD_X: i8 = 4;
const CELL_PAD_Y: i8 = 2;

pub fn local_images_panel(
    ui: &mut egui::Ui,
    app: &mut App,
    icon: Option<egui::ImageSource<'static>>,
) {
    let image_count = app.images.as_ref().map(|images| images.len());
    let mut dangling = app.image_filters.dangling;
    let mut unused = app.image_filters.unused;
    let mut clicked_id = None;

    ui_elements::panel_with_custom_footer(
        ui,
        icon,
        "Local Images",
        |ui| {
            if let Some(count) = image_count {
                ui.label(
                    egui::RichText::new(format!("{count}"))
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

            if let Some(error) = &app.images_error {
                ui_elements::error_label(ui, error);
            }

            let Some(images) = &app.images else {
                ui_elements::panel_loading(ui);
                return;
            };

            let visible: Vec<(&LocalImage, bool, usize)> = images
                .iter()
                .map(|image| {
                    let dangling = image.dangling();
                    let in_use = app.image_in_use(image);
                    (image, dangling, in_use)
                })
                .filter(|(_, dangling, in_use)| app.image_filters.matches(*dangling, *in_use))
                .collect();

            if visible.is_empty() {
                ui.label("No images match filters");
                return;
            }

            let selected_id = app.selected_image.clone();
            egui::ScrollArea::both()
                .id_salt(egui::Id::new("local_images_scroll"))
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    egui::Grid::new("local_images_table")
                        .num_columns(6)
                        .spacing(theme::GRID_SPACING)
                        .min_col_width(16.0)
                        .striped(true)
                        .show(ui, |ui| {
                            header_cell(ui, "Repository");
                            header_cell(ui, "Tag");
                            header_cell(ui, "ID");
                            header_cell(ui, "Created");
                            header_cell(ui, "Size");
                            header_cell(ui, "In use");
                            ui.end_row();

                            for (image, dangling, in_use) in visible {
                                let selected = selected_id.as_deref() == Some(image.id.as_str());
                                if image_row(ui, image, dangling, in_use, selected) {
                                    clicked_id = Some(image.id.clone());
                                }
                            }
                        });
                });
        },
        |ui| {
            ui.horizontal(|ui| {
                filter_toggle(ui, "Dangling", &mut dangling);
                filter_toggle(ui, "Unused", &mut unused);
            });
        },
    );

    app.image_filters.dangling = dangling;
    app.image_filters.unused = unused;
    if let Some(id) = clicked_id {
        app.select_image(id);
    }
}

fn filter_toggle(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    if ui
        .selectable_label(*on, egui::RichText::new(label).size(theme::FONT_BODY))
        .clicked()
    {
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

fn image_row(
    ui: &mut egui::Ui,
    image: &LocalImage,
    dangling: bool,
    in_use: usize,
    selected: bool,
) -> bool {
    let text = if selected {
        theme::colors::OFF_WHITE
    } else {
        theme::colors::HEADER_ICON
    };
    let repo = if dangling {
        "<none>"
    } else if image.repository.is_empty() {
        "—"
    } else {
        image.repository.as_str()
    };
    let tag = if dangling {
        "<none>"
    } else if image.tag.is_empty() {
        "—"
    } else {
        image.tag.as_str()
    };

    let mut clicked = false;
    clicked |= text_cell(ui, selected, repo, text).clicked();
    clicked |= text_cell(ui, selected, tag, text).clicked();
    clicked |= text_cell(ui, selected, image.short_id(), text).clicked();
    clicked |= text_cell(ui, selected, dash(&image.created_since), text).clicked();
    clicked |= text_cell(ui, selected, dash(&image.size), text).clicked();
    clicked |= text_cell(ui, selected, in_use.to_string(), text).clicked();
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
