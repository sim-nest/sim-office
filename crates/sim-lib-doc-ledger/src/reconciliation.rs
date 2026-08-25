//! Exact, non-authoritative reconciliation review documents.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sim_ledger::{
    Candidate, CandidateKind, CorrectionDraft, DecisionDisposition, DecisionRecord,
    ReconciliationCertificate, ReconciliationInputs, ReconciliationRef, verify_certificate,
};
use sim_lib_doc_core::OfficeError;

/// Structural lifecycle state rendered by a review document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionState {
    /// Source evidence was admitted into the frozen snapshot.
    Imported,
    /// An exact candidate was generated but has no human decision.
    Proposed,
    /// A human accepted the exact relation.
    Accepted,
    /// A human rejected the exact relation.
    Rejected,
    /// A human explicitly deferred the relation.
    Deferred,
    /// A balanced correction draft is displayed, but not approved or posted.
    Corrected,
    /// A supplied certificate was independently verified before projection.
    Verified,
    /// Independent verification proved complete coverage and zero residual.
    Closed,
}

/// Exact statement-side evidence exposed to the reviewer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementRowView {
    /// Stable imported source identity.
    pub source_identity: String,
    /// Opaque source/export reference for exact provenance.
    pub source_ref: String,
    /// Exact signed amount in minor units.
    pub amount_minor: i64,
    /// Currency declared by the imported row.
    pub currency: String,
    /// Canonical transaction date.
    pub date: String,
    /// Current structural state.
    pub state: DecisionState,
}

/// Exact ledger-side evidence exposed to the reviewer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerMovementView {
    /// Voucher identity; adjacent voucher contents are deliberately absent.
    pub voucher_id: i64,
    /// Exact signed movement of the reconciled account in minor units.
    pub amount_minor: i64,
    /// Currency declared by the frozen movement.
    pub currency: String,
    /// Canonical posting date.
    pub date: String,
    /// Current structural state.
    pub state: DecisionState,
}

/// Human-readable, deterministic reason for proposing an exact candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReason {
    /// Candidate shape.
    pub kind: String,
    /// Both selected sets have this exact signed total in minor units.
    pub exact_total_minor: i64,
    /// Deterministic review ordering only; it has no authority.
    pub review_rank: u64,
}

/// One exact proposed or decided relation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCandidate {
    /// Exact statement source identities.
    pub row_ids: Vec<String>,
    /// Exact ledger voucher identities.
    pub voucher_ids: Vec<i64>,
    /// Why this relation appears in review.
    pub reason: CandidateReason,
    /// Human decision identity, when one exists.
    pub decision_ref: Option<String>,
    /// Structural state, visually distinct when rendered by a consumer.
    pub state: DecisionState,
}

/// One immutable human decision, including decisions outside generated candidates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanDecisionView {
    /// Stable ledger journal identity.
    pub decision_ref: String,
    /// Exact statement and voucher evidence selected by the reviewer.
    pub evidence: ReconciliationRef,
    /// Structural outcome of the decision.
    pub state: DecisionState,
}

/// One exact term of `statement total - ledger total = residual`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeTerm {
    /// Stable term name.
    pub name: String,
    /// Signed value in minor units.
    pub amount_minor: i64,
    /// Source identities resolving the displayed value.
    pub evidence_refs: Vec<String>,
}

/// A balanced correction shown for review only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionDraftView {
    /// Decision which authorized preparation, not posting.
    pub decision_ref: String,
    /// Exact signed posting amounts in minor units.
    pub posting_amounts_minor: Vec<i64>,
    /// Structural state.
    pub state: DecisionState,
}

/// Complete reconciliation review projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationReviewDocument {
    /// Frozen source identity checked on re-import.
    pub snapshot_ref: String,
    /// Exact cutoff.
    pub cutoff: String,
    /// Sole reviewer identity checked on re-import.
    pub reviewer_ref: String,
    /// Statement evidence rows.
    pub rows: Vec<StatementRowView>,
    /// In-scope ledger movements only.
    pub movements: Vec<LedgerMovementView>,
    /// Proposed and decided exact relations.
    pub candidates: Vec<ReviewCandidate>,
    /// Every immutable human decision in the certificate.
    pub decisions: Vec<HumanDecisionView>,
    /// Statement identities not covered by accepted evidence.
    pub uncovered_rows: Vec<String>,
    /// Voucher identities not covered by accepted evidence.
    pub uncovered_vouchers: Vec<i64>,
    /// Exact arithmetic bridge.
    pub bridge: Vec<BridgeTerm>,
    /// Balanced proposals, never approvals.
    pub correction_drafts: Vec<CorrectionDraftView>,
    /// Verified/closed state only when based on an independently verified certificate.
    pub state: DecisionState,
    /// Human edits are an explicit, initially empty import lane.
    pub edits: Vec<ReviewEdit>,
}

/// Explicit edit accepted by the review import lane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEdit {
    /// New immutable decision identity.
    pub decision_ref: String,
    /// Human author, which must equal the frozen reviewer.
    pub decided_by: String,
    /// Explicit disposition.
    pub disposition: DecisionDisposition,
    /// Exact evidence selected by the human.
    pub evidence: ReconciliationRef,
}

/// Checked command intent. Executing and journaling it belongs to the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationCommand {
    /// Append one immutable human decision record.
    RecordDecision(DecisionRecord),
}

/// Parsed review export which still has no accounting authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedReviewDocument {
    /// Exact checked command intents decoded from the explicit edit lane.
    pub commands: Vec<ReconciliationCommand>,
}

/// Project frozen reconciliation facts into a bounded review document.
pub fn project_reconciliation_review(
    inputs: &ReconciliationInputs,
    candidates: &[Candidate],
    certificate: &ReconciliationCertificate,
    corrections: &[CorrectionDraft],
    independently_verified: bool,
) -> Result<ReconciliationReviewDocument, OfficeError> {
    if certificate.source_inputs != *inputs || certificate.snapshot_ref != inputs.snapshot_ref {
        return Err(error(
            "certificate does not bind the supplied frozen inputs",
        ));
    }
    if independently_verified {
        verify_certificate(inputs, &certificate.decision_records, certificate).map_err(error)?;
    }
    let decisions = certificate
        .decision_records
        .iter()
        .map(|decision| {
            (
                (&decision.evidence.row_ids, &decision.evidence.voucher_ids),
                decision,
            )
        })
        .collect::<Vec<_>>();
    let accepted_rows = certificate
        .accepted
        .iter()
        .flat_map(|d| d.evidence.row_ids.iter())
        .collect::<BTreeSet<_>>();
    let accepted_vouchers = certificate
        .accepted
        .iter()
        .flat_map(|d| d.evidence.voucher_ids.iter())
        .collect::<BTreeSet<_>>();
    let rows = inputs
        .statement
        .rows
        .iter()
        .map(|row| StatementRowView {
            source_identity: row.source_identity.clone(),
            source_ref: row.source_ref.clone(),
            amount_minor: row.amount.0,
            currency: row.currency.clone(),
            date: row.date.clone(),
            state: if accepted_rows.contains(&row.source_identity) {
                DecisionState::Accepted
            } else {
                DecisionState::Imported
            },
        })
        .collect();
    let movements = inputs
        .movements
        .iter()
        .map(|movement| LedgerMovementView {
            voucher_id: movement.voucher_id,
            amount_minor: movement.amount.0,
            currency: movement.currency.clone(),
            date: movement.date.clone(),
            state: if accepted_vouchers.contains(&movement.voucher_id) {
                DecisionState::Accepted
            } else {
                DecisionState::Imported
            },
        })
        .collect();
    let row_amounts = inputs
        .statement
        .rows
        .iter()
        .map(|r| (r.source_identity.as_str(), r.amount.0))
        .collect::<BTreeMap<_, _>>();
    let review_candidates = candidates
        .iter()
        .map(|candidate| {
            let decision = decisions
                .iter()
                .find(|((rows, vouchers), _)| {
                    *rows == &candidate.row_ids && *vouchers == &candidate.voucher_ids
                })
                .map(|(_, decision)| *decision);
            let state = decision.map_or(DecisionState::Proposed, |d| {
                disposition_state(d.disposition)
            });
            let total = candidate.row_ids.iter().try_fold(0_i64, |sum, id| {
                sum.checked_add(
                    *row_amounts
                        .get(id.as_str())
                        .ok_or_else(|| error("candidate names an unknown row"))?,
                )
                .ok_or_else(|| error("candidate amount overflow"))
            })?;
            Ok(ReviewCandidate {
                row_ids: candidate.row_ids.clone(),
                voucher_ids: candidate.voucher_ids.clone(),
                reason: CandidateReason {
                    kind: kind_name(candidate.kind).to_owned(),
                    exact_total_minor: total,
                    review_rank: candidate.review_rank,
                },
                decision_ref: decision.map(|d| d.decision_ref.clone()),
                state,
            })
        })
        .collect::<Result<Vec<_>, OfficeError>>()?;
    let row_refs = inputs
        .statement
        .rows
        .iter()
        .map(|r| format!("statement:{}", r.source_identity))
        .collect();
    let voucher_refs = inputs
        .movements
        .iter()
        .map(|m| format!("voucher:{}", m.voucher_id))
        .collect();
    Ok(ReconciliationReviewDocument {
        snapshot_ref: inputs.snapshot_ref.clone(),
        cutoff: inputs.statement.cutoff.clone(),
        reviewer_ref: inputs.reviewer_ref.clone(),
        rows,
        movements,
        candidates: review_candidates,
        decisions: certificate
            .decision_records
            .iter()
            .map(|decision| HumanDecisionView {
                decision_ref: decision.decision_ref.clone(),
                evidence: decision.evidence.clone(),
                state: disposition_state(decision.disposition),
            })
            .collect(),
        uncovered_rows: certificate.uncovered_rows.clone(),
        uncovered_vouchers: certificate.uncovered_vouchers.clone(),
        bridge: vec![
            BridgeTerm {
                name: "statement-total".into(),
                amount_minor: certificate.statement_total.0,
                evidence_refs: row_refs,
            },
            BridgeTerm {
                name: "less-ledger-total".into(),
                amount_minor: -certificate.ledger_total.0,
                evidence_refs: voucher_refs,
            },
            BridgeTerm {
                name: "residual".into(),
                amount_minor: certificate.residual.0,
                evidence_refs: vec![format!("snapshot:{}", inputs.snapshot_ref)],
            },
        ],
        correction_drafts: corrections
            .iter()
            .map(|draft| CorrectionDraftView {
                decision_ref: draft.decision_ref.clone(),
                posting_amounts_minor: draft.postings.iter().map(|p| p.amount.0).collect(),
                state: DecisionState::Corrected,
            })
            .collect(),
        state: if independently_verified && certificate.closed {
            DecisionState::Closed
        } else if independently_verified {
            DecisionState::Verified
        } else {
            DecisionState::Imported
        },
        edits: Vec::new(),
    })
}

/// Export a deterministic JSON review document.
pub fn export_review_document(
    document: &ReconciliationReviewDocument,
) -> Result<Vec<u8>, OfficeError> {
    serde_json::to_vec_pretty(document).map_err(error)
}

/// Import a review document and decode only explicit, checked command intents.
///
/// All projected fields are compared byte-structurally with a fresh trusted
/// projection. Editing a displayed status, amount, provenance, or closed flag
/// therefore fails instead of becoming authority.
pub fn import_review_document(
    bytes: &[u8],
    trusted_projection: &ReconciliationReviewDocument,
) -> Result<ImportedReviewDocument, OfficeError> {
    let mut imported: ReconciliationReviewDocument =
        serde_json::from_slice(bytes).map_err(error)?;
    let edits = std::mem::take(&mut imported.edits);
    if &imported != trusted_projection {
        return Err(error(
            "review projection was altered outside the explicit edit lane",
        ));
    }
    Ok(ImportedReviewDocument {
        commands: decode_review_edits(trusted_projection, edits)?,
    })
}

/// Decode explicit review edits to ledger command intents after identity checks.
pub fn decode_review_edits(
    projection: &ReconciliationReviewDocument,
    edits: Vec<ReviewEdit>,
) -> Result<Vec<ReconciliationCommand>, OfficeError> {
    let rows = projection
        .rows
        .iter()
        .map(|r| r.source_identity.as_str())
        .collect::<BTreeSet<_>>();
    let vouchers = projection
        .movements
        .iter()
        .map(|m| m.voucher_id)
        .collect::<BTreeSet<_>>();
    let mut refs = BTreeSet::new();
    edits
        .into_iter()
        .map(|edit| {
            if edit.decision_ref.is_empty() || !refs.insert(edit.decision_ref.clone()) {
                return Err(error("empty or duplicate decision reference"));
            }
            if edit.decided_by != projection.reviewer_ref {
                return Err(error("review edit has the wrong reviewer"));
            }
            if edit
                .evidence
                .row_ids
                .iter()
                .any(|id| !rows.contains(id.as_str()))
                || edit
                    .evidence
                    .voucher_ids
                    .iter()
                    .any(|id| !vouchers.contains(id))
            {
                return Err(error(
                    "review edit names evidence outside the frozen projection",
                ));
            }
            if edit.evidence.row_ids.is_empty() || edit.evidence.voucher_ids.is_empty() {
                return Err(error("review edit must name both sides"));
            }
            Ok(ReconciliationCommand::RecordDecision(DecisionRecord {
                decision_ref: edit.decision_ref,
                decided_by: edit.decided_by,
                disposition: edit.disposition,
                evidence: edit.evidence,
            }))
        })
        .collect()
}

fn kind_name(kind: CandidateKind) -> &'static str {
    match kind {
        CandidateKind::OneToOne => "one-to-one",
        CandidateKind::OneToMany => "one-to-many",
        CandidateKind::ManyToOne => "many-to-one",
    }
}

fn disposition_state(disposition: DecisionDisposition) -> DecisionState {
    match disposition {
        DecisionDisposition::Accept | DecisionDisposition::Split | DecisionDisposition::Merge => {
            DecisionState::Accepted
        }
        DecisionDisposition::Reject => DecisionState::Rejected,
        DecisionDisposition::Defer => DecisionState::Deferred,
    }
}
fn error(message: impl std::fmt::Display) -> OfficeError {
    OfficeError::DomainEdit(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_ledger::{
        Amount, CandidateBounds, CanonicalStatementRow, LedgerMovement, Posting,
        ReconciliationInputs, StatementSnapshot, build_certificate, generate_candidates,
        verify_certificate,
    };

    #[test]
    fn unresolved_month_exposes_every_exact_amount_and_state() {
        let inputs = inputs();
        let candidates = generate_candidates(&inputs, bounds()).unwrap();
        let decisions = vec![
            decision("reject-fee", DecisionDisposition::Reject, "bank:fee", 41),
            DecisionRecord {
                decision_ref: "defer-month".into(),
                decided_by: "mia".into(),
                disposition: DecisionDisposition::Defer,
                evidence: ReconciliationRef {
                    row_ids: vec!["bank:fee".into(), "bank:rent".into()],
                    voucher_ids: vec![41, 42],
                },
            },
        ];
        let certificate = build_certificate(&inputs, &decisions, false).unwrap();
        let correction = CorrectionDraft {
            decision_ref: "accepted-correction".into(),
            postings: vec![posting(25), posting(-25)],
        };
        let review =
            project_reconciliation_review(&inputs, &candidates, &certificate, &[correction], false)
                .unwrap();
        assert_eq!(review.state, DecisionState::Imported);
        assert_eq!(review.uncovered_rows, vec!["bank:fee", "bank:rent"]);
        assert!(
            review
                .candidates
                .iter()
                .any(|c| c.state == DecisionState::Rejected)
        );
        assert!(
            review
                .candidates
                .iter()
                .any(|c| c.state == DecisionState::Proposed)
        );
        assert_eq!(
            review.decisions.iter().map(|d| d.state).collect::<Vec<_>>(),
            vec![DecisionState::Rejected, DecisionState::Deferred]
        );
        assert_eq!(review.correction_drafts[0].state, DecisionState::Corrected);
        assert_eq!(
            review
                .bridge
                .iter()
                .map(|term| term.amount_minor)
                .collect::<Vec<_>>(),
            vec![-10_025, 10_025, 0]
        );
        assert!(
            review
                .bridge
                .iter()
                .all(|term| !term.evidence_refs.is_empty())
        );
        assert_eq!(
            review
                .movements
                .iter()
                .map(|m| m.voucher_id)
                .collect::<Vec<_>>(),
            vec![41, 42]
        );
    }

    #[test]
    fn independently_verified_close_is_structurally_closed() {
        let inputs = inputs();
        let candidates = generate_candidates(&inputs, bounds()).unwrap();
        let decisions = vec![
            decision("fee", DecisionDisposition::Accept, "bank:fee", 41),
            decision("rent", DecisionDisposition::Accept, "bank:rent", 42),
        ];
        let certificate = build_certificate(&inputs, &decisions, true).unwrap();
        verify_certificate(&inputs, &decisions, &certificate).unwrap();
        let review =
            project_reconciliation_review(&inputs, &candidates, &certificate, &[], true).unwrap();
        assert_eq!(review.state, DecisionState::Closed);
        assert!(review.uncovered_rows.is_empty() && review.uncovered_vouchers.is_empty());
        assert!(
            review
                .rows
                .iter()
                .all(|row| row.state == DecisionState::Accepted)
        );
    }

    #[test]
    fn reimport_emits_commands_but_cannot_manufacture_approval_or_closure() {
        let inputs = inputs();
        let candidates = generate_candidates(&inputs, bounds()).unwrap();
        let certificate = build_certificate(&inputs, &[], false).unwrap();
        let trusted =
            project_reconciliation_review(&inputs, &candidates, &certificate, &[], false).unwrap();
        let mut edited = trusted.clone();
        edited.edits.push(ReviewEdit {
            decision_ref: "mia-1".into(),
            decided_by: "mia".into(),
            disposition: DecisionDisposition::Accept,
            evidence: ReconciliationRef {
                row_ids: vec!["bank:fee".into()],
                voucher_ids: vec![41],
            },
        });
        let imported =
            import_review_document(&export_review_document(&edited).unwrap(), &trusted).unwrap();
        assert!(
            matches!(&imported.commands[0], ReconciliationCommand::RecordDecision(record) if record.decision_ref == "mia-1")
        );
        let mut forged = trusted.clone();
        forged.state = DecisionState::Closed;
        assert!(
            import_review_document(&export_review_document(&forged).unwrap(), &trusted)
                .unwrap_err()
                .to_string()
                .contains("outside the explicit edit lane")
        );
        let mut forged_amount = trusted.clone();
        forged_amount.rows[0].amount_minor += 1;
        assert!(
            import_review_document(&export_review_document(&forged_amount).unwrap(), &trusted)
                .is_err()
        );
    }

    fn inputs() -> ReconciliationInputs {
        ReconciliationInputs {
            snapshot_ref: "month:2026-07".into(),
            reviewer_ref: "mia".into(),
            statement: StatementSnapshot {
                profile_id: "bank-se".into(),
                source_ref: "bank-export:sha256:statement".into(),
                cutoff: "2026-07-31T23:59:59Z".into(),
                total: Amount(-10_025),
                ledger_balances: Vec::new(),
                rows: vec![
                    row("bank:fee", -25, "2026-07-02"),
                    row("bank:rent", -10_000, "2026-07-25"),
                ],
            },
            movements: vec![
                movement(41, -25, "2026-07-02"),
                movement(42, -10_000, "2026-07-25"),
            ],
        }
    }
    fn row(id: &str, amount: i64, date: &str) -> CanonicalStatementRow {
        CanonicalStatementRow {
            source_identity: id.into(),
            amount: Amount(amount),
            currency: "SEK".into(),
            date: date.into(),
            description: Some(id.into()),
            version: 1,
            source_ref: "bank-export:sha256:statement".into(),
            ordinal: if id == "bank:fee" { 1 } else { 2 },
            profile_id: "bank-se".into(),
            original_bytes: format!("{id},{date},{amount}").into_bytes(),
        }
    }
    fn movement(id: i64, amount: i64, date: &str) -> LedgerMovement {
        LedgerMovement {
            voucher_id: id,
            amount: Amount(amount),
            currency: "SEK".into(),
            date: date.into(),
            text: Some(if id == 41 { "bank:fee" } else { "bank:rent" }.into()),
        }
    }
    fn decision(
        id: &str,
        disposition: DecisionDisposition,
        row: &str,
        voucher: i64,
    ) -> DecisionRecord {
        DecisionRecord {
            decision_ref: id.into(),
            decided_by: "mia".into(),
            disposition,
            evidence: ReconciliationRef {
                row_ids: vec![row.into()],
                voucher_ids: vec![voucher],
            },
        }
    }
    fn posting(amount: i64) -> Posting {
        Posting {
            id: 0,
            source_id: None,
            voucher_id: 0,
            account: 2999,
            amount: Amount(amount),
            text: Some("review correction".into()),
        }
    }
    fn bounds() -> CandidateBounds {
        CandidateBounds {
            max_work: 100,
            max_results: 10,
            max_cardinality: 2,
            max_date_distance_days: 3,
        }
    }
}
