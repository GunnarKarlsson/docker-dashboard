use std::io;
use std::process::Output;

use crate::container::Container;
use crate::error::DockerError;
use crate::image::LocalImage;
use crate::inspect::{InspectReport, InspectTarget};
use crate::system_df::SystemDf;
use crate::system_df_verbose::SystemDfVerbose;
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

    /// Runs `docker system df --format '{{json .}}'` via `transport`.
    pub fn system_df(transport: &Transport) -> Result<SystemDf, DockerError> {
        let output = run_docker(transport, &["system", "df", "--format", "{{json .}}"])?;
        SystemDf::from_ndjson(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker system df -v --format '{{json .}}'` via `transport`.
    pub fn system_df_verbose(transport: &Transport) -> Result<SystemDfVerbose, DockerError> {
        let output = run_docker(transport, &["system", "df", "-v", "--format", "{{json .}}"])?;
        SystemDfVerbose::from_json(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker ps -a --format '{{json .}}'` via `transport`.
    pub fn ps_a(transport: &Transport) -> Result<Vec<Container>, DockerError> {
        let output = run_docker(transport, &["ps", "-a", "--format", "{{json .}}"])?;
        Container::from_ndjson(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker images --format '{{json .}}'` via `transport`.
    pub fn images(transport: &Transport) -> Result<Vec<LocalImage>, DockerError> {
        let output = run_docker(transport, &["images", "--format", "{{json .}}"])?;
        LocalImage::from_ndjson(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker inspect <id>` via `transport`.
    pub fn inspect(
        transport: &Transport,
        target: &InspectTarget,
    ) -> Result<InspectReport, DockerError> {
        let output = run_docker(transport, &["inspect", target.id()])?;
        InspectReport::from_json(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker …` with `args` via `transport`.
    pub fn run(transport: &Transport, args: &[&str]) -> Result<Output, DockerError> {
        run_docker(transport, args)
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

pub(crate) fn run_host(
    transport: &Transport,
    program: &str,
    args: &[&str],
) -> Result<Output, DockerError> {
    let output = transport
        .host_command(program, args)
        .output()
        .map_err(DockerError::Io)?;

    if output.status.success() {
        Ok(output)
    } else {
        let command = transport.describe_host_command(program, args);
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
