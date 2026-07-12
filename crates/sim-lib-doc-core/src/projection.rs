//! Projection selection for office documents.

use std::collections::BTreeMap;

use sim_kernel::{Cx, Symbol, Value};

use crate::{Doc, OfficeError};

/// Open tag key for a preferred document lens.
pub const TAG_LENS: &str = "lens";
/// Open tag key for the intended surface or export target.
pub const TAG_TARGET: &str = "target";
/// Open tag key for a preferred backend or file/service family.
pub const TAG_BACKEND: &str = "backend";
/// Open tag key for statement-specific projections.
pub const TAG_STATEMENT_KIND: &str = "statement-kind";
/// Open tag key for the requested fidelity level.
pub const TAG_FIDELITY: &str = "fidelity";

const LENS_SOURCE: &str = "source";
const LENS_FORMATTED: &str = "formatted";
const TARGET_DECK: &str = "deck";
const TARGET_SCREEN: &str = "screen";
const FIDELITY_SUMMARY: &str = "summary";
const FIDELITY_STANDARD: &str = "standard";
const FIDELITY_FULL: &str = "full";

/// Open projection capabilities.
///
/// Tags are ordinary strings so new office domains, backends, statement kinds,
/// and surface hosts can participate without adding a closed enum to the core.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionCaps {
    /// Open capability tags used by the projection ranker.
    pub tags: BTreeMap<String, String>,
}

impl ProjectionCaps {
    /// Build an empty capability set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert one open tag.
    #[must_use]
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Borrow a tag value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.tags.get(key).map(String::as_str)
    }

    /// Set the preferred document lens.
    #[must_use]
    pub fn lens(self, lens: &str) -> Self {
        self.with_tag(TAG_LENS, lens)
    }

    /// Set the intended surface or export target.
    #[must_use]
    pub fn target(self, target: &str) -> Self {
        self.with_tag(TAG_TARGET, target)
    }

    /// Set the preferred backend or file/service family.
    #[must_use]
    pub fn backend(self, backend: &str) -> Self {
        self.with_tag(TAG_BACKEND, backend)
    }

    /// Set the statement kind.
    #[must_use]
    pub fn statement_kind(self, statement_kind: &str) -> Self {
        self.with_tag(TAG_STATEMENT_KIND, statement_kind)
    }

    /// Set the requested fidelity level.
    #[must_use]
    pub fn fidelity(self, fidelity: &str) -> Self {
        self.with_tag(TAG_FIDELITY, fidelity)
    }
}

/// Office projection caps are surface-cap metadata carried as open tags.
pub type SurfaceCaps = ProjectionCaps;

/// A request to project one document for one capability set.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectionRequest<'a> {
    /// Document being projected.
    pub doc: &'a Doc,
    /// Open surface and export capabilities.
    pub caps: &'a ProjectionCaps,
}

impl<'a> ProjectionRequest<'a> {
    /// Build a projection request.
    #[must_use]
    pub fn new(doc: &'a Doc, caps: &'a ProjectionCaps) -> Self {
        Self { doc, caps }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectionBranch {
    name: &'static str,
    required_tags: &'static [(&'static str, &'static str)],
}

impl ProjectionBranch {
    fn score(self, caps: &ProjectionCaps) -> Option<usize> {
        let mut score = 0;
        for (key, value) in self.required_tags {
            if caps.get(key)? != *value {
                return None;
            }
            score += 10;
        }
        Some(score)
    }
}

const SCREEN_BRANCH: ProjectionBranch = ProjectionBranch {
    name: "screen-pane",
    required_tags: &[(TAG_TARGET, TARGET_SCREEN)],
};
const DECK_BRANCH: ProjectionBranch = ProjectionBranch {
    name: "deck-export",
    required_tags: &[(TAG_TARGET, TARGET_DECK)],
};
const STATEMENT_BRANCH: ProjectionBranch = ProjectionBranch {
    name: "statement-table",
    required_tags: &[(TAG_STATEMENT_KIND, "statement")],
};
const GENERIC_BRANCH: ProjectionBranch = ProjectionBranch {
    name: "generic-doc",
    required_tags: &[],
};

const BRANCHES: &[ProjectionBranch] =
    &[SCREEN_BRANCH, DECK_BRANCH, STATEMENT_BRANCH, GENERIC_BRANCH];

/// Project a document to a deterministic runtime value for the requested caps.
pub fn project(cx: &mut Cx, req: &ProjectionRequest<'_>) -> Result<Value, OfficeError> {
    let branch = rank_branch(req.caps);
    let fidelity = req.caps.get(TAG_FIDELITY).unwrap_or(FIDELITY_STANDARD);
    let lens = req.caps.get(TAG_LENS).unwrap_or(LENS_FORMATTED);
    let mut entries = vec![
        symbol_value(cx, "kind", "office/projection")?,
        string_value(cx, "branch", branch.name)?,
        string_value(cx, "doc-kind", req.doc.kind.as_str())?,
        string_value(cx, "doc-id", req.doc.id.as_str())?,
        string_value(cx, TAG_LENS, lens)?,
        string_value(cx, TAG_FIDELITY, fidelity)?,
    ];

    if fidelity != FIDELITY_SUMMARY {
        entries.push(string_value(
            cx,
            TAG_TARGET,
            req.caps.get(TAG_TARGET).unwrap_or("generic"),
        )?);
        if let Some(backend) = req.caps.get(TAG_BACKEND) {
            entries.push(string_value(cx, TAG_BACKEND, backend)?);
        }
        if let Some(statement_kind) = req.caps.get(TAG_STATEMENT_KIND) {
            entries.push(string_value(cx, TAG_STATEMENT_KIND, statement_kind)?);
        }
        entries.push(body_value(cx, lens, &req.doc.body)?);
    }

    if fidelity == FIDELITY_FULL {
        entries.push(tags_value(cx, req.caps)?);
        entries.push(origin_value(cx, req.doc)?);
    }

    cx.factory().table(entries).map_err(OfficeError::from)
}

fn rank_branch(caps: &ProjectionCaps) -> ProjectionBranch {
    BRANCHES
        .iter()
        .enumerate()
        .filter_map(|(index, branch)| branch.score(caps).map(|score| (*branch, score, index)))
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.2.cmp(&left.2)))
        .map(|(branch, _, _)| branch)
        .unwrap_or(GENERIC_BRANCH)
}

fn symbol_value(cx: &mut Cx, key: &str, value: &str) -> Result<(Symbol, Value), OfficeError> {
    Ok((
        Symbol::new(key),
        cx.factory()
            .symbol(Symbol::new(value.to_owned()))
            .map_err(OfficeError::from)?,
    ))
}

fn string_value(cx: &mut Cx, key: &str, value: &str) -> Result<(Symbol, Value), OfficeError> {
    Ok((
        Symbol::new(key),
        cx.factory()
            .string(value.to_owned())
            .map_err(OfficeError::from)?,
    ))
}

fn body_value(cx: &mut Cx, lens: &str, body: &Value) -> Result<(Symbol, Value), OfficeError> {
    let value = if lens == LENS_SOURCE {
        body.clone()
    } else {
        let display = body.object().display(cx).map_err(OfficeError::from)?;
        cx.factory().string(display).map_err(OfficeError::from)?
    };
    Ok((Symbol::new("body"), value))
}

fn tags_value(cx: &mut Cx, caps: &ProjectionCaps) -> Result<(Symbol, Value), OfficeError> {
    let mut pairs = Vec::with_capacity(caps.tags.len());
    for (key, value) in &caps.tags {
        pairs.push((
            Symbol::new(key.clone()),
            cx.factory()
                .string(value.clone())
                .map_err(OfficeError::from)?,
        ));
    }
    Ok((
        Symbol::new("caps"),
        cx.factory().table(pairs).map_err(OfficeError::from)?,
    ))
}

fn origin_value(cx: &mut Cx, doc: &Doc) -> Result<(Symbol, Value), OfficeError> {
    let mut refs = Vec::with_capacity(doc.origin.len());
    for external in &doc.origin {
        let mut fields = vec![
            string_value(cx, TAG_BACKEND, &external.backend)?,
            string_value(cx, "external-id", &external.external_id)?,
        ];
        if let Some(version) = &external.version {
            fields.push(string_value(cx, "version", version)?);
        }
        if let Some(web_url) = &external.web_url {
            fields.push(string_value(cx, "web-url", web_url)?);
        }
        refs.push(cx.factory().table(fields).map_err(OfficeError::from)?);
    }
    Ok((
        Symbol::new("origin"),
        cx.factory().list(refs).map_err(OfficeError::from)?,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{DefaultFactory, Expr, NoopEvalPolicy};

    use super::*;
    use crate::{DocId, DocKind};

    fn cx() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    fn doc(cx: &mut Cx) -> Doc {
        Doc::new(
            DocKind::new("report"),
            DocId::new("doc-1"),
            cx.factory().string("body text".to_owned()).unwrap(),
            vec![],
        )
    }

    fn projected_expr(cx: &mut Cx, doc: &Doc, caps: &ProjectionCaps) -> Expr {
        project(cx, &ProjectionRequest::new(doc, caps))
            .unwrap()
            .object()
            .as_expr(cx)
            .unwrap()
    }

    fn map_len(expr: &Expr) -> usize {
        let Expr::Map(entries) = expr else {
            panic!("projection must be a map");
        };
        entries.len()
    }

    fn string_field(expr: &Expr, name: &str) -> String {
        let Expr::Map(entries) = expr else {
            panic!("projection must be a map");
        };
        let value = entries
            .iter()
            .find_map(|(key, value)| match key {
                Expr::Symbol(symbol) if symbol.name.as_ref() == name => Some(value),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing field {name}"));
        match value {
            Expr::String(text) => text.clone(),
            Expr::Symbol(symbol) => symbol.to_string(),
            other => panic!("field {name} is not a string-like value: {other:?}"),
        }
    }

    #[test]
    fn summary_projection_has_fewer_fields_than_full() {
        let mut cx = cx();
        let doc = doc(&mut cx);
        let summary = ProjectionCaps::new()
            .target(TARGET_SCREEN)
            .fidelity(FIDELITY_SUMMARY);
        let full = ProjectionCaps::new()
            .target(TARGET_SCREEN)
            .backend("codec/ooxml")
            .statement_kind("statement")
            .fidelity(FIDELITY_FULL);

        let summary_expr = projected_expr(&mut cx, &doc, &summary);
        let full_expr = projected_expr(&mut cx, &doc, &full);

        assert!(map_len(&summary_expr) < map_len(&full_expr));
        assert_eq!(string_field(&summary_expr, "branch"), "screen-pane");
    }

    #[test]
    fn deck_and_screen_targets_select_different_branches() {
        let mut cx = cx();
        let doc = doc(&mut cx);
        let screen = ProjectionCaps::new().target(TARGET_SCREEN);
        let deck = ProjectionCaps::new().target(TARGET_DECK);

        let screen_expr = projected_expr(&mut cx, &doc, &screen);
        let deck_expr = projected_expr(&mut cx, &doc, &deck);

        assert_eq!(string_field(&screen_expr, "branch"), "screen-pane");
        assert_eq!(string_field(&deck_expr, "branch"), "deck-export");
    }

    #[test]
    fn unknown_caps_fall_back_deterministically() {
        let mut cx = cx();
        let doc = doc(&mut cx);
        let caps = ProjectionCaps::new()
            .target("unknown")
            .with_tag("unknown-tag", "value");

        let first = projected_expr(&mut cx, &doc, &caps);
        let second = projected_expr(&mut cx, &doc, &caps);

        assert!(first.canonical_eq(&second));
        assert_eq!(string_field(&first, "branch"), "generic-doc");
    }

    #[test]
    fn source_and_formatted_lenses_rank_without_closed_enum() {
        let mut cx = cx();
        let doc = doc(&mut cx);
        let source = ProjectionCaps::new().lens(LENS_SOURCE);
        let formatted = ProjectionCaps::new().lens(LENS_FORMATTED);

        let source_expr = projected_expr(&mut cx, &doc, &source);
        let formatted_expr = projected_expr(&mut cx, &doc, &formatted);

        assert_eq!(string_field(&source_expr, TAG_LENS), LENS_SOURCE);
        assert_eq!(string_field(&formatted_expr, TAG_LENS), LENS_FORMATTED);
        assert_ne!(source_expr, formatted_expr);
    }
}
