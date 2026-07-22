//! Exact spreadsheet records and expression projection.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use num_bigint::BigInt;
use sim_kernel::{Cx, Expr, NumberLiteral, Symbol};
use sim_lib_numbers_rational::{Rational, parse_rational_parts};

/// Office document kind used for sheet documents.
pub const SHEET_DOC_KIND: &str = "sheet";

pub(crate) const FIELD_CELL: &str = "cell";
pub(crate) const FIELD_CELLS: &str = "cells";
pub(crate) const FIELD_KIND: &str = "kind";
pub(crate) const FIELD_NAME: &str = "name";
pub(crate) const FIELD_VALUE: &str = "value";

const VALUE_BLANK: &str = "sheet-blank";
const VALUE_BOOL: &str = "sheet-bool";
const VALUE_FORMULA: &str = "sheet-formula";
const VALUE_NUMBER: &str = "sheet-number";
const VALUE_TEXT: &str = "sheet-text";

/// A one-based spreadsheet cell reference in A1 notation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellRef {
    /// One-based column index.
    pub column: u32,
    /// One-based row index.
    pub row: u32,
}

impl CellRef {
    /// Builds a cell reference from one-based column and row indexes.
    pub fn new(column: u32, row: u32) -> Result<Self, SheetError> {
        if column == 0 || row == 0 {
            return Err(SheetError::InvalidCellRef(format!(
                "cell indexes are one-based, got column {column}, row {row}"
            )));
        }
        Ok(Self { column, row })
    }

    /// Parses an A1-style cell reference.
    pub fn parse(text: &str) -> Result<Self, SheetError> {
        text.parse()
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut column = self.column;
        let mut letters = Vec::new();
        while column > 0 {
            let rem = ((column - 1) % 26) as u8;
            letters.push((b'A' + rem) as char);
            column = (column - 1) / 26;
        }
        for letter in letters.iter().rev() {
            write!(f, "{letter}")?;
        }
        write!(f, "{}", self.row)
    }
}

impl FromStr for CellRef {
    type Err = SheetError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(SheetError::InvalidCellRef(
                "empty cell reference".to_owned(),
            ));
        }
        let mut column = 0_u32;
        let mut row_text = String::new();
        let mut saw_digit = false;
        for ch in trimmed.chars() {
            if ch.is_ascii_alphabetic() && !saw_digit {
                let upper = ch.to_ascii_uppercase() as u32;
                column = column
                    .checked_mul(26)
                    .and_then(|value| value.checked_add(upper - u32::from(b'A') + 1))
                    .ok_or_else(|| {
                        SheetError::InvalidCellRef(format!("column overflows in {trimmed}"))
                    })?;
            } else if ch.is_ascii_digit() {
                saw_digit = true;
                row_text.push(ch);
            } else {
                return Err(SheetError::InvalidCellRef(format!(
                    "invalid cell reference {trimmed}"
                )));
            }
        }
        if column == 0 || row_text.is_empty() {
            return Err(SheetError::InvalidCellRef(format!(
                "invalid cell reference {trimmed}"
            )));
        }
        let row = row_text.parse::<u32>().map_err(|error| {
            SheetError::InvalidCellRef(format!("invalid row in {trimmed}: {error}"))
        })?;
        Self::new(column, row)
    }
}

/// Exact spreadsheet cell value.
#[derive(Clone)]
pub enum CellValue {
    /// Empty cell.
    Blank,
    /// Text value.
    Text(String),
    /// Exact rational number.
    Number(Rational),
    /// Boolean value.
    Bool(bool),
    /// Formula text, conventionally beginning with `=`.
    Formula(String),
}

impl fmt::Debug for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => f.write_str("Blank"),
            Self::Text(value) => f.debug_tuple("Text").field(value).finish(),
            Self::Number(_) => f.write_str("Number(..)"),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::Formula(value) => f.debug_tuple("Formula").field(value).finish(),
        }
    }
}

/// In-memory sheet document.
#[derive(Clone, Debug)]
pub struct Sheet {
    /// Sheet name.
    pub name: String,
    /// Sparse cell map.
    pub cells: BTreeMap<CellRef, CellValue>,
}

impl Sheet {
    /// Builds an empty sheet.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cells: BTreeMap::new(),
        }
    }

    /// Returns a cell value.
    #[must_use]
    pub fn cell(&self, cell: &CellRef) -> CellValue {
        self.cells.get(cell).cloned().unwrap_or(CellValue::Blank)
    }

    /// Updates one cell. Assigning blank clears the sparse entry.
    pub fn set_cell(&mut self, cell: CellRef, value: CellValue) {
        if matches!(value, CellValue::Blank) {
            self.cells.remove(&cell);
        } else {
            self.cells.insert(cell, value);
        }
    }
}

/// Spreadsheet-domain failure.
#[derive(Debug)]
pub enum SheetError {
    /// A cell reference was malformed.
    InvalidCellRef(String),
    /// A rational literal was malformed.
    InvalidRational(String),
    /// A kernel operation failed.
    Kernel(String),
    /// The document kind was not `sheet`.
    WrongDocKind(String),
    /// The document body did not have the sheet expression shape.
    WrongDocBody(String),
    /// Formula parsing or evaluation failed.
    Formula(String),
    /// Formula evaluation reached a cycle.
    FormulaCycle(CellRef),
    /// A formula expected a number but found another value.
    NonNumericCell(CellRef),
    /// A formula divided by zero.
    DivisionByZero,
    /// An edit payload was not a sheet edit.
    WrongEdit(String),
}

impl fmt::Display for SheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCellRef(message) => write!(f, "invalid cell reference: {message}"),
            Self::InvalidRational(message) => write!(f, "invalid rational: {message}"),
            Self::Kernel(message) => write!(f, "kernel error: {message}"),
            Self::WrongDocKind(kind) => write!(f, "expected sheet document, got {kind}"),
            Self::WrongDocBody(message) => write!(f, "invalid sheet document body: {message}"),
            Self::Formula(message) => write!(f, "formula error: {message}"),
            Self::FormulaCycle(cell) => write!(f, "formula cycle at {cell}"),
            Self::NonNumericCell(cell) => write!(f, "formula expected numeric cell {cell}"),
            Self::DivisionByZero => f.write_str("formula divided by zero"),
            Self::WrongEdit(message) => write!(f, "invalid sheet edit: {message}"),
        }
    }
}

impl std::error::Error for SheetError {}

impl From<sim_kernel::Error> for SheetError {
    fn from(error: sim_kernel::Error) -> Self {
        Self::Kernel(error.to_string())
    }
}

/// Builds an exact rational from an integer or `num/den` literal.
pub fn rational_from_str(cx: &mut Cx, text: &str) -> Result<Rational, SheetError> {
    let source = text.trim();
    let literal = if source.contains('/') {
        source.to_owned()
    } else {
        format!("{source}/1")
    };
    let Some((num, den)) = parse_rational_parts(&literal) else {
        return Err(SheetError::InvalidRational(source.to_owned()));
    };
    rational_from_parts(cx, num, den)
}

/// Returns the compact `num/den` canonical text for a rational.
pub fn rational_to_canonical(cx: &mut Cx, value: &Rational) -> Result<String, SheetError> {
    let (num, den) = rational_parts(cx, value)?;
    Ok(format!("{num}/{den}"))
}

pub(crate) fn rational_from_parts(
    cx: &mut Cx,
    num: BigInt,
    den: BigInt,
) -> Result<Rational, SheetError> {
    let Some((num, den)) = parse_rational_parts(&format!("{num}/{den}")) else {
        return Err(SheetError::InvalidRational(format!("{num}/{den}")));
    };
    let num = cx
        .factory()
        .number_literal(bigint_domain(), num.to_string())?;
    let den = cx
        .factory()
        .number_literal(bigint_domain(), den.to_string())?;
    Ok(Rational { num, den })
}

pub(crate) fn rational_parts(
    cx: &mut Cx,
    value: &Rational,
) -> Result<(BigInt, BigInt), SheetError> {
    Ok((
        integer_value(cx, &value.num, "numerator")?,
        integer_value(cx, &value.den, "denominator")?,
    ))
}

fn integer_value(cx: &mut Cx, value: &sim_kernel::Value, role: &str) -> Result<BigInt, SheetError> {
    let Some(number) = cx.number_value_ref(value.clone())? else {
        return Err(SheetError::InvalidRational(format!(
            "{role} is not a number"
        )));
    };
    let Some(literal) = number.literal else {
        return Err(SheetError::InvalidRational(format!(
            "{role} has no canonical literal"
        )));
    };
    literal.canonical.parse::<BigInt>().map_err(|error| {
        SheetError::InvalidRational(format!("invalid {role} {}: {error}", literal.canonical))
    })
}

pub(crate) fn sheet_to_expr(cx: &mut Cx, sheet: &Sheet) -> Result<Expr, SheetError> {
    let cells = sheet
        .cells
        .iter()
        .map(|(cell, value)| {
            Ok(map(vec![
                field(FIELD_CELL, Expr::String(cell.to_string())),
                field(FIELD_VALUE, cell_value_to_expr(cx, value)?),
            ]))
        })
        .collect::<Result<Vec<_>, SheetError>>()?;
    Ok(map(vec![
        field(FIELD_KIND, Expr::Symbol(office_symbol(SHEET_DOC_KIND))),
        field(FIELD_NAME, Expr::String(sheet.name.clone())),
        field(FIELD_CELLS, Expr::List(cells)),
    ]))
}

pub(crate) fn sheet_from_expr(cx: &mut Cx, expr: &Expr) -> Result<Sheet, SheetError> {
    let entries = expect_map(expr)?;
    expect_kind(entries, SHEET_DOC_KIND)?;
    let name = expect_string(field_value(entries, FIELD_NAME)?, FIELD_NAME)?.to_owned();
    let cell_values = expect_list(field_value(entries, FIELD_CELLS)?, FIELD_CELLS)?;
    let mut sheet = Sheet::new(name);
    for cell_expr in cell_values {
        let entries = expect_map(cell_expr)?;
        let cell = CellRef::parse(expect_string(
            field_value(entries, FIELD_CELL)?,
            FIELD_CELL,
        )?)?;
        let value = cell_value_from_expr(cx, field_value(entries, FIELD_VALUE)?)?;
        sheet.set_cell(cell, value);
    }
    Ok(sheet)
}

pub(crate) fn cell_value_to_expr(cx: &mut Cx, value: &CellValue) -> Result<Expr, SheetError> {
    Ok(match value {
        CellValue::Blank => map(vec![field(
            FIELD_KIND,
            Expr::Symbol(office_symbol(VALUE_BLANK)),
        )]),
        CellValue::Text(text) => map(vec![
            field(FIELD_KIND, Expr::Symbol(office_symbol(VALUE_TEXT))),
            field(FIELD_VALUE, Expr::String(text.clone())),
        ]),
        CellValue::Number(number) => map(vec![
            field(FIELD_KIND, Expr::Symbol(office_symbol(VALUE_NUMBER))),
            field(
                FIELD_VALUE,
                Expr::Number(NumberLiteral {
                    domain: rational_domain(),
                    canonical: rational_to_canonical(cx, number)?,
                }),
            ),
        ]),
        CellValue::Bool(value) => map(vec![
            field(FIELD_KIND, Expr::Symbol(office_symbol(VALUE_BOOL))),
            field(FIELD_VALUE, Expr::Bool(*value)),
        ]),
        CellValue::Formula(formula) => map(vec![
            field(FIELD_KIND, Expr::Symbol(office_symbol(VALUE_FORMULA))),
            field(FIELD_VALUE, Expr::String(formula.clone())),
        ]),
    })
}

pub(crate) fn cell_value_from_expr(cx: &mut Cx, expr: &Expr) -> Result<CellValue, SheetError> {
    let entries = expect_map(expr)?;
    let kind = expect_symbol(field_value(entries, FIELD_KIND)?, FIELD_KIND)?;
    match kind.as_qualified_str().as_str() {
        "office/sheet-blank" => Ok(CellValue::Blank),
        "office/sheet-text" => Ok(CellValue::Text(
            expect_string(field_value(entries, FIELD_VALUE)?, FIELD_VALUE)?.to_owned(),
        )),
        "office/sheet-number" => {
            let Expr::Number(NumberLiteral { domain, canonical }) =
                field_value(entries, FIELD_VALUE)?
            else {
                return Err(SheetError::WrongDocBody(
                    "sheet number value must be a number".to_owned(),
                ));
            };
            if domain != &rational_domain() {
                return Err(SheetError::WrongDocBody(format!(
                    "sheet number domain must be {}, got {}",
                    rational_domain(),
                    domain
                )));
            }
            rational_from_str(cx, canonical).map(CellValue::Number)
        }
        "office/sheet-bool" => {
            let Expr::Bool(value) = field_value(entries, FIELD_VALUE)? else {
                return Err(SheetError::WrongDocBody(
                    "sheet bool value must be a bool".to_owned(),
                ));
            };
            Ok(CellValue::Bool(*value))
        }
        "office/sheet-formula" => Ok(CellValue::Formula(
            expect_string(field_value(entries, FIELD_VALUE)?, FIELD_VALUE)?.to_owned(),
        )),
        other => Err(SheetError::WrongDocBody(format!(
            "unsupported cell value kind {other}"
        ))),
    }
}

pub(crate) fn map(entries: Vec<(Expr, Expr)>) -> Expr {
    Expr::Map(entries)
}

pub(crate) fn field(name: &'static str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

pub(crate) fn office_symbol(name: &'static str) -> Symbol {
    Symbol::qualified("office", name)
}

pub(crate) fn field_value<'a>(
    entries: &'a [(Expr, Expr)],
    name: &'static str,
) -> Result<&'a Expr, SheetError> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == &Symbol::new(name) => Some(value),
            _ => None,
        })
        .ok_or_else(|| SheetError::WrongDocBody(format!("missing field {name}")))
}

pub(crate) fn expect_map(expr: &Expr) -> Result<&[(Expr, Expr)], SheetError> {
    match expr {
        Expr::Map(entries) => Ok(entries),
        _ => Err(SheetError::WrongDocBody("expected map".to_owned())),
    }
}

pub(crate) fn expect_list<'a>(
    expr: &'a Expr,
    field_name: &'static str,
) -> Result<&'a [Expr], SheetError> {
    match expr {
        Expr::List(items) => Ok(items),
        _ => Err(SheetError::WrongDocBody(format!(
            "field {field_name} must be a list"
        ))),
    }
}

pub(crate) fn expect_string<'a>(
    expr: &'a Expr,
    field_name: &'static str,
) -> Result<&'a str, SheetError> {
    match expr {
        Expr::String(value) => Ok(value),
        _ => Err(SheetError::WrongDocBody(format!(
            "field {field_name} must be a string"
        ))),
    }
}

pub(crate) fn expect_symbol<'a>(
    expr: &'a Expr,
    field_name: &'static str,
) -> Result<&'a Symbol, SheetError> {
    match expr {
        Expr::Symbol(value) => Ok(value),
        _ => Err(SheetError::WrongDocBody(format!(
            "field {field_name} must be a symbol"
        ))),
    }
}

fn expect_kind(entries: &[(Expr, Expr)], expected: &'static str) -> Result<(), SheetError> {
    let kind = expect_symbol(field_value(entries, FIELD_KIND)?, FIELD_KIND)?;
    let expected = office_symbol(expected);
    if kind == &expected {
        Ok(())
    } else {
        Err(SheetError::WrongDocBody(format!(
            "expected kind {}, got {}",
            expected,
            kind.as_qualified_str()
        )))
    }
}

fn rational_domain() -> Symbol {
    sim_lib_numbers_rational::number_domain()
}

fn bigint_domain() -> Symbol {
    Symbol::qualified("numbers", "bigint")
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod model_tests;
