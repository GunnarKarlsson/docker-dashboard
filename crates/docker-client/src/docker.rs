use std::io;
use std::process::Output;

use crate::error::DockerError;
use crate::transport::Transport;
use crate::version::DockerVersion;

/// Entry point for running `docker` commands.
pub struct Docker;

impl Docker {
    /// Verifies that `docker` is installed and runnable on the local host.
    pub fn check_available() -> Result<(), DockerError> {
        run_docker(&Transport::Local, &["version"]).map(|_| ())
    }

    /// Runs `docker version --format '{{json .}}'` on the local host.
    pub fn version() -> Result<DockerVersion, DockerError> {
        version_for(&Transport::Local)
    }

    /// Runs `docker version --format '{{json .}}'` via `transport`.
    pub fn version_for(transport: &Transport) -> Result<DockerVersion, DockerError> {
        version_for(transport)
    }
}

pub(crate) fn version_for(transport: &Transport) -> Result<DockerVersion, DockerError> {
    let output = run_docker(transport, &["version", "--format", "{{json .}}"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    DockerVersion::from_json(&stdout)
}

pub(crate) fn run_docker(transport: &Transport, args: &[&str]) -> Result<Output, DockerError> {
    let output = transport.command(args).output().map_err(map_io_error)?;

    if output.status.success() {
        Ok(output)
    } else {
        let command = transport.describe_command(args);
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
