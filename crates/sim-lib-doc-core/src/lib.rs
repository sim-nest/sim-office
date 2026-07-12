//! Office suite document core.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod model;
pub mod shape;

pub use model::{
    DOC_KIND_ARTICLE, DOC_KIND_README, DOC_KIND_REPORT, Doc, DocId, DocKind, ExternalRef,
};
pub use shape::{DocKindShape, OfficeError, doc_shape};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
