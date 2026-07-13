//! Microsoft Graph file transport for presentation decks.
//!
//! Microsoft Graph carries PowerPoint support here as `.pptx` drive item bytes.
//! Deck object decoding and encoding stay in the OOXML codec crate; this module
//! only validates file-shaped transport and builds preview upload edits.

use std::collections::BTreeSet;
use std::io::Cursor;

use sim_kernel::{Cx, Expr};
use sim_lib_doc_core::{DocId, Edit, ExternalRef};
use zip::ZipArchive;

use crate::model::{Deck, DeckError, entry, map, office_symbol};

/// Domain namespace used by preview edits for Graph-backed deck files.
pub const GRAPH_DECK_FILE_EDIT_DOMAIN: &str = "office/deck/graph-file";

/// PowerPoint Open XML MIME type.
pub const PPTX_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation";

const FIELD_BACKEND: &str = "backend";
const FIELD_CONTENT_TYPE: &str = "content-type";
const FIELD_DECK_TITLE: &str = "deck-title";
const FIELD_EXTENSION: &str = "extension";
const FIELD_EXTERNAL_ID: &str = "external-id";
const FIELD_INVERSE: &str = "inverse";
const FIELD_KIND: &str = "kind";
const FIELD_PREVIEW_ONLY: &str = "preview-only";
const FIELD_SLIDE_COUNT: &str = "slide-count";
const FIELD_TARGET_FOLDER: &str = "target-folder";
const FIELD_VERSION: &str = "version";
const FIELD_WEB_URL: &str = "web-url";
const OP_UPLOAD_DECK_FILE: &str = "upload-deck-file";
const OP_UPLOAD_DECK_FILE_INVERSE: &str = "upload-deck-file-inverse";
const PPTX_EXTENSION: &str = ".pptx";
const OLE_COMPOUND_HEADER: &[u8] = b"\xD0\xCF\x11\xE0";

const REQUIRED_PPTX_ENTRIES: [&str; 4] = [
    "[Content_Types].xml",
    "_rels/.rels",
    "ppt/presentation.xml",
    "ppt/_rels/presentation.xml.rels",
];

/// Minimal Microsoft Graph file seam implemented by host Graph adapters.
pub trait MsGraphSite {
    /// Runs a site-local Microsoft Graph GET and returns raw response bytes.
    fn graph_get_bytes(&self, cx: &mut Cx, path: &str) -> Result<Vec<u8>, DeckError>;
}

/// Builds the Microsoft Graph content path for a PowerPoint drive item.
pub fn deck_file_content_path(drive_item: &str) -> Result<String, DeckError> {
    let drive_item = drive_item.trim();
    if drive_item.is_empty() {
        return Err(DeckError::GraphFile("drive item id is empty".to_owned()));
    }
    Ok(format!(
        "/me/drive/items/{}/content",
        path_segment(drive_item)
    ))
}

/// Downloads a Graph drive item and verifies that it is a `.pptx` package.
pub fn download_deck_file(
    cx: &mut Cx,
    site: &dyn MsGraphSite,
    drive_item: &str,
) -> Result<Vec<u8>, DeckError> {
    let path = deck_file_content_path(drive_item)?;
    let bytes = site.graph_get_bytes(cx, &path)?;
    validate_pptx_package(&bytes)?;
    Ok(bytes)
}

/// Builds a preview edit for uploading a deck file to a Graph folder.
pub fn plan_upload_deck_file(
    cx: &mut Cx,
    deck: &Deck,
    target_folder: &ExternalRef,
) -> Result<Edit, DeckError> {
    validate_upload_target(deck, target_folder)?;
    let op = upload_plan_expr(deck, target_folder, OP_UPLOAD_DECK_FILE);
    let inverse = map(vec![
        entry(
            FIELD_KIND,
            Expr::Symbol(office_symbol(OP_UPLOAD_DECK_FILE_INVERSE)),
        ),
        entry(FIELD_TARGET_FOLDER, target_folder_expr(target_folder)),
        entry(FIELD_INVERSE, Expr::Nil),
    ]);
    Ok(Edit::new(
        DocId::new(upload_doc_id(deck, target_folder)),
        GRAPH_DECK_FILE_EDIT_DOMAIN,
        cx.factory().expr(op)?,
        cx.factory().expr(inverse)?,
    ))
}

/// Verifies that bytes have the required PowerPoint Open XML package parts.
pub fn validate_pptx_package(bytes: &[u8]) -> Result<(), DeckError> {
    if bytes.starts_with(OLE_COMPOUND_HEADER) {
        return Err(DeckError::GraphFile(
            ".ppt binary presentations are not supported; use .pptx".to_owned(),
        ));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| DeckError::GraphFile(format!("invalid .pptx zip package: {error}")))?;
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| DeckError::GraphFile(format!("invalid .pptx entry: {error}")))?;
        if !file.is_dir() {
            names.insert(file.name().replace('\\', "/"));
        }
    }
    for required in REQUIRED_PPTX_ENTRIES {
        if !names.contains(required) {
            return Err(DeckError::GraphFile(format!(
                ".pptx package is missing {required}"
            )));
        }
    }
    Ok(())
}

fn validate_upload_target(deck: &Deck, target_folder: &ExternalRef) -> Result<(), DeckError> {
    if deck.title.trim().is_empty() {
        return Err(DeckError::InvalidDeck("deck title is empty".to_owned()));
    }
    if target_folder.external_id.trim().is_empty() {
        return Err(DeckError::GraphFile(
            "target folder external id is empty".to_owned(),
        ));
    }
    Ok(())
}

fn upload_plan_expr(deck: &Deck, target_folder: &ExternalRef, kind: &'static str) -> Expr {
    map(vec![
        entry(FIELD_KIND, Expr::Symbol(office_symbol(kind))),
        entry(FIELD_TARGET_FOLDER, target_folder_expr(target_folder)),
        entry(FIELD_DECK_TITLE, Expr::String(deck.title.clone())),
        entry(
            FIELD_SLIDE_COUNT,
            Expr::String(deck.slides.len().to_string()),
        ),
        entry(FIELD_EXTENSION, Expr::String(PPTX_EXTENSION.to_owned())),
        entry(
            FIELD_CONTENT_TYPE,
            Expr::String(PPTX_CONTENT_TYPE.to_owned()),
        ),
        entry(FIELD_PREVIEW_ONLY, Expr::Bool(true)),
    ])
}

fn target_folder_expr(target_folder: &ExternalRef) -> Expr {
    map(vec![
        entry(FIELD_BACKEND, Expr::String(target_folder.backend.clone())),
        entry(
            FIELD_EXTERNAL_ID,
            Expr::String(target_folder.external_id.clone()),
        ),
        entry(FIELD_VERSION, option_string(&target_folder.version)),
        entry(FIELD_WEB_URL, option_string(&target_folder.web_url)),
    ])
}

fn option_string(value: &Option<String>) -> Expr {
    match value {
        Some(value) => Expr::String(value.clone()),
        None => Expr::Nil,
    }
}

fn upload_doc_id(deck: &Deck, target_folder: &ExternalRef) -> String {
    format!(
        "{}:{}/{}{}",
        target_folder.backend,
        target_folder.external_id,
        deck_slug(&deck.title),
        PPTX_EXTENSION
    )
}

fn deck_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in title.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "deck".to_owned()
    } else {
        slug.to_owned()
    }
}

fn path_segment(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(hex((byte >> 4) & 0x0f));
            encoded.push(hex(byte & 0x0f));
        }
    }
    encoded
}

fn hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => unreachable!("hex nybble"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::sync::Arc;

    use sim_kernel::{Cx, DefaultFactory, Expr, NoopEvalPolicy, Symbol};
    use sim_lib_doc_core::ExternalRef;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::{Deck, Slide};

    struct ModeledFileSite {
        expected_path: String,
        body: Vec<u8>,
    }

    impl MsGraphSite for ModeledFileSite {
        fn graph_get_bytes(&self, _cx: &mut Cx, path: &str) -> Result<Vec<u8>, DeckError> {
            assert_eq!(path, self.expected_path);
            Ok(self.body.clone())
        }
    }

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn modeled_drive_item_bytes_are_pptx() {
        let mut context = test_context();
        let bytes = minimal_pptx_bytes();
        let site = ModeledFileSite {
            expected_path: "/me/drive/items/deck%201/content".to_owned(),
            body: bytes.clone(),
        };

        let downloaded = download_deck_file(&mut context, &site, "deck 1").unwrap();

        assert_eq!(downloaded, bytes);
        validate_pptx_package(&downloaded).unwrap();
    }

    #[test]
    fn upload_planning_creates_preview_edit_only() {
        let mut context = test_context();
        let mut deck = Deck::new("Project Update");
        deck.push_slide(Slide::new("slide-1", "Status"));
        let target = ExternalRef::new(
            "site/msgraph",
            "folder-1",
            Some("etag-1".to_owned()),
            Some("https://example.com/folder".to_owned()),
        );

        let edit = plan_upload_deck_file(&mut context, &deck, &target).unwrap();
        let op = edit.op.object().as_expr(&mut context).unwrap();
        let target_folder = expr_entry(&op, FIELD_TARGET_FOLDER).unwrap();

        assert_eq!(edit.domain, GRAPH_DECK_FILE_EDIT_DOMAIN);
        assert_eq!(
            edit.doc.as_str(),
            "site/msgraph:folder-1/project-update.pptx"
        );
        assert_eq!(
            symbol_entry(&op, FIELD_KIND).as_deref(),
            Some("office/upload-deck-file")
        );
        assert_eq!(
            string_entry(target_folder, FIELD_EXTERNAL_ID),
            Some("folder-1")
        );
        assert_eq!(bool_entry(&op, FIELD_PREVIEW_ONLY), Some(true));
        assert!(expr_entry(&op, "bytes").is_none());
    }

    #[test]
    fn powerpoint_bridge_rejects_missing_pptx_base64() {
        let bridge = include_str!("../../../office-js/powerpoint_bridge.ts");

        assert!(bridge.contains("if (!req?.pptxBase64"));
        assert!(bridge.contains("throw new Error(\"pptxBase64 is required\")"));
        assert!(bridge.contains("insertSlidesFromBase64(req.pptxBase64"));
    }

    fn minimal_pptx_bytes() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for name in REQUIRED_PPTX_ENTRIES {
                writer.start_file(name, options).unwrap();
                writer.write_all(b"<xml/>").unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn expr_entry<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
        let Expr::Map(entries) = expr else {
            return None;
        };
        entries.iter().find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == &Symbol::new(name) => Some(value),
            _ => None,
        })
    }

    fn symbol_entry(expr: &Expr, name: &str) -> Option<String> {
        match expr_entry(expr, name) {
            Some(Expr::Symbol(symbol)) => Some(symbol.as_qualified_str()),
            _ => None,
        }
    }

    fn string_entry<'a>(expr: &'a Expr, name: &str) -> Option<&'a str> {
        match expr_entry(expr, name) {
            Some(Expr::String(value)) => Some(value),
            _ => None,
        }
    }

    fn bool_entry(expr: &Expr, name: &str) -> Option<bool> {
        match expr_entry(expr, name) {
            Some(Expr::Bool(value)) => Some(*value),
            _ => None,
        }
    }
}
