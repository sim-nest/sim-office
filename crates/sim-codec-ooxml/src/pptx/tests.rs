use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_deck::{Deck, Slide, SlideBlock, deck_to_doc, doc_to_deck};
use sim_lib_doc_core::{DocCodecOptions, DocId, ExternalRef};
use zip::ZipArchive;

use super::*;

fn test_context() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn codec_options(context: &mut Cx) -> DocCodecOptions {
    DocCodecOptions::new(context.factory().nil().unwrap())
}

#[test]
fn two_slide_deck_writes_required_parts_and_decodes_back() {
    let mut context = test_context();
    let mut deck = Deck::new("Project Update");
    let mut intro = Slide::new("intro", "Status");
    intro.push_block(SlideBlock::Heading("Status".to_owned()));
    intro.push_block(SlideBlock::BulletList(vec![
        "Local deck model".to_owned(),
        "OOXML export".to_owned(),
    ]));
    let mut numbers = Slide::new("numbers", "Numbers");
    numbers.push_block(SlideBlock::Table {
        columns: vec!["Metric".to_owned(), "Value".to_owned()],
        rows: vec![vec!["Budget".to_owned(), "On track".to_owned()]],
    });
    numbers.push_block(SlideBlock::ImageRef(ExternalRef::new(
        "site/msgraph",
        "drive-item-1",
        Some("etag-1".to_owned()),
        Some("https://example.com/image".to_owned()),
    )));
    deck.push_slide(intro);
    deck.push_slide(numbers);
    let doc = deck_to_doc(&mut context, DocId::new("deck-1"), &deck).unwrap();
    let options = codec_options(&mut context);
    let codec = PptxCodec;

    let (bytes, encode_report) = codec.encode(&mut context, &doc, &options).unwrap();
    let (decoded, decode_report) = codec.decode(&mut context, &bytes, &options).unwrap();
    let decoded_deck = doc_to_deck(&mut context, &decoded).unwrap();

    assert!(encode_report.is_lossless());
    assert!(decode_report.is_lossless());
    assert_eq!(decoded_deck, deck);
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut names = Vec::new();
    for index in 0..archive.len() {
        names.push(archive.by_index(index).unwrap().name().to_owned());
    }
    for required in [
        CONTENT_TYPES,
        ROOT_RELS,
        PRESENTATION,
        PRESENTATION_RELS,
        "ppt/slides/slide1.xml",
        "ppt/slides/_rels/slide1.xml.rels",
        "ppt/slides/slide2.xml",
        "ppt/slides/_rels/slide2.xml.rels",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "missing {required}"
        );
    }
}

#[test]
fn slide_relationship_order_is_used_and_orphans_are_reported() {
    let mut context = test_context();
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_TYPES.to_owned(), content_types(3));
    entries.insert(ROOT_RELS.to_owned(), root_rels());
    entries.insert(
        PRESENTATION.to_owned(),
        presentation_xml_with_order("Imported", &["rSecond", "rFirst"]),
    );
    entries.insert(
        PRESENTATION_RELS.to_owned(),
        presentation_rels_with_targets(&[
            ("rFirst", "slides/slide1.xml"),
            ("rSecond", "slides/slide2.xml"),
        ]),
    );
    entries.insert(
        "ppt/slides/slide1.xml".to_owned(),
        slide_xml(1, &Slide::new("first", "First")),
    );
    entries.insert(
        "ppt/slides/slide2.xml".to_owned(),
        slide_xml(2, &Slide::new("second", "Second")),
    );
    entries.insert(
        "ppt/slides/slide3.xml".to_owned(),
        slide_xml(3, &Slide::new("orphan", "Orphan")),
    );
    let bytes = write_package(entries).unwrap();
    let options = codec_options(&mut context);

    let (decoded, report) = PptxCodec.decode(&mut context, &bytes, &options).unwrap();
    let deck = doc_to_deck(&mut context, &decoded).unwrap();

    assert_eq!(deck.slides.len(), 2);
    assert_eq!(deck.slides[0].title, "Second");
    assert_eq!(deck.slides[1].title, "First");
    assert!(
        report
            .dropped
            .iter()
            .any(|loss| loss.field == "ppt/slides/slide3.xml.orphan")
    );
}

#[test]
fn unsupported_pptx_features_are_reported_as_dropped() {
    let mut context = test_context();
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_TYPES.to_owned(), content_types(1));
    entries.insert(ROOT_RELS.to_owned(), root_rels());
    entries.insert(
        PRESENTATION.to_owned(),
        presentation_xml(&Deck {
            title: "Imported".to_owned(),
            slides: vec![Slide::new("slide-1", "Imported")],
        }),
    );
    entries.insert(PRESENTATION_RELS.to_owned(), presentation_rels(1));
    entries.insert("ppt/slides/slide1.xml".to_owned(), unsupported_slide_xml());
    let bytes = write_package(entries).unwrap();
    let options = codec_options(&mut context);

    let (_doc, report) = PptxCodec.decode(&mut context, &bytes, &options).unwrap();

    assert!(
        report
            .dropped
            .iter()
            .any(|loss| loss.field == "ppt/slides/slide1.xml.transition")
    );
    assert!(
        report
            .dropped
            .iter()
            .any(|loss| loss.field == "ppt/slides/slide1.xml.media")
    );
    assert!(
        report
            .dropped
            .iter()
            .any(|loss| loss.field == "ppt/slides/slide1.xml.shapes")
    );
}

#[test]
fn recipes_export_embedded_books() {
    assert!(
        sim_lib_deck::RECIPES
            .iter()
            .any(|(path, _)| *path == "book.toml")
    );
}

fn presentation_xml_with_order(title: &str, rel_ids: &[&str]) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="{XMLNS_A}" xmlns:p="{XMLNS_P}" xmlns:r="{XMLNS_REL}" xmlns:sim="{XMLNS_SIM}" sim:title="{}"><p:sldIdLst>"#,
        escape_attr(title)
    );
    for (index, rel_id) in rel_ids.iter().enumerate() {
        xml.push_str(&format!(
            r#"<p:sldId id="{}" r:id="{rel_id}"/>"#,
            256 + index
        ));
    }
    xml.push_str(
        r#"</p:sldIdLst><p:sldSz cx="9144000" cy="5143500" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#,
    );
    xml
}

fn presentation_rels_with_targets(targets: &[(&str, &str)]) -> String {
    let mut xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#.to_owned();
    for (rel_id, target) in targets {
        xml.push_str(&format!(
            r#"<Relationship Id="{rel_id}" Type="{REL_SLIDE}" Target="{target}"/>"#
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

fn unsupported_slide_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="{XMLNS_A}" xmlns:p="{XMLNS_P}" xmlns:r="{XMLNS_REL}"><p:cSld name="Imported"><p:spTree>{}<p:sp><p:nvSpPr><p:cNvPr id="2" name="Freeform"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>Loose shape</a:t></a:r></a:p></p:txBody></p:sp><p:pic><p:nvPicPr><p:cNvPr id="3" name="Image"/></p:nvPicPr></p:pic></p:spTree></p:cSld><p:transition/></p:sld>"#,
        group_shape_xml()
    )
}
