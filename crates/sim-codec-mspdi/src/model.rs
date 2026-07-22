//! Portable Gantt document expression projection.

use sim_kernel::{Cx, Expr, Symbol};
use sim_lib_doc_core::{Doc, DocId, DocKind, OfficeError};
use sim_lib_gantt::{GANTT_DOC_KIND, GanttPlan, LinkKind, Task, TaskLink};

use crate::{error, read, write};

const FIELD_FINISH: &str = "finish";
const FIELD_ID: &str = "id";
const FIELD_KIND: &str = "kind";
const FIELD_LAG_DAYS: &str = "lag_days";
const FIELD_LINKS: &str = "links";
const FIELD_NAME: &str = "name";
const FIELD_PERCENT_COMPLETE: &str = "percent_complete";
const FIELD_PREDECESSOR: &str = "predecessor";
const FIELD_START: &str = "start";
const FIELD_SUCCESSOR: &str = "successor";
const FIELD_TASKS: &str = "tasks";

/// Converts a Gantt plan into an office document.
pub fn plan_to_doc(cx: &mut Cx, plan: &GanttPlan) -> Result<Doc, OfficeError> {
    let body = cx.factory().expr(plan_to_expr(plan))?;
    Ok(Doc::new(
        DocKind::new(GANTT_DOC_KIND),
        DocId::new(plan.id.clone()),
        body,
        Vec::new(),
    ))
}

/// Decodes a Gantt plan from an office document.
pub fn doc_to_plan(cx: &mut Cx, doc: &Doc) -> Result<GanttPlan, OfficeError> {
    if doc.kind.as_str() != GANTT_DOC_KIND {
        return Err(error(format!(
            "MSPDI codec does not encode document kind {}",
            doc.kind.as_str()
        )));
    }
    let expr = doc.body.object().as_expr(cx)?;
    plan_from_expr(&expr)
}

fn plan_to_expr(plan: &GanttPlan) -> Expr {
    map(vec![
        field(FIELD_KIND, Expr::Symbol(office_symbol(GANTT_DOC_KIND))),
        field(FIELD_ID, Expr::String(plan.id.clone())),
        field(
            FIELD_TASKS,
            Expr::List(plan.tasks.iter().map(task_to_expr).collect()),
        ),
        field(
            FIELD_LINKS,
            Expr::List(plan.links.iter().map(link_to_expr).collect()),
        ),
    ])
}

fn plan_from_expr(expr: &Expr) -> Result<GanttPlan, OfficeError> {
    let entries = expect_map(expr)?;
    expect_kind(entries, GANTT_DOC_KIND)?;
    let id = expect_string(field_value(entries, FIELD_ID)?, FIELD_ID)?.to_owned();
    let tasks = expect_list(field_value(entries, FIELD_TASKS)?, FIELD_TASKS)?
        .iter()
        .map(task_from_expr)
        .collect::<Result<Vec<_>, _>>()?;
    let links = expect_list(field_value(entries, FIELD_LINKS)?, FIELD_LINKS)?
        .iter()
        .map(link_from_expr)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GanttPlan::new(id, tasks, links))
}

fn task_to_expr(task: &Task) -> Expr {
    map(vec![
        field(FIELD_ID, Expr::String(task.id.clone())),
        field(FIELD_NAME, Expr::String(task.name.clone())),
        field(FIELD_START, Expr::String(write::format_date(task.start))),
        field(FIELD_FINISH, Expr::String(write::format_date(task.finish))),
        field(
            FIELD_PERCENT_COMPLETE,
            Expr::String(task.percent_complete.to_string()),
        ),
    ])
}

fn task_from_expr(expr: &Expr) -> Result<Task, OfficeError> {
    let entries = expect_map(expr)?;
    let id = expect_string(field_value(entries, FIELD_ID)?, FIELD_ID)?.to_owned();
    let name = expect_string(field_value(entries, FIELD_NAME)?, FIELD_NAME)?.to_owned();
    let start = read::parse_date(
        expect_string(field_value(entries, FIELD_START)?, FIELD_START)?,
        FIELD_START,
    )?;
    let finish = read::parse_date(
        expect_string(field_value(entries, FIELD_FINISH)?, FIELD_FINISH)?,
        FIELD_FINISH,
    )?;
    let percent_complete = parse_u8(
        expect_string(
            field_value(entries, FIELD_PERCENT_COMPLETE)?,
            FIELD_PERCENT_COMPLETE,
        )?,
        FIELD_PERCENT_COMPLETE,
    )?;
    Ok(Task::new(id, name, start, finish, percent_complete))
}

fn link_to_expr(link: &TaskLink) -> Expr {
    map(vec![
        field(FIELD_PREDECESSOR, Expr::String(link.predecessor.clone())),
        field(FIELD_SUCCESSOR, Expr::String(link.successor.clone())),
        field(FIELD_KIND, Expr::String(link.kind.as_str().to_owned())),
        field(FIELD_LAG_DAYS, Expr::String(link.lag_days.to_string())),
    ])
}

fn link_from_expr(expr: &Expr) -> Result<TaskLink, OfficeError> {
    let entries = expect_map(expr)?;
    let predecessor =
        expect_string(field_value(entries, FIELD_PREDECESSOR)?, FIELD_PREDECESSOR)?.to_owned();
    let successor =
        expect_string(field_value(entries, FIELD_SUCCESSOR)?, FIELD_SUCCESSOR)?.to_owned();
    let kind_text = expect_string(field_value(entries, FIELD_KIND)?, FIELD_KIND)?;
    let kind = LinkKind::from_token(kind_text)
        .ok_or_else(|| error(format!("unsupported Gantt link kind {kind_text}")))?;
    let lag_days = parse_i32(
        expect_string(field_value(entries, FIELD_LAG_DAYS)?, FIELD_LAG_DAYS)?,
        FIELD_LAG_DAYS,
    )?;
    Ok(TaskLink::new(predecessor, successor, kind, lag_days))
}

pub(crate) fn parse_i32(value: &str, field_name: &'static str) -> Result<i32, OfficeError> {
    value
        .parse::<i32>()
        .map_err(|err| error(format!("invalid {field_name} {value}: {err}")))
}

fn parse_u8(value: &str, field_name: &'static str) -> Result<u8, OfficeError> {
    value
        .parse::<u8>()
        .map_err(|err| error(format!("invalid {field_name} {value}: {err}")))
}

fn map(entries: Vec<(Expr, Expr)>) -> Expr {
    Expr::Map(entries)
}

fn field(name: &'static str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

fn office_symbol(name: &'static str) -> Symbol {
    Symbol::qualified("office", name)
}

fn field_value<'a>(
    entries: &'a [(Expr, Expr)],
    name: &'static str,
) -> Result<&'a Expr, OfficeError> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == &Symbol::new(name) => Some(value),
            _ => None,
        })
        .ok_or_else(|| error(format!("missing field {name}")))
}

fn expect_map(expr: &Expr) -> Result<&[(Expr, Expr)], OfficeError> {
    match expr {
        Expr::Map(entries) => Ok(entries),
        _ => Err(error("expected map")),
    }
}

fn expect_list<'a>(expr: &'a Expr, field_name: &'static str) -> Result<&'a [Expr], OfficeError> {
    match expr {
        Expr::List(items) => Ok(items),
        _ => Err(error(format!("field {field_name} must be a list"))),
    }
}

fn expect_string<'a>(expr: &'a Expr, field_name: &'static str) -> Result<&'a str, OfficeError> {
    match expr {
        Expr::String(value) => Ok(value),
        _ => Err(error(format!("field {field_name} must be a string"))),
    }
}

fn expect_kind(entries: &[(Expr, Expr)], expected: &'static str) -> Result<(), OfficeError> {
    let Expr::Symbol(kind) = field_value(entries, FIELD_KIND)? else {
        return Err(error("field kind must be a symbol"));
    };
    let expected = office_symbol(expected);
    if kind == &expected {
        Ok(())
    } else {
        Err(error(format!(
            "expected kind {}, got {}",
            expected,
            kind.as_qualified_str()
        )))
    }
}
