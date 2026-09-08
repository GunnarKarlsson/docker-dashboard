use serde::Deserialize;

use crate::error::DockerError;

/// Parsed `docker version --format '{{json .}}'` output.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DockerVersion {
    pub client: ClientVersion,
    #[serde(default)]
    pub server: Option<ServerVersion>,
}

/// Docker CLI (client) version fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ClientVersion {
    pub version: String,
    #[serde(default)]
    pub api_version: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

/// Docker Engine (server) version fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ServerVersion {
    pub version: String,
    #[serde(default)]
    pub api_version: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

impl DockerVersion {
    /// Parse JSON from `docker version --format '{{json .}}'`.
    pub fn from_json(text: &str) -> Result<Self, DockerError> {
        serde_json::from_str(text.trim()).map_err(|err| DockerError::ParseFailed(err.to_string()))
    }

    /// Engine/server version when the daemon replied.
    pub fn engine_version(&self) -> Option<&str> {
        self.server.as_ref().map(|server| server.version.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::DockerVersion;

    const VERSION_JSON: &str = include_str!("../../../testdata/docker-client/version.json");
    const VERSION_NO_SERVER_JSON: &str =
        include_str!("../../../testdata/docker-client/version-no-server.json");

    #[test]
    fn parses_client_and_server_version() {
        let version = DockerVersion::from_json(VERSION_JSON).expect("valid version json");
        assert_eq!(version.client.version, "28.3.3");
        assert_eq!(version.client.api_version.as_deref(), Some("1.51"));
        assert_eq!(version.engine_version(), Some("28.3.3"));
        let server = version.server.expect("server present");
        assert_eq!(server.os.as_deref(), Some("linux"));
        assert_eq!(server.arch.as_deref(), Some("arm64"));
    }

    #[test]
    fn parses_client_only_when_server_missing() {
        let version =
            DockerVersion::from_json(VERSION_NO_SERVER_JSON).expect("valid client-only json");
        assert_eq!(version.client.version, "28.3.3");
        assert!(version.server.is_none());
        assert_eq!(version.engine_version(), None);
    }

    #[test]
    fn rejects_malformed_json() {
        let err = DockerVersion::from_json("not json").expect_err("should fail");
        assert!(err.to_string().contains("parse"));
    }
}
