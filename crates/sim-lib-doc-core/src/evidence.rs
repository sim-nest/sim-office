//! Cross-object evidence links between office documents and external records.

use crate::{DocId, ExternalRef};

/// A reference-only evidence link for a document subject.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    /// Local document or task being supported by the external reference.
    pub subject: DocId,
    /// External item, file, message, voucher, task, or published target.
    pub evidence: ExternalRef,
    /// Relationship between the subject and the evidence object.
    pub role: LinkRole,
    /// Authoritative ledger or capture sequence at which the link was recorded.
    pub captured_at_seq: u64,
    /// Optional immutable marker such as an ETag, content hash, or voucher digest.
    pub immutable_hint: Option<String>,
}

impl Evidence {
    /// Builds an evidence link.
    #[must_use]
    pub fn new(
        subject: DocId,
        evidence: ExternalRef,
        role: LinkRole,
        captured_at_seq: u64,
        immutable_hint: Option<String>,
    ) -> Self {
        Self {
            subject,
            evidence,
            role,
            captured_at_seq,
            immutable_hint,
        }
    }

    /// Returns the claim predicate used to store this link as a fact row.
    #[must_use]
    pub fn predicate(&self) -> &'static str {
        self.role.predicate()
    }
}

/// Role used as the predicate for a cross-object evidence fact.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum LinkRole {
    /// The external object is the source document for the subject.
    SourceDocument,
    /// The external object supports an accounting entry or statement.
    AccountingSupport,
    /// The external object anchors a schedule or Gantt task reference.
    ScheduleReference,
    /// The external object is a project issue or field item.
    ProjectIssue,
    /// The subject was published to the external object.
    PublishedTo,
}

impl LinkRole {
    /// Returns the stable claim predicate for this role.
    #[must_use]
    pub fn predicate(self) -> &'static str {
        match self {
            Self::SourceDocument => "office/source-document",
            Self::AccountingSupport => "office/accounting-support",
            Self::ScheduleReference => "office/schedule-reference",
            Self::ProjectIssue => "office/project-issue",
            Self::PublishedTo => "office/published-to",
        }
    }

    /// Decodes a stable claim predicate into a link role.
    #[must_use]
    pub fn from_predicate(predicate: &str) -> Option<Self> {
        match predicate {
            "office/source-document" => Some(Self::SourceDocument),
            "office/accounting-support" => Some(Self::AccountingSupport),
            "office/schedule-reference" => Some(Self::ScheduleReference),
            "office/project-issue" => Some(Self::ProjectIssue),
            "office/published-to" => Some(Self::PublishedTo),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_json_round_trips_reference_only_fields() {
        let evidence = Evidence::new(
            DocId::new("task-17"),
            ExternalRef::new(
                "site/msgraph",
                "messages/msg-1",
                Some("etag-1".to_owned()),
                Some("https://graph.example/messages/msg-1".to_owned()),
            ),
            LinkRole::SourceDocument,
            9,
            Some("sha256:abc".to_owned()),
        );

        let encoded = serde_json::to_string(&evidence).unwrap();
        let decoded: Evidence = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, evidence);
        assert_eq!(decoded.predicate(), "office/source-document");
    }

    #[test]
    fn ledger_voucher_uses_plain_external_ref() {
        let evidence = Evidence::new(
            DocId::new("annual-account-2026"),
            ExternalRef::new("ledger", "voucher/2026/0007", None, None),
            LinkRole::AccountingSupport,
            42,
            Some("voucher-digest".to_owned()),
        );

        assert_eq!(evidence.evidence.backend, "ledger");
        assert_eq!(evidence.predicate(), "office/accounting-support");
    }
}
