//! `.odp` codec implementation for the presentation deck domain.

use std::collections::BTreeMap;

use roxmltree::{Document, Node};
use sim_kernel::Cx;
use sim_lib_deck::{Deck, Slide, SlideBlock, deck_to_doc, doc_to_deck};
use sim_lib_doc_core::{Doc, DocId, ExternalRef, FidelityReport, OfficeError};

use crate::ODF_CODEC_ID;
use crate::package::{
    CONTENT_XML, DRAW_NS, MANIFEST_XML, ODP_MIMETYPE, OdfPackage, PRESENTATION_NS, SIM_NS,
    STYLES_XML, add_loss, attr_any, attr_ns, error, escape_attr, escape_text, manifest_xml,
    parse_xml, styles_xml, write_package,
};

pub(crate) fn decode(
    cx: &mut Cx,
    package: &OdfPackage,
) -> Result<(Doc, FidelityReport), OfficeError> {
    let content_xml = package.text(CONTENT_XML)?;
    let document = parse_xml(content_xml, "presentation content")?;
    let mut report = FidelityReport::new(ODF_CODEC_ID);
    report_styles(package, &document, &mut report)?;
    let title = document
        .descendants()
        .find(|node| node.has_tag_name("presentation"))
        .and_then(|node| attr_ns(node, SIM_NS, "title").or_else(|| attr_any(node, "title")))
        .unwrap_or("Presentation");
    let mut deck = Deck::new(title);
    for (index, page) in document
        .descendants()
        .filter(|node| node.has_tag_name("page"))
        .enumerate()
    {
        deck.push_slide(decode_slide(index + 1, page, &mut report)?);
    }
    let mut doc =
        deck_to_doc(cx, DocId::new(format!("odp:{}", deck.title)), &deck).map_err(deck_error)?;
    doc.origin
        .push(ExternalRef::new(ODF_CODEC_ID, CONTENT_XML, None, None));
    Ok((doc, report))
}

pub(crate) fn encode(cx: &mut Cx, doc: &Doc) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
    let deck = doc_to_deck(cx, doc).map_err(deck_error)?;
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_XML.to_owned(), content_xml(&deck));
    entries.insert(STYLES_XML.to_owned(), styles_xml());
    entries.insert(MANIFEST_XML.to_owned(), manifest_xml(ODP_MIMETYPE));
    let bytes = write_package(ODP_MIMETYPE, entries)?;
    Ok((bytes, FidelityReport::new(ODF_CODEC_ID)))
}

fn content_xml(deck: &Deck) -> String {
    let slides = deck
        .slides
        .iter()
        .enumerate()
        .map(|(index, slide)| slide_xml(index + 1, slide))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="{DRAW_NS}" xmlns:presentation="{PRESENTATION_NS}" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:sim="{SIM_NS}" office:version="1.2"><office:body><office:presentation sim:title="{}">{slides}</office:presentation></office:body></office:document-content>"#,
        escape_attr(&deck.title)
    )
}

fn slide_xml(index: usize, slide: &Slide) -> String {
    let mut xml = format!(
        r#"<draw:page draw:name="slide-{index}" sim:id="{}" sim:title="{}" presentation:use-header-name="false">"#,
        escape_attr(&slide.id),
        escape_attr(&slide.title)
    );
    xml.push_str(&text_frame_xml("title", &slide.title, "0.7in"));
    for (block_index, block) in slide.blocks.iter().enumerate() {
        xml.push_str(&text_frame_xml(
            &format!("block-{}", block_index + 1),
            &block_display_text(block),
            &format!("{}in", block_index + 2),
        ));
    }
    xml.push_str("<presentation:notes><sim:blocks>");
    for block in &slide.blocks {
        xml.push_str(&block_metadata_xml(block));
    }
    xml.push_str("</sim:blocks></presentation:notes></draw:page>");
    xml
}

fn text_frame_xml(name: &str, text: &str, y: &str) -> String {
    format!(
        r#"<draw:frame draw:name="{}" svg:x="0.7in" svg:y="{y}" svg:width="9in" svg:height="0.7in"><draw:text-box>{}</draw:text-box></draw:frame>"#,
        escape_attr(name),
        paragraphs_xml(text)
    )
}

fn paragraphs_xml(text: &str) -> String {
    text.lines()
        .map(|line| format!(r#"<text:p>{}</text:p>"#, escape_text(line)))
        .collect()
}

fn block_metadata_xml(block: &SlideBlock) -> String {
    match block {
        SlideBlock::Heading(value) => format!(
            r#"<sim:block kind="heading"><sim:value>{}</sim:value></sim:block>"#,
            escape_text(value)
        ),
        SlideBlock::BulletList(items) => format!(
            r#"<sim:block kind="bullet-list">{}</sim:block>"#,
            item_list_xml(items)
        ),
        SlideBlock::Table { columns, rows } => format!(
            r#"<sim:block kind="table"><sim:columns>{}</sim:columns><sim:rows>{}</sim:rows></sim:block>"#,
            item_list_xml(columns),
            rows_xml(rows)
        ),
        SlideBlock::ImageRef(reference) => {
            let version = optional_attr("version", &reference.version);
            let web_url = optional_attr("web-url", &reference.web_url);
            format!(
                r#"<sim:block kind="image-ref" backend="{}" external-id="{}"{version}{web_url}/>"#,
                escape_attr(&reference.backend),
                escape_attr(&reference.external_id)
            )
        }
    }
}

fn item_list_xml(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!(r#"<sim:item>{}</sim:item>"#, escape_text(item)))
        .collect()
}

fn rows_xml(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| format!(r#"<sim:row>{}</sim:row>"#, item_list_xml(row)))
        .collect()
}

fn optional_attr(name: &str, value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!(r#" {name}="{}""#, escape_attr(value)))
        .unwrap_or_default()
}

fn block_display_text(block: &SlideBlock) -> String {
    match block {
        SlideBlock::Heading(value) => value.clone(),
        SlideBlock::BulletList(items) => items
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
        SlideBlock::Table { columns, rows } => {
            let mut lines = vec![columns.join(" | ")];
            lines.extend(rows.iter().map(|row| row.join(" | ")));
            lines.join("\n")
        }
        SlideBlock::ImageRef(reference) => format!("[image: {}]", reference.external_id),
    }
}

fn decode_slide(
    index: usize,
    page: Node<'_, '_>,
    report: &mut FidelityReport,
) -> Result<Slide, OfficeError> {
    report_unsupported(index, page, report);
    let id = attr_ns(page, SIM_NS, "id")
        .or_else(|| attr_ns(page, DRAW_NS, "name"))
        .or_else(|| attr_any(page, "name"))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("slide-{index}"));
    let title = attr_ns(page, SIM_NS, "title")
        .or_else(|| attr_ns(page, DRAW_NS, "name"))
        .or_else(|| attr_any(page, "name"))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Slide {index}"));
    let blocks = page
        .descendants()
        .filter(|node| node.has_tag_name("block"))
        .map(block_from_node)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Slide { id, title, blocks })
}

fn report_unsupported(index: usize, page: Node<'_, '_>, report: &mut FidelityReport) {
    let has_sim_blocks = page.descendants().any(|node| node.has_tag_name("block"));
    if attr_ns(page, PRESENTATION_NS, "transition-style").is_some()
        || page
            .descendants()
            .any(|node| node.has_tag_name("animations") || node.has_tag_name("animation"))
    {
        add_loss(
            report,
            format!("slide.{index}.transition"),
            "ODF slide transitions are not represented in the portable deck model",
        );
    }
    if !has_sim_blocks && page.descendants().any(|node| node.has_tag_name("frame")) {
        add_loss(
            report,
            format!("slide.{index}.frames"),
            "ODF drawing frames outside the SIM deck subset are not decoded",
        );
    }
}

fn block_from_node(node: Node<'_, '_>) -> Result<SlideBlock, OfficeError> {
    match attr_any(node, "kind").unwrap_or_default() {
        "heading" => Ok(SlideBlock::Heading(child_text(node, "value"))),
        "bullet-list" => Ok(SlideBlock::BulletList(child_texts(node, "item"))),
        "table" => Ok(SlideBlock::Table {
            columns: node
                .children()
                .find(|child| child.has_tag_name("columns"))
                .map(|columns| child_texts(columns, "item"))
                .unwrap_or_default(),
            rows: node
                .children()
                .find(|child| child.has_tag_name("rows"))
                .map(|rows| {
                    rows.children()
                        .filter(|child| child.has_tag_name("row"))
                        .map(|row| child_texts(row, "item"))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "image-ref" => Ok(SlideBlock::ImageRef(ExternalRef::new(
            attr_any(node, "backend").unwrap_or(ODF_CODEC_ID),
            attr_any(node, "external-id").unwrap_or_default(),
            attr_any(node, "version").map(str::to_owned),
            attr_any(node, "web-url").map(str::to_owned),
        ))),
        other => Err(error(format!("unsupported deck block kind {other}"))),
    }
}

fn child_text(node: Node<'_, '_>, tag: &str) -> String {
    node.children()
        .find(|child| child.has_tag_name(tag))
        .and_then(|child| child.text())
        .unwrap_or_default()
        .to_owned()
}

fn child_texts(node: Node<'_, '_>, tag: &str) -> Vec<String> {
    node.children()
        .filter(|child| child.has_tag_name(tag))
        .filter_map(|child| child.text())
        .map(str::to_owned)
        .collect()
}

fn report_styles(
    package: &OdfPackage,
    document: &Document<'_>,
    report: &mut FidelityReport,
) -> Result<(), OfficeError> {
    let content_has_styles = document
        .descendants()
        .any(|node| attr_any(node, "style-name").is_some());
    let style_file_has_styles =
        package.has(STYLES_XML) && package.text(STYLES_XML)?.contains("<style:style");
    if content_has_styles || style_file_has_styles {
        add_loss(
            report,
            "styles",
            "ODF styles are not represented in the portable deck model",
        );
    }
    Ok(())
}

fn deck_error(error: impl std::fmt::Display) -> OfficeError {
    OfficeError::Kernel(error.to_string())
}
