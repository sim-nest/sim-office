//! `.ods` codec implementation for the exact sheet domain.

use std::collections::BTreeMap;

use roxmltree::{Document, Node};
use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocId, ExternalRef, FidelityReport, OfficeError};
use sim_lib_sheet::{
    CellRef, CellValue, Sheet, doc_to_sheet, rational_from_str, rational_to_canonical, sheet_to_doc,
};

use crate::ODF_CODEC_ID;
use crate::package::{
    CONTENT_XML, MANIFEST_XML, ODS_MIMETYPE, OFFICE_NS, OdfPackage, SIM_NS, STYLES_XML, TABLE_NS,
    add_loss, attr_any, attr_ns, error, escape_attr, escape_text, manifest_xml, parse_xml,
    styles_xml, text_content, write_package,
};

pub(crate) fn decode(
    cx: &mut Cx,
    package: &OdfPackage,
) -> Result<(Doc, FidelityReport), OfficeError> {
    let content_xml = package.text(CONTENT_XML)?;
    let document = parse_xml(content_xml, "spreadsheet content")?;
    let mut report = FidelityReport::new(ODF_CODEC_ID);
    report_styles(package, &document, &mut report)?;
    let table = document
        .descendants()
        .find(|node| node.has_tag_name("table"))
        .ok_or_else(|| error("ODS content has no table"))?;
    let name = attr_ns(table, TABLE_NS, "name")
        .or_else(|| attr_any(table, "name"))
        .unwrap_or("Sheet1");
    let mut sheet = Sheet::new(name);
    decode_rows(cx, table, &mut sheet, &mut report)?;
    let mut doc =
        sheet_to_doc(cx, DocId::new(format!("ods:{}", sheet.name)), &sheet).map_err(sheet_error)?;
    doc.origin
        .push(ExternalRef::new(ODF_CODEC_ID, CONTENT_XML, None, None));
    Ok((doc, report))
}

pub(crate) fn encode(cx: &mut Cx, doc: &Doc) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
    let sheet = doc_to_sheet(cx, doc).map_err(sheet_error)?;
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_XML.to_owned(), content_xml(cx, &sheet)?);
    entries.insert(STYLES_XML.to_owned(), styles_xml());
    entries.insert(MANIFEST_XML.to_owned(), manifest_xml(ODS_MIMETYPE));
    let bytes = write_package(ODS_MIMETYPE, entries)?;
    Ok((bytes, FidelityReport::new(ODF_CODEC_ID)))
}

fn content_xml(cx: &mut Cx, sheet: &Sheet) -> Result<String, OfficeError> {
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="{OFFICE_NS}" xmlns:table="{TABLE_NS}" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:sim="{SIM_NS}" office:version="1.2"><office:body><office:spreadsheet>{}</office:spreadsheet></office:body></office:document-content>"#,
        table_xml(cx, sheet)?
    ))
}

fn table_xml(cx: &mut Cx, sheet: &Sheet) -> Result<String, OfficeError> {
    let mut rows: BTreeMap<u32, Vec<(&CellRef, &CellValue)>> = BTreeMap::new();
    for (cell, value) in &sheet.cells {
        if !matches!(value, CellValue::Blank) {
            rows.entry(cell.row).or_default().push((cell, value));
        }
    }
    let mut xml = format!(r#"<table:table table:name="{}">"#, escape_attr(&sheet.name));
    let mut next_row = 1_u32;
    for (row, cells) in rows {
        while next_row < row {
            xml.push_str("<table:table-row/>");
            next_row += 1;
        }
        xml.push_str("<table:table-row>");
        let mut next_column = 1_u32;
        for (cell, value) in cells {
            if next_column < cell.column {
                xml.push_str(&empty_cells_xml(cell.column - next_column));
            }
            xml.push_str(&cell_xml(cx, cell, value)?);
            next_column = cell.column + 1;
        }
        xml.push_str("</table:table-row>");
        next_row += 1;
    }
    xml.push_str("</table:table>");
    Ok(xml)
}

fn empty_cells_xml(count: u32) -> String {
    if count == 1 {
        "<table:table-cell/>".to_owned()
    } else {
        format!(r#"<table:table-cell table:number-columns-repeated="{count}"/>"#)
    }
}

fn cell_xml(cx: &mut Cx, cell: &CellRef, value: &CellValue) -> Result<String, OfficeError> {
    let cell_ref = cell.to_string();
    let xml = match value {
        CellValue::Blank => String::new(),
        CellValue::Text(text) => format!(
            r#"<table:table-cell office:value-type="string" sim:cell="{cell_ref}" sim:kind="text" sim:value="{}"><text:p>{}</text:p></table:table-cell>"#,
            escape_attr(text),
            escape_text(text)
        ),
        CellValue::Number(number) => {
            let canonical = rational_to_canonical(cx, number).map_err(sheet_error)?;
            format!(
                r#"<table:table-cell office:value-type="string" sim:cell="{cell_ref}" sim:kind="number" sim:value="{}"><text:p>{}</text:p></table:table-cell>"#,
                escape_attr(&canonical),
                escape_text(&canonical)
            )
        }
        CellValue::Bool(value) => format!(
            r#"<table:table-cell office:value-type="boolean" office:boolean-value="{}" sim:cell="{cell_ref}" sim:kind="bool" sim:value="{}"><text:p>{}</text:p></table:table-cell>"#,
            value, value, value
        ),
        CellValue::Formula(formula) => {
            let formula_text = formula.trim_start_matches('=');
            format!(
                r#"<table:table-cell office:value-type="string" table:formula="of:={}" sim:cell="{cell_ref}" sim:kind="formula" sim:value="{}"><text:p>{}</text:p></table:table-cell>"#,
                escape_attr(formula_text),
                escape_attr(formula),
                escape_text(formula)
            )
        }
    };
    Ok(xml)
}

fn decode_rows(
    cx: &mut Cx,
    table: Node<'_, '_>,
    sheet: &mut Sheet,
    report: &mut FidelityReport,
) -> Result<(), OfficeError> {
    let mut row_index = 1_u32;
    for row in table
        .children()
        .filter(|node| node.has_tag_name("table-row"))
    {
        let repeats = repeat_count(row, "number-rows-repeated")?;
        for _ in 0..repeats {
            decode_row(cx, row, row_index, sheet, report)?;
            row_index += 1;
        }
    }
    Ok(())
}

fn decode_row(
    cx: &mut Cx,
    row: Node<'_, '_>,
    row_index: u32,
    sheet: &mut Sheet,
    report: &mut FidelityReport,
) -> Result<(), OfficeError> {
    let mut column_index = 1_u32;
    for cell in row
        .children()
        .filter(|node| node.has_tag_name("table-cell") || node.has_tag_name("covered-table-cell"))
    {
        let repeats = repeat_count(cell, "number-columns-repeated")?;
        if cell.has_tag_name("covered-table-cell") {
            column_index += repeats;
            continue;
        }
        if has_style(cell) {
            add_loss(
                report,
                format!("cell.{column_index}.{row_index}.style"),
                "cell style is not represented in the portable sheet model",
            );
        }
        let value = decode_cell(cx, cell)?;
        for offset in 0..repeats {
            if !matches!(value, CellValue::Blank) {
                let cell_ref = sim_cell(cell)
                    .filter(|_| offset == 0)
                    .map(CellRef::parse)
                    .transpose()
                    .map_err(sheet_error)?
                    .unwrap_or(
                        CellRef::new(column_index + offset, row_index).map_err(sheet_error)?,
                    );
                sheet.set_cell(cell_ref, value.clone());
            }
        }
        column_index += repeats;
    }
    Ok(())
}

fn decode_cell(cx: &mut Cx, cell: Node<'_, '_>) -> Result<CellValue, OfficeError> {
    let sim_kind = attr_ns(cell, SIM_NS, "kind");
    let sim_value = attr_ns(cell, SIM_NS, "value");
    let text = sim_value
        .map(str::to_owned)
        .unwrap_or_else(|| text_content(cell));
    match sim_kind {
        Some("blank") => Ok(CellValue::Blank),
        Some("bool") => bool_value(&text).map(CellValue::Bool),
        Some("formula") => Ok(CellValue::Formula(format!(
            "={}",
            text.trim_start_matches('=')
        ))),
        Some("number") => rational_value(cx, &text),
        Some("text") => Ok(CellValue::Text(text)),
        _ => decode_untyped_cell(cx, cell, &text),
    }
}

fn decode_untyped_cell(
    cx: &mut Cx,
    cell: Node<'_, '_>,
    text: &str,
) -> Result<CellValue, OfficeError> {
    if let Some(formula) = attr_ns(cell, TABLE_NS, "formula").or_else(|| attr_any(cell, "formula"))
    {
        return normalize_formula(formula).map(CellValue::Formula);
    }
    match attr_ns(cell, OFFICE_NS, "value-type") {
        Some("boolean") => bool_value(
            attr_ns(cell, OFFICE_NS, "boolean-value")
                .or_else(|| attr_any(cell, "boolean-value"))
                .unwrap_or(text),
        )
        .map(CellValue::Bool),
        Some("float") | Some("currency") | Some("percentage") => rational_value(
            cx,
            attr_ns(cell, OFFICE_NS, "value")
                .or_else(|| attr_any(cell, "value"))
                .unwrap_or(text),
        ),
        Some("string") | Some("date") | Some("time") => Ok(CellValue::Text(text.to_owned())),
        _ if text.trim().is_empty() => Ok(CellValue::Blank),
        _ => Ok(CellValue::Text(text.to_owned())),
    }
}

fn normalize_formula(formula: &str) -> Result<String, OfficeError> {
    let formula = formula.trim();
    let formula = formula
        .strip_prefix("of:")
        .or_else(|| formula.strip_prefix("oooc:"))
        .unwrap_or(formula);
    if formula.starts_with('=') {
        Ok(formula.to_owned())
    } else {
        Err(error(format!("unsupported ODF formula syntax {formula}")))
    }
}

fn repeat_count(node: Node<'_, '_>, attr: &str) -> Result<u32, OfficeError> {
    attr_ns(node, TABLE_NS, attr)
        .or_else(|| attr_any(node, attr))
        .unwrap_or("1")
        .parse::<u32>()
        .map_err(|err| error(format!("invalid ODF repeat count: {err}")))
}

fn sim_cell<'a>(cell: Node<'a, '_>) -> Option<&'a str> {
    attr_ns(cell, SIM_NS, "cell")
}

fn has_style(node: Node<'_, '_>) -> bool {
    attr_any(node, "style-name").is_some()
}

fn report_styles(
    package: &OdfPackage,
    document: &Document<'_>,
    report: &mut FidelityReport,
) -> Result<(), OfficeError> {
    let content_has_styles = document.descendants().any(has_style);
    let style_file_has_styles =
        package.has(STYLES_XML) && package.text(STYLES_XML)?.contains("<style:style");
    if content_has_styles || style_file_has_styles {
        add_loss(
            report,
            "styles",
            "ODF styles are not represented in the portable sheet model",
        );
    }
    Ok(())
}

fn bool_value(text: &str) -> Result<bool, OfficeError> {
    match text.trim() {
        "1" | "true" | "TRUE" => Ok(true),
        "0" | "false" | "FALSE" | "" => Ok(false),
        other => Err(error(format!("invalid boolean cell value {other}"))),
    }
}

fn rational_value(cx: &mut Cx, text: &str) -> Result<CellValue, OfficeError> {
    let literal = rational_literal(text)?;
    rational_from_str(cx, &literal)
        .map(CellValue::Number)
        .map_err(sheet_error)
}

fn rational_literal(text: &str) -> Result<String, OfficeError> {
    let text = text.trim();
    if text.contains('/') || !text.contains('.') {
        return Ok(text.to_owned());
    }
    if text.contains('e') || text.contains('E') {
        return Err(error(format!(
            "scientific notation is not accepted for exact ODF numbers: {text}"
        )));
    }
    let (sign, unsigned) = text
        .strip_prefix('-')
        .map_or(("", text), |rest| ("-", rest));
    let (whole, frac) = unsigned
        .split_once('.')
        .ok_or_else(|| error(format!("invalid decimal cell value {text}")))?;
    if frac.is_empty() {
        return Ok(format!("{sign}{whole}/1"));
    }
    let mut digits = format!("{whole}{frac}");
    while digits.starts_with('0') && digits.len() > 1 {
        digits.remove(0);
    }
    let denominator = format!("1{}", "0".repeat(frac.len()));
    Ok(format!("{sign}{digits}/{denominator}"))
}

fn sheet_error(error: impl std::fmt::Display) -> OfficeError {
    OfficeError::Kernel(error.to_string())
}
