use std::{cell::RefCell, sync::Arc};

use serde_json::{Value as JsonValue, json};
use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};

use crate::{
    CALENDAR_EVENT_DOC_KIND, MAIL_DOC_KIND, MailError, calendar_event_to_doc, mail_to_doc,
};

use super::graph::{
    CalendarTarget, DraftMessage, MailboxTarget, MsGraphSite, OutlookSelectedItem, create_draft,
    read_calendar_events, read_messages, selected_item_to_selection,
};

struct ModeledGetSite {
    path: String,
    body: JsonValue,
}

impl MsGraphSite for ModeledGetSite {
    fn graph_get(&self, _cx: &mut Cx, path: &str) -> Result<JsonValue, MailError> {
        assert_eq!(path, self.path);
        Ok(self.body.clone())
    }

    fn graph_post(
        &self,
        _cx: &mut Cx,
        _path: &str,
        _body: &JsonValue,
    ) -> Result<JsonValue, MailError> {
        unreachable!("read tests do not post")
    }
}

struct ModeledPostSite {
    path: String,
    response: RefCell<Option<Result<JsonValue, MailError>>>,
    request: RefCell<Option<JsonValue>>,
}

impl MsGraphSite for ModeledPostSite {
    fn graph_get(&self, _cx: &mut Cx, _path: &str) -> Result<JsonValue, MailError> {
        unreachable!("draft tests do not read")
    }

    fn graph_post(
        &self,
        _cx: &mut Cx,
        path: &str,
        body: &JsonValue,
    ) -> Result<JsonValue, MailError> {
        assert_eq!(path, self.path);
        self.request.replace(Some(body.clone()));
        self.response
            .borrow_mut()
            .take()
            .expect("modeled post response should be available")
    }
}

fn test_context() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

#[test]
fn modeled_messages_map_to_mail_docs() {
    let mut cx = test_context();
    let target = MailboxTarget::new("me", Some("inbox".to_owned()));
    let site = ModeledGetSite {
        path: target.messages_path().unwrap(),
        body: json!({
            "value": [{
                "id": "msg-1",
                "subject": "Project update",
                "from": { "emailAddress": { "address": "ada@example.test" } },
                "receivedDateTime": "2026-07-13T09:00:00Z",
                "bodyPreview": "Short preview",
                "attachments": [{
                    "id": "att-1",
                    "@odata.etag": "tag-1",
                    "webLink": "https://example.test/att-1"
                }]
            }]
        }),
    };

    let messages = read_messages(&mut cx, &site, &target).unwrap();
    let doc = mail_to_doc(&mut cx, &messages[0]).unwrap();

    assert_eq!(doc.kind.as_str(), MAIL_DOC_KIND);
    assert_eq!(messages[0].from, "ada@example.test");
    assert_eq!(messages[0].attachments[0].external_id, "att-1");
}

#[test]
fn modeled_events_map_to_calendar_docs() {
    let mut cx = test_context();
    let target = CalendarTarget::new("shared@example.test", Some("cal 1".to_owned()));
    let site = ModeledGetSite {
        path: target.events_path().unwrap(),
        body: json!({
            "value": [{
                "id": "event-1",
                "subject": "Review",
                "start": { "dateTime": "2026-07-13T10:00:00Z", "timeZone": "UTC" },
                "end": { "dateTime": "2026-07-13T11:00:00Z", "timeZone": "UTC" },
                "attendees": [{
                    "emailAddress": { "address": "bo@example.test" }
                }]
            }]
        }),
    };

    let events = read_calendar_events(&mut cx, &site, &target).unwrap();
    let doc = calendar_event_to_doc(&mut cx, &events[0]).unwrap();

    assert_eq!(doc.kind.as_str(), CALENDAR_EVENT_DOC_KIND);
    assert_eq!(events[0].attendees, vec!["bo@example.test"]);
}

#[test]
fn create_draft_returns_ref_and_sends_text_body() {
    let mut cx = test_context();
    let draft = DraftMessage::new(
        MailboxTarget::new("me", None),
        "Question",
        "short body",
        vec!["ada@example.test".to_owned()],
    );
    let site = ModeledPostSite {
        path: "/me/messages".to_owned(),
        response: RefCell::new(Some(Ok(json!({
            "id": "draft-1",
            "changeKey": "change-1",
            "webLink": "https://example.test/draft-1"
        })))),
        request: RefCell::new(None),
    };

    let reference = create_draft(&mut cx, &site, &draft).unwrap();

    assert_eq!(reference.backend, "site/msgraph");
    assert_eq!(reference.external_id, "draft-1");
    let request = site.request.borrow();
    assert_eq!(request.as_ref().unwrap()["body"]["content"], "short body");
}

#[test]
fn draft_errors_redact_long_body() {
    let mut cx = test_context();
    let long_body = "secret ".repeat(40);
    let draft = DraftMessage::new(
        MailboxTarget::new("me", None),
        "Question",
        long_body.clone(),
        vec!["ada@example.test".to_owned()],
    );
    let site = ModeledPostSite {
        path: "/me/messages".to_owned(),
        response: RefCell::new(Some(Err(MailError::WrongDocBody(long_body.clone())))),
        request: RefCell::new(None),
    };

    let error = create_draft(&mut cx, &site, &draft).unwrap_err();
    let message = error.to_string();

    assert!(message.contains("[redacted body:"));
    assert!(!message.contains(&long_body));
}

#[test]
fn selected_item_schema_maps_to_suite_selection() {
    let bridge = include_str!("../../../office-js/outlook_bridge.ts");
    assert!(bridge.contains("Office.context.mailbox.item"));
    assert!(bridge.contains("itemId: item.itemId"));

    let item: OutlookSelectedItem = serde_json::from_value(json!({
        "itemId": "AAMk-1",
        "subject": "Review",
        "itemType": "message"
    }))
    .unwrap();
    let selection = selected_item_to_selection(item).unwrap();

    assert_eq!(selection.doc_id, "outlook:AAMk-1");
    assert_eq!(selection.subject.as_deref(), Some("Review"));
    assert_eq!(selection.item_type.as_deref(), Some("message"));
}
