use eframe::egui;
use egui_tiles::{Behavior, ResizeState, TileId, Tree, UiResponse};

use crate::app::App;
use crate::panels;
use crate::theme;
use crate::ui_elements;

const COL_SHARE: f32 = 1.0;
const CONTEXTS_SHARE: f32 = 2.0;
const HOST_DF_SHARE: f32 = 1.5;
const CONTAINERS_SHARE: f32 = 2.0;
const COMPOSE_SHARE: f32 = 1.5;
const LOGS_SHARE: f32 = 2.0;
const LOG_ERRORS_SHARE: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    Contexts,
    Disk,
    HostDf,
    LocalImages,
    EcrImages,
    Containers,
    Compose,
    RemoteImages,
    Inspect,
    Logs,
    LogErrors,
    Insight,
    StatsEvents,
}

impl PanelId {
    fn title(self) -> &'static str {
        match self {
            PanelId::Contexts => "Contexts",
            PanelId::Disk => "Disk",
            PanelId::HostDf => "Storage Details",
            PanelId::LocalImages => "Local Images",
            PanelId::EcrImages => "ECR Images",
            PanelId::Containers => "Containers",
            PanelId::Compose => "Compose",
            PanelId::RemoteImages => "Remote Images",
            PanelId::Inspect => "Inspect",
            PanelId::Logs => "Logs",
            PanelId::LogErrors => "Log Errors",
            PanelId::Insight => "Insight",
            PanelId::StatsEvents => "Stats / Events",
        }
    }

    fn icon(self) -> Option<egui::ImageSource<'static>> {
        Some(match self {
            PanelId::Contexts => theme::icons::device(),
            PanelId::Disk | PanelId::HostDf => theme::icons::disc(),
            PanelId::LocalImages | PanelId::RemoteImages | PanelId::EcrImages => {
                theme::icons::images()
            }
            PanelId::Containers => theme::icons::containers(),
            PanelId::Compose => theme::icons::compose(),
            PanelId::Inspect => theme::icons::network(),
            PanelId::Logs => theme::icons::logcat(),
            PanelId::LogErrors => theme::icons::errors(),
            PanelId::Insight => theme::icons::insight(),
            PanelId::StatsEvents => theme::icons::traffic(),
        })
    }
}

pub fn create_default_tree() -> Tree<PanelId> {
    let mut tiles = egui_tiles::Tiles::default();

    let contexts = tiles.insert_pane(PanelId::Contexts);
    let disk = tiles.insert_pane(PanelId::Disk);
    let contexts_block = tiles.insert_vertical_tile(vec![contexts, disk]);
    let host_df = tiles.insert_pane(PanelId::HostDf);
    let local_images = tiles.insert_pane(PanelId::LocalImages);
    let ecr_images = tiles.insert_pane(PanelId::EcrImages);
    let left_column =
        tiles.insert_vertical_tile(vec![contexts_block, host_df, local_images, ecr_images]);

    let containers = tiles.insert_pane(PanelId::Containers);
    let compose = tiles.insert_pane(PanelId::Compose);
    let remote_images = tiles.insert_pane(PanelId::RemoteImages);
    let inspect = tiles.insert_pane(PanelId::Inspect);
    let middle_column =
        tiles.insert_vertical_tile(vec![containers, compose, remote_images, inspect]);

    let logs = tiles.insert_pane(PanelId::Logs);
    let log_errors = tiles.insert_pane(PanelId::LogErrors);
    let insight = tiles.insert_pane(PanelId::Insight);
    let stats_events = tiles.insert_pane(PanelId::StatsEvents);
    let right_column = tiles.insert_vertical_tile(vec![logs, log_errors, insight, stats_events]);

    let root = tiles.insert_horizontal_tile(vec![left_column, middle_column, right_column]);

    set_linear_shares(
        &mut tiles,
        root,
        &[
            (left_column, COL_SHARE),
            (middle_column, COL_SHARE),
            (right_column, COL_SHARE),
        ],
    );
    set_linear_shares(
        &mut tiles,
        left_column,
        &[
            (contexts_block, CONTEXTS_SHARE),
            (host_df, HOST_DF_SHARE),
            (local_images, COL_SHARE),
            (ecr_images, COL_SHARE),
        ],
    );
    set_linear_shares(
        &mut tiles,
        contexts_block,
        &[(contexts, COL_SHARE), (disk, COL_SHARE)],
    );
    set_linear_shares(
        &mut tiles,
        middle_column,
        &[
            (containers, CONTAINERS_SHARE),
            (compose, COMPOSE_SHARE),
            (remote_images, COL_SHARE),
            (inspect, COL_SHARE),
        ],
    );
    set_linear_shares(
        &mut tiles,
        right_column,
        &[
            (logs, LOGS_SHARE),
            (log_errors, LOG_ERRORS_SHARE),
            (insight, COL_SHARE),
            (stats_events, COL_SHARE),
        ],
    );

    Tree::new("docker_dashboard_tiles", root, tiles)
}

fn set_linear_shares(
    tiles: &mut egui_tiles::Tiles<PanelId>,
    container_id: egui_tiles::TileId,
    shares: &[(egui_tiles::TileId, f32)],
) {
    let Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(linear))) =
        tiles.get_mut(container_id)
    else {
        return;
    };

    for (tile_id, share) in shares {
        linear.shares.set_share(*tile_id, *share);
    }
}

pub fn show(ui: &mut egui::Ui, tree: &mut Tree<PanelId>, app: &mut App) {
    let mut behavior = AppTilesBehavior { app };
    tree.ui(&mut behavior, ui);
}

struct AppTilesBehavior<'a> {
    app: &'a mut App,
}

impl Behavior<PanelId> for AppTilesBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &PanelId) -> egui::WidgetText {
        pane.title().into()
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        theme::PANEL_GAP
    }

    fn resize_stroke(&self, _style: &egui::Style, resize_state: ResizeState) -> egui::Stroke {
        match resize_state {
            ResizeState::Idle => egui::Stroke::NONE,
            ResizeState::Hovering | ResizeState::Dragging => {
                egui::Stroke::new(1.0, theme::colors::PANEL_SPLITTER_HOVER)
            }
        }
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut PanelId) -> UiResponse {
        match pane {
            PanelId::Contexts => {
                panels::contexts_panel(ui, self.app, pane.icon());
            }
            PanelId::Disk => {
                ui_elements::panel(ui, pane.icon(), pane.title(), |ui| {
                    panels::disk_panel(ui, self.app);
                });
            }
            PanelId::HostDf => {
                ui_elements::panel(ui, pane.icon(), pane.title(), |ui| {
                    panels::host_df_panel(ui, self.app);
                });
            }
            PanelId::Containers => {
                panels::containers_panel(ui, self.app, pane.icon());
            }
            PanelId::Compose => {
                panels::compose_panel(ui, self.app, pane.icon());
            }
            PanelId::LocalImages => {
                panels::local_images_panel(ui, self.app, pane.icon());
            }
            PanelId::Inspect => {
                panels::inspect_panel(ui, self.app, pane.icon());
            }
            PanelId::Logs => {
                let mut show_timestamps = self.app.logs_show_timestamps;
                let mut auto_scroll = self.app.logs_auto_scroll;
                ui_elements::panel_with_footer(
                    ui,
                    pane.icon(),
                    pane.title(),
                    |_| {},
                    |ui, auto_scroll| panels::logs_panel(ui, self.app, auto_scroll),
                    &mut auto_scroll,
                    Some(&mut show_timestamps),
                );
                self.app.logs_show_timestamps = show_timestamps;
                self.app.logs_auto_scroll = auto_scroll;
            }
            PanelId::LogErrors => {
                let mut show_timestamps = self.app.error_show_timestamps;
                let mut auto_scroll = self.app.error_auto_scroll;
                ui_elements::panel_with_footer(
                    ui,
                    pane.icon(),
                    pane.title(),
                    |_| {},
                    |ui, auto_scroll| panels::log_errors_panel(ui, self.app, auto_scroll),
                    &mut auto_scroll,
                    Some(&mut show_timestamps),
                );
                self.app.error_show_timestamps = show_timestamps;
                self.app.error_auto_scroll = auto_scroll;
            }
            _ => {
                ui_elements::panel(ui, pane.icon(), pane.title(), |ui| {
                    panels::placeholder(ui, self.app);
                });
            }
        }
        UiResponse::None
    }
}
