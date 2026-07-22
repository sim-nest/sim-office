//! Registration and modeled realize calls for document sites.

use std::sync::Arc;

use sim_kernel::{
    CORE_LOCAL_EVAL_FABRIC_CLASS_ID, ClassRef, Cx, Datum, Error, EvalFabric, EvalReply,
    EvalRequest, ExportKind, ExportRecord, ExportState, Expr, Object, ObjectCompat,
    Result as KernelResult, RuntimeId, Symbol, Value,
};
use sim_lib_doc_core::{DocId, DocKind, DocSite, Edit, OfficeError};
use sim_value::build::entry;

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
            entry(
                "kind",
                Expr::Symbol(Symbol::qualified("office", "doc-site")),
            ),
            entry("site-id", Expr::String(self.site.site_id.clone())),
            entry(
                "kinds",
                Expr::List(
                    self.site
                        .kinds
                        .iter()
                        .map(|kind| Expr::String(kind.as_str().to_owned()))
                        .collect(),
                ),
            ),
            entry(
                "required-caps",
                Expr::List(
                    self.site
                        .required_caps
                        .iter()
                        .map(|capability| Expr::String(capability.as_str().to_owned()))
                        .collect(),
                ),
            ),
            entry("modeled", Expr::Bool(self.site.default_modeled)),
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

fn datum_field(name: &str, value: Datum) -> (Datum, Datum) {
    (Datum::Symbol(Symbol::new(name)), value)
}
