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
    /// A compressed office package exceeded one of the bounded reader limits.
    PackageTooLarge {
        /// The limit that rejected the package.
        limit: &'static str,
        /// Package or entry details useful to the caller.
        detail: String,
    },
    /// A domain edit could not be applied.
    DomainEdit(String),
    /// A document site could not be resolved or run.
    Site(String),
    /// A document surface scene or intent could not be projected or decoded.
    Surface(String),
}

impl fmt::Display for OfficeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShapeBuild(message) => write!(f, "could not build document shape: {message}"),
            Self::CapabilityDenied(capability) => write!(f, "capability denied: {capability}"),
            Self::Kernel(message) => write!(f, "kernel error: {message}"),
            Self::PackageTooLarge { limit, detail } => {
                write!(f, "office package exceeds {limit} limit: {detail}")
            }
            Self::DomainEdit(message) => write!(f, "domain edit failed: {message}"),
            Self::Site(message) => write!(f, "document site error: {message}"),
            Self::Surface(message) => write!(f, "document surface error: {message}"),
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
