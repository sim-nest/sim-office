//! OOXML spreadsheet codec for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod package;
pub mod xlsx;

pub use xlsx::{XLSX_CODEC_ID, XLSX_EXTENSION, XlsxCodec, xlsx_codec};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
