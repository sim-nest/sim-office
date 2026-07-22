//! Deterministic modeled Dalux responses.

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

use crate::DaluxError;
use crate::client::{redacted_body, status_error};

/// Recorded Dalux responses keyed by API path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModeledDalux {
    get_responses: BTreeMap<String, ModeledResponse>,
    patch_responses: BTreeMap<String, ModeledPatch>,
}

impl ModeledDalux {
    /// Builds an empty modeled Dalux cassette.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a cassette containing one successful GET response.
    #[must_use]
    pub fn with_json(path: impl Into<String>, body: JsonValue) -> Self {
        Self::with_status(path, 200, body)
    }

    /// Builds a cassette containing one GET response with an explicit status.
    #[must_use]
    pub fn with_status(path: impl Into<String>, status: u16, body: JsonValue) -> Self {
        let mut modeled = Self::new();
        modeled.insert_get(path, ModeledResponse::new(status, body));
        modeled
    }

    /// Adds or replaces a GET response.
    pub fn insert_get(&mut self, path: impl Into<String>, response: ModeledResponse) {
        self.get_responses.insert(path.into(), response);
    }

    /// Adds or replaces a PATCH expectation and response.
    #[must_use]
    pub fn with_patch(
        mut self,
        path: impl Into<String>,
        expected_body: JsonValue,
        response: ModeledResponse,
    ) -> Self {
        self.insert_patch(path, expected_body, response);
        self
    }

    /// Adds or replaces a PATCH expectation and response.
    pub fn insert_patch(
        &mut self,
        path: impl Into<String>,
        expected_body: JsonValue,
        response: ModeledResponse,
    ) {
        self.patch_responses
            .insert(path.into(), ModeledPatch::new(expected_body, response));
    }

    /// Reads a modeled response.
    pub fn get(&self, path: &str, token: Option<&str>) -> Result<JsonValue, DaluxError> {
        let response = self.get_responses.get(path).ok_or_else(|| {
            DaluxError::Http(format!("modeled Dalux cassette has no path {path}"))
        })?;
        response.to_result(token)
    }

    /// Applies a modeled PATCH expectation.
    pub fn patch(
        &self,
        path: &str,
        body: &JsonValue,
        token: Option<&str>,
    ) -> Result<JsonValue, DaluxError> {
        let patch = self.patch_responses.get(path).ok_or_else(|| {
            DaluxError::Http(format!("modeled Dalux cassette has no PATCH path {path}"))
        })?;
        if &patch.expected_body != body {
            return Err(DaluxError::Http(format!(
                "modeled Dalux PATCH body mismatch: {}",
                redacted_body(&body.to_string(), token)
            )));
        }
        patch.response.to_result(token)
    }
}

/// Expected modeled PATCH request and response.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeledPatch {
    /// Expected JSON body for the PATCH request.
    pub expected_body: JsonValue,
    /// Response returned when the body matches.
    pub response: ModeledResponse,
}

impl ModeledPatch {
    /// Builds a modeled PATCH expectation.
    #[must_use]
    pub fn new(expected_body: JsonValue, response: ModeledResponse) -> Self {
        Self {
            expected_body,
            response,
        }
    }
}

/// One modeled Dalux HTTP response.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeledResponse {
    /// HTTP status returned by the modeled service.
    pub status: u16,
    /// JSON body returned by the modeled service.
    pub body: JsonValue,
}

impl ModeledResponse {
    /// Builds a modeled response.
    #[must_use]
    pub fn new(status: u16, body: JsonValue) -> Self {
        Self { status, body }
    }

    /// Builds a successful modeled response.
    #[must_use]
    pub fn ok(body: JsonValue) -> Self {
        Self::new(200, body)
    }

    fn to_result(&self, token: Option<&str>) -> Result<JsonValue, DaluxError> {
        if (200..300).contains(&self.status) {
            Ok(self.body.clone())
        } else {
            Err(status_error(self.status, &self.body, token))
        }
    }
}
