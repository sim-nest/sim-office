//! Local SQLite projections for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod codec;
pub mod evidence;
pub mod store;

#[cfg(test)]
mod store_tests;

pub use store::DocStore;

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
