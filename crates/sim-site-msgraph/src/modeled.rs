//! Deterministic modeled Microsoft Graph cassettes.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::client::{GraphError, redacted_body};

/// Recorded Microsoft Graph responses keyed by Graph path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cassette {
    responses: BTreeMap<String, CassetteResponse>,
}

impl Cassette {
    /// Builds an empty cassette.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a cassette containing one successful JSON response.
    #[must_use]
    pub fn with_json(path: impl Into<String>, body: Value) -> Self {
        Self::with_status(path, 200, body)
    }

    /// Builds a cassette containing one response with an explicit HTTP status.
    #[must_use]
    pub fn with_status(path: impl Into<String>, status: u16, body: Value) -> Self {
        let mut cassette = Self::new();
        cassette.insert(path, CassetteResponse::new(status, body));
        cassette
    }

    /// Inserts or replaces a response.
    pub fn insert(&mut self, path: impl Into<String>, response: CassetteResponse) {
        self.responses.insert(path.into(), response);
    }

    /// Reads a modeled response as if it came from Microsoft Graph.
    pub fn get(&self, path: &str) -> Result<Value, GraphError> {
        let response = self
            .responses
            .get(path)
            .ok_or_else(|| GraphError::MissingCassette {
                path: path.to_owned(),
            })?;
        if (200..300).contains(&response.status) {
            return Ok(response.body.clone());
        }
        let body = serde_json::to_string(&response.body)
            .unwrap_or_else(|error| format!("could not encode modeled body: {error}"));
        Err(GraphError::HttpStatus {
            status: response.status,
            body: redacted_body(&body, None),
        })
    }
}

/// One recorded Microsoft Graph HTTP response.
#[derive(Clone, Debug, PartialEq)]
pub struct CassetteResponse {
    /// HTTP status returned by the modeled service.
    pub status: u16,
    /// JSON body returned by the modeled service.
    pub body: Value,
}

impl CassetteResponse {
    /// Builds a recorded response.
    #[must_use]
    pub fn new(status: u16, body: Value) -> Self {
        Self { status, body }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn status_error_truncates_long_body() {
        let cassette = Cassette::with_status(
            "/me/drive/root",
            429,
            json!({
                "error": {
                    "code": "tooManyRequests",
                    "message": "rate limited",
                    "details": "x".repeat(400),
                }
            }),
        );

        let error = cassette.get("/me/drive/root").unwrap_err();

        let GraphError::HttpStatus { status, body } = error else {
            panic!("429 should become a status error");
        };
        assert_eq!(status, 429);
        assert!(body.contains("tooManyRequests"));
        assert!(body.contains("[truncated]"));
        assert!(!body.contains(&"x".repeat(220)));
    }
}
