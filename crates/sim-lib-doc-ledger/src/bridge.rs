//! Bridge from office draft ids to ledger draft previews.

use sim_kernel::{Cx, Symbol, Value};
use sim_lib_doc_core::{DocId, Edit, ExternalRef, OfficeError};
use sim_lib_ledger_books::{JournalDraft, LedgerEvidenceRef, validate_draft};

use crate::{DraftBook, DraftId};

/// Edit domain for office-ledger preview payloads.
pub const LEDGER_EDIT_DOMAIN: &str = "office/ledger";

/// Resolve and validate a host-owned draft id.
pub fn resolve_post_draft(
    _cx: &mut Cx,
    draft: DraftId,
    drafts: &DraftBook,
) -> Result<JournalDraft, OfficeError> {
    let draft = drafts.get(&draft).cloned().ok_or_else(|| {
        OfficeError::DomainEdit(format!("unknown ledger draft {}", draft.as_str()))
    })?;
    validate_draft(&draft).map_err(ledger_error)?;
    Ok(draft)
}

/// Build a validation preview edit for a balanced ledger draft.
///
/// The returned edit is preview-only: it records the operation a host can show
/// to an operator, but it does not write a voucher or year file.
pub fn preview_post(cx: &mut Cx, draft: &JournalDraft) -> Result<Edit, OfficeError> {
    validate_draft(draft).map_err(ledger_error)?;
    let op = preview_payload(cx, "post-ledger-draft", draft)?;
    let inverse = preview_payload(cx, "discard-ledger-draft-preview", draft)?;
    Ok(Edit::new(
        DocId::new(preview_doc_id(draft)),
        LEDGER_EDIT_DOMAIN,
        op,
        inverse,
    ))
}

/// Convert an office external reference into a ledger evidence reference.
#[must_use]
pub fn evidence_ref_from_external(
    reference: &ExternalRef,
    immutable_hint: Option<String>,
) -> LedgerEvidenceRef {
    LedgerEvidenceRef::new(
        reference.backend.clone(),
        reference.external_id.clone(),
        reference.version.clone(),
        reference.web_url.clone(),
        immutable_hint,
    )
}

fn preview_payload(cx: &mut Cx, action: &str, draft: &JournalDraft) -> Result<Value, OfficeError> {
    let evidence = draft
        .evidence
        .iter()
        .map(|reference| evidence_payload(cx, reference))
        .collect::<Result<Vec<_>, _>>()?;
    let postings = draft
        .postings
        .iter()
        .map(|posting| {
            cx.factory().table(vec![
                (
                    Symbol::new("account"),
                    cx.factory().string(posting.account.to_string())?,
                ),
                (
                    Symbol::new("amount-minor"),
                    cx.factory().string(posting.amount.0.to_string())?,
                ),
                (
                    Symbol::new("text"),
                    cx.factory()
                        .string(posting.text.clone().unwrap_or_default())?,
                ),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cx.factory().table(vec![
        (
            Symbol::new("action"),
            cx.factory()
                .symbol(Symbol::qualified("office/ledger", action))?,
        ),
        (
            Symbol::new("date"),
            cx.factory().string(draft.date.to_string())?,
        ),
        (
            Symbol::new("text"),
            cx.factory().string(draft.text.clone())?,
        ),
        (Symbol::new("postings"), cx.factory().list(postings)?),
        (Symbol::new("evidence"), cx.factory().list(evidence)?),
    ])?)
}

fn evidence_payload(cx: &mut Cx, reference: &LedgerEvidenceRef) -> Result<Value, OfficeError> {
    Ok(cx.factory().table(vec![
        (
            Symbol::new("backend"),
            cx.factory().string(reference.backend.clone())?,
        ),
        (
            Symbol::new("external-id"),
            cx.factory().string(reference.external_id.clone())?,
        ),
        (
            Symbol::new("version"),
            cx.factory()
                .string(reference.version.clone().unwrap_or_default())?,
        ),
        (
            Symbol::new("web-url"),
            cx.factory()
                .string(reference.web_url.clone().unwrap_or_default())?,
        ),
        (
            Symbol::new("immutable-hint"),
            cx.factory()
                .string(reference.immutable_hint.clone().unwrap_or_default())?,
        ),
    ])?)
}

fn preview_doc_id(draft: &JournalDraft) -> String {
    let mut text = draft
        .text
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(24)
        .collect::<String>();
    if text.is_empty() {
        text = "draft".to_owned();
    }
    format!("ledger/draft/{}/{}", draft.date, text)
}

fn ledger_error(error: sim_lib_ledger_books::BooksError) -> OfficeError {
    OfficeError::DomainEdit(error.to_string())
}

#[cfg(test)]
mod tests {
    use sim_kernel::{Cx, Expr, testing::bare_cx as cx};
    use sim_ledger::{Amount, LedgerSet, Posting};
    use sim_lib_doc_core::ExternalRef;
    use sim_lib_ledger_books::LedgerEvidenceRef;
    use time::{Date, Month};

    use super::*;

    #[test]
    fn resolves_balanced_draft_by_id() {
        let mut cx = cx();
        let mut book = DraftBook::new();
        let id = DraftId::new("draft-1");
        let draft = draft(vec![posting(100), posting(-100)]);
        book.insert(id.clone(), draft.clone());

        let resolved = resolve_post_draft(&mut cx, id, &book).unwrap();

        assert_eq!(resolved, draft);
    }

    #[test]
    fn preview_post_returns_open_preview_edit() {
        let mut cx = cx();
        let draft = draft(vec![posting(100), posting(-100)]);

        let edit = preview_post(&mut cx, &draft).unwrap();

        assert_eq!(edit.domain, LEDGER_EDIT_DOMAIN);
        assert!(edit.doc.as_str().starts_with("ledger/draft/"));
        assert!(format!("{:?}", value_expr(&mut cx, &edit.op)).contains("post-ledger-draft"));
        assert!(
            format!("{:?}", value_expr(&mut cx, &edit.inverse))
                .contains("discard-ledger-draft-preview")
        );
    }

    #[test]
    fn unbalanced_preview_reports_minor_sum() {
        let mut cx = cx();
        let draft = draft(vec![posting(100), posting(-70)]);

        let err = preview_post(&mut cx, &draft).unwrap_err();

        assert!(err.to_string().contains("30 minor units"));
    }

    #[test]
    fn preview_does_not_write_year_file() {
        let dir = tempfile::tempdir().unwrap();
        let set = LedgerSet::create(dir.path(), "Office bridge").unwrap();
        let mut cx = cx();
        let draft = draft(vec![posting(100), posting(-100)]);

        preview_post(&mut cx, &draft).unwrap();

        assert!(!set.year_path(2026).exists());
    }

    #[test]
    fn external_refs_map_to_reference_only_ledger_evidence() {
        let reference = ExternalRef::new(
            "site/sharepoint",
            "sites/site-1/drive/items/file-9",
            Some("etag-9".to_owned()),
            Some("https://sharepoint.example/file-9".to_owned()),
        );

        let evidence = evidence_ref_from_external(&reference, Some("digest-9".to_owned()));

        assert_eq!(evidence.backend, "site/sharepoint");
        assert_eq!(evidence.external_id, "sites/site-1/drive/items/file-9");
        assert_eq!(evidence.version.as_deref(), Some("etag-9"));
        assert_eq!(evidence.immutable_hint.as_deref(), Some("digest-9"));
    }

    fn value_expr(cx: &mut Cx, value: &Value) -> Expr {
        value.object().as_expr(cx).unwrap()
    }

    fn draft(postings: Vec<Posting>) -> JournalDraft {
        JournalDraft {
            date: Date::from_calendar_date(2026, Month::July, 13).unwrap(),
            text: "Supplier invoice".to_owned(),
            postings,
            evidence: vec![LedgerEvidenceRef::new(
                "site/msgraph",
                "messages/msg-1",
                Some("etag-1".to_owned()),
                None,
                None,
            )],
        }
    }

    fn posting(amount: i64) -> Posting {
        Posting {
            id: 0,
            source_id: None,
            voucher_id: 0,
            account: 2440,
            amount: Amount(amount),
            text: Some("line".to_owned()),
        }
    }
}
