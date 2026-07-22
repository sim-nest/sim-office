use std::sync::Arc;

use sim_kernel::{
    CapabilityName, Consistency, Cx, DefaultFactory, EvalMode, EvalRequest, ExportKind,
    ExportState, Expr, NoopEvalPolicy, RuntimeId, Symbol,
};
use sim_lib_doc_core::{DocKind, DocSite, NET_CONNECT_CAPABILITY, OfficeError, invert};
use sim_value::build::entry;

use crate::{DOC_SITE_DOMAIN, SiteOp, SiteReply, realize_site_op, register_site, site_symbol};

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn modeled_site() -> DocSite {
    DocSite::new(
        "site/msgraph",
        vec![DocKind::new("sheet")],
        vec![CapabilityName::new(NET_CONNECT_CAPABILITY)],
        true,
    )
}

fn live_site() -> DocSite {
    DocSite::new(
        "site/msgraph-live",
        vec![DocKind::new("sheet")],
        vec![CapabilityName::new(NET_CONNECT_CAPABILITY)],
        false,
    )
}

#[test]
fn modeled_site_registers_as_export_site_and_realizes_read() {
    let mut cx = cx();
    let record = register_site(&mut cx, &modeled_site()).unwrap();

    assert_eq!(record.kind, ExportKind::named(ExportKind::SITE));
    assert_eq!(record.symbol, site_symbol("site/msgraph"));
    assert!(matches!(
        record.state,
        ExportState::Resolved {
            id: RuntimeId::Site(_)
        }
    ));
    let value = cx
        .registry()
        .site_by_symbol(&site_symbol("site/msgraph"))
        .unwrap()
        .clone();
    assert!(value.object().as_eval_fabric().is_some());

    let reply = realize_site_op(
        &mut cx,
        "site/msgraph",
        SiteOp::Read {
            path: "/workbook".to_owned(),
        },
    )
    .unwrap();

    let SiteReply::Data(data) = reply else {
        panic!("read should return data");
    };
    let expr = data.object().as_expr(&mut cx).unwrap();
    assert_eq!(map_string(&expr, "path").as_deref(), Some("/workbook"));
}

#[test]
fn eval_fabric_read_uses_registered_runtime() {
    let mut cx = cx();
    register_site(&mut cx, &modeled_site()).unwrap();
    let value = cx
        .registry()
        .site_by_symbol(&site_symbol("site/msgraph"))
        .unwrap()
        .clone();
    let fabric = value.object().as_eval_fabric().unwrap();
    let reply = fabric
        .realize(
            &mut cx,
            EvalRequest {
                expr: Expr::Map(vec![
                    entry("op", Expr::String("read".to_owned())),
                    entry("path", Expr::String("/cassette".to_owned())),
                ]),
                result_shape: None,
                required_capabilities: Vec::new(),
                deadline: None,
                consistency: Consistency::LocalFirst,
                mode: EvalMode::Eval,
                answer_limit: None,
                stream_buffer: None,
                stream: false,
                trace: false,
            },
        )
        .unwrap();

    let expr = reply.value.object().as_expr(&mut cx).unwrap();
    assert_eq!(map_string(&expr, "path").as_deref(), Some("/cassette"));
}

#[test]
fn live_site_read_is_denied_without_required_caps() {
    let mut cx = cx();
    register_site(&mut cx, &live_site()).unwrap();

    let denied = realize_site_op(
        &mut cx,
        "site/msgraph-live",
        SiteOp::Read {
            path: "/workbook".to_owned(),
        },
    )
    .unwrap_err();

    assert!(matches!(
        denied,
        OfficeError::CapabilityDenied(capability)
            if capability.as_str() == NET_CONNECT_CAPABILITY
    ));
}

#[test]
fn preview_write_returns_edit_without_mutating_backend() {
    let mut cx = cx();
    register_site(&mut cx, &modeled_site()).unwrap();
    let body = cx.factory().string("draft body".to_owned()).unwrap();

    let reply = realize_site_op(
        &mut cx,
        "site/msgraph",
        SiteOp::PreviewWrite {
            path: "/draft".to_owned(),
            body,
        },
    )
    .unwrap();

    let SiteReply::Preview(edit) = reply else {
        panic!("preview write should return an edit");
    };
    assert_eq!(edit.domain, DOC_SITE_DOMAIN);
    assert_eq!(edit.doc.as_str(), "site/msgraph:/draft");
    assert_eq!(invert(&invert(&edit)), edit);
}

#[test]
fn canonical_entry_matches_document_site_field_shape() {
    let value = Expr::String("site/msgraph".to_owned());
    assert_eq!(
        entry("site-id", value.clone()),
        (Expr::Symbol(Symbol::new("site-id")), value)
    );
}

fn map_string(expr: &Expr, name: &str) -> Option<String> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match (key, value) {
        (Expr::Symbol(symbol), Expr::String(value))
            if symbol.namespace.is_none() && symbol.name.as_ref() == name =>
        {
            Some(value.clone())
        }
        _ => None,
    })
}
