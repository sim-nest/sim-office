//! Line-delimited JSON bridge to a LibreOffice UNO helper process.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sim_kernel::{CapabilityName, Cx};
use sim_lib_doc_core::{ExternalRef, OfficeError, PROCESS_SPAWN_CAPABILITY};

use crate::site::LIBREOFFICE_SITE_ID;

/// Environment gate required before a live LibreOffice helper may spawn.
pub const LIBREOFFICE_BRIDGE_ENV: &str = "SIM_OFFICE_LIBREOFFICE_BRIDGE";
const OWNED_TEMP_PREFIX: &str = "sim-site-libreoffice-";

#[cfg(test)]
trait GrantOutcome {
    fn expect_granted(self);
}

#[cfg(test)]
impl GrantOutcome for () {
    fn expect_granted(self) {}
}

#[cfg(test)]
impl GrantOutcome for sim_kernel::Result<()> {
    fn expect_granted(self) {
        self.unwrap();
    }
}

#[cfg(test)]
macro_rules! expect_granted {
    ($grant:expr) => {{
        #[allow(clippy::let_unit_value)]
        let grant_result = $grant;
        #[allow(clippy::unit_arg)]
        grant_result.expect_granted();
    }};
}

/// LibreOffice helper process placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibreOfficeSite {
    /// Executable helper path.
    pub helper: PathBuf,
}

impl LibreOfficeSite {
    /// Builds a helper-process site from an executable path.
    #[must_use]
    pub fn new(helper: impl Into<PathBuf>) -> Self {
        Self {
            helper: helper.into(),
        }
    }
}

/// Command sent to the LibreOffice helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnoCommand {
    /// Open a document through LibreOffice.
    Open {
        /// Local document path.
        path: PathBuf,
    },
    /// Export an opened document reference to PDF.
    ExportPdf {
        /// Source document reference.
        doc: ExternalRef,
        /// Output PDF path.
        out: PathBuf,
    },
}

/// Runs one live UNO helper command.
pub fn run_uno(
    cx: &mut Cx,
    site: &LibreOfficeSite,
    cmd: UnoCommand,
) -> Result<ExternalRef, OfficeError> {
    let live_enabled = std::env::var(LIBREOFFICE_BRIDGE_ENV).as_deref() == Ok("1");
    run_uno_inner(cx, site, cmd, live_enabled)
}

fn run_uno_inner(
    cx: &mut Cx,
    site: &LibreOfficeSite,
    cmd: UnoCommand,
    live_enabled: bool,
) -> Result<ExternalRef, OfficeError> {
    cx.require(&CapabilityName::new(PROCESS_SPAWN_CAPABILITY))
        .map_err(OfficeError::from)?;
    if !live_enabled {
        return Err(OfficeError::Site(format!(
            "LibreOffice helper is disabled; set {LIBREOFFICE_BRIDGE_ENV}=1"
        )));
    }

    let request = HelperRequest::from_command(&cmd);
    let mut child = Command::new(&site.helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| helper_error(site, &cmd, format!("could not spawn helper: {err}")))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| helper_error(site, &cmd, "helper stdin was not captured"))?;
        serde_json::to_writer(&mut stdin, &request)
            .map_err(|err| helper_error(site, &cmd, format!("could not encode request: {err}")))?;
        stdin
            .write_all(b"\n")
            .map_err(|err| helper_error(site, &cmd, format!("could not write request: {err}")))?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| helper_error(site, &cmd, "helper stdout was not captured"))?;
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .map_err(|err| helper_error(site, &cmd, format!("could not read helper reply: {err}")))?;
    let output = child
        .wait_with_output()
        .map_err(|err| helper_error(site, &cmd, format!("could not wait for helper: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(helper_error(
            site,
            &cmd,
            format!("helper exited with {}: {stderr}", output.status),
        ));
    }

    let reply: HelperReply = serde_json::from_str(&line)
        .map_err(|err| helper_error(site, &cmd, format!("could not decode helper reply: {err}")))?;
    reply
        .into_result()
        .map_err(|message| helper_error(site, &cmd, message))
}

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum HelperRequest {
    Open { path: String },
    ExportPdf { doc: HelperExternalRef, out: String },
}

impl HelperRequest {
    fn from_command(cmd: &UnoCommand) -> Self {
        match cmd {
            UnoCommand::Open { path } => Self::Open {
                path: path.display().to_string(),
            },
            UnoCommand::ExportPdf { doc, out } => Self::ExportPdf {
                doc: HelperExternalRef::from(doc),
                out: out.display().to_string(),
            },
        }
    }
}

#[derive(Deserialize)]
struct HelperReply {
    backend: Option<String>,
    external_id: Option<String>,
    version: Option<String>,
    web_url: Option<String>,
    error: Option<String>,
}

impl HelperReply {
    fn into_result(self) -> Result<ExternalRef, String> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(ExternalRef::new(
            self.backend
                .unwrap_or_else(|| LIBREOFFICE_SITE_ID.to_owned()),
            self.external_id
                .ok_or_else(|| "helper reply missing external_id".to_owned())?,
            self.version,
            self.web_url,
        ))
    }
}

#[derive(Deserialize, Serialize)]
struct HelperExternalRef {
    backend: String,
    external_id: String,
    version: Option<String>,
    web_url: Option<String>,
}

impl From<&ExternalRef> for HelperExternalRef {
    fn from(value: &ExternalRef) -> Self {
        Self {
            backend: value.backend.clone(),
            external_id: value.external_id.clone(),
            version: value.version.clone(),
            web_url: value.web_url.clone(),
        }
    }
}

fn helper_error(
    site: &LibreOfficeSite,
    cmd: &UnoCommand,
    message: impl Into<String>,
) -> OfficeError {
    let mut message = message.into();
    message = redact_path(&message, &site.helper);
    for path in command_paths(cmd) {
        message = redact_path(&message, path);
    }
    OfficeError::Site(message)
}

fn command_paths(cmd: &UnoCommand) -> Vec<&Path> {
    match cmd {
        UnoCommand::Open { path } => vec![path.as_path()],
        UnoCommand::ExportPdf { out, .. } => vec![out.as_path()],
    }
}

fn redact_path(message: &str, path: &Path) -> String {
    if path_is_in_temp(path) {
        return message.to_owned();
    }
    let rendered = path.display().to_string();
    if rendered.is_empty() {
        message.to_owned()
    } else {
        message.replace(&rendered, "<redacted-path>")
    }
}

fn path_is_in_temp(path: &Path) -> bool {
    let temp = std::env::temp_dir();
    let Ok(relative) = path.strip_prefix(&temp) else {
        return false;
    };
    relative.components().next().is_some_and(|component| {
        matches!(
            component,
            std::path::Component::Normal(name)
                if name.to_string_lossy().starts_with(OWNED_TEMP_PREFIX)
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sim_kernel::{DefaultFactory, NoopEvalPolicy};

    use super::*;

    fn cx_with_process_spawn() -> Cx {
        let (mut cx, seat) = Cx::new_seated(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        expect_granted!(seat.grant(&mut cx, CapabilityName::new(PROCESS_SPAWN_CAPABILITY)));
        cx
    }

    fn cx_without_process_spawn() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn fake_helper_returns_export_receipt() {
        let mut cx = cx_with_process_spawn();
        let helper = fake_helper(
            "receipt",
            r#"{"backend":"site/libreoffice","external_id":"pdf:out","version":"1","web_url":null}"#,
        );
        let site = LibreOfficeSite::new(&helper);
        let doc = ExternalRef::new("codec/odf", "opened:1", None, None);

        let receipt = run_uno_inner(
            &mut cx,
            &site,
            UnoCommand::ExportPdf {
                doc,
                out: temp_path("export.pdf"),
            },
            true,
        )
        .unwrap();

        assert_eq!(receipt.backend, LIBREOFFICE_SITE_ID);
        assert_eq!(receipt.external_id, "pdf:out");
        assert_eq!(receipt.version, Some("1".to_owned()));
    }

    #[test]
    fn live_helper_is_denied_without_process_spawn_capability() {
        let mut cx = cx_without_process_spawn();
        let site = LibreOfficeSite::new(temp_path("helper"));

        let denied = run_uno_inner(
            &mut cx,
            &site,
            UnoCommand::Open {
                path: temp_path("input.ods"),
            },
            true,
        )
        .unwrap_err();

        assert!(matches!(
            denied,
            OfficeError::CapabilityDenied(capability)
                if capability.as_str() == PROCESS_SPAWN_CAPABILITY
        ));
    }

    #[test]
    fn helper_errors_redact_non_temp_paths() {
        let mut cx = cx_with_process_spawn();
        let helper = fake_helper(
            "error",
            r#"{"error":"could not open /home/bo/private/file.ods"}"#,
        );
        let site = LibreOfficeSite::new(&helper);

        let err = run_uno_inner(
            &mut cx,
            &site,
            UnoCommand::Open {
                path: PathBuf::from("/home/bo/private/file.ods"),
            },
            true,
        )
        .unwrap_err();
        let rendered = err.to_string();

        assert!(rendered.contains("<redacted-path>"));
        assert!(!rendered.contains("/home/bo/private/file.ods"));
    }

    #[test]
    fn redaction_leaves_only_owned_temp_paths_visible() {
        let owned = temp_path("input.ods");
        let owned_message = format!("could not open {}", owned.display());
        assert_eq!(redact_path(&owned_message, &owned), owned_message);

        let arbitrary_temp = std::env::temp_dir().join("private-file.ods");
        let arbitrary_message = format!("could not open {}", arbitrary_temp.display());
        let redacted = redact_path(&arbitrary_message, &arbitrary_temp);

        assert!(redacted.contains("<redacted-path>"));
        assert!(!redacted.contains(&arbitrary_temp.display().to_string()));
    }

    fn fake_helper(name: &str, reply: &str) -> PathBuf {
        let path = temp_path(&format!("{name}-helper.sh"));
        let script = format!("#!/bin/sh\nread _line\nprintf '%s\\n' '{reply}'\n");
        fs::write(&path, script).unwrap();
        make_executable(&path);
        path
    }

    fn temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{OWNED_TEMP_PREFIX}{stamp}-{name}"))
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
