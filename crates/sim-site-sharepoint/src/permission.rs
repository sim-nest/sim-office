//! SharePoint REST fallback permission and redaction helpers.

use crate::SharePointError;

/// Hosts that appear in public fixtures and can be shown verbatim in errors.
pub const PUBLIC_FIXTURE_HOSTS: &[&str] = &["contoso.sharepoint.com", "example.sharepoint.com"];

const REDACTED_TENANT_URL: &str = "[redacted-sharepoint-url]";
const HTTPS_SCHEME: &str = "https://";

/// Returns true when `host` is an explicitly public fixture tenant.
#[must_use]
pub fn is_public_fixture_host(host: &str) -> bool {
    PUBLIC_FIXTURE_HOSTS
        .iter()
        .any(|fixture| host.eq_ignore_ascii_case(fixture))
}

/// Redacts private SharePoint tenant URLs while preserving public fixture URLs.
#[must_use]
pub fn redact_tenant_urls(message: &str) -> String {
    let mut redacted = String::new();
    let mut rest = message;

    while let Some(index) = rest.find(HTTPS_SCHEME) {
        let (prefix, candidate) = rest.split_at(index);
        redacted.push_str(prefix);
        let end = candidate
            .find(|character: char| {
                character.is_whitespace()
                    || matches!(character, '"' | '\'' | ')' | ']' | '}' | '<' | '>')
            })
            .unwrap_or(candidate.len());
        let (url, suffix) = candidate.split_at(end);
        if should_redact_url(url) {
            redacted.push_str(REDACTED_TENANT_URL);
        } else {
            redacted.push_str(url);
        }
        rest = suffix;
    }

    redacted.push_str(rest);
    redacted
}

/// Builds a REST fallback error with tenant URLs redacted.
#[must_use]
pub fn rest_error(message: impl AsRef<str>) -> SharePointError {
    SharePointError::Rest(redact_tenant_urls(message.as_ref()))
}

pub(crate) fn rest_site_error(site_url: &str, message: impl AsRef<str>) -> SharePointError {
    let message = format!("{} for {}", message.as_ref(), site_url);
    rest_error(message)
}

fn should_redact_url(url: &str) -> bool {
    sharepoint_host(url).is_some_and(|host| !is_public_fixture_host(host))
}

fn sharepoint_host(url: &str) -> Option<&str> {
    let without_scheme = url.strip_prefix(HTTPS_SCHEME)?;
    let host_end = without_scheme
        .find(['/', '?', '#'])
        .unwrap_or(without_scheme.len());
    let host = &without_scheme[..host_end];
    host.ends_with(".sharepoint.com").then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_tenant_urls_are_redacted() {
        let message =
            "GET https://tenant-private.sharepoint.com/sites/design/_api/web/lists failed";

        assert_eq!(
            redact_tenant_urls(message),
            "GET [redacted-sharepoint-url] failed"
        );
    }

    #[test]
    fn public_fixture_urls_are_preserved() {
        let message = "GET https://contoso.sharepoint.com/sites/design/_api/web/lists";

        assert_eq!(redact_tenant_urls(message), message);
    }
}
