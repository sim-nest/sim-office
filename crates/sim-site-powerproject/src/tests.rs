use std::sync::Arc;

use sim_kernel::{DefaultFactory, ExportKind, ExportState, NoopEvalPolicy, RuntimeId};
use sim_lib_doc_core::{CREDENTIALS_CAPABILITY, NET_CONNECT_CAPABILITY, PROCESS_SPAWN_CAPABILITY};
use sim_lib_doc_site::site_symbol;

use super::*;

fn test_context() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

#[test]
fn live_site_carries_project_capabilities() {
    let site = live_powerproject_site();
    let caps: Vec<_> = site
        .required_caps
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect();

    assert_eq!(site.site_id, POWERPROJECT_SITE_ID);
    assert!(!site.default_modeled);
    assert_eq!(
        caps,
        vec![
            PROCESS_SPAWN_CAPABILITY,
            NET_CONNECT_CAPABILITY,
            CREDENTIALS_CAPABILITY
        ]
    );
}

#[test]
fn site_registers_as_export_site() {
    let mut cx = test_context();

    let record = register_powerproject_site(&mut cx, true).unwrap();

    assert_eq!(record.kind, ExportKind::named(ExportKind::SITE));
    assert_eq!(record.symbol, site_symbol(POWERPROJECT_SITE_ID));
    assert!(matches!(
        record.state,
        ExportState::Resolved {
            id: RuntimeId::Site(_)
        }
    ));
    assert!(
        cx.registry()
            .site_by_symbol(&site_symbol(POWERPROJECT_SITE_ID))
            .is_some()
    );
}

#[test]
fn recipes_are_embedded() {
    let cards = sim_cookbook::recipes_from_embedded(RECIPES).unwrap();

    assert!(
        cards
            .iter()
            .any(|card| card.id.ends_with("powerproject-placement"))
    );
}
