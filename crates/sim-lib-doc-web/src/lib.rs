//! Durable, network-free web evidence in the ordinary office model.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod anchor;
mod citation;
mod persistence;
mod projection;
mod reading;
mod records;

pub use anchor::*;
pub use citation::*;
pub use persistence::*;
pub use projection::*;
pub use reading::*;

use records::cid_text;

fn err(error: impl std::fmt::Display) -> WebEvidenceError {
    WebEvidenceError::from_display(error)
}

#[cfg(test)]
use sim_codec_doc::MarkupDoc;
#[cfg(test)]
use sim_kernel::Cx;
#[cfg(test)]
use sim_lib_doc_core::DocId;
#[cfg(test)]
use sim_lib_doc_store::DocStore;
#[cfg(test)]
use sim_lib_web_core::{
    DecodeLimits, EvidenceSelector, RepresentationMetadata, WebCapture, WebRepresentation,
};

/// Cookbook recipes embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod reading_tests;
#[cfg(test)]
mod tests;
