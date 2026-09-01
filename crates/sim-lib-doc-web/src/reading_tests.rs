use super::*;
use sim_kernel::Datum;
use sim_lib_web_core::{DecodeLimits, RepresentationMetadata, WebRepresentation};

fn representation(text: &str) -> WebRepresentation {
    let raw = Datum::Bytes(text.as_bytes().to_vec()).content_id().unwrap();
    WebRepresentation::checked(
        raw,
        text.into(),
        RepresentationMetadata {
            codec: "supplied-text".into(),
            codec_version: "1".into(),
            media_type: "text/plain".into(),
            charset: Some("utf-8".into()),
            language: Some("en".into()),
            fidelity_warnings: vec![],
        },
        DecodeLimits::default(),
    )
    .unwrap()
}

fn policy(max_quote_chars: u32, permits_export: bool) -> LicencePolicy {
    LicencePolicy {
        name: "test-policy".into(),
        max_quote_chars,
        max_export_chars: max_quote_chars * 2,
        permits_export,
    }
}

#[test]
fn parallel_reading_keeps_source_witness_and_conflicting_annotations_separate() {
    let rep = representation("The sea was calm. The sea was iron.");
    let edition = EditionId("edition:parallel-1".into());
    let span = SpanId("span:calm".into());
    let mut library = SourceLibrary::default();
    library
        .import(
            edition.clone(),
            SourceLocator::PublicId {
                scheme: "catalogue".into(),
                value: "synthetic-1".into(),
            },
            &rep,
            policy(32, true),
            "2026-08-25T00:00:00Z",
            Some("2027-01-01T00:00:00Z".into()),
        )
        .unwrap();
    library
        .admit_span(&edition, span.clone(), rep.select(4, 16).unwrap(), &rep)
        .unwrap();
    for (id, layer, text) in [
        (
            "witness",
            ReadingLayer::Witness,
            "parallel witness retains ‘calm’",
        ),
        (
            "reading-a",
            ReadingLayer::Interpretation,
            "calm signals safety",
        ),
        (
            "reading-b",
            ReadingLayer::Interpretation,
            "calm conceals danger",
        ),
        (
            "reflection",
            ReadingLayer::Reflection,
            "the readings conflict",
        ),
        ("response", ReadingLayer::Response, "preserve both readings"),
        ("context", ReadingLayer::Context, "synthetic specimen"),
    ] {
        library
            .annotate(ReadingAnnotation {
                id: id.into(),
                span_id: span.clone(),
                layer,
                author: AnnotationAuthor::ModelProposal("specimen-model".into()),
                text: text.into(),
            })
            .unwrap();
    }
    let quote = library
        .quote(std::slice::from_ref(&span), "2026-08-26T00:00:00Z", true)
        .unwrap();
    assert_eq!(quote[0].text, "sea was calm");
    assert_eq!(library.edition(&edition).unwrap().text, rep.text);
}

#[test]
fn stale_source_and_over_limit_or_unadmitted_quotation_are_refused() {
    let rep = representation("0123456789 long quotation");
    let stale = EditionId("edition:stale".into());
    let limited = EditionId("edition:limited".into());
    let mut library = SourceLibrary::default();
    library
        .import(
            stale.clone(),
            SourceLocator::PrivateCollection {
                collection: "reader".into(),
                item: "stale".into(),
            },
            &rep,
            policy(20, false),
            "2025-01-01T00:00:00Z",
            Some("2026-01-01T00:00:00Z".into()),
        )
        .unwrap();
    library
        .import(
            limited.clone(),
            SourceLocator::PrivateCollection {
                collection: "reader".into(),
                item: "limited".into(),
            },
            &rep,
            policy(4, true),
            "2026-08-25T00:00:00Z",
            None,
        )
        .unwrap();
    let stale_span = SpanId("span:stale".into());
    let long_span = SpanId("span:long".into());
    library
        .admit_span(&stale, stale_span.clone(), rep.select(0, 10).unwrap(), &rep)
        .unwrap();
    library
        .admit_span(
            &limited,
            long_span.clone(),
            rep.select(0, 10).unwrap(),
            &rep,
        )
        .unwrap();
    assert!(
        library
            .quote(&[stale_span], "2026-08-25T00:00:00Z", false)
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
    assert!(
        library
            .quote(&[long_span], "2026-08-25T00:00:00Z", true)
            .unwrap_err()
            .to_string()
            .contains("ceiling")
    );
    assert!(
        library
            .quote(
                &[SpanId("provider-snippet".into())],
                "2026-08-25T00:00:00Z",
                false
            )
            .is_err()
    );
}

#[test]
fn removal_orphans_exact_annotation_and_never_reattaches_to_substitute_text() {
    let original = representation("same visible words");
    let substitute = representation("same visible words\n");
    let removed = EditionId("edition:removed".into());
    let replacement = EditionId("edition:replacement".into());
    let old_span = SpanId("span:old".into());
    let mut library = SourceLibrary::default();
    library
        .import(
            removed.clone(),
            SourceLocator::PrivateCollection {
                collection: "reader".into(),
                item: "old".into(),
            },
            &original,
            policy(64, false),
            "2026-08-25T00:00:00Z",
            None,
        )
        .unwrap();
    library
        .admit_span(
            &removed,
            old_span.clone(),
            original.select(0, 18).unwrap(),
            &original,
        )
        .unwrap();
    library
        .annotate(ReadingAnnotation {
            id: "annotation:old".into(),
            span_id: old_span.clone(),
            layer: ReadingLayer::Context,
            author: AnnotationAuthor::Human("reader".into()),
            text: "belongs only to old edition".into(),
        })
        .unwrap();
    assert!(library.remove(&removed));
    library
        .import(
            replacement.clone(),
            SourceLocator::PrivateCollection {
                collection: "reader".into(),
                item: "new".into(),
            },
            &substitute,
            policy(64, false),
            "2026-08-25T00:00:00Z",
            None,
        )
        .unwrap();
    library
        .admit_span(
            &replacement,
            SpanId("span:new".into()),
            substitute.select(0, 18).unwrap(),
            &substitute,
        )
        .unwrap();
    library.rebuild_indexes();
    assert_eq!(library.orphaned_annotations()[0].span_id, old_span);
    assert!(
        library
            .quote(&[old_span], "2026-08-25T00:00:00Z", false)
            .is_err()
    );
    assert_ne!(
        library.representation_key(&replacement),
        Some(cid_text(&original.content_id))
    );
}
