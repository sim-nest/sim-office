use crate::{
    AnchorKind, EvidenceAnchor, WebEvidenceError,
    anchor::validate_context,
    records::{AnchorRecord, MetadataRecord, cid_text},
};
use sim_kernel::Datum;
use sim_lib_doc_store::{
    DocStore,
    web::{WebAnchorRow, WebCaptureRow, WebRepresentationRow},
};
use sim_lib_web_core::{DecodeLimits, WebCapture, WebRepresentation};

/// Saves exact capture bytes.
pub fn save_capture(store: &mut DocStore, capture: &WebCapture) -> Result<(), WebEvidenceError> {
    let exchange = serde_json::json!({
        "method": capture.exchange.method,
        "status": capture.exchange.status,
        "final_uri": capture.exchange.final_uri,
        "media_type": capture.exchange.media_type,
        "received_bytes": capture.exchange.received_bytes,
    });
    store.save_web_capture(&WebCaptureRow {
        capture_id: cid_text(&capture.content_id),
        source_uri: capture.retrieval_uri.as_str().into(),
        body: capture.body.clone(),
        exchange_json: exchange.to_string(),
    })?;
    Ok(())
}

/// Saves normalized representation text and codec provenance.
pub fn save_representation(
    store: &mut DocStore,
    representation: &WebRepresentation,
) -> Result<(), WebEvidenceError> {
    let metadata = MetadataRecord::from(representation);
    store.save_web_representation(&WebRepresentationRow {
        representation_id: cid_text(&representation.content_id),
        capture_id: cid_text(&representation.raw_source_id),
        text: representation.text.clone(),
        metadata_json: serde_json::to_string(&metadata).map_err(WebEvidenceError::from_display)?,
    })?;
    Ok(())
}

/// Persists an anchor only after recomputing representation identity and selector validity.
pub fn save_anchor(store: &mut DocStore, anchor: &EvidenceAnchor) -> Result<(), WebEvidenceError> {
    let representation = checked_representation(store, &cid_text(&anchor.representation_id))?;
    verify_anchor(anchor, &representation)?;
    store.save_web_anchor(&WebAnchorRow {
        anchor_id: anchor.anchor_id.clone(),
        subject: anchor.evidence.subject.as_str().into(),
        representation_id: cid_text(&anchor.representation_id),
        record_json: serde_json::to_string(&AnchorRecord::from(anchor))
            .map_err(WebEvidenceError::from_display)?,
    })?;
    Ok(())
}

/// Loads and revalidates an anchor before returning it to callers.
pub fn load_anchor(store: &DocStore, id: &str) -> Result<Option<EvidenceAnchor>, WebEvidenceError> {
    let Some(row) = store.load_web_anchor(id)? else {
        return Ok(None);
    };
    let representation = checked_representation(store, &row.representation_id)?;
    let record: AnchorRecord =
        serde_json::from_str(&row.record_json).map_err(WebEvidenceError::from_display)?;
    let anchor = record.into_anchor(&representation)?;
    if anchor.anchor_id != row.anchor_id
        || anchor.evidence.subject.as_str() != row.subject
        || cid_text(&anchor.representation_id) != row.representation_id
    {
        return Err(WebEvidenceError::message(
            "anchor index does not match record",
        ));
    }
    verify_anchor(&anchor, &representation)?;
    Ok(Some(anchor))
}

fn checked_representation(
    store: &DocStore,
    id: &str,
) -> Result<WebRepresentation, WebEvidenceError> {
    let row = store
        .load_web_representation(id)?
        .ok_or_else(|| WebEvidenceError::message("missing representation"))?;
    let metadata: MetadataRecord =
        serde_json::from_str(&row.metadata_json).map_err(WebEvidenceError::from_display)?;
    let raw = metadata.raw_id.to_cid()?;
    if cid_text(&raw) != row.capture_id {
        return Err(WebEvidenceError::message(
            "representation capture relation mismatch",
        ));
    }
    let capture = store
        .load_web_capture(&row.capture_id)?
        .ok_or_else(|| WebEvidenceError::message("missing raw capture"))?;
    let observed_raw = Datum::Bytes(capture.body)
        .content_id()
        .map_err(WebEvidenceError::from_display)?;
    if observed_raw != raw {
        return Err(WebEvidenceError::message(
            "raw capture content identity mismatch",
        ));
    }
    let representation = WebRepresentation::checked(
        raw,
        row.text,
        metadata.into_metadata(),
        DecodeLimits::default(),
    )
    .map_err(WebEvidenceError::from_display)?;
    if cid_text(&representation.content_id) != row.representation_id || row.representation_id != id
    {
        return Err(WebEvidenceError::message(
            "representation content identity mismatch",
        ));
    }
    Ok(representation)
}

fn verify_anchor(
    anchor: &EvidenceAnchor,
    representation: &WebRepresentation,
) -> Result<(), WebEvidenceError> {
    if anchor.representation_id != representation.content_id
        || anchor.capture_id != representation.raw_source_id
        || anchor.codec != representation.codec
        || anchor.codec_version != representation.codec_version
        || anchor.fidelity_warnings != representation.fidelity_warnings
    {
        return Err(WebEvidenceError::message(
            "anchor provenance does not match representation",
        ));
    }
    match (&anchor.kind, &anchor.selector) {
        (AnchorKind::WholeDocument, None) => Ok(()),
        (_, Some(selector)) => {
            selector
                .verify(representation)
                .map_err(WebEvidenceError::from_display)?;
            validate_context(selector, &representation.text)
        }
        _ => Err(WebEvidenceError::message("anchor kind requires selector")),
    }
}
