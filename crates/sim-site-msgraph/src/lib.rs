//! Microsoft Graph site adapter for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod auth;
pub mod client;
pub mod modeled;
pub mod site;

pub use auth::{StaticTokenProvider, TokenProvider};
pub use client::{
    GRAPH_DEFAULT_SCOPE, GRAPH_LIVE_ENV, GraphError, GraphMode, graph_get, graph_get_bytes,
};
pub use modeled::{Cassette, CassetteBytesResponse, CassetteResponse};
pub use site::{
    MSGRAPH_SITE_ID, live_msgraph_site, modeled_msgraph_site, msgraph_site, register_msgraph_site,
};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
