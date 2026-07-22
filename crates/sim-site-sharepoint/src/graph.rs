//! Microsoft Graph mapping for SharePoint lists and drives.

use std::collections::BTreeSet;

use serde_json::Value as JsonValue;
use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocId, DocKind, ExternalRef};
use sim_lib_sheet::{CellRef, CellValue, SHEET_DOC_KIND, Sheet, sheet_to_doc};
use sim_site_msgraph::{GraphMode, TokenProvider};

use crate::{SHAREPOINT_SITE_ID, SharePointDriveTarget, SharePointError, SharePointListTarget};

const FIELD_ETAG: &str = "eTag";
const FIELD_FIELDS: &str = "fields";
const FIELD_ID: &str = "id";
const FIELD_VALUE: &str = "value";
const FIELD_WEB_URL: &str = "webUrl";
const ODATA_ETAG: &str = "@odata.etag";

/// Minimal Microsoft Graph read seam used by SharePoint placement functions.
pub trait MsGraphSite {
    /// Runs a site-local Microsoft Graph GET and returns the decoded JSON body.
    fn graph_get(&self, cx: &mut Cx, path: &str) -> Result<JsonValue, SharePointError>;
}

impl<T: TokenProvider> MsGraphSite for GraphMode<T> {
    fn graph_get(&self, cx: &mut Cx, path: &str) -> Result<JsonValue, SharePointError> {
        sim_site_msgraph::graph_get(cx, self, path).map_err(SharePointError::from)
    }
}

/// Reads SharePoint list items through Graph and projects them as a sheet document.
pub fn list_items(
    cx: &mut Cx,
    graph: &dyn MsGraphSite,
    target: &SharePointListTarget,
) -> Result<Doc, SharePointError> {
    let path = target.items_path();
    let body = graph.graph_get(cx, &path)?;
    let items = value_array(&body, &path)?;
    let mut sheet = Sheet::new(format!("SharePoint list {}", target.list_id));
    let field_names = list_field_names(items, &path)?;
    let headers = list_headers(&field_names);
    write_header_row(&mut sheet, &headers)?;

    for (row_index, item) in items.iter().enumerate() {
        write_list_item_row(&mut sheet, row_index as u32 + 2, item, &headers, &path)?;
    }

    let mut doc = sheet_to_doc(
        cx,
        DocId::new(format!(
            "{SHAREPOINT_SITE_ID}/{}/lists/{}",
            target.site_id, target.list_id
        )),
        &sheet,
    )?;
    doc.kind = DocKind::new(SHEET_DOC_KIND);
    doc.origin.push(ExternalRef::new(
        SHAREPOINT_SITE_ID,
        format!("sites/{}/lists/{}", target.site_id, target.list_id),
        None,
        None,
    ));
    Ok(doc)
}

/// Reads SharePoint drive children through Graph and projects them as external refs.
pub fn drive_children(
    cx: &mut Cx,
    graph: &dyn MsGraphSite,
    target: &SharePointDriveTarget,
) -> Result<Vec<ExternalRef>, SharePointError> {
    let path = target.children_path();
    let body = graph.graph_get(cx, &path)?;
    value_array(&body, &path)?
        .iter()
        .map(|item| drive_item_ref(item, target, &path))
        .collect()
}

fn value_array<'a>(body: &'a JsonValue, path: &str) -> Result<&'a [JsonValue], SharePointError> {
    body.get(FIELD_VALUE)
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            SharePointError::WrongShape(format!(
                "Graph response at {path} must contain an array value"
            ))
        })
}

fn list_field_names(items: &[JsonValue], path: &str) -> Result<BTreeSet<String>, SharePointError> {
    let mut names = BTreeSet::new();
    for item in items {
        let fields = fields_object(item, path)?;
        names.extend(
            fields
                .keys()
                .filter(|name| name.as_str() != ODATA_ETAG)
                .cloned(),
        );
    }
    Ok(names)
}

fn list_headers(field_names: &BTreeSet<String>) -> Vec<String> {
    let mut headers = vec![
        FIELD_ID.to_owned(),
        FIELD_WEB_URL.to_owned(),
        FIELD_ETAG.to_owned(),
    ];
    headers.extend(field_names.iter().cloned());
    headers
}

fn write_header_row(sheet: &mut Sheet, headers: &[String]) -> Result<(), SharePointError> {
    for (index, header) in headers.iter().enumerate() {
        sheet.set_cell(
            CellRef::new(index as u32 + 1, 1)?,
            CellValue::Text(header.clone()),
        );
    }
    Ok(())
}

fn write_list_item_row(
    sheet: &mut Sheet,
    row: u32,
    item: &JsonValue,
    headers: &[String],
    path: &str,
) -> Result<(), SharePointError> {
    let item_id = required_text(item, FIELD_ID, path)?;
    let etag = item_etag(item).ok_or_else(|| SharePointError::WritePrecondition {
        path: path.to_owned(),
        item_id: item_id.to_owned(),
    })?;
    let web_url = optional_text(item, FIELD_WEB_URL).unwrap_or_default();
    let fields = fields_object(item, path)?;

    for (index, header) in headers.iter().enumerate() {
        let value = match header.as_str() {
            FIELD_ID => CellValue::Text(item_id.to_owned()),
            FIELD_WEB_URL => CellValue::Text(web_url.to_owned()),
            FIELD_ETAG => CellValue::Text(etag.to_owned()),
            name => json_to_cell(fields.get(name).unwrap_or(&JsonValue::Null)),
        };
        sheet.set_cell(CellRef::new(index as u32 + 1, row)?, value);
    }
    Ok(())
}

fn drive_item_ref(
    item: &JsonValue,
    target: &SharePointDriveTarget,
    path: &str,
) -> Result<ExternalRef, SharePointError> {
    let item_id = required_text(item, FIELD_ID, path)?;
    let etag = item_etag(item).ok_or_else(|| SharePointError::WritePrecondition {
        path: path.to_owned(),
        item_id: item_id.to_owned(),
    })?;
    let web_url = optional_text(item, FIELD_WEB_URL).map(ToOwned::to_owned);
    Ok(ExternalRef::new(
        SHAREPOINT_SITE_ID,
        format!(
            "sites/{}/drives/{}/items/{item_id}",
            target.site_id, target.drive_id
        ),
        Some(etag.to_owned()),
        web_url,
    ))
}

fn fields_object<'a>(
    item: &'a JsonValue,
    path: &str,
) -> Result<&'a serde_json::Map<String, JsonValue>, SharePointError> {
    item.get(FIELD_FIELDS)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| SharePointError::MissingField {
            path: path.to_owned(),
            field: FIELD_FIELDS.to_owned(),
        })
}

fn required_text<'a>(
    item: &'a JsonValue,
    field: &str,
    path: &str,
) -> Result<&'a str, SharePointError> {
    optional_text(item, field).ok_or_else(|| SharePointError::MissingField {
        path: path.to_owned(),
        field: field.to_owned(),
    })
}

fn optional_text<'a>(item: &'a JsonValue, field: &str) -> Option<&'a str> {
    item.get(field).and_then(JsonValue::as_str)
}

fn item_etag(item: &JsonValue) -> Option<&str> {
    optional_text(item, FIELD_ETAG).or_else(|| optional_text(item, ODATA_ETAG))
}

fn json_to_cell(value: &JsonValue) -> CellValue {
    match value {
        JsonValue::Null => CellValue::Blank,
        JsonValue::Bool(value) => CellValue::Bool(*value),
        JsonValue::Number(value) => CellValue::Text(value.to_string()),
        JsonValue::String(value) if value.is_empty() => CellValue::Blank,
        JsonValue::String(value) => CellValue::Text(value.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => CellValue::Text(
            serde_json::to_string(value).unwrap_or_else(|error| format!("json error: {error}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
    use sim_lib_sheet::{CellValue, doc_to_sheet};
    use sim_site_msgraph::{Cassette, GraphMode, StaticTokenProvider};

    use super::*;

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    fn modeled_graph(path: &str, body: JsonValue) -> GraphMode<StaticTokenProvider> {
        GraphMode::Modeled(Cassette::with_json(path, body))
    }

    fn text_at(sheet: &Sheet, cell: &str) -> String {
        match sheet.cell(&CellRef::parse(cell).unwrap()) {
            CellValue::Text(value) => value,
            other => panic!("expected text at {cell}, got {other:?}"),
        }
    }

    #[test]
    fn modeled_list_items_become_sheet_doc() {
        let mut cx = test_context();
        let target = SharePointListTarget::new("site-1", "list-1");
        let graph = modeled_graph(
            &target.items_path(),
            json!({
                "value": [
                    {
                        "id": "1",
                        "webUrl": "https://contoso/sites/site-1/lists/list-1/1",
                        "eTag": "\"1\"",
                        "fields": {
                            "Title": "Door review",
                            "Status": "Open"
                        }
                    }
                ]
            }),
        );

        let doc = list_items(&mut cx, &graph, &target).unwrap();
        let sheet = doc_to_sheet(&mut cx, &doc).unwrap();

        assert_eq!(doc.kind.as_str(), SHEET_DOC_KIND);
        assert_eq!(text_at(&sheet, "A1"), "id");
        assert_eq!(text_at(&sheet, "D1"), "Status");
        assert_eq!(text_at(&sheet, "E1"), "Title");
        assert_eq!(text_at(&sheet, "A2"), "1");
        assert_eq!(text_at(&sheet, "D2"), "Open");
        assert_eq!(text_at(&sheet, "E2"), "Door review");
        assert!(
            doc.origin
                .iter()
                .any(|origin| origin.backend == SHAREPOINT_SITE_ID)
        );
    }

    #[test]
    fn drive_items_become_external_refs() {
        let mut cx = test_context();
        let target = SharePointDriveTarget::new("site-1", "drive-1", None);
        let graph = modeled_graph(
            &target.children_path(),
            json!({
                "value": [
                    {
                        "id": "file-1",
                        "name": "Spec.docx",
                        "eTag": "\"abc\"",
                        "webUrl": "https://contoso/sites/site-1/Spec.docx"
                    }
                ]
            }),
        );

        let refs = drive_children(&mut cx, &graph, &target).unwrap();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].backend, SHAREPOINT_SITE_ID);
        assert_eq!(
            refs[0].external_id,
            "sites/site-1/drives/drive-1/items/file-1"
        );
        assert_eq!(refs[0].version.as_deref(), Some("\"abc\""));
        assert_eq!(
            refs[0].web_url.as_deref(),
            Some("https://contoso/sites/site-1/Spec.docx")
        );
    }

    #[test]
    fn missing_etag_returns_write_precondition_error() {
        let mut cx = test_context();
        let target = SharePointDriveTarget::new("site-1", "drive-1", None);
        let graph = modeled_graph(
            &target.children_path(),
            json!({
                "value": [
                    {
                        "id": "file-1",
                        "name": "Spec.docx"
                    }
                ]
            }),
        );

        let error = drive_children(&mut cx, &graph, &target).unwrap_err();

        assert!(matches!(
            error,
            SharePointError::WritePrecondition { item_id, .. } if item_id == "file-1"
        ));
    }
}
