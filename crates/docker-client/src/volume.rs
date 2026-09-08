use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

use crate::error::DockerError;
use crate::stats::parse_ndjson;

/// One volume from `docker volume ls --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Volume {
    pub name: String,
    #[serde(default)]
    pub driver: String,
    #[serde(default)]
    pub mountpoint: String,
    #[serde(default)]
    pub labels: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default, skip_deserializing)]
    pub dangling: bool,
    #[serde(default, skip_deserializing)]
    pub created_at: String,
}

impl Volume {
    /// Parse NDJSON from `docker volume ls --format '{{json .}}'`.
    pub fn from_ndjson(text: &str) -> Result<Vec<Self>, DockerError> {
        parse_ndjson(text, "docker volume ls")
    }

    pub fn mark_dangling(volumes: &mut [Self], dangling_names: &HashSet<String>) {
        for volume in volumes {
            volume.dangling = dangling_names.contains(&volume.name);
        }
    }

    /// Overlay `CreatedAt` from `docker volume inspect`.
    pub fn merge_inspect(volumes: &mut [Self], text: &str) -> Result<(), DockerError> {
        let inspected = inspect_objects(text)?;
        for object in inspected {
            let Some(name) = object.get("Name").and_then(Value::as_str) else {
                continue;
            };
            let Some(volume) = volumes.iter_mut().find(|volume| volume.name == name) else {
                continue;
            };
            if let Some(created) = object.get("CreatedAt").and_then(Value::as_str) {
                volume.created_at = created.to_string();
            }
            if volume.mountpoint.is_empty() {
                if let Some(mount) = object.get("Mountpoint").and_then(Value::as_str) {
                    volume.mountpoint = mount.to_string();
                }
            }
            if volume.driver.is_empty() {
                if let Some(driver) = object.get("Driver").and_then(Value::as_str) {
                    volume.driver = driver.to_string();
                }
            }
        }
        Ok(())
    }

    pub fn compose_project(&self) -> Option<&str> {
        label_value(&self.labels, "com.docker.compose.project")
    }

    pub fn compose_volume(&self) -> Option<&str> {
        label_value(&self.labels, "com.docker.compose.volume")
    }
}

pub(crate) fn inspect_objects(text: &str) -> Result<Vec<Value>, DockerError> {
    let value: Value = serde_json::from_str(text.trim())
        .map_err(|err| DockerError::ParseFailed(format!("docker inspect: {err}")))?;
    match value {
        Value::Array(items) => Ok(items),
        object @ Value::Object(_) => Ok(vec![object]),
        _ => Err(DockerError::ParseFailed(
            "docker inspect: expected JSON object or array".into(),
        )),
    }
}

pub(crate) fn label_value<'a>(labels: &'a str, key: &str) -> Option<&'a str> {
    for part in labels.split(',') {
        let part = part.trim();
        if let Some(value) = part
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
        {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

pub fn dangling_names(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{dangling_names, Volume};

    #[test]
    fn parses_compose_volume_and_dangling() {
        let mut volumes = Volume::from_ndjson(
            r#"{"Driver":"local","Labels":"com.docker.compose.project=dd-mock,com.docker.compose.volume=db-data","Mountpoint":"/var/lib/docker/volumes/dd-mock_db-data/_data","Name":"dd-mock_db-data","Scope":"local"}
{"Driver":"local","Labels":"","Mountpoint":"/var/lib/docker/volumes/orphan/_data","Name":"orphan","Scope":"local"}"#,
        )
        .expect("volume ls");
        Volume::mark_dangling(&mut volumes, &dangling_names("orphan\n"));
        Volume::merge_inspect(
            &mut volumes,
            r#"[{"Name":"dd-mock_db-data","CreatedAt":"2026-09-08T04:42:36Z","Driver":"local"}]"#,
        )
        .expect("inspect");

        assert_eq!(volumes[0].compose_project(), Some("dd-mock"));
        assert_eq!(volumes[0].compose_volume(), Some("db-data"));
        assert!(!volumes[0].dangling);
        assert_eq!(volumes[0].created_at, "2026-09-08T04:42:36Z");
        assert!(volumes[1].dangling);
    }

    #[test]
    fn empty_volumes_is_ok() {
        assert!(Volume::from_ndjson("\n").unwrap().is_empty());
    }
}
