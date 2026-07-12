//! Plain document records shared by office codecs, stores, views, and sites.

use sim_kernel::{Cx, Object, Result, Value};

/// Prose article document kind reserved by the office core vocabulary.
pub const DOC_KIND_ARTICLE: &str = "article";
/// Prose report document kind reserved by the office core vocabulary.
pub const DOC_KIND_REPORT: &str = "report";
/// Readme document kind reserved by the office core vocabulary.
pub const DOC_KIND_README: &str = "readme";

/// An office-family document carried as runtime data.
#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    /// Open document kind string.
    pub kind: DocKind,
    /// Stable document id.
    pub id: DocId,
    /// Opaque runtime body owned by the domain layer.
    pub body: Value,
    /// External records this document came from or syncs with.
    pub origin: Vec<ExternalRef>,
}

impl Doc {
    /// Build a document record.
    #[must_use]
    pub fn new(kind: DocKind, id: DocId, body: Value, origin: Vec<ExternalRef>) -> Self {
        Self {
            kind,
            id,
            body,
            origin,
        }
    }
}

impl Object for Doc {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<doc {} {}>", self.kind.0, self.id.0))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for Doc {}

/// Open document kind name.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct DocKind(pub String);

impl DocKind {
    /// Build a document kind from an open string.
    #[must_use]
    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }

    /// Borrow the kind string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable document identifier.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct DocId(pub String);

impl DocId {
    /// Build a document id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reference to an external file, service object, row, task, or source record.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExternalRef {
    /// Backend namespace such as a codec or site.
    pub backend: String,
    /// Backend-local id.
    pub external_id: String,
    /// Optional backend version, revision, row version, ETag, or content hash.
    pub version: Option<String>,
    /// Optional browser-facing URL.
    pub web_url: Option<String>,
}

impl ExternalRef {
    /// Build an external reference.
    #[must_use]
    pub fn new(
        backend: impl Into<String>,
        external_id: impl Into<String>,
        version: Option<String>,
        web_url: Option<String>,
    ) -> Self {
        Self {
            backend: backend.into(),
            external_id: external_id.into(),
            version,
            web_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_json_round_trip() {
        let refs = vec![ExternalRef::new(
            "codec/plain",
            "doc-1",
            Some("rev-2".to_owned()),
            Some("https://example.com/doc-1".to_owned()),
        )];
        let encoded = serde_json::to_string(&refs).unwrap();
        let decoded: Vec<ExternalRef> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, refs);
    }

    #[test]
    fn prose_kind_constants_are_reserved() {
        assert_eq!(DocKind::new(DOC_KIND_ARTICLE).as_str(), "article");
        assert_eq!(DocKind::new(DOC_KIND_REPORT).as_str(), "report");
        assert_eq!(DocKind::new(DOC_KIND_README).as_str(), "readme");
    }
}
