use crate::records::cid_text;
use serde::{Deserialize, Serialize};
use sim_kernel::ContentId;
use sim_lib_doc_core::{DocId, Evidence, ExternalRef, LinkRole};
use sim_lib_web_core::{EvidenceSelector, WebCapture, WebRepresentation};
use std::{error::Error, fmt};

/// The supported relationship between a citation and its selected text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorKind {
    /// An exact quotation.
    Quote,
    /// Evidence supporting a paraphrase.
    ParaphraseSupport,
    /// A reference to the complete source document.
    WholeDocument,
}

/// A checked office evidence link bound to one captured representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceAnchor {
    /// Existing office evidence identity and relationship.
    pub evidence: Evidence,
    /// Stable anchor id.
    pub anchor_id: String,
    /// Raw capture content id.
    pub capture_id: ContentId,
    /// Normalized representation content id.
    pub representation_id: ContentId,
    /// Checked Unicode selector; absent only for a whole-document reference.
    pub selector: Option<EvidenceSelector>,
    /// Canonical source URI.
    pub source_uri: String,
    /// Source title.
    pub source_title: String,
    /// RFC 3339 retrieval time supplied by the capture boundary.
    pub retrieved_at: String,
    /// Representation codec.
    pub codec: String,
    /// Representation codec version.
    pub codec_version: String,
    /// Fidelity warnings retained from decoding.
    pub fidelity_warnings: Vec<String>,
    /// Stable policy receipt id.
    pub policy_receipt_id: String,
    /// Optional provider claim, visibly non-authoritative.
    pub provider_claim: Option<String>,
    /// Citation relationship.
    pub kind: AnchorKind,
}

/// Fail-closed validation or persistence error.
#[derive(Debug)]
pub struct WebEvidenceError(pub(crate) String);

impl WebEvidenceError {
    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub(crate) fn from_display(error: impl fmt::Display) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for WebEvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for WebEvidenceError {}

impl From<sim_lib_doc_store::StoreError> for WebEvidenceError {
    fn from(error: sim_lib_doc_store::StoreError) -> Self {
        Self(error.to_string())
    }
}

impl EvidenceAnchor {
    /// Constructs an exact quotation after validating the selector and its context.
    pub fn quote(
        input: AnchorInput<'_>,
        selector: EvidenceSelector,
    ) -> Result<Self, WebEvidenceError> {
        selector
            .verify(input.representation)
            .map_err(WebEvidenceError::from_display)?;
        validate_context(&selector, &input.representation.text)?;
        Self::build(input, Some(selector), AnchorKind::Quote)
    }

    /// Constructs evidence supporting a paraphrase from checked selected source text.
    pub fn paraphrase_support(
        input: AnchorInput<'_>,
        selector: EvidenceSelector,
    ) -> Result<Self, WebEvidenceError> {
        selector
            .verify(input.representation)
            .map_err(WebEvidenceError::from_display)?;
        Self::build(input, Some(selector), AnchorKind::ParaphraseSupport)
    }

    /// Constructs a reference to the complete immutable representation.
    pub fn whole_document(input: AnchorInput<'_>) -> Result<Self, WebEvidenceError> {
        Self::build(input, None, AnchorKind::WholeDocument)
    }

    fn build(
        input: AnchorInput<'_>,
        selector: Option<EvidenceSelector>,
        kind: AnchorKind,
    ) -> Result<Self, WebEvidenceError> {
        if input.capture.content_id != input.representation.raw_source_id {
            return Err(WebEvidenceError::message(
                "representation does not name supplied capture",
            ));
        }
        if input.source_uri != input.capture.retrieval_uri.as_str() {
            return Err(WebEvidenceError::message(
                "source URI differs from capture URI",
            ));
        }
        let external = ExternalRef::new(
            "web/capture",
            cid_text(&input.capture.content_id),
            Some(cid_text(&input.representation.content_id)),
            Some(input.source_uri.into()),
        );
        let evidence = Evidence::new(
            input.subject.clone(),
            external,
            LinkRole::SourceDocument,
            input.captured_at_seq,
            Some(cid_text(&input.representation.content_id)),
        );
        Ok(Self {
            evidence,
            anchor_id: input.anchor_id.into(),
            capture_id: input.capture.content_id.clone(),
            representation_id: input.representation.content_id.clone(),
            selector,
            source_uri: input.source_uri.into(),
            source_title: input.source_title.into(),
            retrieved_at: input.retrieved_at.into(),
            codec: input.representation.codec.clone(),
            codec_version: input.representation.codec_version.clone(),
            fidelity_warnings: input.representation.fidelity_warnings.clone(),
            policy_receipt_id: input.policy_receipt_id.into(),
            provider_claim: input.provider_claim.map(str::to_owned),
            kind,
        })
    }
}

/// Inputs shared by all checked anchor constructors. No provider snippet field exists.
pub struct AnchorInput<'a> {
    /// Anchor id.
    pub anchor_id: &'a str,
    /// Office subject.
    pub subject: &'a DocId,
    /// Checked raw capture.
    pub capture: &'a WebCapture,
    /// Checked representation.
    pub representation: &'a WebRepresentation,
    /// Source URI.
    pub source_uri: &'a str,
    /// Source title.
    pub source_title: &'a str,
    /// Retrieval timestamp.
    pub retrieved_at: &'a str,
    /// Policy receipt id.
    pub policy_receipt_id: &'a str,
    /// Optional provider claim (not quote input).
    pub provider_claim: Option<&'a str>,
    /// Evidence ledger sequence.
    pub captured_at_seq: u64,
}

pub(crate) fn validate_context(
    selector: &EvidenceSelector,
    text: &str,
) -> Result<(), WebEvidenceError> {
    let chars: Vec<char> = text.chars().collect();
    let start = selector.start as usize;
    let end = selector.end as usize;
    if let Some(prefix) = &selector.prefix {
        let observed: String = chars[..start]
            .iter()
            .rev()
            .take(prefix.chars().count())
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if &observed != prefix {
            return Err(WebEvidenceError::message("selector prefix mismatch"));
        }
    }
    if let Some(suffix) = &selector.suffix {
        let observed: String = chars[end..].iter().take(suffix.chars().count()).collect();
        if &observed != suffix {
            return Err(WebEvidenceError::message("selector suffix mismatch"));
        }
    }
    Ok(())
}
