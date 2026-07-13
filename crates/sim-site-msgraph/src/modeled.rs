//! Deterministic modeled Microsoft Graph cassettes.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::client::{GraphError, redacted_body};

/// Recorded Microsoft Graph responses keyed by Graph path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cassette {
    responses: BTreeMap<String, CassetteResponse>,
    post_responses: BTreeMap<String, CassetteResponse>,
    byte_responses: BTreeMap<String, CassetteBytesResponse>,
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

    /// Builds a cassette containing one successful JSON `POST` response.
    #[must_use]
    pub fn with_post_json(path: impl Into<String>, body: Value) -> Self {
        Self::with_post_status(path, 200, body)
    }

    /// Builds a cassette containing one successful byte response.
    #[must_use]
    pub fn with_bytes(path: impl Into<String>, body: Vec<u8>) -> Self {
        Self::with_bytes_status(path, 200, body)
    }

    /// Builds a cassette containing one response with an explicit HTTP status.
    #[must_use]
    pub fn with_status(path: impl Into<String>, status: u16, body: Value) -> Self {
        let mut cassette = Self::new();
        cassette.insert(path, CassetteResponse::new(status, body));
        cassette
    }

    /// Builds a cassette containing one `POST` response with an explicit HTTP status.
    #[must_use]
    pub fn with_post_status(path: impl Into<String>, status: u16, body: Value) -> Self {
        let mut cassette = Self::new();
        cassette.insert_post(path, CassetteResponse::new(status, body));
        cassette
    }

    /// Builds a cassette containing one byte response with an explicit status.
    #[must_use]
    pub fn with_bytes_status(path: impl Into<String>, status: u16, body: Vec<u8>) -> Self {
        let mut cassette = Self::new();
        cassette.insert_bytes(path, CassetteBytesResponse::new(status, body));
        cassette
    }

    /// Inserts or replaces a response.
    pub fn insert(&mut self, path: impl Into<String>, response: CassetteResponse) {
        self.responses.insert(path.into(), response);
    }

    /// Inserts or replaces a `POST` response.
    pub fn insert_post(&mut self, path: impl Into<String>, response: CassetteResponse) {
        self.post_responses.insert(path.into(), response);
    }

    /// Inserts or replaces a byte response.
    pub fn insert_bytes(&mut self, path: impl Into<String>, response: CassetteBytesResponse) {
        self.byte_responses.insert(path.into(), response);
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

    /// Reads a modeled `POST` response as if it came from Microsoft Graph.
    pub fn post(&self, path: &str, _body: &Value) -> Result<Value, GraphError> {
        let response =
            self.post_responses
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

    /// Reads a modeled byte response as if it came from Microsoft Graph.
    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, GraphError> {
        let response =
            self.byte_responses
                .get(path)
                .ok_or_else(|| GraphError::MissingCassette {
                    path: path.to_owned(),
                })?;
        if (200..300).contains(&response.status) {
            return Ok(response.body.clone());
        }
        Err(GraphError::HttpStatus {
            status: response.status,
            body: redacted_body(&String::from_utf8_lossy(&response.body), None),
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

/// One recorded Microsoft Graph byte response.
#[derive(Clone, Debug, PartialEq)]
pub struct CassetteBytesResponse {
    /// HTTP status returned by the modeled service.
    pub status: u16,
    /// Raw body returned by the modeled service.
    pub body: Vec<u8>,
}

impl CassetteBytesResponse {
    /// Builds a recorded byte response.
    #[must_use]
    pub fn new(status: u16, body: Vec<u8>) -> Self {
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

    #[test]
    fn modeled_bytes_are_deterministic() {
        let cassette = Cassette::with_bytes("/me/drive/items/deck-1/content", vec![1, 2, 3]);

        let first = cassette
            .get_bytes("/me/drive/items/deck-1/content")
            .unwrap();
        let second = cassette
            .get_bytes("/me/drive/items/deck-1/content")
            .unwrap();

        assert_eq!(first, vec![1, 2, 3]);
        assert_eq!(first, second);
    }

    #[test]
    fn modeled_posts_are_deterministic() {
        let cassette = Cassette::with_post_json("/me/messages", json!({ "id": "draft-1" }));

        let first = cassette
            .post("/me/messages", &json!({ "subject": "Hi" }))
            .unwrap();
        let second = cassette
            .post("/me/messages", &json!({ "subject": "Hi" }))
            .unwrap();

        assert_eq!(first, json!({ "id": "draft-1" }));
        assert_eq!(first, second);
    }
}
