//! Dalux project item projection into office document records.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocId, ExternalRef};
use sim_lib_sheet::{CellRef, CellValue, Sheet, sheet_to_doc};

use crate::{DALUX_SITE_ID, DaluxError};

const FIELD_ID: &str = "id";
const FIELD_LOCATION: &str = "location";
const FIELD_NOTE: &str = "note";
const FIELD_STATUS: &str = "status";
const FIELD_TITLE: &str = "title";
const FIELD_UPDATED_AT: &str = "updatedAt";
const FIELD_WEB_URL: &str = "webUrl";

const ITEM_HEADERS: &[&str] = &[
    FIELD_ID,
    FIELD_TITLE,
    FIELD_STATUS,
    FIELD_LOCATION,
    FIELD_NOTE,
];

/// Dalux project item fields carried into local office records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaluxItem {
    /// Dalux item id.
    pub id: String,
    /// Human-facing title.
    pub title: String,
    /// Workflow status.
    pub status: String,
    /// Location label.
    pub location: String,
    /// Narrow annotation note.
    pub note: String,
    /// Optional update marker.
    pub updated_at: Option<String>,
    /// Optional browser-facing URL.
    pub web_url: Option<String>,
}

impl DaluxItem {
    /// Decodes one Dalux item from a JSON object.
    pub fn from_json(item: &JsonValue) -> Result<Self, DaluxError> {
        let id = json_text(item, FIELD_ID)
            .ok_or_else(|| DaluxError::WrongShape("Dalux item is missing string id".to_owned()))?;
        Ok(Self {
            id: id.to_owned(),
            title: json_text(item, FIELD_TITLE).unwrap_or_default().to_owned(),
            status: json_text(item, FIELD_STATUS).unwrap_or_default().to_owned(),
            location: json_text(item, FIELD_LOCATION)
                .unwrap_or_default()
                .to_owned(),
            note: json_text(item, FIELD_NOTE).unwrap_or_default().to_owned(),
            updated_at: json_text(item, FIELD_UPDATED_AT).map(ToOwned::to_owned),
            web_url: json_text(item, FIELD_WEB_URL).map(ToOwned::to_owned),
        })
    }
}

/// Builds the Dalux project-item API path.
pub fn project_items_path(project_id: &str) -> Result<String, DaluxError> {
    Ok(format!("/projects/{}/items", api_segment(project_id)?))
}

/// Builds the Dalux item API path.
pub fn item_path(item_id: &str) -> Result<String, DaluxError> {
    Ok(format!("/items/{}", api_segment(item_id)?))
}

/// Projects a Dalux item-list response into a local sheet document.
pub fn items_doc(cx: &mut Cx, project_id: &str, body: &JsonValue) -> Result<Doc, DaluxError> {
    let items = items_from_body(body)?;
    let mut sheet = Sheet::new(format!("Dalux project {project_id} items"));
    write_header_row(&mut sheet)?;
    for (row_index, item) in items.iter().enumerate() {
        write_item_row(&mut sheet, row_index as u32 + 2, item)?;
    }
    let mut doc = sheet_to_doc(
        cx,
        DocId::new(format!("{DALUX_SITE_ID}/projects/{project_id}/items")),
        &sheet,
    )?;
    doc.origin.push(ExternalRef::new(
        DALUX_SITE_ID,
        format!("projects/{project_id}/items"),
        None,
        None,
    ));
    Ok(doc)
}

/// Projects a Dalux note-patch response into an external reference.
pub fn patch_external_ref(item_id: &str, body: &JsonValue) -> Result<ExternalRef, DaluxError> {
    let id = json_text(body, FIELD_ID).unwrap_or(item_id);
    Ok(ExternalRef::new(
        DALUX_SITE_ID,
        format!("items/{id}"),
        json_text(body, FIELD_UPDATED_AT).map(ToOwned::to_owned),
        json_text(body, FIELD_WEB_URL).map(ToOwned::to_owned),
    ))
}

fn items_from_body(body: &JsonValue) -> Result<Vec<DaluxItem>, DaluxError> {
    let items = body
        .get("items")
        .or_else(|| body.get("data"))
        .or_else(|| body.get("value"))
        .and_then(JsonValue::as_array)
        .ok_or_else(|| DaluxError::WrongShape("Dalux response needs an items array".to_owned()))?;
    items.iter().map(DaluxItem::from_json).collect()
}

fn write_header_row(sheet: &mut Sheet) -> Result<(), DaluxError> {
    for (index, header) in ITEM_HEADERS.iter().enumerate() {
        sheet.set_cell(
            CellRef::new(index as u32 + 1, 1)?,
            CellValue::Text((*header).to_owned()),
        );
    }
    Ok(())
}

fn write_item_row(sheet: &mut Sheet, row: u32, item: &DaluxItem) -> Result<(), DaluxError> {
    let values = [
        item.id.as_str(),
        item.title.as_str(),
        item.status.as_str(),
        item.location.as_str(),
        item.note.as_str(),
    ];
    for (index, value) in values.iter().enumerate() {
        sheet.set_cell(
            CellRef::new(index as u32 + 1, row)?,
            CellValue::Text((*value).to_owned()),
        );
    }
    Ok(())
}

fn json_text<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
    value.get(key).and_then(JsonValue::as_str)
}

fn api_segment(value: &str) -> Result<String, DaluxError> {
    let value = value.trim();
    if value.is_empty() || value.contains("://") {
        return Err(DaluxError::InvalidTarget(format!(
            "invalid Dalux identifier {value:?}"
        )));
    }
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn paths_escape_segments() {
        assert_eq!(
            project_items_path("project 1").unwrap(),
            "/projects/project%201/items"
        );
        assert_eq!(item_path("item/1").unwrap(), "/items/item%2F1");
    }

    #[test]
    fn items_decode_from_public_shape() {
        let items = items_from_body(&json!({
            "items": [{ "id": "item-1", "title": "Door" }]
        }))
        .unwrap();

        assert_eq!(items[0].id, "item-1");
        assert_eq!(items[0].title, "Door");
    }
}
