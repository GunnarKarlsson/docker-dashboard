mod app;
mod format;
mod layout;
#[cfg(target_os = "macos")]
mod macos;
mod panels;
mod theme;
mod ui_elements;

use compose_client::Compose;
use docker_client::Docker;
use ecr_client::Ecr;
use egui_tiles::Tree;

use crate::app::App;
use crate::layout::PanelId;

struct TerminalApp {
    inner: App,
    layout_tree: Tree<PanelId>,
}

impl TerminalApp {
    fn new() -> Self {
        Self {
            inner: App::new(),
            layout_tree: layout::create_default_tree(),
        }
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.inner.tick(ctx);

        #[cfg(target_os = "macos")]
        ui_elements::title_bar(ctx, frame);

        eframe::egui::CentralPanel::default()
            .frame(ui_elements::shell_frame(ctx))
            .show(ctx, |ui| {
                ui_elements::canvas_margin_frame().show(ui, |ui| {
                    layout::show(ui, &mut self.layout_tree, &mut self.inner);
                });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.inner.shutdown();
    }
}

fn main() -> eframe::Result<()> {
    load_dotenv();
    init_tracing();
    tracing::info!("docker-terminal started");

    if let Err(err) = Docker::check_available() {
        tracing::warn!("docker not available: {}", err.user_message());
    }
    if let Err(err) = Compose::check_available() {
        tracing::warn!("compose not available: {err}");
    }
    if let Err(err) = Ecr::check_available() {
        tracing::warn!("ecr not available: {err}");
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(theme::DEFAULT_WINDOW_SIZE)
        .with_title("Docker Dashboard");
    #[cfg(target_os = "macos")]
    {
        // Content draws under the traffic lights; we paint a dark grey title strip.
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }

    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "Docker Dashboard",
        options,
        Box::new(|cc| {
            theme::configure(&cc.egui_ctx);
            Ok(Box::new(TerminalApp::new()))
        }),
    )
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("ai_insight=info,docker_terminal=info"))
        .add_directive("ai_insight=info".parse().expect("valid directive"))
        .add_directive("docker_terminal=info".parse().expect("valid directive"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .init();
}

/// Loads `crates/docker-terminal/.env` into the process environment.
fn load_dotenv() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    if let Err(err) = dotenvy::from_path(&path) {
        if err.not_found() {
            return;
        }
        eprintln!("failed to load .env: {err}");
    }
}
