use std::collections::{BTreeMap, BTreeSet};

use roxmltree::{Document, Node};
use sim_lib_deck::{Deck, Slide, SlideBlock};
use sim_lib_doc_core::{ExternalRef, FidelityReport, OfficeError};

use crate::package::{OoxmlPackage, PRESENTATION, PRESENTATION_RELS};

use super::{PPTX_CODEC_ID, REL_SLIDE, XMLNS_REL, error};

pub(super) fn decode_deck(package: &OoxmlPackage) -> Result<(Deck, FidelityReport), OfficeError> {
    let presentation_xml = package.text(PRESENTATION)?;
    let mut report = FidelityReport::new(PPTX_CODEC_ID);
    let title = presentation_title(presentation_xml)?;
    let paths = slide_paths(package, presentation_xml, &mut report)?;
    let mut deck = Deck::new(title);
    for (index, path) in paths.iter().enumerate() {
        let slide = decode_slide(index + 1, path, package.text(path)?, &mut report)?;
        deck.push_slide(slide);
    }
    Ok((deck, report))
}

fn presentation_title(xml: &str) -> Result<String, OfficeError> {
    let document = parse_xml(xml, "presentation")?;
    Ok(attr(document.root_element(), "title")
        .unwrap_or("Presentation")
        .to_owned())
}

fn slide_paths(
    package: &OoxmlPackage,
    presentation_xml: &str,
    report: &mut FidelityReport,
) -> Result<Vec<String>, OfficeError> {
    let rel_ids = presentation_slide_rel_ids(presentation_xml)?;
    let relationships = slide_relationships(package.text(PRESENTATION_RELS)?)?;
    let mut paths = Vec::new();
    for rel_id in rel_ids {
        let target = relationships.get(&rel_id).ok_or_else(|| {
            error(format!(
                "presentation slide relationship {rel_id} does not resolve to a slide"
            ))
        })?;
        paths.push(resolve_presentation_target(target));
    }
    report_orphan_slide_parts(package, &paths, report);
    Ok(paths)
}

fn presentation_slide_rel_ids(presentation_xml: &str) -> Result<Vec<String>, OfficeError> {
    let document = parse_xml(presentation_xml, "presentation")?;
    Ok(document
        .descendants()
        .filter(|node| node.has_tag_name("sldId"))
        .filter_map(|node| attr_ns(node, XMLNS_REL, "id").map(str::to_owned))
        .collect())
}

fn slide_relationships(rels_xml: &str) -> Result<BTreeMap<String, String>, OfficeError> {
    let document = parse_xml(rels_xml, "presentation relationships")?;
    let mut relationships = BTreeMap::new();
    for relationship in document
        .descendants()
        .filter(|node| node.has_tag_name("Relationship"))
    {
        if attr(relationship, "Type") != Some(REL_SLIDE) {
            continue;
        }
        let id = attr(relationship, "Id")
            .ok_or_else(|| error("slide relationship is missing Id"))?
            .to_owned();
        let target = attr(relationship, "Target")
            .ok_or_else(|| error(format!("slide relationship {id} has no Target")))?
            .to_owned();
        relationships.insert(id, target);
    }
    Ok(relationships)
}

fn resolve_presentation_target(target: &str) -> String {
    let target = target.replace('\\', "/");
    if let Some(stripped) = target.strip_prefix('/') {
        stripped.to_owned()
    } else if target.starts_with("ppt/") {
        target
    } else {
        format!("ppt/{target}")
    }
}

fn report_orphan_slide_parts(
    package: &OoxmlPackage,
    decoded_paths: &[String],
    report: &mut FidelityReport,
) {
    let decoded_paths = decoded_paths
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for path in package.names().filter(|name| {
        name.starts_with("ppt/slides/slide") && name.ends_with(".xml") && !name.contains("/_rels/")
    }) {
        if !decoded_paths.contains(path) {
            add_loss(
                report,
                format!("{path}.orphan"),
                "slide part is not referenced by presentation.xml relationships",
            );
        }
    }
}

fn decode_slide(
    index: usize,
    path: &str,
    xml: &str,
    report: &mut FidelityReport,
) -> Result<Slide, OfficeError> {
    let document = parse_xml(xml, path)?;
    let root = document.root_element();
    report_unsupported(path, &document, report);
    let id = attr(root, "id")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("slide-{index}"));
    let title = attr(root, "title")
        .or_else(|| attr(root, "name"))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Slide {index}"));
    let blocks = root
        .descendants()
        .filter(|node| node.has_tag_name("block"))
        .map(block_from_node)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Slide { id, title, blocks })
}

fn report_unsupported(path: &str, document: &Document<'_>, report: &mut FidelityReport) {
    let has_sim_blocks = document
        .descendants()
        .any(|node| node.has_tag_name("block"));
    if document
        .descendants()
        .any(|node| node.has_tag_name("transition"))
    {
        add_loss(
            report,
            format!("{path}.transition"),
            "slide transitions are not represented in the portable deck model",
        );
    }
    if document.descendants().any(|node| {
        node.has_tag_name("pic") || node.has_tag_name("audio") || node.has_tag_name("video")
    }) {
        add_loss(
            report,
            format!("{path}.media"),
            "embedded media is represented only by explicit external image references",
        );
    }
    if !has_sim_blocks
        && document
            .descendants()
            .any(|node| node.has_tag_name("sp") || node.has_tag_name("graphicFrame"))
    {
        add_loss(
            report,
            format!("{path}.shapes"),
            "presentation shapes outside the SIM deck subset are not decoded",
        );
    }
}

fn block_from_node(node: Node<'_, '_>) -> Result<SlideBlock, OfficeError> {
    match attr(node, "kind").unwrap_or_default() {
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
            attr(node, "backend").unwrap_or(PPTX_CODEC_ID),
            attr(node, "external-id").unwrap_or_default(),
            attr(node, "version").map(str::to_owned),
            attr(node, "web-url").map(str::to_owned),
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

fn parse_xml<'a>(text: &'a str, label: &str) -> Result<Document<'a>, OfficeError> {
    Document::parse(text).map_err(|err| error(format!("could not parse {label} XML: {err}")))
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn attr_ns<'a>(node: Node<'a, '_>, namespace: &str, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name && attribute.namespace() == Some(namespace))
        .map(|attribute| attribute.value())
}

fn add_loss(report: &mut FidelityReport, field: impl Into<String>, reason: impl Into<String>) {
    let current = std::mem::take(report);
    *report = current.with_dropped(field, reason);
}
