//! Sheet document and edit adapters.

use sim_kernel::{Cx, Expr, Symbol};
use sim_lib_doc_core::{Doc, DocId, DocKind, Edit};

use crate::model::{
    CellRef, CellValue, FIELD_CELL, FIELD_KIND, FIELD_VALUE, SHEET_DOC_KIND, Sheet, SheetError,
    cell_value_from_expr, cell_value_to_expr, expect_map, expect_string, expect_symbol, field,
    field_value, map, office_symbol, sheet_from_expr, sheet_to_expr,
};

/// Domain namespace used by open office edits carrying sheet operations.
pub const SHEET_EDIT_DOMAIN: &str = "office/sheet";

const OP_SET_CELL: &str = "set-cell";

/// Converts a sheet model into an office document.
pub fn sheet_to_doc(cx: &mut Cx, id: DocId, sheet: &Sheet) -> Result<Doc, SheetError> {
    let body_expr = sheet_to_expr(cx, sheet)?;
    let body = cx.factory().expr(body_expr)?;
    Ok(Doc::new(DocKind::new(SHEET_DOC_KIND), id, body, Vec::new()))
}

/// Decodes an office sheet document into the sheet model.
pub fn doc_to_sheet(cx: &mut Cx, doc: &Doc) -> Result<Sheet, SheetError> {
    ensure_sheet(doc)?;
    let expr = doc.body.object().as_expr(cx)?;
    sheet_from_expr(cx, &expr)
}

/// Builds an open edit that sets one cell and carries the inverse cell value.
pub fn set_cell_edit(
    cx: &mut Cx,
    doc: &Doc,
    cell: CellRef,
    value: CellValue,
) -> Result<Edit, SheetError> {
    let sheet = doc_to_sheet(cx, doc)?;
    let inverse = sheet.cell(&cell);
    let op_expr = set_cell_expr(cx, &cell, &value)?;
    let inverse_expr = set_cell_expr(cx, &cell, &inverse)?;
    let op = cx.factory().expr(op_expr)?;
    let inverse = cx.factory().expr(inverse_expr)?;
    Ok(Edit::new(doc.id.clone(), SHEET_EDIT_DOMAIN, op, inverse))
}

/// Applies one sheet edit payload to a sheet document.
pub fn apply_sheet_edit(cx: &mut Cx, doc: &mut Doc, edit: &Edit) -> Result<(), SheetError> {
    ensure_sheet(doc)?;
    if edit.domain != SHEET_EDIT_DOMAIN {
        return Err(SheetError::WrongEdit(format!(
            "expected {SHEET_EDIT_DOMAIN}, got {}",
            edit.domain
        )));
    }
    if edit.doc != doc.id {
        return Err(SheetError::WrongEdit(format!(
            "edit targets {}, not {}",
            edit.doc.as_str(),
            doc.id.as_str()
        )));
    }
    let op_expr = edit.op.object().as_expr(cx)?;
    let (cell, value) = set_cell_from_expr(cx, &op_expr)?;
    let mut sheet = doc_to_sheet(cx, doc)?;
    sheet.set_cell(cell, value);
    let body_expr = sheet_to_expr(cx, &sheet)?;
    doc.body = cx.factory().expr(body_expr)?;
    Ok(())
}

fn ensure_sheet(doc: &Doc) -> Result<(), SheetError> {
    if doc.kind.as_str() == SHEET_DOC_KIND {
        Ok(())
    } else {
        Err(SheetError::WrongDocKind(doc.kind.as_str().to_owned()))
    }
}

fn set_cell_expr(cx: &mut Cx, cell: &CellRef, value: &CellValue) -> Result<Expr, SheetError> {
    Ok(map(vec![
        field(FIELD_KIND, Expr::Symbol(office_symbol(OP_SET_CELL))),
        field(FIELD_CELL, Expr::String(cell.to_string())),
        field(FIELD_VALUE, cell_value_to_expr(cx, value)?),
    ]))
}

fn set_cell_from_expr(cx: &mut Cx, expr: &Expr) -> Result<(CellRef, CellValue), SheetError> {
    let entries = expect_map(expr)?;
    let kind = expect_symbol(field_value(entries, FIELD_KIND)?, FIELD_KIND)?;
    if kind != &Symbol::qualified("office", OP_SET_CELL) {
        return Err(SheetError::WrongEdit(format!(
            "expected office/{OP_SET_CELL}, got {}",
            kind.as_qualified_str()
        )));
    }
    let cell = CellRef::parse(expect_string(
        field_value(entries, FIELD_CELL)?,
        FIELD_CELL,
    )?)?;
    let value = cell_value_from_expr(cx, field_value(entries, FIELD_VALUE)?)?;
    Ok((cell, value))
}

#[cfg(test)]
mod tests {
    use sim_kernel::testing::bare_cx as cx;
    use sim_lib_doc_core::invert;

    use crate::{rational_from_str, rational_to_canonical};

    use super::*;

    #[test]
    fn sheet_docs_round_trip() {
        let mut cx = cx();
        let mut sheet = Sheet::new("Sheet1");
        sheet.set_cell(
            CellRef::parse("A1").unwrap(),
            CellValue::Number(rational_from_str(&mut cx, "5/2").unwrap()),
        );

        let doc = sheet_to_doc(&mut cx, DocId::new("sheet-1"), &sheet).unwrap();
        let decoded = doc_to_sheet(&mut cx, &doc).unwrap();
        let CellValue::Number(number) = decoded.cell(&CellRef::parse("A1").unwrap()) else {
            panic!("expected number");
        };

        assert_eq!(decoded.name, "Sheet1");
        assert_eq!(rational_to_canonical(&mut cx, &number).unwrap(), "5/2");
    }

    #[test]
    fn set_cell_edit_updates_one_cell_and_inverts() {
        let mut cx = cx();
        let mut sheet = Sheet::new("Sheet1");
        sheet.set_cell(
            CellRef::parse("A1").unwrap(),
            CellValue::Number(rational_from_str(&mut cx, "1").unwrap()),
        );
        sheet.set_cell(
            CellRef::parse("B1").unwrap(),
            CellValue::Number(rational_from_str(&mut cx, "2").unwrap()),
        );
        let mut doc = sheet_to_doc(&mut cx, DocId::new("sheet-1"), &sheet).unwrap();

        let replacement = CellValue::Number(rational_from_str(&mut cx, "7/2").unwrap());
        let edit =
            set_cell_edit(&mut cx, &doc, CellRef::parse("A1").unwrap(), replacement).unwrap();
        apply_sheet_edit(&mut cx, &mut doc, &edit).unwrap();

        let updated = doc_to_sheet(&mut cx, &doc).unwrap();
        let CellValue::Number(a1) = updated.cell(&CellRef::parse("A1").unwrap()) else {
            panic!("expected A1 number");
        };
        let CellValue::Number(b1) = updated.cell(&CellRef::parse("B1").unwrap()) else {
            panic!("expected B1 number");
        };
        assert_eq!(rational_to_canonical(&mut cx, &a1).unwrap(), "7/2");
        assert_eq!(rational_to_canonical(&mut cx, &b1).unwrap(), "2/1");

        apply_sheet_edit(&mut cx, &mut doc, &invert(&edit)).unwrap();
        let restored = doc_to_sheet(&mut cx, &doc).unwrap();
        let CellValue::Number(a1) = restored.cell(&CellRef::parse("A1").unwrap()) else {
            panic!("expected A1 number");
        };
        assert_eq!(rational_to_canonical(&mut cx, &a1).unwrap(), "1/1");
    }
}
