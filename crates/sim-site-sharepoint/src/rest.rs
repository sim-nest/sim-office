//! Explicit SharePoint REST `_api` fallback operations.

use serde_json::{Value as JsonValue, json};
use sim_kernel::Cx;
use sim_site_msgraph::{GraphMode, TokenProvider};

use crate::batch::{BATCH_API_PATH, RestBatchOp, odata_batch_body, odata_batch_content_type};
use crate::{SharePointError, permission};

/// Token-backed transport used by SharePoint REST fallback operations.
pub trait SharePointRestTokenSite {
    /// Runs one site-local SharePoint REST `GET` operation.
    fn rest_get_json(
        &self,
        cx: &mut Cx,
        site_url: &str,
        api_path: &str,
    ) -> Result<JsonValue, SharePointError>;

    /// Runs one site-local SharePoint REST `POST` operation.
    fn rest_post_json(
        &self,
        cx: &mut Cx,
        site_url: &str,
        api_path: &str,
        body: &JsonValue,
    ) -> Result<JsonValue, SharePointError>;
}

impl<T: TokenProvider> SharePointRestTokenSite for GraphMode<T> {
    fn rest_get_json(
        &self,
        cx: &mut Cx,
        site_url: &str,
        api_path: &str,
    ) -> Result<JsonValue, SharePointError> {
        let path = normalize_api_path(api_path)?;
        match self {
            GraphMode::Modeled(_) => sim_site_msgraph::graph_get(cx, self, &path)
                .map_err(|error| rest_transport_error(site_url, error)),
            GraphMode::Live { .. } => Err(permission::rest_site_error(
                site_url,
                "live SharePoint REST fallback requires a SharePoint REST transport",
            )),
        }
    }

    fn rest_post_json(
        &self,
        cx: &mut Cx,
        site_url: &str,
        api_path: &str,
        body: &JsonValue,
    ) -> Result<JsonValue, SharePointError> {
        let path = normalize_api_path(api_path)?;
        match self {
            GraphMode::Modeled(_) => sim_site_msgraph::graph_post(cx, self, &path, body)
                .map_err(|error| rest_transport_error(site_url, error)),
            GraphMode::Live { .. } => Err(permission::rest_site_error(
                site_url,
                "live SharePoint REST fallback requires a SharePoint REST transport",
            )),
        }
    }
}

/// Per-operation SharePoint REST `_api` fallback behind the SharePoint placement.
#[derive(Clone, Debug)]
pub struct SharePointRestSite<G> {
    /// Absolute SharePoint site URL.
    pub site_url: String,
    /// Token-backed site transport supplied by the host.
    pub token_site: G,
}

impl<G> SharePointRestSite<G> {
    /// Builds a SharePoint REST fallback bound to one SharePoint site URL.
    #[must_use]
    pub fn new(site_url: impl Into<String>, token_site: G) -> Self {
        Self {
            site_url: site_url.into(),
            token_site,
        }
    }

    /// Builds an absolute SharePoint REST `_api` URL below this site.
    pub fn api_url(&self, path: &str) -> Result<String, SharePointError> {
        api_url_for(&self.site_url, path)
    }
}

impl<G: SharePointRestTokenSite> SharePointRestSite<G> {
    /// Runs one explicit SharePoint REST `_api` `GET` fallback operation.
    pub fn get_json(&self, cx: &mut Cx, api_path: &str) -> Result<JsonValue, SharePointError> {
        let path = normalize_api_path(api_path)?;
        self.token_site.rest_get_json(cx, &self.site_url, &path)
    }

    /// Runs a deterministic SharePoint REST `_api/$batch` fallback operation.
    pub fn batch(&self, cx: &mut Cx, ops: &[RestBatchOp]) -> Result<JsonValue, SharePointError> {
        let request_body = odata_batch_body(&self.site_url, ops)?;
        let request = json!({
            "content_type": odata_batch_content_type(),
            "body": request_body,
        });
        self.token_site
            .rest_post_json(cx, &self.site_url, BATCH_API_PATH, &request)
    }
}

pub(crate) fn api_url_for(site_url: &str, path: &str) -> Result<String, SharePointError> {
    let site = normalize_site_url(site_url)?;
    let path = normalize_api_path(path)?;
    Ok(format!("{site}{path}"))
}

pub(crate) fn normalize_api_path(path: &str) -> Result<String, SharePointError> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.contains("://") || trimmed.starts_with("//") {
        return Err(permission::rest_error(format!(
            "invalid SharePoint REST _api path {trimmed:?}"
        )));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(permission::rest_error(format!(
            "SharePoint REST _api path must be URL-encoded: {trimmed:?}"
        )));
    }
    let path = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    if path == "/_api" || path.starts_with("/_api/") {
        Ok(path)
    } else {
        Err(permission::rest_error(format!(
            "SharePoint REST fallback only accepts _api paths: {trimmed:?}"
        )))
    }
}

fn normalize_site_url(site_url: &str) -> Result<String, SharePointError> {
    let trimmed = site_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(permission::rest_error(
            "SharePoint REST fallback requires a site URL",
        ));
    }
    if !trimmed.starts_with("https://") || trimmed.contains('?') || trimmed.contains('#') {
        return Err(permission::rest_site_error(
            site_url,
            "invalid SharePoint REST site URL",
        ));
    }
    Ok(trimmed.to_owned())
}

fn rest_transport_error(site_url: &str, error: sim_site_msgraph::GraphError) -> SharePointError {
    permission::rest_site_error(site_url, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
    use sim_site_msgraph::{Cassette, GraphMode, StaticTokenProvider};

    use super::*;

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    fn modeled_rest(
        path: &str,
        body: JsonValue,
    ) -> SharePointRestSite<GraphMode<StaticTokenProvider>> {
        SharePointRestSite::new(
            "https://contoso.sharepoint.com/sites/design",
            GraphMode::Modeled(Cassette::with_json(path, body)),
        )
    }

    #[test]
    fn api_urls_are_site_stable() {
        let rest = modeled_rest("/_api/web", json!({}));

        assert_eq!(
            rest.api_url("_api/web/lists/getbytitle('Tasks')/items?$select=Title")
                .unwrap(),
            "https://contoso.sharepoint.com/sites/design/_api/web/lists/getbytitle('Tasks')/items?$select=Title"
        );
    }

    #[test]
    fn get_json_uses_site_local_api_path() {
        let mut cx = test_context();
        let rest = modeled_rest(
            "/_api/web/lists/getbytitle('Tasks')/items",
            json!({ "value": [{ "Title": "Door review" }] }),
        );

        let body = rest
            .get_json(&mut cx, "_api/web/lists/getbytitle('Tasks')/items")
            .unwrap();

        assert_eq!(body["value"][0]["Title"], "Door review");
    }

    #[test]
    fn batch_posts_to_api_batch_path() {
        let mut cx = test_context();
        let site_url = "https://contoso.sharepoint.com/sites/design";
        let ops = [RestBatchOp::get("_api/web/lists/getbytitle('Tasks')/items")];
        let expected_body = json!({
            "content_type": odata_batch_content_type(),
            "body": odata_batch_body(site_url, &ops).unwrap(),
        });
        let rest = SharePointRestSite::new(
            site_url,
            GraphMode::<StaticTokenProvider>::Modeled(Cassette::with_post_json(
                BATCH_API_PATH,
                expected_body,
                json!({ "status": "accepted" }),
            )),
        );

        let response = rest.batch(&mut cx, &ops).unwrap();

        assert_eq!(response["status"], "accepted");
    }

    #[test]
    fn rest_errors_redact_private_tenants() {
        let rest = SharePointRestSite::new(
            "https://private-tenant.sharepoint.com/sites/secret",
            GraphMode::<StaticTokenProvider>::Modeled(Cassette::new()),
        );
        let error = rest
            .api_url("https://private-tenant.sharepoint.com/sites/secret/_api/web")
            .unwrap_err();

        assert!(!error.to_string().contains("private-tenant.sharepoint.com"));
        assert!(error.to_string().contains("[redacted-sharepoint-url]"));
    }
}
