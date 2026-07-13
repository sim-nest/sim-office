//! `.xlsx` codec implementation for the exact sheet domain.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use roxmltree::{Document, Node};
use sim_kernel::Cx;
use sim_lib_doc_core::{
    Doc, DocCodec, DocCodecOptions, DocId, DocKind, ExternalRef, FidelityReport, OfficeError,
};
use sim_lib_sheet::{
    CellRef, CellValue, SHEET_DOC_KIND, Sheet, doc_to_sheet, rational_from_str,
    rational_to_canonical, sheet_to_doc,
};

use crate::package::{
    CONTENT_TYPES, ROOT_RELS, WORKBOOK, WORKBOOK_RELS, WORKSHEET, XlsxPackage, write_package,
};

/// Stable codec id for local OOXML spreadsheet packages.
pub const XLSX_CODEC_ID: &str = "codec/ooxml-xlsx";
/// File extension accepted by this codec.
pub const XLSX_EXTENSION: &str = ".xlsx";

const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const XMLNS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const XMLNS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XMLNS_SIM: &str = "https://sim.nest/office/ooxml";

/// Local `.xlsx` codec for exact sheet documents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XlsxCodec;

/// Builds the local OOXML spreadsheet codec.
#[must_use]
pub fn xlsx_codec() -> XlsxCodec {
    XlsxCodec
}

impl DocCodec for XlsxCodec {
    fn codec_id(&self) -> &'static str {
        XLSX_CODEC_ID
    }

    fn kinds(&self) -> &'static [DocKind] {
        static KINDS: OnceLock<Vec<DocKind>> = OnceLock::new();
        KINDS
            .get_or_init(|| vec![DocKind::new(SHEET_DOC_KIND)])
            .as_slice()
    }

    fn decode(
        &self,
        cx: &mut Cx,
        bytes: &[u8],
        _options: &DocCodecOptions,
    ) -> Result<(Doc, FidelityReport), OfficeError> {
        let package = XlsxPackage::read(bytes)?;
        let sheet_info = first_sheet(package.text(WORKBOOK)?)?;
        let sheet_path = worksheet_path(&package, &sheet_info.rel_id)?;
        let shared_strings = shared_strings(&package)?;
        let mut report = FidelityReport::new(XLSX_CODEC_ID);
        if package.has("xl/styles.xml") {
            add_loss(
                &mut report,
                "styles",
                "workbook styles are not represented in the portable sheet model",
            );
        }
        let sheet_xml = package.text(&sheet_path)?;
        let sheet = decode_sheet(
            cx,
            &sheet_info.name,
            sheet_xml,
            &shared_strings,
            &mut report,
        )?;
        let mut doc = sheet_to_doc(cx, DocId::new(format!("xlsx:{}", sheet.name)), &sheet)
            .map_err(sheet_error)?;
        doc.origin
            .push(ExternalRef::new(XLSX_CODEC_ID, sheet_path, None, None));
        Ok((doc, report))
    }

    fn encode(
        &self,
        cx: &mut Cx,
        doc: &Doc,
        _options: &DocCodecOptions,
    ) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
        let sheet = doc_to_sheet(cx, doc).map_err(sheet_error)?;
        let entries = package_entries(cx, &sheet)?;
        let bytes = write_package(entries)?;
        Ok((bytes, FidelityReport::new(XLSX_CODEC_ID)))
    }
}

struct SheetInfo {
    name: String,
    rel_id: String,
}

fn package_entries(cx: &mut Cx, sheet: &Sheet) -> Result<BTreeMap<String, String>, OfficeError> {
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_TYPES.to_owned(), content_types());
    entries.insert(ROOT_RELS.to_owned(), root_rels());
    entries.insert(WORKBOOK.to_owned(), workbook_xml(&sheet.name));
    entries.insert(WORKBOOK_RELS.to_owned(), workbook_rels());
    entries.insert(WORKSHEET.to_owned(), worksheet_xml(cx, sheet)?);
    Ok(entries)
}

fn content_types() -> String {
    concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        r#"<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
        r#"</Types>"#
    )
    .to_owned()
}

fn root_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_OFFICE_DOCUMENT}" Target="xl/workbook.xml"/></Relationships>"#
    )
}

fn workbook_xml(name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{XMLNS_MAIN}" xmlns:r="{XMLNS_REL}"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        escape_attr(name)
    )
}

fn workbook_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_WORKSHEET}" Target="worksheets/sheet1.xml"/></Relationships>"#
    )
}

fn worksheet_xml(cx: &mut Cx, sheet: &Sheet) -> Result<String, OfficeError> {
    let mut rows: BTreeMap<u32, Vec<(&CellRef, &CellValue)>> = BTreeMap::new();
    for (cell, value) in &sheet.cells {
        if !matches!(value, CellValue::Blank) {
            rows.entry(cell.row).or_default().push((cell, value));
        }
    }
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{XMLNS_MAIN}" xmlns:sim="{XMLNS_SIM}"><sheetData>"#
    );
    for (row, cells) in rows {
        xml.push_str(&format!(r#"<row r="{row}">"#));
        for (cell, value) in cells {
            xml.push_str(&cell_xml(cx, cell, value)?);
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    Ok(xml)
}

fn cell_xml(cx: &mut Cx, cell: &CellRef, value: &CellValue) -> Result<String, OfficeError> {
    let cell_ref = cell.to_string();
    let xml = match value {
        CellValue::Blank => String::new(),
        CellValue::Text(text) => format!(
            r#"<c r="{cell_ref}" t="inlineStr" sim:kind="text"><is><t>{}</t></is></c>"#,
            escape_text(text)
        ),
        CellValue::Number(number) => format!(
            r#"<c r="{cell_ref}" t="str" sim:kind="number"><v>{}</v></c>"#,
            escape_text(&rational_to_canonical(cx, number).map_err(sheet_error)?)
        ),
        CellValue::Bool(value) => format!(
            r#"<c r="{cell_ref}" t="b" sim:kind="bool"><v>{}</v></c>"#,
            if *value { "1" } else { "0" }
        ),
        CellValue::Formula(formula) => format!(
            r#"<c r="{cell_ref}" sim:kind="formula"><f>{}</f></c>"#,
            escape_text(formula.trim_start_matches('='))
        ),
    };
    Ok(xml)
}

fn first_sheet(workbook_xml: &str) -> Result<SheetInfo, OfficeError> {
    let document = parse_xml(workbook_xml, "workbook")?;
    let sheet = document
        .descendants()
        .find(|node| node.has_tag_name("sheet"))
        .ok_or_else(|| error("workbook has no sheet records"))?;
    let name = attr(sheet, "name").unwrap_or("Sheet1").to_owned();
    let rel_id = attr(sheet, "id").unwrap_or("rId1").to_owned();
    Ok(SheetInfo { name, rel_id })
}

fn worksheet_path(package: &XlsxPackage, rel_id: &str) -> Result<String, OfficeError> {
    let rels = parse_xml(package.text(WORKBOOK_RELS)?, "workbook relationships")?;
    for relationship in rels
        .descendants()
        .filter(|node| node.has_tag_name("Relationship"))
    {
        if attr(relationship, "Id") == Some(rel_id) {
            let target = attr(relationship, "Target")
                .ok_or_else(|| error(format!("relationship {rel_id} has no Target")))?;
            return Ok(resolve_workbook_target(target));
        }
    }
    Err(error(format!(
        "workbook relationship {rel_id} does not resolve to a worksheet"
    )))
}

fn resolve_workbook_target(target: &str) -> String {
    let target = target.replace('\\', "/");
    if let Some(stripped) = target.strip_prefix('/') {
        stripped.to_owned()
    } else if target.starts_with("xl/") {
        target
    } else {
        format!("xl/{target}")
    }
}

fn shared_strings(package: &XlsxPackage) -> Result<Vec<String>, OfficeError> {
    if !package.has("xl/sharedStrings.xml") {
        return Ok(Vec::new());
    }
    let document = parse_xml(package.text("xl/sharedStrings.xml")?, "shared strings")?;
    let strings = document
        .descendants()
        .filter(|node| node.has_tag_name("si"))
        .map(|item| {
            item.descendants()
                .filter(|node| node.has_tag_name("t"))
                .filter_map(|node| node.text())
                .collect::<String>()
        })
        .collect();
    Ok(strings)
}

fn decode_sheet(
    cx: &mut Cx,
    name: &str,
    sheet_xml: &str,
    shared_strings: &[String],
    report: &mut FidelityReport,
) -> Result<Sheet, OfficeError> {
    let document = parse_xml(sheet_xml, "worksheet")?;
    if document
        .descendants()
        .any(|node| node.has_tag_name("mergeCell") || node.has_tag_name("mergeCells"))
    {
        add_loss(
            report,
            "mergeCells",
            "merged cell ranges are not represented in the portable sheet model",
        );
    }
    let mut sheet = Sheet::new(name);
    for cell in document.descendants().filter(|node| node.has_tag_name("c")) {
        let cell_ref = attr(cell, "r").ok_or_else(|| error("cell is missing r attribute"))?;
        if attr(cell, "s").is_some() {
            add_loss(
                report,
                format!("cell.{cell_ref}.style"),
                "cell style is not represented in the portable sheet model",
            );
        }
        let cell_ref = CellRef::parse(cell_ref).map_err(sheet_error)?;
        let value = decode_cell(cx, cell, shared_strings)?;
        sheet.set_cell(cell_ref, value);
    }
    Ok(sheet)
}

fn decode_cell(
    cx: &mut Cx,
    cell: Node<'_, '_>,
    shared_strings: &[String],
) -> Result<CellValue, OfficeError> {
    if let Some(formula) = child_text(cell, "f") {
        return Ok(CellValue::Formula(format!(
            "={}",
            formula.trim_start_matches('=')
        )));
    }
    let sim_kind = attr(cell, "kind");
    let cell_type = attr(cell, "t");
    let text = cell_text(cell, cell_type, shared_strings)?;
    match sim_kind {
        Some("blank") => Ok(CellValue::Blank),
        Some("bool") => bool_value(&text).map(CellValue::Bool),
        Some("formula") => Ok(CellValue::Formula(format!(
            "={}",
            text.trim_start_matches('=')
        ))),
        Some("number") => rational_value(cx, &text),
        Some("text") => Ok(CellValue::Text(text)),
        _ => match cell_type {
            Some("b") => bool_value(&text).map(CellValue::Bool),
            Some("s") | Some("str") | Some("inlineStr") => Ok(CellValue::Text(text)),
            Some(other) if !matches!(other, "n") => Ok(CellValue::Text(text)),
            _ if text.trim().is_empty() => Ok(CellValue::Blank),
            _ => rational_value(cx, &text),
        },
    }
}

fn cell_text(
    cell: Node<'_, '_>,
    cell_type: Option<&str>,
    shared_strings: &[String],
) -> Result<String, OfficeError> {
    match cell_type {
        Some("inlineStr") => Ok(cell
            .descendants()
            .find(|node| node.has_tag_name("t"))
            .and_then(|node| node.text())
            .unwrap_or_default()
            .to_owned()),
        Some("s") => {
            let index = child_text(cell, "v")
                .ok_or_else(|| error("shared string cell is missing v"))?
                .parse::<usize>()
                .map_err(|err| error(format!("shared string index is invalid: {err}")))?;
            shared_strings
                .get(index)
                .cloned()
                .ok_or_else(|| error(format!("shared string index {index} is missing")))
        }
        _ => Ok(child_text(cell, "v").unwrap_or_default()),
    }
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
            "scientific notation is not accepted for exact xlsx numbers: {text}"
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

fn child_text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(tag))
        .and_then(|child| child.text())
        .map(str::to_owned)
}

fn parse_xml<'a>(text: &'a str, label: &str) -> Result<Document<'a>, OfficeError> {
    Document::parse(text).map_err(|err| error(format!("could not parse {label} XML: {err}")))
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn add_loss(report: &mut FidelityReport, field: impl Into<String>, reason: impl Into<String>) {
    let current = std::mem::take(report);
    *report = current.with_dropped(field, reason);
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    escape_text(text)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sheet_error(error: impl std::fmt::Display) -> OfficeError {
    OfficeError::Kernel(error.to_string())
}

fn error(message: impl Into<String>) -> OfficeError {
    OfficeError::Kernel(message.into())
}

#[cfg(test)]
#[path = "xlsx_tests.rs"]
mod tests;
