//! Deterministic SharePoint REST `_api/$batch` request bodies.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value as JsonValue;

use crate::SharePointError;
use crate::rest::api_url_for;

/// SharePoint REST batch endpoint below a site URL.
pub const BATCH_API_PATH: &str = "/_api/$batch";

const BATCH_BOUNDARY: &str = "batch_sim_sharepoint";
const DEFAULT_ACCEPT: &str = "application/json;odata=nometadata";
const HEADER_ACCEPT: &str = "Accept";
const HEADER_CONTENT_TYPE: &str = "Content-Type";
const HEADER_TRANSFER_ENCODING: &str = "Content-Transfer-Encoding";
const JSON_CONTENT_TYPE: &str = "application/json;odata=nometadata";

/// HTTP method accepted by a SharePoint REST batch operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestBatchMethod {
    /// Read an `_api` resource.
    Get,
    /// Create an `_api` resource.
    Post,
    /// Update an `_api` resource.
    Patch,
    /// Delete an `_api` resource.
    Delete,
}

impl RestBatchMethod {
    /// Returns the HTTP token used in the OData request line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

impl fmt::Display for RestBatchMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One site-local SharePoint REST operation inside an OData batch.
#[derive(Clone, Debug, PartialEq)]
pub struct RestBatchOp {
    /// HTTP method.
    pub method: RestBatchMethod,
    /// Site-local `_api` path.
    pub api_path: String,
    /// Additional operation headers, emitted in sorted order.
    pub headers: BTreeMap<String, String>,
    /// Optional JSON request body.
    pub body: Option<JsonValue>,
}

impl RestBatchOp {
    /// Builds a read operation.
    #[must_use]
    pub fn get(api_path: impl Into<String>) -> Self {
        Self::new(RestBatchMethod::Get, api_path, None)
    }

    /// Builds a create operation.
    #[must_use]
    pub fn post(api_path: impl Into<String>, body: JsonValue) -> Self {
        Self::new(RestBatchMethod::Post, api_path, Some(body))
    }

    /// Builds an update operation.
    #[must_use]
    pub fn patch(api_path: impl Into<String>, body: JsonValue) -> Self {
        Self::new(RestBatchMethod::Patch, api_path, Some(body))
    }

    /// Builds a delete operation.
    #[must_use]
    pub fn delete(api_path: impl Into<String>) -> Self {
        Self::new(RestBatchMethod::Delete, api_path, None)
    }

    /// Builds an operation with an explicit method and optional body.
    #[must_use]
    pub fn new(
        method: RestBatchMethod,
        api_path: impl Into<String>,
        body: Option<JsonValue>,
    ) -> Self {
        Self {
            method,
            api_path: api_path.into(),
            headers: BTreeMap::new(),
            body,
        }
    }

    /// Adds or replaces an operation header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

/// Builds a deterministic OData multipart batch body for site-local `_api` calls.
pub fn odata_batch_body(site_url: &str, ops: &[RestBatchOp]) -> Result<String, SharePointError> {
    if ops.is_empty() {
        return Err(crate::permission::rest_site_error(
            site_url,
            "SharePoint REST batch requires at least one operation",
        ));
    }

    let mut body = String::new();
    for op in ops {
        write_operation(&mut body, site_url, op)?;
    }
    body.push_str("--");
    body.push_str(BATCH_BOUNDARY);
    body.push_str("--\r\n");
    Ok(body)
}

/// Returns the multipart content type for `odata_batch_body`.
#[must_use]
pub fn odata_batch_content_type() -> String {
    format!("multipart/mixed; boundary={BATCH_BOUNDARY}")
}

fn write_operation(
    body: &mut String,
    site_url: &str,
    op: &RestBatchOp,
) -> Result<(), SharePointError> {
    let url = api_url_for(site_url, &op.api_path)?;
    body.push_str("--");
    body.push_str(BATCH_BOUNDARY);
    body.push_str("\r\nContent-Type: application/http\r\n");
    body.push_str(HEADER_TRANSFER_ENCODING);
    body.push_str(": binary\r\n\r\n");
    body.push_str(op.method.as_str());
    body.push(' ');
    body.push_str(&url);
    body.push_str(" HTTP/1.1\r\n");

    let headers = operation_headers(op);
    for (name, value) in headers {
        body.push_str(&name);
        body.push_str(": ");
        body.push_str(&value);
        body.push_str("\r\n");
    }
    body.push_str("\r\n");

    if let Some(json) = &op.body {
        body.push_str(&serde_json::to_string(json).map_err(|error| {
            crate::permission::rest_site_error(
                site_url,
                format!("could not encode SharePoint REST batch body: {error}"),
            )
        })?);
        body.push_str("\r\n");
    }
    Ok(())
}

fn operation_headers(op: &RestBatchOp) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    headers.insert(HEADER_ACCEPT.to_owned(), DEFAULT_ACCEPT.to_owned());
    if op.body.is_some() {
        headers.insert(HEADER_CONTENT_TYPE.to_owned(), JSON_CONTENT_TYPE.to_owned());
    }
    headers.extend(
        op.headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    headers
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn odata_batch_body_is_deterministic() {
        let body = odata_batch_body(
            "https://contoso.sharepoint.com/sites/design",
            &[
                RestBatchOp::get("_api/web/lists/getbytitle('Tasks')/items?$select=Title")
                    .with_header("X-RequestDigest", "digest-1"),
            ],
        )
        .unwrap();

        assert_eq!(
            body,
            "--batch_sim_sharepoint\r\n\
Content-Type: application/http\r\n\
Content-Transfer-Encoding: binary\r\n\r\n\
GET https://contoso.sharepoint.com/sites/design/_api/web/lists/getbytitle('Tasks')/items?$select=Title HTTP/1.1\r\n\
Accept: application/json;odata=nometadata\r\n\
X-RequestDigest: digest-1\r\n\r\n\
--batch_sim_sharepoint--\r\n"
        );
    }

    #[test]
    fn json_batch_body_adds_content_type() {
        let body = odata_batch_body(
            "https://contoso.sharepoint.com/sites/design",
            &[RestBatchOp::post(
                "_api/web/lists",
                json!({ "Title": "Tasks" }),
            )],
        )
        .unwrap();

        assert!(body.contains("Content-Type: application/json;odata=nometadata\r\n"));
        assert!(body.contains(r#"{"Title":"Tasks"}"#));
    }
}
