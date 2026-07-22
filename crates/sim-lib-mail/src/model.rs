//! Mail and calendar records and expression projection.

use std::fmt;

use sim_kernel::{Expr, Symbol};
use sim_lib_doc_core::ExternalRef;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Office document kind used for mail message documents.
pub const MAIL_DOC_KIND: &str = "mail";
/// Office document kind used for calendar event documents.
pub const CALENDAR_EVENT_DOC_KIND: &str = "calendar-event";

const FIELD_ATTACHMENTS: &str = "attachments";
const FIELD_ATTENDEES: &str = "attendees";
const FIELD_BACKEND: &str = "backend";
const FIELD_BODY_PREVIEW: &str = "body-preview";
const FIELD_ENDS_AT: &str = "ends-at";
const FIELD_EXTERNAL_ID: &str = "external-id";
const FIELD_FROM: &str = "from";
const FIELD_ID: &str = "id";
const FIELD_KIND: &str = "kind";
const FIELD_RECEIVED_AT: &str = "received-at";
const FIELD_STARTS_AT: &str = "starts-at";
const FIELD_SUBJECT: &str = "subject";
const FIELD_VERSION: &str = "version";
const FIELD_WEB_URL: &str = "web-url";

/// Local mail message record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mail {
    /// Stable message id.
    pub id: String,
    /// Message subject.
    pub subject: String,
    /// Sender display or address.
    pub from: String,
    /// Received timestamp when known.
    pub received_at: Option<OffsetDateTime>,
    /// Short body preview safe for errors and logs.
    pub body_preview: String,
    /// Attachment references. Attachment bytes are never stored here.
    pub attachments: Vec<ExternalRef>,
}

impl Mail {
    /// Builds a mail message record.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        subject: impl Into<String>,
        from: impl Into<String>,
        received_at: Option<OffsetDateTime>,
        body_preview: impl Into<String>,
        attachments: Vec<ExternalRef>,
    ) -> Self {
        Self {
            id: id.into(),
            subject: subject.into(),
            from: from.into(),
            received_at,
            body_preview: body_preview.into(),
            attachments,
        }
    }
}

/// Local calendar event record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEvent {
    /// Stable event id.
    pub id: String,
    /// Event subject.
    pub subject: String,
    /// Event start timestamp.
    pub starts_at: OffsetDateTime,
    /// Event end timestamp.
    pub ends_at: OffsetDateTime,
    /// Attendee display names or addresses.
    pub attendees: Vec<String>,
}

impl CalendarEvent {
    /// Builds a calendar event record.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        subject: impl Into<String>,
        starts_at: OffsetDateTime,
        ends_at: OffsetDateTime,
        attendees: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            subject: subject.into(),
            starts_at,
            ends_at,
            attendees,
        }
    }
}

/// Mail-domain failure.
#[derive(Debug)]
pub enum MailError {
    /// A kernel operation failed.
    Kernel(String),
    /// The document kind did not match the requested mail domain.
    WrongDocKind(String),
    /// The document body did not have the expected expression shape.
    WrongDocBody(String),
    /// A mail or calendar record was invalid.
    InvalidMail(String),
}

impl fmt::Display for MailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel(message) => write!(f, "kernel error: {message}"),
            Self::WrongDocKind(kind) => write!(f, "unexpected mail document kind: {kind}"),
            Self::WrongDocBody(message) => write!(f, "invalid mail document body: {message}"),
            Self::InvalidMail(message) => write!(f, "invalid mail record: {message}"),
        }
    }
}

impl std::error::Error for MailError {}

impl From<sim_kernel::Error> for MailError {
    fn from(error: sim_kernel::Error) -> Self {
        Self::Kernel(error.to_string())
    }
}

pub(crate) fn mail_to_expr(mail: &Mail) -> Result<Expr, MailError> {
    validate_id(&mail.id, "mail id")?;
    Ok(map(vec![
        entry(FIELD_KIND, Expr::Symbol(office_symbol(MAIL_DOC_KIND))),
        entry(FIELD_ID, Expr::String(mail.id.clone())),
        entry(FIELD_SUBJECT, Expr::String(mail.subject.clone())),
        entry(FIELD_FROM, Expr::String(mail.from.clone())),
        entry(FIELD_RECEIVED_AT, option_datetime(mail.received_at)?),
        entry(FIELD_BODY_PREVIEW, Expr::String(mail.body_preview.clone())),
        entry(
            FIELD_ATTACHMENTS,
            Expr::List(mail.attachments.iter().map(external_ref_expr).collect()),
        ),
    ]))
}

pub(crate) fn mail_from_expr(expr: &Expr) -> Result<Mail, MailError> {
    let entries = expect_map(expr)?;
    expect_kind(entries, MAIL_DOC_KIND)?;
    Ok(Mail {
        id: expect_string(value_for(entries, FIELD_ID)?, FIELD_ID)?.to_owned(),
        subject: expect_string(value_for(entries, FIELD_SUBJECT)?, FIELD_SUBJECT)?.to_owned(),
        from: expect_string(value_for(entries, FIELD_FROM)?, FIELD_FROM)?.to_owned(),
        received_at: optional_datetime(value_for(entries, FIELD_RECEIVED_AT)?)?,
        body_preview: expect_string(value_for(entries, FIELD_BODY_PREVIEW)?, FIELD_BODY_PREVIEW)?
            .to_owned(),
        attachments: external_refs(value_for(entries, FIELD_ATTACHMENTS)?)?,
    })
}

pub(crate) fn calendar_event_to_expr(event: &CalendarEvent) -> Result<Expr, MailError> {
    validate_id(&event.id, "calendar event id")?;
    if event.ends_at < event.starts_at {
        return Err(MailError::InvalidMail(
            "calendar event ends before it starts".to_owned(),
        ));
    }
    Ok(map(vec![
        entry(
            FIELD_KIND,
            Expr::Symbol(office_symbol(CALENDAR_EVENT_DOC_KIND)),
        ),
        entry(FIELD_ID, Expr::String(event.id.clone())),
        entry(FIELD_SUBJECT, Expr::String(event.subject.clone())),
        entry(FIELD_STARTS_AT, datetime_expr(event.starts_at)?),
        entry(FIELD_ENDS_AT, datetime_expr(event.ends_at)?),
        entry(
            FIELD_ATTENDEES,
            Expr::List(event.attendees.iter().cloned().map(Expr::String).collect()),
        ),
    ]))
}

pub(crate) fn calendar_event_from_expr(expr: &Expr) -> Result<CalendarEvent, MailError> {
    let entries = expect_map(expr)?;
    expect_kind(entries, CALENDAR_EVENT_DOC_KIND)?;
    Ok(CalendarEvent {
        id: expect_string(value_for(entries, FIELD_ID)?, FIELD_ID)?.to_owned(),
        subject: expect_string(value_for(entries, FIELD_SUBJECT)?, FIELD_SUBJECT)?.to_owned(),
        starts_at: datetime_from_expr(value_for(entries, FIELD_STARTS_AT)?, FIELD_STARTS_AT)?,
        ends_at: datetime_from_expr(value_for(entries, FIELD_ENDS_AT)?, FIELD_ENDS_AT)?,
        attendees: strings_from_expr(value_for(entries, FIELD_ATTENDEES)?, FIELD_ATTENDEES)?,
    })
}

fn validate_id(value: &str, label: &str) -> Result<(), MailError> {
    if value.trim().is_empty() {
        Err(MailError::InvalidMail(format!("{label} is empty")))
    } else {
        Ok(())
    }
}

fn external_ref_expr(reference: &ExternalRef) -> Expr {
    map(vec![
        entry(FIELD_BACKEND, Expr::String(reference.backend.clone())),
        entry(
            FIELD_EXTERNAL_ID,
            Expr::String(reference.external_id.clone()),
        ),
        entry(FIELD_VERSION, option_string(&reference.version)),
        entry(FIELD_WEB_URL, option_string(&reference.web_url)),
    ])
}

fn external_ref_from_expr(expr: &Expr) -> Result<ExternalRef, MailError> {
    let entries = expect_map(expr)?;
    Ok(ExternalRef::new(
        expect_string(value_for(entries, FIELD_BACKEND)?, FIELD_BACKEND)?,
        expect_string(value_for(entries, FIELD_EXTERNAL_ID)?, FIELD_EXTERNAL_ID)?,
        optional_string(value_for(entries, FIELD_VERSION)?)?,
        optional_string(value_for(entries, FIELD_WEB_URL)?)?,
    ))
}

fn external_refs(expr: &Expr) -> Result<Vec<ExternalRef>, MailError> {
    expect_list(expr, FIELD_ATTACHMENTS)?
        .iter()
        .map(external_ref_from_expr)
        .collect()
}

fn option_datetime(value: Option<OffsetDateTime>) -> Result<Expr, MailError> {
    match value {
        Some(value) => datetime_expr(value),
        None => Ok(Expr::Nil),
    }
}

fn optional_datetime(expr: &Expr) -> Result<Option<OffsetDateTime>, MailError> {
    match expr {
        Expr::Nil => Ok(None),
        other => datetime_from_expr(other, FIELD_RECEIVED_AT).map(Some),
    }
}

fn datetime_expr(value: OffsetDateTime) -> Result<Expr, MailError> {
    value
        .format(&Rfc3339)
        .map(Expr::String)
        .map_err(|error| MailError::WrongDocBody(format!("could not format timestamp: {error}")))
}

fn datetime_from_expr(expr: &Expr, label: &'static str) -> Result<OffsetDateTime, MailError> {
    let text = expect_string(expr, label)?;
    OffsetDateTime::parse(text, &Rfc3339)
        .map_err(|error| MailError::WrongDocBody(format!("invalid {label}: {error}")))
}

fn option_string(value: &Option<String>) -> Expr {
    match value {
        Some(value) => Expr::String(value.clone()),
        None => Expr::Nil,
    }
}

fn optional_string(expr: &Expr) -> Result<Option<String>, MailError> {
    match expr {
        Expr::Nil => Ok(None),
        Expr::String(value) => Ok(Some(value.clone())),
        _ => Err(MailError::WrongDocBody(
            "optional reference field must be nil or string".to_owned(),
        )),
    }
}

fn strings_from_expr(expr: &Expr, label: &'static str) -> Result<Vec<String>, MailError> {
    expect_list(expr, label)?
        .iter()
        .map(|item| expect_string(item, label).map(str::to_owned))
        .collect()
}

fn map(entries: Vec<(Expr, Expr)>) -> Expr {
    Expr::Map(entries)
}

fn entry(name: &'static str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

fn office_symbol(name: &'static str) -> Symbol {
    Symbol::qualified("office", name)
}

fn value_for<'a>(entries: &'a [(Expr, Expr)], name: &'static str) -> Result<&'a Expr, MailError> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == &Symbol::new(name) => Some(value),
            _ => None,
        })
        .ok_or_else(|| MailError::WrongDocBody(format!("missing field {name}")))
}

fn expect_map(expr: &Expr) -> Result<&[(Expr, Expr)], MailError> {
    match expr {
        Expr::Map(entries) => Ok(entries),
        _ => Err(MailError::WrongDocBody("expected map".to_owned())),
    }
}

fn expect_list<'a>(expr: &'a Expr, label: &'static str) -> Result<&'a [Expr], MailError> {
    match expr {
        Expr::List(items) => Ok(items),
        _ => Err(MailError::WrongDocBody(format!(
            "field {label} must be a list"
        ))),
    }
}

fn expect_string<'a>(expr: &'a Expr, label: &'static str) -> Result<&'a str, MailError> {
    match expr {
        Expr::String(value) => Ok(value),
        _ => Err(MailError::WrongDocBody(format!(
            "field {label} must be a string"
        ))),
    }
}

fn expect_symbol<'a>(expr: &'a Expr, label: &'static str) -> Result<&'a Symbol, MailError> {
    match expr {
        Expr::Symbol(value) => Ok(value),
        _ => Err(MailError::WrongDocBody(format!(
            "field {label} must be a symbol"
        ))),
    }
}

fn expect_kind(entries: &[(Expr, Expr)], expected: &'static str) -> Result<(), MailError> {
    let kind = expect_symbol(value_for(entries, FIELD_KIND)?, FIELD_KIND)?;
    let expected = office_symbol(expected);
    if kind == &expected {
        Ok(())
    } else {
        Err(MailError::WrongDocBody(format!(
            "expected kind {}, got {}",
            expected,
            kind.as_qualified_str()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_expression_keeps_attachments_as_refs() {
        let received = OffsetDateTime::parse("2026-07-13T09:00:00Z", &Rfc3339).unwrap();
        let mail = Mail::new(
            "msg-1",
            "Project update",
            "ada@example.test",
            Some(received),
            "Short preview",
            vec![ExternalRef::new(
                "site/msgraph",
                "drive-item-1",
                Some("etag-1".to_owned()),
                None,
            )],
        );

        let expr = mail_to_expr(&mail).unwrap();
        let decoded = mail_from_expr(&expr).unwrap();

        assert_eq!(decoded, mail);
        assert!(!expr_has_bytes(&expr));
    }

    #[test]
    fn calendar_expression_round_trips() {
        let starts_at = OffsetDateTime::parse("2026-07-13T09:00:00Z", &Rfc3339).unwrap();
        let ends_at = OffsetDateTime::parse("2026-07-13T10:00:00Z", &Rfc3339).unwrap();
        let event = CalendarEvent::new(
            "evt-1",
            "Review",
            starts_at,
            ends_at,
            vec!["ada@example.test".to_owned(), "bo@example.test".to_owned()],
        );

        let expr = calendar_event_to_expr(&event).unwrap();
        let decoded = calendar_event_from_expr(&expr).unwrap();

        assert_eq!(decoded, event);
    }

    fn expr_has_bytes(expr: &Expr) -> bool {
        match expr {
            Expr::Bytes(_) => true,
            Expr::List(items) | Expr::Vector(items) | Expr::Set(items) => {
                items.iter().any(expr_has_bytes)
            }
            Expr::Block(items) => items.iter().any(expr_has_bytes),
            Expr::Map(entries) => entries
                .iter()
                .any(|(key, value)| expr_has_bytes(key) || expr_has_bytes(value)),
            Expr::Call { operator, args } => {
                expr_has_bytes(operator) || args.iter().any(expr_has_bytes)
            }
            Expr::Infix { left, right, .. } => expr_has_bytes(left) || expr_has_bytes(right),
            Expr::Prefix { arg, .. } | Expr::Postfix { arg, .. } => expr_has_bytes(arg),
            Expr::Quote { expr, .. } => expr_has_bytes(expr),
            Expr::Annotated { expr, annotations } => {
                expr_has_bytes(expr) || annotations.iter().any(|(_, value)| expr_has_bytes(value))
            }
            Expr::Extension { payload, .. } => expr_has_bytes(payload),
            Expr::Nil
            | Expr::Bool(_)
            | Expr::Number(_)
            | Expr::Symbol(_)
            | Expr::Local(_)
            | Expr::String(_) => false,
        }
    }
}
