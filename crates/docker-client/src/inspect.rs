use serde_json::Value;

use crate::error::DockerError;

const MASK: &str = "••••";

/// What `docker inspect` should target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectTarget {
    Container(String),
    Image(String),
}

impl InspectTarget {
    pub fn id(&self) -> &str {
        match self {
            Self::Container(id) | Self::Image(id) => id,
        }
    }
}

/// Container vs image inspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectKind {
    Container,
    Image,
}

/// JSON-lite inspect view for the Inspect panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectReport {
    pub kind: InspectKind,
    pub title: String,
    pub id: String,
    pub image_digest: String,
    pub command: String,
    pub entrypoint: String,
    pub env: Vec<(String, String)>,
    pub mounts: Vec<String>,
    pub ports: Vec<String>,
    pub restart_policy: String,
    pub networks: Vec<String>,
    pub health_status: String,
    pub health_output: String,
    pub started_at: String,
    pub finished_at: String,
    pub exit_code: Option<i64>,
    pub state: String,
    pub compose_project: String,
    pub compose_service: String,
    pub compose_workdir: String,
}

impl InspectReport {
    /// Parse `docker inspect` JSON (array or single object).
    pub fn from_json(text: &str) -> Result<Self, DockerError> {
        let value: Value = serde_json::from_str(text.trim())
            .map_err(|err| DockerError::ParseFailed(format!("docker inspect: {err}")))?;
        let object = match value {
            Value::Array(mut items) => items.pop().ok_or_else(|| {
                DockerError::ParseFailed("docker inspect returned no objects".into())
            })?,
            object @ Value::Object(_) => object,
            _ => {
                return Err(DockerError::ParseFailed(
                    "docker inspect: expected JSON object or array".into(),
                ));
            }
        };
        Ok(from_object(&object))
    }
}

fn from_object(value: &Value) -> InspectReport {
    if value.pointer("/State/Status").is_some() || value.get("HostConfig").is_some() {
        from_container(value)
    } else {
        from_image(value)
    }
}

fn from_container(value: &Value) -> InspectReport {
    let config = value.get("Config").unwrap_or(&Value::Null);
    let host = value.get("HostConfig").unwrap_or(&Value::Null);
    let state = value.get("State").unwrap_or(&Value::Null);
    let net = value.get("NetworkSettings").unwrap_or(&Value::Null);
    let labels = config.get("Labels").unwrap_or(&Value::Null);
    let health = state.get("Health").unwrap_or(&Value::Null);

    let name = str_field(value, "Name").trim_start_matches('/').to_string();
    let image_digest = str_field(value, "Image");
    let cmd = join_cmd(config.get("Cmd")).or_else(|| {
        let path = str_field(value, "Path");
        let args = join_cmd(value.get("Args"));
        if path.is_empty() {
            args
        } else {
            Some(match args {
                Some(args) => format!("{path} {args}"),
                None => path,
            })
        }
    });

    InspectReport {
        kind: InspectKind::Container,
        title: if name.is_empty() {
            str_field(value, "Id").chars().take(12).collect()
        } else {
            name
        },
        id: short_id(&str_field(value, "Id")),
        image_digest: short_digest(&image_digest),
        command: cmd.unwrap_or_default(),
        entrypoint: join_cmd(config.get("Entrypoint")).unwrap_or_default(),
        env: parse_env(config.get("Env")),
        mounts: parse_mounts(value.get("Mounts")),
        ports: parse_ports(net.get("Ports")),
        restart_policy: restart_policy(host.get("RestartPolicy")),
        networks: parse_networks(net.get("Networks")),
        health_status: str_field(health, "Status"),
        health_output: last_health_output(health),
        started_at: str_field(state, "StartedAt"),
        finished_at: str_field(state, "FinishedAt"),
        exit_code: state.get("ExitCode").and_then(Value::as_i64),
        state: str_field(state, "Status"),
        compose_project: label(labels, "com.docker.compose.project"),
        compose_service: label(labels, "com.docker.compose.service"),
        compose_workdir: label(labels, "com.docker.compose.project.working_dir"),
    }
}

fn from_image(value: &Value) -> InspectReport {
    let config = value.get("Config").unwrap_or(&Value::Null);
    let tags = value
        .get("RepoTags")
        .and_then(Value::as_array)
        .and_then(|tags| tags.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let digest = value
        .get("RepoDigests")
        .and_then(Value::as_array)
        .and_then(|digests| digests.first())
        .and_then(Value::as_str)
        .map(short_digest)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| short_digest(&str_field(value, "Id")));

    InspectReport {
        kind: InspectKind::Image,
        title: if tags.is_empty() {
            short_id(&str_field(value, "Id"))
        } else {
            tags
        },
        id: short_id(&str_field(value, "Id")),
        image_digest: digest,
        command: join_cmd(config.get("Cmd")).unwrap_or_default(),
        entrypoint: join_cmd(config.get("Entrypoint")).unwrap_or_default(),
        env: parse_env(config.get("Env")),
        mounts: Vec::new(),
        ports: Vec::new(),
        restart_policy: String::new(),
        networks: Vec::new(),
        health_status: String::new(),
        health_output: String::new(),
        started_at: str_field(value, "Created"),
        finished_at: String::new(),
        exit_code: None,
        state: String::new(),
        compose_project: String::new(),
        compose_service: String::new(),
        compose_workdir: String::new(),
    }
}

fn parse_env(value: Option<&Value>) -> Vec<(String, String)> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .map(|entry| match entry.split_once('=') {
            Some((key, value)) => {
                let display = if is_secret_env_key(key) {
                    MASK.to_string()
                } else {
                    value.to_string()
                };
                (key.to_string(), display)
            }
            None => (entry.to_string(), String::new()),
        })
        .collect()
}

pub fn is_secret_env_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    key.contains("PASSWORD")
        || key.contains("SECRET")
        || key.contains("TOKEN")
        || key.contains("CREDENTIAL")
        || key.contains("PRIVATE")
        || key.ends_with("_KEY")
        || key.contains("APIKEY")
        || key.contains("API_KEY")
}

fn parse_mounts(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .map(|mount| {
            let source = str_field(mount, "Source");
            let source = if source.is_empty() {
                str_field(mount, "Name")
            } else {
                source
            };
            let dest = str_field(mount, "Destination");
            let mode = if mount.get("RW").and_then(Value::as_bool) == Some(false) {
                "ro"
            } else {
                "rw"
            };
            if dest.is_empty() {
                source
            } else {
                format!("{source} → {dest} ({mode})")
            }
        })
        .filter(|line| !line.is_empty())
        .collect()
}

fn parse_ports(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (container_port, bindings) in map {
        match bindings {
            Value::Null => rows.push(container_port.clone()),
            Value::Array(items) => {
                for item in items {
                    let host_ip = str_field(item, "HostIp");
                    let host_port = str_field(item, "HostPort");
                    if host_port.is_empty() {
                        rows.push(container_port.clone());
                    } else if host_ip.is_empty() || host_ip == "0.0.0.0" || host_ip == "::" {
                        rows.push(format!("{host_port}→{container_port}"));
                    } else {
                        rows.push(format!("{host_ip}:{host_port}→{container_port}"));
                    }
                }
            }
            _ => rows.push(container_port.clone()),
        }
    }
    rows.sort();
    rows.dedup();
    rows
}

fn parse_networks(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (name, network) in map {
        let ip = str_field(network, "IPAddress");
        if ip.is_empty() {
            rows.push(name.clone());
        } else {
            rows.push(format!("{name} {ip}"));
        }
    }
    rows.sort();
    rows
}

fn restart_policy(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let name = str_field(value, "Name");
    let retries = value
        .get("MaximumRetryCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if name.is_empty() {
        String::new()
    } else if retries == 0 {
        name
    } else {
        format!("{name} ({retries})")
    }
}

fn last_health_output(health: &Value) -> String {
    health
        .get("Log")
        .and_then(Value::as_array)
        .and_then(|log| log.last())
        .map(|entry| str_field(entry, "Output").trim().to_string())
        .unwrap_or_default()
}

fn label(labels: &Value, key: &str) -> String {
    labels
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn join_cmd(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Array(items)) => {
            let parts: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        _ => None,
    }
}

fn short_id(id: &str) -> String {
    let hex = id.strip_prefix("sha256:").unwrap_or(id);
    hex.chars().take(12).collect()
}

fn short_digest(id: &str) -> String {
    if id.is_empty() {
        return String::new();
    }
    if let Some((name, digest)) = id.rsplit_once('@') {
        return format!("{name}@{}", short_id(digest));
    }
    if id.starts_with("sha256:") {
        format!("sha256:{}", short_id(id))
    } else {
        short_id(id)
    }
}

#[cfg(test)]
mod tests {
    use super::{is_secret_env_key, InspectKind, InspectReport};

    #[test]
    fn parses_container_inspect_and_masks_secrets() {
        let report = InspectReport::from_json(
            r#"[{
              "Id":"a4b37b6e7cad43114beaa4cd7abdadaaf6f14a07bf68b045fca269469ed7bc80",
              "Name":"/dd-mock-api-1",
              "Image":"sha256:6d1204af47e8549e5784d1879dc36c1c7ade3a136d197663ae0ad7a35fb2ac34",
              "Path":"/scripts/api.sh",
              "Args":[],
              "State":{
                "Status":"running",
                "ExitCode":0,
                "StartedAt":"2026-09-08T06:21:15.123Z",
                "FinishedAt":"0001-01-01T00:00:00Z",
                "Health":{"Status":"healthy","Log":[{"Output":"ok\n"}]}
              },
              "HostConfig":{"RestartPolicy":{"Name":"unless-stopped","MaximumRetryCount":0}},
              "Config":{
                "Env":["PATH=/usr/bin","DB_PASSWORD=hunter2"],
                "Cmd":["/scripts/api.sh"],
                "Entrypoint":null,
                "Labels":{
                  "com.docker.compose.project":"dd-mock",
                  "com.docker.compose.service":"api",
                  "com.docker.compose.project.working_dir":"/tmp/mock"
                }
              },
              "NetworkSettings":{
                "Ports":{"8080/tcp":[{"HostIp":"0.0.0.0","HostPort":"18080"}]},
                "Networks":{"dd-mock_default":{"IPAddress":"172.18.0.2"}}
              },
              "Mounts":[]
            }]"#,
        )
        .expect("inspect json");

        assert_eq!(report.kind, InspectKind::Container);
        assert_eq!(report.title, "dd-mock-api-1");
        assert_eq!(report.command, "/scripts/api.sh");
        assert_eq!(report.restart_policy, "unless-stopped");
        assert_eq!(report.health_status, "healthy");
        assert_eq!(report.health_output, "ok");
        assert_eq!(report.ports, vec!["18080→8080/tcp"]);
        assert_eq!(report.networks, vec!["dd-mock_default 172.18.0.2"]);
        assert_eq!(report.compose_service, "api");
        assert_eq!(
            report.env,
            vec![
                ("PATH".into(), "/usr/bin".into()),
                ("DB_PASSWORD".into(), "••••".into()),
            ]
        );
    }

    #[test]
    fn parses_volume_mount_and_unhealthy_output() {
        let report = InspectReport::from_json(
            r#"{"Id":"abc123def456","Name":"/dd-mock-db-1","Image":"sha256:43da5ed74ce7d541a35eee4c84a2c81d53f9372ca6fe99cd5c90826e51b8a547","State":{"Status":"running","ExitCode":0,"Health":{"Status":"unhealthy","Log":[{"Output":"exit 1\n"}]}},"HostConfig":{"RestartPolicy":{"Name":"unless-stopped"}},"Config":{"Env":[],"Cmd":["/scripts/db.sh"],"Labels":{"com.docker.compose.service":"db"}},"NetworkSettings":{"Ports":{},"Networks":{}},"Mounts":[{"Name":"dd-mock_db-data","Source":"/var/lib/docker/volumes/dd-mock_db-data/_data","Destination":"/data","RW":true}]}"#,
        )
        .unwrap();
        assert_eq!(
            report.mounts,
            vec!["/var/lib/docker/volumes/dd-mock_db-data/_data → /data (rw)"]
        );
        assert_eq!(report.health_status, "unhealthy");
        assert_eq!(report.health_output, "exit 1");
    }

    #[test]
    fn secret_key_detection() {
        assert!(is_secret_env_key("DB_PASSWORD"));
        assert!(is_secret_env_key("AWS_SECRET_ACCESS_KEY"));
        assert!(is_secret_env_key("GITHUB_TOKEN"));
        assert!(!is_secret_env_key("PATH"));
        assert!(!is_secret_env_key("PORT"));
    }
}
