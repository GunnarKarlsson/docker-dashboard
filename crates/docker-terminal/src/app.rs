//! Application state: context selection drives pollers.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use compose_client::{Compose, ComposeProject};
use crossbeam_channel::Receiver;
use docker_client::{
    fetch_host_stats, Container, Docker, HeartbeatPoller, HeartbeatUpdate, HostStats,
    InspectReport, InspectTarget, LocalImage, LogLine, LogTarget, LogsMux, Poller, SystemDf,
    SystemDfVerbose, Transport,
};
use eframe::egui;

const REPAINT_INTERVAL: Duration = Duration::from_millis(200);
const SYSTEM_DF_INTERVAL: Duration = Duration::from_secs(10);
const SYSTEM_DF_VERBOSE_INTERVAL: Duration = Duration::from_secs(20);
const HOST_STATS_INTERVAL: Duration = Duration::from_secs(5);
const CONTAINERS_INTERVAL: Duration = Duration::from_secs(2);
const COMPOSE_INTERVAL: Duration = Duration::from_secs(3);
const IMAGES_INTERVAL: Duration = Duration::from_secs(10);
const INSPECT_INTERVAL: Duration = Duration::from_secs(3);
pub const MAX_LOG_LINES: usize = 10_000;

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

pub struct App {
    pub local: LocalContext,
    pub contexts_refreshed_at: Option<Instant>,
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
    logs_rx: Option<Receiver<LogLine>>,
    logs_mux: Option<LogsMux>,
    error_logs_rx: Option<Receiver<LogLine>>,
    error_logs_mux: Option<LogsMux>,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            local: LocalContext::unreachable("Not checked"),
            contexts_refreshed_at: None,
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
            logs_rx: None,
            logs_mux: None,
            error_logs_rx: None,
            error_logs_mux: None,
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
        if self.drain_logs() {
            needs_repaint = true;
        }
        if self.drain_error_logs() {
            needs_repaint = true;
        }
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
        self.inspect_target = Some(InspectTarget::Container(id));
        self.sync_log_streams();
        self.sync_inspect_poller();
    }

    pub fn select_image(&mut self, id: String) {
        self.selected_image = Some(id.clone());
        self.inspect_target = Some(InspectTarget::Image(id));
        self.sync_inspect_poller();
    }

    pub fn toggle_compose_service(&mut self, name: &str) {
        if self.selected_compose_service.as_deref() == Some(name) {
            self.selected_compose_service = None;
        } else {
            self.selected_compose_service = Some(name.to_string());
            if let Some(id) = self.container_id_for_service(name) {
                self.selected_container = Some(id.clone());
                self.inspect_target = Some(InspectTarget::Container(id));
                self.sync_inspect_poller();
            }
        }
        self.sync_log_streams();
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
        drain_feed(
            self.error_logs_rx.as_ref(),
            &mut self.error_lines,
            &mut self.pending_error_lines,
            self.error_auto_scroll,
            LogLine::is_error_line,
        )
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
        let Some(name) = &self.selected_compose_service else {
            return;
        };
        let still_present = self
            .compose
            .as_ref()
            .is_some_and(|project| project.services.iter().any(|service| service.name == *name));
        if !still_present {
            self.selected_compose_service = None;
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
