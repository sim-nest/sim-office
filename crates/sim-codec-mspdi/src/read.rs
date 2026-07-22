//! MSPDI XML reader.

use std::str;

use roxmltree::{Document, Node};
use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, ExternalRef, FidelityReport, LossNote, OfficeError};
use sim_lib_gantt::{GanttPlan, LinkKind, Task, TaskLink, validate_gantt_plan};
use time::{Date, Month};

use crate::{
    MSPDI_CODEC_ID, MSPDI_LAG_FORMAT_DAYS, TENTHS_PER_WORKDAY, error, model::parse_i32, plan_to_doc,
};

const FIELD_FINISH: &str = "Finish";
const FIELD_ID: &str = "ID";
const FIELD_NAME: &str = "Name";
const FIELD_PERCENT_COMPLETE: &str = "PercentComplete";
const FIELD_START: &str = "Start";
const FIELD_TASKS: &str = "Tasks";
const FIELD_UID: &str = "UID";
const LINK_LAG: &str = "LinkLag";
const LINK_LAG_FORMAT: &str = "LagFormat";
const LINK_TYPE: &str = "Type";
const PREDECESSOR_LINK: &str = "PredecessorLink";
const PREDECESSOR_UID: &str = "PredecessorUID";

pub(crate) fn decode(cx: &mut Cx, bytes: &[u8]) -> Result<(Doc, FidelityReport), OfficeError> {
    let text =
        str::from_utf8(bytes).map_err(|err| error(format!("MSPDI XML is not UTF-8: {err}")))?;
    let document =
        Document::parse(text).map_err(|err| error(format!("invalid MSPDI XML: {err}")))?;
    let root = document.root_element();
    if root.tag_name().name() != "Project" {
        return Err(error(format!(
            "MSPDI root must be Project, got {}",
            root.tag_name().name()
        )));
    }

    let mut report = FidelityReport::new(MSPDI_CODEC_ID);
    report_project_extras(root, &mut report);
    let id = clean_text(child_text(root, FIELD_NAME))
        .filter(|value| !value.is_empty())
        .unwrap_or("mspdi-project")
        .to_owned();
    let tasks_node = child(root, FIELD_TASKS).ok_or_else(|| error("MSPDI Project has no Tasks"))?;
    let mut tasks = Vec::new();
    let mut links = Vec::new();

    for task_node in element_children(tasks_node).filter(|node| node.tag_name().name() == "Task") {
        let task = decode_task(task_node, &mut links, &mut report)?;
        tasks.push(task);
    }

    let plan = GanttPlan::new(id, tasks, links);
    ensure_valid(&plan)?;
    let mut doc = plan_to_doc(cx, &plan)?;
    doc.origin
        .push(ExternalRef::new(MSPDI_CODEC_ID, "Project", None, None));
    Ok((doc, report))
}

pub(crate) fn parse_date(value: &str, field_name: &str) -> Result<Date, OfficeError> {
    let trimmed = value.trim();
    let date_text = trimmed
        .get(..10)
        .ok_or_else(|| error(format!("field {field_name} has invalid date {trimmed}")))?;
    let year = date_text[0..4]
        .parse::<i32>()
        .map_err(|err| error(format!("field {field_name} has invalid year: {err}")))?;
    let month = date_text[5..7]
        .parse::<u8>()
        .map_err(|err| error(format!("field {field_name} has invalid month: {err}")))?;
    let day = date_text[8..10]
        .parse::<u8>()
        .map_err(|err| error(format!("field {field_name} has invalid day: {err}")))?;
    if &date_text[4..5] != "-" || &date_text[7..8] != "-" {
        return Err(error(format!(
            "field {field_name} has invalid date {trimmed}"
        )));
    }
    let month = Month::try_from(month)
        .map_err(|err| error(format!("invalid month in {field_name}: {err}")))?;
    Date::from_calendar_date(year, month, day)
        .map_err(|err| error(format!("invalid date in {field_name}: {err}")))
}

fn decode_task(
    node: Node<'_, '_>,
    links: &mut Vec<TaskLink>,
    report: &mut FidelityReport,
) -> Result<Task, OfficeError> {
    let id = clean_text(child_text(node, FIELD_UID))
        .or_else(|| clean_text(child_text(node, FIELD_ID)))
        .ok_or_else(|| error("MSPDI task has no UID or ID"))?
        .to_owned();
    let name = clean_text(child_text(node, FIELD_NAME))
        .filter(|value| !value.is_empty())
        .unwrap_or(&id)
        .to_owned();
    let start_text = clean_text(child_text(node, FIELD_START))
        .ok_or_else(|| error(format!("task {id} has no Start")))?;
    report_date_time_loss(start_text, &format!("Task[{id}].Start"), report);
    let start = parse_date(start_text, FIELD_START)?;
    let finish_text = clean_text(child_text(node, FIELD_FINISH))
        .ok_or_else(|| error(format!("task {id} has no Finish")))?;
    report_date_time_loss(finish_text, &format!("Task[{id}].Finish"), report);
    let finish = parse_date(finish_text, FIELD_FINISH)?;
    let percent_complete = clean_text(child_text(node, FIELD_PERCENT_COMPLETE))
        .unwrap_or("0")
        .parse::<u8>()
        .map_err(|err| error(format!("task {id} has invalid PercentComplete: {err}")))?;

    for child in element_children(node) {
        match child.tag_name().name() {
            FIELD_UID
            | FIELD_ID
            | FIELD_NAME
            | FIELD_START
            | FIELD_FINISH
            | FIELD_PERCENT_COMPLETE => {}
            PREDECESSOR_LINK => decode_link(&id, child, links, report)?,
            other => report_extra_loss(format!("Task[{id}].{other}"), report),
        }
    }

    Ok(Task::new(id, name, start, finish, percent_complete))
}

fn decode_link(
    successor: &str,
    node: Node<'_, '_>,
    links: &mut Vec<TaskLink>,
    report: &mut FidelityReport,
) -> Result<(), OfficeError> {
    let predecessor = clean_text(child_text(node, PREDECESSOR_UID))
        .ok_or_else(|| error(format!("task {successor} has predecessor link without UID")))?
        .to_owned();
    let kind = clean_text(child_text(node, LINK_TYPE))
        .map(mspdi_link_type)
        .transpose()?
        .unwrap_or(LinkKind::FinishStart);
    let lag_days = link_lag_days(node)?;

    for child in element_children(node) {
        match child.tag_name().name() {
            PREDECESSOR_UID | LINK_TYPE | LINK_LAG | LINK_LAG_FORMAT => {}
            other => {
                report_extra_loss(format!("Task[{successor}].PredecessorLink.{other}"), report)
            }
        }
    }

    links.push(TaskLink::new(
        predecessor,
        successor.to_owned(),
        kind,
        lag_days,
    ));
    Ok(())
}

fn link_lag_days(link: Node<'_, '_>) -> Result<i32, OfficeError> {
    let raw = clean_text(child_text(link, LINK_LAG))
        .map(|value| parse_i32(value, LINK_LAG))
        .transpose()?
        .unwrap_or(0);
    let format = clean_text(child_text(link, LINK_LAG_FORMAT)).unwrap_or(MSPDI_LAG_FORMAT_DAYS);
    match format {
        MSPDI_LAG_FORMAT_DAYS => checked_div_exact(raw, TENTHS_PER_WORKDAY, LINK_LAG),
        other => Err(error(format!("unsupported MSPDI LagFormat {other}"))),
    }
}

fn checked_div_exact(value: i32, divisor: i32, field: &str) -> Result<i32, OfficeError> {
    if value % divisor == 0 {
        Ok(value / divisor)
    } else {
        Err(error(format!(
            "MSPDI {field} value {value} is not an exact whole-day lag"
        )))
    }
}

fn mspdi_link_type(value: &str) -> Result<LinkKind, OfficeError> {
    match value.trim() {
        "0" => Ok(LinkKind::FinishFinish),
        "1" => Ok(LinkKind::FinishStart),
        "2" => Ok(LinkKind::StartFinish),
        "3" => Ok(LinkKind::StartStart),
        other => Err(error(format!("unsupported MSPDI predecessor type {other}"))),
    }
}

fn ensure_valid(plan: &GanttPlan) -> Result<(), OfficeError> {
    validate_gantt_plan(plan).map_err(|err| error(format!("invalid Gantt plan: {err}")))
}

fn report_project_extras(root: Node<'_, '_>, report: &mut FidelityReport) {
    for child in element_children(root) {
        match child.tag_name().name() {
            FIELD_NAME | FIELD_TASKS => {}
            other => report_extra_loss(format!("Project.{other}"), report),
        }
    }
}

fn report_date_time_loss(value: &str, field: &str, report: &mut FidelityReport) {
    let Some(time) = value.trim().get(10..) else {
        return;
    };
    let time = time.trim();
    if !time.is_empty() && time != "T00:00:00" {
        report.dropped.push(LossNote::new(
            format!("{field}.time"),
            "portable Gantt dates preserve local dates, not time-of-day",
        ));
    }
}

fn report_extra_loss(field: impl Into<String>, report: &mut FidelityReport) {
    let field = field.into();
    report.preserved_extras.push(field.clone());
    report.dropped.push(LossNote::new(
        field,
        "not represented in the portable Gantt model",
    ));
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    element_children(node).find(|child| child.tag_name().name() == name)
}

fn child_text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|child| child.text())
}

fn element_children<'a, 'input>(
    node: Node<'a, 'input>,
) -> impl Iterator<Item = Node<'a, 'input>> + 'a {
    node.children().filter(Node::is_element)
}

fn clean_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
