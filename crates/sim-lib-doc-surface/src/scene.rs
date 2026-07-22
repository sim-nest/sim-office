//! Scene projection for suite document panes.

use std::collections::BTreeMap;

use sim_kernel::{Cx, Expr, Value};
use sim_lib_doc_core::{
    Doc, DocId, OfficeError, ProjectionRequest, SurfaceCaps, TAG_FIDELITY, TAG_LENS, TAG_TARGET,
    project,
};
use sim_lib_scene::{node, sym, validate_scene};

/// One document pane requested by a suite surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuitePane {
    /// Document id shown in this pane.
    pub doc: DocId,
    /// Open surface caps used to project the document.
    pub caps: SurfaceCaps,
}

impl SuitePane {
    /// Build a suite pane request.
    #[must_use]
    pub fn new(doc: DocId, caps: SurfaceCaps) -> Self {
        Self { doc, caps }
    }
}

/// Project documents into a deterministic Scene value for suite hosts.
///
/// When `panes` is empty, every document is rendered once in stable document-id
/// order with default screen caps. When `panes` is supplied, pane order is the
/// caller's layout order while document lookup stays deterministic.
pub fn suite_scene(cx: &mut Cx, panes: &[SuitePane], docs: &[Doc]) -> Result<Value, OfficeError> {
    let docs_by_id = docs_by_id(docs);
    let active_panes = active_panes(panes, docs);
    let mut children = Vec::with_capacity(active_panes.len());
    for pane in &active_panes {
        let doc = docs_by_id.get(&pane.doc).copied().ok_or_else(|| {
            OfficeError::Surface(format!("unknown suite pane doc {}", pane.doc.0))
        })?;
        children.push(pane_scene(cx, pane, doc)?);
    }
    let scene = node(
        "stack",
        vec![
            ("id", sym("office-suite")),
            ("role", sym("suite")),
            ("dir", sym("column")),
            ("children", Expr::List(children)),
        ],
    );
    validate_scene(&scene)
        .map_err(|error| OfficeError::Surface(format!("suite scene did not validate: {error}")))?;
    cx.factory().expr(scene).map_err(OfficeError::from)
}

fn docs_by_id(docs: &[Doc]) -> BTreeMap<DocId, &Doc> {
    docs.iter().map(|doc| (doc.id.clone(), doc)).collect()
}

fn active_panes(panes: &[SuitePane], docs: &[Doc]) -> Vec<SuitePane> {
    if !panes.is_empty() {
        return panes.to_vec();
    }
    let mut docs: Vec<&Doc> = docs.iter().collect();
    docs.sort_by(|left, right| left.id.cmp(&right.id));
    docs.into_iter()
        .map(|doc| {
            SuitePane::new(
                doc.id.clone(),
                SurfaceCaps::new()
                    .target("screen")
                    .lens("formatted")
                    .fidelity("standard"),
            )
        })
        .collect()
}

fn pane_scene(cx: &mut Cx, pane: &SuitePane, doc: &Doc) -> Result<Expr, OfficeError> {
    let projected = project(cx, &ProjectionRequest::new(doc, &pane.caps))?;
    let projected = projected.object().as_expr(cx).map_err(OfficeError::from)?;
    Ok(node(
        "box",
        vec![
            ("id", Expr::String(pane.doc.0.clone())),
            ("role", sym("suite-pane")),
            (
                "children",
                Expr::List(vec![
                    sim_lib_scene::text_node(format!("{} {}", doc.kind.0, doc.id.0)),
                    caps_summary(&pane.caps),
                    node(
                        "embed",
                        vec![
                            ("lens", sym("view:default")),
                            ("scene", projection_scene(projected)),
                        ],
                    ),
                ]),
            ),
        ],
    ))
}

fn caps_summary(caps: &SurfaceCaps) -> Expr {
    let lens = caps.get(TAG_LENS).unwrap_or("formatted");
    let target = caps.get(TAG_TARGET).unwrap_or("screen");
    let fidelity = caps.get(TAG_FIDELITY).unwrap_or("standard");
    sim_lib_scene::badge("projection", &format!("{target}/{lens}/{fidelity}"))
}

fn projection_scene(projected: Expr) -> Expr {
    node(
        "box",
        vec![
            ("role", sym("projection")),
            (
                "children",
                Expr::List(vec![sim_lib_scene::text_node(format!("{projected:?}"))]),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use sim_kernel::testing::bare_cx as cx;
    use sim_lib_doc_core::{DocKind, ProjectionCaps};

    use super::*;

    // conformance: document surfaces project suite panes into checked scenes.

    fn doc(cx: &mut Cx, id: &str, kind: &str, body: &str) -> Doc {
        Doc::new(
            DocKind::new(kind),
            DocId::new(id),
            cx.factory().string(body.to_owned()).unwrap(),
            vec![],
        )
    }

    fn expr_of(cx: &mut Cx, value: Value) -> Expr {
        value.object().as_expr(cx).unwrap()
    }

    fn child_ids(scene: &Expr) -> Vec<String> {
        let children = field(scene, "children").and_then(|value| match value {
            Expr::List(children) => Some(children),
            _ => None,
        });
        children
            .into_iter()
            .flatten()
            .filter_map(|child| match field(child, "id") {
                Some(Expr::String(id)) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    fn field<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
        let Expr::Map(entries) = expr else {
            return None;
        };
        entries.iter().find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
    }

    #[test]
    fn mixed_docs_produce_stable_node_order() {
        let mut cx = cx();
        let docs = vec![
            doc(&mut cx, "doc-b", "report", "b"),
            doc(&mut cx, "doc-a", "article", "a"),
        ];

        let first_value = suite_scene(&mut cx, &[], &docs).unwrap();
        let first = expr_of(&mut cx, first_value);
        let second_value = suite_scene(&mut cx, &[], &docs).unwrap();
        let second = expr_of(&mut cx, second_value);

        sim_lib_scene::validate_scene(&first).unwrap();
        assert_eq!(first, second);
        assert_eq!(child_ids(&first), ["doc-a", "doc-b"]);
    }

    #[test]
    fn requested_panes_keep_layout_order() {
        let mut cx = cx();
        let docs = vec![
            doc(&mut cx, "doc-a", "article", "a"),
            doc(&mut cx, "doc-b", "report", "b"),
        ];
        let panes = vec![
            SuitePane::new(DocId::new("doc-b"), ProjectionCaps::new().target("deck")),
            SuitePane::new(DocId::new("doc-a"), ProjectionCaps::new().target("screen")),
        ];

        let scene_value = suite_scene(&mut cx, &panes, &docs).unwrap();
        let scene = expr_of(&mut cx, scene_value);

        assert_eq!(child_ids(&scene), ["doc-b", "doc-a"]);
    }

    #[test]
    fn headless_view_doc_fixture_accepts_suite_scene_as_cached_embed() {
        let mut cx = cx();
        let docs = vec![doc(&mut cx, "doc-a", "article", "a")];
        let scene_value = suite_scene(&mut cx, &[], &docs).unwrap();
        let scene = expr_of(&mut cx, scene_value);
        sim_lib_scene::validate_scene(&scene).unwrap();

        let block = sim_lib_view_doc::embed_block(Expr::String("suite".to_owned()), "view:default");
        let cached = sim_lib_view_doc::with_cache(block, scene);
        let article = sim_lib_view_doc::article("Suite", vec![cached]);
        let formatted = sim_lib_view_doc::article_formatted(&article);

        sim_lib_scene::validate_scene(&formatted).unwrap();
    }
}
