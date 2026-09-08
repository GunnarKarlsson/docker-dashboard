use std::io;
use std::process::Output;

use crate::container::Container;
use crate::error::DockerError;
use crate::event::DockerEvent;
use crate::image::LocalImage;
use crate::inspect::{InspectReport, InspectTarget};
use crate::network::Network;
use crate::stats::ContainerStats;
use crate::system_df::SystemDf;
use crate::system_df_verbose::SystemDfVerbose;
use crate::transport::Transport;
use crate::version::DockerVersion;
use crate::volume::{dangling_names, Volume};

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

    /// Runs `docker stats --no-stream --format '{{json .}}'` via `transport`.
    pub fn stats(transport: &Transport) -> Result<Vec<ContainerStats>, DockerError> {
        let output = run_docker(
            transport,
            &["stats", "--no-stream", "--format", "{{json .}}"],
        )?;
        ContainerStats::from_ndjson(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker events --since 30m --until 0s` via `transport`.
    pub fn events(transport: &Transport) -> Result<Vec<DockerEvent>, DockerError> {
        let output = run_docker(transport, EVENT_ARGS)?;
        DockerEvent::from_ndjson(&String::from_utf8_lossy(&output.stdout))
    }

    /// Runs `docker volume ls` plus inspect / dangling via `transport`.
    pub fn volumes(transport: &Transport) -> Result<Vec<Volume>, DockerError> {
        let output = run_docker(transport, &["volume", "ls", "--format", "{{json .}}"])?;
        let mut volumes = Volume::from_ndjson(&String::from_utf8_lossy(&output.stdout))?;

        if let Ok(dangling) = run_docker(
            transport,
            &[
                "volume",
                "ls",
                "-f",
                "dangling=true",
                "--format",
                "{{.Name}}",
            ],
        ) {
            Volume::mark_dangling(
                &mut volumes,
                &dangling_names(&String::from_utf8_lossy(&dangling.stdout)),
            );
        }

        if !volumes.is_empty() {
            let mut args = vec!["volume".to_string(), "inspect".to_string()];
            args.extend(volumes.iter().map(|volume| volume.name.clone()));
            if let Ok(inspect) = run_docker_owned(transport, &args) {
                let _ =
                    Volume::merge_inspect(&mut volumes, &String::from_utf8_lossy(&inspect.stdout));
            }
        }

        Ok(volumes)
    }

    /// Runs `docker network ls` plus inspect via `transport`.
    pub fn networks(transport: &Transport) -> Result<Vec<Network>, DockerError> {
        let output = run_docker(transport, &["network", "ls", "--format", "{{json .}}"])?;
        let mut networks = Network::from_ndjson(&String::from_utf8_lossy(&output.stdout))?;

        if !networks.is_empty() {
            let mut args = vec!["network".to_string(), "inspect".to_string()];
            args.extend(networks.iter().map(|network| network.id.clone()));
            if let Ok(inspect) = run_docker_owned(transport, &args) {
                let _ = Network::merge_inspect(
                    &mut networks,
                    &String::from_utf8_lossy(&inspect.stdout),
                );
            }
        }

        Ok(networks)
    }

    /// Runs `docker …` with `args` via `transport`.
    pub fn run(transport: &Transport, args: &[&str]) -> Result<Output, DockerError> {
        run_docker(transport, args)
    }
}

const EVENT_ARGS: &[&str] = &[
    "events",
    "--since",
    "30m",
    "--until",
    "0s",
    "--format",
    "{{json .}}",
    "--filter",
    "event=pull",
    "--filter",
    "event=create",
    "--filter",
    "event=start",
    "--filter",
    "event=die",
    "--filter",
    "event=oom",
    "--filter",
    "event=health_status",
    "--filter",
    "event=kill",
    "--filter",
    "event=destroy",
    "--filter",
    "event=restart",
];

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

fn run_docker_owned(transport: &Transport, args: &[String]) -> Result<Output, DockerError> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_docker(transport, &refs)
}

fn map_io_error(err: io::Error) -> DockerError {
    if err.kind() == io::ErrorKind::NotFound {
        DockerError::NotFound
    } else {
        DockerError::Io(err)
    }
}
