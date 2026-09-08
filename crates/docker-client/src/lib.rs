//! Docker CLI client for local and remote hosts.

mod docker;
mod error;
mod version;

pub use docker::Docker;
pub use error::DockerError;
pub use version::{ClientVersion, DockerVersion, ServerVersion};
