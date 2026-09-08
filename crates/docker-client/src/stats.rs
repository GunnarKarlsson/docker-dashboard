use serde::Deserialize;

use crate::error::DockerError;

/// One row from `docker stats --no-stream --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerStats {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub container: String,
    #[serde(default, rename = "CPUPerc")]
    pub cpu_perc: String,
    #[serde(default, rename = "MemUsage")]
    pub mem_usage: String,
    #[serde(default, rename = "MemPerc")]
    pub mem_perc: String,
    #[serde(default, rename = "NetIO")]
    pub net_io: String,
    #[serde(default, rename = "BlockIO")]
    pub block_io: String,
    #[serde(default, rename = "PIDs")]
    pub pids: String,
}

impl ContainerStats {
    /// Parse NDJSON from `docker stats --no-stream --format '{{json .}}'`.
    pub fn from_ndjson(text: &str) -> Result<Vec<Self>, DockerError> {
        parse_ndjson(text, "docker stats")
    }

    pub fn matches_container_id(&self, container_id: &str) -> bool {
        !self.id.is_empty()
            && (container_id == self.id
                || container_id.starts_with(&self.id)
                || self.id.starts_with(container_id))
    }
}

pub(crate) fn parse_ndjson<T>(text: &str, label: &str) -> Result<Vec<T>, DockerError>
where
    T: serde::de::DeserializeOwned,
{
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
    use super::ContainerStats;

    #[test]
    fn parses_running_container_stats() {
        let rows = ContainerStats::from_ndjson(
            r#"{"BlockIO":"0B / 0B","CPUPerc":"0.17%","Container":"a4b37b6e7cad","ID":"a4b37b6e7cad","MemPerc":"0.01%","MemUsage":"1.355MiB / 9.459GiB","Name":"dd-mock-api-1","NetIO":"8.27kB / 126B","PIDs":"4"}
{"BlockIO":"0B / 3.4MB","CPUPerc":"0.00%","Container":"06f6a7db301e","ID":"06f6a7db301e","MemPerc":"0.01%","MemUsage":"752KiB / 9.459GiB","Name":"dd-mock-db-1","NetIO":"8.31kB / 126B","PIDs":"2"}"#,
        )
        .expect("stats json");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "dd-mock-api-1");
        assert_eq!(rows[0].cpu_perc, "0.17%");
        assert_eq!(rows[0].mem_usage, "1.355MiB / 9.459GiB");
        assert!(rows[0].matches_container_id("a4b37b6e7cadabcdef"));
        assert_eq!(rows[1].pids, "2");
    }

    #[test]
    fn empty_stats_is_ok() {
        assert!(ContainerStats::from_ndjson("\n").unwrap().is_empty());
    }
}
