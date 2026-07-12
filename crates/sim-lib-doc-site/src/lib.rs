//! Document site registration and modeled realize spine.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod register;

pub use register::{
    DOC_SITE_DOMAIN, DocSiteRuntime, SiteOp, SiteReply, realize_site_op, register_site, site_symbol,
};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
