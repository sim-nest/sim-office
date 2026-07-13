//! SharePoint Graph target records.

/// SharePoint list target addressed through Microsoft Graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharePointListTarget {
    /// Microsoft Graph SharePoint site id.
    pub site_id: String,
    /// Microsoft Graph list id.
    pub list_id: String,
}

impl SharePointListTarget {
    /// Builds a SharePoint list target.
    #[must_use]
    pub fn new(site_id: impl Into<String>, list_id: impl Into<String>) -> Self {
        Self {
            site_id: site_id.into(),
            list_id: list_id.into(),
        }
    }

    /// Builds the Microsoft Graph path for list items with fields expanded.
    #[must_use]
    pub fn items_path(&self) -> String {
        format!(
            "/sites/{}/lists/{}/items?$expand=fields",
            graph_segment(&self.site_id),
            graph_segment(&self.list_id)
        )
    }
}

/// SharePoint document library drive target addressed through Microsoft Graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharePointDriveTarget {
    /// Microsoft Graph SharePoint site id.
    pub site_id: String,
    /// Microsoft Graph drive id.
    pub drive_id: String,
    /// Optional drive item id. `None` addresses the drive root.
    pub item_id: Option<String>,
}

impl SharePointDriveTarget {
    /// Builds a SharePoint drive target.
    #[must_use]
    pub fn new(
        site_id: impl Into<String>,
        drive_id: impl Into<String>,
        item_id: Option<String>,
    ) -> Self {
        Self {
            site_id: site_id.into(),
            drive_id: drive_id.into(),
            item_id,
        }
    }

    /// Builds the Microsoft Graph path for children below this drive target.
    #[must_use]
    pub fn children_path(&self) -> String {
        match &self.item_id {
            Some(item_id) => format!(
                "/sites/{}/drives/{}/items/{}/children",
                graph_segment(&self.site_id),
                graph_segment(&self.drive_id),
                graph_segment(item_id)
            ),
            None => format!(
                "/sites/{}/drives/{}/root/children",
                graph_segment(&self.site_id),
                graph_segment(&self.drive_id)
            ),
        }
    }
}

fn graph_segment(value: &str) -> String {
    let mut escaped = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' | b':' => {
                escaped.push(byte as char)
            }
            _ => escaped.push_str(&format!("%{byte:02X}")),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_target_builds_graph_path() {
        let target = SharePointListTarget::new("contoso.sharepoint.com,site,web", "Tasks");

        assert_eq!(
            target.items_path(),
            "/sites/contoso.sharepoint.com,site,web/lists/Tasks/items?$expand=fields"
        );
    }

    #[test]
    fn drive_target_builds_root_and_item_paths() {
        let root = SharePointDriveTarget::new("site-1", "drive 1", None);
        let item = SharePointDriveTarget::new("site-1", "drive 1", Some("folder/1".to_owned()));

        assert_eq!(
            root.children_path(),
            "/sites/site-1/drives/drive%201/root/children"
        );
        assert_eq!(
            item.children_path(),
            "/sites/site-1/drives/drive%201/items/folder%2F1/children"
        );
    }
}
