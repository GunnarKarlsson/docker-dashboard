//! Application state: context selection drives pollers.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use ai_insight::{
    build_snapshot, spawn_insight, InsightConfig, InsightLine, InsightUpdate, LevelMask,
};
use compose_client::{Compose, ComposeProject};
use crossbeam_channel::Receiver;
use docker_client::{
    fetch_host_stats, Container, ContainerStats, Docker, DockerEvent, HeartbeatPoller,
    HeartbeatUpdate, HostStats, InspectReport, InspectTarget, LocalImage, LogLevel, LogLine,
    LogTarget, LogsMux, Network, Poller, SystemDf, SystemDfVerbose, Transport, Volume,
};
use eframe::egui;

use crate::selection::{self, AppMode, Selection};

const REPAINT_INTERVAL: Duration = Duration::from_millis(200);
const SYSTEM_DF_INTERVAL: Duration = Duration::from_secs(10);
const SYSTEM_DF_VERBOSE_INTERVAL: Duration = Duration::from_secs(20);
const HOST_STATS_INTERVAL: Duration = Duration::from_secs(5);
const CONTAINERS_INTERVAL: Duration = Duration::from_secs(2);
const COMPOSE_INTERVAL: Duration = Duration::from_secs(3);
const IMAGES_INTERVAL: Duration = Duration::from_secs(10);
const INSPECT_INTERVAL: Duration = Duration::from_secs(3);
const STATS_INTERVAL: Duration = Duration::from_secs(2);
const EVENTS_INTERVAL: Duration = Duration::from_secs(5);
const VOLUMES_INTERVAL: Duration = Duration::from_secs(10);
const NETWORKS_INTERVAL: Duration = Duration::from_secs(10);
pub const MAX_LOG_LINES: usize = 10_000;
const MAX_INSIGHTS: usize = 100;
const INSIGHT_SETTLE: Duration = Duration::from_secs(5);
const INSIGHT_COOLDOWN: Duration = Duration::from_secs(30);

/// Which context is driving the rest of the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextId {
    Local,
}

impl ContextId {
    fn transport(self) -> Transport {
        match self {
            ContextId::Local => Transport::Local,
        }
    }

    fn as_key(self) -> &'static str {
        match self {
            ContextId::Local => "local",
        }
    }
}

/// Probe result for the local Docker engine.
pub struct LocalContext {
    pub reachable: bool,
    pub engine_version: Option<String>,
    pub error: Option<String>,
}

impl LocalContext {
    fn unreachable(error: impl Into<String>) -> Self {
        Self {
            reachable: false,
            engine_version: None,
            error: Some(error.into()),
        }
    }
}

/// Header filters for the Containers table.
pub struct ContainerFilters {
    pub running: bool,
    pub paused: bool,
    pub exited: bool,
    pub created: bool,
    pub restarting: bool,
    pub query: String,
}

impl Default for ContainerFilters {
    fn default() -> Self {
        Self {
            running: true,
            paused: true,
            exited: true,
            created: true,
            restarting: true,
            query: String::new(),
        }
    }
}

impl ContainerFilters {
    pub fn matches(&self, container: &Container) -> bool {
        if !self.state_enabled(&container.state) {
            return false;
        }

        let query = self.query.trim().to_ascii_lowercase();
        if !query.is_empty() {
            let name = container.names.to_ascii_lowercase();
            let image = container.image.to_ascii_lowercase();
            if !name.contains(&query) && !image.contains(&query) {
                return false;
            }
        }

        true
    }

    fn state_enabled(&self, state: &str) -> bool {
        match state.to_ascii_lowercase().as_str() {
            "running" => self.running,
            "paused" => self.paused,
            "exited" | "dead" | "removing" => self.exited,
            "created" => self.created,
            "restarting" => self.restarting,
            _ => true,
        }
    }
}

/// Footer filters for the Local Images table.
#[derive(Default)]
pub struct ImageFilters {
    pub dangling: bool,
    pub unused: bool,
}

impl ImageFilters {
    pub fn matches(&self, dangling: bool, in_use: usize) -> bool {
        if !self.dangling && !self.unused {
            return true;
        }
        (self.dangling && dangling) || (self.unused && in_use == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsightStatus {
    Idle,
    RequestSent,
    RequestFailed,
}

pub struct InsightState {
    pub status: InsightStatus,
    pub replies: VecDeque<String>,
    last_analyze: Option<Instant>,
    last_error_at: Option<Instant>,
    last_sent_key: Option<String>,
    ever_succeeded: bool,
    generation: u64,
}

impl Default for InsightState {
    fn default() -> Self {
        Self {
            status: InsightStatus::Idle,
            replies: VecDeque::new(),
            last_analyze: None,
            last_error_at: None,
            last_sent_key: None,
            ever_succeeded: false,
            generation: 0,
        }
    }
}

pub struct App {
    pub local: LocalContext,
    pub contexts_refreshed_at: Option<Instant>,
    /// Investigate vs Runtime. Header toggle lands in a later step.
    #[allow(dead_code)]
    pub mode: AppMode,
    pub selection: Selection,
    pub selected_context: Option<ContextId>,
    pub heartbeat_error: Option<String>,
    pub system_df: Option<SystemDf>,
    pub system_df_error: Option<String>,
    pub system_df_verbose: Option<SystemDfVerbose>,
    pub system_df_verbose_error: Option<String>,
    pub host_stats: Option<HostStats>,
    pub host_stats_error: Option<String>,
    pub containers: Option<Vec<Container>>,
    pub containers_error: Option<String>,
    pub selected_container: Option<String>,
    pub selected_compose_service: Option<String>,
    pub container_filters: ContainerFilters,
    pub compose: Option<ComposeProject>,
    pub compose_error: Option<String>,
    pub compose_missing: bool,
    pub images: Option<Vec<LocalImage>>,
    pub images_error: Option<String>,
    pub image_filters: ImageFilters,
    pub selected_image: Option<String>,
    pub inspect: Option<InspectReport>,
    pub inspect_error: Option<String>,
    pub inspect_target: Option<InspectTarget>,
    pub stats: Option<Vec<ContainerStats>>,
    pub stats_error: Option<String>,
    pub events: Option<Vec<DockerEvent>>,
    pub events_error: Option<String>,
    pub volumes: Option<Vec<Volume>>,
    pub volumes_error: Option<String>,
    pub networks: Option<Vec<Network>>,
    pub networks_error: Option<String>,
    pub log_lines: VecDeque<LogLine>,
    pub pending_log_lines: VecDeque<LogLine>,
    pub error_lines: VecDeque<LogLine>,
    pub pending_error_lines: VecDeque<LogLine>,
    pub logs_excluded: HashSet<String>,
    pub error_logs_excluded: HashSet<String>,
    pub logs_filter: String,
    pub error_logs_filter: String,
    pub logs_auto_scroll: bool,
    pub error_auto_scroll: bool,
    pub logs_show_timestamps: bool,
    pub error_show_timestamps: bool,
    pub insight_auto_scroll: bool,
    pub insight: InsightState,
    heartbeat_rx: Option<Receiver<HeartbeatUpdate>>,
    heartbeat_poller: Option<HeartbeatPoller>,
    system_df_rx: Option<Receiver<Result<SystemDf, String>>>,
    system_df_poller: Option<Poller>,
    system_df_verbose_rx: Option<Receiver<Result<SystemDfVerbose, String>>>,
    system_df_verbose_poller: Option<Poller>,
    host_stats_rx: Option<Receiver<Result<HostStats, String>>>,
    host_stats_poller: Option<Poller>,
    containers_rx: Option<Receiver<Result<Vec<Container>, String>>>,
    containers_poller: Option<Poller>,
    compose_rx: Option<Receiver<Result<Option<ComposeProject>, String>>>,
    compose_poller: Option<Poller>,
    images_rx: Option<Receiver<Result<Vec<LocalImage>, String>>>,
    images_poller: Option<Poller>,
    inspect_rx: Option<Receiver<Result<InspectReport, String>>>,
    inspect_poller: Option<Poller>,
    inspect_poller_for: Option<InspectTarget>,
    stats_rx: Option<Receiver<Result<Vec<ContainerStats>, String>>>,
    stats_poller: Option<Poller>,
    events_rx: Option<Receiver<Result<Vec<DockerEvent>, String>>>,
    events_poller: Option<Poller>,
    volumes_rx: Option<Receiver<Result<Vec<Volume>, String>>>,
    volumes_poller: Option<Poller>,
    networks_rx: Option<Receiver<Result<Vec<Network>, String>>>,
    networks_poller: Option<Poller>,
    logs_rx: Option<Receiver<LogLine>>,
    logs_mux: Option<LogsMux>,
    error_logs_rx: Option<Receiver<LogLine>>,
    error_logs_mux: Option<LogsMux>,
    insight_rx: Option<Receiver<InsightUpdate>>,
    insight_context: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            local: LocalContext::unreachable("Not checked"),
            contexts_refreshed_at: None,
            mode: AppMode::Investigate,
            selection: Selection::None,
            selected_context: None,
            heartbeat_error: None,
            system_df: None,
            system_df_error: None,
            system_df_verbose: None,
            system_df_verbose_error: None,
            host_stats: None,
            host_stats_error: None,
            containers: None,
            containers_error: None,
            selected_container: None,
            selected_compose_service: None,
            container_filters: ContainerFilters::default(),
            compose: None,
            compose_error: None,
            compose_missing: false,
            images: None,
            images_error: None,
            image_filters: ImageFilters::default(),
            selected_image: None,
            inspect: None,
            inspect_error: None,
            inspect_target: None,
            stats: None,
            stats_error: None,
            events: None,
            events_error: None,
            volumes: None,
            volumes_error: None,
            networks: None,
            networks_error: None,
            log_lines: VecDeque::new(),
            pending_log_lines: VecDeque::new(),
            error_lines: VecDeque::new(),
            pending_error_lines: VecDeque::new(),
            logs_excluded: HashSet::new(),
            error_logs_excluded: HashSet::new(),
            logs_filter: String::new(),
            error_logs_filter: String::new(),
            logs_auto_scroll: true,
            error_auto_scroll: true,
            logs_show_timestamps: true,
            error_show_timestamps: true,
            insight_auto_scroll: true,
            insight: InsightState::default(),
            heartbeat_rx: None,
            heartbeat_poller: None,
            system_df_rx: None,
            system_df_poller: None,
            system_df_verbose_rx: None,
            system_df_verbose_poller: None,
            host_stats_rx: None,
            host_stats_poller: None,
            containers_rx: None,
            containers_poller: None,
            compose_rx: None,
            compose_poller: None,
            images_rx: None,
            images_poller: None,
            inspect_rx: None,
            inspect_poller: None,
            inspect_poller_for: None,
            stats_rx: None,
            stats_poller: None,
            events_rx: None,
            events_poller: None,
            volumes_rx: None,
            volumes_poller: None,
            networks_rx: None,
            networks_poller: None,
            logs_rx: None,
            logs_mux: None,
            error_logs_rx: None,
            error_logs_mux: None,
            insight_rx: None,
            insight_context: None,
        };
        app.refresh_contexts();
        app
    }

    pub fn refresh_contexts(&mut self) {
        self.contexts_refreshed_at = Some(Instant::now());
        match Docker::version() {
            Ok(version) => match version.engine_version() {
                Some(engine) => {
                    self.local = LocalContext {
                        reachable: true,
                        engine_version: Some(engine.to_string()),
                        error: None,
                    };
                    if self.selected_context.is_none() {
                        self.select_context(ContextId::Local);
                    }
                }
                None => {
                    self.local = LocalContext::unreachable("Docker daemon is not running");
                    self.deselect_context();
                }
            },
            Err(err) => {
                self.local = LocalContext::unreachable(err.user_message());
                self.deselect_context();
            }
        }
    }

    pub fn select_context(&mut self, id: ContextId) {
        if self.selected_context == Some(id) {
            return;
        }
        if !self.is_selectable(id) {
            return;
        }

        self.stop_streams();
        self.clear_context_data();
        self.selected_context = Some(id);
        self.start_streams(id);
    }

    pub fn deselect_context(&mut self) {
        if self.selected_context.is_none() {
            return;
        }
        self.stop_streams();
        self.clear_context_data();
        self.selected_context = None;
    }

    pub fn is_selectable(&self, id: ContextId) -> bool {
        match id {
            ContextId::Local => self.local.reachable,
        }
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        let mut needs_repaint = false;
        if self.drain_heartbeat() {
            needs_repaint = true;
        }
        if drain_result(
            &self.system_df_rx,
            &mut self.system_df,
            &mut self.system_df_error,
        ) {
            needs_repaint = true;
        }
        if drain_result(
            &self.system_df_verbose_rx,
            &mut self.system_df_verbose,
            &mut self.system_df_verbose_error,
        ) {
            needs_repaint = true;
        }
        if drain_result(
            &self.host_stats_rx,
            &mut self.host_stats,
            &mut self.host_stats_error,
        ) {
            needs_repaint = true;
        }
        if drain_result(
            &self.containers_rx,
            &mut self.containers,
            &mut self.containers_error,
        ) {
            self.prune_selected_container();
            self.sync_log_streams();
            needs_repaint = true;
        }
        if self.drain_compose() {
            self.prune_selected_compose_service();
            self.sync_log_streams();
            needs_repaint = true;
        }
        if drain_result(&self.images_rx, &mut self.images, &mut self.images_error) {
            self.prune_selected_image();
            needs_repaint = true;
        }
        if drain_result(&self.inspect_rx, &mut self.inspect, &mut self.inspect_error) {
            needs_repaint = true;
        }
        if drain_result(&self.stats_rx, &mut self.stats, &mut self.stats_error) {
            needs_repaint = true;
        }
        if drain_result(&self.events_rx, &mut self.events, &mut self.events_error) {
            needs_repaint = true;
        }
        if drain_result(&self.volumes_rx, &mut self.volumes, &mut self.volumes_error) {
            needs_repaint = true;
        }
        if drain_result(
            &self.networks_rx,
            &mut self.networks,
            &mut self.networks_error,
        ) {
            needs_repaint = true;
        }
        if self.drain_logs() {
            needs_repaint = true;
        }
        if self.drain_error_logs() {
            needs_repaint = true;
        }
        if self.drain_insight() {
            needs_repaint = true;
        }
        self.maybe_request_insight();
        if needs_repaint {
            ctx.request_repaint();
        }
        if self.selected_context.is_some() {
            ctx.request_repaint_after(REPAINT_INTERVAL);
        }
    }

    pub fn shutdown(&mut self) {
        self.stop_streams();
    }

    fn start_streams(&mut self, id: ContextId) {
        let transport = id.transport();

        match HeartbeatPoller::spawn(transport.clone()) {
            Ok((rx, poller)) => {
                self.heartbeat_rx = Some(rx);
                self.heartbeat_poller = Some(poller);
            }
            Err(err) => self.heartbeat_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), SYSTEM_DF_INTERVAL, Docker::system_df) {
            Ok((rx, poller)) => {
                self.system_df_rx = Some(rx);
                self.system_df_poller = Some(poller);
            }
            Err(err) => self.system_df_error = Some(err.user_message()),
        }

        match Poller::spawn(
            transport.clone(),
            SYSTEM_DF_VERBOSE_INTERVAL,
            Docker::system_df_verbose,
        ) {
            Ok((rx, poller)) => {
                self.system_df_verbose_rx = Some(rx);
                self.system_df_verbose_poller = Some(poller);
            }
            Err(err) => self.system_df_verbose_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), HOST_STATS_INTERVAL, fetch_host_stats) {
            Ok((rx, poller)) => {
                self.host_stats_rx = Some(rx);
                self.host_stats_poller = Some(poller);
            }
            Err(err) => self.host_stats_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), CONTAINERS_INTERVAL, Docker::ps_a) {
            Ok((rx, poller)) => {
                self.containers_rx = Some(rx);
                self.containers_poller = Some(poller);
            }
            Err(err) => self.containers_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), COMPOSE_INTERVAL, Compose::snapshot) {
            Ok((rx, poller)) => {
                self.compose_rx = Some(rx);
                self.compose_poller = Some(poller);
            }
            Err(err) => self.compose_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), IMAGES_INTERVAL, Docker::images) {
            Ok((rx, poller)) => {
                self.images_rx = Some(rx);
                self.images_poller = Some(poller);
            }
            Err(err) => self.images_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), STATS_INTERVAL, Docker::stats) {
            Ok((rx, poller)) => {
                self.stats_rx = Some(rx);
                self.stats_poller = Some(poller);
            }
            Err(err) => self.stats_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), EVENTS_INTERVAL, Docker::events) {
            Ok((rx, poller)) => {
                self.events_rx = Some(rx);
                self.events_poller = Some(poller);
            }
            Err(err) => self.events_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), VOLUMES_INTERVAL, Docker::volumes) {
            Ok((rx, poller)) => {
                self.volumes_rx = Some(rx);
                self.volumes_poller = Some(poller);
            }
            Err(err) => self.volumes_error = Some(err.user_message()),
        }

        match Poller::spawn(transport.clone(), NETWORKS_INTERVAL, Docker::networks) {
            Ok((rx, poller)) => {
                self.networks_rx = Some(rx);
                self.networks_poller = Some(poller);
            }
            Err(err) => self.networks_error = Some(err.user_message()),
        }

        let (logs_rx, logs_mux) = LogsMux::spawn(transport.clone());
        self.logs_rx = Some(logs_rx);
        self.logs_mux = Some(logs_mux);

        let (error_logs_rx, error_logs_mux) = LogsMux::spawn(transport);
        self.error_logs_rx = Some(error_logs_rx);
        self.error_logs_mux = Some(error_logs_mux);
    }

    fn stop_streams(&mut self) {
        if let Some(poller) = self.heartbeat_poller.take() {
            poller.stop();
        }
        self.heartbeat_rx = None;
        if let Some(poller) = self.system_df_poller.take() {
            poller.stop();
        }
        self.system_df_rx = None;
        if let Some(poller) = self.system_df_verbose_poller.take() {
            poller.stop();
        }
        self.system_df_verbose_rx = None;
        if let Some(poller) = self.host_stats_poller.take() {
            poller.stop();
        }
        self.host_stats_rx = None;
        if let Some(poller) = self.containers_poller.take() {
            poller.stop();
        }
        self.containers_rx = None;
        if let Some(poller) = self.compose_poller.take() {
            poller.stop();
        }
        self.compose_rx = None;
        if let Some(poller) = self.images_poller.take() {
            poller.stop();
        }
        self.images_rx = None;
        self.stop_inspect_poller();
        if let Some(poller) = self.stats_poller.take() {
            poller.stop();
        }
        self.stats_rx = None;
        if let Some(poller) = self.events_poller.take() {
            poller.stop();
        }
        self.events_rx = None;
        if let Some(poller) = self.volumes_poller.take() {
            poller.stop();
        }
        self.volumes_rx = None;
        if let Some(poller) = self.networks_poller.take() {
            poller.stop();
        }
        self.networks_rx = None;
        if let Some(mux) = self.logs_mux.take() {
            mux.stop();
        }
        self.logs_rx = None;
        if let Some(mux) = self.error_logs_mux.take() {
            mux.stop();
        }
        self.error_logs_rx = None;
    }

    fn clear_context_data(&mut self) {
        self.heartbeat_error = None;
        self.system_df = None;
        self.system_df_error = None;
        self.system_df_verbose = None;
        self.system_df_verbose_error = None;
        self.host_stats = None;
        self.host_stats_error = None;
        self.containers = None;
        self.containers_error = None;
        self.selected_container = None;
        self.selected_compose_service = None;
        self.selection = Selection::None;
        self.compose = None;
        self.compose_error = None;
        self.compose_missing = false;
        self.images = None;
        self.images_error = None;
        self.image_filters = ImageFilters::default();
        self.selected_image = None;
        self.inspect = None;
        self.inspect_error = None;
        self.inspect_target = None;
        self.stats = None;
        self.stats_error = None;
        self.events = None;
        self.events_error = None;
        self.volumes = None;
        self.volumes_error = None;
        self.networks = None;
        self.networks_error = None;
        self.log_lines.clear();
        self.pending_log_lines.clear();
        self.error_lines.clear();
        self.pending_error_lines.clear();
        self.logs_excluded.clear();
        self.error_logs_excluded.clear();
        self.logs_filter.clear();
        self.error_logs_filter.clear();
        self.logs_auto_scroll = true;
        self.error_auto_scroll = true;
        self.logs_show_timestamps = true;
        self.error_show_timestamps = true;
        self.insight_auto_scroll = true;
        self.insight = InsightState::default();
        self.insight_rx = None;
        self.insight_context = None;
    }

    pub fn log_targets(&self) -> Vec<(String, String)> {
        self.containers
            .as_ref()
            .map(|containers| {
                containers
                    .iter()
                    .filter(|container| container.is_log_target())
                    .filter(|container| self.matches_selected_compose_service(container))
                    .map(|container| (container.id.clone(), container.display_name()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn select_container(&mut self, id: String) {
        self.selected_container = Some(id.clone());
        self.selected_compose_service = self.containers.as_ref().and_then(|containers| {
            containers
                .iter()
                .find(|container| container.id == id)
                .and_then(|container| container.compose_service().map(str::to_string))
        });
        self.selection = self.selection_for_container_id(&id);
        self.inspect_target = Some(InspectTarget::Container(id));
        self.sync_log_streams();
        self.sync_inspect_poller();
    }

    pub fn select_image(&mut self, id: String) {
        self.selected_image = Some(id.clone());
        self.selection = Selection::Image { id: id.clone() };
        self.inspect_target = Some(InspectTarget::Image(id));
        self.sync_inspect_poller();
    }

    pub fn toggle_compose_service(&mut self, name: &str) {
        if self.selected_compose_service.as_deref() == Some(name) {
            self.selected_compose_service = None;
            self.selection = self
                .selected_container
                .as_ref()
                .map(|id| Selection::Container { id: id.clone() })
                .unwrap_or(Selection::None);
        } else {
            self.selected_compose_service = Some(name.to_string());
            self.selection = Selection::Service {
                name: name.to_string(),
            };
            if let Some(id) = self.container_id_for_service(name) {
                self.selected_container = Some(id.clone());
                self.inspect_target = Some(InspectTarget::Container(id));
                self.sync_inspect_poller();
            }
        }
        self.sync_log_streams();
    }

    #[allow(dead_code)]
    pub fn selected_service_name(&self) -> Option<&str> {
        self.selection
            .service_name()
            .or(self.selected_compose_service.as_deref())
    }

    #[allow(dead_code)]
    pub fn containers_for_selected_service(&self) -> Vec<&Container> {
        let Some(name) = self.selected_service_name() else {
            return Vec::new();
        };
        self.containers
            .as_deref()
            .map(|containers| selection::containers_for_service(containers, name))
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn project_containers(&self) -> Vec<&Container> {
        let Some(project) = self.compose.as_ref() else {
            return Vec::new();
        };
        self.containers
            .as_deref()
            .map(|containers| selection::project_containers(containers, &project.name))
            .unwrap_or_default()
    }

    pub fn stats_display_name(&self, stats: &ContainerStats) -> String {
        self.container_matching_id(&stats.id)
            .map(Container::display_name)
            .unwrap_or_else(|| stats.name.clone())
    }

    pub fn container_matching_id(&self, id: &str) -> Option<&Container> {
        self.containers.as_ref().and_then(|containers| {
            containers.iter().find(|container| {
                container.id == id || container.id.starts_with(id) || id.starts_with(&container.id)
            })
        })
    }

    pub fn container_matching_name(&self, name: &str) -> Option<&Container> {
        self.containers.as_ref().and_then(|containers| {
            containers.iter().find(|container| {
                container
                    .names
                    .trim_start_matches('/')
                    .split(',')
                    .any(|entry| entry == name)
            })
        })
    }

    pub fn image_in_use(&self, image: &LocalImage) -> usize {
        self.containers
            .as_ref()
            .map(|containers| {
                containers
                    .iter()
                    .filter(|container| image.matches_container_image(&container.image))
                    .count()
            })
            .unwrap_or(0)
    }

    fn matches_selected_compose_service(&self, container: &Container) -> bool {
        match &self.selected_compose_service {
            None => true,
            Some(service) => container.compose_service() == Some(service.as_str()),
        }
    }

    fn container_id_for_service(&self, name: &str) -> Option<String> {
        self.containers.as_ref().and_then(|containers| {
            containers
                .iter()
                .find(|container| container.compose_service() == Some(name))
                .map(|container| container.id.clone())
        })
    }

    pub fn toggle_log_container(&mut self, id: &str) {
        toggle_excluded(&mut self.logs_excluded, id);
        prune_excluded_lines(&mut self.log_lines, &self.logs_excluded);
        prune_excluded_lines(&mut self.pending_log_lines, &self.logs_excluded);
        self.sync_logs_mux();
    }

    pub fn toggle_error_log_container(&mut self, id: &str) {
        toggle_excluded(&mut self.error_logs_excluded, id);
        prune_excluded_lines(&mut self.error_lines, &self.error_logs_excluded);
        prune_excluded_lines(&mut self.pending_error_lines, &self.error_logs_excluded);
        self.sync_error_logs_mux();
    }

    fn sync_log_streams(&mut self) {
        self.sync_logs_mux();
        self.sync_error_logs_mux();
    }

    fn sync_logs_mux(&mut self) {
        let targets = self.mux_targets(&self.logs_excluded);
        if let Some(mux) = self.logs_mux.as_mut() {
            mux.sync(&targets);
        }
    }

    fn sync_error_logs_mux(&mut self) {
        let targets = self.mux_targets(&self.error_logs_excluded);
        if let Some(mux) = self.error_logs_mux.as_mut() {
            mux.sync(&targets);
        }
    }

    fn mux_targets(&self, excluded: &HashSet<String>) -> Vec<LogTarget> {
        self.log_targets()
            .into_iter()
            .filter(|(id, _)| !excluded.contains(id))
            .map(|(id, name)| LogTarget { id, name })
            .collect()
    }

    fn drain_logs(&mut self) -> bool {
        drain_feed(
            self.logs_rx.as_ref(),
            &mut self.log_lines,
            &mut self.pending_log_lines,
            self.logs_auto_scroll,
            |_| true,
        )
    }

    fn drain_error_logs(&mut self) -> bool {
        let had_incoming = self.error_logs_rx.as_ref().is_some_and(|rx| !rx.is_empty());
        let changed = drain_feed(
            self.error_logs_rx.as_ref(),
            &mut self.error_lines,
            &mut self.pending_error_lines,
            self.error_auto_scroll,
            LogLine::is_error_line,
        );
        if had_incoming {
            self.insight.last_error_at = Some(Instant::now());
        }
        changed
    }

    fn request_insight(&mut self) {
        let Some(context) = self.selected_context else {
            return;
        };
        if self.insight.status == InsightStatus::RequestSent {
            return;
        }

        let now = Instant::now();
        let snapshot = build_snapshot(
            self.insight_lines(),
            LevelMask::Error,
            self.insight_host_name(),
            context.as_key(),
            now,
        );
        if snapshot.clusters.is_empty() {
            return;
        }
        let key = snapshot.digest_key();
        let context_id = context.as_key().to_string();
        self.insight.generation = self.insight.generation.wrapping_add(1);
        self.insight.status = InsightStatus::RequestSent;
        self.insight.last_analyze = Some(now);
        self.insight.last_sent_key = Some(key);
        self.insight_context = Some(context_id.clone());
        self.insight_rx = Some(spawn_insight(snapshot, self.insight.generation, context_id));
        tracing::info!(
            generation = self.insight.generation,
            "insight request queued"
        );
    }

    /// Queues an insight POST when recent errors settle or the digest changes.
    fn maybe_request_insight(&mut self) {
        if self.selected_context.is_none() {
            return;
        }
        if self.insight.status == InsightStatus::RequestSent {
            return;
        }
        if !InsightConfig::from_env().has_api_key() {
            return;
        }

        let Some(context) = self.selected_context else {
            return;
        };
        let now = Instant::now();
        let snapshot = build_snapshot(
            self.insight_lines(),
            LevelMask::Error,
            self.insight_host_name(),
            context.as_key(),
            now,
        );
        if snapshot.clusters.is_empty() {
            return;
        }
        let key = snapshot.digest_key();

        let should_send = match self.insight.last_sent_key.as_deref() {
            None => self
                .insight
                .last_error_at
                .is_some_and(|at| at.elapsed() >= INSIGHT_SETTLE),
            Some(prev) => {
                let cooled = self
                    .insight
                    .last_analyze
                    .is_none_or(|at| at.elapsed() >= INSIGHT_COOLDOWN);
                let key_changed = key != prev;
                let high = snapshot.has_new_high_severity(prev);
                if key_changed {
                    high || cooled
                } else {
                    self.insight.status == InsightStatus::RequestFailed && cooled
                }
            }
        };

        if should_send {
            self.request_insight();
        }
    }

    fn drain_insight(&mut self) -> bool {
        let Some(rx) = self.insight_rx.as_ref() else {
            return false;
        };

        let mut updated = false;
        while let Ok(update) = rx.try_recv() {
            let (generation, context_id) = match &update {
                InsightUpdate::Started {
                    generation,
                    context_id,
                }
                | InsightUpdate::Reply {
                    generation,
                    context_id,
                    ..
                }
                | InsightUpdate::Error {
                    generation,
                    context_id,
                    ..
                } => (*generation, context_id.as_str()),
            };
            if generation != self.insight.generation {
                continue;
            }
            if self.insight_context.as_deref() != Some(context_id) {
                continue;
            }
            if self.selected_context.map(ContextId::as_key) != Some(context_id) {
                continue;
            }
            match update {
                InsightUpdate::Started { .. } => {
                    self.insight.status = InsightStatus::RequestSent;
                    updated = true;
                }
                InsightUpdate::Reply { text, .. } => {
                    self.insight.replies.push_back(text);
                    while self.insight.replies.len() > MAX_INSIGHTS {
                        self.insight.replies.pop_front();
                    }
                    self.insight.status = InsightStatus::Idle;
                    self.insight.ever_succeeded = true;
                    tracing::info!(stored = self.insight.replies.len(), "insight reply stored");
                    updated = true;
                }
                InsightUpdate::Error { .. } => {
                    self.insight.status = InsightStatus::RequestFailed;
                    if !self.insight.ever_succeeded {
                        self.insight.last_sent_key = None;
                        self.insight.last_error_at = Some(Instant::now());
                    }
                    updated = true;
                }
            }
        }
        updated
    }

    fn insight_lines(&self) -> impl Iterator<Item = InsightLine> + '_ {
        self.error_lines.iter().map(|line| InsightLine {
            received_at: line.received_at,
            level: insight_level(line.level),
            tag: line.container_name.clone(),
            message: line.message.clone(),
        })
    }

    fn insight_host_name(&self) -> &str {
        self.local.engine_version.as_deref().unwrap_or("Docker")
    }

    fn prune_selected_container(&mut self) {
        let Some(id) = self.selected_container.clone() else {
            return;
        };
        let still_present = self
            .containers
            .as_ref()
            .is_some_and(|containers| containers.iter().any(|container| container.id == id));
        if !still_present {
            self.selected_container = None;
            if matches!(&self.selection, Selection::Container { id: selected } if *selected == id) {
                self.selection = Selection::None;
            }
            if matches!(&self.inspect_target, Some(InspectTarget::Container(target)) if *target == id)
            {
                self.inspect_target = None;
                self.inspect = None;
                self.sync_inspect_poller();
            }
        }
    }

    fn prune_selected_image(&mut self) {
        let Some(id) = self.selected_image.clone() else {
            return;
        };
        let still_present = self.images.as_ref().is_some_and(|images| {
            images
                .iter()
                .any(|image| image.id == id || image.short_id() == id)
        });
        if !still_present {
            self.selected_image = None;
            if matches!(&self.selection, Selection::Image { id: selected } if *selected == id) {
                self.selection = Selection::None;
            }
            if matches!(&self.inspect_target, Some(InspectTarget::Image(target)) if *target == id) {
                self.inspect_target = None;
                self.inspect = None;
                self.sync_inspect_poller();
            }
        }
    }

    fn sync_inspect_poller(&mut self) {
        if self.inspect_poller_for == self.inspect_target {
            return;
        }
        self.stop_inspect_poller();
        self.inspect = None;
        self.inspect_error = None;

        let Some(context) = self.selected_context else {
            return;
        };
        let Some(target) = self.inspect_target.clone() else {
            return;
        };
        let fetch_target = target.clone();
        match Poller::spawn(context.transport(), INSPECT_INTERVAL, move |transport| {
            Docker::inspect(transport, &fetch_target)
        }) {
            Ok((rx, poller)) => {
                self.inspect_rx = Some(rx);
                self.inspect_poller = Some(poller);
                self.inspect_poller_for = Some(target);
            }
            Err(err) => self.inspect_error = Some(err.user_message()),
        }
    }

    fn stop_inspect_poller(&mut self) {
        if let Some(poller) = self.inspect_poller.take() {
            poller.stop();
        }
        self.inspect_rx = None;
        self.inspect_poller_for = None;
    }

    fn prune_selected_compose_service(&mut self) {
        let Some(name) = self.selected_compose_service.clone() else {
            return;
        };
        let still_present = self
            .compose
            .as_ref()
            .is_some_and(|project| project.services.iter().any(|service| service.name == name));
        if !still_present {
            self.selected_compose_service = None;
            if matches!(&self.selection, Selection::Service { name: selected } if *selected == name)
            {
                self.selection = self
                    .selected_container
                    .as_ref()
                    .map(|id| Selection::Container { id: id.clone() })
                    .unwrap_or(Selection::None);
            }
        }
    }

    fn selection_for_container_id(&self, id: &str) -> Selection {
        match self.containers.as_ref().and_then(|containers| {
            containers
                .iter()
                .find(|container| container.id == id)
                .and_then(Container::compose_service)
        }) {
            Some(name) => Selection::Service {
                name: name.to_string(),
            },
            None => Selection::Container { id: id.to_string() },
        }
    }

    fn drain_compose(&mut self) -> bool {
        let Some(rx) = &self.compose_rx else {
            return false;
        };
        let mut changed = false;
        while let Ok(update) = rx.try_recv() {
            changed = true;
            match update {
                Ok(Some(project)) => {
                    self.compose = Some(project);
                    self.compose_missing = false;
                    self.compose_error = None;
                }
                Ok(None) => {
                    self.compose = None;
                    self.compose_missing = true;
                    self.compose_error = None;
                }
                Err(message) => self.compose_error = Some(message),
            }
        }
        changed
    }

    fn drain_heartbeat(&mut self) -> bool {
        let Some(rx) = &self.heartbeat_rx else {
            return false;
        };
        let mut changed = false;
        while let Ok(update) = rx.try_recv() {
            changed = true;
            match update {
                HeartbeatUpdate::Alive { engine_version } => {
                    self.heartbeat_error = None;
                    if self.local.reachable {
                        self.local.engine_version = Some(engine_version);
                    }
                }
                HeartbeatUpdate::Error(message) => {
                    self.heartbeat_error = Some(message);
                }
            }
        }
        changed
    }
}

fn insight_level(level: LogLevel) -> char {
    match level {
        LogLevel::Fatal => 'F',
        _ => 'E',
    }
}

fn drain_result<T>(
    rx: &Option<Receiver<Result<T, String>>>,
    value: &mut Option<T>,
    error: &mut Option<String>,
) -> bool {
    let Some(rx) = rx else {
        return false;
    };
    let mut changed = false;
    while let Ok(update) = rx.try_recv() {
        changed = true;
        match update {
            Ok(next) => {
                *value = Some(next);
                *error = None;
            }
            Err(message) => *error = Some(message),
        }
    }
    changed
}

fn drain_feed(
    rx: Option<&Receiver<LogLine>>,
    lines: &mut VecDeque<LogLine>,
    pending: &mut VecDeque<LogLine>,
    auto_scroll: bool,
    keep: impl Fn(&LogLine) -> bool,
) -> bool {
    let incoming: Vec<LogLine> = take_log_lines(rx).into_iter().filter(keep).collect();
    if incoming.is_empty() && pending.is_empty() {
        return false;
    }

    if auto_scroll {
        let mut updated = false;
        if !pending.is_empty() {
            lines.extend(pending.drain(..));
            updated = true;
        }
        if !incoming.is_empty() {
            lines.extend(incoming);
            updated = true;
        }
        if updated {
            trim_buffer(lines);
        }
        updated
    } else {
        if !incoming.is_empty() {
            pending.extend(incoming);
            trim_buffer(pending);
        }
        false
    }
}

fn take_log_lines(rx: Option<&Receiver<LogLine>>) -> Vec<LogLine> {
    let Some(rx) = rx else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    while let Ok(line) = rx.try_recv() {
        lines.push(line);
    }
    lines
}

fn toggle_excluded(excluded: &mut HashSet<String>, id: &str) {
    if !excluded.remove(id) {
        excluded.insert(id.to_string());
    }
}

fn prune_excluded_lines(buffer: &mut VecDeque<LogLine>, excluded: &HashSet<String>) {
    buffer.retain(|line| line.container_id.is_empty() || !excluded.contains(&line.container_id));
}

fn trim_buffer(buffer: &mut VecDeque<LogLine>) {
    while buffer.len() > MAX_LOG_LINES {
        buffer.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use compose_client::{ComposeProject, ComposeService, ProjectStatus};
    use docker_client::Container;

    use super::*;

    fn container(id: &str, names: &str, labels: &str) -> Container {
        Container {
            id: id.into(),
            names: names.into(),
            image: "dd-mock".into(),
            state: "running".into(),
            status: "Up".into(),
            ports: String::new(),
            created_at: String::new(),
            running_for: String::new(),
            labels: labels.into(),
        }
    }

    fn compose_project(services: &[&str]) -> ComposeProject {
        ComposeProject {
            name: "dd-mock".into(),
            config_file: String::new(),
            working_dir: String::new(),
            status: ProjectStatus::Running,
            status_label: "running".into(),
            services: services
                .iter()
                .map(|name| ComposeService {
                    name: (*name).into(),
                    container_id: None,
                    image: String::new(),
                    desired: 1,
                    current: "running".into(),
                    health: String::new(),
                    restarts: None,
                    ports: String::new(),
                    depends_on: String::new(),
                })
                .collect(),
            resolved_services: services.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    #[test]
    fn starts_in_investigate_with_no_selection() {
        let app = App::new();
        assert_eq!(app.mode, AppMode::Investigate);
        assert_eq!(app.selection, Selection::None);
    }

    #[test]
    fn selecting_compose_container_selects_the_service() {
        let mut app = App::new();
        app.containers = Some(vec![container(
            "abc",
            "/dd-mock-worker-1",
            "com.docker.compose.project=dd-mock,com.docker.compose.service=worker",
        )]);
        app.select_container("abc".into());

        assert_eq!(app.selected_container.as_deref(), Some("abc"));
        assert_eq!(app.selected_compose_service.as_deref(), Some("worker"));
        assert_eq!(
            app.selection,
            Selection::Service {
                name: "worker".into()
            }
        );
        assert_eq!(app.selected_service_name(), Some("worker"));
        assert_eq!(app.containers_for_selected_service().len(), 1);
    }

    #[test]
    fn selecting_unlabeled_container_selects_the_container() {
        let mut app = App::new();
        app.containers = Some(vec![container("abc", "/orphan", "")]);
        app.select_container("abc".into());

        assert_eq!(app.selection, Selection::Container { id: "abc".into() });
        assert!(app.selected_service_name().is_none());
        assert!(app.containers_for_selected_service().is_empty());
    }

    #[test]
    fn toggling_compose_service_updates_selection() {
        let mut app = App::new();
        app.containers = Some(vec![container(
            "abc",
            "/dd-mock-api-1",
            "com.docker.compose.project=dd-mock,com.docker.compose.service=api",
        )]);
        app.toggle_compose_service("api");
        assert_eq!(app.selection, Selection::Service { name: "api".into() });

        app.toggle_compose_service("api");
        assert!(app.selected_compose_service.is_none());
        assert_eq!(app.selection, Selection::Container { id: "abc".into() });
    }

    #[test]
    fn project_containers_match_compose_labels() {
        let mut app = App::new();
        app.compose = Some(compose_project(&["api", "worker"]));
        app.containers = Some(vec![
            container(
                "1",
                "/dd-mock-api-1",
                "com.docker.compose.project=dd-mock,com.docker.compose.service=api",
            ),
            container("2", "/orphan", ""),
        ]);
        let project = app.project_containers();
        assert_eq!(project.len(), 1);
        assert_eq!(project[0].id, "1");
    }

    #[test]
    fn prune_clears_stale_selection() {
        let mut app = App::new();
        app.containers = Some(vec![container("abc", "/orphan", "")]);
        app.select_container("abc".into());
        app.containers = Some(Vec::new());
        app.prune_selected_container();
        assert!(app.selected_container.is_none());
        assert_eq!(app.selection, Selection::None);

        app.compose = Some(compose_project(&["worker"]));
        app.toggle_compose_service("worker");
        app.compose = Some(compose_project(&[]));
        app.prune_selected_compose_service();
        assert!(app.selected_compose_service.is_none());
        assert_eq!(app.selection, Selection::None);
    }
}
