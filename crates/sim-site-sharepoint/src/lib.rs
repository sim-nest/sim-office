//! SharePoint placement through Microsoft Graph.
//!
//! Microsoft Graph v1 provides the site-local API used here for SharePoint
//! sites, lists, list items, drives, and drive items. Graph v1 is not the
//! site-provisioning backend; this crate keeps provisioning outside the
//! SharePoint document placement and focuses on read/write-precondition data.
//! SharePoint REST `_api` calls are available as explicit fallback operations
//! for gaps in Graph coverage; they are not a second site frontend.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt;

use sim_kernel::{CapabilityName, Cx, ExportRecord};
use sim_lib_doc_core::{
    CREDENTIALS_CAPABILITY, DocKind, DocSite, NET_CONNECT_CAPABILITY, OfficeError,
};
use sim_lib_doc_site::register_site;
use sim_lib_sheet::SHEET_DOC_KIND;

pub mod batch;
pub mod graph;
pub mod model;
pub mod permission;
pub mod rest;

pub use batch::{BATCH_API_PATH, RestBatchMethod, RestBatchOp, odata_batch_body};
pub use graph::{MsGraphSite, drive_children, list_items};
pub use model::{SharePointDriveTarget, SharePointListTarget};
pub use rest::{SharePointRestSite, SharePointRestTokenSite};

/// Stable office site id for SharePoint Graph placements.
pub const SHAREPOINT_SITE_ID: &str = "site/sharepoint";

/// Error reported by the SharePoint Graph placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharePointError {
    /// A Microsoft Graph request failed.
    Graph(String),
    /// A SharePoint REST fallback request failed.
    Rest(String),
    /// A Graph response did not have the expected shape.
    WrongShape(String),
    /// A required field was absent or not a string.
    MissingField {
        /// Graph path being decoded.
        path: String,
        /// Missing field name.
        field: String,
    },
    /// A drive or list item lacked an ETag needed for safe writes.
    WritePrecondition {
        /// Graph path being decoded.
        path: String,
        /// Item id whose ETag was missing.
        item_id: String,
    },
    /// The sheet projection failed.
    Sheet(String),
    /// The shared office site layer rejected an operation.
    Office(String),
}

impl fmt::Display for SharePointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graph(message) => write!(f, "SharePoint Graph request failed: {message}"),
            Self::Rest(message) => write!(f, "SharePoint REST fallback failed: {message}"),
            Self::WrongShape(message) => write!(f, "SharePoint Graph shape failed: {message}"),
            Self::MissingField { path, field } => {
                write!(f, "SharePoint Graph response at {path} missing {field}")
            }
            Self::WritePrecondition { path, item_id } => write!(
                f,
                "SharePoint Graph response at {path} item {item_id} is missing an ETag"
            ),
            Self::Sheet(message) => write!(f, "SharePoint sheet projection failed: {message}"),
            Self::Office(message) => write!(f, "SharePoint site registration failed: {message}"),
        }
    }
}

impl std::error::Error for SharePointError {}

impl From<sim_site_msgraph::GraphError> for SharePointError {
    fn from(error: sim_site_msgraph::GraphError) -> Self {
        Self::Graph(error.to_string())
    }
}

impl From<sim_lib_sheet::SheetError> for SharePointError {
    fn from(error: sim_lib_sheet::SheetError) -> Self {
        Self::Sheet(error.to_string())
    }
}

impl From<OfficeError> for SharePointError {
    fn from(error: OfficeError) -> Self {
        Self::Office(error.to_string())
    }
}

/// Builds the SharePoint Graph site descriptor.
#[must_use]
pub fn sharepoint_site(default_modeled: bool) -> DocSite {
    DocSite::new(
        SHAREPOINT_SITE_ID,
        vec![DocKind::new(SHEET_DOC_KIND)],
        vec![
            CapabilityName::new(NET_CONNECT_CAPABILITY),
            CapabilityName::new(CREDENTIALS_CAPABILITY),
        ],
        default_modeled,
    )
}

/// Builds the modeled SharePoint site descriptor used by deterministic tests.
#[must_use]
pub fn modeled_sharepoint_site() -> DocSite {
    sharepoint_site(true)
}

/// Builds the live SharePoint site descriptor used by hosts with credentials.
#[must_use]
pub fn live_sharepoint_site() -> DocSite {
    sharepoint_site(false)
}

/// Registers the SharePoint site through the shared office site spine.
pub fn register_sharepoint_site(
    cx: &mut Cx,
    default_modeled: bool,
) -> Result<ExportRecord, OfficeError> {
    register_site(cx, &sharepoint_site(default_modeled))
}

/// Cookbook recipes shipped with this site crate.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;
