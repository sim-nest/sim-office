//! Open reversible edit contract for document domains.

use sim_kernel::{Cx, Value};

use crate::{Doc, DocId, OfficeError};

/// Reversible edit carried by a document domain.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    /// Document the edit targets.
    pub doc: DocId,
    /// Domain namespace that understands the operation payload.
    pub domain: String,
    /// Domain-owned operation payload.
    pub op: Value,
    /// Domain-owned inverse payload.
    pub inverse: Value,
}

impl Edit {
    /// Builds an open reversible edit.
    #[must_use]
    pub fn new(doc: DocId, domain: impl Into<String>, op: Value, inverse: Value) -> Self {
        Self {
            doc,
            domain: domain.into(),
            op,
            inverse,
        }
    }

    /// Returns the edit that reverses this edit.
    #[must_use]
    pub fn inverted(&self) -> Self {
        Self {
            doc: self.doc.clone(),
            domain: self.domain.clone(),
            op: self.inverse.clone(),
            inverse: self.op.clone(),
        }
    }
}

/// Domain implementation for applying and inverting open edit payloads.
pub trait DomainEdit {
    /// Stable domain namespace for this edit implementation.
    fn domain(&self) -> &'static str;
    /// Applies a domain operation to a document.
    fn apply(&self, cx: &mut Cx, doc: &mut Doc, op: &Value) -> Result<(), OfficeError>;
    /// Produces the domain operation that reverses `op`.
    fn invert(&self, op: &Value) -> Value;
}

/// Returns the inverse edit.
#[must_use]
pub fn invert(edit: &Edit) -> Edit {
    edit.inverted()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, NoopEvalPolicy};

    use crate::DocKind;

    use super::*;

    struct BodySwapEdit {
        inverse: Value,
    }

    impl DomainEdit for BodySwapEdit {
        fn domain(&self) -> &'static str {
            "office/body-swap"
        }

        fn apply(&self, _cx: &mut Cx, doc: &mut Doc, op: &Value) -> Result<(), OfficeError> {
            doc.body = op.clone();
            Ok(())
        }

        fn invert(&self, _op: &Value) -> Value {
            self.inverse.clone()
        }
    }

    #[test]
    fn edit_round_trips_through_double_invert() {
        let cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        let op = cx.factory().string("set title".to_owned()).unwrap();
        let inverse = cx.factory().string("restore title".to_owned()).unwrap();
        let edit = Edit::new(DocId::new("doc-1"), "office/body", op, inverse);

        assert_eq!(invert(&invert(&edit)), edit);
    }

    #[test]
    fn domain_edit_round_trips_without_core_enum_variant() {
        let mut cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        let old_body = cx.factory().string("old".to_owned()).unwrap();
        let new_body = cx.factory().string("new".to_owned()).unwrap();
        let domain = BodySwapEdit {
            inverse: old_body.clone(),
        };
        let mut doc = Doc::new(
            DocKind::new("report"),
            DocId::new("doc-1"),
            old_body.clone(),
            vec![],
        );
        let edit = Edit::new(
            doc.id.clone(),
            domain.domain(),
            new_body.clone(),
            domain.invert(&new_body),
        );

        domain.apply(&mut cx, &mut doc, &edit.op).unwrap();
        assert_eq!(doc.body, new_body);
        domain.apply(&mut cx, &mut doc, &edit.inverse).unwrap();
        assert_eq!(doc.body, old_body);
        assert_eq!(invert(&invert(&edit)), edit);
    }
}
