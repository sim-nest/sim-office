//! Modeled Powerproject OLE receipts.

use sim_codec_mspdi::MspdiCodec;
use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocCodec, DocCodecOptions, ExternalRef, FidelityReport, OfficeError};

use crate::{POWERPROJECT_SITE_ID, PowerprojectError};

/// Deterministic receipt for an OLE export that produced MSPDI XML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeledOleReceipt {
    /// Stable receipt id in the modeled host.
    pub receipt_id: String,
    /// MSPDI XML bytes produced by the modeled export.
    pub mspdi_xml: Vec<u8>,
}

impl ModeledOleReceipt {
    /// Builds a modeled OLE export receipt.
    #[must_use]
    pub fn new(receipt_id: impl Into<String>, mspdi_xml: impl Into<Vec<u8>>) -> Self {
        Self {
            receipt_id: receipt_id.into(),
            mspdi_xml: mspdi_xml.into(),
        }
    }
}

/// Imports a modeled OLE receipt by decoding its MSPDI payload.
pub fn import_modeled_ole_receipt(
    cx: &mut Cx,
    receipt: &ModeledOleReceipt,
) -> Result<(Doc, FidelityReport), PowerprojectError> {
    let options = DocCodecOptions::new(cx.factory().nil().map_err(OfficeError::from)?);
    let (mut doc, report) = MspdiCodec.decode(cx, &receipt.mspdi_xml, &options)?;
    doc.origin.push(ExternalRef::new(
        POWERPROJECT_SITE_ID,
        receipt.receipt_id.clone(),
        None,
        None,
    ));
    Ok((
        doc,
        report.with_preserved_extra(format!("ole-receipt:{}", receipt.receipt_id)),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_codec_mspdi::doc_to_plan;
    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};

    use super::*;

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn modeled_ole_receipt_imports_mspdi() {
        let mut cx = test_context();
        let receipt = ModeledOleReceipt::new(
            "ole-export-1",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Powerproject Receipt</Name>
  <Tasks>
    <Task>
      <UID>1</UID>
      <ID>1</ID>
      <Name>Design</Name>
      <Start>2026-07-01T08:00:00</Start>
      <Finish>2026-07-03T17:00:00</Finish>
      <PercentComplete>25</PercentComplete>
    </Task>
    <Task>
      <UID>2</UID>
      <ID>2</ID>
      <Name>Build</Name>
      <Start>2026-07-04T08:00:00</Start>
      <Finish>2026-07-08T17:00:00</Finish>
      <PercentComplete>0</PercentComplete>
      <PredecessorLink>
        <PredecessorUID>1</PredecessorUID>
        <Type>1</Type>
        <LinkLag>0</LinkLag>
      </PredecessorLink>
    </Task>
  </Tasks>
</Project>"#
                .to_vec(),
        );

        let (doc, report) = import_modeled_ole_receipt(&mut cx, &receipt).unwrap();
        let plan = doc_to_plan(&mut cx, &doc).unwrap();

        assert_eq!(plan.id, "Powerproject Receipt");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.links.len(), 1);
        assert!(
            doc.origin
                .iter()
                .any(|origin| origin.backend == POWERPROJECT_SITE_ID
                    && origin.external_id == "ole-export-1")
        );
        assert!(
            report
                .preserved_extras
                .iter()
                .any(|extra| extra == "ole-receipt:ole-export-1")
        );
    }
}
