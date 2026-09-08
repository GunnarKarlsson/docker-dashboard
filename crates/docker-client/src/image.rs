use serde::Deserialize;

use crate::error::DockerError;

/// One image from `docker images --format '{{json .}}'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LocalImage {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub created_since: String,
    #[serde(default)]
    pub size: String,
}

impl LocalImage {
    /// Parse NDJSON from `docker images --format '{{json .}}'`.
    pub fn from_ndjson(text: &str) -> Result<Vec<Self>, DockerError> {
        let mut rows = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let row = serde_json::from_str(line).map_err(|err| {
                DockerError::ParseFailed(format!("docker images line {}: {err}", index + 1))
            })?;
            rows.push(row);
        }
        Ok(rows)
    }

    pub fn dangling(&self) -> bool {
        is_none(&self.repository) && is_none(&self.tag)
    }

    pub fn short_id(&self) -> String {
        let hex = self.id.strip_prefix("sha256:").unwrap_or(&self.id);
        hex.chars().take(12).collect()
    }

    pub fn repo_tag(&self) -> String {
        if self.dangling() {
            return "<none>:<none>".to_string();
        }
        let repo = if self.repository.is_empty() {
            "<none>"
        } else {
            &self.repository
        };
        let tag = if self.tag.is_empty() {
            "<none>"
        } else {
            &self.tag
        };
        format!("{repo}:{tag}")
    }

    /// Whether a `docker ps` Image field refers to this image.
    pub fn matches_container_image(&self, image_ref: &str) -> bool {
        let image_ref = image_ref.trim();
        if image_ref.is_empty() {
            return false;
        }
        if self.matches_id(image_ref) {
            return true;
        }
        if self.dangling() {
            return false;
        }
        let tag = if self.tag.is_empty() || is_none(&self.tag) {
            "latest"
        } else {
            &self.tag
        };
        image_ref == self.repository || image_ref == format!("{}:{tag}", self.repository)
    }

    fn matches_id(&self, image_ref: &str) -> bool {
        let hex = self.id.strip_prefix("sha256:").unwrap_or(&self.id);
        let reference = image_ref.strip_prefix("sha256:").unwrap_or(image_ref);
        reference.len() >= 12 && hex.starts_with(reference)
    }
}

fn is_none(value: &str) -> bool {
    value.is_empty() || value == "<none>"
}

#[cfg(test)]
mod tests {
    use super::LocalImage;

    #[test]
    fn parses_tagged_and_dangling_images() {
        let rows = LocalImage::from_ndjson(
            r#"{"ID":"sha256:6d1204af47e8549e5784d1879dc36c1c7ade3a136d197663ae0ad7a35fb2ac34","Repository":"dd-mock-api","Tag":"latest","CreatedSince":"26 minutes ago","Size":"8.82MB"}
{"ID":"sha256:aaaaaaaaaaaabbbbbbbbbbbbccccccccccccddddddddddddeeeeeeeeeeeeffff","Repository":"<none>","Tag":"<none>","CreatedSince":"2 hours ago","Size":"1.2MB"}"#,
        )
        .expect("images json");
        assert_eq!(rows[0].repo_tag(), "dd-mock-api:latest");
        assert_eq!(rows[0].short_id(), "6d1204af47e8");
        assert!(!rows[0].dangling());
        assert!(rows[0].matches_container_image("dd-mock-api"));
        assert!(rows[0].matches_container_image("dd-mock-api:latest"));
        assert!(rows[0].matches_container_image("6d1204af47e8"));
        assert!(rows[1].dangling());
        assert_eq!(rows[1].repo_tag(), "<none>:<none>");
        assert!(!rows[1].matches_container_image("dd-mock-api"));
    }

    #[test]
    fn empty_images_is_ok() {
        assert!(LocalImage::from_ndjson("\n").unwrap().is_empty());
    }
}
