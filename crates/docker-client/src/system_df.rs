use serde::Deserialize;

use crate::error::DockerError;
use crate::size::parse_docker_size;

/// One line of `docker system df --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemDfRow {
    #[serde(rename = "Type")]
    pub kind: String,
    pub total_count: String,
    pub active: String,
    pub size: String,
    pub reclaimable: String,
}

impl SystemDfRow {
    pub fn size_bytes(&self) -> u64 {
        parse_docker_size(&self.size)
    }

    pub fn reclaimable_bytes(&self) -> u64 {
        parse_docker_size(&self.reclaimable)
    }

    pub fn in_use_bytes(&self) -> u64 {
        self.size_bytes().saturating_sub(self.reclaimable_bytes())
    }
}

/// Summary of Docker disk usage (images, containers, volumes, build cache).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemDf {
    pub rows: Vec<SystemDfRow>,
}

impl SystemDf {
    /// Parse NDJSON from `docker system df --format '{{json .}}'`.
    pub fn from_ndjson(text: &str) -> Result<Self, DockerError> {
        let mut rows = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let row = serde_json::from_str(line).map_err(|err| {
                DockerError::ParseFailed(format!("system df line {}: {err}", index + 1))
            })?;
            rows.push(row);
        }
        if rows.is_empty() {
            return Err(DockerError::ParseFailed(
                "system df produced no rows".to_string(),
            ));
        }
        Ok(Self { rows })
    }

    pub fn total_bytes(&self) -> u64 {
        self.rows.iter().map(SystemDfRow::size_bytes).sum()
    }

    pub fn reclaimable_bytes(&self) -> u64 {
        self.rows.iter().map(SystemDfRow::reclaimable_bytes).sum()
    }

    pub fn in_use_bytes(&self) -> u64 {
        self.rows.iter().map(SystemDfRow::in_use_bytes).sum()
    }

    pub fn used_fraction(&self) -> f32 {
        let total = self.total_bytes();
        if total == 0 {
            0.0
        } else {
            self.in_use_bytes() as f32 / total as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemDf;

    #[test]
    fn rejects_empty_output() {
        let err = SystemDf::from_ndjson("\n").expect_err("empty");
        assert!(err.to_string().contains("no rows"));
    }
}
