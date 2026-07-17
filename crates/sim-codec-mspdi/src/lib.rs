//! MSPDI XML codec for local Gantt schedule documents.
//!
//! The codec maps Microsoft Project XML task and predecessor records to the
//! portable `sim-lib-gantt` model. Fields outside that portable model are
//! reported through `FidelityReport` so callers can keep file exchange honest.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod model;
mod read;
mod write;

use std::sync::OnceLock;

use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocCodec, DocCodecOptions, DocKind, FidelityReport, OfficeError};
use sim_lib_gantt::GANTT_DOC_KIND;

pub use model::{doc_to_plan, plan_to_doc};

/// Stable codec id for Microsoft Project XML schedule exchange.
pub const MSPDI_CODEC_ID: &str = "codec/mspdi";

pub(crate) const MSPDI_LAG_FORMAT_DAYS: &str = "7";
pub(crate) const TENTHS_PER_WORKDAY: i32 = 8 * 60 * 10;

/// Local MSPDI XML codec for Gantt documents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MspdiCodec;

/// Builds the local MSPDI codec.
#[must_use]
pub fn mspdi_codec() -> MspdiCodec {
    MspdiCodec
}

impl DocCodec for MspdiCodec {
    fn codec_id(&self) -> &'static str {
        MSPDI_CODEC_ID
    }

    fn kinds(&self) -> &'static [DocKind] {
        static KINDS: OnceLock<Vec<DocKind>> = OnceLock::new();
        KINDS
            .get_or_init(|| vec![DocKind::new(GANTT_DOC_KIND)])
            .as_slice()
    }

    fn decode(
        &self,
        cx: &mut Cx,
        bytes: &[u8],
        _options: &DocCodecOptions,
    ) -> Result<(Doc, FidelityReport), OfficeError> {
        read::decode(cx, bytes)
    }

    fn encode(
        &self,
        cx: &mut Cx,
        doc: &Doc,
        _options: &DocCodecOptions,
    ) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
        write::encode(cx, doc)
    }
}

pub(crate) fn error(message: impl Into<String>) -> OfficeError {
    OfficeError::Kernel(message.into())
}

/// Embedded cookbook recipe books shipped with this library.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;
