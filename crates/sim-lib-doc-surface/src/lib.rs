//! Scene and intent surface for office document panes.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod intent;
pub mod scene;

pub use intent::{
    CELL_EDIT_DOMAIN, CELL_PATH_SEGMENT, OP_RESTORE_CELL, OP_SET_CELL, decode_suite_intent,
};
pub use scene::{SuitePane, suite_scene};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
