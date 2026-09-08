use compose_client::Compose;
use docker_client::Docker;
use ecr_client::Ecr;
use eframe::egui::{self, Color32};

const WINDOW_SIZE: [f32; 2] = [1400.0, 900.0];
const BG: Color32 = Color32::from_rgb(16, 20, 28);

struct TerminalApp;

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG))
            .show(ctx, |ui| {
                ui.colored_label(Color32::from_rgb(232, 236, 242), "Docker dashboard");
            });
    }
}

fn main() -> eframe::Result<()> {
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

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(WINDOW_SIZE)
        .with_title("Docker Dashboard");

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Docker Dashboard",
        options,
        Box::new(|_cc| Ok(Box::new(TerminalApp))),
    )
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("docker_terminal=info"))
        .add_directive("docker_terminal=info".parse().expect("valid directive"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .init();
}
