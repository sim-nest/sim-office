//! Microsoft Graph workbook range bridge for exact sheet records.
//!
//! Microsoft Graph Excel REST works with `.xlsx` workbooks stored in Microsoft
//! 365 business storage such as OneDrive for Business and SharePoint. Active
//! workbook ranges use the Excel Office.js host bridge; this module handles the
//! backend range JSON and write-plan shape shared with that host bridge.

use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sim_kernel::{Cx, Expr};
use sim_lib_doc_core::{DocId, Edit};

use crate::model::{
    CellRef, CellValue, Sheet, SheetError, field, map, office_symbol, rational_from_parts,
    rational_from_str, rational_to_canonical,
};

/// Domain namespace used by Graph range write plans.
pub const GRAPH_RANGE_EDIT_DOMAIN: &str = "office/sheet/graph-range";

const FIELD_ADDRESS: &str = "address";
const FIELD_DRIVE_ITEM: &str = "drive-item";
const FIELD_INVERSE: &str = "inverse";
const FIELD_KIND: &str = "kind";
const FIELD_RANGE: &str = "range";
const FIELD_VALUES: &str = "values";
const FIELD_WORKSHEET: &str = "worksheet";
const OP_WRITE_GRAPH_RANGE: &str = "write-graph-range";
const OP_WRITE_GRAPH_RANGE_INVERSE: &str = "write-graph-range-inverse";

/// Target range inside a workbook stored as a Microsoft Graph drive item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookRangeTarget {
    /// Microsoft Graph drive item id for the workbook.
    pub drive_item: String,
    /// Worksheet name.
    pub worksheet: String,
    /// A1-style range address inside the worksheet.
    pub range: String,
}

impl WorkbookRangeTarget {
    /// Builds the Microsoft Graph path for this workbook range.
    #[must_use]
    pub fn graph_path(&self) -> String {
        format!(
            "/me/drive/items/{}/workbook/worksheets('{}')/range(address='{}')",
            path_segment(&self.drive_item),
            odata_string(&self.worksheet),
            odata_string(&self.range)
        )
    }
}

/// Minimal Microsoft Graph read seam implemented by host Graph adapters.
pub trait MsGraphSite {
    /// Runs a site-local Microsoft Graph GET and returns the decoded JSON body.
    fn graph_get(&self, cx: &mut Cx, path: &str) -> Result<JsonValue, SheetError>;
}

/// Office.js active-workbook range request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcelBridgeRangeRequest {
    /// Worksheet name.
    pub worksheet: String,
    /// A1-style range address.
    pub address: String,
}

/// Office.js active-workbook range reply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExcelBridgeRangeReply {
    /// Worksheet name echoed by the host.
    pub worksheet: String,
    /// Resolved range address returned by Office.js.
    pub address: String,
    /// Rectangular Office.js range values.
    pub values: Vec<Vec<JsonValue>>,
}

/// Reads one Microsoft Graph workbook range into an exact sparse sheet.
pub fn read_graph_range(
    cx: &mut Cx,
    site: &dyn MsGraphSite,
    target: &WorkbookRangeTarget,
) -> Result<Sheet, SheetError> {
    let body = site.graph_get(cx, &target.graph_path())?;
    graph_range_json_to_sheet(cx, target, &body)
}

/// Builds a Graph range write plan for a sheet rectangle.
pub fn plan_write_graph_range(
    cx: &mut Cx,
    sheet: &Sheet,
    target: &WorkbookRangeTarget,
) -> Result<Edit, SheetError> {
    let op = map(vec![
        field(
            FIELD_KIND,
            Expr::Symbol(office_symbol(OP_WRITE_GRAPH_RANGE)),
        ),
        field(FIELD_DRIVE_ITEM, Expr::String(target.drive_item.clone())),
        field(FIELD_WORKSHEET, Expr::String(target.worksheet.clone())),
        field(FIELD_RANGE, Expr::String(target.range.clone())),
        field(FIELD_VALUES, range_values_expr(cx, sheet, &target.range)?),
    ]);
    let inverse = map(vec![
        field(
            FIELD_KIND,
            Expr::Symbol(office_symbol(OP_WRITE_GRAPH_RANGE_INVERSE)),
        ),
        field(FIELD_DRIVE_ITEM, Expr::String(target.drive_item.clone())),
        field(FIELD_WORKSHEET, Expr::String(target.worksheet.clone())),
        field(FIELD_RANGE, Expr::String(target.range.clone())),
        field(FIELD_INVERSE, Expr::Nil),
    ]);
    Ok(Edit::new(
        DocId::new(format!(
            "site/msgraph/{}/{}!{}",
            target.drive_item, target.worksheet, target.range
        )),
        GRAPH_RANGE_EDIT_DOMAIN,
        cx.factory().expr(op)?,
        cx.factory().expr(inverse)?,
    ))
}

fn graph_range_json_to_sheet(
    cx: &mut Cx,
    target: &WorkbookRangeTarget,
    body: &JsonValue,
) -> Result<Sheet, SheetError> {
    let address = body
        .get(FIELD_ADDRESS)
        .and_then(JsonValue::as_str)
        .unwrap_or(target.range.as_str());
    let start = range_start(address)?;
    let values = values_rows(body)?;
    let mut sheet = Sheet::new(target.worksheet.clone());
    for (row_index, row_value) in values.iter().enumerate() {
        let row = row_value.as_array().ok_or_else(|| {
            SheetError::WrongDocBody("Graph range values must be row arrays".to_owned())
        })?;
        for (column_index, value) in row.iter().enumerate() {
            let cell = CellRef::new(
                start.column + column_index as u32,
                start.row + row_index as u32,
            )?;
            sheet.set_cell(cell, cell_value_from_json(cx, value)?);
        }
    }
    Ok(sheet)
}

fn values_rows(body: &JsonValue) -> Result<&[JsonValue], SheetError> {
    body.get(FIELD_VALUES)
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| SheetError::WrongDocBody("Graph range JSON must include values".to_owned()))
}

fn cell_value_from_json(cx: &mut Cx, value: &JsonValue) -> Result<CellValue, SheetError> {
    match value {
        JsonValue::Null => Ok(CellValue::Blank),
        JsonValue::Bool(value) => Ok(CellValue::Bool(*value)),
        JsonValue::Number(number) => number_to_cell_value(cx, number),
        JsonValue::String(text) => {
            if text.is_empty() {
                Ok(CellValue::Blank)
            } else if text.starts_with('=') {
                Ok(CellValue::Formula(text.clone()))
            } else {
                match rational_from_str(cx, text) {
                    Ok(number) => Ok(CellValue::Number(number)),
                    Err(SheetError::InvalidRational(_)) => Ok(CellValue::Text(text.clone())),
                    Err(error) => Err(error),
                }
            }
        }
        JsonValue::Array(_) | JsonValue::Object(_) => Err(SheetError::WrongDocBody(
            "Graph range cell values must be scalar".to_owned(),
        )),
    }
}

fn number_to_cell_value(cx: &mut Cx, number: &serde_json::Number) -> Result<CellValue, SheetError> {
    let text = number.to_string();
    if text.contains('.') {
        decimal_rational(cx, &text).map(CellValue::Number)
    } else {
        rational_from_str(cx, &text).map(CellValue::Number)
    }
}

fn decimal_rational(
    cx: &mut Cx,
    text: &str,
) -> Result<sim_lib_numbers_rational::Rational, SheetError> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let Some((whole, fraction)) = unsigned.split_once('.') else {
        return rational_from_str(cx, text);
    };
    if whole.is_empty()
        || fraction.is_empty()
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(SheetError::InvalidRational(text.to_owned()));
    }
    let denominator = BigInt::from(10_u32).pow(fraction.len() as u32);
    let mut numerator = whole
        .parse::<BigInt>()
        .map_err(|error| SheetError::InvalidRational(format!("{text}: {error}")))?
        * &denominator
        + fraction
            .parse::<BigInt>()
            .map_err(|error| SheetError::InvalidRational(format!("{text}: {error}")))?;
    if negative {
        numerator = -numerator;
    }
    rational_from_parts(cx, numerator, denominator)
}

fn range_values_expr(cx: &mut Cx, sheet: &Sheet, range: &str) -> Result<Expr, SheetError> {
    let (start, end) = range_bounds(range)?;
    if end.column < start.column || end.row < start.row {
        return Err(SheetError::InvalidCellRef(format!(
            "range end {end} precedes start {start}"
        )));
    }
    let mut rows = Vec::new();
    for row in start.row..=end.row {
        let mut values = Vec::new();
        for column in start.column..=end.column {
            let cell = CellRef::new(column, row)?;
            values.push(cell_value_to_bridge_expr(cx, &sheet.cell(&cell))?);
        }
        rows.push(Expr::List(values));
    }
    Ok(Expr::List(rows))
}

fn cell_value_to_bridge_expr(cx: &mut Cx, value: &CellValue) -> Result<Expr, SheetError> {
    Ok(match value {
        CellValue::Blank => Expr::Nil,
        CellValue::Bool(value) => Expr::Bool(*value),
        CellValue::Formula(formula) => Expr::String(formula.clone()),
        CellValue::Text(text) => Expr::String(text.clone()),
        CellValue::Number(number) => Expr::String(rational_to_canonical(cx, number)?),
    })
}

fn range_start(address: &str) -> Result<CellRef, SheetError> {
    let (start, _) = range_bounds(address)?;
    Ok(start)
}

fn range_bounds(address: &str) -> Result<(CellRef, CellRef), SheetError> {
    let address = address.rsplit('!').next().unwrap_or(address).trim();
    let (start, end) = address.split_once(':').unwrap_or((address, address));
    Ok((
        CellRef::parse(&normalize_cell_ref(start))?,
        CellRef::parse(&normalize_cell_ref(end))?,
    ))
}

fn normalize_cell_ref(cell: &str) -> String {
    cell.trim()
        .trim_matches('\'')
        .chars()
        .filter(|ch| *ch != '$')
        .collect()
}

fn path_segment(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(hex((byte >> 4) & 0x0f));
            encoded.push(hex(byte & 0x0f));
        }
    }
    encoded
}

fn hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => unreachable!("hex nybble"),
    }
}

fn odata_string(input: &str) -> String {
    input.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy, Symbol};

    use super::*;

    struct ModeledSite {
        path: String,
        body: JsonValue,
    }

    impl MsGraphSite for ModeledSite {
        fn graph_get(&self, _cx: &mut Cx, path: &str) -> Result<JsonValue, SheetError> {
            assert_eq!(path, self.path);
            Ok(self.body.clone())
        }
    }

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    fn target() -> WorkbookRangeTarget {
        WorkbookRangeTarget {
            drive_item: "drive item 1".to_owned(),
            worksheet: "Q1".to_owned(),
            range: "A1:B2".to_owned(),
        }
    }

    #[test]
    fn modeled_range_json_becomes_sheet() {
        let mut cx = test_context();
        let target = target();
        let site = ModeledSite {
            path: target.graph_path(),
            body: json!({
                "address": "Q1!A1:B2",
                "values": [["5/2", "memo"], [true, "=A1"]]
            }),
        };

        let sheet = read_graph_range(&mut cx, &site, &target).unwrap();

        assert!(matches!(
            sheet.cell(&CellRef::parse("A1").unwrap()),
            CellValue::Number(_)
        ));
        assert!(matches!(
            sheet.cell(&CellRef::parse("B1").unwrap()),
            CellValue::Text(ref value) if value == "memo"
        ));
        assert!(matches!(
            sheet.cell(&CellRef::parse("A2").unwrap()),
            CellValue::Bool(true)
        ));
        assert!(matches!(
            sheet.cell(&CellRef::parse("B2").unwrap()),
            CellValue::Formula(ref value) if value == "=A1"
        ));
    }

    #[test]
    fn write_plan_keeps_decimal_text_and_exact_numbers_as_strings() {
        let mut cx = test_context();
        let target = target();
        let mut sheet = Sheet::new("Q1");
        sheet.set_cell(
            CellRef::parse("A1").unwrap(),
            CellValue::Number(rational_from_str(&mut cx, "3/2").unwrap()),
        );
        sheet.set_cell(
            CellRef::parse("B1").unwrap(),
            CellValue::Text("1.25".to_owned()),
        );

        let edit = plan_write_graph_range(&mut cx, &sheet, &target).unwrap();
        let op = edit.op.object().as_expr(&mut cx).unwrap();
        let Some(Expr::List(rows)) = lookup_expr_entry(&op, FIELD_VALUES) else {
            panic!("write op should carry rows");
        };
        let Expr::List(first_row) = &rows[0] else {
            panic!("first row should be a list");
        };

        assert_eq!(first_row[0], Expr::String("3/2".to_owned()));
        assert_eq!(first_row[1], Expr::String("1.25".to_owned()));
    }

    #[test]
    fn bridge_schema_matches_office_js_contract() {
        let request: ExcelBridgeRangeRequest =
            serde_json::from_value(json!({ "worksheet": "Q1", "address": "A1:B2" })).unwrap();
        let reply: ExcelBridgeRangeReply = serde_json::from_value(json!({
            "worksheet": "Q1",
            "address": "Q1!A1:B2",
            "values": [[1, "memo"], [true, null]]
        }))
        .unwrap();

        assert_eq!(request.worksheet, "Q1");
        assert_eq!(request.address, "A1:B2");
        let encoded = serde_json::to_value(reply).unwrap();
        assert_eq!(encoded["worksheet"], "Q1");
        assert_eq!(encoded["address"], "Q1!A1:B2");
        assert_eq!(encoded["values"][0][1], "memo");
    }

    fn lookup_expr_entry<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
        let Expr::Map(entries) = expr else {
            return None;
        };
        entries.iter().find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == &Symbol::new(name) => Some(value),
            _ => None,
        })
    }
}
