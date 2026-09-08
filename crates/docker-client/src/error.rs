use std::io;

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("docker not found on PATH. Install Docker and ensure `docker` is available.")]
    NotFound,

    #[error("failed to run docker: {0}")]
    Io(io::Error),

    #[error("docker version check failed: {0}")]
    VersionCheckFailed(String),

    #[error("docker command failed ({command}): {stderr}")]
    CommandFailed { command: String, stderr: String },

    #[error("failed to parse docker output: {0}")]
    ParseFailed(String),
}

impl DockerError {
    /// Returns a short, user-facing description of the error.
    pub fn user_message(&self) -> String {
        match self {
            DockerError::NotFound => {
                "docker not found on PATH. Install Docker Desktop or the Docker CLI.".to_string()
            }
            DockerError::Io(err) => format!("Failed to run docker: {err}"),
            DockerError::VersionCheckFailed(stderr) => classify_docker_stderr(stderr),
            DockerError::CommandFailed { stderr, .. } => classify_docker_stderr(stderr),
            DockerError::ParseFailed(message) => {
                format!("Failed to parse docker output: {message}")
            }
        }
    }
}

fn classify_docker_stderr(stderr: &str) -> String {
    let trimmed = stderr.trim();
    let lower = trimmed.to_lowercase();

    if lower.contains("cannot connect to the docker daemon")
        || lower.contains("is the docker daemon running")
    {
        "Docker daemon is not running".to_string()
    } else if lower.contains("permission denied") {
        "Permission denied talking to the Docker daemon".to_string()
    } else if trimmed.is_empty() {
        "docker command failed".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_daemon_not_running() {
        assert_eq!(
            classify_docker_stderr("Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"),
            "Docker daemon is not running"
        );
    }

    #[test]
    fn classify_permission_denied() {
        assert_eq!(
            classify_docker_stderr(
                "permission denied while trying to connect to the Docker daemon socket"
            ),
            "Permission denied talking to the Docker daemon"
        );
    }

    #[test]
    fn not_found_user_message() {
        assert!(DockerError::NotFound.user_message().contains("PATH"));
    }
}
