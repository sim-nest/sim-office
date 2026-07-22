//! Optional Windows OLE bridge for Powerproject desktop exports.

use std::path::Path;

use sim_kernel::{CapabilityName, Cx};
use sim_lib_doc_core::{OfficeError, PROCESS_SPAWN_CAPABILITY};

/// Environment variable naming the host bridge executable allowed to drive
/// Powerproject OLE automation.
pub const POWERPROJECT_OLE_BRIDGE_ENV: &str = "SIM_OFFICE_POWERPROJECT_OLE_BRIDGE";

/// Reports whether the live OLE export function is compiled into this build.
#[must_use]
pub fn ole_export_compiled() -> bool {
    cfg!(all(windows, feature = "powerproject-ole"))
}

/// Exports the currently open Powerproject project to MSPDI XML through a host bridge.
pub fn export_current_project_to_mspdi(
    cx: &mut Cx,
    out: &Path,
) -> Result<(), crate::PowerprojectError> {
    cx.require(&CapabilityName::new(PROCESS_SPAWN_CAPABILITY))
        .map_err(OfficeError::from)?;
    export_current_project_to_mspdi_inner(cx, out, std::env::var(POWERPROJECT_OLE_BRIDGE_ENV).ok())
}

fn export_current_project_to_mspdi_inner(
    _cx: &mut Cx,
    out: &Path,
    bridge: Option<String>,
) -> Result<(), crate::PowerprojectError> {
    let bridge = bridge.ok_or_else(|| {
        crate::PowerprojectError::OleUnavailable(format!(
            "set {POWERPROJECT_OLE_BRIDGE_ENV} to an approved Powerproject OLE bridge"
        ))
    })?;
    if out.as_os_str().is_empty() {
        return Err(crate::PowerprojectError::OleUnavailable(
            "MSPDI output path is empty".to_owned(),
        ));
    }
    run_bridge(&bridge, out)
}

#[cfg(all(windows, feature = "powerproject-ole"))]
fn run_bridge(bridge: &str, out: &Path) -> Result<(), crate::PowerprojectError> {
    use std::process::Command;

    let status = Command::new(bridge)
        .arg("export-current-project-mspdi")
        .arg(out)
        .status()
        .map_err(|err| {
            crate::PowerprojectError::OleUnavailable(format!(
                "could not run Powerproject OLE bridge: {err}"
            ))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(crate::PowerprojectError::OleUnavailable(format!(
            "Powerproject OLE bridge exited with {status}"
        )))
    }
}

#[cfg(not(all(windows, feature = "powerproject-ole")))]
fn run_bridge(_bridge: &str, _out: &Path) -> Result<(), crate::PowerprojectError> {
    Err(crate::PowerprojectError::OleUnavailable(
        "Powerproject OLE export is not compiled into this build".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, NoopEvalPolicy};

    use super::*;

    trait GrantOutcome {
        fn expect_granted(self);
    }

    impl GrantOutcome for () {
        fn expect_granted(self) {}
    }

    impl GrantOutcome for sim_kernel::Result<()> {
        fn expect_granted(self) {
            self.unwrap();
        }
    }

    macro_rules! expect_granted {
        ($grant:expr) => {{
            #[allow(clippy::let_unit_value)]
            let grant_result = $grant;
            #[allow(clippy::unit_arg)]
            grant_result.expect_granted();
        }};
    }

    #[test]
    fn ole_export_is_not_compiled_by_default() {
        assert!(!ole_export_compiled());
    }

    #[test]
    fn ole_export_is_denied_without_process_spawn_capability() {
        let mut cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));

        let denied =
            export_current_project_to_mspdi(&mut cx, &PathBuf::from("/tmp/powerproject.xml"))
                .unwrap_err();

        assert!(matches!(
            denied,
            crate::PowerprojectError::Office(OfficeError::CapabilityDenied(capability))
                if capability.as_str() == PROCESS_SPAWN_CAPABILITY
        ));
    }

    #[test]
    fn ole_export_is_not_attempted_without_environment_gate() {
        let (mut cx, seat) = Cx::new_seated(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        expect_granted!(seat.grant(&mut cx, CapabilityName::new(PROCESS_SPAWN_CAPABILITY)));

        let unavailable = export_current_project_to_mspdi_inner(
            &mut cx,
            &PathBuf::from("/tmp/powerproject.xml"),
            None,
        )
        .unwrap_err();

        assert!(
            unavailable
                .to_string()
                .contains(POWERPROJECT_OLE_BRIDGE_ENV)
        );
    }
}
