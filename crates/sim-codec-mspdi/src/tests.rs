use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_doc_core::{DocCodec, DocCodecOptions};
use sim_lib_gantt::{GanttPlan, LinkKind, Task, TaskLink};
use time::{Date, Month};

use super::*;

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn options(cx: &mut Cx) -> DocCodecOptions {
    DocCodecOptions::new(cx.factory().nil().unwrap())
}

#[test]
fn task_dates_and_dependencies_round_trip() {
    let mut cx = cx();
    let plan = GanttPlan::new(
        "Project & Schedule",
        vec![
            Task::new(
                "1",
                "Design",
                Date::from_calendar_date(2026, Month::July, 1).unwrap(),
                Date::from_calendar_date(2026, Month::July, 3).unwrap(),
                25,
            ),
            Task::new(
                "2",
                "Build",
                Date::from_calendar_date(2026, Month::July, 4).unwrap(),
                Date::from_calendar_date(2026, Month::July, 8).unwrap(),
                0,
            ),
        ],
        vec![TaskLink::new("1", "2", LinkKind::FinishStart, 1)],
    );
    let doc = plan_to_doc(&mut cx, &plan).unwrap();
    let codec = MspdiCodec;

    let encode_options = options(&mut cx);
    let (bytes, encode_report) = codec.encode(&mut cx, &doc, &encode_options).unwrap();
    assert!(encode_report.is_lossless());
    let xml = std::str::from_utf8(&bytes).unwrap();
    assert!(xml.contains("<LinkLag>4800</LinkLag><LagFormat>7</LagFormat>"));

    let decode_options = options(&mut cx);
    let (decoded, decode_report) = codec.decode(&mut cx, &bytes, &decode_options).unwrap();
    assert!(decode_report.is_lossless());
    let decoded_plan = doc_to_plan(&mut cx, &decoded).unwrap();

    assert_eq!(decoded_plan, plan);
}

#[test]
fn negative_lead_lag_round_trips_through_mspdi_units() {
    let mut cx = cx();
    let plan = GanttPlan::new(
        "Lead Plan",
        vec![
            Task::new(
                "1",
                "Design",
                Date::from_calendar_date(2026, Month::July, 1).unwrap(),
                Date::from_calendar_date(2026, Month::July, 3).unwrap(),
                25,
            ),
            Task::new(
                "2",
                "Build",
                Date::from_calendar_date(2026, Month::July, 4).unwrap(),
                Date::from_calendar_date(2026, Month::July, 8).unwrap(),
                0,
            ),
        ],
        vec![TaskLink::new("1", "2", LinkKind::FinishStart, -1)],
    );
    let doc = plan_to_doc(&mut cx, &plan).unwrap();
    let codec = MspdiCodec;

    let encode_options = options(&mut cx);
    let (bytes, _report) = codec.encode(&mut cx, &doc, &encode_options).unwrap();
    let xml = std::str::from_utf8(&bytes).unwrap();
    assert!(xml.contains("<LinkLag>-4800</LinkLag><LagFormat>7</LagFormat>"));

    let decode_options = options(&mut cx);
    let (decoded, _report) = codec.decode(&mut cx, &bytes, &decode_options).unwrap();
    let decoded_plan = doc_to_plan(&mut cx, &decoded).unwrap();
    assert_eq!(decoded_plan.links[0].lag_days, -1);
}

#[test]
fn unsupported_fields_are_reported() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Imported</Name>
  <Calendars><Calendar><UID>1</UID></Calendar></Calendars>
  <Tasks>
    <Task>
      <UID>1</UID>
      <ID>1</ID>
      <Name>Design</Name>
      <Start>2026-07-01T08:30:00</Start>
      <Finish>2026-07-02T17:00:00</Finish>
      <PercentComplete>50</PercentComplete>
      <OutlineCode>Phase A</OutlineCode>
    </Task>
    <Task>
      <UID>2</UID>
      <ID>2</ID>
      <Name>Build</Name>
      <Start>2026-07-03T00:00:00</Start>
      <Finish>2026-07-04T00:00:00</Finish>
      <PercentComplete>0</PercentComplete>
      <PredecessorLink>
        <PredecessorUID>1</PredecessorUID>
        <Type>1</Type>
        <LinkLag>0</LinkLag>
        <CrossProjectName>Other</CrossProjectName>
      </PredecessorLink>
    </Task>
  </Tasks>
</Project>"#;

    let (_doc, report) = MspdiCodec.decode(&mut cx, xml, &decode_options).unwrap();

    assert!(
        report
            .preserved_extras
            .iter()
            .any(|extra| extra == "Project.Calendars")
    );
    assert!(
        report
            .preserved_extras
            .iter()
            .any(|extra| extra == "Task[1].OutlineCode")
    );
    assert!(
        report
            .dropped
            .iter()
            .any(|note| note.field == "Task[1].Start.time")
    );
    assert!(
        report
            .dropped
            .iter()
            .any(|note| { note.field == "Task[2].PredecessorLink.CrossProjectName" })
    );
}

#[test]
fn unsupported_lag_format_fails_closed() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Imported</Name>
  <Tasks>
    <Task>
      <UID>1</UID>
      <Name>Design</Name>
      <Start>2026-07-01T00:00:00</Start>
      <Finish>2026-07-02T00:00:00</Finish>
    </Task>
    <Task>
      <UID>2</UID>
      <Name>Build</Name>
      <Start>2026-07-03T00:00:00</Start>
      <Finish>2026-07-04T00:00:00</Finish>
      <PredecessorLink>
        <PredecessorUID>1</PredecessorUID>
        <Type>1</Type>
        <LinkLag>0</LinkLag>
        <LagFormat>5</LagFormat>
      </PredecessorLink>
    </Task>
  </Tasks>
</Project>"#;

    let error = MspdiCodec
        .decode(&mut cx, xml, &decode_options)
        .unwrap_err();

    assert!(error.to_string().contains("unsupported MSPDI LagFormat 5"));
}

#[test]
fn inexact_day_lag_fails_closed() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Imported</Name>
  <Tasks>
    <Task>
      <UID>1</UID>
      <Name>Design</Name>
      <Start>2026-07-01T00:00:00</Start>
      <Finish>2026-07-02T00:00:00</Finish>
    </Task>
    <Task>
      <UID>2</UID>
      <Name>Build</Name>
      <Start>2026-07-03T00:00:00</Start>
      <Finish>2026-07-04T00:00:00</Finish>
      <PredecessorLink>
        <PredecessorUID>1</PredecessorUID>
        <Type>1</Type>
        <LinkLag>1</LinkLag>
        <LagFormat>7</LagFormat>
      </PredecessorLink>
    </Task>
  </Tasks>
</Project>"#;

    let error = MspdiCodec
        .decode(&mut cx, xml, &decode_options)
        .unwrap_err();

    assert!(error.to_string().contains("not an exact whole-day lag"));
}

#[test]
fn invalid_gantt_plan_fails_closed_on_decode() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Imported</Name>
  <Tasks>
    <Task>
      <UID>1</UID>
      <Name>Design</Name>
      <Start>2026-07-02T00:00:00</Start>
      <Finish>2026-07-01T00:00:00</Finish>
      <PercentComplete>101</PercentComplete>
    </Task>
  </Tasks>
</Project>"#;

    let error = MspdiCodec
        .decode(&mut cx, xml, &decode_options)
        .unwrap_err();

    assert!(error.to_string().contains("invalid Gantt plan"));
    assert!(error.to_string().contains("percent_complete"));
}

#[test]
fn missing_predecessor_fails_closed_on_decode() {
    let mut cx = cx();
    let decode_options = options(&mut cx);
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Imported</Name>
  <Tasks>
    <Task>
      <UID>2</UID>
      <Name>Build</Name>
      <Start>2026-07-03T00:00:00</Start>
      <Finish>2026-07-04T00:00:00</Finish>
      <PredecessorLink>
        <PredecessorUID>missing</PredecessorUID>
        <Type>1</Type>
        <LinkLag>0</LinkLag>
        <LagFormat>7</LagFormat>
      </PredecessorLink>
    </Task>
  </Tasks>
</Project>"#;

    let error = MspdiCodec
        .decode(&mut cx, xml, &decode_options)
        .unwrap_err();

    assert!(error.to_string().contains("missing task id missing"));
}

#[test]
fn recipes_are_embedded() {
    let cards = sim_cookbook::recipes_from_embedded(RECIPES).unwrap();

    assert!(
        cards
            .iter()
            .any(|card| card.id.ends_with("mspdi-round-trip"))
    );
}
