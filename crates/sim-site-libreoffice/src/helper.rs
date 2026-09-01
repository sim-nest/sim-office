//! Portable request boundary for a LibreOffice UNO helper.

use crate::site::LIBREOFFICE_SITE_ID;
use serde::{Deserialize, Serialize};
use sim_kernel::{CapabilityName, Cx};
use sim_lib_doc_core::{ExternalRef, OfficeError, PROCESS_SPAWN_CAPABILITY};
use sim_lib_exec::{
    ProcessAttempt, ProcessBudget, ProcessCancellation, ProcessPort, ProcessRefusal,
    ProcessRequest, ProgramRef, ProjectRootRef, SealedBindings,
};
use sim_transport_ports::{IpcAddress, IpcPort, TransportErrorKind};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

/// Explicit, preopened filesystem roots supplied to the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibreOfficeMounts {
    /// Root containing the helper and its immutable support files.
    pub helper_root: PathBuf,
    /// Root containing admitted input documents.
    pub input_root: PathBuf,
    /// Root receiving helper output.
    pub output_root: PathBuf,
}
/// Bounded helper configuration supplied by the site assembler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibreOfficeConfig {
    /// Exact helper path beneath the helper root.
    pub helper: PathBuf,
    /// Exact diminished environment; nothing is inherited.
    pub environment: BTreeMap<String, String>,
    /// Monotonic request deadline.
    pub timeout_ms: u64,
    /// Combined output and diagnostic byte budget.
    pub max_output_bytes: usize,
}
/// Realization selected explicitly by the platform/site composition layer.
pub enum LibreOfficeTransport<'a> {
    /// Confined process realization.
    Process(&'a dyn ProcessPort),
    /// Already activated helper reached over capsule-owned IPC.
    Ipc {
        /// Platform IPC realization.
        port: &'a dyn IpcPort,
        /// Exact platform-specific address.
        address: IpcAddress,
    },
}
/// Privacy-safe receipt for a completed helper exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibreOfficeReceipt {
    /// Privacy-safe platform provider id.
    pub provider: String,
    /// Platform-reported elapsed monotonic time, when available.
    pub elapsed_mono_ns: Option<u64>,
    /// Office-domain result.
    pub external: ExternalRef,
}
/// Typed office-domain refusal from the helper boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibreOfficeError {
    /// Required capability was absent.
    Denied(CapabilityName),
    /// Configured helper could not be realized.
    MissingHelper,
    /// Bounded request deadline expired.
    Timeout,
    /// Local IPC was absent or refused the address.
    IpcUnavailable,
    /// Helper protocol or policy refused the request.
    HelperRefused(String),
}
impl std::fmt::Display for LibreOfficeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(c) => write!(f, "capability denied: {c}"),
            Self::MissingHelper => f.write_str("LibreOffice helper is unavailable"),
            Self::Timeout => f.write_str("LibreOffice helper timed out"),
            Self::IpcUnavailable => f.write_str("LibreOffice local IPC is unavailable"),
            Self::HelperRefused(d) => write!(f, "LibreOffice helper refused request: {d}"),
        }
    }
}
impl std::error::Error for LibreOfficeError {}
/// LibreOffice site policy and explicit platform resources.
pub struct LibreOfficeSite<'a> {
    /// Bounded helper configuration.
    pub config: LibreOfficeConfig,
    /// Preopened roots admitted to office policy.
    pub mounts: LibreOfficeMounts,
    /// Explicit platform realization.
    pub transport: LibreOfficeTransport<'a>,
}
/// Command sent to the LibreOffice helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnoCommand {
    /// Open an admitted input document.
    Open {
        /// Path beneath the supplied input mount.
        path: PathBuf,
    },
    /// Export an opened document into the output mount.
    ExportPdf {
        /// Previously opened office document reference.
        doc: ExternalRef,
        /// Destination beneath the supplied output mount.
        out: PathBuf,
    },
}

/// Runs one bounded helper exchange and returns its ledger-ready receipt.
pub fn run_uno(
    cx: &mut Cx,
    site: &LibreOfficeSite<'_>,
    cmd: UnoCommand,
) -> Result<LibreOfficeReceipt, LibreOfficeError> {
    validate_site(site, &cmd)?;
    let mut framed = serde_json::to_vec(&HelperRequest::from_command(&cmd))
        .map_err(|e| LibreOfficeError::HelperRefused(e.to_string()))?;
    framed.push(b'\n');
    let (provider, elapsed_mono_ns, reply) = match &site.transport {
        LibreOfficeTransport::Process(port) => {
            cx.require(&CapabilityName::new(PROCESS_SPAWN_CAPABILITY))
                .map_err(|e| match e {
                    sim_kernel::Error::CapabilityDenied { capability } => {
                        LibreOfficeError::Denied(capability)
                    }
                    other => LibreOfficeError::HelperRefused(other.to_string()),
                })?;
            let request = ProcessRequest {
                program: ProgramRef::new(site.config.helper.display().to_string())
                    .map_err(|error| LibreOfficeError::HelperRefused(error.to_string()))?,
                argv: Vec::new(),
                root: ProjectRootRef::new(site.mounts.helper_root.display().to_string())
                    .map_err(|error| LibreOfficeError::HelperRefused(error.to_string()))?,
                environment: SealedBindings::literals(site.config.environment.clone())
                    .map_err(|error| LibreOfficeError::HelperRefused(error.to_string()))?,
                private_artifacts: Vec::new(),
                budget: ProcessBudget {
                    timeout_ms: site.config.timeout_ms,
                    max_output_bytes: site.config.max_output_bytes,
                    stdin: Some(framed),
                },
            };
            let receipt = process_receipt(port.run(&request, &ProcessCancellation::default()))?;
            if receipt.result.exit_code != 0 {
                return Err(LibreOfficeError::HelperRefused(redact(
                    &receipt.result.stderr,
                )));
            }
            (
                receipt.provider,
                Some(receipt.elapsed_mono_ns),
                receipt.result.stdout,
            )
        }
        LibreOfficeTransport::Ipc { port, address } => {
            let mut stream = port.connect(address).map_err(|e| match e.kind {
                TransportErrorKind::NotFound
                | TransportErrorKind::ConnectionRefused
                | TransportErrorKind::Unsupported => LibreOfficeError::IpcUnavailable,
                TransportErrorKind::TimedOut => LibreOfficeError::Timeout,
                _ => LibreOfficeError::HelperRefused(redact(&e.detail)),
            })?;
            stream
                .set_read_timeout(Some(std::time::Duration::from_millis(
                    site.config.timeout_ms,
                )))
                .map_err(|_| LibreOfficeError::IpcUnavailable)?;
            stream
                .write_all(&framed)
                .map_err(|_| LibreOfficeError::IpcUnavailable)?;
            stream
                .flush()
                .map_err(|_| LibreOfficeError::IpcUnavailable)?;
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).map_err(|e| {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    LibreOfficeError::Timeout
                } else {
                    LibreOfficeError::IpcUnavailable
                }
            })?;
            ("platform/ipc".to_owned(), None, line)
        }
    };
    let external = serde_json::from_str::<HelperReply>(&reply)
        .map_err(|e| LibreOfficeError::HelperRefused(e.to_string()))?
        .into_result()
        .map_err(LibreOfficeError::HelperRefused)?;
    Ok(LibreOfficeReceipt {
        provider,
        elapsed_mono_ns,
        external,
    })
}
fn validate_site(site: &LibreOfficeSite<'_>, cmd: &UnoCommand) -> Result<(), LibreOfficeError> {
    if site.config.timeout_ms == 0 || site.config.max_output_bytes == 0 {
        return Err(LibreOfficeError::HelperRefused("zero helper budget".into()));
    }
    if !site.config.helper.starts_with(&site.mounts.helper_root) {
        return Err(LibreOfficeError::MissingHelper);
    }
    let admitted = match cmd {
        UnoCommand::Open { path } => path.starts_with(&site.mounts.input_root),
        UnoCommand::ExportPdf { out, .. } => out.starts_with(&site.mounts.output_root),
    };
    if !admitted {
        return Err(LibreOfficeError::HelperRefused(
            "document path is outside supplied mounts".into(),
        ));
    }
    Ok(())
}
fn process_receipt(
    attempt: ProcessAttempt,
) -> Result<sim_lib_exec::ProcessReceipt, LibreOfficeError> {
    match attempt {
        ProcessAttempt::Completed { receipt } => Ok(receipt),
        ProcessAttempt::NotDispatched {
            refusal: ProcessRefusal::SpawnFailed(_),
        } => Err(LibreOfficeError::MissingHelper),
        ProcessAttempt::NotDispatched { refusal } => Err(LibreOfficeError::HelperRefused(redact(
            &format!("{refusal:?}"),
        ))),
        ProcessAttempt::StoppedAfterTimeout { .. } => Err(LibreOfficeError::Timeout),
        ProcessAttempt::StoppedAfterCancel { .. } => Err(LibreOfficeError::HelperRefused(
            "request cancelled after proven cleanup".into(),
        )),
        ProcessAttempt::UnknownAfterDispatch { evidence } => Err(LibreOfficeError::HelperRefused(
            redact(&format!("{evidence:?}")),
        )),
    }
}
fn redact(detail: &str) -> String {
    if detail.is_empty() {
        "provider failure".into()
    } else {
        "provider failure (details redacted)".into()
    }
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
        if let Some(e) = self.error {
            return Err(redact(&e));
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
    fn from(v: &ExternalRef) -> Self {
        Self {
            backend: v.backend.clone(),
            external_id: v.external_id.clone(),
            version: v.version.clone(),
            web_url: v.web_url.clone(),
        }
    }
}
impl From<LibreOfficeError> for OfficeError {
    fn from(e: LibreOfficeError) -> Self {
        match e {
            LibreOfficeError::Denied(c) => Self::CapabilityDenied(c),
            other => Self::Site(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_lib_exec::{ProcResult, ProcessReceipt, StopReceipt};
    use sim_transport_ports::{IpcListener, Stream, TransportError};
    struct ModelProcess(ProcessAttempt);
    impl ProcessPort for ModelProcess {
        fn run(&self, _: &ProcessRequest, _: &ProcessCancellation) -> ProcessAttempt {
            self.0.clone()
        }
    }
    struct MissingIpc;
    impl IpcPort for MissingIpc {
        fn listen(&self, _: &IpcAddress) -> sim_transport_ports::Result<Box<dyn IpcListener>> {
            Err(TransportError::new(
                TransportErrorKind::Unsupported,
                "model has no local helper",
            ))
        }
        fn connect(&self, _: &IpcAddress) -> sim_transport_ports::Result<Box<dyn Stream>> {
            Err(TransportError::new(
                TransportErrorKind::ConnectionRefused,
                "model has no local helper",
            ))
        }
    }
    fn site(port: &dyn ProcessPort) -> LibreOfficeSite<'_> {
        LibreOfficeSite {
            config: LibreOfficeConfig {
                helper: "/capsule/bin/uno-helper".into(),
                environment: BTreeMap::new(),
                timeout_ms: 50,
                max_output_bytes: 4096,
            },
            mounts: LibreOfficeMounts {
                helper_root: "/capsule".into(),
                input_root: "/inputs".into(),
                output_root: "/outputs".into(),
            },
            transport: LibreOfficeTransport::Process(port),
        }
    }
    fn allowed() -> Cx {
        let (mut cx, seat) = Cx::new_seated(
            std::sync::Arc::new(sim_kernel::NoopEvalPolicy),
            std::sync::Arc::new(sim_kernel::DefaultFactory),
            sim_kernel::HandleSeed::new(0x4c4f_4f41),
        );
        let _ = seat.grant(&mut cx, CapabilityName::new(PROCESS_SPAWN_CAPABILITY));
        cx
    }
    #[test]
    fn modeled_process_returns_receipt_without_host_mechanics() {
        let port = ModelProcess(ProcessAttempt::Completed {
            receipt: ProcessReceipt {
                provider: "platform/site/model".into(),
                elapsed_mono_ns: 7,
                result: ProcResult {
                    stdout: r#"{"external_id":"doc:1"}"#.into(),
                    stderr: String::new(),
                    exit_code: 0,
                    truncated: false,
                },
            },
        });
        let receipt = run_uno(
            &mut allowed(),
            &site(&port),
            UnoCommand::Open {
                path: "/inputs/doc.ods".into(),
            },
        )
        .unwrap();
        assert_eq!(receipt.provider, "platform/site/model");
        assert_eq!(receipt.external.external_id, "doc:1");
    }
    #[test]
    fn missing_timeout_and_denial_are_typed() {
        let missing = ModelProcess(ProcessAttempt::NotDispatched {
            refusal: ProcessRefusal::SpawnFailed("native path".into()),
        });
        let timeout = ModelProcess(ProcessAttempt::StoppedAfterTimeout {
            receipt: StopReceipt {
                provider: "platform/site/model".into(),
                elapsed_mono_ns: 50,
                cleanup: "process group killed and reaped".into(),
            },
        });
        let mut denied = Cx::new(
            std::sync::Arc::new(sim_kernel::NoopEvalPolicy),
            std::sync::Arc::new(sim_kernel::DefaultFactory),
            sim_kernel::HandleSeed::new(0x4c4f_4f44),
        );
        assert!(matches!(
            run_uno(
                &mut denied,
                &site(&missing),
                UnoCommand::Open {
                    path: "/inputs/a".into()
                }
            ),
            Err(LibreOfficeError::Denied(_))
        ));
        assert_eq!(
            run_uno(
                &mut allowed(),
                &site(&missing),
                UnoCommand::Open {
                    path: "/inputs/a".into()
                }
            )
            .unwrap_err(),
            LibreOfficeError::MissingHelper
        );
        assert_eq!(
            run_uno(
                &mut allowed(),
                &site(&timeout),
                UnoCommand::Open {
                    path: "/inputs/a".into()
                }
            )
            .unwrap_err(),
            LibreOfficeError::Timeout
        );
    }
    #[test]
    fn unavailable_local_ipc_is_typed() {
        let process = ModelProcess(ProcessAttempt::NotDispatched {
            refusal: ProcessRefusal::SpawnFailed("unused".into()),
        });
        let mut site = site(&process);
        site.transport = LibreOfficeTransport::Ipc {
            port: &MissingIpc,
            address: IpcAddress::UnixPath("/capsule/run/uno".into()),
        };
        assert_eq!(
            run_uno(
                &mut allowed(),
                &site,
                UnoCommand::Open {
                    path: "/inputs/a".into()
                }
            )
            .unwrap_err(),
            LibreOfficeError::IpcUnavailable
        );
    }
}
