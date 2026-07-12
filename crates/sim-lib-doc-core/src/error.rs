//! Error type shared by the office document core.

use std::fmt;

use sim_kernel::CapabilityName;

/// Error reported by the office document core.
#[derive(Clone, Debug)]
pub enum OfficeError {
    /// A kernel factory failed while building a shape value.
    ShapeBuild(String),
    /// A capability was required but absent from the active context.
    CapabilityDenied(CapabilityName),
    /// The kernel reported an error that does not have a narrower office-core
    /// category yet.
    Kernel(String),
    /// A domain edit could not be applied.
    DomainEdit(String),
}

impl fmt::Display for OfficeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShapeBuild(message) => write!(f, "could not build document shape: {message}"),
            Self::CapabilityDenied(capability) => write!(f, "capability denied: {capability}"),
            Self::Kernel(message) => write!(f, "kernel error: {message}"),
            Self::DomainEdit(message) => write!(f, "domain edit failed: {message}"),
        }
    }
}

impl std::error::Error for OfficeError {}

impl From<sim_kernel::Error> for OfficeError {
    fn from(error: sim_kernel::Error) -> Self {
        match error {
            sim_kernel::Error::CapabilityDenied { capability } => {
                Self::CapabilityDenied(capability)
            }
            other => Self::Kernel(other.to_string()),
        }
    }
}
