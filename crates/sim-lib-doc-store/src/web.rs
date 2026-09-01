//! Typed persistence rows for web captures and evidence anchors.
use crate::{
    DocStore,
    store::{StoreError, StoreResult, bytes, cell_bytes, cell_text, text},
};
/// Serialized immutable capture row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebCaptureRow {
    /// Stable raw capture id.
    pub capture_id: String,
    /// Normalized retrieval URI.
    pub source_uri: String,
    /// Exact response bytes.
    pub body: Vec<u8>,
    /// Versioned exchange metadata.
    pub exchange_json: String,
}
/// Serialized normalized representation row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRepresentationRow {
    /// Stable representation id.
    pub representation_id: String,
    /// Referenced raw capture id.
    pub capture_id: String,
    /// Immutable normalized Unicode text.
    pub text: String,
    /// Versioned codec and fidelity metadata.
    pub metadata_json: String,
}
/// Serialized evidence anchor row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAnchorRow {
    /// Stable anchor id.
    pub anchor_id: String,
    /// Office evidence subject id.
    pub subject: String,
    /// Addressed representation id.
    pub representation_id: String,
    /// Complete versioned anchor record.
    pub record_json: String,
}
impl DocStore {
    /// Saves a capture, rejecting an id already bound to different bytes.
    pub fn save_web_capture(&mut self, row: &WebCaptureRow) -> StoreResult<()> {
        if let Some(old) = self.load_web_capture(&row.capture_id)? {
            if old.body != row.body {
                return Err(StoreError::Invalid(
                    "capture id already names different bytes".into(),
                ));
            }
            return Ok(());
        }
        self.insert(
            "web_captures",
            &["capture_id", "source_uri", "body", "exchange_json"],
            vec![
                text(&row.capture_id),
                text(&row.source_uri),
                bytes(row.body.clone()),
                text(&row.exchange_json),
            ],
        )
    }
    /// Saves a representation whose capture must already exist.
    pub fn save_web_representation(&mut self, row: &WebRepresentationRow) -> StoreResult<()> {
        self.upsert(
            "web_representations",
            &["representation_id", "capture_id", "text", "metadata_json"],
            vec![
                text(&row.representation_id),
                text(&row.capture_id),
                text(&row.text),
                text(&row.metadata_json),
            ],
        )
    }
    /// Loads representation fields used to revalidate an anchor.
    pub fn load_web_representation(&self, id: &str) -> StoreResult<Option<WebRepresentationRow>> {
        self.select(
            "web_representations",
            &["capture_id", "text", "metadata_json"],
            &[("representation_id", text(id))],
            &[],
            Some(1),
        )?
        .into_iter()
        .next()
        .map(|r| {
            Ok(WebRepresentationRow {
                representation_id: id.into(),
                capture_id: cell_text(&r, 0)?.into(),
                text: cell_text(&r, 1)?.into(),
                metadata_json: cell_text(&r, 2)?.into(),
            })
        })
        .transpose()
    }
    /// Loads capture provenance fields.
    pub fn load_web_capture(&self, id: &str) -> StoreResult<Option<WebCaptureRow>> {
        self.select(
            "web_captures",
            &["source_uri", "body", "exchange_json"],
            &[("capture_id", text(id))],
            &[],
            Some(1),
        )?
        .into_iter()
        .next()
        .map(|r| {
            Ok(WebCaptureRow {
                capture_id: id.into(),
                source_uri: cell_text(&r, 0)?.into(),
                body: cell_bytes(&r, 1)?.to_vec(),
                exchange_json: cell_text(&r, 2)?.into(),
            })
        })
        .transpose()
    }
    /// Makes a validated anchor visible.
    pub fn save_web_anchor(&mut self, row: &WebAnchorRow) -> StoreResult<()> {
        self.upsert(
            "web_evidence_anchors",
            &["anchor_id", "subject", "representation_id", "record_json"],
            vec![
                text(&row.anchor_id),
                text(&row.subject),
                text(&row.representation_id),
                text(&row.record_json),
            ],
        )
    }
    /// Loads an anchor row.
    pub fn load_web_anchor(&self, id: &str) -> StoreResult<Option<WebAnchorRow>> {
        self.select(
            "web_evidence_anchors",
            &["subject", "representation_id", "record_json"],
            &[("anchor_id", text(id))],
            &[],
            Some(1),
        )?
        .into_iter()
        .next()
        .map(|r| {
            Ok(WebAnchorRow {
                anchor_id: id.into(),
                subject: cell_text(&r, 0)?.into(),
                representation_id: cell_text(&r, 1)?.into(),
                record_json: cell_text(&r, 2)?.into(),
            })
        })
        .transpose()
    }
}
