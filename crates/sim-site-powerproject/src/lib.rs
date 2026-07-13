//! Powerproject and Project for the web placement for SIM Gantt documents.
//!
//! The crate keeps vendor project tools outside the local Gantt model. A site
//! descriptor carries the live access requirements, modeled OLE receipts import
//! MSPDI through the shared codec, and Dataverse operation plans describe Project
//! Schedule Service updates without sending them.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt;

use sim_kernel::{CapabilityName, Cx, ExportRecord};
use sim_lib_doc_core::{
    CREDENTIALS_CAPABILITY, DocKind, DocSite, NET_CONNECT_CAPABILITY, OfficeError,
    PROCESS_SPAWN_CAPABILITY,
};
use sim_lib_doc_site::register_site;
use sim_lib_gantt::GANTT_DOC_KIND;

pub mod dataverse;
pub mod modeled;
pub mod ole;

pub use dataverse::{DataverseAction, DataverseOperation, DataverseProjectTarget, plan_pss_update};
pub use modeled::{ModeledOleReceipt, import_modeled_ole_receipt};
pub use ole::{POWERPROJECT_OLE_BRIDGE_ENV, ole_export_compiled};

/// Stable office site id for Powerproject and Project for the web placements.
pub const POWERPROJECT_SITE_ID: &str = "site/powerproject";

/// Error reported by the Powerproject placement crate.
#[derive(Clone, Debug)]
pub enum PowerprojectError {
    /// The shared office document layer rejected an operation.
    Office(OfficeError),
    /// The optional OLE bridge is not available in the current host.
    OleUnavailable(String),
    /// The Project for the web target or operation set is invalid.
    Dataverse(String),
}

impl fmt::Display for PowerprojectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Office(error) => write!(f, "{error}"),
            Self::OleUnavailable(message) => write!(f, "Powerproject OLE unavailable: {message}"),
            Self::Dataverse(message) => write!(f, "Dataverse Project mapping failed: {message}"),
        }
    }
}

impl std::error::Error for PowerprojectError {}

impl From<OfficeError> for PowerprojectError {
    fn from(error: OfficeError) -> Self {
        Self::Office(error)
    }
}

/// Builds the Powerproject site descriptor.
#[must_use]
pub fn powerproject_site(default_modeled: bool) -> DocSite {
    DocSite::new(
        POWERPROJECT_SITE_ID,
        vec![DocKind::new(GANTT_DOC_KIND)],
        vec![
            CapabilityName::new(PROCESS_SPAWN_CAPABILITY),
            CapabilityName::new(NET_CONNECT_CAPABILITY),
            CapabilityName::new(CREDENTIALS_CAPABILITY),
        ],
        default_modeled,
    )
}

/// Builds the modeled Powerproject descriptor used by deterministic tests.
#[must_use]
pub fn modeled_powerproject_site() -> DocSite {
    powerproject_site(true)
}

/// Builds the live Powerproject descriptor used by hosts with desktop and cloud access.
#[must_use]
pub fn live_powerproject_site() -> DocSite {
    powerproject_site(false)
}

/// Registers the Powerproject site through the shared office site spine.
pub fn register_powerproject_site(
    cx: &mut Cx,
    default_modeled: bool,
) -> Result<ExportRecord, OfficeError> {
    register_site(cx, &powerproject_site(default_modeled))
}

/// Cookbook recipes shipped with this site crate.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, ExportKind, ExportState, NoopEvalPolicy, RuntimeId};
    use sim_lib_doc_core::{
        CREDENTIALS_CAPABILITY, NET_CONNECT_CAPABILITY, PROCESS_SPAWN_CAPABILITY,
    };
    use sim_lib_doc_site::site_symbol;

    use super::*;

    fn cx() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn live_site_carries_project_capabilities() {
        let site = live_powerproject_site();
        let caps: Vec<_> = site
            .required_caps
            .iter()
            .map(|capability| capability.as_str().to_owned())
            .collect();

        assert_eq!(site.site_id, POWERPROJECT_SITE_ID);
        assert!(!site.default_modeled);
        assert_eq!(
            caps,
            vec![
                PROCESS_SPAWN_CAPABILITY,
                NET_CONNECT_CAPABILITY,
                CREDENTIALS_CAPABILITY
            ]
        );
    }

    #[test]
    fn site_registers_as_export_site() {
        let mut cx = cx();

        let record = register_powerproject_site(&mut cx, true).unwrap();

        assert_eq!(record.kind, ExportKind::named(ExportKind::SITE));
        assert_eq!(record.symbol, site_symbol(POWERPROJECT_SITE_ID));
        assert!(matches!(
            record.state,
            ExportState::Resolved {
                id: RuntimeId::Site(_)
            }
        ));
        assert!(
            cx.registry()
                .site_by_symbol(&site_symbol(POWERPROJECT_SITE_ID))
                .is_some()
        );
    }

    #[test]
    fn recipes_are_embedded() {
        let cards = sim_cookbook::recipes_from_embedded(RECIPES).unwrap();

        assert!(
            cards
                .iter()
                .any(|card| card.id.ends_with("powerproject-placement"))
        );
    }
}
