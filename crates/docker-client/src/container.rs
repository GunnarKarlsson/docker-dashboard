use serde::Deserialize;

use crate::error::DockerError;

const COMPOSE_PROJECT: &str = "com.docker.compose.project";
const COMPOSE_SERVICE: &str = "com.docker.compose.service";

/// One container from `docker ps -a --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
    #[serde(rename = "ID")]
    pub id: String,
    pub names: String,
    pub image: String,
    pub state: String,
    pub status: String,
    #[serde(default)]
    pub ports: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub running_for: String,
    #[serde(default)]
    pub labels: String,
}

impl Container {
    /// Parse NDJSON from `docker ps -a --format '{{json .}}'`. Empty output is no containers.
    pub fn from_ndjson(text: &str) -> Result<Vec<Self>, DockerError> {
        let mut rows = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let row = serde_json::from_str(line).map_err(|err| {
                DockerError::ParseFailed(format!("docker ps line {}: {err}", index + 1))
            })?;
            rows.push(row);
        }
        Ok(rows)
    }

    pub fn compose_project(&self) -> Option<&str> {
        label_value(&self.labels, COMPOSE_PROJECT)
    }

    pub fn compose_service(&self) -> Option<&str> {
        label_value(&self.labels, COMPOSE_SERVICE)
    }

    pub fn compose_label(&self) -> Option<String> {
        match (self.compose_project(), self.compose_service()) {
            (Some(project), Some(service)) => Some(format!("{project}/{service}")),
            (Some(project), None) => Some(project.to_string()),
            (None, Some(service)) => Some(service.to_string()),
            (None, None) => None,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{label_value, Container, COMPOSE_PROJECT, COMPOSE_SERVICE};

    #[test]
    fn extracts_compose_project_and_service() {
        let labels = "com.docker.compose.project=dd-mock,com.docker.compose.service=worker,com.docker.compose.oneoff=False";
        assert_eq!(label_value(labels, COMPOSE_PROJECT), Some("dd-mock"));
        assert_eq!(label_value(labels, COMPOSE_SERVICE), Some("worker"));
    }

    #[test]
    fn empty_ps_is_no_containers() {
        let rows = Container::from_ndjson("\n").expect("empty ok");
        assert!(rows.is_empty());
    }
}
