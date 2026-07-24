// conformance: sheet formulas reuse incremental dependency tracking and exact recomputation parity.

use sim_kernel::testing::bare_cx as cx;

use crate::{
    CellRef, CellValue, Sheet, SheetError, SheetFormulaEngine, eval_formula, rational_from_str,
    rational_to_canonical,
};

fn cell(text: &str) -> CellRef {
    CellRef::parse(text).unwrap()
}

fn number(cx: &mut sim_kernel::Cx, text: &str) -> CellValue {
    CellValue::Number(rational_from_str(cx, text).unwrap())
}

fn canonical(cx: &mut sim_kernel::Cx, value: CellValue) -> String {
    let CellValue::Number(value) = value else {
        panic!("expected number");
    };
    rational_to_canonical(cx, &value).unwrap()
}

#[test]
fn formula_addition_keeps_exact_rationals() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(cell("A1"), number(&mut cx, "1"));
    sheet.set_cell(cell("B1"), number(&mut cx, "5/2"));

    let value = eval_formula(&mut cx, &sheet, "=A1+B1").unwrap();

    assert_eq!(canonical(&mut cx, value), "7/2");
}

#[test]
fn incremental_engine_invalidates_formula_dependencies() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(cell("A1"), number(&mut cx, "1"));
    sheet.set_cell(cell("B1"), CellValue::Formula("=A1*2".to_owned()));
    let mut engine = SheetFormulaEngine::from_sheet(&mut cx, &sheet).unwrap();

    let value = engine.eval_formula(&mut cx, "=B1+1").unwrap();
    assert_eq!(canonical(&mut cx, value), "3/1");

    let next = number(&mut cx, "4");
    sheet.set_cell(cell("A1"), next.clone());
    engine.set_cell(&mut cx, cell("A1"), next).unwrap();

    let value = engine.eval_formula(&mut cx, "=B1+1").unwrap();
    assert_eq!(canonical(&mut cx, value), "9/1");
}

#[test]
fn cycle_error_names_repeated_cell() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(cell("A1"), CellValue::Formula("=B1+1".to_owned()));
    sheet.set_cell(cell("B1"), CellValue::Formula("=A1+1".to_owned()));
    let mut engine = SheetFormulaEngine::from_sheet(&mut cx, &sheet).unwrap();

    let err = engine.eval_formula(&mut cx, "=A1").unwrap_err();

    let SheetError::FormulaCycle(cycle_cell) = err else {
        panic!("expected formula cycle");
    };
    assert_eq!(cycle_cell, cell("A1"));
}

#[test]
fn value_cutoff_preserves_downstream_revision() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(cell("B1"), number(&mut cx, "1"));
    sheet.set_cell(cell("A1"), CellValue::Formula("=B1-B1".to_owned()));
    sheet.set_cell(cell("C1"), CellValue::Formula("=A1+1".to_owned()));
    let mut engine = SheetFormulaEngine::from_sheet(&mut cx, &sheet).unwrap();

    let value = engine.eval_formula(&mut cx, "=C1").unwrap();
    assert_eq!(canonical(&mut cx, value), "1/1");
    let c1_revision = engine.cell_memo_revision(&cell("C1")).unwrap();

    let next = number(&mut cx, "2");
    sheet.set_cell(cell("B1"), next.clone());
    engine.set_cell(&mut cx, cell("B1"), next).unwrap();

    let value = engine.eval_formula(&mut cx, "=C1").unwrap();
    assert_eq!(canonical(&mut cx, value), "1/1");
    assert_eq!(engine.cell_memo_revision(&cell("C1")), Some(c1_revision));
}

#[test]
fn missing_cell_errors_recheck_after_cell_is_added() {
    let mut cx = cx();
    let mut engine = SheetFormulaEngine::new();

    let err = engine.eval_formula(&mut cx, "=A1").unwrap_err();
    let SheetError::NonNumericCell(missing_cell) = err else {
        panic!("expected nonnumeric cell");
    };
    assert_eq!(missing_cell, cell("A1"));

    let value = number(&mut cx, "7");
    engine.set_cell(&mut cx, cell("A1"), value).unwrap();
    let value = engine.eval_formula(&mut cx, "=A1").unwrap();

    assert_eq!(canonical(&mut cx, value), "7/1");
}

#[test]
fn repeated_edits_match_full_recomputation() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(cell("A1"), number(&mut cx, "1"));
    sheet.set_cell(cell("B1"), CellValue::Formula("=A1*2".to_owned()));
    sheet.set_cell(cell("C1"), CellValue::Formula("=B1+3".to_owned()));
    let mut engine = SheetFormulaEngine::from_sheet(&mut cx, &sheet).unwrap();

    for literal in ["2", "5", "13"] {
        let next = number(&mut cx, literal);
        sheet.set_cell(cell("A1"), next.clone());
        engine.set_cell(&mut cx, cell("A1"), next).unwrap();

        let incremental_value = engine.eval_formula(&mut cx, "=C1").unwrap();
        let incremental = canonical(&mut cx, incremental_value);
        let full_value = eval_formula(&mut cx, &sheet, "=C1").unwrap();
        let full = canonical(&mut cx, full_value);
        assert_eq!(incremental, full);
    }
}
