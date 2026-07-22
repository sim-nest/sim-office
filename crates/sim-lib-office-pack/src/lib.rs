//! Annual accounts pack planner for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod archive;
mod export;
mod pack;

#[cfg(test)]
mod tests;

pub use archive::{PACK_EDIT_DOMAIN, plan_archive, plan_archive_with_cx};
pub use export::{EncodedStatementFiles, GeneratedFile, encode_statement_files};
pub use pack::{AnnualAccountsPack, ExportTargets, OutlookDraftTarget, PackError};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
