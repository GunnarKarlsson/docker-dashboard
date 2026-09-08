use serde::Deserialize;

use crate::error::DockerError;
use crate::size::parse_docker_size;

/// Parsed `docker system df -v --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemDfVerbose {
    #[serde(default)]
    pub images: Vec<DfImage>,
    #[serde(default)]
    pub volumes: Vec<DfVolume>,
    #[serde(default)]
    pub build_cache: Vec<DfBuildCache>,
}

/// One image row from verbose system df.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DfImage {
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub tag: String,
    #[serde(rename = "ID")]
    pub id: String,
    pub size: String,
    #[serde(default)]
    pub unique_size: String,
    #[serde(default)]
    pub containers: String,
}

/// One volume row from verbose system df.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DfVolume {
    pub name: String,
    #[serde(default)]
    pub links: String,
    pub size: String,
}

/// One build-cache row from verbose system df.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DfBuildCache {
    #[serde(rename = "ID")]
    pub id: String,
    pub size: String,
    #[serde(default, rename = "InUse")]
    pub in_use: String,
}

impl SystemDfVerbose {
    pub fn from_json(text: &str) -> Result<Self, DockerError> {
        serde_json::from_str(text.trim()).map_err(|err| DockerError::ParseFailed(err.to_string()))
    }

    pub fn unused_images(&self) -> Vec<&DfImage> {
        let mut unused: Vec<&DfImage> = self
            .images
            .iter()
            .filter(|image| image.is_unused())
            .collect();
        unused.sort_by(|a, b| {
            b.unique_bytes()
                .cmp(&a.unique_bytes())
                .then_with(|| b.size_bytes().cmp(&a.size_bytes()))
        });
        unused
    }

    pub fn unused_cache_bytes(&self) -> u64 {
        self.build_cache
            .iter()
            .filter(|entry| !entry.is_in_use())
            .map(DfBuildCache::size_bytes)
            .sum()
    }

    pub fn unused_cache_count(&self) -> usize {
        self.build_cache
            .iter()
            .filter(|entry| !entry.is_in_use())
            .count()
    }
}

impl DfImage {
    pub fn container_count(&self) -> u32 {
        self.containers.parse().unwrap_or(0)
    }

    pub fn is_unused(&self) -> bool {
        self.container_count() == 0
    }

    pub fn size_bytes(&self) -> u64 {
        parse_docker_size(&self.size)
    }

    pub fn unique_bytes(&self) -> u64 {
        parse_docker_size(&self.unique_size)
    }

    pub fn short_id(&self) -> &str {
        let id = self.id.strip_prefix("sha256:").unwrap_or(&self.id);
        if id.len() > 12 {
            &id[..12]
        } else {
            id
        }
    }

    pub fn display_name(&self) -> String {
        if self.repository.is_empty() || self.repository == "<none>" {
            format!("<none>@{}", self.short_id())
        } else if self.tag.is_empty() || self.tag == "<none>" {
            format!("{}@{}", self.repository, self.short_id())
        } else {
            format!("{}:{}", self.repository, self.tag)
        }
    }
}

impl DfVolume {
    pub fn size_bytes(&self) -> u64 {
        parse_docker_size(&self.size)
    }
}

impl DfBuildCache {
    pub fn is_in_use(&self) -> bool {
        self.in_use.eq_ignore_ascii_case("true")
    }

    pub fn size_bytes(&self) -> u64 {
        parse_docker_size(&self.size)
    }
}
