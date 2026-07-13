//! Markup document codecs for office article documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod codec;

#[cfg(test)]
mod tests;

pub use codec::{
    MARKUP_DOC_KIND, MarkupDocCodec, markdown_doc_codec, office_fidelity, typst_doc_codec,
};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
