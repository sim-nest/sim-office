//! Microsoft Graph client boundary.

use std::fmt;
use std::io::Read;

use serde_json::Value;
use sim_kernel::{CapabilityName, Cx};
use sim_lib_deck::{DeckError, MsGraphSite as DeckMsGraphSite};
use sim_lib_doc_core::{CREDENTIALS_CAPABILITY, NET_CONNECT_CAPABILITY};
use sim_lib_mail::{MailError, MsGraphSite as MailMsGraphSite};
use sim_lib_sheet::{MsGraphSite as SheetMsGraphSite, SheetError};

use crate::{Cassette, TokenProvider};

/// Environment variable that must be set to `1` before live Graph calls run.
pub const GRAPH_LIVE_ENV: &str = "SIM_OFFICE_LIVE_MS_GRAPH";

/// Default Microsoft Graph application scope requested from token providers.
pub const GRAPH_DEFAULT_SCOPE: &str = "https://graph.microsoft.com/.default";

const MAX_ERROR_BODY_CHARS: usize = 160;

/// Execution mode for Microsoft Graph calls.
#[derive(Clone, Debug)]
pub enum GraphMode<T> {
    /// Deterministic responses recorded in a local cassette.
    Modeled(Cassette),
    /// Live Microsoft Graph access.
    Live {
        /// Base URL such as `https://graph.microsoft.com/v1.0`.
        base_url: String,
        /// Bearer-token provider owned by the host.
        token_provider: T,
    },
}

/// Error returned by the Microsoft Graph adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// A required capability was not granted.
    CapabilityDenied {
        /// Missing capability name.
        capability: CapabilityName,
    },
    /// Live calls are disabled until the environment gate is set.
    LiveDisabled {
        /// Name of the required environment variable.
        env: &'static str,
    },
    /// The Graph path is not a site-local absolute path.
    InvalidPath {
        /// Rejected path.
        path: String,
    },
    /// A modeled cassette did not contain the requested path.
    MissingCassette {
        /// Missing Graph path.
        path: String,
    },
    /// Microsoft Graph returned a non-success HTTP status.
    HttpStatus {
        /// HTTP status code.
        status: u16,
        /// Redacted response body.
        body: String,
    },
    /// The HTTP transport failed before a response was decoded.
    Transport {
        /// Redacted transport message.
        message: String,
    },
    /// A JSON response could not be decoded.
    Decode {
        /// Decoder message.
        message: String,
    },
    /// Token acquisition failed.
    Token {
        /// Redacted token-provider message.
        message: String,
    },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityDenied { capability } => {
                write!(f, "capability denied: {capability}")
            }
            Self::LiveDisabled { env } => {
                write!(f, "live Microsoft Graph is disabled: set {env}=1")
            }
            Self::InvalidPath { path } => write!(f, "invalid Microsoft Graph path: {path}"),
            Self::MissingCassette { path } => {
                write!(f, "modeled Microsoft Graph cassette has no path {path}")
            }
            Self::HttpStatus { status, body } => {
                write!(f, "Microsoft Graph returned HTTP {status}: {body}")
            }
            Self::Transport { message } => write!(f, "Microsoft Graph transport failed: {message}"),
            Self::Decode { message } => {
                write!(f, "Microsoft Graph response decode failed: {message}")
            }
            Self::Token { message } => write!(f, "Microsoft Graph token failed: {message}"),
        }
    }
}

impl std::error::Error for GraphError {}

impl From<sim_kernel::Error> for GraphError {
    fn from(error: sim_kernel::Error) -> Self {
        match error {
            sim_kernel::Error::CapabilityDenied { capability } => {
                Self::CapabilityDenied { capability }
            }
            other => Self::Transport {
                message: other.to_string(),
            },
        }
    }
}

/// Runs one Microsoft Graph `GET` call in modeled or live mode.
pub fn graph_get<T: TokenProvider>(
    cx: &mut Cx,
    mode: &GraphMode<T>,
    path: &str,
) -> Result<Value, GraphError> {
    validate_graph_path(path)?;
    match mode {
        GraphMode::Modeled(cassette) => cassette.get(path),
        GraphMode::Live {
            base_url,
            token_provider,
        } => live_graph_get(cx, base_url, token_provider, path),
    }
}

/// Runs one Microsoft Graph `POST` call in modeled or live mode.
pub fn graph_post<T: TokenProvider>(
    cx: &mut Cx,
    mode: &GraphMode<T>,
    path: &str,
    body: &Value,
) -> Result<Value, GraphError> {
    validate_graph_path(path)?;
    match mode {
        GraphMode::Modeled(cassette) => cassette.post(path, body),
        GraphMode::Live {
            base_url,
            token_provider,
        } => live_graph_post(cx, base_url, token_provider, path, body),
    }
}

/// Runs one Microsoft Graph `GET` call in modeled or live mode and returns bytes.
pub fn graph_get_bytes<T: TokenProvider>(
    cx: &mut Cx,
    mode: &GraphMode<T>,
    path: &str,
) -> Result<Vec<u8>, GraphError> {
    validate_graph_path(path)?;
    match mode {
        GraphMode::Modeled(cassette) => cassette.get_bytes(path),
        GraphMode::Live {
            base_url,
            token_provider,
        } => live_graph_get_bytes(cx, base_url, token_provider, path),
    }
}

impl<T: TokenProvider> SheetMsGraphSite for GraphMode<T> {
    fn graph_get(&self, cx: &mut Cx, path: &str) -> Result<Value, SheetError> {
        graph_get(cx, self, path)
            .map_err(|error| SheetError::WrongDocBody(format!("Microsoft Graph read: {error}")))
    }
}

impl<T: TokenProvider> DeckMsGraphSite for GraphMode<T> {
    fn graph_get_bytes(&self, cx: &mut Cx, path: &str) -> Result<Vec<u8>, DeckError> {
        graph_get_bytes(cx, self, path)
            .map_err(|error| DeckError::GraphFile(format!("Microsoft Graph file read: {error}")))
    }
}

impl<T: TokenProvider> MailMsGraphSite for GraphMode<T> {
    fn graph_get(&self, cx: &mut Cx, path: &str) -> Result<Value, MailError> {
        graph_get(cx, self, path)
            .map_err(|error| MailError::WrongDocBody(format!("Microsoft Graph mail read: {error}")))
    }

    fn graph_post(&self, cx: &mut Cx, path: &str, body: &Value) -> Result<Value, MailError> {
        graph_post(cx, self, path, body).map_err(|error| {
            MailError::WrongDocBody(format!("Microsoft Graph mail write: {error}"))
        })
    }
}

fn live_graph_get<T: TokenProvider>(
    cx: &Cx,
    base_url: &str,
    token_provider: &T,
    path: &str,
) -> Result<Value, GraphError> {
    require_live_gate(cx)?;
    let token = token_provider
        .bearer(&[GRAPH_DEFAULT_SCOPE])
        .map_err(|error| GraphError::Token {
            message: error.to_string(),
        })?;
    let url = graph_url(base_url, path)?;
    let auth = format!("Bearer {token}");
    let response = ureq::get(&url)
        .set("Accept", "application/json")
        .set("Authorization", &auth)
        .call();

    match response {
        Ok(response) => decode_response(response.status(), response.into_string(), Some(&token)),
        Err(ureq::Error::Status(status, response)) => {
            decode_status_error(status, response.into_string(), Some(&token))
        }
        Err(error) => Err(GraphError::Transport {
            message: redacted_body(&error.to_string(), Some(&token)),
        }),
    }
}

fn live_graph_post<T: TokenProvider>(
    cx: &Cx,
    base_url: &str,
    token_provider: &T,
    path: &str,
    body: &Value,
) -> Result<Value, GraphError> {
    require_live_gate(cx)?;
    let token = token_provider
        .bearer(&[GRAPH_DEFAULT_SCOPE])
        .map_err(|error| GraphError::Token {
            message: error.to_string(),
        })?;
    let url = graph_url(base_url, path)?;
    let auth = format!("Bearer {token}");
    let response = ureq::post(&url)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .set("Authorization", &auth)
        .send_string(&body.to_string());

    match response {
        Ok(response) => decode_response(response.status(), response.into_string(), Some(&token)),
        Err(ureq::Error::Status(status, response)) => {
            decode_status_error(status, response.into_string(), Some(&token))
        }
        Err(error) => Err(GraphError::Transport {
            message: redacted_body(&error.to_string(), Some(&token)),
        }),
    }
}

fn live_graph_get_bytes<T: TokenProvider>(
    cx: &Cx,
    base_url: &str,
    token_provider: &T,
    path: &str,
) -> Result<Vec<u8>, GraphError> {
    require_live_gate(cx)?;
    let token = token_provider
        .bearer(&[GRAPH_DEFAULT_SCOPE])
        .map_err(|error| GraphError::Token {
            message: error.to_string(),
        })?;
    let url = graph_url(base_url, path)?;
    let auth = format!("Bearer {token}");
    let response = ureq::get(&url)
        .set(
            "Accept",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        )
        .set("Authorization", &auth)
        .call();

    match response {
        Ok(response) => {
            decode_byte_response(response.status(), response.into_reader(), Some(&token))
        }
        Err(ureq::Error::Status(status, response)) => {
            decode_status_error_bytes(status, response.into_reader(), Some(&token))
        }
        Err(error) => Err(GraphError::Transport {
            message: redacted_body(&error.to_string(), Some(&token)),
        }),
    }
}

fn require_live_gate(cx: &Cx) -> Result<(), GraphError> {
    require_capability(cx, NET_CONNECT_CAPABILITY)?;
    require_capability(cx, CREDENTIALS_CAPABILITY)?;
    if std::env::var(GRAPH_LIVE_ENV).ok().as_deref() == Some("1") {
        Ok(())
    } else {
        Err(GraphError::LiveDisabled {
            env: GRAPH_LIVE_ENV,
        })
    }
}

fn require_capability(cx: &Cx, capability: &str) -> Result<(), GraphError> {
    cx.require(&CapabilityName::new(capability.to_owned()))
        .map_err(GraphError::from)
}

fn graph_url(base_url: &str, path: &str) -> Result<String, GraphError> {
    if base_url.trim().is_empty() {
        return Err(GraphError::InvalidPath {
            path: base_url.to_owned(),
        });
    }
    Ok(format!("{}{}", base_url.trim_end_matches('/'), path))
}

fn validate_graph_path(path: &str) -> Result<(), GraphError> {
    if path.starts_with('/') && !path.contains("://") {
        Ok(())
    } else {
        Err(GraphError::InvalidPath {
            path: path.to_owned(),
        })
    }
}

fn decode_response(
    status: u16,
    body: Result<String, std::io::Error>,
    token: Option<&str>,
) -> Result<Value, GraphError> {
    let body = body.map_err(|error| GraphError::Transport {
        message: redacted_body(&error.to_string(), token),
    })?;
    if !(200..300).contains(&status) {
        return Err(GraphError::HttpStatus {
            status,
            body: redacted_body(&body, token),
        });
    }
    serde_json::from_str(&body).map_err(|error| GraphError::Decode {
        message: redacted_body(&error.to_string(), token),
    })
}

fn decode_status_error(
    status: u16,
    body: Result<String, std::io::Error>,
    token: Option<&str>,
) -> Result<Value, GraphError> {
    let body = body
        .map(|body| redacted_body(&body, token))
        .unwrap_or_else(|error| redacted_body(&error.to_string(), token));
    Err(GraphError::HttpStatus { status, body })
}

fn decode_byte_response(
    status: u16,
    mut body: impl Read,
    token: Option<&str>,
) -> Result<Vec<u8>, GraphError> {
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .map_err(|error| GraphError::Transport {
            message: redacted_body(&error.to_string(), token),
        })?;
    if !(200..300).contains(&status) {
        return Err(GraphError::HttpStatus {
            status,
            body: redacted_body(&String::from_utf8_lossy(&bytes), token),
        });
    }
    Ok(bytes)
}

fn decode_status_error_bytes(
    status: u16,
    mut body: impl Read,
    token: Option<&str>,
) -> Result<Vec<u8>, GraphError> {
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .map_err(|error| GraphError::Transport {
            message: redacted_body(&error.to_string(), token),
        })?;
    Err(GraphError::HttpStatus {
        status,
        body: redacted_body(&String::from_utf8_lossy(&bytes), token),
    })
}

pub(crate) fn redacted_body(body: &str, token: Option<&str>) -> String {
    let mut redacted = match token {
        Some(token) if !token.is_empty() => body.replace(token, "[redacted-token]"),
        _ => body.to_owned(),
    };
    if redacted.chars().count() > MAX_ERROR_BODY_CHARS {
        redacted = redacted.chars().take(MAX_ERROR_BODY_CHARS).collect();
        redacted.push_str("...[truncated]");
    }
    redacted
}
