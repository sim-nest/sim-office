//! Source-edition and parallel-reading contracts built on verified evidence selectors.

use super::{WebEvidenceError, cid_text, err};
use serde::{Deserialize, Serialize};
use sim_kernel::ContentId;
use sim_lib_web_core::{EvidenceSelector, WebRepresentation};
use std::collections::{BTreeMap, BTreeSet};

/// Stable identity of one admitted edition of a supplied source.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EditionId(pub String);

/// Offline provenance for legally supplied source material. No acquisition API is implied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceLocator {
    /// A public identifier such as an ISBN, DOI, or archive catalogue key.
    PublicId {
        /// Identifier scheme, such as `isbn` or `doi`.
        scheme: String,
        /// Identifier value within the named scheme.
        value: String,
    },
    /// A user-controlled private collection and opaque item key.
    PrivateCollection {
        /// Stable name of the user-controlled collection.
        collection: String,
        /// Opaque item key within that collection.
        item: String,
    },
}

/// Legal admission and quotation/export ceilings for one edition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicencePolicy {
    /// Stable policy name shown with derived output.
    pub name: String,
    /// Maximum Unicode scalar values in any single quotation.
    pub max_quote_chars: u32,
    /// Maximum Unicode scalar values exported across one request.
    pub max_export_chars: u32,
    /// Whether the supplied material may leave the private workspace.
    pub permits_export: bool,
}

/// Admission binding an edition to an immutable normalized representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdition {
    /// Caller-assigned stable edition identity.
    pub id: EditionId,
    /// Offline source locator.
    pub locator: SourceLocator,
    /// Immutable representation content identity.
    pub representation_id: ContentId,
    /// Exact supplied text; annotations never modify it.
    pub text: String,
    /// Licence decision governing quotations and exports.
    pub licence: LicencePolicy,
    /// RFC 3339 instant at which admission was checked.
    pub checked_at: String,
    /// Optional RFC 3339 instant after which admission is stale.
    pub expires_at: Option<String>,
}

/// Stable identity of a span admitted through the delivered verified selector.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpanId(pub String);

/// An edition-bound verified span, never raw offsets or rematched text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    /// Stable span identity.
    pub id: SpanId,
    /// Edition containing the immutable representation.
    pub edition_id: EditionId,
    /// Delivered checked selector, including representation identity and exact text.
    pub selector: EvidenceSelector,
}

/// Separate annotation layers; source text itself is held only by `SourceEdition`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadingLayer {
    /// Independently supplied textual witness.
    Witness,
    /// Context supplied by a reader or tool.
    Context,
    /// Interpretation, which may conflict with another interpretation.
    Interpretation,
    /// Reflective note.
    Reflection,
    /// Response to another reading.
    Response,
}

/// Annotation authorship; models may propose annotations but never source spans.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationAuthor {
    /// Human-authored annotation.
    Human(String),
    /// Model-proposed annotation retained as a proposal.
    ModelProposal(String),
}

/// Annotation stored independently from immutable source text and span admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingAnnotation {
    /// Stable annotation id.
    pub id: String,
    /// Exact target span, retained even after it becomes orphaned.
    pub span_id: SpanId,
    /// Semantic layer.
    pub layer: ReadingLayer,
    /// Authorship/proposal provenance.
    pub author: AnnotationAuthor,
    /// Exact supplied annotation text.
    pub text: String,
}

/// Checked quotation returned with edition and admitted-span identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quotation {
    /// Edition identity.
    pub edition_id: EditionId,
    /// Admitted source span identity.
    pub span_id: SpanId,
    /// Exact selected source text.
    pub text: String,
}

/// Offline source library with rebuildable derived indexes.
#[derive(Default)]
pub struct SourceLibrary {
    editions: BTreeMap<EditionId, SourceEdition>,
    spans: BTreeMap<SpanId, SourceSpan>,
    annotations: BTreeMap<String, ReadingAnnotation>,
    spans_by_edition: BTreeMap<EditionId, BTreeSet<SpanId>>,
}

impl SourceLibrary {
    /// Imports an already supplied source edition; this performs no acquisition.
    pub fn import(
        &mut self,
        id: EditionId,
        locator: SourceLocator,
        representation: &WebRepresentation,
        licence: LicencePolicy,
        checked_at: impl Into<String>,
        expires_at: Option<String>,
    ) -> Result<(), WebEvidenceError> {
        if licence.max_quote_chars == 0 || licence.max_export_chars < licence.max_quote_chars {
            return Err(WebEvidenceError(
                "invalid licence quotation ceilings".into(),
            ));
        }
        let edition = SourceEdition {
            id: id.clone(),
            locator,
            representation_id: representation.content_id.clone(),
            text: representation.text.clone(),
            licence,
            checked_at: checked_at.into(),
            expires_at,
        };
        if let Some(existing) = self.editions.get(&id) {
            return if existing == &edition {
                Ok(())
            } else {
                Err(WebEvidenceError(
                    "edition identity already names different material".into(),
                ))
            };
        }
        self.editions.insert(id, edition);
        self.rebuild_indexes();
        Ok(())
    }

    /// Admits a span only after the delivered selector verifies against the edition.
    pub fn admit_span(
        &mut self,
        edition_id: &EditionId,
        id: SpanId,
        selector: EvidenceSelector,
        representation: &WebRepresentation,
    ) -> Result<(), WebEvidenceError> {
        let edition = self
            .editions
            .get(edition_id)
            .ok_or_else(|| WebEvidenceError("missing edition".into()))?;
        if edition.representation_id != representation.content_id
            || edition.text != representation.text
        {
            return Err(WebEvidenceError("edition representation mismatch".into()));
        }
        selector.verify(representation).map_err(err)?;
        if self.spans.contains_key(&id) {
            return Err(WebEvidenceError("span identity already admitted".into()));
        }
        self.spans.insert(
            id.clone(),
            SourceSpan {
                id,
                edition_id: edition_id.clone(),
                selector,
            },
        );
        self.rebuild_indexes();
        Ok(())
    }

    /// Adds an annotation without changing source text or span identity.
    pub fn annotate(&mut self, annotation: ReadingAnnotation) -> Result<(), WebEvidenceError> {
        if !self.spans.contains_key(&annotation.span_id) {
            return Err(WebEvidenceError(
                "annotation target is not an admitted span".into(),
            ));
        }
        if self.annotations.contains_key(&annotation.id) {
            return Err(WebEvidenceError(
                "annotation identity already exists".into(),
            ));
        }
        self.annotations.insert(annotation.id.clone(), annotation);
        Ok(())
    }

    /// Resolves admitted spans and enforces freshness and licence-specific ceilings.
    pub fn quote(
        &self,
        span_ids: &[SpanId],
        checked_at: &str,
        for_export: bool,
    ) -> Result<Vec<Quotation>, WebEvidenceError> {
        let mut used: BTreeMap<&EditionId, u32> = BTreeMap::new();
        let mut out = Vec::with_capacity(span_ids.len());
        for span_id in span_ids {
            let span = self
                .spans
                .get(span_id)
                .ok_or_else(|| WebEvidenceError("quotation span is not admitted".into()))?;
            let edition = self
                .editions
                .get(&span.edition_id)
                .ok_or_else(|| WebEvidenceError("quotation source was removed".into()))?;
            if edition
                .expires_at
                .as_deref()
                .is_some_and(|expiry| checked_at >= expiry)
            {
                return Err(WebEvidenceError("source admission is stale".into()));
            }
            if for_export && !edition.licence.permits_export {
                return Err(WebEvidenceError("licence refuses source export".into()));
            }
            let chars = u32::try_from(span.selector.exact.chars().count()).map_err(err)?;
            if chars > edition.licence.max_quote_chars {
                return Err(WebEvidenceError("quotation exceeds licence ceiling".into()));
            }
            let total = used.entry(&span.edition_id).or_default();
            *total = total
                .checked_add(chars)
                .ok_or_else(|| WebEvidenceError("quotation size overflow".into()))?;
            if *total > edition.licence.max_export_chars {
                return Err(WebEvidenceError(
                    "quotation request exceeds export ceiling".into(),
                ));
            }
            out.push(Quotation {
                edition_id: span.edition_id.clone(),
                span_id: span.id.clone(),
                text: span.selector.exact.clone(),
            });
        }
        Ok(out)
    }

    /// Removes source and admitted spans; annotations remain explicitly orphaned.
    pub fn remove(&mut self, edition_id: &EditionId) -> bool {
        let removed = self.editions.remove(edition_id).is_some();
        self.spans.retain(|_, span| &span.edition_id != edition_id);
        self.rebuild_indexes();
        removed
    }

    /// Rebuilds all derived indexes from authoritative editions and spans.
    pub fn rebuild_indexes(&mut self) {
        self.spans_by_edition.clear();
        for (id, span) in &self.spans {
            if self.editions.contains_key(&span.edition_id) {
                self.spans_by_edition
                    .entry(span.edition_id.clone())
                    .or_default()
                    .insert(id.clone());
            }
        }
    }

    /// Returns annotations whose exact target span is no longer admitted.
    pub fn orphaned_annotations(&self) -> Vec<&ReadingAnnotation> {
        self.annotations
            .values()
            .filter(|a| !self.spans.contains_key(&a.span_id))
            .collect()
    }

    /// Returns an edition for projections and policy display.
    pub fn edition(&self, id: &EditionId) -> Option<&SourceEdition> {
        self.editions.get(id)
    }

    /// Stable representation identity for offline import receipts.
    pub fn representation_key(&self, id: &EditionId) -> Option<String> {
        self.editions
            .get(id)
            .map(|edition| cid_text(&edition.representation_id))
    }
}
