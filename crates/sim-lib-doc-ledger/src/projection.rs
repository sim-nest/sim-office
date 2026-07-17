//! Projection from ledger statements into office document models.

use sim_kernel::Cx;
use sim_lib_deck::{Deck, Slide, SlideBlock};
use sim_lib_doc_core::OfficeError;
use sim_lib_ledger_close::{FinancialStatements, StatementTable};
use sim_lib_sheet::{CellRef, CellValue, Sheet, rational_from_str};

/// Office projections for one closed-year statement set.
#[derive(Clone, Debug)]
pub struct StatementProjection {
    /// Spreadsheet view of the statements.
    pub sheet: Sheet,
    /// Presentation view of the statements.
    pub deck: Deck,
}

/// Project statements into both sheet and deck document models.
pub fn project_statements(
    cx: &mut Cx,
    statements: &FinancialStatements,
) -> Result<StatementProjection, OfficeError> {
    Ok(StatementProjection {
        sheet: statements_to_sheet(cx, statements)?,
        deck: statements_to_deck(statements)?,
    })
}

/// Project statements into a sheet with exact numeric amount cells.
pub fn statements_to_sheet(
    cx: &mut Cx,
    statements: &FinancialStatements,
) -> Result<Sheet, OfficeError> {
    let mut sheet = Sheet::new(format!("Statements {}", statements.year));
    write_table(cx, &mut sheet, 1, &statements.income_statement)?;
    let next = statements.income_statement.rows.len() as u32 + 4;
    write_table(cx, &mut sheet, next, &statements.balance_sheet)?;
    Ok(sheet)
}

/// Project statements into a deck with one slide per statement table.
pub fn statements_to_deck(statements: &FinancialStatements) -> Result<Deck, OfficeError> {
    let mut deck = Deck::new(format!("Statements {}", statements.year));
    deck.push_slide(table_slide(
        "income-statement",
        &statements.income_statement,
    )?);
    deck.push_slide(table_slide("balance-sheet", &statements.balance_sheet)?);
    Ok(deck)
}

fn write_table(
    cx: &mut Cx,
    sheet: &mut Sheet,
    start_row: u32,
    table: &StatementTable,
) -> Result<(), OfficeError> {
    set_text(sheet, 1, start_row, &table.title)?;
    set_text(sheet, 1, start_row + 1, "Label")?;
    set_text(sheet, 2, start_row + 1, "Amount minor")?;
    for (index, row) in table.rows.iter().enumerate() {
        let sheet_row = start_row + 2 + index as u32;
        set_text(sheet, 1, sheet_row, &row.label)?;
        set_number(cx, sheet, 2, sheet_row, row.amount_minor)?;
    }
    let total_row = start_row + 2 + table.rows.len() as u32;
    set_text(sheet, 1, total_row, "Total")?;
    set_number(
        cx,
        sheet,
        2,
        total_row,
        table.total_minor().map_err(office_error)?,
    )?;
    Ok(())
}

fn table_slide(id: &str, table: &StatementTable) -> Result<Slide, OfficeError> {
    let mut slide = Slide::new(id, &table.title);
    let mut rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| vec![row.label.clone(), row.amount_minor.to_string()])
        .collect();
    rows.push(vec![
        "Total".to_owned(),
        table.total_minor().map_err(office_error)?.to_string(),
    ]);
    slide.push_block(SlideBlock::Table {
        columns: vec!["Label".to_owned(), "Amount minor".to_owned()],
        rows,
    });
    Ok(slide)
}

fn set_text(sheet: &mut Sheet, column: u32, row: u32, text: &str) -> Result<(), OfficeError> {
    sheet.set_cell(
        CellRef::new(column, row).map_err(office_error)?,
        CellValue::Text(text.to_owned()),
    );
    Ok(())
}

fn set_number(
    cx: &mut Cx,
    sheet: &mut Sheet,
    column: u32,
    row: u32,
    amount_minor: i64,
) -> Result<(), OfficeError> {
    let value = rational_from_str(cx, &amount_minor.to_string()).map_err(office_error)?;
    sheet.set_cell(
        CellRef::new(column, row).map_err(office_error)?,
        CellValue::Number(value),
    );
    Ok(())
}

fn office_error(error: impl std::fmt::Display) -> OfficeError {
    OfficeError::DomainEdit(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
    use sim_lib_deck::SlideBlock;
    use sim_lib_ledger_close::{FinancialStatements, StatementNote, StatementRow, StatementTable};
    use sim_lib_sheet::{CellRef, CellValue, rational_to_canonical};

    use super::*;

    #[test]
    fn projected_statements_preserve_exact_totals() {
        let mut cx = cx();
        let statements = statements();

        let projection = project_statements(&mut cx, &statements).unwrap();

        assert_eq!(numeric_cell(&mut cx, &projection.sheet, 2, 3), "-1200/1");
        assert_eq!(numeric_cell(&mut cx, &projection.sheet, 2, 7), "1200/1");
        assert_eq!(deck_total(&projection.deck, "Income statement"), -1_200);
        assert_eq!(deck_total(&projection.deck, "Balance sheet"), 1_200);
    }

    #[test]
    fn deck_projection_returns_error_on_total_overflow() {
        let statements = overflow_statements();

        let error = statements_to_deck(&statements).unwrap_err();

        assert!(error.to_string().contains("statement total"));
    }

    fn cx() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    fn statements() -> FinancialStatements {
        FinancialStatements {
            year: 2026,
            trial_balance: Vec::new(),
            income_statement: StatementTable {
                title: "Income statement".to_owned(),
                rows: vec![StatementRow {
                    label: "SRU 3000".to_owned(),
                    amount_minor: -1_200,
                }],
            },
            balance_sheet: StatementTable {
                title: "Balance sheet".to_owned(),
                rows: vec![StatementRow {
                    label: "SRU 1000".to_owned(),
                    amount_minor: 1_200,
                }],
            },
            notes: vec![StatementNote {
                id: "basis".to_owned(),
                text: "Exact fixture".to_owned(),
            }],
        }
    }

    fn overflow_statements() -> FinancialStatements {
        FinancialStatements {
            income_statement: StatementTable {
                title: "Income statement".to_owned(),
                rows: vec![
                    StatementRow {
                        label: "max".to_owned(),
                        amount_minor: i64::MAX,
                    },
                    StatementRow {
                        label: "one".to_owned(),
                        amount_minor: 1,
                    },
                ],
            },
            ..statements()
        }
    }

    fn numeric_cell(cx: &mut Cx, sheet: &Sheet, column: u32, row: u32) -> String {
        match sheet.cell(&CellRef::new(column, row).unwrap()) {
            CellValue::Number(value) => rational_to_canonical(cx, &value).unwrap(),
            other => panic!("expected numeric cell, got {other:?}"),
        }
    }

    fn deck_total(deck: &Deck, title: &str) -> i64 {
        let slide = deck
            .slides
            .iter()
            .find(|slide| slide.title == title)
            .unwrap();
        let SlideBlock::Table { rows, .. } = &slide.blocks[0] else {
            panic!("expected table block");
        };
        rows.last().unwrap()[1].parse().unwrap()
    }
}
