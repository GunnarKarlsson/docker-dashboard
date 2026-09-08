//! Docker Compose CLI client (stub).

use thiserror::Error;

/// Placeholder error type for Compose commands.
#[derive(Debug, Error)]
pub enum ComposeError {
    #[error("compose client is not implemented yet")]
    Unimplemented,
}

/// Entry point for running `docker compose` commands.
pub struct Compose;

impl Compose {
    /// Placeholder availability check. Real Compose probing lands in a later step.
    pub fn check_available() -> Result<(), ComposeError> {
        Ok(())
    }
}
