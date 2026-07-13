//! Dalux API-identity client boundary.

use serde_json::{Value as JsonValue, json};
use sim_kernel::{CapabilityName, Cx};
use sim_lib_doc_core::{CREDENTIALS_CAPABILITY, Doc, ExternalRef, NET_CONNECT_CAPABILITY};

use crate::DaluxError;
use crate::model::{item_path, items_doc, patch_external_ref, project_items_path};
use crate::modeled::ModeledDalux;

/// Environment variable that must be set to `1` before live Dalux calls run.
pub const DALUX_LIVE_ENV: &str = "SIM_OFFICE_LIVE_DALUX";

const MAX_ERROR_BODY_CHARS: usize = 180;
const MAX_JSON_STRING_CHARS: usize = 96;

/// Supplies API-identity bearer tokens for Dalux calls.
pub trait DaluxCredentialProvider: Send + Sync {
    /// Returns a bearer token for the Dalux API identity.
    fn access_token(&self) -> Result<String, DaluxError>;
}

/// Static Dalux credential provider used by tests and host adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticDaluxCredentialProvider {
    token: Option<String>,
    company_api_key: bool,
}

impl StaticDaluxCredentialProvider {
    /// Builds a static API-identity bearer token provider.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: Some(token.into()),
            company_api_key: false,
        }
    }

    /// Builds a rejected company API key provider.
    #[must_use]
    pub fn company_api_key(_key: impl Into<String>) -> Self {
        Self {
            token: None,
            company_api_key: true,
        }
    }
}

impl DaluxCredentialProvider for StaticDaluxCredentialProvider {
    fn access_token(&self) -> Result<String, DaluxError> {
        if self.company_api_key {
            return Err(DaluxError::CompanyApiKeyUnsupported);
        }
        self.token
            .clone()
            .ok_or_else(|| DaluxError::Credentials("missing API identity token".to_owned()))
    }
}

/// Execution mode for Dalux API calls.
#[derive(Clone, Debug, PartialEq)]
pub enum DaluxClientMode {
    /// Deterministic modeled responses.
    Modeled(ModeledDalux),
    /// Live HTTP access to Dalux.
    Live,
}

/// Dalux client configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct DaluxClient<C> {
    /// Base URL such as `https://api.dalux.com`.
    pub base_url: String,
    /// API identity credential provider.
    pub credentials: C,
    /// Modeled or live execution mode.
    pub mode: DaluxClientMode,
}

impl<C> DaluxClient<C> {
    /// Builds a live Dalux client.
    #[must_use]
    pub fn live(base_url: impl Into<String>, credentials: C) -> Self {
        Self {
            base_url: base_url.into(),
            credentials,
            mode: DaluxClientMode::Live,
        }
    }

    /// Builds a modeled Dalux client backed by deterministic responses.
    #[must_use]
    pub fn modeled(modeled: ModeledDalux, credentials: C) -> Self {
        Self {
            base_url: "https://example.com/dalux".to_owned(),
            credentials,
            mode: DaluxClientMode::Modeled(modeled),
        }
    }
}

impl<C: DaluxCredentialProvider> DaluxClient<C> {
    pub(crate) fn get_json(&self, cx: &mut Cx, path: &str) -> Result<JsonValue, DaluxError> {
        validate_path(path)?;
        match &self.mode {
            DaluxClientMode::Modeled(modeled) => {
                let token = self.credentials.access_token()?;
                modeled.get(path, Some(&token))
            }
            DaluxClientMode::Live => {
                require_live_gate(cx)?;
                let token = self.credentials.access_token()?;
                live_get_json(&self.base_url, path, &token)
            }
        }
    }

    pub(crate) fn patch_json(
        &self,
        cx: &mut Cx,
        path: &str,
        body: &JsonValue,
    ) -> Result<JsonValue, DaluxError> {
        validate_path(path)?;
        match &self.mode {
            DaluxClientMode::Modeled(modeled) => {
                let token = self.credentials.access_token()?;
                modeled.patch(path, body, Some(&token))
            }
            DaluxClientMode::Live => {
                require_live_gate(cx)?;
                let token = self.credentials.access_token()?;
                live_patch_json(&self.base_url, path, body, &token)
            }
        }
    }
}

/// Reads Dalux project items and projects them into a local office document.
pub fn get_project_items<C: DaluxCredentialProvider>(
    cx: &mut Cx,
    client: &DaluxClient<C>,
    project_id: &str,
) -> Result<Doc, DaluxError> {
    let path = project_items_path(project_id)?;
    let body = client.get_json(cx, &path)?;
    items_doc(cx, project_id, &body)
}

/// Patches the note field for one Dalux item and returns an external reference.
pub fn patch_item_note<C: DaluxCredentialProvider>(
    cx: &mut Cx,
    client: &DaluxClient<C>,
    item_id: &str,
    note: &str,
) -> Result<ExternalRef, DaluxError> {
    let path = item_path(item_id)?;
    let body = json!({ "note": note });
    let response = client.patch_json(cx, &path, &body)?;
    patch_external_ref(item_id, &response)
}

/// Redacts bearer tokens, long JSON strings, and long bodies from Dalux errors.
#[must_use]
pub fn redacted_body(body: &str, token: Option<&str>) -> String {
    let mut redacted = match token {
        Some(token) if !token.is_empty() => body.replace(token, "[redacted-token]"),
        _ => body.to_owned(),
    };
    redacted = redact_long_json_strings(&redacted);
    if redacted.chars().count() > MAX_ERROR_BODY_CHARS {
        redacted = redacted.chars().take(MAX_ERROR_BODY_CHARS).collect();
        redacted.push_str("...[truncated]");
    }
    redacted
}

pub(crate) fn status_error(status: u16, body: &JsonValue, token: Option<&str>) -> DaluxError {
    let body = serde_json::to_string(body)
        .unwrap_or_else(|error| format!("could not encode Dalux error body: {error}"));
    DaluxError::Http(format!("HTTP {status}: {}", redacted_body(&body, token)))
}

fn live_get_json(base_url: &str, path: &str, token: &str) -> Result<JsonValue, DaluxError> {
    let url = api_url(base_url, path)?;
    let auth = format!("Bearer {token}");
    let response = ureq::get(&url)
        .set("Accept", "application/json")
        .set("Authorization", &auth)
        .call();
    decode_response(response, token)
}

fn live_patch_json(
    base_url: &str,
    path: &str,
    body: &JsonValue,
    token: &str,
) -> Result<JsonValue, DaluxError> {
    let url = api_url(base_url, path)?;
    let auth = format!("Bearer {token}");
    let response = ureq::request("PATCH", &url)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .set("Authorization", &auth)
        .send_string(&body.to_string());
    decode_response(response, token)
}

fn decode_response(
    response: Result<ureq::Response, ureq::Error>,
    token: &str,
) -> Result<JsonValue, DaluxError> {
    match response {
        Ok(response) => decode_status_body(response.status(), response.into_string(), token),
        Err(ureq::Error::Status(status, response)) => {
            decode_status_body(status, response.into_string(), token)
        }
        Err(error) => Err(DaluxError::Http(redacted_body(
            &error.to_string(),
            Some(token),
        ))),
    }
}

fn decode_status_body(
    status: u16,
    body: Result<String, std::io::Error>,
    token: &str,
) -> Result<JsonValue, DaluxError> {
    let body =
        body.map_err(|error| DaluxError::Http(redacted_body(&error.to_string(), Some(token))))?;
    if !(200..300).contains(&status) {
        return Err(DaluxError::Http(format!(
            "HTTP {status}: {}",
            redacted_body(&body, Some(token))
        )));
    }
    serde_json::from_str(&body)
        .map_err(|error| DaluxError::Http(redacted_body(&error.to_string(), Some(token))))
}

fn require_live_gate(cx: &Cx) -> Result<(), DaluxError> {
    require_capability(cx, NET_CONNECT_CAPABILITY)?;
    require_capability(cx, CREDENTIALS_CAPABILITY)?;
    if std::env::var(DALUX_LIVE_ENV).ok().as_deref() == Some("1") {
        Ok(())
    } else {
        Err(DaluxError::Http(format!(
            "live Dalux access is disabled: set {DALUX_LIVE_ENV}=1"
        )))
    }
}

fn require_capability(cx: &Cx, capability: &str) -> Result<(), DaluxError> {
    cx.require(&CapabilityName::new(capability.to_owned()))
        .map_err(|error| DaluxError::Http(error.to_string()))
}

fn api_url(base_url: &str, path: &str) -> Result<String, DaluxError> {
    let base = base_url.trim().trim_end_matches('/');
    if !base.starts_with("https://") || base.contains('?') || base.contains('#') {
        return Err(DaluxError::InvalidTarget(format!(
            "invalid Dalux base URL {base_url:?}"
        )));
    }
    Ok(format!("{base}{path}"))
}

fn validate_path(path: &str) -> Result<(), DaluxError> {
    if path.starts_with('/') && !path.contains("://") {
        Ok(())
    } else {
        Err(DaluxError::InvalidTarget(format!(
            "invalid Dalux API path {path:?}"
        )))
    }
}

fn redact_long_json_strings(body: &str) -> String {
    let mut redacted = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            redacted.push(ch);
            continue;
        }
        let mut content = String::new();
        let mut closed = false;
        while let Some(next) = chars.next() {
            if next == '\\' {
                content.push(next);
                if let Some(escaped) = chars.next() {
                    content.push(escaped);
                }
            } else if next == '"' {
                closed = true;
                break;
            } else {
                content.push(next);
            }
        }
        if content.chars().count() > MAX_JSON_STRING_CHARS {
            redacted.push_str("\"[redacted-long-field]\"");
        } else {
            redacted.push('"');
            redacted.push_str(&content);
            if closed {
                redacted.push('"');
            }
        }
    }
    redacted
}
