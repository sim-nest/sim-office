//! Suite surface helpers for markup-backed article documents.

use sim_codec_doc::{MarkupDoc, MarkupEdit, apply_edit, invert_edit};
use sim_kernel::{Cx, Expr, Value};
use sim_lib_doc_core::{Doc, Edit, OfficeError, SurfaceCaps, TAG_BACKEND, TAG_LENS};
use sim_lib_scene::{node, sym, validate_scene};

use crate::{MARKUP_DOC_KIND, preferred_backend};

/// Domain namespace used by open office edits carrying `MarkupEdit` payloads.
pub const MARKUP_EDIT_DOMAIN: &str = "office/markup";

const LENS_SOURCE: &str = "source";
const LENS_FORMATTED: &str = "formatted";

/// Project a markup article document into the suite scene surface.
pub fn markup_suite_scene(
    cx: &mut Cx,
    doc: &Doc,
    caps: &SurfaceCaps,
) -> Result<Value, OfficeError> {
    ensure_article(doc)?;
    let article = article_expr(cx, doc)?;
    let lens = caps.get(TAG_LENS).unwrap_or(LENS_FORMATTED);
    let article_scene = match lens {
        LENS_SOURCE => sim_lib_view_doc::article_source(&article),
        LENS_FORMATTED => sim_lib_view_doc::article_formatted(&article),
        other => {
            return Err(OfficeError::Surface(format!(
                "unsupported markup article lens {other}"
            )));
        }
    };
    let scene = node(
        "box",
        vec![
            ("id", Expr::String(doc.id.as_str().to_owned())),
            ("role", sym("markup-suite-pane")),
            (
                "backend",
                Expr::String(
                    caps.get(TAG_BACKEND)
                        .map(str::to_owned)
                        .or_else(|| preferred_backend(doc).map(|id| id.as_str().to_owned()))
                        .unwrap_or_else(|| "unknown".to_owned()),
                ),
            ),
            (
                "children",
                Expr::List(vec![
                    sim_lib_scene::badge("lens", lens),
                    node(
                        "embed",
                        vec![
                            (
                                "lens",
                                sym(if lens == LENS_SOURCE {
                                    sim_lib_view_doc::ARTICLE_SOURCE_LENS
                                } else {
                                    sim_lib_view_doc::ARTICLE_FORMATTED_LENS
                                }),
                            ),
                            ("scene", article_scene),
                        ],
                    ),
                ]),
            ),
        ],
    );
    validate_scene(&scene)
        .map_err(|error| OfficeError::Surface(format!("markup scene did not validate: {error}")))?;
    cx.factory().expr(scene).map_err(OfficeError::from)
}

/// Decode a suite article intent into an open OFFICE edit carrying `MarkupEdit`.
pub fn decode_markup_suite_intent(
    cx: &mut Cx,
    doc: &Doc,
    intent: Value,
) -> Result<Edit, OfficeError> {
    ensure_article(doc)?;
    let intent = intent.object().as_expr(cx).map_err(OfficeError::from)?;
    sim_lib_intent::validate_intent(&intent)
        .map_err(|error| OfficeError::Surface(format!("invalid markup intent: {error}")))?;
    let article = article_expr(cx, doc)?;
    let edit = sim_lib_view_doc::markup_edit_from_intent(&article, &intent)
        .map_err(|error| OfficeError::Surface(error.to_string()))?;
    let inverse = invert_edit(&edit);
    let op = cx
        .factory()
        .expr(edit.as_expr())
        .map_err(OfficeError::from)?;
    let inverse = cx
        .factory()
        .expr(inverse.as_expr())
        .map_err(OfficeError::from)?;
    Ok(Edit::new(doc.id.clone(), MARKUP_EDIT_DOMAIN, op, inverse))
}

/// Apply one open office markup edit to a document body.
pub fn apply_markup_edit(cx: &mut Cx, doc: &mut Doc, edit: &Edit) -> Result<(), OfficeError> {
    ensure_article(doc)?;
    if edit.domain != MARKUP_EDIT_DOMAIN {
        return Err(OfficeError::DomainEdit(format!(
            "expected {MARKUP_EDIT_DOMAIN} edit, got {}",
            edit.domain
        )));
    }
    if edit.doc != doc.id {
        return Err(OfficeError::DomainEdit(format!(
            "edit targets {}, not {}",
            edit.doc.as_str(),
            doc.id.as_str()
        )));
    }
    let expr = doc.body.object().as_expr(cx).map_err(OfficeError::from)?;
    let mut markup =
        MarkupDoc::from_expr(&expr).map_err(|error| OfficeError::DomainEdit(error.to_string()))?;
    let op = edit.op.object().as_expr(cx).map_err(OfficeError::from)?;
    let op =
        MarkupEdit::from_expr(&op).map_err(|error| OfficeError::DomainEdit(error.to_string()))?;
    apply_edit(&mut markup, &op).map_err(|error| OfficeError::DomainEdit(error.to_string()))?;
    doc.body = cx
        .factory()
        .expr(markup.as_expr())
        .map_err(OfficeError::from)?;
    Ok(())
}

fn ensure_article(doc: &Doc) -> Result<(), OfficeError> {
    if doc.kind.as_str() == MARKUP_DOC_KIND {
        Ok(())
    } else {
        Err(OfficeError::Surface(format!(
            "markup surface handles only {MARKUP_DOC_KIND} documents, got {}",
            doc.kind.as_str()
        )))
    }
}

fn article_expr(cx: &mut Cx, doc: &Doc) -> Result<Expr, OfficeError> {
    let expr = doc.body.object().as_expr(cx).map_err(OfficeError::from)?;
    MarkupDoc::from_expr(&expr)
        .map(|markup| sim_lib_view_doc::article_from_markup(&markup))
        .map_err(|error| OfficeError::Surface(format!("document body is not markup data: {error}")))
}
