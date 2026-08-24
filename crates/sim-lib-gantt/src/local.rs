//! Provider-neutral relational persistence for local Gantt plans.

use crate::critical::{ScheduleError, validate_plan};
use crate::model::{GanttPlan, LinkKind, Task, TaskLink};
use sim_kernel::{Datum, NumberLiteral, Symbol};
use sim_platform_sqlite::{PreopenedStores, SqliteDriver};
use sim_relation_core::{
    BaseDomain, BindingName, Cell, ColumnName, DomainCatalog, FieldName, FieldType, ProviderName,
    RevisionName, Row, RowType, SchemaName, SourceName, StorageRepr, TableName,
};
use sim_relation_migrate::AdoptionManifest;
use sim_relation_plan::{
    AdmissionLimits, CheckedMutation, ConflictAction, ConflictTarget, FieldRef, Mutation,
    NamedScalar, OrderDirection, OrderKey, Rel, Scalar, ScalarOp, admit_mutation, admit_query,
};
use sim_relation_schema::{
    AcceptAllValues, ColumnBuilder, Constraint, ForeignKey, PhysicalColumn, PhysicalSchema,
    PhysicalTable, PrimaryKey, Schema, SchemaBuilder, TableBuilder,
};
use sim_relation_site::{Bindings, Driver, Limits, RowSink, Session, SiteError, StorageAccess};
use std::{cell::RefCell, fmt, path::Path};
use time::Date;

const EMPTY_STORE: &[u8] = include_bytes!("../fixtures/empty-gantt-store-v1.sqlite");

/// Gantt persistence through checked relation plans and an opaque storage site.
pub struct GanttStore {
    session: RefCell<Box<dyn Session>>,
    schema: Schema,
    domains: DomainCatalog,
    limits: Limits,
}
impl GanttStore {
    /// Opens or creates an exactly compatible store.
    pub fn create(path: &Path) -> Result<Self, ScheduleError> {
        if !path.exists() {
            std::fs::write(path, EMPTY_STORE).map_err(|e| err(e.to_string()))?;
        }
        Self::open(path, StorageAccess::ReadWrite)
    }
    /// Opens an unstamped old-format file without write authority.
    pub fn open_read_only(path: &Path) -> Result<Self, ScheduleError> {
        Self::open(path, StorageAccess::ReadOnly)
    }
    fn open(path: &Path, access: StorageAccess) -> Result<Self, ScheduleError> {
        let domains = domains()?;
        let schema = gantt_schema(&domains)?;
        let reference = Symbol::new("office-gantt-store");
        let driver = SqliteDriver::new(
            domains.clone(),
            PreopenedStores::new([(reference.clone(), path.to_path_buf())]),
        );
        let limits = Limits::new(100_000, 1_000_000, 64 * 1024 * 1024, 1_000_000)?;
        let locator = Datum::Node {
            tag: Symbol::qualified("relation", "preopened"),
            fields: vec![
                (Symbol::new("ref"), Datum::Symbol(reference)),
                (
                    Symbol::new("access"),
                    Datum::Symbol(Symbol::new(if access == StorageAccess::ReadOnly {
                        "read-only"
                    } else {
                        "read-write"
                    })),
                ),
            ],
        };
        Ok(Self {
            session: RefCell::new(driver.connect(&locator, &limits)?),
            schema,
            domains,
            limits,
        })
    }
    /// Atomically replaces the complete snapshot for a plan id.
    pub fn save_plan(&self, plan: &GanttPlan) -> Result<(), ScheduleError> {
        self.save_inner(plan, false)
    }
    fn save_inner(&self, plan: &GanttPlan, inject_failure: bool) -> Result<(), ScheduleError> {
        validate_plan(plan)?;
        let plan_row = self.insert("gantt_plans", &["id"], vec![text(&plan.id)], true)?;
        let delete_links = self.delete("gantt_links", &plan.id)?;
        let delete_tasks = self.delete("gantt_tasks", &plan.id)?;
        let tasks = plan
            .tasks
            .iter()
            .enumerate()
            .map(|(p, t)| {
                self.insert(
                    "gantt_tasks",
                    &[
                        "plan_id",
                        "id",
                        "name",
                        "start_julian",
                        "finish_julian",
                        "percent_complete",
                        "position",
                    ],
                    vec![
                        text(&plan.id),
                        text(&t.id),
                        text(&t.name),
                        integer(i64::from(t.start.to_julian_day())),
                        integer(i64::from(t.finish.to_julian_day())),
                        integer(i64::from(t.percent_complete)),
                        integer(position(p)?),
                    ],
                    false,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let links = plan
            .links
            .iter()
            .enumerate()
            .map(|(p, l)| {
                self.insert(
                    "gantt_links",
                    &[
                        "plan_id",
                        "predecessor",
                        "successor",
                        "kind",
                        "lag_days",
                        "position",
                    ],
                    vec![
                        text(&plan.id),
                        text(&l.predecessor),
                        text(&l.successor),
                        text(l.kind.as_str()),
                        integer(i64::from(l.lag_days)),
                        integer(position(p)?),
                    ],
                    false,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bindings = empty_bindings()?;
        self.session.borrow_mut().transaction(&mut |tx| {
            let mut sink = VecSink::default();
            for op in [&plan_row, &delete_links, &delete_tasks] {
                tx.mutate(op, &bindings, &self.limits, &mut sink)?;
            }
            for op in &tasks {
                tx.mutate(op, &bindings, &self.limits, &mut sink)?;
            }
            if inject_failure {
                return Err(SiteError::Provider);
            }
            for op in &links {
                tx.mutate(op, &bindings, &self.limits, &mut sink)?;
            }
            Ok(())
        })?;
        Ok(())
    }
    /// Loads a checked snapshot, explicitly ordered by persisted positions.
    pub fn load_plan(&self, id: &str) -> Result<Option<GanttPlan>, ScheduleError> {
        let plans = self.select("gantt_plans", &["id"], &[("id", text(id))], &[], Some(1))?;
        let Some(row) = plans.first() else {
            return Ok(None);
        };
        let plan_id = cell_text(row, 0)?.to_owned();
        let tasks = self
            .select(
                "gantt_tasks",
                &[
                    "id",
                    "name",
                    "start_julian",
                    "finish_julian",
                    "percent_complete",
                ],
                &[("plan_id", text(&plan_id))],
                &[("position", OrderDirection::Asc)],
                None,
            )?
            .into_iter()
            .map(|r| {
                Ok(Task::new(
                    cell_text(&r, 0)?,
                    cell_text(&r, 1)?,
                    date(cell_i64(&r, 2)?, "start_julian")?,
                    date(cell_i64(&r, 3)?, "finish_julian")?,
                    percent(cell_i64(&r, 4)?)?,
                ))
            })
            .collect::<Result<Vec<_>, ScheduleError>>()?;
        let links = self
            .select(
                "gantt_links",
                &["predecessor", "successor", "kind", "lag_days"],
                &[("plan_id", text(&plan_id))],
                &[("position", OrderDirection::Asc)],
                None,
            )?
            .into_iter()
            .map(|r| {
                let token = cell_text(&r, 2)?;
                let kind = LinkKind::from_token(token)
                    .ok_or_else(|| err(format!("unknown gantt link kind {token}")))?;
                Ok(TaskLink::new(
                    cell_text(&r, 0)?,
                    cell_text(&r, 1)?,
                    kind,
                    i32_value(cell_i64(&r, 3)?, "lag_days")?,
                ))
            })
            .collect::<Result<Vec<_>, ScheduleError>>()?;
        Ok(Some(GanttPlan::new(plan_id, tasks, links)))
    }
    fn insert(
        &self,
        table: &str,
        columns: &[&str],
        cells: Vec<Cell>,
        upsert: bool,
    ) -> Result<CheckedMutation, ScheduleError> {
        let ty = row_type(columns, &cells)?;
        let row = Row::new(ty.clone(), cells).map_err(|e| err(e.to_string()))?;
        let conflict = if upsert {
            ConflictAction::DoNothing {
                target: ConflictTarget::PrimaryKey,
            }
        } else {
            ConflictAction::Fail
        };
        admit_mutation(
            Mutation::Insert {
                table: table_name(table),
                columns: columns.iter().map(|v| column(v)).collect(),
                input: Box::new(Rel::Values {
                    bind: binding("input"),
                    row_type: ty,
                    rows: vec![row],
                }),
                conflict,
                returning: vec![],
            },
            &self.schema,
            &self.domains,
            empty_type()?,
            AdmissionLimits::default(),
        )
        .map_err(|e| err(e.to_string()))
    }
    fn delete(&self, table: &str, plan_id: &str) -> Result<CheckedMutation, ScheduleError> {
        admit_mutation(
            Mutation::Delete {
                table: table_name(table),
                bind: binding("row"),
                predicate: Some(Scalar::Call(
                    ScalarOp::Eq,
                    vec![field("row", "plan_id"), Scalar::Literal(text(plan_id))],
                )),
                returning: vec![],
            },
            &self.schema,
            &self.domains,
            empty_type()?,
            AdmissionLimits::default(),
        )
        .map_err(|e| err(e.to_string()))
    }
    fn select(
        &self,
        table: &str,
        columns: &[&str],
        filters: &[(&str, Cell)],
        order: &[(&str, OrderDirection)],
        limit: Option<u64>,
    ) -> Result<Vec<Row>, ScheduleError> {
        let mut rel = Rel::Scan {
            source: source("main"),
            table: table_name(table),
            bind: binding("row"),
        };
        for (name, value) in filters {
            rel = Rel::Filter {
                input: Box::new(rel),
                predicate: Scalar::Call(
                    ScalarOp::Eq,
                    vec![field("row", name), Scalar::Literal(value.clone())],
                ),
            };
        }
        if !order.is_empty() {
            rel = Rel::Order {
                input: Box::new(rel),
                keys: order
                    .iter()
                    .map(|(n, d)| OrderKey {
                        scalar: field("row", n),
                        direction: *d,
                    })
                    .collect(),
            };
        }
        if let Some(count) = limit {
            rel = Rel::Limit {
                input: Box::new(rel),
                count: Some(count),
                offset: 0,
            };
        }
        rel = Rel::Project {
            input: Box::new(rel),
            bind: binding("output"),
            fields: columns
                .iter()
                .map(|n| NamedScalar {
                    name: field_name(n),
                    scalar: field("row", n),
                })
                .collect(),
        };
        let plan = admit_query(
            rel,
            &self.schema,
            &self.domains,
            empty_type()?,
            AdmissionLimits::default(),
        )
        .map_err(|e| err(e.to_string()))?;
        let mut sink = VecSink::default();
        self.session
            .borrow_mut()
            .query(&plan, &empty_bindings()?, &self.limits, &mut sink)?;
        Ok(sink.rows)
    }
}
impl From<SiteError> for ScheduleError {
    fn from(v: SiteError) -> Self {
        err(v.to_string())
    }
}
#[derive(Default)]
struct VecSink {
    rows: Vec<Row>,
}
impl RowSink for VecSink {
    fn push(&mut self, row: Row) -> Result<(), SiteError> {
        self.rows.push(row);
        Ok(())
    }
}
fn err(v: impl Into<String>) -> ScheduleError {
    ScheduleError::Store(v.into())
}
fn text(v: impl Into<String>) -> Cell {
    Cell::new(BaseDomain::Text.id(), Some(Datum::String(v.into())))
}
fn integer(v: i64) -> Cell {
    Cell::new(
        BaseDomain::I64.id(),
        Some(Datum::Number(NumberLiteral {
            domain: Symbol::qualified("core", "i64"),
            canonical: v.to_string(),
        })),
    )
}
fn cell_text(r: &Row, i: usize) -> Result<&str, ScheduleError> {
    match r.cells().get(i).and_then(Cell::value) {
        Some(Datum::String(v)) => Ok(v),
        _ => Err(err("persisted text conversion failed")),
    }
}
fn cell_i64(r: &Row, i: usize) -> Result<i64, ScheduleError> {
    match r.cells().get(i).and_then(Cell::value) {
        Some(Datum::Number(v)) => v
            .canonical
            .parse()
            .map_err(|_| err("persisted integer conversion failed")),
        _ => Err(err("persisted integer conversion failed")),
    }
}
fn date(v: i64, field: &'static str) -> Result<Date, ScheduleError> {
    Date::from_julian_day(i32_value(v, field)?).map_err(|e| err(format!("invalid {field}: {e}")))
}
fn percent(v: i64) -> Result<u8, ScheduleError> {
    let p = u8::try_from(v).map_err(|_| err(format!("percent_complete {v} is out of range")))?;
    if p > 100 {
        Err(err(format!("percent_complete {v} is out of range")))
    } else {
        Ok(p)
    }
}
fn i32_value(v: i64, field: &'static str) -> Result<i32, ScheduleError> {
    i32::try_from(v).map_err(|_| err(format!("{field} {v} is out of range")))
}
fn position(v: usize) -> Result<i64, ScheduleError> {
    i64::try_from(v).map_err(|_| err(format!("position {v} does not fit relational INTEGER")))
}
fn name<T: TryFrom<Symbol>>(v: &str) -> T
where
    T::Error: fmt::Debug,
{
    T::try_from(Symbol::new(v)).expect("static relation name")
}
fn table_name(v: &str) -> TableName {
    name(v)
}
fn column(v: &str) -> ColumnName {
    name(v)
}
fn field_name(v: &str) -> FieldName {
    name(v)
}
fn binding(v: &str) -> BindingName {
    name(v)
}
fn source(v: &str) -> SourceName {
    name(v)
}
fn field(b: &str, n: &str) -> Scalar {
    Scalar::Field(FieldRef {
        binding: binding(b),
        field: field_name(n),
    })
}
fn empty_type() -> Result<RowType, ScheduleError> {
    RowType::new([]).map_err(|e| err(e.to_string()))
}
fn empty_bindings() -> Result<Bindings, ScheduleError> {
    Ok(Bindings::new(&empty_type()?, [])?)
}
fn row_type(ns: &[&str], cs: &[Cell]) -> Result<RowType, ScheduleError> {
    RowType::new(ns.iter().zip(cs).map(|(n, c)| FieldType {
        name: field_name(n),
        domain: c.domain().clone(),
        nullable: c.value().is_none(),
    }))
    .map_err(|e| err(e.to_string()))
}
fn domains() -> Result<DomainCatalog, ScheduleError> {
    DomainCatalog::new([BaseDomain::I64.spec(), BaseDomain::Text.spec()])
        .map_err(|e| err(e.to_string()))
}

/// Checked logical schema corresponding exactly to the legacy Gantt DDL.
pub fn gantt_schema(domains: &DomainCatalog) -> Result<Schema, ScheduleError> {
    let req = |n, d| ColumnBuilder::required(column(n), d).build();
    let pk = |n: &str, cs: &[&str]| {
        Constraint::Primary(PrimaryKey {
            name: name(n),
            columns: cs.iter().map(|v| column(v)).collect(),
        })
    };
    let fk = |n: &str, cs: &[&str], t: &str, ts: &[&str]| {
        Constraint::Foreign(ForeignKey {
            name: name(n),
            columns: cs.iter().map(|v| column(v)).collect(),
            target_table: table_name(t),
            target_columns: ts.iter().map(|v| column(v)).collect(),
        })
    };
    let plans = TableBuilder::new(table_name("gantt_plans"))
        .column(req("id", BaseDomain::Text.id()))
        .constraint(pk("gantt_plans_pk", &["id"]))
        .build();
    let tasks = TableBuilder::new(table_name("gantt_tasks"))
        .column(req("plan_id", BaseDomain::Text.id()))
        .column(req("id", BaseDomain::Text.id()))
        .column(req("name", BaseDomain::Text.id()))
        .column(req("start_julian", BaseDomain::I64.id()))
        .column(req("finish_julian", BaseDomain::I64.id()))
        .column(req("percent_complete", BaseDomain::I64.id()))
        .column(req("position", BaseDomain::I64.id()))
        .constraint(pk("gantt_tasks_pk", &["plan_id", "id"]))
        .constraint(fk(
            "gantt_tasks_plan_fk",
            &["plan_id"],
            "gantt_plans",
            &["id"],
        ))
        .build();
    let links = TableBuilder::new(table_name("gantt_links"))
        .column(req("plan_id", BaseDomain::Text.id()))
        .column(req("predecessor", BaseDomain::Text.id()))
        .column(req("successor", BaseDomain::Text.id()))
        .column(req("kind", BaseDomain::Text.id()))
        .column(req("lag_days", BaseDomain::I64.id()))
        .column(req("position", BaseDomain::I64.id()))
        .constraint(pk("gantt_links_pk", &["plan_id", "position"]))
        .constraint(fk(
            "gantt_links_plan_fk",
            &["plan_id"],
            "gantt_plans",
            &["id"],
        ))
        .constraint(fk(
            "gantt_links_predecessor_fk",
            &["plan_id", "predecessor"],
            "gantt_tasks",
            &["plan_id", "id"],
        ))
        .constraint(fk(
            "gantt_links_successor_fk",
            &["plan_id", "successor"],
            "gantt_tasks",
            &["plan_id", "id"],
        ))
        .build();
    SchemaBuilder::new(name::<SchemaName>("main"))
        .table(plans)
        .table(tasks)
        .table(links)
        .build(domains, &AcceptAllValues)
        .map_err(|e| err(e.to_string()))
}
/// Logical and normalized physical identities for exact old-file adoption.
pub fn legacy_adoption_manifest() -> Result<AdoptionManifest, ScheduleError> {
    Ok(AdoptionManifest {
        logical_schema: gantt_schema(&domains()?)?
            .id()
            .map_err(|e| err(e.to_string()))?,
        physical_schema: physical_schema()?.id().map_err(|e| err(e.to_string()))?,
    })
}
fn physical_schema() -> Result<PhysicalSchema, ScheduleError> {
    let col = |n: &str, s, o| PhysicalColumn {
        name: column(n),
        domain: if s == StorageRepr::I64 {
            BaseDomain::I64.id()
        } else {
            BaseDomain::Text.id()
        },
        storage: s,
        nullable: false,
        ordinal: o,
    };
    PhysicalSchema::normalize(
        ProviderName::new(Symbol::qualified("relation/provider", "sqlite"))
            .expect("static provider name"),
        name::<SchemaName>("main"),
        name::<RevisionName>("gantt-store-v1"),
        vec![
            PhysicalTable {
                name: table_name("gantt_plans"),
                columns: vec![col("id", StorageRepr::Text, 0)],
                indexes: vec![],
            },
            PhysicalTable {
                name: table_name("gantt_tasks"),
                columns: vec![
                    col("plan_id", StorageRepr::Text, 0),
                    col("id", StorageRepr::Text, 1),
                    col("name", StorageRepr::Text, 2),
                    col("start_julian", StorageRepr::I64, 3),
                    col("finish_julian", StorageRepr::I64, 4),
                    col("percent_complete", StorageRepr::I64, 5),
                    col("position", StorageRepr::I64, 6),
                ],
                indexes: vec![],
            },
            PhysicalTable {
                name: table_name("gantt_links"),
                columns: vec![
                    col("plan_id", StorageRepr::Text, 0),
                    col("predecessor", StorageRepr::Text, 1),
                    col("successor", StorageRepr::Text, 2),
                    col("kind", StorageRepr::Text, 3),
                    col("lag_days", StorageRepr::I64, 4),
                    col("position", StorageRepr::I64, 5),
                ],
                indexes: vec![],
            },
        ],
    )
    .map_err(|e| err(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;
    const LEGACY: &[u8] = include_bytes!("../fixtures/legacy-gantt-store-v1.sqlite");
    fn d(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::July, day).unwrap()
    }
    fn plan(name: &str) -> GanttPlan {
        GanttPlan::new(
            "plan-1",
            vec![
                Task::new("build", name, d(3), d(5), 0),
                Task::new("design", "Design", d(1), d(3), 50),
            ],
            vec![TaskLink::new("design", "build", LinkKind::FinishStart, 0)],
        )
    }
    #[test]
    fn legacy_fixture_freezes_order_and_exact_plan() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.sqlite");
        std::fs::write(&path, LEGACY).unwrap();
        let store = GanttStore::create(&path).unwrap();
        assert_eq!(store.load_plan("plan-1").unwrap(), Some(plan("Build")));
    }
    #[test]
    fn overwrite_replaces_children_and_preserves_new_order() {
        let dir = tempfile::tempdir().unwrap();
        let store = GanttStore::create(&dir.path().join("g.sqlite")).unwrap();
        store.save_plan(&plan("Old")).unwrap();
        let new = plan("Replacement");
        store.save_plan(&new).unwrap();
        assert_eq!(store.load_plan("plan-1").unwrap(), Some(new));
    }
    #[test]
    fn injected_middle_failure_rolls_back_every_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = GanttStore::create(&dir.path().join("g.sqlite")).unwrap();
        let old = plan("Original");
        store.save_plan(&old).unwrap();
        assert!(store.save_inner(&plan("Rollback"), true).is_err());
        assert_eq!(store.load_plan("plan-1").unwrap(), Some(old));
    }
    #[test]
    fn read_only_old_file_is_byte_exact_and_unstamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.sqlite");
        std::fs::write(&path, LEGACY).unwrap();
        let before = std::fs::read(&path).unwrap();
        let store = GanttStore::open_read_only(&path).unwrap();
        assert!(store.load_plan("plan-1").unwrap().is_some());
        drop(store);
        assert_eq!(std::fs::read(path).unwrap(), before);
        let manifest = legacy_adoption_manifest().unwrap();
        assert_ne!(manifest.logical_schema, manifest.physical_schema);
    }
}
