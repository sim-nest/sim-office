use std::sync::Arc;

use sim_codec_doc::{BackendId, MarkupDoc, MarkupFidelity, MarkupLoss};
use sim_kernel::{Cx, DefaultFactory, Expr, NoopEvalPolicy, Symbol};
use sim_lib_doc_core::{Doc, DocCodec, DocCodecOptions, DocId, DocKind};

use crate::{MarkupDocCodec, office_fidelity};

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn default_options(cx: &mut Cx) -> DocCodecOptions {
    DocCodecOptions::new(cx.factory().nil().unwrap())
}

fn options(cx: &mut Cx, entries: Vec<(&str, Expr)>) -> DocCodecOptions {
    let fields = entries
        .into_iter()
        .map(|(name, value)| {
            (
                Symbol::new(name),
                cx.factory().expr(value).expect("option value"),
            )
        })
        .collect();
    DocCodecOptions::new(cx.factory().table(fields).unwrap())
}

fn markup_body(cx: &mut Cx, doc: &Doc) -> MarkupDoc {
    let expr = doc.body.object().as_expr(cx).unwrap();
    MarkupDoc::from_expr(&expr).unwrap()
}

#[test]
fn markdown_bytes_decode_to_office_article_doc() {
    let mut cx = cx();
    let codec = MarkupDocCodec::markdown();
    let source = b"# Guide\n\nA **portable** article.\n";
    let opts = default_options(&mut cx);

    let (doc, report) = codec.decode(&mut cx, source, &opts).unwrap();

    assert_eq!(doc.kind, DocKind::new("article"));
    assert_eq!(report.backend, "codec/markup/markdown");
    assert!(report.is_lossless());
    assert_eq!(markup_body(&mut cx, &doc).title.as_deref(), Some("Guide"));
}

#[test]
fn typst_bytes_encode_from_same_doc() {
    let mut cx = cx();
    let markdown = MarkupDocCodec::markdown();
    let typst = MarkupDocCodec::typst();
    let decode_opts = default_options(&mut cx);
    let (doc, _) = markdown
        .decode(
            &mut cx,
            b"# Guide\n\nA **portable** article.\n",
            &decode_opts,
        )
        .unwrap();

    let encode_opts = default_options(&mut cx);
    let (bytes, report) = typst.encode(&mut cx, &doc, &encode_opts).unwrap();
    let encoded = String::from_utf8(bytes).unwrap();

    assert!(report.is_lossless());
    assert!(encoded.contains("Guide"));
    assert!(encoded.contains("portable"));
}

#[test]
fn unknown_doc_kind_fails_closed() {
    let mut cx = cx();
    let codec = MarkupDocCodec::markdown();
    let body = cx.factory().expr(Expr::Nil).unwrap();
    let doc = Doc::new(DocKind::new("sheet"), DocId::new("sheet-1"), body, vec![]);
    let opts = default_options(&mut cx);

    let err = codec.encode(&mut cx, &doc, &opts).unwrap_err();

    assert!(err.to_string().contains("encodes only article"));
}

#[test]
fn losses_map_to_loss_notes() {
    let report = office_fidelity(MarkupFidelity {
        backend: BackendId::new("markdown"),
        preserved_raw: vec!["<aside>x</aside>".to_owned()],
        dropped: vec![MarkupLoss {
            path: "block[2]".to_owned(),
            reason: "target backend has no aside".to_owned(),
        }],
        warnings: vec!["ambiguous table alignment".to_owned()],
    });

    assert_eq!(report.backend, "codec/markup/markdown");
    assert_eq!(report.preserved_extras, vec!["raw:<aside>x</aside>"]);
    assert_eq!(report.dropped[0].field, "block[2]");
    assert_eq!(report.dropped[0].reason, "target backend has no aside");
    assert_eq!(report.warnings, vec!["ambiguous table alignment"]);
}

#[test]
fn decode_options_disable_raw_preservation() {
    let mut cx = cx();
    let codec = MarkupDocCodec::markdown();
    let opts = options(&mut cx, vec![("preserve-raw", Expr::Bool(false))]);
    let (_doc, report) = codec
        .decode(&mut cx, b"# Guide\n\n<div>raw</div>\n", &opts)
        .unwrap();

    assert!(!report.is_lossless());
    assert_eq!(report.dropped[0].field, "html-block");
}

#[test]
fn doc_core_is_markup_free() {
    let manifest = include_str!("../../sim-lib-doc-core/Cargo.toml");

    assert!(!manifest.contains("sim-codec-doc"));
}
