//! Authentication contracts for Microsoft Graph calls.

use crate::GraphError;

/// Supplies bearer tokens for live Microsoft Graph calls.
pub trait TokenProvider: Send + Sync {
    /// Returns a bearer token valid for the requested Microsoft Graph scopes.
    fn bearer(&self, scopes: &[&str]) -> Result<String, GraphError>;
}

/// Token provider backed by one pre-supplied bearer token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticTokenProvider {
    token: String,
}

impl StaticTokenProvider {
    /// Builds a static token provider.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl TokenProvider for StaticTokenProvider {
    fn bearer(&self, _scopes: &[&str]) -> Result<String, GraphError> {
        Ok(self.token.clone())
    }
}
