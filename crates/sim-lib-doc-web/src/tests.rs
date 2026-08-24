use super::*;
use sim_kernel::{Datum, DefaultFactory, HandleSeed, NoopEvalPolicy};
use sim_lib_net_core::normalize_retrieval_uri;
use sim_lib_web_core::{WebExchange, WebRecordError};
use std::sync::Arc;
use tempfile::tempdir;

fn fixture(text: &str) -> (WebCapture, WebRepresentation) {
    let body = text.as_bytes().to_vec();
    let raw_id = Datum::Bytes(body.clone()).content_id().unwrap();
    let capture = WebCapture::checked(
        normalize_retrieval_uri("https://example.test/article").unwrap(),
        raw_id.clone(),
        body,
        WebExchange {
            method: "GET".into(),
            status: 200,
            final_uri: "https://example.test/article".into(),
            media_type: Some("text/html".into()),
            received_bytes: text.len() as u64,
        },
        DecodeLimits::default(),
    )
    .unwrap();
    let representation = WebRepresentation::checked(
        raw_id,
        text.into(),
        RepresentationMetadata {
            codec: "html5".into(),
            codec_version: "1".into(),
            media_type: "text/html".into(),
            charset: Some("utf-8".into()),
            language: Some("en".into()),
            fidelity_warnings: vec!["script omitted".into()],
        },
        DecodeLimits::default(),
    )
    .unwrap();
    (capture, representation)
}

fn quote_anchor<'a>(capture: &'a WebCapture, rep: &'a WebRepresentation) -> EvidenceAnchor {
    let selector = rep.select(6, 10).unwrap().with_context(
        Some("Hello ".into()),
        Some(" 🦀".into()),
        Some(vec!["html".into(), "body".into(), "p[1]".into()]),
    );
    EvidenceAnchor::quote(
        AnchorInput {
            anchor_id: "anchor-1",
            subject: &DocId::new("source-doc"),
            capture,
            representation: rep,
            source_uri: capture.retrieval_uri.as_str(),
            source_title: "Example",
            retrieved_at: "2026-08-24T12:00:00Z",
            policy_receipt_id: "policy:abc",
            provider_claim: Some("provider called this relevant"),
            captured_at_seq: 7,
        },
        selector,
    )
    .unwrap()
}

#[test]
fn quote_reloads_and_renders_identically_after_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("evidence.sqlite");
    let (capture, rep) = fixture("Hello café 🦀");
    let anchor = quote_anchor(&capture, &rep);
    let expected = [
        CitationFormat::Plain,
        CitationFormat::Markdown,
        CitationFormat::Lisp,
        CitationFormat::Json,
    ]
    .map(|f| anchor.render(f).unwrap());
    {
        let mut store = DocStore::create(&path).unwrap();
        save_capture(&mut store, &capture).unwrap();
        save_representation(&mut store, &rep).unwrap();
        save_anchor(&mut store, &anchor).unwrap();
    }
    let store = DocStore::create(&path).unwrap();
    let loaded = load_anchor(&store, "anchor-1").unwrap().unwrap();
    let actual = [
        CitationFormat::Plain,
        CitationFormat::Markdown,
        CitationFormat::Lisp,
        CitationFormat::Json,
    ]
    .map(|f| loaded.render(f).unwrap());
    assert_eq!(actual, expected);
    assert!(actual[3].contains("provider_claim"));
    assert!(actual[0].contains("script omitted"));
}

#[test]
fn one_code_point_tamper_and_changed_representation_id_fail_closed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("evidence.sqlite");
    let (capture, rep) = fixture("Hello café 🦀");
    let anchor = quote_anchor(&capture, &rep);
    {
        let mut store = DocStore::create(&path).unwrap();
        save_capture(&mut store, &capture).unwrap();
        save_representation(&mut store, &rep).unwrap();
        save_anchor(&mut store, &anchor).unwrap();
    }
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute("UPDATE web_representations SET text='Hello cafe 🦀'", [])
            .unwrap();
    }
    assert!(load_anchor(&DocStore::create(&path).unwrap(), "anchor-1").is_err());
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        db.execute(
            "UPDATE web_representations SET text=?1, representation_id='wrong'",
            [&rep.text],
        )
        .unwrap();
        db.execute(
            "UPDATE web_evidence_anchors SET representation_id='wrong'",
            [],
        )
        .unwrap();
    }
    assert!(load_anchor(&DocStore::create(&path).unwrap(), "anchor-1").is_err());
}

#[test]
fn raw_capture_tamper_fails_anchor_load_closed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("capture-tamper.sqlite");
    let (capture, rep) = fixture("Hello café 🦀");
    let anchor = quote_anchor(&capture, &rep);
    let mut store = DocStore::create(&path).unwrap();
    save_capture(&mut store, &capture).unwrap();
    save_representation(&mut store, &rep).unwrap();
    save_anchor(&mut store, &anchor).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE web_captures SET body=X'00'", [])
        .unwrap();
    drop(db);
    assert!(load_anchor(&DocStore::create(&path).unwrap(), "anchor-1").is_err());
}

#[test]
fn unicode_normalization_codec_upgrade_deleted_index_and_duplicate_source_are_safe() {
    let (capture, rep) = fixture("Hello café 🦀");
    assert!(matches!(
        EvidenceSelector::checked(
            rep.content_id.clone(),
            6,
            10,
            "cafe\u{301}".into(),
            &rep.text
        ),
        Err(WebRecordError::InvalidSelector)
    ));
    let dir = tempdir().unwrap();
    let path = dir.path().join("e.sqlite");
    let mut store = DocStore::create(&path).unwrap();
    save_capture(&mut store, &capture).unwrap();
    save_capture(&mut store, &capture).unwrap();
    save_representation(&mut store, &rep).unwrap();
    let anchor = quote_anchor(&capture, &rep);
    save_anchor(&mut store, &anchor).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE web_representations SET metadata_json=replace(metadata_json,'\"codec_version\":\"1\"','\"codec_version\":\"2\"')",[]).unwrap();
    drop(db);
    assert!(load_anchor(&DocStore::create(&path).unwrap(), "anchor-1").is_err());
    let deleted = tempdir().unwrap();
    let deleted_path = deleted.path().join("e.sqlite");
    let mut store = DocStore::create(&deleted_path).unwrap();
    save_capture(&mut store, &capture).unwrap();
    save_representation(&mut store, &rep).unwrap();
    save_anchor(&mut store, &anchor).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&deleted_path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM web_representations;")
        .unwrap();
    drop(db);
    assert!(load_anchor(&DocStore::create(&deleted_path).unwrap(), "anchor-1").is_err());
}

#[test]
fn projection_retains_source_identity_and_structural_path() {
    let (capture, rep) = fixture("Hello café 🦀");
    let anchor = quote_anchor(&capture, &rep);
    let mut cx = Cx::new(
        Arc::new(NoopEvalPolicy),
        Arc::new(DefaultFactory),
        HandleSeed::new(0x5745_4254),
    );
    let doc = project_document(&mut cx, &anchor, &rep).unwrap();
    assert_eq!(doc.origin[0], anchor.evidence.evidence);
    let expr = doc.body.object().as_expr(&mut cx).unwrap();
    let markup = MarkupDoc::from_expr(&expr).unwrap();
    assert_eq!(
        markup.attrs["web/representation-id"],
        sim_kernel::Expr::String(cid_text(&rep.content_id))
    );
    assert!(
        matches!(&markup.attrs["web/structural-path"],sim_kernel::Expr::Vector(v) if v.len()==3)
    );
}

#[test]
fn constructors_exclude_snippets_and_distinguish_reference_kinds() {
    let (capture, rep) = fixture("Hello café 🦀");
    let subject = DocId::new("doc");
    let input = || AnchorInput {
        anchor_id: "a",
        subject: &subject,
        capture: &capture,
        representation: &rep,
        source_uri: capture.retrieval_uri.as_str(),
        source_title: "Example",
        retrieved_at: "2026-08-24T12:00:00Z",
        policy_receipt_id: "p",
        provider_claim: None,
        captured_at_seq: 1,
    };
    let selector = rep.select(6, 10).unwrap();
    assert_eq!(
        EvidenceAnchor::paraphrase_support(input(), selector)
            .unwrap()
            .kind,
        AnchorKind::ParaphraseSupport
    );
    assert_eq!(
        EvidenceAnchor::whole_document(input()).unwrap().kind,
        AnchorKind::WholeDocument
    );
}
