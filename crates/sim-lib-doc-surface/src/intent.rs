//! Intent decoding for suite document panes.

use sim_kernel::{Cx, Expr, Symbol, Value};
use sim_lib_doc_core::{Doc, DocId, Edit, OfficeError};
use sim_lib_intent::{field, intent_kind_of, validate_intent};

/// Domain namespace used for suite cell edits.
pub const CELL_EDIT_DOMAIN: &str = "office/cell";
/// Path segment that marks a cell edit intent.
pub const CELL_PATH_SEGMENT: &str = "cell";
/// Operation kind emitted when a cell value is set.
pub const OP_SET_CELL: &str = "set-cell";
/// Inverse operation kind emitted when the prior cell value is unknown.
pub const OP_RESTORE_CELL: &str = "restore-cell";

/// Decode a validated suite Intent into an open document edit.
pub fn decode_suite_intent(cx: &mut Cx, docs: &[Doc], intent: Value) -> Result<Edit, OfficeError> {
    let intent = intent.object().as_expr(cx).map_err(OfficeError::from)?;
    validate_intent(&intent)
        .map_err(|error| OfficeError::Surface(format!("invalid suite intent: {error}")))?;
    let kind = intent_kind_of(&intent)
        .ok_or_else(|| OfficeError::Surface("suite intent has no kind".to_owned()))?;
    if kind.name.as_ref() != "edit-field" {
        return Err(OfficeError::Surface(format!(
            "unsupported suite intent kind {kind}"
        )));
    }
    let doc_id = target_doc(&intent)?;
    if !docs.iter().any(|doc| doc.id == doc_id) {
        return Err(OfficeError::Surface(format!(
            "suite intent targets unknown doc {}",
            doc_id.0
        )));
    }
    let path = field(&intent, "path")
        .ok_or_else(|| OfficeError::Surface("suite edit intent has no path".to_owned()))?;
    let cell = cell_path(path)?;
    let value = field(&intent, "value")
        .ok_or_else(|| OfficeError::Surface("suite cell intent has no value".to_owned()))?
        .clone();
    let op = cx.factory().expr(cell_op(OP_SET_CELL, &cell, value))?;
    let inverse = cx
        .factory()
        .expr(cell_op(OP_RESTORE_CELL, &cell, Expr::Nil))?;
    Ok(Edit::new(doc_id, CELL_EDIT_DOMAIN, op, inverse))
}

fn target_doc(intent: &Expr) -> Result<DocId, OfficeError> {
    let target = field(intent, "target")
        .ok_or_else(|| OfficeError::Surface("suite intent has no target".to_owned()))?;
    match target {
        Expr::String(id) => Ok(DocId::new(id.clone())),
        Expr::Symbol(symbol) => Ok(DocId::new(symbol.name.to_string())),
        other => Err(OfficeError::Surface(format!(
            "suite intent target must be a document id, got {other:?}"
        ))),
    }
}

fn cell_path(path: &Expr) -> Result<String, OfficeError> {
    let Expr::List(segments) = path else {
        return Err(OfficeError::Surface(
            "suite cell intent path must be a list".to_owned(),
        ));
    };
    let Some(first) = segments.first() else {
        return Err(OfficeError::Surface(
            "suite cell intent path is empty".to_owned(),
        ));
    };
    if segment(first).as_deref() != Some(CELL_PATH_SEGMENT) {
        return Err(OfficeError::Surface(
            "suite edit path is not a cell path".to_owned(),
        ));
    }
    let Some(cell) = segments.get(1).and_then(segment) else {
        return Err(OfficeError::Surface(
            "suite cell intent path has no cell reference".to_owned(),
        ));
    };
    Ok(cell)
}

fn segment(expr: &Expr) -> Option<String> {
    match expr {
        Expr::String(text) => Some(text.clone()),
        Expr::Symbol(symbol) => Some(symbol.name.to_string()),
        _ => None,
    }
}

fn cell_op(kind: &str, cell: &str, value: Expr) -> Expr {
    Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("kind")),
            Expr::Symbol(Symbol::qualified("office", kind)),
        ),
        (
            Expr::Symbol(Symbol::new(CELL_PATH_SEGMENT)),
            Expr::String(cell.to_owned()),
        ),
        (Expr::Symbol(Symbol::new("value")), value),
    ])
}

#[cfg(test)]
mod tests {
    use sim_kernel::testing::bare_cx as cx;
    use sim_lib_doc_core::{DocKind, SurfaceCaps};
    use sim_lib_intent::{Origin, intent};

    use super::*;
    use crate::{SuitePane, suite_scene};

    fn doc(cx: &mut Cx) -> Doc {
        Doc::new(
            DocKind::new("sheet"),
            DocId::new("sheet-1"),
            cx.factory().string("sheet body".to_owned()).unwrap(),
            vec![],
        )
    }

    fn field_expr<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
        let Expr::Map(entries) = expr else {
            return None;
        };
        entries.iter().find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
    }

    #[test]
    fn cell_intent_becomes_set_cell_edit() {
        let mut cx = cx();
        let docs = vec![doc(&mut cx)];
        let intent = intent(
            "edit-field",
            Origin::human(7),
            vec![
                ("target", Expr::String("sheet-1".to_owned())),
                (
                    "path",
                    Expr::List(vec![
                        Expr::String("cell".to_owned()),
                        Expr::String("B2".to_owned()),
                    ]),
                ),
                ("value", Expr::String("42".to_owned())),
            ],
        );
        let value = cx.factory().expr(intent).unwrap();

        let edit = decode_suite_intent(&mut cx, &docs, value).unwrap();
        let op = edit.op.object().as_expr(&mut cx).unwrap();

        assert_eq!(edit.doc, DocId::new("sheet-1"));
        assert_eq!(edit.domain, CELL_EDIT_DOMAIN);
        assert_eq!(
            field_expr(&op, "kind"),
            Some(&Expr::Symbol(Symbol::qualified("office", OP_SET_CELL)))
        );
        assert_eq!(
            field_expr(&op, "cell"),
            Some(&Expr::String("B2".to_owned()))
        );
        assert_eq!(
            field_expr(&op, "value"),
            Some(&Expr::String("42".to_owned()))
        );
    }

    #[test]
    fn non_cell_intent_fails_closed() {
        let mut cx = cx();
        let docs = vec![doc(&mut cx)];
        let intent = intent(
            "edit-field",
            Origin::human(7),
            vec![
                ("target", Expr::String("sheet-1".to_owned())),
                ("path", Expr::List(vec![Expr::String("title".to_owned())])),
                ("value", Expr::String("42".to_owned())),
            ],
        );
        let value = cx.factory().expr(intent).unwrap();

        let err = decode_suite_intent(&mut cx, &docs, value).unwrap_err();

        assert!(err.to_string().contains("not a cell path"));
    }

    #[test]
    fn decoded_edits_pair_with_projected_panes() {
        let mut cx = cx();
        let docs = vec![doc(&mut cx)];
        let panes = vec![SuitePane::new(
            DocId::new("sheet-1"),
            SurfaceCaps::new().target("screen"),
        )];

        let scene = suite_scene(&mut cx, &panes, &docs).unwrap();

        sim_lib_scene::validate_scene(&scene.object().as_expr(&mut cx).unwrap()).unwrap();
    }
}
