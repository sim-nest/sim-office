//! MSPDI XML writer.

use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, FidelityReport, OfficeError};
use sim_lib_gantt::{GanttPlan, LinkKind, TaskLink, validate_gantt_plan};
use time::Date;

use crate::{MSPDI_CODEC_ID, MSPDI_LAG_FORMAT_DAYS, TENTHS_PER_WORKDAY, doc_to_plan, error};

const MSPDI_XMLNS: &str = "http://schemas.microsoft.com/project";

pub(crate) fn encode(cx: &mut Cx, doc: &Doc) -> Result<(Vec<u8>, FidelityReport), OfficeError> {
    let plan = doc_to_plan(cx, doc)?;
    validate_gantt_plan(&plan).map_err(|err| error(format!("invalid Gantt plan: {err}")))?;
    let xml = project_xml(&plan)?;
    Ok((xml.into_bytes(), FidelityReport::new(MSPDI_CODEC_ID)))
}

pub(crate) fn format_date(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        month_number(date),
        date.day()
    )
}

fn project_xml(plan: &GanttPlan) -> Result<String, OfficeError> {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Project xmlns="{MSPDI_XMLNS}"><Name>{}</Name><Tasks>"#,
        escape_text(&plan.id)
    );
    for (index, task) in plan.tasks.iter().enumerate() {
        xml.push_str(&format!(
            "<Task><UID>{}</UID><ID>{}</ID><Name>{}</Name><Start>{}T00:00:00</Start><Finish>{}T00:00:00</Finish><PercentComplete>{}</PercentComplete>",
            escape_text(&task.id),
            index + 1,
            escape_text(&task.name),
            format_date(task.start),
            format_date(task.finish),
            task.percent_complete
        ));
        for link in plan.links.iter().filter(|link| link.successor == task.id) {
            xml.push_str(&link_xml(link)?);
        }
        xml.push_str("</Task>");
    }
    xml.push_str("</Tasks></Project>");
    Ok(xml)
}

fn link_xml(link: &TaskLink) -> Result<String, OfficeError> {
    let link_lag = link_lag_value(link.lag_days)?;
    Ok(format!(
        "<PredecessorLink><PredecessorUID>{}</PredecessorUID><Type>{}</Type><LinkLag>{}</LinkLag><LagFormat>{}</LagFormat></PredecessorLink>",
        escape_text(&link.predecessor),
        mspdi_link_type(link.kind),
        link_lag,
        MSPDI_LAG_FORMAT_DAYS
    ))
}

fn link_lag_value(lag_days: i32) -> Result<i32, OfficeError> {
    lag_days.checked_mul(TENTHS_PER_WORKDAY).ok_or_else(|| {
        error(format!(
            "Gantt lag_days {lag_days} does not fit MSPDI LinkLag"
        ))
    })
}

fn mspdi_link_type(kind: LinkKind) -> u8 {
    match kind {
        LinkKind::FinishFinish => 0,
        LinkKind::FinishStart => 1,
        LinkKind::StartFinish => 2,
        LinkKind::StartStart => 3,
    }
}

fn month_number(date: Date) -> u8 {
    date.month() as u8
}

fn escape_text(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
