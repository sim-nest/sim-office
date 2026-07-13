//! Dalux API-identity site adapter for SIM office documents.
//!
//! The crate reads Dalux project items into local office documents and keeps
//! live calls behind API-identity bearer tokens plus explicit host capability
//! gates. The only write helper is a narrow item-note patch.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt;

use sim_kernel::{CapabilityName, Cx, ExportRecord};
use sim_lib_doc_core::{
    CREDENTIALS_CAPABILITY, DocKind, DocSite, NET_CONNECT_CAPABILITY, OfficeError,
};
use sim_lib_doc_site::register_site;
use sim_lib_sheet::SHEET_DOC_KIND;

pub mod client;
pub mod model;
pub mod modeled;
#[cfg(test)]
mod tests;

pub use client::{
    DALUX_LIVE_ENV, DaluxClient, DaluxClientMode, DaluxCredentialProvider,
    StaticDaluxCredentialProvider, get_project_items, patch_item_note, redacted_body,
};
pub use model::{DaluxItem, item_path, items_doc, patch_external_ref, project_items_path};
pub use modeled::{ModeledDalux, ModeledPatch, ModeledResponse};

/// Stable office site id for Dalux project-item placement.
pub const DALUX_SITE_ID: &str = "site/dalux";

/// Error reported by the Dalux site adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaluxError {
    /// Credential acquisition failed.
    Credentials(String),
    /// Company API keys are not accepted by this adapter.
    CompanyApiKeyUnsupported,
    /// Live HTTP access was denied or failed.
    Http(String),
    /// A project, item, or path identifier was invalid.
    InvalidTarget(String),
    /// The Dalux response did not match the expected shape.
    WrongShape(String),
    /// The sheet projection failed.
    Sheet(String),
    /// The shared office site layer rejected an operation.
    Office(String),
}

impl fmt::Display for DaluxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credentials(message) => write!(f, "Dalux credentials failed: {message}"),
            Self::CompanyApiKeyUnsupported => {
                f.write_str("Dalux company API keys are not accepted; use an API identity token")
            }
            Self::Http(message) => write!(f, "Dalux API failed: {message}"),
            Self::InvalidTarget(message) => write!(f, "Dalux target failed: {message}"),
            Self::WrongShape(message) => write!(f, "Dalux response shape failed: {message}"),
            Self::Sheet(message) => write!(f, "Dalux sheet projection failed: {message}"),
            Self::Office(message) => write!(f, "Dalux site registration failed: {message}"),
        }
    }
}

impl std::error::Error for DaluxError {}

impl From<sim_lib_sheet::SheetError> for DaluxError {
    fn from(error: sim_lib_sheet::SheetError) -> Self {
        Self::Sheet(error.to_string())
    }
}

impl From<OfficeError> for DaluxError {
    fn from(error: OfficeError) -> Self {
        Self::Office(error.to_string())
    }
}

/// Builds the Dalux site descriptor.
#[must_use]
pub fn dalux_site(default_modeled: bool) -> DocSite {
    DocSite::new(
        DALUX_SITE_ID,
        vec![DocKind::new(SHEET_DOC_KIND)],
        vec![
            CapabilityName::new(NET_CONNECT_CAPABILITY),
            CapabilityName::new(CREDENTIALS_CAPABILITY),
        ],
        default_modeled,
    )
}

/// Builds the modeled Dalux site descriptor used by deterministic tests.
#[must_use]
pub fn modeled_dalux_site() -> DocSite {
    dalux_site(true)
}

/// Builds the live Dalux site descriptor used by hosts with API identities.
#[must_use]
pub fn live_dalux_site() -> DocSite {
    dalux_site(false)
}

/// Registers the Dalux site through the shared office site spine.
pub fn register_dalux_site(
    cx: &mut Cx,
    default_modeled: bool,
) -> Result<ExportRecord, OfficeError> {
    register_site(cx, &dalux_site(default_modeled))
}

/// Cookbook recipes shipped with this site crate.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
