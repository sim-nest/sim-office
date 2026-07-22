//! Markup document codecs for office article documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod codec;
mod store;
mod surface;

#[cfg(test)]
mod tests;

pub use codec::{
    MARKUP_DOC_KIND, MarkupDocCodec, markdown_doc_codec, office_fidelity, typst_doc_codec,
};
pub use store::{
    MARKUP_BACKEND_META_PREFIX, load_article_doc, preferred_backend, save_article_doc,
    with_preferred_backend,
};
pub use surface::{
    MARKUP_EDIT_DOMAIN, apply_markup_edit, decode_markup_suite_intent, markup_suite_scene,
};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
