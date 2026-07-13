//! Office `DocCodec` adapter for the shared markup backends.

use std::str;
use std::sync::OnceLock;

use sim_codec_doc::{
    BackendId, MarkupDecodeOptions, MarkupDoc, MarkupEncodeOptions, MarkupError, MarkupFidelity,
    default_backend_registry,
};
use sim_kernel::{Cx, Expr};
use sim_lib_doc_core::{
    DOC_KIND_ARTICLE, Doc, DocCodec, DocCodecOptions, DocId, DocKind, ExternalRef, FidelityReport,
    OfficeError,
};

/// Office document kind produced by markup document codecs.
pub const MARKUP_DOC_KIND: &str = DOC_KIND_ARTICLE;

/// Office document codec backed by one implemented markup backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkupDocCodec {
    /// Backend selected from the markup backend registry.
    pub backend: BackendId,
}

impl MarkupDocCodec {
    /// Builds an office codec for a markup backend id.
    #[must_use]
    pub fn new(backend: BackendId) -> Self {
        Self { backend }
    }

    /// Builds the Markdown office document codec.
    #[must_use]
    pub fn markdown() -> Self {
        markdown_doc_codec()
    }

    /// Builds the Typst office document codec.
    #[must_use]
    pub fn typst() -> Self {
        typst_doc_codec()
    }

    /// Borrows the selected markup backend id.
    #[must_use]
    pub fn backend(&self) -> &BackendId {
        &self.backend
    }
}

/// Builds the Markdown office document codec.
#[must_use]
pub fn markdown_doc_codec() -> MarkupDocCodec {
    MarkupDocCodec::new(BackendId::new("markdown"))
}

/// Builds the Typst office document codec.
#[must_use]
pub fn typst_doc_codec() -> MarkupDocCodec {
    MarkupDocCodec::new(BackendId::new("typst"))
}

impl DocCodec for MarkupDocCodec {
    fn codec_id(&self) -> &'static str {
        codec_id_for(&self.backend)
    }

    fn kinds(&self) -> &'static [DocKind] {
        static KINDS: OnceLock<Vec<DocKind>> = OnceLock::new();
        KINDS
            .get_or_init(|| vec![DocKind::new(MARKUP_DOC_KIND)])
            .as_slice()
    }

    fn decode(
        &self,
        cx: &mut Cx,
        bytes: &[u8],
        options: &DocCodecOptions,
    ) -> Result<(Doc, FidelityReport), OfficeError> {
        let source = str::from_utf8(bytes).map_err(|error| {
            OfficeError::Kernel(format!("markup source must be UTF-8: {error}"))
        })?;
        let backend = backend(&self.backend)?;
        let decode_options = decode_options(cx, options)?;
        let (markup, fidelity) = backend
            .decode(source, &decode_options)
            .map_err(markup_error)?;
        let body = cx
            .factory()
            .expr(markup.as_expr())
            .map_err(OfficeError::from)?;
        let doc = Doc::new(
            DocKind::new(MARKUP_DOC_KIND),
            decoded_doc_id(&self.backend),
            body,
            vec![ExternalRef::new(
                self.codec_id(),
                "inline-source",
                None,
                None,
            )],
        );
        Ok((doc, office_fidelity(fidelity)))
    }

    fn encode(
        &self,
        cx: &mut Cx,
        doc: &Doc,
        options: &DocCodecOptions,
    ) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
        if doc.kind.as_str() != MARKUP_DOC_KIND {
            return Err(OfficeError::Kernel(format!(
                "{} encodes only {MARKUP_DOC_KIND} documents, got {}",
                self.codec_id(),
                doc.kind.as_str()
            )));
        }
        let expr = doc.body.object().as_expr(cx).map_err(OfficeError::from)?;
        let markup = MarkupDoc::from_expr(&expr).map_err(|error| {
            OfficeError::Kernel(format!("document body is not markup data: {error}"))
        })?;
        let backend = backend(&self.backend)?;
        let encode_options = encode_options(cx, options)?;
        let fail_on_loss = encode_options.fail_on_loss;
        let (source, fidelity) = backend
            .encode(&markup, &encode_options)
            .map_err(markup_error)?;
        let report = office_fidelity(fidelity);
        if fail_on_loss && !report.dropped.is_empty() {
            return Err(OfficeError::Kernel(format!(
                "{} reported {} dropped part(s)",
                self.codec_id(),
                report.dropped.len()
            )));
        }
        Ok((source.into_bytes(), report))
    }
}

/// Converts markup fidelity into the office fidelity report shape.
#[must_use]
pub fn office_fidelity(fidelity: MarkupFidelity) -> FidelityReport {
    let mut report = FidelityReport::new(format!("codec/markup/{}", fidelity.backend.as_str()));
    for extra in fidelity.preserved_raw {
        report = report.with_preserved_extra(format!("raw:{extra}"));
    }
    for loss in fidelity.dropped {
        report = report.with_dropped(loss.path, loss.reason);
    }
    for warning in fidelity.warnings {
        report = report.with_warning(warning);
    }
    report
}

fn backend(
    id: &BackendId,
) -> Result<std::sync::Arc<dyn sim_codec_doc::MarkupBackend>, OfficeError> {
    default_backend_registry().backend(id).map_err(markup_error)
}

fn markup_error(error: MarkupError) -> OfficeError {
    OfficeError::Kernel(error.to_string())
}

fn codec_id_for(backend: &BackendId) -> &'static str {
    match backend.as_str() {
        "asciidoc" => "codec/markup-doc/asciidoc",
        "latex" => "codec/markup-doc/latex",
        "markdown" => "codec/markup-doc/markdown",
        "typst" => "codec/markup-doc/typst",
        _ => "codec/markup-doc/unknown",
    }
}

fn decoded_doc_id(backend: &BackendId) -> DocId {
    DocId::new(format!("markup:{}:decoded", backend.as_str()))
}

fn decode_options(
    cx: &mut Cx,
    options: &DocCodecOptions,
) -> Result<MarkupDecodeOptions, OfficeError> {
    let expr = options
        .value()
        .object()
        .as_expr(cx)
        .map_err(OfficeError::from)?;
    let mut options = MarkupDecodeOptions::default();
    if let Some(value) = bool_option(&expr, &["preserve-source", "preserve_source"])? {
        options.preserve_source = value;
    }
    if let Some(value) = bool_option(&expr, &["preserve-raw", "preserve_raw"])? {
        options.preserve_raw = value;
    }
    Ok(options)
}

fn encode_options(
    cx: &mut Cx,
    options: &DocCodecOptions,
) -> Result<MarkupEncodeOptions, OfficeError> {
    let expr = options
        .value()
        .object()
        .as_expr(cx)
        .map_err(OfficeError::from)?;
    let mut options = MarkupEncodeOptions::default();
    if let Some(value) = bool_option(&expr, &["fail-on-loss", "fail_on_loss"])? {
        options.fail_on_loss = value;
    }
    if let Some(value) = bool_option(&expr, &["preserve-raw", "preserve_raw"])? {
        options.preserve_raw = value;
    }
    Ok(options)
}

fn bool_option(expr: &Expr, names: &[&str]) -> Result<Option<bool>, OfficeError> {
    let entries = match expr {
        Expr::Nil => return Ok(None),
        Expr::Map(entries) => entries.as_slice(),
        _ => {
            return Err(OfficeError::Kernel(
                "doc codec options must be nil or a map".to_owned(),
            ));
        }
    };
    for name in names {
        if let Some(value) = field(entries, name) {
            return match value {
                Expr::Bool(value) => Ok(Some(*value)),
                _ => Err(OfficeError::Kernel(format!(
                    "doc codec option {name} must be bool"
                ))),
            };
        }
    }
    Ok(None)
}

fn field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Option<&'a Expr> {
    entries.iter().find_map(|(key, value)| match key {
        Expr::Symbol(symbol) if symbol.as_qualified_str() == name => Some(value),
        Expr::String(text) if text == name => Some(value),
        _ => None,
    })
}
