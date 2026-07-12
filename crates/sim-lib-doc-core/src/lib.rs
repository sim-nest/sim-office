//! Office suite document core.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod caps;
pub mod edit;
pub mod error;
pub mod fidelity;
pub mod model;
pub mod place;
pub mod shape;

pub use caps::{
    CREDENTIALS_CAPABILITY, NET_CONNECT_CAPABILITY, OfficeCapabilityProfile,
    PROCESS_SPAWN_CAPABILITY, WALL_CLOCK_CAPABILITY,
};
pub use edit::{DomainEdit, Edit, invert};
pub use error::OfficeError;
pub use fidelity::{FidelityReport, LossNote};
pub use model::{
    DOC_KIND_ARTICLE, DOC_KIND_README, DOC_KIND_REPORT, Doc, DocId, DocKind, ExternalRef,
};
pub use place::{DocCodec, DocCodecOptions, DocSite, Placement};
pub use shape::{DocKindShape, doc_shape};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
