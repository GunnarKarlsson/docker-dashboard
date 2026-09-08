//! AWS ECR client (stub).

use thiserror::Error;

/// Placeholder error type for ECR commands.
#[derive(Debug, Error)]
pub enum EcrError {
    #[error("ecr client is not implemented yet")]
    Unimplemented,
}

/// Entry point for running AWS ECR commands.
pub struct Ecr;

impl Ecr {
    /// Placeholder availability check. Real ECR probing lands in a later step.
    pub fn check_available() -> Result<(), EcrError> {
        Ok(())
    }
}
