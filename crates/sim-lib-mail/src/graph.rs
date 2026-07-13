//! Microsoft Graph mail and calendar bridge.
//!
//! This module targets Exchange Online and shared mailbox data through
//! Microsoft Graph. Host authentication belongs outside this crate, normally
//! through nested app authentication or MSAL. EWS, legacy callback tokens, and
//! in-place archive actions are intentionally outside this domain seam; the
//! bridge reads mail/calendar data and creates drafts that later surfaces can
//! preview or commit.

use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use sim_kernel::Cx;
use sim_lib_doc_core::ExternalRef;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{CalendarEvent, Mail, MailError, redact_body_for_error};

const GRAPH_BACKEND: &str = "site/msgraph";
const FIELD_ATTENDEES: &str = "attendees";
const FIELD_BODY: &str = "body";
const FIELD_BODY_PREVIEW: &str = "bodyPreview";
const FIELD_CHANGE_KEY: &str = "changeKey";
const FIELD_CONTENT: &str = "content";
const FIELD_CONTENT_TYPE: &str = "contentType";
const FIELD_DATE_TIME: &str = "dateTime";
const FIELD_EMAIL_ADDRESS: &str = "emailAddress";
const FIELD_END: &str = "end";
const FIELD_FROM: &str = "from";
const FIELD_ID: &str = "id";
const FIELD_NAME: &str = "name";
const FIELD_ODATA_ETAG: &str = "@odata.etag";
const FIELD_RECEIVED_DATE_TIME: &str = "receivedDateTime";
const FIELD_START: &str = "start";
const FIELD_SUBJECT: &str = "subject";
const FIELD_TO_RECIPIENTS: &str = "toRecipients";
const FIELD_VALUE: &str = "value";
const FIELD_WEB_LINK: &str = "webLink";
const MESSAGE_QUERY: &str =
    "?$select=id,subject,from,receivedDateTime,bodyPreview,webLink&$expand=attachments";
const EVENT_QUERY: &str = "?$select=id,subject,start,end,attendees,webLink";

/// Minimal Microsoft Graph mail seam implemented by host Graph adapters.
pub trait MsGraphSite {
    /// Runs a site-local Microsoft Graph `GET` and returns the decoded JSON body.
    fn graph_get(&self, cx: &mut Cx, path: &str) -> Result<JsonValue, MailError>;

    /// Runs a site-local Microsoft Graph `POST` and returns the decoded JSON body.
    fn graph_post(&self, cx: &mut Cx, path: &str, body: &JsonValue)
    -> Result<JsonValue, MailError>;
}

/// Microsoft Graph mailbox target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxTarget {
    /// `me` for the signed-in mailbox, or a user/shared mailbox id.
    pub user_id_or_me: String,
    /// Optional mail folder id or well-known folder name such as `inbox`.
    pub folder: Option<String>,
}

impl MailboxTarget {
    /// Builds a mailbox target.
    #[must_use]
    pub fn new(user_id_or_me: impl Into<String>, folder: Option<String>) -> Self {
        Self {
            user_id_or_me: user_id_or_me.into(),
            folder,
        }
    }

    /// Builds the Microsoft Graph path used to list messages.
    pub fn messages_path(&self) -> Result<String, MailError> {
        let base = mailbox_base(&self.user_id_or_me)?;
        let path = match self.folder.as_deref().map(str::trim) {
            Some(folder) if !folder.is_empty() => {
                format!("{base}/mailFolders/{}/messages", path_segment(folder))
            }
            _ => format!("{base}/messages"),
        };
        Ok(format!("{path}{MESSAGE_QUERY}"))
    }

    fn drafts_path(&self) -> Result<String, MailError> {
        Ok(format!("{}/messages", mailbox_base(&self.user_id_or_me)?))
    }
}

/// Microsoft Graph calendar target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarTarget {
    /// `me` for the signed-in mailbox, or a user/shared mailbox id.
    pub user_id_or_me: String,
    /// Optional calendar id. The default calendar is used when omitted.
    pub calendar: Option<String>,
}

impl CalendarTarget {
    /// Builds a calendar target.
    #[must_use]
    pub fn new(user_id_or_me: impl Into<String>, calendar: Option<String>) -> Self {
        Self {
            user_id_or_me: user_id_or_me.into(),
            calendar,
        }
    }

    /// Builds the Microsoft Graph path used to list calendar events.
    pub fn events_path(&self) -> Result<String, MailError> {
        let base = mailbox_base(&self.user_id_or_me)?;
        let path = match self.calendar.as_deref().map(str::trim) {
            Some(calendar) if !calendar.is_empty() => {
                format!("{base}/calendars/{}/events", path_segment(calendar))
            }
            _ => format!("{base}/events"),
        };
        Ok(format!("{path}{EVENT_QUERY}"))
    }
}

/// Draft mail message sent to Microsoft Graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftMessage {
    /// Target mailbox where the draft is created.
    pub target: MailboxTarget,
    /// Draft subject.
    pub subject: String,
    /// Plain-text draft body. Error reporting always redacts long bodies.
    pub body: String,
    /// Recipient email addresses.
    pub to: Vec<String>,
}

impl DraftMessage {
    /// Builds a draft message.
    #[must_use]
    pub fn new(
        target: MailboxTarget,
        subject: impl Into<String>,
        body: impl Into<String>,
        to: Vec<String>,
    ) -> Self {
        Self {
            target,
            subject: subject.into(),
            body: body.into(),
            to,
        }
    }
}

/// Outlook selected-item payload returned by the Office.js host bridge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlookSelectedItem {
    /// Outlook item id.
    #[serde(rename = "itemId")]
    pub item_id: String,
    /// Selected item subject.
    pub subject: Option<String>,
    /// Outlook item type, usually `message` or `appointment`.
    #[serde(rename = "itemType")]
    pub item_type: Option<String>,
}

/// Suite selection distilled from an Outlook selected item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteSelection {
    /// Stable suite document id for the selected item.
    pub doc_id: String,
    /// Selected item subject.
    pub subject: Option<String>,
    /// Outlook item type.
    pub item_type: Option<String>,
}

/// Reads Microsoft Graph messages and maps them into local mail records.
pub fn read_messages(
    cx: &mut Cx,
    site: &dyn MsGraphSite,
    target: &MailboxTarget,
) -> Result<Vec<Mail>, MailError> {
    let path = target.messages_path()?;
    let body = site
        .graph_get(cx, &path)
        .map_err(|error| graph_error("Microsoft Graph mail read", error))?;
    value_array(&body, FIELD_VALUE)?
        .iter()
        .map(message_from_json)
        .collect()
}

/// Reads Microsoft Graph calendar events and maps them into local event records.
pub fn read_calendar_events(
    cx: &mut Cx,
    site: &dyn MsGraphSite,
    target: &CalendarTarget,
) -> Result<Vec<CalendarEvent>, MailError> {
    let path = target.events_path()?;
    let body = site
        .graph_get(cx, &path)
        .map_err(|error| graph_error("Microsoft Graph calendar read", error))?;
    value_array(&body, FIELD_VALUE)?
        .iter()
        .map(calendar_event_from_json)
        .collect()
}

/// Creates a Microsoft Graph draft and returns a reference to the remote item.
pub fn create_draft(
    cx: &mut Cx,
    site: &dyn MsGraphSite,
    draft: &DraftMessage,
) -> Result<ExternalRef, MailError> {
    validate_draft(draft)?;
    let path = draft.target.drafts_path()?;
    let body = draft_to_json(draft);
    let response = site
        .graph_post(cx, &path, &body)
        .map_err(|error| draft_graph_error(draft, error))?;
    external_ref_from_response(&response, FIELD_ID)
}

/// Converts an Outlook selected item into the suite selection shape.
pub fn selected_item_to_selection(item: OutlookSelectedItem) -> Result<SuiteSelection, MailError> {
    if item.item_id.trim().is_empty() {
        return Err(MailError::InvalidMail(
            "Outlook selected item id is empty".to_owned(),
        ));
    }
    Ok(SuiteSelection {
        doc_id: format!("outlook:{}", item.item_id),
        subject: item.subject,
        item_type: item.item_type,
    })
}

fn validate_draft(draft: &DraftMessage) -> Result<(), MailError> {
    if draft.subject.trim().is_empty() {
        return Err(MailError::InvalidMail("draft subject is empty".to_owned()));
    }
    if draft.to.is_empty() || draft.to.iter().any(|recipient| recipient.trim().is_empty()) {
        return Err(MailError::InvalidMail(
            "draft recipients must be non-empty".to_owned(),
        ));
    }
    Ok(())
}

fn draft_to_json(draft: &DraftMessage) -> JsonValue {
    json!({
        FIELD_SUBJECT: draft.subject,
        FIELD_BODY: {
            FIELD_CONTENT_TYPE: "Text",
            FIELD_CONTENT: draft.body,
        },
        FIELD_TO_RECIPIENTS: draft.to.iter().map(|recipient| {
            json!({ FIELD_EMAIL_ADDRESS: { "address": recipient } })
        }).collect::<Vec<JsonValue>>(),
    })
}

fn message_from_json(value: &JsonValue) -> Result<Mail, MailError> {
    let id = string_field(value, FIELD_ID)?;
    Ok(Mail::new(
        id,
        optional_string_field(value, FIELD_SUBJECT).unwrap_or_default(),
        graph_email(value.get(FIELD_FROM)).unwrap_or_default(),
        optional_datetime(value.get(FIELD_RECEIVED_DATE_TIME))?,
        optional_string_field(value, FIELD_BODY_PREVIEW).unwrap_or_default(),
        attachments_from_message(value)?,
    ))
}

fn calendar_event_from_json(value: &JsonValue) -> Result<CalendarEvent, MailError> {
    Ok(CalendarEvent::new(
        string_field(value, FIELD_ID)?,
        optional_string_field(value, FIELD_SUBJECT).unwrap_or_default(),
        graph_datetime_field(value.get(FIELD_START), FIELD_START)?,
        graph_datetime_field(value.get(FIELD_END), FIELD_END)?,
        attendees_from_event(value)?,
    ))
}

fn attachments_from_message(value: &JsonValue) -> Result<Vec<ExternalRef>, MailError> {
    match value.get("attachments").and_then(JsonValue::as_array) {
        Some(attachments) => attachments
            .iter()
            .map(|attachment| external_ref_from_response(attachment, FIELD_ID))
            .collect(),
        None => Ok(Vec::new()),
    }
}

fn attendees_from_event(value: &JsonValue) -> Result<Vec<String>, MailError> {
    match value.get(FIELD_ATTENDEES).and_then(JsonValue::as_array) {
        Some(attendees) => Ok(attendees
            .iter()
            .filter_map(|attendee| graph_email(Some(attendee)))
            .collect()),
        None => Ok(Vec::new()),
    }
}

fn external_ref_from_response(
    value: &JsonValue,
    id_field: &'static str,
) -> Result<ExternalRef, MailError> {
    Ok(ExternalRef::new(
        GRAPH_BACKEND,
        string_field(value, id_field)?,
        optional_string_field(value, FIELD_ODATA_ETAG)
            .or_else(|| optional_string_field(value, FIELD_CHANGE_KEY)),
        optional_string_field(value, FIELD_WEB_LINK),
    ))
}

fn graph_datetime_field(
    value: Option<&JsonValue>,
    label: &'static str,
) -> Result<OffsetDateTime, MailError> {
    let Some(value) = value else {
        return Err(MailError::WrongDocBody(format!(
            "Graph field {label} is missing"
        )));
    };
    if let Some(text) = value.as_str() {
        return parse_graph_datetime(text, label);
    }
    if let Some(text) = value.get(FIELD_DATE_TIME).and_then(JsonValue::as_str) {
        return parse_graph_datetime(text, label);
    }
    Err(MailError::WrongDocBody(format!(
        "Graph field {label} must be a datetime string or object"
    )))
}

fn optional_datetime(value: Option<&JsonValue>) -> Result<Option<OffsetDateTime>, MailError> {
    match value {
        Some(JsonValue::Null) | None => Ok(None),
        Some(value) => graph_datetime_field(Some(value), FIELD_RECEIVED_DATE_TIME).map(Some),
    }
}

fn parse_graph_datetime(text: &str, label: &'static str) -> Result<OffsetDateTime, MailError> {
    match OffsetDateTime::parse(text, &Rfc3339) {
        Ok(value) => Ok(value),
        Err(first_error) if !has_explicit_offset(text) => {
            let normalized = format!("{text}Z");
            OffsetDateTime::parse(&normalized, &Rfc3339).map_err(|error| {
                MailError::WrongDocBody(format!(
                    "invalid Graph datetime {label}: {first_error}; UTC fallback failed: {error}"
                ))
            })
        }
        Err(error) => Err(MailError::WrongDocBody(format!(
            "invalid Graph datetime {label}: {error}"
        ))),
    }
}

fn has_explicit_offset(text: &str) -> bool {
    if text.ends_with('Z') {
        return true;
    }
    let Some(time_index) = text.find('T') else {
        return false;
    };
    text[time_index + 1..].contains('+') || text[time_index + 1..].contains('-')
}

fn graph_email(value: Option<&JsonValue>) -> Option<String> {
    let value = value?;
    let address = value.get(FIELD_EMAIL_ADDRESS).unwrap_or(value);
    optional_string_field(address, "address").or_else(|| optional_string_field(address, FIELD_NAME))
}

fn value_array<'a>(
    value: &'a JsonValue,
    field: &'static str,
) -> Result<&'a [JsonValue], MailError> {
    value
        .get(field)
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| MailError::WrongDocBody(format!("Graph response must include {field}")))
}

fn string_field<'a>(value: &'a JsonValue, field: &'static str) -> Result<&'a str, MailError> {
    value
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| MailError::WrongDocBody(format!("Graph field {field} must be a string")))
}

fn optional_string_field(value: &JsonValue, field: &'static str) -> Option<String> {
    value
        .get(field)
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn mailbox_base(user_id_or_me: &str) -> Result<String, MailError> {
    let user = user_id_or_me.trim();
    if user.is_empty() {
        return Err(MailError::InvalidMail("mailbox target is empty".to_owned()));
    }
    if user == "me" {
        Ok("/me".to_owned())
    } else {
        Ok(format!("/users/{}", path_segment(user)))
    }
}

fn path_segment(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

fn graph_error(context: &'static str, error: MailError) -> MailError {
    MailError::WrongDocBody(format!(
        "{context}: {}",
        redact_body_for_error(&error.to_string())
    ))
}

fn draft_graph_error(draft: &DraftMessage, error: MailError) -> MailError {
    MailError::InvalidMail(format!(
        "Microsoft Graph draft create failed for body {}: {}",
        redact_body_for_error(&draft.body),
        redact_body_for_error(&error.to_string())
    ))
}
