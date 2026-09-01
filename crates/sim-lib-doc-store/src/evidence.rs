//! Evidence-link storage through the checked relational session.
use crate::{
    DocStore,
    store::{
        StoreError, StoreResult, cell_i64, cell_optional_text, cell_text, integer, nullable_text,
        text,
    },
};
use sim_lib_doc_core::{DocId, Evidence, ExternalRef, LinkRole};
use sim_relation_plan::OrderDirection;
/// Attaches an evidence fact.
pub fn attach(store: &DocStore, e: &Evidence) -> StoreResult<()> {
    let object =
        serde_json::to_string(&e.evidence).map_err(|x| StoreError::Codec(x.to_string()))?;
    let seq = i64::try_from(e.captured_at_seq).map_err(|_| {
        StoreError::Invalid(format!(
            "evidence sequence {} exceeds signed relational domain",
            e.captured_at_seq
        ))
    })?;
    store.upsert(
        "evidence_facts",
        &[
            "subject",
            "predicate",
            "object",
            "captured_at_seq",
            "immutable_hint",
        ],
        vec![
            text(e.subject.as_str()),
            text(e.predicate()),
            text(object),
            integer(seq),
            nullable_text(e.immutable_hint.clone()),
        ],
    )
}
/// Returns evidence ordered by capture sequence and stable fact identity.
pub fn evidence_for(store: &DocStore, subject: &DocId) -> StoreResult<Vec<Evidence>> {
    store
        .select(
            "evidence_facts",
            &["predicate", "object", "captured_at_seq", "immutable_hint"],
            &[("subject", text(subject.as_str()))],
            &[
                ("captured_at_seq", OrderDirection::Asc),
                ("predicate", OrderDirection::Asc),
                ("object", OrderDirection::Asc),
            ],
            None,
        )?
        .into_iter()
        .map(|r| {
            let predicate = cell_text(&r, 0)?;
            let role = LinkRole::from_predicate(predicate).ok_or_else(|| {
                StoreError::Codec(format!("unknown evidence predicate {predicate}"))
            })?;
            let reference: ExternalRef = serde_json::from_str(cell_text(&r, 1)?)
                .map_err(|e| StoreError::Codec(e.to_string()))?;
            let seq = cell_i64(&r, 2)?;
            let seq = u64::try_from(seq)
                .map_err(|_| StoreError::Codec(format!("negative evidence sequence {seq}")))?;
            Ok(Evidence::new(
                subject.clone(),
                reference,
                role,
                seq,
                cell_optional_text(&r, 3)?.map(str::to_owned),
            ))
        })
        .collect()
}
