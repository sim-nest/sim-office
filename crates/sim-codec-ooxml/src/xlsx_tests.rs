use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_doc_core::{DocCodecOptions, DocId};
use sim_lib_sheet::{rational_from_str, rational_to_canonical};
use zip::ZipArchive;

use super::*;

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn options(cx: &mut Cx) -> DocCodecOptions {
    DocCodecOptions::new(cx.factory().nil().unwrap())
}

#[test]
fn two_cell_sheet_round_trips() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(
        CellRef::parse("A1").unwrap(),
        CellValue::Number(rational_from_str(&mut cx, "1").unwrap()),
    );
    sheet.set_cell(
        CellRef::parse("B1").unwrap(),
        CellValue::Number(rational_from_str(&mut cx, "5/2").unwrap()),
    );
    let doc = sheet_to_doc(&mut cx, DocId::new("sheet-1"), &sheet).unwrap();
    let codec = XlsxCodec;

    let encode_options = options(&mut cx);
    let (bytes, encode_report) = codec.encode(&mut cx, &doc, &encode_options).unwrap();
    let decode_options = options(&mut cx);
    let (decoded, decode_report) = codec.decode(&mut cx, &bytes, &decode_options).unwrap();
    let sheet = doc_to_sheet(&mut cx, &decoded).unwrap();

    assert!(encode_report.is_lossless());
    assert!(decode_report.is_lossless());
    let CellValue::Number(a1) = sheet.cell(&CellRef::parse("A1").unwrap()) else {
        panic!("expected A1 number");
    };
    let CellValue::Number(b1) = sheet.cell(&CellRef::parse("B1").unwrap()) else {
        panic!("expected B1 number");
    };
    assert_eq!(rational_to_canonical(&mut cx, &a1).unwrap(), "1/1");
    assert_eq!(rational_to_canonical(&mut cx, &b1).unwrap(), "5/2");
}

#[test]
fn encoded_package_has_required_parts() {
    let mut cx = cx();
    let doc = sheet_to_doc(&mut cx, DocId::new("empty"), &Sheet::new("Sheet1")).unwrap();
    let encode_options = options(&mut cx);
    let (bytes, _) = XlsxCodec.encode(&mut cx, &doc, &encode_options).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut names = Vec::new();
    for index in 0..archive.len() {
        names.push(archive.by_index(index).unwrap().name().to_owned());
    }

    for required in [CONTENT_TYPES, ROOT_RELS, WORKBOOK, WORKBOOK_RELS, WORKSHEET] {
        assert!(
            names.iter().any(|name| name == required),
            "missing {required}"
        );
    }
}

#[test]
fn binary_xls_is_rejected() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let err = XlsxCodec
        .decode(&mut cx, b"\xD0\xCF\x11\xE0not-xlsx", &decode_options)
        .unwrap_err();

    assert!(err.to_string().contains(".xls"));
}

#[test]
fn styled_fixture_reports_loss() {
    let mut cx = cx();
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_TYPES.to_owned(), content_types_with_styles());
    entries.insert(ROOT_RELS.to_owned(), root_rels());
    entries.insert(WORKBOOK.to_owned(), workbook_xml("Styled"));
    entries.insert(WORKBOOK_RELS.to_owned(), workbook_rels());
    entries.insert("xl/styles.xml".to_owned(), styles_xml());
    entries.insert(WORKSHEET.to_owned(), styled_worksheet_xml());
    let bytes = write_package(entries).unwrap();

    let decode_options = options(&mut cx);
    let (_doc, report) = XlsxCodec.decode(&mut cx, &bytes, &decode_options).unwrap();

    assert!(report.dropped.iter().any(|loss| loss.field == "styles"));
    assert!(report.dropped.iter().any(|loss| loss.field == "mergeCells"));
    assert!(
        report
            .dropped
            .iter()
            .any(|loss| loss.field == "cell.A1.style")
    );
}

fn content_types_with_styles() -> String {
    concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        r#"<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
        r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
        r#"</Types>"#
    )
    .to_owned()
}

fn styles_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="{XMLNS_MAIN}"><fonts count="1"><font><b/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="0" fontId="0" fillId="0" borderId="0" applyFont="1"/></cellXfs></styleSheet>"#
    )
}

fn styled_worksheet_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{XMLNS_MAIN}" xmlns:sim="{XMLNS_SIM}"><sheetData><row r="1"><c r="A1" s="1" t="str" sim:kind="number"><v>1/1</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells></worksheet>"#
    )
}
