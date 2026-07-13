//! Presentation deck domain for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod doc;
pub mod graph_file;
pub mod model;

pub use doc::{deck_to_doc, doc_to_deck};
pub use graph_file::{
    GRAPH_DECK_FILE_EDIT_DOMAIN, MsGraphSite, deck_file_content_path, download_deck_file,
    plan_upload_deck_file, validate_pptx_package,
};
pub use model::{DECK_DOC_KIND, Deck, DeckError, Slide, SlideBlock};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
