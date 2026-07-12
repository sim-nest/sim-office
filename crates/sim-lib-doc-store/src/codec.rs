use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};
use sim_kernel::{
    Cx, Datum, DefaultFactory, NoopEvalPolicy, NumberLiteral, Symbol, Value, value_from_datum,
};
use sim_lib_doc_core::{Doc, DocId, DocKind, Edit, ExternalRef};

#[derive(Debug)]
pub(crate) struct CodecError {
    message: String,
}

impl CodecError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CodecError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredSymbol {
    namespace: Option<String>,
    name: String,
}

impl StoredSymbol {
    fn from_symbol(symbol: &Symbol) -> Self {
        Self {
            namespace: symbol.namespace.as_ref().map(ToString::to_string),
            name: symbol.name.to_string(),
        }
    }

    fn into_symbol(self) -> Symbol {
        match self.namespace {
            Some(namespace) => Symbol::qualified(namespace, self.name),
            None => Symbol::new(self.name),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum StoredDatum {
    Nil,
    Bool {
        value: bool,
    },
    Number {
        domain: StoredSymbol,
        canonical: String,
    },
    Symbol {
        symbol: StoredSymbol,
    },
    String {
        value: String,
    },
    Bytes {
        value: Vec<u8>,
    },
    List {
        items: Vec<StoredDatum>,
    },
    Vector {
        items: Vec<StoredDatum>,
    },
    Map {
        entries: Vec<(StoredDatum, StoredDatum)>,
    },
    Set {
        items: Vec<StoredDatum>,
    },
    Node {
        tag: StoredSymbol,
        fields: Vec<(StoredSymbol, StoredDatum)>,
    },
}

impl StoredDatum {
    fn from_datum(datum: &Datum) -> Self {
        match datum {
            Datum::Nil => Self::Nil,
            Datum::Bool(value) => Self::Bool { value: *value },
            Datum::Number(number) => Self::Number {
                domain: StoredSymbol::from_symbol(&number.domain),
                canonical: number.canonical.clone(),
            },
            Datum::Symbol(symbol) => Self::Symbol {
                symbol: StoredSymbol::from_symbol(symbol),
            },
            Datum::String(value) => Self::String {
                value: value.clone(),
            },
            Datum::Bytes(value) => Self::Bytes {
                value: value.clone(),
            },
            Datum::List(items) => Self::List {
                items: items.iter().map(Self::from_datum).collect(),
            },
            Datum::Vector(items) => Self::Vector {
                items: items.iter().map(Self::from_datum).collect(),
            },
            Datum::Map(entries) => Self::Map {
                entries: entries
                    .iter()
                    .map(|(key, value)| (Self::from_datum(key), Self::from_datum(value)))
                    .collect(),
            },
            Datum::Set(items) => Self::Set {
                items: items.iter().map(Self::from_datum).collect(),
            },
            Datum::Node { tag, fields } => Self::Node {
                tag: StoredSymbol::from_symbol(tag),
                fields: fields
                    .iter()
                    .map(|(name, value)| (StoredSymbol::from_symbol(name), Self::from_datum(value)))
                    .collect(),
            },
        }
    }

    fn into_datum(self) -> Datum {
        match self {
            Self::Nil => Datum::Nil,
            Self::Bool { value } => Datum::Bool(value),
            Self::Number { domain, canonical } => Datum::Number(NumberLiteral {
                domain: domain.into_symbol(),
                canonical,
            }),
            Self::Symbol { symbol } => Datum::Symbol(symbol.into_symbol()),
            Self::String { value } => Datum::String(value),
            Self::Bytes { value } => Datum::Bytes(value),
            Self::List { items } => Datum::List(items.into_iter().map(Self::into_datum).collect()),
            Self::Vector { items } => {
                Datum::Vector(items.into_iter().map(Self::into_datum).collect())
            }
            Self::Map { entries } => Datum::Map(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.into_datum(), value.into_datum()))
                    .collect(),
            ),
            Self::Set { items } => Datum::Set(items.into_iter().map(Self::into_datum).collect()),
            Self::Node { tag, fields } => Datum::Node {
                tag: tag.into_symbol(),
                fields: fields
                    .into_iter()
                    .map(|(name, value)| (name.into_symbol(), value.into_datum()))
                    .collect(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct StoredDoc {
    id: String,
    kind: String,
    body: StoredDatum,
    origin: Vec<ExternalRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct StoredEdit {
    doc: String,
    domain: String,
    op: StoredDatum,
    inverse: StoredDatum,
}

pub(crate) fn encode_doc(doc: &Doc) -> Result<String, CodecError> {
    let stored = StoredDoc {
        id: doc.id.as_str().to_owned(),
        kind: doc.kind.as_str().to_owned(),
        body: snapshot_value(&doc.body)?,
        origin: doc.origin.clone(),
    };
    serde_json::to_string(&stored).map_err(|error| CodecError::new(error.to_string()))
}

pub(crate) fn decode_doc(encoded: &str) -> Result<Doc, CodecError> {
    let stored: StoredDoc =
        serde_json::from_str(encoded).map_err(|error| CodecError::new(error.to_string()))?;
    Ok(Doc::new(
        DocKind::new(stored.kind),
        DocId::new(stored.id),
        value_from_stored(stored.body)?,
        stored.origin,
    ))
}

pub(crate) fn encode_edit(edit: &Edit) -> Result<String, CodecError> {
    let stored = StoredEdit {
        doc: edit.doc.as_str().to_owned(),
        domain: edit.domain.clone(),
        op: snapshot_value(&edit.op)?,
        inverse: snapshot_value(&edit.inverse)?,
    };
    serde_json::to_string(&stored).map_err(|error| CodecError::new(error.to_string()))
}

pub(crate) fn decode_edit(encoded: &str) -> Result<Edit, CodecError> {
    let stored: StoredEdit =
        serde_json::from_str(encoded).map_err(|error| CodecError::new(error.to_string()))?;
    Ok(Edit::new(
        DocId::new(stored.doc),
        stored.domain,
        value_from_stored(stored.op)?,
        value_from_stored(stored.inverse)?,
    ))
}

fn snapshot_value(value: &Value) -> Result<StoredDatum, CodecError> {
    let mut cx = scratch_cx();
    let datum = value
        .object()
        .snapshot(&mut cx)
        .map_err(|error| CodecError::new(format!("value snapshot failed: {error}")))?
        .ok_or_else(|| CodecError::new("value does not expose a pure datum snapshot"))?;
    Ok(StoredDatum::from_datum(&datum))
}

fn value_from_stored(stored: StoredDatum) -> Result<Value, CodecError> {
    let mut cx = scratch_cx();
    value_from_datum(&mut cx, stored.into_datum())
        .map_err(|error| CodecError::new(format!("value restore failed: {error}")))
}

fn scratch_cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}
