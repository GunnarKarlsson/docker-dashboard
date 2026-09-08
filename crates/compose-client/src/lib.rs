//! Docker Compose CLI client.

mod snapshot;

pub use snapshot::{ComposeProject, ComposeService, ProjectStatus};

use docker_client::{Docker, DockerError, Transport};
use snapshot::load_project;

/// Entry point for running `docker compose` commands.
pub struct Compose;

impl Compose {
    /// Verifies that `docker compose` is runnable on the local host.
    pub fn check_available() -> Result<(), DockerError> {
        Docker::run(&Transport::Local, &["compose", "version"]).map(|_| ())
    }

    /// Discovers a Compose project (preferring `dd-mock`) and loads its services.
    pub fn snapshot(transport: &Transport) -> Result<Option<ComposeProject>, DockerError> {
        load_project(transport)
    }
}
