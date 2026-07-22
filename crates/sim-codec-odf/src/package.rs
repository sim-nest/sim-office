//! ODF ZIP package helpers.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use roxmltree::{Document, Node};
use sim_lib_doc_core::{FidelityReport, OfficeError, ZipLimits, read_zip_entries};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub(crate) const CONTENT_XML: &str = "content.xml";
pub(crate) const STYLES_XML: &str = "styles.xml";
pub(crate) const MANIFEST_XML: &str = "META-INF/manifest.xml";

pub(crate) const ODS_MIMETYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";
pub(crate) const ODP_MIMETYPE: &str = "application/vnd.oasis.opendocument.presentation";

pub(crate) const OFFICE_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
pub(crate) const TABLE_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
pub(crate) const DRAW_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
pub(crate) const PRESENTATION_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0";
pub(crate) const TEXT_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
pub(crate) const SIM_NS: &str = "https://sim.nest/office/odf";

pub(crate) struct OdfPackage {
    mimetype: String,
    entries: BTreeMap<String, Vec<u8>>,
}

impl OdfPackage {
    pub(crate) fn read(bytes: &[u8]) -> Result<Self, OfficeError> {
        Self::read_with_limits(bytes, &ZipLimits::office())
    }

    pub(crate) fn read_with_limits(bytes: &[u8], limits: &ZipLimits) -> Result<Self, OfficeError> {
        validate_mimetype_first(bytes)?;
        let entries = read_zip_entries(bytes, limits)?;

        let mimetype = std::str::from_utf8(
            entries
                .get("mimetype")
                .ok_or_else(|| error("ODF package is missing mimetype"))?,
        )
        .map_err(|err| error(format!("ODF mimetype entry is not UTF-8: {err}")))?
        .trim()
        .to_owned();

        let package = Self { mimetype, entries };
        package.require(CONTENT_XML)?;
        package.require(MANIFEST_XML)?;
        Ok(package)
    }

    pub(crate) fn mimetype(&self) -> &str {
        &self.mimetype
    }

    pub(crate) fn text(&self, name: &str) -> Result<&str, OfficeError> {
        let bytes = self.require(name)?;
        std::str::from_utf8(bytes)
            .map_err(|err| error(format!("zip entry {name} is not UTF-8 XML: {err}")))
    }

    pub(crate) fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub(crate) fn require(&self, name: &str) -> Result<&[u8], OfficeError> {
        self.entries
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| error(format!("ODF package is missing {name}")))
    }
}

pub(crate) fn write_package(
    mimetype: &str,
    entries: BTreeMap<String, String>,
) -> Result<Vec<u8>, OfficeError> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file("mimetype", stored).map_err(zip_error)?;
        writer
            .write_all(mimetype.as_bytes())
            .map_err(|err| error(format!("could not write mimetype entry: {err}")))?;

        let compressed =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, text) in entries {
            if name == "mimetype" {
                continue;
            }
            writer.start_file(name, compressed).map_err(zip_error)?;
            writer
                .write_all(text.as_bytes())
                .map_err(|err| error(format!("could not write zip entry: {err}")))?;
        }
        writer.finish().map_err(zip_error)?;
    }
    Ok(cursor.into_inner())
}

pub(crate) fn manifest_xml(mimetype: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2"><manifest:file-entry manifest:media-type="{mimetype}" manifest:full-path="/"/><manifest:file-entry manifest:media-type="text/xml" manifest:full-path="content.xml"/><manifest:file-entry manifest:media-type="text/xml" manifest:full-path="styles.xml"/></manifest:manifest>"#
    )
}

pub(crate) fn styles_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles xmlns:office="{OFFICE_NS}" xmlns:text="{TEXT_NS}" office:version="1.2"><office:styles/></office:document-styles>"#
    )
}

pub(crate) fn parse_xml<'a>(text: &'a str, label: &str) -> Result<Document<'a>, OfficeError> {
    Document::parse(text).map_err(|err| error(format!("could not parse {label} XML: {err}")))
}

pub(crate) fn attr_ns<'a>(node: Node<'a, '_>, namespace: &str, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name && attribute.namespace() == Some(namespace))
        .map(|attribute| attribute.value())
}

pub(crate) fn attr_any<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

pub(crate) fn text_content(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|descendant| descendant.has_tag_name("p"))
        .filter_map(|descendant| descendant.text())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn add_loss(
    report: &mut FidelityReport,
    field: impl Into<String>,
    reason: impl Into<String>,
) {
    let current = std::mem::take(report);
    *report = current.with_dropped(field, reason);
}

pub(crate) fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn escape_attr(text: &str) -> String {
    escape_text(text)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn error(message: impl Into<String>) -> OfficeError {
    OfficeError::Kernel(message.into())
}

fn zip_error(error: zip::result::ZipError) -> OfficeError {
    self::error(format!("invalid ODF zip package: {error}"))
}

fn validate_mimetype_first(bytes: &[u8]) -> Result<(), OfficeError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if archive.is_empty() {
        return Err(error("ODF package is empty"));
    }
    let first = archive.by_index(0).map_err(zip_error)?;
    let first_name = first.name().replace('\\', "/");
    if first_name != "mimetype" || first.compression() != CompressionMethod::Stored {
        return Err(error(
            "ODF package must put the uncompressed mimetype entry first",
        ));
    }
    Ok(())
}
