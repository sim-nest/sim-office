//! Provider-neutral document projection storage.

use crate::codec::{CodecError, decode_doc, decode_edit, encode_doc, encode_edit};
use sim_kernel::{Datum, NumberLiteral, Symbol};
use sim_lib_doc_core::{Doc, DocId, Edit};
use sim_platform_sqlite::{PreopenedStores, SqliteDriver};
use sim_relation_core::{
    BaseDomain, BindingName, Cell, ColumnName, DomainCatalog, FieldName, FieldType, IndexName,
    ProviderName, RevisionName, Row, RowType, SchemaName, SourceName, StorageRepr, TableName,
};
use sim_relation_migrate::AdoptionManifest;
use sim_relation_plan::{
    AdmissionLimits, ConflictAction, ConflictTarget, FieldRef, Mutation, NamedScalar,
    OrderDirection, OrderKey, Rel, Scalar, ScalarOp, admit_mutation, admit_query,
};
use sim_relation_schema::{
    AcceptAllValues, ColumnBuilder, Constraint, ForeignKey, Index, PhysicalColumn, PhysicalIndex,
    PhysicalSchema, PhysicalTable, PrimaryKey, Schema, SchemaBuilder, TableBuilder,
};
use sim_relation_site::{Bindings, Driver, Limits, RowSink, Session, SiteError, StorageAccess};
use std::{cell::RefCell, fmt, path::Path};

const EMPTY_STORE: &[u8] = include_bytes!("../fixtures/empty-doc-store-v1.sqlite");

/// Stable document-store failure categories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// Invalid caller or schema value.
    Invalid(String),
    /// Persisted codec failure.
    Codec(String),
    /// Relational provider failure.
    Storage(SiteError),
    /// Host materialization failure.
    Host(String),
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for StoreError {}
impl From<SiteError> for StoreError {
    fn from(v: SiteError) -> Self {
        Self::Storage(v)
    }
}
impl From<CodecError> for StoreError {
    fn from(v: CodecError) -> Self {
        Self::Codec(v.to_string())
    }
}
/// Document-store result.
pub type StoreResult<T> = Result<T, StoreError>;

/// A document store with one private provider-neutral session.
pub struct DocStore {
    session: RefCell<Box<dyn Session>>,
    schema: Schema,
    domains: DomainCatalog,
    limits: Limits,
}
impl DocStore {
    /// Opens or creates an exactly compatible store.
    pub fn create(path: &Path) -> StoreResult<Self> {
        if !path.exists() {
            std::fs::write(path, EMPTY_STORE).map_err(|e| StoreError::Host(e.to_string()))?;
        }
        Self::open(path, StorageAccess::ReadWrite)
    }
    /// Opens a legacy store without write authority or stamping it.
    pub fn open_read_only(path: &Path) -> StoreResult<Self> {
        Self::open(path, StorageAccess::ReadOnly)
    }
    fn open(path: &Path, access: StorageAccess) -> StoreResult<Self> {
        let domains = domains()?;
        let schema = document_schema(&domains)?;
        let reference = Symbol::new("office-doc-store");
        let driver = SqliteDriver::new(
            domains.clone(),
            PreopenedStores::new([(reference.clone(), path.to_path_buf())]),
        );
        let limits = Limits::new(100_000, 1_000_000, 256 * 1024 * 1024, 1_000_000)?;
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
    /// Saves a document snapshot.
    pub fn save_doc(&self, doc: &Doc) -> StoreResult<()> {
        self.upsert(
            "docs",
            &["id", "kind", "body"],
            vec![
                text(doc.id.as_str()),
                text(doc.kind.as_str()),
                text(encode_doc(doc)?),
            ],
        )
    }
    /// Loads a document snapshot by id.
    pub fn load_doc(&self, id: &DocId) -> StoreResult<Option<Doc>> {
        self.select(
            "docs",
            &["body"],
            &[("id", text(id.as_str()))],
            &[],
            Some(1),
        )?
        .into_iter()
        .next()
        .map(|r| decode_doc(cell_text(&r, 0)?).map_err(StoreError::from))
        .transpose()
    }
    /// Records a projected ledger commit.
    pub fn project_commit(&self, doc: &DocId, edit: &Edit, seq: u64) -> StoreResult<u64> {
        if &edit.doc != doc {
            return Err(StoreError::Invalid(format!(
                "edit targets {}, not {}",
                edit.doc.as_str(),
                doc.as_str()
            )));
        }
        let signed = sequence(seq)?;
        self.insert(
            "edit_projection",
            &["seq", "doc", "edit", "inverse"],
            vec![
                integer(signed),
                text(doc.as_str()),
                text(encode_edit(edit)?),
                text(encode_edit(&edit.inverted())?),
            ],
        )?;
        Ok(seq)
    }
    /// Returns the inverse edit for the latest projected ledger sequence.
    pub fn undo_last(&self, doc: &DocId) -> StoreResult<Option<Edit>> {
        self.select(
            "edit_projection",
            &["inverse"],
            &[("doc", text(doc.as_str()))],
            &[("seq", OrderDirection::Desc)],
            Some(1),
        )?
        .into_iter()
        .next()
        .map(|r| decode_edit(cell_text(&r, 0)?).map_err(StoreError::from))
        .transpose()
    }
    pub(crate) fn insert(
        &self,
        table: &str,
        columns: &[&str],
        cells: Vec<Cell>,
    ) -> StoreResult<()> {
        self.mutate(table, columns, cells, ConflictAction::Fail)
    }
    pub(crate) fn upsert(
        &self,
        table: &str,
        columns: &[&str],
        cells: Vec<Cell>,
    ) -> StoreResult<()> {
        let assignments = columns
            .iter()
            .zip(&cells)
            .skip(1)
            .map(|(column_name, value)| (column(column_name), Scalar::Literal(value.clone())))
            .collect();
        self.mutate(
            table,
            columns,
            cells,
            ConflictAction::DoUpdate {
                target: ConflictTarget::PrimaryKey,
                assignments,
                predicate: None,
            },
        )
    }
    fn mutate(
        &self,
        table: &str,
        columns: &[&str],
        cells: Vec<Cell>,
        conflict: ConflictAction,
    ) -> StoreResult<()> {
        let ty = row_type(columns, &cells)?;
        let row = Row::new(ty.clone(), cells).map_err(|e| StoreError::Invalid(e.to_string()))?;
        let raw = Mutation::Insert {
            table: table_name(table),
            columns: columns.iter().map(|c| column(c)).collect(),
            input: Box::new(Rel::Values {
                bind: binding("input"),
                row_type: ty,
                rows: vec![row],
            }),
            conflict,
            returning: vec![],
        };
        let plan = admit_mutation(
            raw,
            &self.schema,
            &self.domains,
            RowType::new([]).unwrap(),
            AdmissionLimits::default(),
        )
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
        let bindings = Bindings::new(&RowType::new([]).unwrap(), [])?;
        self.session.borrow_mut().transaction(&mut |transaction| {
            transaction.mutate(&plan, &bindings, &self.limits, &mut VecSink::default())?;
            Ok(())
        })?;
        Ok(())
    }
    pub(crate) fn select(
        &self,
        table: &str,
        columns: &[&str],
        filters: &[(&str, Cell)],
        order: &[(&str, OrderDirection)],
        limit: Option<u64>,
    ) -> StoreResult<Vec<Row>> {
        let bind = "row";
        let mut rel = Rel::Scan {
            source: source("main"),
            table: table_name(table),
            bind: binding(bind),
        };
        for (name, value) in filters {
            rel = Rel::Filter {
                input: Box::new(rel),
                predicate: Scalar::Call(
                    ScalarOp::Eq,
                    vec![field(bind, name), Scalar::Literal(value.clone())],
                ),
            };
        }
        if !order.is_empty() {
            rel = Rel::Order {
                input: Box::new(rel),
                keys: order
                    .iter()
                    .map(|(n, d)| OrderKey {
                        scalar: field(bind, n),
                        direction: *d,
                    })
                    .collect(),
            };
        }
        if limit.is_some() {
            rel = Rel::Limit {
                input: Box::new(rel),
                count: limit,
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
                    scalar: field(bind, n),
                })
                .collect(),
        };
        let plan = admit_query(
            rel,
            &self.schema,
            &self.domains,
            RowType::new([]).unwrap(),
            AdmissionLimits::default(),
        )
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
        let bindings = Bindings::new(&RowType::new([]).unwrap(), [])?;
        let mut sink = VecSink::default();
        self.session
            .borrow_mut()
            .query(&plan, &bindings, &self.limits, &mut sink)?;
        Ok(sink.rows)
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
pub(crate) fn text(v: impl Into<String>) -> Cell {
    Cell::new(BaseDomain::Text.id(), Some(Datum::String(v.into())))
}
pub(crate) fn bytes(v: impl Into<Vec<u8>>) -> Cell {
    Cell::new(BaseDomain::Bytes.id(), Some(Datum::Bytes(v.into())))
}
pub(crate) fn integer(v: i64) -> Cell {
    Cell::new(
        BaseDomain::I64.id(),
        Some(Datum::Number(NumberLiteral {
            domain: Symbol::qualified("core", "i64"),
            canonical: v.to_string(),
        })),
    )
}
pub(crate) fn nullable_text(v: Option<String>) -> Cell {
    v.map(text)
        .unwrap_or_else(|| Cell::null(BaseDomain::Text.id()))
}
pub(crate) fn cell_text(r: &Row, i: usize) -> StoreResult<&str> {
    match r.cells().get(i).and_then(Cell::value) {
        Some(Datum::String(v)) => Ok(v),
        _ => Err(StoreError::Storage(SiteError::Conversion)),
    }
}
pub(crate) fn cell_bytes(r: &Row, i: usize) -> StoreResult<&[u8]> {
    match r.cells().get(i).and_then(Cell::value) {
        Some(Datum::Bytes(v)) => Ok(v),
        _ => Err(StoreError::Storage(SiteError::Conversion)),
    }
}
pub(crate) fn cell_optional_text(r: &Row, i: usize) -> StoreResult<Option<&str>> {
    match r.cells().get(i).and_then(Cell::value) {
        Some(Datum::String(v)) => Ok(Some(v)),
        None => Ok(None),
        _ => Err(StoreError::Storage(SiteError::Conversion)),
    }
}
pub(crate) fn cell_i64(r: &Row, i: usize) -> StoreResult<i64> {
    match r.cells().get(i).and_then(Cell::value) {
        Some(Datum::Number(v)) => v
            .canonical
            .parse()
            .map_err(|_| StoreError::Storage(SiteError::Conversion)),
        _ => Err(StoreError::Storage(SiteError::Conversion)),
    }
}
fn sequence(v: u64) -> StoreResult<i64> {
    i64::try_from(v)
        .map_err(|_| StoreError::Invalid(format!("sequence {v} exceeds signed relational domain")))
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
fn row_type(ns: &[&str], cs: &[Cell]) -> StoreResult<RowType> {
    RowType::new(ns.iter().zip(cs).map(|(n, c)| FieldType {
        name: field_name(n),
        domain: c.domain().clone(),
        nullable: c.value().is_none(),
    }))
    .map_err(|e| StoreError::Invalid(e.to_string()))
}
fn domains() -> StoreResult<DomainCatalog> {
    DomainCatalog::new([
        BaseDomain::I64.spec(),
        BaseDomain::Text.spec(),
        BaseDomain::Bytes.spec(),
    ])
    .map_err(|e| StoreError::Invalid(e.to_string()))
}

/// Exact logical schema corresponding to the legacy DDL and normalized fixture.
pub fn document_schema(domains: &DomainCatalog) -> StoreResult<Schema> {
    let req = |n, d| ColumnBuilder::required(column(n), d).build();
    let nul = |n, d| ColumnBuilder::nullable(column(n), d).build();
    let pk = |t: &str, cs: &[&str]| {
        Constraint::Primary(PrimaryKey {
            name: name(&format!("{t}_pk")),
            columns: cs.iter().map(|c| column(c)).collect(),
        })
    };
    let ix = |n: &str, cs: &[&str]| Index {
        name: name(n),
        columns: cs.iter().map(|c| column(c)).collect(),
        unique: false,
    };
    let docs = TableBuilder::new(table_name("docs"))
        .column(req("id", BaseDomain::Text.id()))
        .column(req("kind", BaseDomain::Text.id()))
        .column(req("body", BaseDomain::Text.id()))
        .constraint(pk("docs", &["id"]))
        .build();
    let edits = TableBuilder::new(table_name("edit_projection"))
        .column(req("seq", BaseDomain::I64.id()))
        .column(req("doc", BaseDomain::Text.id()))
        .column(req("edit", BaseDomain::Text.id()))
        .column(req("inverse", BaseDomain::Text.id()))
        .constraint(pk("edit_projection", &["seq"]))
        .index(ix("edit_projection_doc_seq", &["doc", "seq"]))
        .build();
    let ev = TableBuilder::new(table_name("evidence_facts"))
        .column(req("subject", BaseDomain::Text.id()))
        .column(req("predicate", BaseDomain::Text.id()))
        .column(req("object", BaseDomain::Text.id()))
        .column(req("captured_at_seq", BaseDomain::I64.id()))
        .column(nul("immutable_hint", BaseDomain::Text.id()))
        .constraint(pk(
            "evidence_facts",
            &["subject", "predicate", "object", "captured_at_seq"],
        ))
        .index(ix(
            "evidence_facts_subject_seq",
            &["subject", "captured_at_seq", "predicate", "object"],
        ))
        .build();
    let cap = TableBuilder::new(table_name("web_captures"))
        .column(req("capture_id", BaseDomain::Text.id()))
        .column(req("source_uri", BaseDomain::Text.id()))
        .column(req("body", BaseDomain::Bytes.id()))
        .column(req("exchange_json", BaseDomain::Text.id()))
        .constraint(pk("web_captures", &["capture_id"]))
        .build();
    let rep = TableBuilder::new(table_name("web_representations"))
        .column(req("representation_id", BaseDomain::Text.id()))
        .column(req("capture_id", BaseDomain::Text.id()))
        .column(req("text", BaseDomain::Text.id()))
        .column(req("metadata_json", BaseDomain::Text.id()))
        .constraint(pk("web_representations", &["representation_id"]))
        .constraint(Constraint::Foreign(ForeignKey {
            name: name("web_representations_capture_fk"),
            columns: vec![column("capture_id")],
            target_table: table_name("web_captures"),
            target_columns: vec![column("capture_id")],
        }))
        .build();
    let anc = TableBuilder::new(table_name("web_evidence_anchors"))
        .column(req("anchor_id", BaseDomain::Text.id()))
        .column(req("subject", BaseDomain::Text.id()))
        .column(req("representation_id", BaseDomain::Text.id()))
        .column(req("record_json", BaseDomain::Text.id()))
        .constraint(pk("web_evidence_anchors", &["anchor_id"]))
        .constraint(Constraint::Foreign(ForeignKey {
            name: name("web_evidence_anchors_representation_fk"),
            columns: vec![column("representation_id")],
            target_table: table_name("web_representations"),
            target_columns: vec![column("representation_id")],
        }))
        .index(ix("web_evidence_anchor_subject", &["subject", "anchor_id"]))
        .build();
    SchemaBuilder::new(SchemaName::new(Symbol::new("main")).unwrap())
        .table(docs)
        .table(edits)
        .table(ev)
        .table(cap)
        .table(rep)
        .table(anc)
        .build(domains, &AcceptAllValues)
        .map_err(|e| StoreError::Invalid(e.to_string()))
}

/// Exact manifest required to adopt an unstamped v1 document-store file.
pub fn legacy_adoption_manifest() -> StoreResult<AdoptionManifest> {
    let domains = domains()?;
    let logical_schema = document_schema(&domains)?
        .id()
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
    let physical_schema = legacy_physical_schema()?
        .id()
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
    Ok(AdoptionManifest {
        logical_schema,
        physical_schema,
    })
}

fn legacy_physical_schema() -> StoreResult<PhysicalSchema> {
    let col = |name_: &str, storage, nullable, ordinal| PhysicalColumn {
        name: column(name_),
        domain: match storage {
            StorageRepr::I64 => BaseDomain::I64.id(),
            StorageRepr::Text => BaseDomain::Text.id(),
            _ => BaseDomain::Bytes.id(),
        },
        storage,
        nullable,
        ordinal,
    };
    let index = |name_: &str, columns: &[&str]| PhysicalIndex {
        name: name::<IndexName>(name_),
        columns: columns.iter().map(|v| column(v)).collect(),
        unique: false,
    };
    let table = |name_: &str, columns, indexes| PhysicalTable {
        name: table_name(name_),
        columns,
        indexes,
    };
    let tables = vec![
        table(
            "docs",
            vec![
                col("id", StorageRepr::Text, false, 0),
                col("kind", StorageRepr::Text, false, 1),
                col("body", StorageRepr::Text, false, 2),
            ],
            vec![],
        ),
        table(
            "edit_projection",
            vec![
                col("seq", StorageRepr::I64, true, 0),
                col("doc", StorageRepr::Text, false, 1),
                col("edit", StorageRepr::Text, false, 2),
                col("inverse", StorageRepr::Text, false, 3),
            ],
            vec![index("edit_projection_doc_seq", &["doc", "seq"])],
        ),
        table(
            "evidence_facts",
            vec![
                col("subject", StorageRepr::Text, false, 0),
                col("predicate", StorageRepr::Text, false, 1),
                col("object", StorageRepr::Text, false, 2),
                col("captured_at_seq", StorageRepr::I64, false, 3),
                col("immutable_hint", StorageRepr::Text, true, 4),
            ],
            vec![index(
                "evidence_facts_subject_seq",
                &["subject", "captured_at_seq", "predicate", "object"],
            )],
        ),
        table(
            "web_captures",
            vec![
                col("capture_id", StorageRepr::Text, false, 0),
                col("source_uri", StorageRepr::Text, false, 1),
                col("body", StorageRepr::Bytes, false, 2),
                col("exchange_json", StorageRepr::Text, false, 3),
            ],
            vec![],
        ),
        table(
            "web_evidence_anchors",
            vec![
                col("anchor_id", StorageRepr::Text, false, 0),
                col("subject", StorageRepr::Text, false, 1),
                col("representation_id", StorageRepr::Text, false, 2),
                col("record_json", StorageRepr::Text, false, 3),
            ],
            vec![index(
                "web_evidence_anchor_subject",
                &["subject", "anchor_id"],
            )],
        ),
        table(
            "web_representations",
            vec![
                col("representation_id", StorageRepr::Text, false, 0),
                col("capture_id", StorageRepr::Text, false, 1),
                col("text", StorageRepr::Text, false, 2),
                col("metadata_json", StorageRepr::Text, false, 3),
            ],
            vec![],
        ),
    ];
    PhysicalSchema::normalize(
        ProviderName::new(Symbol::qualified("relation/provider", "sqlite")).unwrap(),
        name::<SchemaName>("main"),
        name::<RevisionName>("doc-store-v1"),
        tables,
    )
    .map_err(|e| StoreError::Invalid(e.to_string()))
}
