//! Ring-3 office bridge for ledger draft previews.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bridge;
mod draft;
mod projection;

pub use bridge::{
    LEDGER_EDIT_DOMAIN, evidence_ref_from_external, preview_post, resolve_post_draft,
};
pub use draft::{DraftBook, DraftId};
pub use projection::{
    StatementProjection, project_statements, statements_to_deck, statements_to_sheet,
};

/// Cookbook recipes for this bridge crate, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
