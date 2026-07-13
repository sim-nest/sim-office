//! Optional Windows OLE bridge for Powerproject desktop exports.

/// Environment variable naming the host bridge executable allowed to drive
/// Powerproject OLE automation.
pub const POWERPROJECT_OLE_BRIDGE_ENV: &str = "SIM_OFFICE_POWERPROJECT_OLE_BRIDGE";

/// Reports whether the live OLE export function is compiled into this build.
#[must_use]
pub fn ole_export_compiled() -> bool {
    cfg!(all(windows, feature = "powerproject-ole"))
}

/// Exports the currently open Powerproject project to MSPDI XML through a host bridge.
#[cfg(all(windows, feature = "powerproject-ole"))]
pub fn export_current_project_to_mspdi(
    out: &std::path::Path,
) -> Result<(), crate::PowerprojectError> {
    use std::process::Command;

    let bridge = std::env::var(POWERPROJECT_OLE_BRIDGE_ENV).map_err(|_| {
        crate::PowerprojectError::OleUnavailable(format!(
            "set {POWERPROJECT_OLE_BRIDGE_ENV} to an approved Powerproject OLE bridge"
        ))
    })?;
    if out.as_os_str().is_empty() {
        return Err(crate::PowerprojectError::OleUnavailable(
            "MSPDI output path is empty".to_owned(),
        ));
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ole_export_is_not_compiled_by_default() {
        assert!(!ole_export_compiled());
    }
}
