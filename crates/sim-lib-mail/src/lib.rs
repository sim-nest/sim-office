//! Mail and calendar domain for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod doc;
pub mod graph;
pub mod model;
pub mod redact;

#[cfg(test)]
mod graph_tests;

pub use doc::{calendar_event_to_doc, doc_to_calendar_event, doc_to_mail, mail_to_doc};
pub use graph::{
    CalendarTarget, DraftMessage, MailboxTarget, MsGraphSite, OutlookSelectedItem, SuiteSelection,
    create_draft, read_calendar_events, read_messages, selected_item_to_selection,
};
pub use model::{CALENDAR_EVENT_DOC_KIND, CalendarEvent, MAIL_DOC_KIND, Mail, MailError};
pub use redact::{BODY_ERROR_PREVIEW_CHARS, redact_body_for_error};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));
