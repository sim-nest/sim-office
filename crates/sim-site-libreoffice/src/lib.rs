//! LibreOffice helper-process site for office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod helper;
pub mod site;

pub use helper::{LibreOfficeSite, UnoCommand, run_uno};
pub use site::{
    LIBREOFFICE_SITE_ID, libreoffice_site, live_libreoffice_site, modeled_libreoffice_site,
    register_libreoffice_site,
};

/// Embedded cookbook recipe books shipped with this library.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
