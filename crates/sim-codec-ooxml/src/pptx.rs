//! `.pptx` codec implementation for the presentation deck domain.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use sim_kernel::Cx;
use sim_lib_deck::{DECK_DOC_KIND, Deck, Slide, SlideBlock, deck_to_doc, doc_to_deck};
use sim_lib_doc_core::{
    Doc, DocCodec, DocCodecOptions, DocId, DocKind, ExternalRef, FidelityReport, OfficeError,
};

use crate::package::{
    CONTENT_TYPES, OoxmlPackage, PRESENTATION, PRESENTATION_RELS, ROOT_RELS, write_package,
};

/// Stable codec id for local OOXML presentation packages.
pub const PPTX_CODEC_ID: &str = "codec/ooxml-pptx";
/// File extension accepted by this codec.
pub const PPTX_EXTENSION: &str = ".pptx";

const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_SLIDE_LAYOUT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_SLIDE_MASTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const XMLNS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const XMLNS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const XMLNS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XMLNS_SIM: &str = "https://sim.nest/office/ooxml";

mod decode;

#[cfg(test)]
mod tests;

/// Local `.pptx` codec for presentation deck documents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PptxCodec;

/// Builds the local OOXML presentation codec.
#[must_use]
pub fn pptx_codec() -> PptxCodec {
    PptxCodec
}

impl DocCodec for PptxCodec {
    fn codec_id(&self) -> &'static str {
        PPTX_CODEC_ID
    }

    fn kinds(&self) -> &'static [DocKind] {
        static KINDS: OnceLock<Vec<DocKind>> = OnceLock::new();
        KINDS
            .get_or_init(|| vec![DocKind::new(DECK_DOC_KIND)])
            .as_slice()
    }

    fn decode(
        &self,
        cx: &mut Cx,
        bytes: &[u8],
        _options: &DocCodecOptions,
    ) -> Result<(Doc, FidelityReport), OfficeError> {
        let package = OoxmlPackage::read(bytes, PPTX_EXTENSION)?;
        package.require(PRESENTATION)?;
        package.require(PRESENTATION_RELS)?;
        let (deck, report) = decode::decode_deck(&package)?;
        let mut doc = deck_to_doc(cx, DocId::new(format!("pptx:{}", deck.title)), &deck)
            .map_err(deck_error)?;
        doc.origin
            .push(ExternalRef::new(PPTX_CODEC_ID, PRESENTATION, None, None));
        Ok((doc, report))
    }

    fn encode(
        &self,
        cx: &mut Cx,
        doc: &Doc,
        _options: &DocCodecOptions,
    ) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
        let deck = doc_to_deck(cx, doc).map_err(deck_error)?;
        let entries = package_entries(&deck);
        let bytes = write_package(entries)?;
        Ok((bytes, FidelityReport::new(PPTX_CODEC_ID)))
    }
}

fn package_entries(deck: &Deck) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    entries.insert(CONTENT_TYPES.to_owned(), content_types(deck.slides.len()));
    entries.insert(ROOT_RELS.to_owned(), root_rels());
    entries.insert(PRESENTATION.to_owned(), presentation_xml(deck));
    entries.insert(
        PRESENTATION_RELS.to_owned(),
        presentation_rels(deck.slides.len()),
    );
    entries.insert("docProps/app.xml".to_owned(), app_props_xml());
    entries.insert("docProps/core.xml".to_owned(), core_props_xml(deck));
    entries.insert(
        "ppt/slideMasters/slideMaster1.xml".to_owned(),
        slide_master_xml(),
    );
    entries.insert(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_owned(),
        slide_master_rels(),
    );
    entries.insert(
        "ppt/slideLayouts/slideLayout1.xml".to_owned(),
        slide_layout_xml(),
    );
    entries.insert(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_owned(),
        slide_layout_rels(),
    );
    entries.insert("ppt/theme/theme1.xml".to_owned(), theme_xml());
    for (index, slide) in deck.slides.iter().enumerate() {
        let slide_number = index + 1;
        entries.insert(slide_path(slide_number), slide_xml(slide_number, slide));
        entries.insert(slide_rels_path(slide_number), slide_rels());
    }
    entries
}

fn content_types(slide_count: usize) -> String {
    let mut xml = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>"#,
        r#"<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>"#,
        r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>"#,
        r#"<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>"#,
        r#"<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#,
        r#"<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#
    )
    .to_owned();
    for index in 1..=slide_count {
        xml.push_str(&format!(
            r#"<Override PartName="/{}" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
            slide_path(index)
        ));
    }
    xml.push_str("</Types>");
    xml
}

fn root_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_OFFICE_DOCUMENT}" Target="ppt/presentation.xml"/></Relationships>"#
    )
}

fn presentation_xml(deck: &Deck) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="{XMLNS_A}" xmlns:p="{XMLNS_P}" xmlns:r="{XMLNS_REL}" xmlns:sim="{XMLNS_SIM}" sim:title="{}"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>"#,
        escape_attr(&deck.title)
    );
    for index in 1..=deck.slides.len() {
        xml.push_str(&format!(
            r#"<p:sldId id="{}" r:id="rId{}"/>"#,
            255 + index,
            index + 1
        ));
    }
    xml.push_str(
        r#"</p:sldIdLst><p:sldSz cx="9144000" cy="5143500" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#,
    );
    xml
}

fn presentation_rels(slide_count: usize) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_SLIDE_MASTER}" Target="slideMasters/slideMaster1.xml"/>"#
    );
    for index in 1..=slide_count {
        xml.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="{REL_SLIDE}" Target="slides/slide{index}.xml"/>"#,
            index + 1
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

fn slide_xml(index: usize, slide: &Slide) -> String {
    let mut shapes = group_shape_xml().to_owned();
    let mut metadata = String::new();
    let mut shape_id = 2_u32;
    shapes.push_str(&text_shape_xml(shape_id, "Title", &slide.title, 340_000));
    shape_id += 1;
    for block in &slide.blocks {
        metadata.push_str(&block_metadata_xml(block));
        shapes.push_str(&text_shape_xml(
            shape_id,
            "Content",
            &block_display_text(block),
            340_000 + (shape_id - 1) * 520_000,
        ));
        shape_id += 1;
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="{XMLNS_A}" xmlns:p="{XMLNS_P}" xmlns:r="{XMLNS_REL}" xmlns:sim="{XMLNS_SIM}" sim:id="{}" sim:title="{}"><p:cSld name="{}"><p:spTree>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr><p:extLst><p:ext uri="{{7A0DA402-A8FA-46A2-B652-91C2BB718A5D}}"><sim:blocks>{metadata}</sim:blocks><sim:index>{index}</sim:index></p:ext></p:extLst></p:sld>"#,
        escape_attr(&slide.id),
        escape_attr(&slide.title),
        escape_attr(&slide.title)
    )
}

fn group_shape_xml() -> &'static str {
    r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#
}

fn text_shape_xml(id: u32, name: &str, text: &str, y: u32) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{} {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="620000" y="{y}"/><a:ext cx="7900000" cy="420000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody></p:sp>"#,
        escape_attr(name),
        paragraphs_xml(text)
    )
}

fn paragraphs_xml(text: &str) -> String {
    text.lines()
        .map(|line| format!(r#"<a:p><a:r><a:t>{}</a:t></a:r></a:p>"#, escape_text(line)))
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

fn app_props_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>SIM</Application><PresentationFormat>On-screen Show (16:9)</PresentationFormat></Properties>"#.to_owned()
}

fn core_props_xml(deck: &Deck) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title></cp:coreProperties>"#,
        escape_text(&deck.title)
    )
}

fn slide_master_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="{XMLNS_A}" xmlns:p="{XMLNS_P}" xmlns:r="{XMLNS_REL}"><p:cSld><p:spTree>{}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#,
        group_shape_xml()
    )
}

fn slide_master_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_SLIDE_LAYOUT}" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{REL_THEME}" Target="../theme/theme1.xml"/></Relationships>"#
    )
}

fn slide_layout_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="{XMLNS_A}" xmlns:p="{XMLNS_P}" xmlns:r="{XMLNS_REL}" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        group_shape_xml()
    )
}

fn slide_layout_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_SLIDE_MASTER}" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
    )
}

fn theme_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="SIM"><a:themeElements><a:clrScheme name="SIM"><a:dk1><a:srgbClr val="111111"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="333333"/></a:dk2><a:lt2><a:srgbClr val="F2F2F2"/></a:lt2><a:accent1><a:srgbClr val="3465A4"/></a:accent1><a:accent2><a:srgbClr val="4E9A06"/></a:accent2><a:accent3><a:srgbClr val="CC0000"/></a:accent3><a:accent4><a:srgbClr val="75507B"/></a:accent4><a:accent5><a:srgbClr val="C17D11"/></a:accent5><a:accent6><a:srgbClr val="204A87"/></a:accent6><a:hlink><a:srgbClr val="3465A4"/></a:hlink><a:folHlink><a:srgbClr val="75507B"/></a:folHlink></a:clrScheme><a:fontScheme name="SIM"><a:majorFont><a:latin typeface="Aptos Display"/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/></a:minorFont></a:fontScheme><a:fmtScheme name="SIM"><a:fillStyleLst/><a:lnStyleLst/><a:effectStyleLst/><a:bgFillStyleLst/></a:fmtScheme></a:themeElements></a:theme>"#.to_owned()
}

fn slide_path(index: usize) -> String {
    format!("ppt/slides/slide{index}.xml")
}

fn slide_rels_path(index: usize) -> String {
    format!("ppt/slides/_rels/slide{index}.xml.rels")
}

fn slide_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_SLIDE_LAYOUT}" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#
    )
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

fn deck_error(error: impl std::fmt::Display) -> OfficeError {
    OfficeError::Kernel(error.to_string())
}

fn error(message: impl Into<String>) -> OfficeError {
    OfficeError::Kernel(message.into())
}
