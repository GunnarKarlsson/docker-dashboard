use serde::Deserialize;
use serde_json::Value;

use crate::error::DockerError;
use crate::stats::parse_ndjson;
use crate::volume::{inspect_objects, label_value};

/// One network from `docker network ls --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Network {
    #[serde(rename = "ID")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub driver: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub labels: String,
    #[serde(default)]
    pub internal: String,
    #[serde(default, skip_deserializing)]
    pub containers: Vec<NetworkContainer>,
}

/// A container attached to a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkContainer {
    pub name: String,
    pub ipv4: String,
}

impl Network {
    /// Parse NDJSON from `docker network ls --format '{{json .}}'`.
    pub fn from_ndjson(text: &str) -> Result<Vec<Self>, DockerError> {
        parse_ndjson(text, "docker network ls")
    }

    /// Overlay attached containers from `docker network inspect`.
    pub fn merge_inspect(networks: &mut [Self], text: &str) -> Result<(), DockerError> {
        for object in inspect_objects(text)? {
            let name = object.get("Name").and_then(Value::as_str).unwrap_or("");
            let id = object.get("Id").and_then(Value::as_str).unwrap_or("");
            let Some(network) = networks.iter_mut().find(|network| {
                (!name.is_empty() && network.name == name)
                    || (!id.is_empty()
                        && (id.starts_with(&network.id) || network.id.starts_with(id)))
            }) else {
                continue;
            };
            network.containers = attached_containers(&object);
        }
        Ok(())
    }

    pub fn compose_project(&self) -> Option<&str> {
        label_value(&self.labels, "com.docker.compose.project")
    }

    pub fn is_internal(&self) -> bool {
        self.internal.eq_ignore_ascii_case("true")
    }
}

fn attached_containers(object: &Value) -> Vec<NetworkContainer> {
    let Some(map) = object.get("Containers").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut attached: Vec<NetworkContainer> = map
        .values()
        .filter_map(|endpoint| {
            let name = endpoint.get("Name")?.as_str()?.to_string();
            if name.is_empty() {
                return None;
            }
            let ipv4 = endpoint
                .get("IPv4Address")
                .and_then(Value::as_str)
                .unwrap_or("")
                .split('/')
                .next()
                .unwrap_or("")
                .to_string();
            Some(NetworkContainer { name, ipv4 })
        })
        .collect();
    attached.sort_by(|a, b| a.name.cmp(&b.name));
    attached
}

#[cfg(test)]
mod tests {
    use super::Network;

    #[test]
    fn parses_ls_and_attached_containers() {
        let mut networks = Network::from_ndjson(
            r#"{"CreatedAt":"2026-09-08 04:42:36","Driver":"bridge","ID":"b41b69930e72","IPv6":"false","Internal":"false","Labels":"com.docker.compose.project=dd-mock,com.docker.compose.network=default","Name":"dd-mock_default","Scope":"local"}
{"CreatedAt":"2026-09-08 04:42:34","Driver":"bridge","ID":"9726206b120b","Internal":"false","Labels":"","Name":"bridge","Scope":"local"}"#,
        )
        .expect("network ls");
        assert_eq!(networks[0].compose_project(), Some("dd-mock"));
        assert!(!networks[0].is_internal());

        Network::merge_inspect(
            &mut networks,
            r#"[{"Name":"dd-mock_default","Id":"b41b69930e72aaaaaaaa","Containers":{"06f6a7db301e":{"Name":"dd-mock-db-1","IPv4Address":"172.18.0.3/16"},"a4b37b6e7cad":{"Name":"dd-mock-api-1","IPv4Address":"172.18.0.2/16"}}}]"#,
        )
        .expect("inspect");

        assert_eq!(networks[0].containers.len(), 2);
        assert_eq!(networks[0].containers[0].name, "dd-mock-api-1");
        assert_eq!(networks[0].containers[0].ipv4, "172.18.0.2");
        assert!(networks[1].containers.is_empty());
    }

    #[test]
    fn empty_networks_is_ok() {
        assert!(Network::from_ndjson("\n").unwrap().is_empty());
    }
}
