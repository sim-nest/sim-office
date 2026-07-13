use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
use sim_lib_doc_core::{DocId, Evidence, ExternalRef, LinkRole};
use sim_lib_ledger_close::{FinancialStatements, StatementNote, StatementRow, StatementTable};

use crate::{
    AnnualAccountsPack, ExportTargets, PACK_EDIT_DOMAIN, encode_statement_files,
    plan_archive_with_cx,
};

#[test]
fn synthetic_statements_plan_preview_only_exports() {
    let mut cx = cx();
    let pack = pack();
    let targets = targets();

    let files = encode_statement_files(&mut cx, &pack).unwrap();
    assert_eq!(files.spreadsheet.filename, "annual-accounts-2026.xlsx");
    assert_eq!(files.deck.filename, "annual-accounts-2026.pptx");
    assert!(files.spreadsheet.bytes.starts_with(b"PK"));
    assert!(files.deck.bytes.starts_with(b"PK"));

    let edits = plan_archive_with_cx(&mut cx, &pack, &targets).unwrap();

    assert_eq!(edits.len(), 4);
    assert!(edits.iter().all(|edit| edit.domain == PACK_EDIT_DOMAIN));
    assert!(
        edits
            .iter()
            .any(|edit| edit.doc.as_str().ends_with("annual-accounts-2026.xlsx"))
    );
    assert!(
        edits
            .iter()
            .any(|edit| edit.doc.as_str().ends_with("annual-accounts-2026.pptx"))
    );
    assert!(
        edits
            .iter()
            .any(|edit| edit.doc.as_str().ends_with("outlook-draft"))
    );
    assert!(
        edits
            .iter()
            .any(|edit| edit.doc.as_str().ends_with("sharepoint-archive"))
    );
}

#[test]
fn empty_targets_are_rejected() {
    let err = crate::plan_archive(&pack(), &ExportTargets::new()).unwrap_err();

    assert_eq!(
        err.to_string(),
        "annual accounts pack has no selected targets"
    );
}

fn cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn pack() -> AnnualAccountsPack {
    AnnualAccountsPack::new(
        2026,
        statements(),
        vec![Evidence::new(
            DocId::new("annual-accounts-2026"),
            ExternalRef::new("ledger", "voucher/2026/0007", None, None),
            LinkRole::AccountingSupport,
            7,
            Some("voucher-digest".to_owned()),
        )],
    )
}

fn statements() -> FinancialStatements {
    FinancialStatements {
        year: 2026,
        trial_balance: Vec::new(),
        income_statement: StatementTable {
            title: "Income statement".to_owned(),
            rows: vec![StatementRow {
                label: "SRU 3000".to_owned(),
                amount_minor: -1_200,
            }],
        },
        balance_sheet: StatementTable {
            title: "Balance sheet".to_owned(),
            rows: vec![StatementRow {
                label: "SRU 1000".to_owned(),
                amount_minor: 1_200,
            }],
        },
        notes: vec![StatementNote {
            id: "basis".to_owned(),
            text: "Exact fixture".to_owned(),
        }],
    }
}

fn targets() -> ExportTargets {
    ExportTargets::new()
        .with_spreadsheet(ExternalRef::new(
            "codec/ooxml-xlsx",
            "annual-accounts-2026.xlsx",
            None,
            None,
        ))
        .with_deck(ExternalRef::new(
            "codec/ooxml-pptx",
            "annual-accounts-2026.pptx",
            None,
            None,
        ))
        .with_outlook_draft(ExternalRef::new("site/msgraph", "me/messages", None, None))
        .with_sharepoint_archive(ExternalRef::new(
            "site/sharepoint",
            "sites/contoso/drive/accounting",
            None,
            None,
        ))
}
