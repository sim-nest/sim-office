use sim_kernel::{Cx, Expr, Value, testing::bare_cx as cx};
use sim_lib_doc_core::{Doc, DocId, DocKind, Edit, Evidence, ExternalRef, LinkRole};
use tempfile::TempDir;

use crate::{DocStore, evidence};

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

#[test]
fn evidence_links_reopen_ordered_without_payload_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("docs.sqlite");
    let subject = DocId::new("site/powerproject/projects/p-1/tasks/17");
    let mail_body = "private mail body must not be stored";
    let project_data = "project item payload must not be stored";
    let rows = vec![
        Evidence::new(
            subject.clone(),
            ExternalRef::new(
                "site/msgraph",
                "messages/msg-1",
                Some("change-key-1".to_owned()),
                Some("https://graph.example/messages/msg-1".to_owned()),
            ),
            LinkRole::SourceDocument,
            30,
            Some("mail-etag-1".to_owned()),
        ),
        Evidence::new(
            subject.clone(),
            ExternalRef::new(
                "site/sharepoint",
                "sites/site-1/drive/items/file-9",
                Some("sharepoint-etag-9".to_owned()),
                Some("https://sharepoint.example/file-9".to_owned()),
            ),
            LinkRole::AccountingSupport,
            10,
            None,
        ),
        Evidence::new(
            subject.clone(),
            ExternalRef::new(
                "site/dalux",
                "items/issue-4",
                Some("2026-07-13T10:00:00Z".to_owned()),
                Some("https://dalux.example/items/issue-4".to_owned()),
            ),
            LinkRole::ProjectIssue,
            20,
            Some("dalux-updated-at".to_owned()),
        ),
        Evidence::new(
            subject.clone(),
            ExternalRef::new("ledger", "voucher/2026/0007", None, None),
            LinkRole::AccountingSupport,
            40,
            Some("voucher-digest".to_owned()),
        ),
    ];

    {
        let store = DocStore::create(&path).unwrap();
        for row in &rows {
            evidence::attach(&store, row).unwrap();
        }
    }

    let store = DocStore::create(&path).unwrap();
    let loaded = evidence::evidence_for(&store, &subject).unwrap();

    assert_eq!(
        loaded
            .iter()
            .map(|row| row.captured_at_seq)
            .collect::<Vec<_>>(),
        vec![10, 20, 30, 40]
    );
    assert_eq!(loaded[0].evidence.backend, "site/sharepoint");
    assert_eq!(loaded[1].evidence.backend, "site/dalux");
    assert_eq!(loaded[2].evidence.backend, "site/msgraph");
    assert_eq!(loaded[3].evidence.backend, "ledger");
    assert!(loaded.iter().all(
        |row| row.evidence.external_id != mail_body && row.evidence.external_id != project_data
    ));

    let bytes = std::fs::read(&path).unwrap();
    assert!(
        !bytes
            .windows(mail_body.len())
            .any(|window| window == mail_body.as_bytes())
    );
    assert!(
        !bytes
            .windows(project_data.len())
            .any(|window| window == project_data.as_bytes())
    );
}

fn value_expr(cx: &mut Cx, value: &Value) -> Expr {
    value.object().as_expr(cx).unwrap()
}
