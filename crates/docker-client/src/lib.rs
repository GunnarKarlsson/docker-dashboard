//! Docker CLI client for local and remote hosts.

mod docker;
mod error;

pub use docker::Docker;
pub use error::DockerError;
