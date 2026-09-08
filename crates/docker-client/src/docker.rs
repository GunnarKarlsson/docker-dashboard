use std::io;
use std::process::{Command, Output};

use crate::error::DockerError;

/// Entry point for running `docker` commands.
pub struct Docker;

impl Docker {
    /// Verifies that `docker` is installed and runnable.
    pub fn check_available() -> Result<(), DockerError> {
        run_docker(&["version"]).map(|_| ())
    }
}

pub(crate) fn run_docker(args: &[&str]) -> Result<Output, DockerError> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .map_err(map_io_error)?;

    if output.status.success() {
        Ok(output)
    } else {
        let command = format!("docker {}", args.join(" "));
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Err(DockerError::CommandFailed { command, stderr })
    }
}

fn map_io_error(err: io::Error) -> DockerError {
    if err.kind() == io::ErrorKind::NotFound {
        DockerError::NotFound
    } else {
        DockerError::Io(err)
    }
}
