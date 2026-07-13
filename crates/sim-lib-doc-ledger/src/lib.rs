//! Ring-3 office bridge for ledger draft previews.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bridge;
mod draft;

pub use bridge::{
    LEDGER_EDIT_DOMAIN, evidence_ref_from_external, preview_post, resolve_post_draft,
};
pub use draft::{DraftBook, DraftId};

/// Cookbook recipes for this bridge crate, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
