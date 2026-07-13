use std::sync::Arc;

use sim_codec_doc::{
    BackendId, Inline, MarkupBlock, MarkupDoc, MarkupEdit, MarkupFidelity, MarkupLoss,
};
use sim_kernel::{Cx, DefaultFactory, Expr, NoopEvalPolicy, Symbol};
use sim_lib_doc_core::{Doc, DocCodec, DocCodecOptions, DocId, DocKind, SurfaceCaps};
use sim_lib_doc_store::DocStore;
use sim_lib_intent::{Origin, intent};

use crate::{
    MARKUP_EDIT_DOMAIN, MarkupDocCodec, apply_markup_edit, decode_markup_suite_intent,
    load_article_doc, markup_suite_scene, office_fidelity, preferred_backend, save_article_doc,
    with_preferred_backend,
};

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

fn article_doc(cx: &mut Cx) -> Doc {
    let codec = MarkupDocCodec::markdown();
    let opts = default_options(cx);
    let (mut doc, _) = codec
        .decode(cx, b"# Guide\n\nA **portable** article.\n", &opts)
        .unwrap();
    doc.id = DocId::new("article-1");
    with_preferred_backend(doc, &BackendId::new("markdown")).unwrap()
}

fn sheet_doc(cx: &mut Cx) -> Doc {
    Doc::new(
        DocKind::new("sheet"),
        DocId::new("sheet-1"),
        cx.factory().string("sheet body".to_owned()).unwrap(),
        vec![],
    )
}

fn value_expr(cx: &mut Cx, value: sim_kernel::Value) -> Expr {
    value.object().as_expr(cx).unwrap()
}

fn field<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match key {
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
            Some(value)
        }
        _ => None,
    })
}

fn child_ids(scene: &Expr) -> Vec<String> {
    let children = field(scene, "children").and_then(|value| match value {
        Expr::List(children) => Some(children),
        _ => None,
    });
    children
        .into_iter()
        .flatten()
        .filter_map(|child| match field(child, "id") {
            Some(Expr::String(id)) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

fn contains_text(expr: &Expr, needle: &str) -> bool {
    match expr {
        Expr::String(text) => text.contains(needle),
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| contains_text(key, needle) || contains_text(value, needle)),
        Expr::List(items) | Expr::Vector(items) => {
            items.iter().any(|item| contains_text(item, needle))
        }
        _ => false,
    }
}

fn block_text(block: &MarkupBlock) -> String {
    match block {
        MarkupBlock::Heading { text, .. } | MarkupBlock::Paragraph { content: text, .. } => {
            inline_text(text)
        }
        other => format!("{other:?}"),
    }
}

fn inline_text(items: &[Inline]) -> String {
    items
        .iter()
        .map(|item| match item {
            Inline::Text(text) | Inline::Code(text) => text.clone(),
            Inline::Emph(children) | Inline::Strong(children) => inline_text(children),
            Inline::Link { label, .. } => inline_text(label),
            Inline::Math(source) => source.text.clone(),
            Inline::Raw { text, .. } => text.clone(),
        })
        .collect()
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

#[test]
fn article_and_sheet_share_one_suite_scene() {
    let mut cx = cx();
    let article = article_doc(&mut cx);
    let sheet = sheet_doc(&mut cx);
    let docs = vec![sheet, article];

    let scene_value = sim_lib_doc_surface::suite_scene(&mut cx, &[], &docs).unwrap();
    let scene = value_expr(&mut cx, scene_value);

    sim_lib_scene::validate_scene(&scene).unwrap();
    assert_eq!(child_ids(&scene), ["article-1", "sheet-1"]);
}

#[test]
fn markup_intent_is_preview_edit() {
    let mut cx = cx();
    let doc = article_doc(&mut cx);
    let replacement = MarkupBlock::Paragraph {
        content: vec![Inline::Text("Updated prose.".to_owned())],
        span: None,
    };
    let intent = intent(
        "edit-field",
        Origin::human(3),
        vec![
            ("target", Expr::String("article-1".to_owned())),
            (
                "path",
                Expr::List(vec![
                    Expr::String("blocks".to_owned()),
                    Expr::String("1".to_owned()),
                ]),
            ),
            ("value", replacement.as_expr()),
        ],
    );
    let value = cx.factory().expr(intent).unwrap();

    let edit = decode_markup_suite_intent(&mut cx, &doc, value).unwrap();
    let op = edit.op.object().as_expr(&mut cx).unwrap();
    let decoded = MarkupEdit::from_expr(&op).unwrap();

    assert_eq!(edit.doc, DocId::new("article-1"));
    assert_eq!(edit.domain, MARKUP_EDIT_DOMAIN);
    assert!(matches!(decoded, MarkupEdit::ReplaceBlock { index: 1, .. }));
    assert_eq!(
        block_text(&markup_body(&mut cx, &doc).blocks[1]),
        "A portable article."
    );

    let mut applied = doc.clone();
    apply_markup_edit(&mut cx, &mut applied, &edit).unwrap();
    assert_eq!(markup_body(&mut cx, &applied).blocks[1], replacement);
}

#[test]
fn reload_preserves_preferred_backend() {
    let dir = tempfile::tempdir().unwrap();
    let store = DocStore::create(&dir.path().join("docs.sqlite")).unwrap();
    let mut cx = cx();
    let doc = article_doc(&mut cx);

    save_article_doc(&store, &doc).unwrap();
    let loaded = load_article_doc(&store, &doc.id).unwrap().unwrap();

    assert_eq!(preferred_backend(&loaded), Some(BackendId::new("markdown")));
    assert_eq!(
        markup_body(&mut cx, &loaded).title.as_deref(),
        Some("Guide")
    );
}

#[test]
fn source_and_formatted_lens_caps_project_article_scenes() {
    let mut cx = cx();
    let doc = article_doc(&mut cx);
    let source_caps = SurfaceCaps::new()
        .lens("source")
        .backend("markdown")
        .target("screen");
    let formatted_caps = SurfaceCaps::new()
        .lens("formatted")
        .backend("markdown")
        .target("screen");

    let source_value = markup_suite_scene(&mut cx, &doc, &source_caps).unwrap();
    let source = value_expr(&mut cx, source_value);
    let formatted_value = markup_suite_scene(&mut cx, &doc, &formatted_caps).unwrap();
    let formatted = value_expr(&mut cx, formatted_value);

    sim_lib_scene::validate_scene(&source).unwrap();
    sim_lib_scene::validate_scene(&formatted).unwrap();
    assert!(contains_text(&source, "# Guide"));
    assert!(contains_text(&source, "**portable**"));
    assert!(contains_text(&formatted, "portable"));
    assert!(!contains_text(&formatted, "**portable**"));
}
