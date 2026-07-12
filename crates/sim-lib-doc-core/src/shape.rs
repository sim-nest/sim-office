//! Shape values for open document kinds.

use std::{fmt, sync::Arc};

use sim_kernel::{
    Cx, DefaultFactory, Expr, Factory, ShapeRef, Symbol, Value,
    shape::{MatchScore, Shape, ShapeDoc, ShapeMatch},
};

use crate::model::{Doc, DocKind};

/// Error reported by the office document core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfficeError {
    /// A kernel factory failed while building a shape value.
    ShapeBuild(String),
}

impl fmt::Display for OfficeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShapeBuild(message) => write!(f, "could not build document shape: {message}"),
        }
    }
}

impl std::error::Error for OfficeError {}

/// A shape that accepts [`Doc`] values with one open [`DocKind`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocKindShape {
    kind: DocKind,
}

impl DocKindShape {
    /// Build a shape for one document kind.
    #[must_use]
    pub fn new(kind: DocKind) -> Self {
        Self { kind }
    }

    /// Borrow the document kind matched by this shape.
    #[must_use]
    pub fn kind(&self) -> &DocKind {
        &self.kind
    }
}

/// Build a kernel shape value for one open document kind.
pub fn doc_shape(kind: &DocKind) -> Result<ShapeRef, OfficeError> {
    DefaultFactory
        .opaque(Arc::new(DocKindShape::new(kind.clone())))
        .map_err(|err| OfficeError::ShapeBuild(err.to_string()))
}

impl Shape for DocKindShape {
    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol::qualified("office/doc", self.kind.0.clone()))
    }

    fn check_value(&self, _cx: &mut Cx, value: Value) -> sim_kernel::Result<ShapeMatch> {
        let Some(doc) = value.object().downcast_ref::<Doc>() else {
            return Ok(ShapeMatch::reject(format!(
                "expected {} document value",
                self.kind.0
            )));
        };
        if doc.kind == self.kind {
            Ok(ShapeMatch::accept(MatchScore::exact(20)))
        } else {
            Ok(ShapeMatch::reject(format!(
                "expected {} document value, found {}",
                self.kind.0, doc.kind.0
            )))
        }
    }

    fn check_expr(&self, _cx: &mut Cx, _expr: &Expr) -> sim_kernel::Result<ShapeMatch> {
        Ok(ShapeMatch::reject(format!(
            "expected {} document value",
            self.kind.0
        )))
    }

    fn describe(&self, _cx: &mut Cx) -> sim_kernel::Result<ShapeDoc> {
        Ok(ShapeDoc::new(format!("office document {}", self.kind.0))
            .with_detail("matches Doc values by open DocKind string"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, NoopEvalPolicy};

    use super::*;

    #[test]
    fn doc_shape_is_object_accessible() {
        let value = doc_shape(&DocKind::new("sheet")).unwrap();
        assert!(value.object().as_shape().is_some());
    }

    #[test]
    fn doc_shape_matches_document_kind() {
        let mut cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
        let body = cx.factory().nil().unwrap();
        let doc = Doc::new(
            DocKind::new("report"),
            crate::DocId::new("r1"),
            body,
            vec![],
        );
        let value = cx.factory().opaque(Arc::new(doc)).unwrap();
        let shape_value = doc_shape(&DocKind::new("report")).unwrap();
        let shape = shape_value.object().as_shape().unwrap();

        assert!(shape.check_value(&mut cx, value).unwrap().accepted);
        assert_eq!(
            shape.describe(&mut cx).unwrap().name,
            "office document report"
        );
    }
}
