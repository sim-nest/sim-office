use crate::{EvidenceAnchor, WebEvidenceError, records::cid_text};
use sim_codec_doc::{BackendId, Inline, MarkupBlock, MarkupDoc, SourceDoc};
use sim_kernel::{Cx, Expr};
use sim_lib_doc_core::{Doc, DocKind};
use sim_lib_web_core::WebRepresentation;
use std::collections::BTreeMap;

/// Projects normalized HTML or feed text into an ordinary markup-backed office document.
pub fn project_document(
    cx: &mut Cx,
    anchor: &EvidenceAnchor,
    representation: &WebRepresentation,
) -> Result<Doc, WebEvidenceError> {
    if anchor.representation_id != representation.content_id {
        return Err(WebEvidenceError::message(
            "projection representation mismatch",
        ));
    }
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "web/representation-id".into(),
        Expr::String(cid_text(&representation.content_id)),
    );
    if let Some(path) = anchor
        .selector
        .as_ref()
        .and_then(|selector| selector.structural_path.as_ref())
    {
        attrs.insert(
            "web/structural-path".into(),
            Expr::Vector(path.iter().cloned().map(Expr::String).collect()),
        );
    }
    let markup = MarkupDoc {
        title: Some(anchor.source_title.clone()),
        blocks: vec![MarkupBlock::Paragraph {
            content: vec![Inline::Text(representation.text.clone())],
            span: None,
        }],
        attrs,
        source: Some(SourceDoc {
            backend: BackendId::new(representation.codec.clone()),
            text: representation.text.clone(),
        }),
    };
    let body = cx
        .factory()
        .expr(markup.as_expr())
        .map_err(WebEvidenceError::from_display)?;
    Ok(Doc::new(
        DocKind::new("web-source"),
        anchor.evidence.subject.clone(),
        body,
        vec![anchor.evidence.evidence.clone()],
    ))
}
