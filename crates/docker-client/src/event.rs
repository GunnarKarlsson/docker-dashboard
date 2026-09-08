use std::collections::HashMap;

use serde::Deserialize;

use crate::error::DockerError;
use crate::stats::parse_ndjson;

const MAX_EVENTS: usize = 200;

const NOTABLE_ACTIONS: &[&str] = &[
    "pull",
    "create",
    "start",
    "die",
    "oom",
    "kill",
    "destroy",
    "restart",
    "health_status",
];

/// One line from `docker events --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DockerEvent {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "from")]
    pub from: String,
    #[serde(default, rename = "Type")]
    pub kind: String,
    #[serde(default, rename = "Action")]
    pub action: String,
    #[serde(default, rename = "Actor")]
    pub actor: EventActor,
    #[serde(default)]
    pub time: i64,
}

/// Actor block on a Docker event.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct EventActor {
    #[serde(default, rename = "ID")]
    pub id: String,
    #[serde(default, rename = "Attributes")]
    pub attributes: HashMap<String, String>,
}

impl DockerEvent {
    /// Parse NDJSON from `docker events`, keeping a compact notable feed.
    pub fn from_ndjson(text: &str) -> Result<Vec<Self>, DockerError> {
        let parsed: Vec<Self> = parse_ndjson(text, "docker events")?;
        let mut notable: Vec<Self> = parsed.into_iter().filter(Self::is_notable).collect();
        if notable.len() > MAX_EVENTS {
            notable.drain(0..notable.len() - MAX_EVENTS);
        }
        Ok(notable)
    }

    pub fn is_notable(&self) -> bool {
        NOTABLE_ACTIONS.contains(&self.action_kind())
    }

    /// Action token before `:` or space (`health_status: unhealthy` → `health_status`).
    pub fn action_kind(&self) -> &str {
        let action = if self.action.is_empty() {
            self.status.as_str()
        } else {
            self.action.as_str()
        };
        action.split([':', ' ']).next().unwrap_or(action)
    }

    pub fn target_name(&self) -> String {
        if let Some(service) = self.attr("com.docker.compose.service") {
            return service.to_string();
        }
        if let Some(name) = self.attr("name") {
            return name.trim_start_matches('/').to_string();
        }
        if !self.from.is_empty() {
            return self.from.clone();
        }
        let id = if self.id.is_empty() {
            self.actor.id.as_str()
        } else {
            self.id.as_str()
        };
        if id.len() > 12 {
            id[..12].to_string()
        } else {
            id.to_string()
        }
    }

    pub fn detail(&self) -> Option<String> {
        if let Some(code) = self.attr("exitCode") {
            return Some(format!("exit {code}"));
        }
        if self.action_kind() == "health_status" {
            if let Some((_, status)) = self.action.split_once(':') {
                let status = status.trim();
                if !status.is_empty() {
                    return Some(status.to_string());
                }
            }
        }
        None
    }

    /// `HH:MM:SS` UTC from the event unix timestamp.
    pub fn time_utc(&self) -> String {
        if self.time <= 0 {
            return String::new();
        }
        let secs = self.time.rem_euclid(86_400) as u32;
        format!(
            "{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }

    pub fn is_failure(&self) -> bool {
        matches!(self.action_kind(), "die" | "oom" | "kill")
            || self
                .detail()
                .is_some_and(|detail| detail.contains("unhealthy"))
    }

    fn attr(&self, key: &str) -> Option<&str> {
        self.actor
            .attributes
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::DockerEvent;

    const DIE: &str = r#"{"status":"die","id":"e7ebda7f6de8","from":"dd-mock-worker","Type":"container","Action":"die","Actor":{"ID":"e7ebda7f6de8","Attributes":{"com.docker.compose.service":"worker","exitCode":"1","image":"dd-mock-worker","name":"dd-mock-worker-1"}},"time":1788857794}"#;
    const START: &str = r#"{"status":"start","id":"e7ebda7f6de8","from":"dd-mock-worker","Type":"container","Action":"start","Actor":{"ID":"e7ebda7f6de8","Attributes":{"com.docker.compose.service":"worker","image":"dd-mock-worker","name":"dd-mock-worker-1"}},"time":1788857794}"#;
    const EXEC: &str = r#"{"status":"exec_create: /bin/sh -c exit 1","id":"a525d032e33e","from":"dd-mock-unhealthy","Type":"container","Action":"exec_create: /bin/sh -c exit 1","Actor":{"ID":"a525d032e33e","Attributes":{"com.docker.compose.service":"unhealthy","name":"dd-mock-unhealthy-1"}},"time":1788857415}"#;
    const HEALTH: &str = r#"{"status":"health_status: unhealthy","id":"a525d032e33e","Type":"container","Action":"health_status: unhealthy","Actor":{"ID":"a525d032e33e","Attributes":{"com.docker.compose.service":"unhealthy","name":"dd-mock-unhealthy-1"}},"time":1788857415}"#;

    #[test]
    fn keeps_die_start_health_and_drops_exec() {
        let text = format!("{EXEC}\n{DIE}\n{START}\n{HEALTH}\n");
        let events = DockerEvent::from_ndjson(&text).expect("events json");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].action_kind(), "die");
        assert_eq!(events[0].target_name(), "worker");
        assert_eq!(events[0].detail().as_deref(), Some("exit 1"));
        assert!(events[0].is_failure());
        assert_eq!(events[1].action_kind(), "start");
        assert_eq!(events[2].action_kind(), "health_status");
        assert_eq!(events[2].detail().as_deref(), Some("unhealthy"));
        assert_eq!(events[0].time_utc(), "08:56:34");
    }

    #[test]
    fn empty_events_is_ok() {
        assert!(DockerEvent::from_ndjson("\n").unwrap().is_empty());
    }
}
