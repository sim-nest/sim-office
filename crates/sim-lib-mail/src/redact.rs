//! Mail body redaction helpers.

/// Maximum characters of a body preview carried into error text.
pub const BODY_ERROR_PREVIEW_CHARS: usize = 80;

/// Returns a short, non-sensitive body preview for errors and diagnostics.
#[must_use]
pub fn redact_body_for_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= BODY_ERROR_PREVIEW_CHARS {
        return trimmed.to_owned();
    }
    format!("[redacted body: {} chars]", trimmed.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_body_is_kept_as_preview() {
        assert_eq!(redact_body_for_error(" short note "), "short note");
    }

    #[test]
    fn long_body_is_redacted() {
        let secret = "quarterly-plan-secret";
        let body = format!("{secret} {}", "x".repeat(BODY_ERROR_PREVIEW_CHARS + 40));

        let redacted = redact_body_for_error(&body);

        assert!(redacted.contains("redacted body"));
        assert!(!redacted.contains(secret));
        assert!(redacted.len() < BODY_ERROR_PREVIEW_CHARS);
    }
}
