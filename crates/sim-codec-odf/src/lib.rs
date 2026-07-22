//! ODF package codecs for sheet and deck office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod odp;
mod ods;
mod package;

use std::sync::OnceLock;

use sim_kernel::Cx;
use sim_lib_deck::DECK_DOC_KIND;
use sim_lib_doc_core::{Doc, DocCodec, DocCodecOptions, DocKind, FidelityReport, OfficeError};
use sim_lib_sheet::SHEET_DOC_KIND;

use crate::package::{ODP_MIMETYPE, ODS_MIMETYPE, OdfPackage};

/// Stable codec id for local ODF spreadsheet and presentation packages.
pub const ODF_CODEC_ID: &str = "codec/odf";
/// File extension used for ODF spreadsheets.
pub const ODS_EXTENSION: &str = ".ods";
/// File extension used for ODF presentations.
pub const ODP_EXTENSION: &str = ".odp";

/// Local ODF codec for sheet and deck documents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OdfCodec;

/// Builds the local ODF codec.
#[must_use]
pub fn odf_codec() -> OdfCodec {
    OdfCodec
}

impl DocCodec for OdfCodec {
    fn codec_id(&self) -> &'static str {
        ODF_CODEC_ID
    }

    fn kinds(&self) -> &'static [DocKind] {
        static KINDS: OnceLock<Vec<DocKind>> = OnceLock::new();
        KINDS
            .get_or_init(|| vec![DocKind::new(SHEET_DOC_KIND), DocKind::new(DECK_DOC_KIND)])
            .as_slice()
    }

    fn decode(
        &self,
        cx: &mut Cx,
        bytes: &[u8],
        _options: &DocCodecOptions,
    ) -> Result<(Doc, FidelityReport), OfficeError> {
        let package = OdfPackage::read(bytes)?;
        match package.mimetype() {
            ODS_MIMETYPE => ods::decode(cx, &package),
            ODP_MIMETYPE => odp::decode(cx, &package),
            other => Err(package::error(format!("unsupported ODF mimetype {other}"))),
        }
    }

    fn encode(
        &self,
        cx: &mut Cx,
        doc: &Doc,
        _options: &DocCodecOptions,
    ) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
        match doc.kind.as_str() {
            SHEET_DOC_KIND => ods::encode(cx, doc),
            DECK_DOC_KIND => odp::encode(cx, doc),
            other => Err(package::error(format!(
                "ODF codec does not encode document kind {other}"
            ))),
        }
    }
}

/// Embedded cookbook recipe books shipped with this library.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod package_tests;
#[cfg(test)]
mod tests;
