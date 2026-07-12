use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, Expr, NoopEvalPolicy, Value};
use sim_lib_doc_core::{Doc, DocId, DocKind, Edit, ExternalRef};
use tempfile::TempDir;

use crate::DocStore;

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn store() -> (TempDir, DocStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = DocStore::create(&dir.path().join("docs.sqlite")).unwrap();
    (dir, store)
}

fn doc_with_body(cx: &mut Cx, body: &str) -> Doc {
    Doc::new(
        DocKind::new("report"),
        DocId::new("doc-1"),
        cx.factory().string(body.to_owned()).unwrap(),
        vec![ExternalRef::new(
            "local",
            "doc-1.md",
            Some("rev-1".to_owned()),
            None,
        )],
    )
}

#[test]
fn save_and_load_doc_snapshot() {
    let (_dir, store) = store();
    let mut cx = cx();
    let doc = doc_with_body(&mut cx, "body text");

    store.save_doc(&doc).unwrap();
    let loaded = store.load_doc(&doc.id).unwrap().unwrap();

    assert_eq!(loaded.id, doc.id);
    assert_eq!(loaded.kind, doc.kind);
    assert_eq!(loaded.origin, doc.origin);
    assert_eq!(
        value_expr(&mut cx, &loaded.body),
        Expr::String("body text".to_owned())
    );
}

#[test]
fn project_commit_then_undo_last_returns_inverse_edit() {
    let (_dir, store) = store();
    let mut cx = cx();
    let doc_id = DocId::new("doc-1");
    let edit = Edit::new(
        doc_id.clone(),
        "office/body",
        cx.factory().string("new body".to_owned()).unwrap(),
        cx.factory().string("old body".to_owned()).unwrap(),
    );

    let seq = store.project_commit(&doc_id, &edit, 42).unwrap();
    let undo = store.undo_last(&doc_id).unwrap().unwrap();

    assert_eq!(seq, 42);
    assert_eq!(undo.doc, doc_id);
    assert_eq!(undo.domain, "office/body");
    assert_eq!(
        value_expr(&mut cx, &undo.op),
        Expr::String("old body".to_owned())
    );
    assert_eq!(
        value_expr(&mut cx, &undo.inverse),
        Expr::String("new body".to_owned())
    );
}

#[test]
fn projected_edit_does_not_replace_saved_doc_body() {
    let (_dir, store) = store();
    let mut cx = cx();
    let doc = doc_with_body(&mut cx, "ledger body before edit");
    let edit = Edit::new(
        doc.id.clone(),
        "office/body",
        cx.factory()
            .string("projected body after edit".to_owned())
            .unwrap(),
        cx.factory()
            .string("ledger body before edit".to_owned())
            .unwrap(),
    );

    store.save_doc(&doc).unwrap();
    store.project_commit(&doc.id, &edit, 7).unwrap();
    let loaded = store.load_doc(&doc.id).unwrap().unwrap();

    assert_eq!(
        value_expr(&mut cx, &loaded.body),
        Expr::String("ledger body before edit".to_owned())
    );
}

fn value_expr(cx: &mut Cx, value: &Value) -> Expr {
    value.object().as_expr(cx).unwrap()
}
