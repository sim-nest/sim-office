//! Placement contracts for document codecs and service sites.

use sim_kernel::{CapabilityName, Cx, Value};

use crate::{Doc, DocKind, OfficeError, fidelity::FidelityReport};

/// Opaque options passed to document codecs.
#[derive(Clone, Debug, PartialEq)]
pub struct DocCodecOptions(pub Value);

impl DocCodecOptions {
    /// Wraps an opaque runtime value as codec options.
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    /// Borrows the wrapped runtime value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.0
    }
}

/// File or wire codec for one or more document kinds.
pub trait DocCodec {
    /// Stable codec id such as `codec/ooxml-xlsx`.
    fn codec_id(&self) -> &'static str;
    /// Document kinds accepted by this codec.
    fn kinds(&self) -> &'static [DocKind];
    /// Decodes bytes into a document plus fidelity report.
    fn decode(
        &self,
        cx: &mut Cx,
        bytes: &[u8],
        options: &DocCodecOptions,
    ) -> Result<(Doc, FidelityReport), OfficeError>;
    /// Encodes a document into bytes plus fidelity report.
    fn encode(
        &self,
        cx: &mut Cx,
        doc: &Doc,
        options: &DocCodecOptions,
    ) -> Result<(Vec<u8>, FidelityReport), OfficeError>;
}

/// A service or helper-process placement for document operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocSite {
    /// Stable site id such as `site/msgraph`.
    pub site_id: String,
    /// Document kinds served by this site.
    pub kinds: Vec<DocKind>,
    /// Capabilities required for live access.
    pub required_caps: Vec<CapabilityName>,
    /// Whether the site defaults to deterministic modeled behavior.
    pub default_modeled: bool,
}

impl DocSite {
    /// Builds a document site.
    #[must_use]
    pub fn new(
        site_id: impl Into<String>,
        kinds: Vec<DocKind>,
        required_caps: Vec<CapabilityName>,
        default_modeled: bool,
    ) -> Self {
        Self {
            site_id: site_id.into(),
            kinds,
            required_caps,
            default_modeled,
        }
    }

    /// Requires live capabilities unless this site is currently modeled.
    pub fn authorize(&self, cx: &Cx) -> Result<(), OfficeError> {
        if self.default_modeled {
            return Ok(());
        }
        cx.require_all(&self.required_caps)
            .map_err(OfficeError::from)
    }
}

/// Where a document operation is placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// The local document store.
    LocalStore,
    /// A named file or wire codec.
    Codec(String),
    /// A named service or helper-process site.
    Site(String),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, NoopEvalPolicy};

    use crate::{DocId, caps::NET_CONNECT_CAPABILITY};

    use super::*;

    struct EchoCodec;

    impl DocCodec for EchoCodec {
        fn codec_id(&self) -> &'static str {
            "codec/echo"
        }

        fn kinds(&self) -> &'static [DocKind] {
            static KINDS: std::sync::OnceLock<Vec<DocKind>> = std::sync::OnceLock::new();
            KINDS.get_or_init(|| vec![DocKind::new("report")])
        }

        fn decode(
            &self,
            _cx: &mut Cx,
            bytes: &[u8],
            options: &DocCodecOptions,
        ) -> Result<(Doc, FidelityReport), OfficeError> {
            let body = options.value().clone();
            let doc = Doc::new(
                DocKind::new(String::from_utf8_lossy(bytes).to_string()),
                DocId::new("decoded"),
                body,
                vec![],
            );
            Ok((
                doc,
                FidelityReport::new(self.codec_id()).with_warning("used options body"),
            ))
        }

        fn encode(
            &self,
            _cx: &mut Cx,
            doc: &Doc,
            options: &DocCodecOptions,
        ) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
            let uses_options = options.value() == &doc.body;
            let report = if uses_options {
                FidelityReport::new(self.codec_id()).with_warning("options matched body")
            } else {
                FidelityReport::new(self.codec_id())
            };
            Ok((doc.kind.as_str().as_bytes().to_vec(), report))
        }
    }

    #[test]
    fn codec_options_select_behavior() {
        let mut cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        let option_value = cx.factory().string("option-body".to_owned()).unwrap();
        let options = DocCodecOptions::new(option_value.clone());
        let codec = EchoCodec;

        let (doc, decode_report) = codec.decode(&mut cx, b"report", &options).unwrap();
        let (encoded, encode_report) = codec.encode(&mut cx, &doc, &options).unwrap();

        assert_eq!(doc.body, option_value);
        assert_eq!(encoded, b"report");
        assert_eq!(decode_report.warnings, vec!["used options body"]);
        assert_eq!(encode_report.warnings, vec!["options matched body"]);
    }

    #[test]
    fn live_site_requiring_network_is_denied_by_default() {
        let cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        let site = DocSite::new(
            "site/msgraph",
            vec![DocKind::new("sheet")],
            vec![CapabilityName::new(NET_CONNECT_CAPABILITY)],
            false,
        );

        let denied = site.authorize(&cx).unwrap_err();

        assert!(
            matches!(denied, OfficeError::CapabilityDenied(capability) if capability.as_str() == NET_CONNECT_CAPABILITY)
        );
    }

    #[test]
    fn modeled_site_is_allowed_without_live_capabilities() {
        let cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        let site = DocSite::new(
            "site/msgraph",
            vec![DocKind::new("sheet")],
            vec![CapabilityName::new(NET_CONNECT_CAPABILITY)],
            true,
        );

        site.authorize(&cx).unwrap();
    }
}
