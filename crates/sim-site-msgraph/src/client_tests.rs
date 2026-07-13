use std::sync::Arc;

use serde_json::json;
use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_doc_core::NET_CONNECT_CAPABILITY;

use crate::{
    Cassette, GraphError, GraphMode, StaticTokenProvider,
    client::{graph_get, graph_get_bytes, graph_post, redacted_body},
};

fn test_context() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

#[test]
fn modeled_reads_are_deterministic() {
    let mut cx = test_context();
    let mode: GraphMode<StaticTokenProvider> = GraphMode::Modeled(Cassette::with_json(
        "/me/drive/root",
        json!({ "id": "root", "name": "Documents" }),
    ));

    let first = graph_get(&mut cx, &mode, "/me/drive/root").unwrap();
    let second = graph_get(&mut cx, &mode, "/me/drive/root").unwrap();

    assert_eq!(first, second);
    assert_eq!(first["id"], "root");
}

#[test]
fn modeled_file_reads_are_deterministic() {
    let mut cx = test_context();
    let mode: GraphMode<StaticTokenProvider> = GraphMode::Modeled(Cassette::with_bytes(
        "/me/drive/items/deck-1/content",
        vec![1, 2, 3],
    ));

    let first = graph_get_bytes(&mut cx, &mode, "/me/drive/items/deck-1/content").unwrap();
    let second = graph_get_bytes(&mut cx, &mode, "/me/drive/items/deck-1/content").unwrap();

    assert_eq!(first, vec![1, 2, 3]);
    assert_eq!(first, second);
}

#[test]
fn modeled_posts_are_deterministic() {
    let mut cx = test_context();
    let mode: GraphMode<StaticTokenProvider> = GraphMode::Modeled(Cassette::with_post_json(
        "/me/messages",
        json!({ "id": "draft-1" }),
    ));

    let first = graph_post(&mut cx, &mode, "/me/messages", &json!({ "subject": "Hi" })).unwrap();
    let second = graph_post(&mut cx, &mode, "/me/messages", &json!({ "subject": "Hi" })).unwrap();

    assert_eq!(first, json!({ "id": "draft-1" }));
    assert_eq!(first, second);
}

#[test]
fn live_mode_is_denied_without_network_capability() {
    let mut cx = test_context();
    let mode = GraphMode::Live {
        base_url: "https://graph.microsoft.com/v1.0".to_owned(),
        token_provider: StaticTokenProvider::new("secret-token"),
    };

    let error = graph_get(&mut cx, &mode, "/me/drive/root").unwrap_err();

    assert!(matches!(
        error,
        GraphError::CapabilityDenied { capability }
            if capability.as_str() == NET_CONNECT_CAPABILITY
    ));
}

#[test]
fn status_errors_redact_tokens_and_long_bodies() {
    let body = format!("token=secret-token {}", "x".repeat(400));

    let redacted = redacted_body(&body, Some("secret-token"));

    assert!(redacted.contains("[redacted-token]"));
    assert!(!redacted.contains("secret-token"));
    assert!(redacted.contains("[truncated]"));
    assert!(!redacted.contains(&"x".repeat(220)));
}

#[test]
fn graph_paths_are_site_local() {
    let mut cx = test_context();
    let mode: GraphMode<StaticTokenProvider> = GraphMode::Modeled(Cassette::new());

    let error = graph_get(&mut cx, &mode, "https://example.com/me").unwrap_err();

    assert!(matches!(error, GraphError::InvalidPath { .. }));
}
