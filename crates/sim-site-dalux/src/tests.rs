use std::sync::Arc;

use serde_json::json;
use sim_kernel::{Cx, DefaultFactory, ExportKind, ExportState, NoopEvalPolicy, RuntimeId};
use sim_lib_doc_core::{CREDENTIALS_CAPABILITY, NET_CONNECT_CAPABILITY};
use sim_lib_doc_site::site_symbol;

use crate::*;

// conformance: office site workflows model site placement and document exchange.

fn test_context() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn text_at(sheet: &sim_lib_sheet::Sheet, cell: &str) -> String {
    match sheet.cell(&sim_lib_sheet::CellRef::parse(cell).unwrap()) {
        sim_lib_sheet::CellValue::Text(value) => value,
        other => panic!("expected text at {cell}, got {other:?}"),
    }
}

#[test]
fn live_site_carries_dalux_capabilities() {
    let site = live_dalux_site();
    let caps: Vec<_> = site
        .required_caps
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect();

    assert_eq!(site.site_id, DALUX_SITE_ID);
    assert!(!site.default_modeled);
    assert_eq!(caps, vec![NET_CONNECT_CAPABILITY, CREDENTIALS_CAPABILITY]);
}

#[test]
fn site_registers_as_export_site() {
    let mut cx = test_context();

    let record = register_dalux_site(&mut cx, true).unwrap();

    assert_eq!(record.kind, ExportKind::named(ExportKind::SITE));
    assert_eq!(record.symbol, site_symbol(DALUX_SITE_ID));
    assert!(matches!(
        record.state,
        ExportState::Resolved {
            id: RuntimeId::Site(_)
        }
    ));
    assert!(
        cx.registry()
            .site_by_symbol(&site_symbol(DALUX_SITE_ID))
            .is_some()
    );
}

#[test]
fn recipes_are_embedded() {
    let cards = sim_cookbook::recipes_from_embedded(RECIPES).unwrap();

    assert!(
        cards
            .iter()
            .any(|card| card.id.ends_with("dalux-modeled-items"))
    );
}

#[test]
fn company_api_key_provider_is_rejected() {
    let provider = StaticDaluxCredentialProvider::company_api_key("old-key");

    assert!(matches!(
        provider.access_token(),
        Err(DaluxError::CompanyApiKeyUnsupported)
    ));
}

#[test]
fn modeled_project_items_become_dalux_doc() {
    let mut cx = test_context();
    let client = DaluxClient::modeled(
        ModeledDalux::with_json(
            "/projects/project-1/items",
            json!({
                "items": [
                    {
                        "id": "item-1",
                        "title": "Door review",
                        "status": "open",
                        "location": "Level 2",
                        "note": "Check frame",
                        "updatedAt": "2026-07-13T10:00:00Z",
                        "webUrl": "https://example.com/dalux/items/item-1"
                    }
                ]
            }),
        ),
        StaticDaluxCredentialProvider::new("token-1"),
    );

    let doc = get_project_items(&mut cx, &client, "project-1").unwrap();
    let sheet = sim_lib_sheet::doc_to_sheet(&mut cx, &doc).unwrap();

    assert_eq!(doc.id.as_str(), "site/dalux/projects/project-1/items");
    assert_eq!(text_at(&sheet, "A2"), "item-1");
    assert_eq!(text_at(&sheet, "B2"), "Door review");
}

#[test]
fn modeled_patch_sends_only_note_field() {
    let mut cx = test_context();
    let client = DaluxClient::modeled(
        ModeledDalux::new().with_patch(
            "/items/item-1",
            json!({ "note": "Reviewed in SIM" }),
            ModeledResponse::ok(json!({
                "id": "item-1",
                "updatedAt": "2026-07-13T11:00:00Z",
                "webUrl": "https://example.com/dalux/items/item-1"
            })),
        ),
        StaticDaluxCredentialProvider::new("token-1"),
    );

    let external = patch_item_note(&mut cx, &client, "item-1", "Reviewed in SIM").unwrap();

    assert_eq!(external.backend, DALUX_SITE_ID);
    assert_eq!(external.external_id, "items/item-1");
    assert_eq!(external.version.as_deref(), Some("2026-07-13T11:00:00Z"));
}

#[test]
fn errors_redact_tokens_and_long_project_names() {
    let mut cx = test_context();
    let token = "secret-token";
    let long_name = format!("project-{}", "x".repeat(140));
    let client = DaluxClient::modeled(
        ModeledDalux::with_status(
            "/projects/project-1/items",
            403,
            json!({
                "token": token,
                "projectName": long_name,
                "message": "denied"
            }),
        ),
        StaticDaluxCredentialProvider::new(token),
    );

    let error = get_project_items(&mut cx, &client, "project-1")
        .unwrap_err()
        .to_string();

    assert!(!error.contains(token));
    assert!(!error.contains(&long_name));
    assert!(error.contains("[redacted-token]"));
    assert!(error.contains("[redacted-long-field]"));
}
