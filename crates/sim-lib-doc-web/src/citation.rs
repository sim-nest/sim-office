use crate::{
    EvidenceAnchor, WebEvidenceError,
    records::{CitationRecord, cid_text},
};

/// Citation output format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CitationFormat {
    /// Plain text.
    Plain,
    /// Markdown.
    Markdown,
    /// Lisp data.
    Lisp,
    /// JSON.
    Json,
}

impl EvidenceAnchor {
    /// Renders one canonical citation record.
    pub fn render(&self, format: CitationFormat) -> Result<String, WebEvidenceError> {
        let quote = self
            .selector
            .as_ref()
            .map(|selector| selector.exact.as_str());
        match format {
            CitationFormat::Plain => Ok(format!(
                "{} — {} (retrieved {}; capture {}; representation {}; policy {}){}{}",
                self.source_title,
                self.source_uri,
                self.retrieved_at,
                cid_text(&self.capture_id),
                cid_text(&self.representation_id),
                self.policy_receipt_id,
                quote.map_or(String::new(), |quote| format!(": “{quote}”")),
                warnings(self)
            )),
            CitationFormat::Markdown => Ok(format!(
                "[{}]({}) (retrieved `{}`; capture `{}`; representation `{}`; policy `{}`){}{}",
                self.source_title,
                self.source_uri,
                self.retrieved_at,
                cid_text(&self.capture_id),
                cid_text(&self.representation_id),
                self.policy_receipt_id,
                quote.map_or(String::new(), |quote| format!(": > {quote}")),
                warnings(self)
            )),
            CitationFormat::Lisp => Ok(format!(
                "(citation :title {:?} :uri {:?} :retrieved {:?} :capture {:?} :representation {:?} :policy {:?} :quote {:?} :warnings {:?} :provider-claim {:?})",
                self.source_title,
                self.source_uri,
                self.retrieved_at,
                cid_text(&self.capture_id),
                cid_text(&self.representation_id),
                self.policy_receipt_id,
                quote,
                self.fidelity_warnings,
                self.provider_claim
            )),
            CitationFormat::Json => serde_json::to_string(&CitationRecord::from(self))
                .map_err(WebEvidenceError::from_display),
        }
    }
}

fn warnings(anchor: &EvidenceAnchor) -> String {
    let mut warnings = String::new();
    if !anchor.fidelity_warnings.is_empty() {
        warnings.push_str(&format!(
            "; fidelity warnings: {}",
            anchor.fidelity_warnings.join(", ")
        ));
    }
    if let Some(claim) = &anchor.provider_claim {
        warnings.push_str(&format!("; provider claim: {claim}"));
    }
    warnings
}
