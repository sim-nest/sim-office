use crate::{AnchorKind, EvidenceAnchor, WebEvidenceError};
use serde::{Deserialize, Serialize};
use sim_kernel::{ContentId, Symbol};
use sim_lib_doc_core::{DocId, Evidence, ExternalRef, LinkRole};
use sim_lib_web_core::{EvidenceSelector, RepresentationMetadata, WebRepresentation};

#[derive(Serialize, Deserialize)]
pub(crate) struct CidRecord {
    namespace: Option<String>,
    name: String,
    bytes: Vec<u8>,
}

impl CidRecord {
    fn from_cid(content_id: &ContentId) -> Self {
        Self {
            namespace: content_id
                .algorithm
                .namespace
                .as_ref()
                .map(ToString::to_string),
            name: content_id.algorithm.name.to_string(),
            bytes: content_id.bytes.to_vec(),
        }
    }

    pub(crate) fn to_cid(&self) -> Result<ContentId, WebEvidenceError> {
        let bytes: [u8; 32] = self
            .bytes
            .clone()
            .try_into()
            .map_err(|_| WebEvidenceError::message("invalid content digest"))?;
        let algorithm = self.namespace.as_ref().map_or_else(
            || Symbol::new(self.name.clone()),
            |namespace| Symbol::qualified(namespace.as_str(), self.name.as_str()),
        );
        Ok(ContentId::from_bytes(algorithm, bytes))
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MetadataRecord {
    pub(crate) raw_id: CidRecord,
    codec: String,
    codec_version: String,
    media_type: String,
    charset: Option<String>,
    language: Option<String>,
    fidelity_warnings: Vec<String>,
}

impl From<&WebRepresentation> for MetadataRecord {
    fn from(representation: &WebRepresentation) -> Self {
        Self {
            raw_id: CidRecord::from_cid(&representation.raw_source_id),
            codec: representation.codec.clone(),
            codec_version: representation.codec_version.clone(),
            media_type: representation.media_type.clone(),
            charset: representation.charset.clone(),
            language: representation.language.clone(),
            fidelity_warnings: representation.fidelity_warnings.clone(),
        }
    }
}

impl MetadataRecord {
    pub(crate) fn into_metadata(self) -> RepresentationMetadata {
        RepresentationMetadata {
            codec: self.codec,
            codec_version: self.codec_version,
            media_type: self.media_type,
            charset: self.charset,
            language: self.language,
            fidelity_warnings: self.fidelity_warnings,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SelectorRecord {
    start: u32,
    end: u32,
    exact: String,
    prefix: Option<String>,
    suffix: Option<String>,
    path: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct AnchorRecord {
    anchor_id: String,
    subject: String,
    capture: CidRecord,
    representation: CidRecord,
    selector: Option<SelectorRecord>,
    source_uri: String,
    source_title: String,
    retrieved_at: String,
    policy_receipt_id: String,
    provider_claim: Option<String>,
    kind: AnchorKind,
    captured_at_seq: u64,
}

impl From<&EvidenceAnchor> for AnchorRecord {
    fn from(anchor: &EvidenceAnchor) -> Self {
        Self {
            anchor_id: anchor.anchor_id.clone(),
            subject: anchor.evidence.subject.as_str().into(),
            capture: CidRecord::from_cid(&anchor.capture_id),
            representation: CidRecord::from_cid(&anchor.representation_id),
            selector: anchor.selector.as_ref().map(|selector| SelectorRecord {
                start: selector.start,
                end: selector.end,
                exact: selector.exact.clone(),
                prefix: selector.prefix.clone(),
                suffix: selector.suffix.clone(),
                path: selector.structural_path.clone(),
            }),
            source_uri: anchor.source_uri.clone(),
            source_title: anchor.source_title.clone(),
            retrieved_at: anchor.retrieved_at.clone(),
            policy_receipt_id: anchor.policy_receipt_id.clone(),
            provider_claim: anchor.provider_claim.clone(),
            kind: anchor.kind,
            captured_at_seq: anchor.evidence.captured_at_seq,
        }
    }
}

impl AnchorRecord {
    pub(crate) fn into_anchor(
        self,
        representation: &WebRepresentation,
    ) -> Result<EvidenceAnchor, WebEvidenceError> {
        let representation_id = self.representation.to_cid()?;
        let selector = self
            .selector
            .map(|selector| {
                EvidenceSelector::checked(
                    representation_id.clone(),
                    selector.start,
                    selector.end,
                    selector.exact,
                    &representation.text,
                )
                .map(|checked| {
                    checked.with_context(selector.prefix, selector.suffix, selector.path)
                })
            })
            .transpose()
            .map_err(WebEvidenceError::from_display)?;
        let capture_id = self.capture.to_cid()?;
        let evidence = Evidence::new(
            DocId::new(self.subject),
            ExternalRef::new(
                "web/capture",
                cid_text(&capture_id),
                Some(cid_text(&representation_id)),
                Some(self.source_uri.clone()),
            ),
            LinkRole::SourceDocument,
            self.captured_at_seq,
            Some(cid_text(&representation_id)),
        );
        Ok(EvidenceAnchor {
            evidence,
            anchor_id: self.anchor_id,
            capture_id,
            representation_id,
            selector,
            source_uri: self.source_uri,
            source_title: self.source_title,
            retrieved_at: self.retrieved_at,
            codec: representation.codec.clone(),
            codec_version: representation.codec_version.clone(),
            fidelity_warnings: representation.fidelity_warnings.clone(),
            policy_receipt_id: self.policy_receipt_id,
            provider_claim: self.provider_claim,
            kind: self.kind,
        })
    }
}

#[derive(Serialize)]
pub(crate) struct CitationRecord<'a> {
    title: &'a str,
    uri: &'a str,
    retrieved_at: &'a str,
    capture_id: String,
    representation_id: String,
    policy_receipt_id: &'a str,
    quote: Option<&'a str>,
    fidelity_warnings: &'a [String],
    provider_claim: Option<&'a str>,
}

impl<'a> From<&'a EvidenceAnchor> for CitationRecord<'a> {
    fn from(anchor: &'a EvidenceAnchor) -> Self {
        Self {
            title: &anchor.source_title,
            uri: &anchor.source_uri,
            retrieved_at: &anchor.retrieved_at,
            capture_id: cid_text(&anchor.capture_id),
            representation_id: cid_text(&anchor.representation_id),
            policy_receipt_id: &anchor.policy_receipt_id,
            quote: anchor
                .selector
                .as_ref()
                .map(|selector| selector.exact.as_str()),
            fidelity_warnings: &anchor.fidelity_warnings,
            provider_claim: anchor.provider_claim.as_deref(),
        }
    }
}

pub(crate) fn cid_text(content_id: &ContentId) -> String {
    format!(
        "{}:{}",
        content_id.algorithm,
        content_id
            .bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
