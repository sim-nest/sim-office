//! Durable, network-free web evidence in the ordinary office model.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use sim_codec_doc::{BackendId, Inline, MarkupBlock, MarkupDoc, SourceDoc};
use sim_kernel::{ContentId, Cx, Datum, Symbol};
use sim_lib_doc_core::{Doc, DocId, DocKind, Evidence, ExternalRef, LinkRole};
use sim_lib_doc_store::{
    DocStore,
    web::{WebAnchorRow, WebCaptureRow, WebRepresentationRow},
};
use sim_lib_web_core::{
    DecodeLimits, EvidenceSelector, RepresentationMetadata, WebCapture, WebRepresentation,
};
use std::{collections::BTreeMap, error::Error, fmt};

/// Cookbook recipes embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

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
pub struct WebEvidenceError(String);
impl fmt::Display for WebEvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for WebEvidenceError {}
impl From<sim_lib_doc_store::StoreError> for WebEvidenceError {
    fn from(e: sim_lib_doc_store::StoreError) -> Self {
        Self(e.to_string())
    }
}

impl EvidenceAnchor {
    /// Constructs an exact quotation after validating the selector and its context.
    pub fn quote(
        input: AnchorInput<'_>,
        selector: EvidenceSelector,
    ) -> Result<Self, WebEvidenceError> {
        selector.verify(input.representation).map_err(err)?;
        validate_context(&selector, &input.representation.text)?;
        Self::build(input, Some(selector), AnchorKind::Quote)
    }
    /// Constructs evidence supporting a paraphrase from checked selected source text.
    pub fn paraphrase_support(
        input: AnchorInput<'_>,
        selector: EvidenceSelector,
    ) -> Result<Self, WebEvidenceError> {
        selector.verify(input.representation).map_err(err)?;
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
            return Err(WebEvidenceError(
                "representation does not name supplied capture".into(),
            ));
        }
        if input.source_uri != input.capture.retrieval_uri.as_str() {
            return Err(WebEvidenceError(
                "source URI differs from capture URI".into(),
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
    /// Renders one canonical citation record.
    pub fn render(&self, format: CitationFormat) -> Result<String, WebEvidenceError> {
        let quote = self.selector.as_ref().map(|s| s.exact.as_str());
        match format {
            CitationFormat::Plain => Ok(format!(
                "{} — {} (retrieved {}; capture {}; representation {}; policy {}){}{}",
                self.source_title,
                self.source_uri,
                self.retrieved_at,
                cid_text(&self.capture_id),
                cid_text(&self.representation_id),
                self.policy_receipt_id,
                quote.map_or(String::new(), |q| format!(": “{q}”")),
                warnings(self)
            )),
            CitationFormat::Markdown => Ok(format!(
                "[{}]({}) (retrieved `{}`; capture `{}`; representation `{}`; policy `{}`){}{}",
                self.source_title,
                self.source_uri,
                self.retrieved_at,
                cid_text(&self.capture_id),
                cid_text(&self.representation_id),
                self.policy_receipt_id,
                quote.map_or(String::new(), |q| format!(": > {q}")),
                warnings(self)
            )),
            CitationFormat::Lisp => Ok(format!(
                "(citation :title {:?} :uri {:?} :retrieved {:?} :capture {:?} :representation {:?} :policy {:?} :quote {:?} :warnings {:?} :provider-claim {:?})",
                self.source_title,
                self.source_uri,
                self.retrieved_at,
                cid_text(&self.capture_id),
                cid_text(&self.representation_id),
                self.policy_receipt_id,
                quote,
                self.fidelity_warnings,
                self.provider_claim
            )),
            CitationFormat::Json => serde_json::to_string(&CitationRecord::from(self)).map_err(err),
        }
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

/// Citation output format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CitationFormat {
    /// Plain text.
    Plain,
    /// Markdown.
    Markdown,
    /// Lisp data.
    Lisp,
    /// JSON.
    Json,
}

/// Saves exact capture bytes.
pub fn save_capture(store: &mut DocStore, capture: &WebCapture) -> Result<(), WebEvidenceError> {
    let exchange = serde_json::json!({"method":capture.exchange.method,"status":capture.exchange.status,"final_uri":capture.exchange.final_uri,"media_type":capture.exchange.media_type,"received_bytes":capture.exchange.received_bytes});
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
    rep: &WebRepresentation,
) -> Result<(), WebEvidenceError> {
    let metadata = MetadataRecord::from(rep);
    store.save_web_representation(&WebRepresentationRow {
        representation_id: cid_text(&rep.content_id),
        capture_id: cid_text(&rep.raw_source_id),
        text: rep.text.clone(),
        metadata_json: serde_json::to_string(&metadata).map_err(err)?,
    })?;
    Ok(())
}
/// Persists an anchor only after recomputing representation identity and selector validity.
pub fn save_anchor(store: &mut DocStore, anchor: &EvidenceAnchor) -> Result<(), WebEvidenceError> {
    let rep = checked_representation(store, &cid_text(&anchor.representation_id))?;
    verify_anchor(anchor, &rep)?;
    store.save_web_anchor(&WebAnchorRow {
        anchor_id: anchor.anchor_id.clone(),
        subject: anchor.evidence.subject.as_str().into(),
        representation_id: cid_text(&anchor.representation_id),
        record_json: serde_json::to_string(&AnchorRecord::from(anchor)).map_err(err)?,
    })?;
    Ok(())
}
/// Loads and revalidates an anchor before returning it to callers.
pub fn load_anchor(store: &DocStore, id: &str) -> Result<Option<EvidenceAnchor>, WebEvidenceError> {
    let Some(row) = store.load_web_anchor(id)? else {
        return Ok(None);
    };
    let rep = checked_representation(store, &row.representation_id)?;
    let record: AnchorRecord = serde_json::from_str(&row.record_json).map_err(err)?;
    let anchor = record.into_anchor(&rep)?;
    if anchor.anchor_id != row.anchor_id
        || anchor.evidence.subject.as_str() != row.subject
        || cid_text(&anchor.representation_id) != row.representation_id
    {
        return Err(WebEvidenceError(
            "anchor index does not match record".into(),
        ));
    }
    verify_anchor(&anchor, &rep)?;
    Ok(Some(anchor))
}

/// Projects normalized HTML or feed text into an ordinary markup-backed office document.
pub fn project_document(
    cx: &mut Cx,
    anchor: &EvidenceAnchor,
    rep: &WebRepresentation,
) -> Result<Doc, WebEvidenceError> {
    if anchor.representation_id != rep.content_id {
        return Err(WebEvidenceError(
            "projection representation mismatch".into(),
        ));
    }
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "web/representation-id".into(),
        sim_kernel::Expr::String(cid_text(&rep.content_id)),
    );
    if let Some(path) = anchor
        .selector
        .as_ref()
        .and_then(|s| s.structural_path.as_ref())
    {
        attrs.insert(
            "web/structural-path".into(),
            sim_kernel::Expr::Vector(path.iter().cloned().map(sim_kernel::Expr::String).collect()),
        );
    }
    let markup = MarkupDoc {
        title: Some(anchor.source_title.clone()),
        blocks: vec![MarkupBlock::Paragraph {
            content: vec![Inline::Text(rep.text.clone())],
            span: None,
        }],
        attrs,
        source: Some(SourceDoc {
            backend: BackendId::new(rep.codec.clone()),
            text: rep.text.clone(),
        }),
    };
    let body = cx.factory().expr(markup.as_expr()).map_err(err)?;
    Ok(Doc::new(
        DocKind::new("web-source"),
        anchor.evidence.subject.clone(),
        body,
        vec![anchor.evidence.evidence.clone()],
    ))
}

fn checked_representation(
    store: &DocStore,
    id: &str,
) -> Result<WebRepresentation, WebEvidenceError> {
    let row = store
        .load_web_representation(id)?
        .ok_or_else(|| WebEvidenceError("missing representation".into()))?;
    let meta: MetadataRecord = serde_json::from_str(&row.metadata_json).map_err(err)?;
    let raw = meta.raw_id.to_cid()?;
    if cid_text(&raw) != row.capture_id {
        return Err(WebEvidenceError(
            "representation capture relation mismatch".into(),
        ));
    }
    let capture = store
        .load_web_capture(&row.capture_id)?
        .ok_or_else(|| WebEvidenceError("missing raw capture".into()))?;
    let observed_raw = Datum::Bytes(capture.body).content_id().map_err(err)?;
    if observed_raw != raw {
        return Err(WebEvidenceError(
            "raw capture content identity mismatch".into(),
        ));
    }
    let rep =
        WebRepresentation::checked(raw, row.text, meta.into_metadata(), DecodeLimits::default())
            .map_err(err)?;
    if cid_text(&rep.content_id) != row.representation_id || row.representation_id != id {
        return Err(WebEvidenceError(
            "representation content identity mismatch".into(),
        ));
    }
    Ok(rep)
}
fn verify_anchor(anchor: &EvidenceAnchor, rep: &WebRepresentation) -> Result<(), WebEvidenceError> {
    if anchor.representation_id != rep.content_id
        || anchor.capture_id != rep.raw_source_id
        || anchor.codec != rep.codec
        || anchor.codec_version != rep.codec_version
        || anchor.fidelity_warnings != rep.fidelity_warnings
    {
        return Err(WebEvidenceError(
            "anchor provenance does not match representation".into(),
        ));
    }
    match (&anchor.kind, &anchor.selector) {
        (AnchorKind::WholeDocument, None) => Ok(()),
        (_, Some(s)) => {
            s.verify(rep).map_err(err)?;
            validate_context(s, &rep.text)
        }
        _ => Err(WebEvidenceError("anchor kind requires selector".into())),
    }
}
fn validate_context(s: &EvidenceSelector, text: &str) -> Result<(), WebEvidenceError> {
    let chars: Vec<char> = text.chars().collect();
    let start = s.start as usize;
    let end = s.end as usize;
    if let Some(p) = &s.prefix {
        let observed: String = chars[..start]
            .iter()
            .rev()
            .take(p.chars().count())
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if &observed != p {
            return Err(WebEvidenceError("selector prefix mismatch".into()));
        }
    }
    if let Some(x) = &s.suffix {
        let observed: String = chars[end..].iter().take(x.chars().count()).collect();
        if &observed != x {
            return Err(WebEvidenceError("selector suffix mismatch".into()));
        }
    }
    Ok(())
}
fn warnings(a: &EvidenceAnchor) -> String {
    let mut v = String::new();
    if !a.fidelity_warnings.is_empty() {
        v.push_str(&format!(
            "; fidelity warnings: {}",
            a.fidelity_warnings.join(", ")
        ))
    }
    if let Some(c) = &a.provider_claim {
        v.push_str(&format!("; provider claim: {c}"))
    }
    v
}
fn err(e: impl fmt::Display) -> WebEvidenceError {
    WebEvidenceError(e.to_string())
}
fn cid_text(id: &ContentId) -> String {
    format!(
        "{}:{}",
        id.algorithm,
        id.bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

#[derive(Serialize, Deserialize)]
struct CidRecord {
    namespace: Option<String>,
    name: String,
    bytes: Vec<u8>,
}
impl CidRecord {
    fn from_cid(c: &ContentId) -> Self {
        Self {
            namespace: c.algorithm.namespace.as_ref().map(|v| v.to_string()),
            name: c.algorithm.name.to_string(),
            bytes: c.bytes.to_vec(),
        }
    }
    fn to_cid(&self) -> Result<ContentId, WebEvidenceError> {
        let bytes: [u8; 32] = self
            .bytes
            .clone()
            .try_into()
            .map_err(|_| WebEvidenceError("invalid content digest".into()))?;
        let s = self.namespace.as_ref().map_or_else(
            || Symbol::new(self.name.clone()),
            |n| Symbol::qualified(n.as_str(), self.name.as_str()),
        );
        Ok(ContentId::from_bytes(s, bytes))
    }
}
#[derive(Serialize, Deserialize)]
struct MetadataRecord {
    raw_id: CidRecord,
    codec: String,
    codec_version: String,
    media_type: String,
    charset: Option<String>,
    language: Option<String>,
    fidelity_warnings: Vec<String>,
}
impl From<&WebRepresentation> for MetadataRecord {
    fn from(r: &WebRepresentation) -> Self {
        Self {
            raw_id: CidRecord::from_cid(&r.raw_source_id),
            codec: r.codec.clone(),
            codec_version: r.codec_version.clone(),
            media_type: r.media_type.clone(),
            charset: r.charset.clone(),
            language: r.language.clone(),
            fidelity_warnings: r.fidelity_warnings.clone(),
        }
    }
}
impl MetadataRecord {
    fn into_metadata(self) -> RepresentationMetadata {
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
struct AnchorRecord {
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
    fn from(a: &EvidenceAnchor) -> Self {
        Self {
            anchor_id: a.anchor_id.clone(),
            subject: a.evidence.subject.as_str().into(),
            capture: CidRecord::from_cid(&a.capture_id),
            representation: CidRecord::from_cid(&a.representation_id),
            selector: a.selector.as_ref().map(|s| SelectorRecord {
                start: s.start,
                end: s.end,
                exact: s.exact.clone(),
                prefix: s.prefix.clone(),
                suffix: s.suffix.clone(),
                path: s.structural_path.clone(),
            }),
            source_uri: a.source_uri.clone(),
            source_title: a.source_title.clone(),
            retrieved_at: a.retrieved_at.clone(),
            policy_receipt_id: a.policy_receipt_id.clone(),
            provider_claim: a.provider_claim.clone(),
            kind: a.kind,
            captured_at_seq: a.evidence.captured_at_seq,
        }
    }
}
impl AnchorRecord {
    fn into_anchor(self, rep: &WebRepresentation) -> Result<EvidenceAnchor, WebEvidenceError> {
        let representation = self.representation.to_cid()?;
        let selector = self
            .selector
            .map(|s| {
                EvidenceSelector::checked(
                    representation.clone(),
                    s.start,
                    s.end,
                    s.exact,
                    &rep.text,
                )
                .map(|v| v.with_context(s.prefix, s.suffix, s.path))
            })
            .transpose()
            .map_err(err)?;
        let capture = self.capture.to_cid()?;
        let evidence = Evidence::new(
            DocId::new(self.subject),
            ExternalRef::new(
                "web/capture",
                cid_text(&capture),
                Some(cid_text(&representation)),
                Some(self.source_uri.clone()),
            ),
            LinkRole::SourceDocument,
            self.captured_at_seq,
            Some(cid_text(&representation)),
        );
        Ok(EvidenceAnchor {
            evidence,
            anchor_id: self.anchor_id,
            capture_id: capture,
            representation_id: representation,
            selector,
            source_uri: self.source_uri,
            source_title: self.source_title,
            retrieved_at: self.retrieved_at,
            codec: rep.codec.clone(),
            codec_version: rep.codec_version.clone(),
            fidelity_warnings: rep.fidelity_warnings.clone(),
            policy_receipt_id: self.policy_receipt_id,
            provider_claim: self.provider_claim,
            kind: self.kind,
        })
    }
}
#[derive(Serialize)]
struct CitationRecord<'a> {
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
    fn from(a: &'a EvidenceAnchor) -> Self {
        Self {
            title: &a.source_title,
            uri: &a.source_uri,
            retrieved_at: &a.retrieved_at,
            capture_id: cid_text(&a.capture_id),
            representation_id: cid_text(&a.representation_id),
            policy_receipt_id: &a.policy_receipt_id,
            quote: a.selector.as_ref().map(|s| s.exact.as_str()),
            fidelity_warnings: &a.fidelity_warnings,
            provider_claim: a.provider_claim.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests;
