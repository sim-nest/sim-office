use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_deck::{Deck, Slide, SlideBlock, deck_to_doc, doc_to_deck};
use sim_lib_doc_core::{DocCodec, DocCodecOptions, DocId, ExternalRef};
use sim_lib_sheet::{
    CellRef, CellValue, Sheet, doc_to_sheet, rational_from_str, rational_to_canonical, sheet_to_doc,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::*;
use crate::package::{
    CONTENT_XML, MANIFEST_XML, ODP_MIMETYPE, ODS_MIMETYPE, OFFICE_NS, STYLES_XML, TABLE_NS,
    manifest_xml, styles_xml, write_package,
};

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn options(cx: &mut Cx) -> DocCodecOptions {
    DocCodecOptions::new(cx.factory().nil().unwrap())
}

#[test]
fn ods_round_trips_sheet_and_writes_mimetype_first() {
    let mut cx = cx();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set_cell(
        CellRef::parse("A1").unwrap(),
        CellValue::Number(rational_from_str(&mut cx, "5/2").unwrap()),
    );
    sheet.set_cell(
        CellRef::parse("C2").unwrap(),
        CellValue::Text("ready".to_owned()),
    );
    sheet.set_cell(CellRef::parse("D3").unwrap(), CellValue::Bool(true));
    let doc = sheet_to_doc(&mut cx, DocId::new("sheet-1"), &sheet).unwrap();
    let codec = OdfCodec;

    let encode_options = options(&mut cx);
    let (bytes, encode_report) = codec.encode(&mut cx, &doc, &encode_options).unwrap();
    assert!(encode_report.is_lossless());
    assert_mimetype_first(&bytes, ODS_MIMETYPE);

    let decode_options = options(&mut cx);
    let (decoded, decode_report) = codec.decode(&mut cx, &bytes, &decode_options).unwrap();
    assert!(decode_report.is_lossless());
    let decoded_sheet = doc_to_sheet(&mut cx, &decoded).unwrap();
    let CellValue::Number(a1) = decoded_sheet.cell(&CellRef::parse("A1").unwrap()) else {
        panic!("expected A1 number");
    };
    assert_eq!(rational_to_canonical(&mut cx, &a1).unwrap(), "5/2");
    assert!(matches!(
        decoded_sheet.cell(&CellRef::parse("C2").unwrap()),
        CellValue::Text(text) if text == "ready"
    ));
    assert!(matches!(
        decoded_sheet.cell(&CellRef::parse("D3").unwrap()),
        CellValue::Bool(true)
    ));
}

#[test]
fn odp_round_trips_deck() {
    let mut cx = cx();
    let mut deck = Deck::new("Project Update");
    let mut slide = Slide::new("intro", "Status");
    slide.push_block(SlideBlock::Heading("Status".to_owned()));
    slide.push_block(SlideBlock::BulletList(vec![
        "ODF export".to_owned(),
        "Portable deck blocks".to_owned(),
    ]));
    slide.push_block(SlideBlock::Table {
        columns: vec!["Metric".to_owned(), "Value".to_owned()],
        rows: vec![vec!["Budget".to_owned(), "Green".to_owned()]],
    });
    slide.push_block(SlideBlock::ImageRef(ExternalRef::new(
        "site/msgraph",
        "drive-item-1",
        Some("etag-1".to_owned()),
        Some("https://example.com/image".to_owned()),
    )));
    deck.push_slide(slide);
    let doc = deck_to_doc(&mut cx, DocId::new("deck-1"), &deck).unwrap();
    let codec = OdfCodec;

    let encode_options = options(&mut cx);
    let (bytes, encode_report) = codec.encode(&mut cx, &doc, &encode_options).unwrap();
    assert!(encode_report.is_lossless());
    assert_mimetype_first(&bytes, ODP_MIMETYPE);

    let decode_options = options(&mut cx);
    let (decoded, decode_report) = codec.decode(&mut cx, &bytes, &decode_options).unwrap();
    let decoded_deck = doc_to_deck(&mut cx, &decoded).unwrap();

    assert!(decode_report.is_lossless());
    assert_eq!(decoded_deck, deck);
}

#[test]
fn compressed_first_mimetype_is_rejected() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let bytes = package_with_compressed_mimetype();

    let err = OdfCodec
        .decode(&mut cx, &bytes, &decode_options)
        .unwrap_err();

    assert!(err.to_string().contains("uncompressed mimetype"));
}

#[test]
fn ods_formula_without_sim_metadata_preserves_formula() {
    let mut cx = cx();
    let bytes = ods_package(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="{OFFICE_NS}" xmlns:table="{TABLE_NS}" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2"><office:body><office:spreadsheet><table:table table:name="Formulas"><table:table-row><table:table-cell office:value-type="string" table:formula="of:=A1+B1"><text:p>999</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
    ));
    let decode_options = options(&mut cx);

    let (decoded, report) = OdfCodec.decode(&mut cx, &bytes, &decode_options).unwrap();
    let sheet = doc_to_sheet(&mut cx, &decoded).unwrap();

    assert!(report.is_lossless());
    assert!(matches!(
        sheet.cell(&CellRef::parse("A1").unwrap()),
        CellValue::Formula(formula) if formula == "=A1+B1"
    ));
}

#[test]
fn recipes_are_embedded() {
    let cards = sim_cookbook::recipes_from_embedded(RECIPES).unwrap();

    assert!(cards.iter().any(|card| card.id.ends_with("ods-round-trip")));
    assert!(cards.iter().any(|card| card.id.ends_with("odp-round-trip")));
}

fn ods_package(content_xml: &str) -> Vec<u8> {
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_XML.to_owned(), content_xml.to_owned());
    entries.insert(STYLES_XML.to_owned(), styles_xml());
    entries.insert(MANIFEST_XML.to_owned(), manifest_xml(ODS_MIMETYPE));
    write_package(ODS_MIMETYPE, entries).unwrap()
}

fn assert_mimetype_first(bytes: &[u8], expected: &str) {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    {
        let mut first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
        let mut value = String::new();
        std::io::Read::read_to_string(&mut first, &mut value).unwrap();
        assert_eq!(value, expected);
    }

    let mut names = Vec::new();
    for index in 0..archive.len() {
        names.push(archive.by_index(index).unwrap().name().to_owned());
    }
    for required in [CONTENT_XML, MANIFEST_XML, STYLES_XML] {
        assert!(
            names.iter().any(|name| name == required),
            "missing {required}"
        );
    }
}

fn package_with_compressed_mimetype() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let compressed =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("mimetype", compressed).unwrap();
        writer.write_all(ODS_MIMETYPE.as_bytes()).unwrap();
        writer.start_file(CONTENT_XML, compressed).unwrap();
        writer.write_all(b"<office:document-content/>").unwrap();
        writer.finish().unwrap();
    }
    cursor.into_inner()
}
