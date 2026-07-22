//! ZIP package helpers for OOXML containers.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use sim_lib_doc_core::{OfficeError, ZipLimits, read_zip_entries};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const OLE_COMPOUND_HEADER: &[u8] = b"\xD0\xCF\x11\xE0";

pub(crate) const CONTENT_TYPES: &str = "[Content_Types].xml";
pub(crate) const ROOT_RELS: &str = "_rels/.rels";
pub(crate) const WORKBOOK: &str = "xl/workbook.xml";
pub(crate) const WORKBOOK_RELS: &str = "xl/_rels/workbook.xml.rels";
pub(crate) const WORKSHEET: &str = "xl/worksheets/sheet1.xml";
pub(crate) const PRESENTATION: &str = "ppt/presentation.xml";
pub(crate) const PRESENTATION_RELS: &str = "ppt/_rels/presentation.xml.rels";

/// In-memory OOXML package entries.
pub(crate) struct OoxmlPackage {
    entries: BTreeMap<String, Vec<u8>>,
}

impl OoxmlPackage {
    /// Reads a ZIP-backed OOXML package.
    pub(crate) fn read(bytes: &[u8], extension: &str) -> Result<Self, OfficeError> {
        Self::read_with_limits(bytes, extension, &ZipLimits::office())
    }

    pub(crate) fn read_with_limits(
        bytes: &[u8],
        extension: &str,
        limits: &ZipLimits,
    ) -> Result<Self, OfficeError> {
        if bytes.starts_with(OLE_COMPOUND_HEADER) {
            return Err(ole_error(extension));
        }
        let entries = read_zip_entries(bytes, limits)?;
        let package = Self { entries };
        package.require(CONTENT_TYPES)?;
        package.require(ROOT_RELS)?;
        Ok(package)
    }

    /// Borrows an entry as UTF-8 text.
    pub(crate) fn text(&self, name: &str) -> Result<&str, OfficeError> {
        let bytes = self.require(name)?;
        std::str::from_utf8(bytes)
            .map_err(|err| error(format!("zip entry {name} is not UTF-8 XML: {err}")))
    }

    /// Returns whether an entry exists.
    pub(crate) fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Returns sorted package entry names.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub(crate) fn require(&self, name: &str) -> Result<&[u8], OfficeError> {
        self.entries
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| error(format!("OOXML package is missing {name}")))
    }
}

/// Writes a ZIP-backed OOXML package from UTF-8 XML entries.
pub(crate) fn write_package(entries: BTreeMap<String, String>) -> Result<Vec<u8>, OfficeError> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, text) in entries {
            writer.start_file(name, options).map_err(zip_error)?;
            writer
                .write_all(text.as_bytes())
                .map_err(|err| error(format!("could not write zip entry: {err}")))?;
        }
        writer.finish().map_err(zip_error)?;
    }
    Ok(cursor.into_inner())
}

fn zip_error(error: zip::result::ZipError) -> OfficeError {
    self::error(format!("invalid OOXML zip package: {error}"))
}

fn ole_error(extension: &str) -> OfficeError {
    let message = match extension {
        ".xlsx" => ".xls binary workbooks are not supported; use .xlsx".to_owned(),
        ".pptx" => ".ppt binary presentations are not supported; use .pptx".to_owned(),
        other => format!("binary Office packages are not supported for {other}"),
    };
    error(message)
}

fn error(message: impl Into<String>) -> OfficeError {
    OfficeError::Kernel(message.into())
}
