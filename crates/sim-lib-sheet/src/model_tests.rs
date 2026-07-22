use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};

use super::*;

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

#[test]
fn cell_ref_round_trips_a1_notation() {
    assert_eq!(CellRef::parse("A1").unwrap().to_string(), "A1");
    assert_eq!(CellRef::parse("AA12").unwrap().to_string(), "AA12");
    assert!(CellRef::parse("1A").is_err());
}

#[test]
fn rational_literals_are_reduced() {
    let mut cx = cx();
    let number = rational_from_str(&mut cx, "6/8").unwrap();
    assert_eq!(rational_to_canonical(&mut cx, &number).unwrap(), "3/4");
}

#[test]
fn sheet_expr_round_trips() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(
        CellRef::parse("B2").unwrap(),
        CellValue::Number(rational_from_str(&mut cx, "5/2").unwrap()),
    );
    let expr = sheet_to_expr(&mut cx, &sheet).unwrap();
    let decoded = sheet_from_expr(&mut cx, &expr).unwrap();
    let CellValue::Number(number) = decoded.cell(&CellRef::parse("B2").unwrap()) else {
        panic!("expected number");
    };
    assert_eq!(rational_to_canonical(&mut cx, &number).unwrap(), "5/2");
}
