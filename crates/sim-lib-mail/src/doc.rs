//! Mail document adapters.

use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocId, DocKind};

use crate::model::{
    CALENDAR_EVENT_DOC_KIND, CalendarEvent, MAIL_DOC_KIND, Mail, MailError,
    calendar_event_from_expr, calendar_event_to_expr, mail_from_expr, mail_to_expr,
};

/// Converts a mail message into an office document.
pub fn mail_to_doc(cx: &mut Cx, mail: &Mail) -> Result<Doc, MailError> {
    let body = cx.factory().expr(mail_to_expr(mail)?)?;
    Ok(Doc::new(
        DocKind::new(MAIL_DOC_KIND),
        DocId::new(mail.id.clone()),
        body,
        Vec::new(),
    ))
}

/// Decodes an office mail document into a mail message.
pub fn doc_to_mail(cx: &mut Cx, doc: &Doc) -> Result<Mail, MailError> {
    ensure_kind(doc, MAIL_DOC_KIND)?;
    let expr = doc.body.object().as_expr(cx)?;
    mail_from_expr(&expr)
}

/// Converts a calendar event into an office document.
pub fn calendar_event_to_doc(cx: &mut Cx, event: &CalendarEvent) -> Result<Doc, MailError> {
    let body = cx.factory().expr(calendar_event_to_expr(event)?)?;
    Ok(Doc::new(
        DocKind::new(CALENDAR_EVENT_DOC_KIND),
        DocId::new(event.id.clone()),
        body,
        Vec::new(),
    ))
}

/// Decodes an office calendar document into a calendar event.
pub fn doc_to_calendar_event(cx: &mut Cx, doc: &Doc) -> Result<CalendarEvent, MailError> {
    ensure_kind(doc, CALENDAR_EVENT_DOC_KIND)?;
    let expr = doc.body.object().as_expr(cx)?;
    calendar_event_from_expr(&expr)
}

fn ensure_kind(doc: &Doc, expected: &'static str) -> Result<(), MailError> {
    if doc.kind.as_str() == expected {
        Ok(())
    } else {
        Err(MailError::WrongDocKind(doc.kind.as_str().to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
    use sim_lib_doc_core::ExternalRef;
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    use super::*;

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn mail_docs_round_trip() {
        let mut context = test_context();
        let received = OffsetDateTime::parse("2026-07-13T09:00:00Z", &Rfc3339).unwrap();
        let mail = Mail::new(
            "msg-1",
            "Project update",
            "ada@example.test",
            Some(received),
            "Short preview",
            vec![ExternalRef::new("site/msgraph", "drive-item-1", None, None)],
        );

        let doc = mail_to_doc(&mut context, &mail).unwrap();
        let decoded = doc_to_mail(&mut context, &doc).unwrap();

        assert_eq!(doc.kind.as_str(), MAIL_DOC_KIND);
        assert_eq!(decoded, mail);
    }

    #[test]
    fn calendar_event_docs_round_trip() {
        let mut context = test_context();
        let starts_at = OffsetDateTime::parse("2026-07-13T09:00:00Z", &Rfc3339).unwrap();
        let ends_at = OffsetDateTime::parse("2026-07-13T10:00:00Z", &Rfc3339).unwrap();
        let event = CalendarEvent::new(
            "evt-1",
            "Review",
            starts_at,
            ends_at,
            vec!["ada@example.test".to_owned()],
        );

        let doc = calendar_event_to_doc(&mut context, &event).unwrap();
        let decoded = doc_to_calendar_event(&mut context, &doc).unwrap();

        assert_eq!(doc.kind.as_str(), CALENDAR_EVENT_DOC_KIND);
        assert_eq!(decoded, event);
    }

    #[test]
    fn recipes_export_embedded_books() {
        assert!(crate::RECIPES.iter().any(|(path, _)| *path == "book.toml"));
    }
}
