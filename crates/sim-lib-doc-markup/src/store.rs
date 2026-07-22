//! Article document storage helpers for markup-backed office documents.

use sim_codec_doc::BackendId;
use sim_lib_doc_core::{Doc, DocId, ExternalRef, OfficeError};
use sim_lib_doc_store::DocStore;

use crate::MARKUP_DOC_KIND;

/// Prefix used for open preferred-backend metadata in document origins.
pub const MARKUP_BACKEND_META_PREFIX: &str = "codec/markup-doc/";

/// Attach or replace the preferred markup backend on an article document.
pub fn with_preferred_backend(mut doc: Doc, backend: &BackendId) -> Result<Doc, OfficeError> {
    ensure_article(&doc)?;
    let backend = format!("{MARKUP_BACKEND_META_PREFIX}{}", backend.as_str());
    doc.origin
        .retain(|origin| !origin.backend.starts_with(MARKUP_BACKEND_META_PREFIX));
    doc.origin
        .push(ExternalRef::new(backend, "preferred-source", None, None));
    Ok(doc)
}

/// Returns the preferred markup backend carried by open document metadata.
#[must_use]
pub fn preferred_backend(doc: &Doc) -> Option<BackendId> {
    doc.origin.iter().find_map(|origin| {
        origin
            .backend
            .strip_prefix(MARKUP_BACKEND_META_PREFIX)
            .map(BackendId::new)
    })
}

/// Save a markup article document through the office document store.
pub fn save_article_doc(store: &DocStore, doc: &Doc) -> Result<(), OfficeError> {
    ensure_article(doc)?;
    store
        .save_doc(doc)
        .map_err(|error| OfficeError::Kernel(format!("document store error: {error}")))
}

/// Load a markup article document through the office document store.
pub fn load_article_doc(store: &DocStore, id: &DocId) -> Result<Option<Doc>, OfficeError> {
    let Some(doc) = store
        .load_doc(id)
        .map_err(|error| OfficeError::Kernel(format!("document store error: {error}")))?
    else {
        return Ok(None);
    };
    ensure_article(&doc)?;
    Ok(Some(doc))
}

fn ensure_article(doc: &Doc) -> Result<(), OfficeError> {
    if doc.kind.as_str() == MARKUP_DOC_KIND {
        Ok(())
    } else {
        Err(OfficeError::Kernel(format!(
            "markup document store handles only {MARKUP_DOC_KIND} documents, got {}",
            doc.kind.as_str()
        )))
    }
}
