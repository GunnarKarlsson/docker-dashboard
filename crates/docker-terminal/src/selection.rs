//! Investigate/Runtime mode and the unit of UI selection.
//!
//! Existing `App` fields (`selected_container`, `selected_compose_service`) stay
//! the source of inspect/log behavior. These types are the new mental model and
//! are kept in sync from the same click handlers.
#![allow(dead_code)]

use docker_client::Container;

/// Primary workspace vs Docker inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Investigate,
    Runtime,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Investigate
    }
}

/// What the user is looking at. The compose service is the preferred unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    None,
    Project,
    Service { name: String },
    Container { id: String },
    Image { id: String },
    Volume { name: String },
}

impl Default for Selection {
    fn default() -> Self {
        Self::None
    }
}

impl Selection {
    pub fn service_name(&self) -> Option<&str> {
        match self {
            Self::Service { name } => Some(name),
            _ => None,
        }
    }
}

pub fn containers_for_service<'a>(
    containers: &'a [Container],
    service: &str,
) -> Vec<&'a Container> {
    containers
        .iter()
        .filter(|container| container.compose_service() == Some(service))
        .collect()
}

pub fn project_containers<'a>(containers: &'a [Container], project: &str) -> Vec<&'a Container> {
    containers
        .iter()
        .filter(|container| container.compose_project() == Some(project))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{containers_for_service, project_containers, AppMode, Selection};
    use docker_client::Container;

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

    #[test]
    fn defaults_are_investigate_and_none() {
        assert_eq!(AppMode::default(), AppMode::Investigate);
        assert_eq!(Selection::default(), Selection::None);
        let _ = AppMode::Runtime;
        let _ = [
            Selection::Project,
            Selection::Service {
                name: "worker".into(),
            },
            Selection::Container { id: "abc".into() },
            Selection::Image { id: "sha".into() },
            Selection::Volume {
                name: "db-data".into(),
            },
        ];
    }

    #[test]
    fn service_name_only_from_service_variant() {
        assert_eq!(
            Selection::Service {
                name: "worker".into()
            }
            .service_name(),
            Some("worker")
        );
        assert_eq!(Selection::None.service_name(), None);
        assert_eq!(
            Selection::Container { id: "abc".into() }.service_name(),
            None
        );
    }

    #[test]
    fn filters_containers_by_compose_service_and_project() {
        let api = container(
            "1",
            "/dd-mock-api-1",
            "com.docker.compose.project=dd-mock,com.docker.compose.service=api",
        );
        let worker = container(
            "2",
            "/dd-mock-worker-1",
            "com.docker.compose.project=dd-mock,com.docker.compose.service=worker",
        );
        let other = container("3", "/orphan", "");
        let containers = [api, worker, other];

        let services = containers_for_service(&containers, "worker");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].id, "2");

        let project = project_containers(&containers, "dd-mock");
        assert_eq!(project.len(), 2);
        assert!(project.iter().all(|c| c.id == "1" || c.id == "2"));
    }
}
