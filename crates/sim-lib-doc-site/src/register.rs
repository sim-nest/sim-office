//! Registration and modeled realize calls for document sites.

use std::sync::Arc;

use sim_kernel::{
    CORE_LOCAL_EVAL_FABRIC_CLASS_ID, ClassRef, Cx, Datum, Error, EvalFabric, EvalReply,
    EvalRequest, ExportKind, ExportRecord, ExportState, Expr, Object, ObjectCompat,
    Result as KernelResult, RuntimeId, Symbol, Value,
};
use sim_lib_doc_core::{DocId, DocKind, DocSite, Edit, OfficeError};

/// Domain string used for preview edits emitted by document sites.
pub const DOC_SITE_DOMAIN: &str = "office/doc-site";

/// Register a document site as an opaque kernel `site` export.
pub fn register_site(cx: &mut Cx, site: &DocSite) -> Result<ExportRecord, OfficeError> {
    let symbol = site_symbol(&site.site_id);
    if let Some(site_id) = cx.registry().sites().get(&symbol).copied() {
        return Ok(site_export_record(symbol, RuntimeId::Site(site_id)));
    }

    let runtime = cx
        .factory()
        .opaque(Arc::new(DocSiteRuntime::new(site.clone())))?;
    let id = cx
        .registry_mut()
        .register_site_value(symbol.clone(), runtime)?;
    Ok(site_export_record(symbol, id))
}

/// Resolve a registered document site and run one modeled or capability-gated operation.
pub fn realize_site_op(cx: &mut Cx, site_id: &str, op: SiteOp) -> Result<SiteReply, OfficeError> {
    let symbol = site_symbol(site_id);
    let value = cx
        .registry()
        .site_by_symbol(&symbol)
        .cloned()
        .ok_or_else(|| OfficeError::Site(format!("site {site_id} is not registered")))?;
    let runtime = value
        .object()
        .downcast_ref::<DocSiteRuntime>()
        .ok_or_else(|| OfficeError::Site(format!("site {site_id} has incompatible runtime")))?;
    runtime.run(cx, op)
}

/// Build the registry symbol used for a document site id.
#[must_use]
pub fn site_symbol(site_id: &str) -> Symbol {
    Symbol::new(site_id.to_owned())
}

fn site_export_record(symbol: Symbol, id: RuntimeId) -> ExportRecord {
    ExportRecord {
        kind: ExportKind::named(ExportKind::SITE),
        symbol,
        state: ExportState::Resolved { id },
    }
}

/// Operation submitted to a document site.
#[derive(Clone, Debug, PartialEq)]
pub enum SiteOp {
    /// Read a modeled path.
    Read {
        /// Site-local path.
        path: String,
    },
    /// Preview a write. The operation returns an edit and never mutates a live backend.
    PreviewWrite {
        /// Site-local path.
        path: String,
        /// Body proposed for the write.
        body: Value,
    },
}

/// Reply returned by a document site operation.
#[derive(Clone, Debug, PartialEq)]
pub enum SiteReply {
    /// Modeled data for a read.
    Data(Value),
    /// Preview edit for a write.
    Preview(Edit),
}

/// Runtime object registered as the opaque kernel site value.
#[derive(Clone, Debug, PartialEq)]
pub struct DocSiteRuntime {
    site: DocSite,
}

impl DocSiteRuntime {
    /// Build a runtime object for a document site.
    #[must_use]
    pub fn new(site: DocSite) -> Self {
        Self { site }
    }

    /// Borrow the site descriptor carried by this runtime.
    #[must_use]
    pub fn site(&self) -> &DocSite {
        &self.site
    }

    /// Run one site operation.
    pub fn run(&self, cx: &mut Cx, op: SiteOp) -> Result<SiteReply, OfficeError> {
        self.site.authorize(cx)?;
        match op {
            SiteOp::Read { path } => modeled_read(cx, &self.site, &path).map(SiteReply::Data),
            SiteOp::PreviewWrite { path, body } => {
                preview_write(cx, &self.site, path, body).map(SiteReply::Preview)
            }
        }
    }

    fn metadata_expr(&self) -> Expr {
        Expr::Map(vec![
            symbol_field(
                "kind",
                Expr::Symbol(Symbol::qualified("office", "doc-site")),
            ),
            symbol_field("site-id", Expr::String(self.site.site_id.clone())),
            symbol_field(
                "kinds",
                Expr::List(
                    self.site
                        .kinds
                        .iter()
                        .map(|kind| Expr::String(kind.as_str().to_owned()))
                        .collect(),
                ),
            ),
            symbol_field(
                "required-caps",
                Expr::List(
                    self.site
                        .required_caps
                        .iter()
                        .map(|capability| Expr::String(capability.as_str().to_owned()))
                        .collect(),
                ),
            ),
            symbol_field("modeled", Expr::Bool(self.site.default_modeled)),
        ])
    }

    fn metadata_datum(&self) -> Datum {
        Datum::Map(vec![
            datum_field(
                "kind",
                Datum::Symbol(Symbol::qualified("office", "doc-site")),
            ),
            datum_field("site-id", Datum::String(self.site.site_id.clone())),
            datum_field(
                "kinds",
                Datum::List(
                    self.site
                        .kinds
                        .iter()
                        .map(|kind| Datum::String(kind.as_str().to_owned()))
                        .collect(),
                ),
            ),
            datum_field(
                "required-caps",
                Datum::List(
                    self.site
                        .required_caps
                        .iter()
                        .map(|capability| Datum::String(capability.as_str().to_owned()))
                        .collect(),
                ),
            ),
            datum_field("modeled", Datum::Bool(self.site.default_modeled)),
        ])
    }
}

impl Object for DocSiteRuntime {
    fn display(&self, _cx: &mut Cx) -> KernelResult<String> {
        Ok(format!("#<doc-site {}>", self.site.site_id))
    }

    fn snapshot(&self, _cx: &mut Cx) -> KernelResult<Option<Datum>> {
        Ok(Some(self.metadata_datum()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for DocSiteRuntime {
    fn class(&self, cx: &mut Cx) -> KernelResult<ClassRef> {
        cx.factory().class_stub(
            CORE_LOCAL_EVAL_FABRIC_CLASS_ID,
            Symbol::qualified("core", "LocalEvalFabric"),
        )
    }

    fn as_expr(&self, _cx: &mut Cx) -> KernelResult<Expr> {
        Ok(self.metadata_expr())
    }

    fn as_eval_fabric(&self) -> Option<&dyn EvalFabric> {
        Some(self)
    }
}

impl EvalFabric for DocSiteRuntime {
    fn realize(&self, cx: &mut Cx, request: EvalRequest) -> KernelResult<EvalReply> {
        let op = site_op_from_expr(cx, request.expr)?;
        let reply = self
            .run(cx, op)
            .map_err(|err| Error::Eval(err.to_string()))?;
        let value = site_reply_value(cx, reply).map_err(|err| Error::Eval(err.to_string()))?;
        Ok(EvalReply {
            value,
            diagnostics: Vec::new(),
            trace: None,
        })
    }
}

fn modeled_read(cx: &mut Cx, site: &DocSite, path: &str) -> Result<Value, OfficeError> {
    let kinds = kinds_value(cx, &site.kinds)?;
    let required_caps = capabilities_value(cx, &site.required_caps)?;
    cx.factory()
        .table(vec![
            (
                Symbol::new("kind"),
                cx.factory()
                    .symbol(Symbol::qualified("office", "doc-site-cassette"))?,
            ),
            (
                Symbol::new("site-id"),
                cx.factory().string(site.site_id.clone())?,
            ),
            (Symbol::new("path"), cx.factory().string(path.to_owned())?),
            (
                Symbol::new("modeled"),
                cx.factory().bool(site.default_modeled)?,
            ),
            (Symbol::new("kinds"), kinds),
            (Symbol::new("required-caps"), required_caps),
        ])
        .map_err(OfficeError::from)
}

fn preview_write(
    cx: &mut Cx,
    site: &DocSite,
    path: String,
    body: Value,
) -> Result<Edit, OfficeError> {
    let op = cx.factory().table(vec![
        (
            Symbol::new("kind"),
            cx.factory()
                .symbol(Symbol::qualified("office", "doc-site-preview-write"))?,
        ),
        (
            Symbol::new("site-id"),
            cx.factory().string(site.site_id.clone())?,
        ),
        (Symbol::new("path"), cx.factory().string(path.clone())?),
        (Symbol::new("body"), body),
    ])?;
    let inverse = cx.factory().table(vec![
        (
            Symbol::new("kind"),
            cx.factory()
                .symbol(Symbol::qualified("office", "doc-site-preview-cancel"))?,
        ),
        (
            Symbol::new("site-id"),
            cx.factory().string(site.site_id.clone())?,
        ),
        (Symbol::new("path"), cx.factory().string(path.clone())?),
    ])?;
    Ok(Edit::new(
        DocId::new(format!("{}:{path}", site.site_id)),
        DOC_SITE_DOMAIN,
        op,
        inverse,
    ))
}

fn kinds_value(cx: &mut Cx, kinds: &[DocKind]) -> KernelResult<Value> {
    let values = kinds
        .iter()
        .map(|kind| cx.factory().string(kind.as_str().to_owned()))
        .collect::<KernelResult<Vec<_>>>()?;
    cx.factory().list(values)
}

fn capabilities_value(
    cx: &mut Cx,
    capabilities: &[sim_kernel::CapabilityName],
) -> KernelResult<Value> {
    let values = capabilities
        .iter()
        .map(|capability| cx.factory().string(capability.as_str().to_owned()))
        .collect::<KernelResult<Vec<_>>>()?;
    cx.factory().list(values)
}

fn site_reply_value(cx: &mut Cx, reply: SiteReply) -> Result<Value, OfficeError> {
    match reply {
        SiteReply::Data(value) => Ok(value),
        SiteReply::Preview(edit) => cx
            .factory()
            .table(vec![
                (
                    Symbol::new("kind"),
                    cx.factory()
                        .symbol(Symbol::qualified("office", "doc-site-preview"))?,
                ),
                (Symbol::new("doc"), cx.factory().string(edit.doc.0.clone())?),
                (
                    Symbol::new("domain"),
                    cx.factory().string(edit.domain.clone())?,
                ),
                (Symbol::new("op"), edit.op),
                (Symbol::new("inverse"), edit.inverse),
            ])
            .map_err(OfficeError::from),
    }
}

fn site_op_from_expr(cx: &mut Cx, expr: Expr) -> KernelResult<SiteOp> {
    let entries = match expr {
        Expr::Map(entries) => entries,
        _ => {
            return Err(Error::TypeMismatch {
                expected: "site op map",
                found: "non-map",
            });
        }
    };
    let op = string_field(&entries, "op")?;
    let path = string_field(&entries, "path")?;
    match op.as_str() {
        "read" => Ok(SiteOp::Read { path }),
        "preview-write" => {
            let body_expr = expr_field(&entries, "body").cloned().unwrap_or(Expr::Nil);
            Ok(SiteOp::PreviewWrite {
                path,
                body: cx.factory().expr(body_expr)?,
            })
        }
        other => Err(Error::Eval(format!("unsupported document site op {other}"))),
    }
}

fn string_field(entries: &[(Expr, Expr)], name: &str) -> KernelResult<String> {
    match expr_field(entries, name) {
        Some(Expr::String(value)) => Ok(value.clone()),
        Some(_) => Err(Error::TypeMismatch {
            expected: "string field",
            found: "non-string",
        }),
        None => Err(Error::Eval(format!("missing document site field {name}"))),
    }
}

fn expr_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Option<&'a Expr> {
    entries.iter().find_map(|(key, value)| match key {
        Expr::Symbol(symbol) if symbol.name.as_ref() == name && symbol.namespace.is_none() => {
            Some(value)
        }
        Expr::String(text) if text == name => Some(value),
        _ => None,
    })
}

fn symbol_field(name: &str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

fn datum_field(name: &str, value: Datum) -> (Datum, Datum) {
    (Datum::Symbol(Symbol::new(name)), value)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{
        CapabilityName, Consistency, DefaultFactory, EvalMode, NoopEvalPolicy, RuntimeId,
    };
    use sim_lib_doc_core::{NET_CONNECT_CAPABILITY, invert};

    use super::*;

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
                        symbol_field("op", Expr::String("read".to_owned())),
                        symbol_field("path", Expr::String("/cassette".to_owned())),
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
}
