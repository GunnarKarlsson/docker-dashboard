use docker_client::{Network, SystemDf, SystemDfVerbose};
use eframe::egui;

use crate::app::App;
use crate::format::format_bytes;
use crate::theme;
use crate::ui_elements;

const UNUSED_IMAGE_LIMIT: usize = 12;

pub fn host_df_panel(ui: &mut egui::Ui, app: &App) {
    if app.selected_context.is_none() {
        ui.label("No context selected");
        return;
    }

    if let Some(error) = &app.system_df_error {
        ui_elements::error_label(ui, error);
    }
    if let Some(error) = &app.system_df_verbose_error {
        ui_elements::error_label(ui, error);
    }
    if let Some(error) = &app.volumes_error {
        ui_elements::error_label(ui, error);
    }
    if let Some(error) = &app.networks_error {
        ui_elements::error_label(ui, error);
    }

    if app.system_df.is_none()
        && app.system_df_verbose.is_none()
        && app.volumes.is_none()
        && app.networks.is_none()
    {
        ui_elements::panel_loading(ui);
        return;
    }

    ui_elements::panel_body(ui, theme::colors::MEMORY_DISK_BODY, |ui| {
        egui::ScrollArea::both()
            .id_salt(egui::Id::new("storage_details_scroll"))
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                if let Some(df) = &app.system_df {
                    show_summary(ui, df);
                }
                if let Some(verbose) = &app.system_df_verbose {
                    ui.separator();
                    show_unused_images(ui, verbose);
                }
                ui.separator();
                show_volumes(ui, app);
                ui.separator();
                show_networks(ui, app);
                if let Some(verbose) = &app.system_df_verbose {
                    ui.separator();
                    show_build_cache(ui, verbose);
                }
            });
    });
}

fn show_summary(ui: &mut egui::Ui, df: &SystemDf) {
    ui.label("Docker disk");

    egui::Grid::new("system_df_summary")
        .num_columns(5)
        .spacing(theme::GRID_SPACING)
        .striped(true)
        .show(ui, |ui| {
            ui.label("Type");
            ui.label("Total");
            ui.label("Active");
            ui.label("Size");
            ui.label("Reclaimable");
            ui.end_row();

            for row in &df.rows {
                ui.label(&row.kind);
                ui.label(&row.total_count);
                ui.label(&row.active);
                ui.label(&row.size);
                ui.label(&row.reclaimable);
                ui.end_row();
            }
        });

    ui.add_space(theme::ITEM_SPACING_Y);
    ui.label(format!(
        "Would free {} (read-only — no prune)",
        format_bytes(df.reclaimable_bytes())
    ));
}

fn show_unused_images(ui: &mut egui::Ui, verbose: &SystemDfVerbose) {
    ui.label("Largest unused images");

    let unused = verbose.unused_images();
    if unused.is_empty() {
        ui.label("None");
        return;
    }

    egui::Grid::new("unused_images")
        .num_columns(3)
        .spacing(theme::GRID_SPACING)
        .striped(true)
        .show(ui, |ui| {
            extend_label(ui, "Image");
            ui.label("Size");
            ui.label("Unique");
            ui.end_row();

            for image in unused.into_iter().take(UNUSED_IMAGE_LIMIT) {
                extend_label(ui, image.display_name());
                ui.label(format_bytes(image.size_bytes()));
                ui.label(format_bytes(image.unique_bytes()));
                ui.end_row();
            }
        });
}

fn show_volumes(ui: &mut egui::Ui, app: &App) {
    ui.label("Volumes");

    if app.volumes.is_none() && app.system_df_verbose.is_none() {
        ui_elements::panel_loading(ui);
        return;
    }

    if let Some(volumes) = &app.volumes {
        if volumes.is_empty() {
            ui.label("None");
            return;
        }
        egui::Grid::new("df_volumes")
            .num_columns(5)
            .spacing(theme::GRID_SPACING)
            .striped(true)
            .show(ui, |ui| {
                extend_label(ui, "Name");
                ui.label("Driver");
                ui.label("Links");
                ui.label("Size");
                ui.label("Dangling");
                ui.end_row();

                for volume in volumes {
                    let (links, size) = volume_usage(app.system_df_verbose.as_ref(), &volume.name);
                    extend_label(ui, &volume.name);
                    ui.label(dash(&volume.driver));
                    ui.label(links);
                    ui.label(size);
                    if volume.dangling {
                        ui.colored_label(theme::colors::ERROR, "yes");
                    } else {
                        ui.label("—");
                    }
                    ui.end_row();
                }
            });
        return;
    }

    let Some(verbose) = &app.system_df_verbose else {
        return;
    };
    if verbose.volumes.is_empty() {
        ui.label("None");
        return;
    }

    egui::Grid::new("df_volumes")
        .num_columns(3)
        .spacing(theme::GRID_SPACING)
        .striped(true)
        .show(ui, |ui| {
            extend_label(ui, "Name");
            ui.label("Links");
            ui.label("Size");
            ui.end_row();

            for volume in &verbose.volumes {
                extend_label(ui, &volume.name);
                ui.label(&volume.links);
                ui.label(format_bytes(volume.size_bytes()));
                ui.end_row();
            }
        });
}

fn show_networks(ui: &mut egui::Ui, app: &App) {
    ui.label("Networks");

    let Some(networks) = &app.networks else {
        ui_elements::panel_loading(ui);
        return;
    };

    if networks.is_empty() {
        ui.label("None");
        return;
    }

    let mut rows: Vec<&Network> = networks.iter().collect();
    rows.sort_by(|a, b| {
        b.compose_project()
            .is_some()
            .cmp(&a.compose_project().is_some())
            .then_with(|| a.name.cmp(&b.name))
    });

    egui::Grid::new("df_networks")
        .num_columns(3)
        .spacing(theme::GRID_SPACING)
        .striped(true)
        .show(ui, |ui| {
            extend_label(ui, "Name");
            ui.label("Driver");
            extend_label(ui, "Containers");
            ui.end_row();

            for network in rows {
                extend_label(ui, &network.name);
                ui.label(dash(&network.driver));
                extend_label(ui, &attached_label(app, network));
                ui.end_row();
            }
        });
}

fn volume_usage(verbose: Option<&SystemDfVerbose>, name: &str) -> (String, String) {
    verbose
        .and_then(|verbose| verbose.volumes.iter().find(|volume| volume.name == name))
        .map(|volume| {
            (
                if volume.links.is_empty() {
                    "—".to_string()
                } else {
                    volume.links.clone()
                },
                format_bytes(volume.size_bytes()),
            )
        })
        .unwrap_or_else(|| ("—".into(), "—".into()))
}

fn attached_label(app: &App, network: &Network) -> String {
    if network.containers.is_empty() {
        return "—".to_string();
    }
    network
        .containers
        .iter()
        .map(|attached| {
            let name = app
                .container_matching_name(&attached.name)
                .map(|container| container.display_name())
                .unwrap_or_else(|| attached.name.clone());
            if attached.ipv4.is_empty() {
                name
            } else {
                format!("{name} {}", attached.ipv4)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn dash(value: &str) -> &str {
    if value.is_empty() {
        "—"
    } else {
        value
    }
}

fn show_build_cache(ui: &mut egui::Ui, verbose: &SystemDfVerbose) {
    ui.label(format!(
        "Build cache unused: {} entries, {}",
        verbose.unused_cache_count(),
        format_bytes(verbose.unused_cache_bytes())
    ));
}

fn extend_label(ui: &mut egui::Ui, text: impl AsRef<str>) {
    ui.add(egui::Label::new(text.as_ref()).wrap_mode(egui::TextWrapMode::Extend));
}
