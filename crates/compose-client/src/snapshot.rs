use std::collections::HashMap;
use std::path::Path;

use docker_client::{Docker, DockerError, Transport};
use serde::Deserialize;

const PREFERRED_PROJECT: &str = "dd-mock";
const LABEL_WORKING_DIR: &str = "com.docker.compose.project.working_dir";
const LABEL_IMAGE: &str = "com.docker.compose.image";
const LABEL_DEPENDS_ON: &str = "com.docker.compose.depends_on";

/// High-level project status for the Compose panel header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStatus {
    Running,
    Partial,
    Stopped,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Partial => "partial",
            Self::Stopped => "stopped",
        }
    }

    fn from_services(services: &[ComposeService]) -> Self {
        if services.is_empty() {
            return Self::Stopped;
        }
        let all_running = services.iter().all(|service| service.current == "running");
        let any_up = services
            .iter()
            .any(|service| service.current == "running" || service.current == "restarting");
        if all_running {
            Self::Running
        } else if any_up {
            Self::Partial
        } else {
            Self::Stopped
        }
    }
}

/// A discovered Compose project and its services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeProject {
    pub name: String,
    pub config_file: String,
    pub working_dir: String,
    pub status: ProjectStatus,
    pub status_label: String,
    pub services: Vec<ComposeService>,
    pub resolved_services: Vec<String>,
}

/// One service row in the Compose panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeService {
    pub name: String,
    pub container_id: Option<String>,
    pub image: String,
    pub desired: u32,
    pub current: String,
    pub health: String,
    pub restarts: Option<u64>,
    pub ports: String,
    pub depends_on: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListedProject {
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    config_files: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ComposePsRow {
    #[serde(rename = "ID")]
    id: String,
    #[serde(default)]
    service: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    health: String,
    #[serde(default)]
    ports: String,
    #[serde(default)]
    labels: String,
    #[serde(default)]
    publishers: Vec<Publisher>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Publisher {
    #[serde(default)]
    target_port: u16,
    #[serde(default)]
    published_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ComposeImage {
    #[serde(rename = "ID")]
    id: String,
    #[serde(default)]
    container_name: String,
    #[serde(default)]
    repository: String,
    #[serde(default)]
    tag: String,
}

#[derive(Debug, Deserialize)]
struct InspectRow {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "RestartCount")]
    restart_count: u64,
    #[serde(rename = "State", default)]
    state: InspectState,
}

#[derive(Debug, Default, Deserialize)]
struct InspectState {
    #[serde(rename = "Health", default)]
    health: Option<InspectHealth>,
}

#[derive(Debug, Deserialize)]
struct InspectHealth {
    #[serde(rename = "Status")]
    status: String,
}

pub(crate) fn load_project(transport: &Transport) -> Result<Option<ComposeProject>, DockerError> {
    let listed = list_projects(transport)?;
    let Some(listed) = pick_project(listed) else {
        return Ok(None);
    };

    let config_file = listed
        .config_files
        .split([',', ':'])
        .map(str::trim)
        .find(|path| !path.is_empty())
        .unwrap_or_default()
        .to_string();

    let file_args = compose_file_args(&listed.name, &config_file);
    let ps_rows = compose_ps(transport, &file_args)?;
    let resolved_services = compose_services(transport, &file_args)?;
    let images = compose_images(transport, &file_args).unwrap_or_default();
    let restarts = inspect_restarts(transport, &ps_rows).unwrap_or_default();

    Ok(Some(build_project(
        listed,
        config_file,
        ps_rows,
        resolved_services,
        images,
        restarts,
    )))
}

fn pick_project(projects: Vec<ListedProject>) -> Option<ListedProject> {
    projects
        .iter()
        .find(|project| project.name == PREFERRED_PROJECT)
        .cloned()
        .or_else(|| projects.into_iter().next())
}

fn compose_file_args<'a>(name: &'a str, config_file: &'a str) -> Vec<&'a str> {
    if config_file.is_empty() {
        vec!["-p", name]
    } else {
        vec!["-f", config_file]
    }
}

fn list_projects(transport: &Transport) -> Result<Vec<ListedProject>, DockerError> {
    let output = Docker::run(transport, &["compose", "ls", "--format", "json"])?;
    parse_ls(&String::from_utf8_lossy(&output.stdout))
}

fn compose_ps(transport: &Transport, file_args: &[&str]) -> Result<Vec<ComposePsRow>, DockerError> {
    let mut args = vec!["compose"];
    args.extend_from_slice(file_args);
    args.extend(["ps", "-a", "--format", "json"]);
    let output = Docker::run(transport, &args)?;
    parse_ps(&String::from_utf8_lossy(&output.stdout))
}

fn compose_services(transport: &Transport, file_args: &[&str]) -> Result<Vec<String>, DockerError> {
    let mut args = vec!["compose"];
    args.extend_from_slice(file_args);
    args.extend(["config", "--services"]);
    let output = Docker::run(transport, &args)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn compose_images(
    transport: &Transport,
    file_args: &[&str],
) -> Result<Vec<ComposeImage>, DockerError> {
    let mut args = vec!["compose"];
    args.extend_from_slice(file_args);
    args.extend(["images", "--format", "json"]);
    let output = Docker::run(transport, &args)?;
    parse_images(&String::from_utf8_lossy(&output.stdout))
}

fn inspect_restarts(
    transport: &Transport,
    rows: &[ComposePsRow],
) -> Result<HashMap<String, (u64, Option<String>)>, DockerError> {
    let ids: Vec<&str> = rows
        .iter()
        .map(|row| row.id.as_str())
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut args = vec!["inspect"];
    args.extend(ids);
    let output = Docker::run(transport, &args)?;
    parse_inspect(&String::from_utf8_lossy(&output.stdout))
}

fn build_project(
    listed: ListedProject,
    config_file: String,
    ps_rows: Vec<ComposePsRow>,
    resolved_services: Vec<String>,
    images: Vec<ComposeImage>,
    restarts: HashMap<String, (u64, Option<String>)>,
) -> ComposeProject {
    let working_dir = working_dir_from(&ps_rows, &config_file);
    let mut names = resolved_services.clone();
    for row in &ps_rows {
        if !row.service.is_empty() && !names.iter().any(|name| name == &row.service) {
            names.push(row.service.clone());
        }
    }

    let services = names
        .iter()
        .map(|name| {
            let row = ps_rows.iter().find(|row| row.service == *name);
            service_from_row(name, row, &images, &restarts)
        })
        .collect::<Vec<_>>();

    ComposeProject {
        name: listed.name,
        config_file,
        working_dir,
        status: ProjectStatus::from_services(&services),
        status_label: listed.status,
        services,
        resolved_services,
    }
}

fn service_from_row(
    name: &str,
    row: Option<&ComposePsRow>,
    images: &[ComposeImage],
    restarts: &HashMap<String, (u64, Option<String>)>,
) -> ComposeService {
    let Some(row) = row else {
        return ComposeService {
            name: name.to_string(),
            container_id: None,
            image: String::new(),
            desired: 1,
            current: "missing".to_string(),
            health: String::new(),
            restarts: None,
            ports: String::new(),
            depends_on: String::new(),
        };
    };

    let inspect = restarts.iter().find(|(id, _)| id.starts_with(&row.id));
    let restart_count = inspect.map(|(_, (count, _))| *count);
    let inspect_health = inspect.and_then(|(_, (_, health))| health.clone());
    let health = inspect_health
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if row.health.is_empty() {
                None
            } else {
                Some(row.health.clone())
            }
        })
        .unwrap_or_default();

    ComposeService {
        name: name.to_string(),
        container_id: Some(row.id.clone()),
        image: image_label(row, images),
        desired: 1,
        current: row.state.to_ascii_lowercase(),
        health,
        restarts: restart_count,
        ports: format_ports(row),
        depends_on: depends_on_from_labels(&row.labels),
    }
}

fn image_label(row: &ComposePsRow, images: &[ComposeImage]) -> String {
    let image = images
        .iter()
        .find(|image| image.repository == row.image)
        .or_else(|| {
            images.iter().find(|image| {
                !image.container_name.is_empty() && image.container_name.contains(&row.service)
            })
        });

    if let Some(image) = image {
        let tag = if image.tag.is_empty() {
            "latest".to_string()
        } else {
            image.tag.clone()
        };
        let repo = if image.repository.is_empty() {
            row.image.clone()
        } else {
            image.repository.clone()
        };
        return format!("{repo}:{tag} {}", short_digest(&image.id));
    }

    let digest = label_value(&row.labels, LABEL_IMAGE)
        .map(short_digest)
        .unwrap_or_default();
    if digest.is_empty() {
        row.image.clone()
    } else if row.image.is_empty() {
        digest
    } else {
        format!("{} {digest}", row.image)
    }
}

fn format_ports(row: &ComposePsRow) -> String {
    if !row.publishers.is_empty() {
        let mut seen = Vec::new();
        for publisher in &row.publishers {
            if publisher.published_port == 0 {
                continue;
            }
            let text = format!("{}→{}", publisher.published_port, publisher.target_port);
            if !seen.contains(&text) {
                seen.push(text);
            }
        }
        if !seen.is_empty() {
            return seen.join(", ");
        }
    }
    row.ports.clone()
}

fn working_dir_from(rows: &[ComposePsRow], config_file: &str) -> String {
    for row in rows {
        if let Some(dir) = label_value(&row.labels, LABEL_WORKING_DIR) {
            return dir.to_string();
        }
    }
    Path::new(config_file)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn depends_on_from_labels(labels: &str) -> String {
    let Some(raw) = label_value(labels, LABEL_DEPENDS_ON) else {
        return String::new();
    };
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.split(':').next().unwrap_or(part))
        .collect::<Vec<_>>()
        .join(", ")
}

fn label_value<'a>(labels: &'a str, key: &str) -> Option<&'a str> {
    for part in labels.split(',') {
        let part = part.trim();
        if let Some(value) = part
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
        {
            if value.is_empty() {
                return None;
            }
            return Some(value);
        }
    }
    None
}

fn short_digest(id: &str) -> String {
    let hex = id.strip_prefix("sha256:").unwrap_or(id);
    hex.chars().take(12).collect()
}

fn parse_ls(text: &str) -> Result<Vec<ListedProject>, DockerError> {
    parse_json_list(text, "compose ls")
}

fn parse_ps(text: &str) -> Result<Vec<ComposePsRow>, DockerError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|err| DockerError::ParseFailed(format!("compose ps: {err}")))
    } else {
        parse_ndjson(trimmed, "compose ps")
    }
}

fn parse_images(text: &str) -> Result<Vec<ComposeImage>, DockerError> {
    parse_json_list(text, "compose images")
}

fn parse_inspect(text: &str) -> Result<HashMap<String, (u64, Option<String>)>, DockerError> {
    let rows: Vec<InspectRow> = serde_json::from_str(text.trim())
        .map_err(|err| DockerError::ParseFailed(format!("docker inspect: {err}")))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let health = row.state.health.map(|health| health.status);
            (row.id, (row.restart_count, health))
        })
        .collect())
}

fn parse_json_list<T: for<'de> Deserialize<'de>>(
    text: &str,
    label: &str,
) -> Result<Vec<T>, DockerError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|err| DockerError::ParseFailed(format!("{label}: {err}")))
    } else {
        parse_ndjson(trimmed, label)
    }
}

fn parse_ndjson<T: for<'de> Deserialize<'de>>(
    text: &str,
    label: &str,
) -> Result<Vec<T>, DockerError> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row = serde_json::from_str(line).map_err(|err| {
            DockerError::ParseFailed(format!("{label} line {}: {err}", index + 1))
        })?;
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{
        build_project, parse_images, parse_inspect, parse_ls, parse_ps, ListedProject,
        ProjectStatus,
    };

    #[test]
    fn parses_compose_ls_array() {
        let projects = parse_ls(
            r#"[{"Name":"dd-mock","Status":"restarting(1), running(3)","ConfigFiles":"/tmp/docker-compose.yml"}]"#,
        )
        .expect("ls json");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "dd-mock");
        assert!(projects[0].status.contains("restarting"));
    }

    #[test]
    fn parses_compose_ps_ndjson_health() {
        let rows = parse_ps(
            r#"{"ID":"abc123def456","Service":"unhealthy","Image":"dd-mock-unhealthy","State":"running","Health":"unhealthy","Ports":"","Labels":"com.docker.compose.service=unhealthy","Publishers":[]}
{"ID":"aaa111bbb222","Service":"worker","Image":"dd-mock-worker","State":"restarting","Health":"","Ports":"","Labels":"com.docker.compose.service=worker","Publishers":[]}
{"ID":"ccc333ddd444","Service":"api","Image":"dd-mock-api","State":"running","Health":"","Ports":"0.0.0.0:18080->8080/tcp","Labels":"com.docker.compose.project.working_dir=/tmp/mock,com.docker.compose.image=sha256:6d1204af47e8549e5784d1879dc36c1c7ade3a136d197663ae0ad7a35fb2ac34","Publishers":[{"URL":"0.0.0.0","TargetPort":8080,"PublishedPort":18080,"Protocol":"tcp"}]}"#,
        )
        .expect("ps ndjson");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].health, "unhealthy");
        assert_eq!(rows[1].state, "restarting");
        assert_eq!(rows[2].service, "api");
    }

    #[test]
    fn parses_compose_images_array() {
        let images = parse_images(
            r#"[{"ID":"sha256:6d1204af47e8549e5784d1879dc36c1c7ade3a136d197663ae0ad7a35fb2ac34","ContainerName":"dd-mock-api-1","Repository":"dd-mock-api","Tag":"latest","Size":8824817}]"#,
        )
        .expect("images json");
        assert_eq!(images[0].repository, "dd-mock-api");
        assert_eq!(images[0].tag, "latest");
    }

    #[test]
    fn parses_inspect_restart_and_health() {
        let map = parse_inspect(
            r#"[{"Id":"abc123def456ffff","RestartCount":4,"State":{"Health":{"Status":"unhealthy"}}},{"Id":"aaa111bbb222ffff","RestartCount":96,"State":{}}]"#,
        )
        .expect("inspect json");
        assert_eq!(
            map.get("abc123def456ffff")
                .map(|(count, health)| (*count, health.as_deref())),
            Some((4, Some("unhealthy")))
        );
        assert_eq!(
            map.get("aaa111bbb222ffff").map(|(count, _)| *count),
            Some(96)
        );
    }

    #[test]
    fn project_status_partial_when_worker_restarting() {
        let listed = ListedProject {
            name: "dd-mock".into(),
            status: "restarting(1), running(3)".into(),
            config_files: "/tmp/docker-compose.yml".into(),
        };
        let ps = parse_ps(
            r#"{"ID":"apiid","Service":"api","Image":"dd-mock-api","State":"running","Health":"","Ports":"","Labels":"","Publishers":[]}
{"ID":"workid","Service":"worker","Image":"dd-mock-worker","State":"restarting","Health":"","Ports":"","Labels":"","Publishers":[]}
{"ID":"unhid","Service":"unhealthy","Image":"dd-mock-unhealthy","State":"running","Health":"unhealthy","Ports":"","Labels":"","Publishers":[]}"#,
        )
        .unwrap();
        let project = build_project(
            listed,
            "/tmp/docker-compose.yml".into(),
            ps,
            vec!["api".into(), "worker".into(), "unhealthy".into()],
            Vec::new(),
            Default::default(),
        );
        assert_eq!(project.status, ProjectStatus::Partial);
        assert_eq!(project.services[0].current, "running");
        assert_eq!(project.services[1].current, "restarting");
        assert_eq!(project.services[2].health, "unhealthy");
        assert_eq!(project.services[0].name, "api");
    }

    #[test]
    fn image_row_uses_tag_and_short_digest() {
        let listed = ListedProject {
            name: "dd-mock".into(),
            status: "running(1)".into(),
            config_files: "/tmp/docker-compose.yml".into(),
        };
        let ps = parse_ps(
            r#"{"ID":"apiid","Service":"api","Image":"dd-mock-api","State":"running","Health":"","Ports":"","Labels":"","Publishers":[]}"#,
        )
        .unwrap();
        let images = parse_images(
            r#"[{"ID":"sha256:6d1204af47e8549e5784d1879dc36c1c7ade3a136d197663ae0ad7a35fb2ac34","ContainerName":"dd-mock-api-1","Repository":"dd-mock-api","Tag":"latest"}]"#,
        )
        .unwrap();
        let project = build_project(
            listed,
            "/tmp/docker-compose.yml".into(),
            ps,
            vec!["api".into()],
            images,
            Default::default(),
        );
        assert_eq!(project.services[0].image, "dd-mock-api:latest 6d1204af47e8");
        assert_eq!(project.status, ProjectStatus::Running);
    }
}
