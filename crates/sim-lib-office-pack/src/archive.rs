//! Preview edit planning for annual accounts packs.

use sim_kernel::{Cx, Symbol, Value};
use sim_lib_doc_core::{DocId, Edit, Evidence, ExternalRef};
use sim_lib_mail::{DraftMessage, MailboxTarget};

use crate::{
    AnnualAccountsPack, EncodedStatementFiles, GeneratedFile, PackError, pack::default_context,
};

/// Edit domain for annual accounts pack preview operations.
pub const PACK_EDIT_DOMAIN: &str = "office/annual-accounts-pack";

const PACK_SYMBOL_NAMESPACE: &str = "office/pack";

/// Plans preview edits for all selected annual accounts targets.
pub fn plan_archive(
    pack: &AnnualAccountsPack,
    targets: &crate::ExportTargets,
) -> Result<Vec<Edit>, PackError> {
    let mut cx = default_context();
    plan_archive_with_cx(&mut cx, pack, targets)
}

/// Plans preview edits using the caller's kernel context.
pub fn plan_archive_with_cx(
    cx: &mut Cx,
    pack: &AnnualAccountsPack,
    targets: &crate::ExportTargets,
) -> Result<Vec<Edit>, PackError> {
    if targets.is_empty() {
        return Err(PackError::EmptyTargets);
    }

    let files = crate::encode_statement_files(cx, pack)?;
    let mut edits = Vec::new();
    if let Some(target) = &targets.spreadsheet {
        edits.push(file_export_edit(
            cx,
            pack.year,
            "export-spreadsheet",
            &files.spreadsheet,
            target,
        )?);
    }
    if let Some(target) = &targets.deck {
        edits.push(file_export_edit(
            cx,
            pack.year,
            "export-deck",
            &files.deck,
            target,
        )?);
    }
    if let Some(target) = &targets.outlook_draft {
        edits.push(outlook_draft_edit(cx, pack, &files, target)?);
    }
    if let Some(target) = &targets.sharepoint_archive {
        edits.push(sharepoint_archive_edit(cx, pack, &files, target)?);
    }
    Ok(edits)
}

fn file_export_edit(
    cx: &mut Cx,
    year: i32,
    action: &str,
    file: &GeneratedFile,
    target: &ExternalRef,
) -> Result<Edit, PackError> {
    let entries = vec![
        action_entry(cx, action)?,
        string_entry(cx, "year", year.to_string())?,
        file_entry(cx, file, true)?,
        target_entry(cx, target)?,
    ];
    let op = cx.factory().table(entries)?;
    let inverse = discard_payload(cx, action)?;
    Ok(Edit::new(
        DocId::new(format!("annual-accounts/{year}/{}", file.filename)),
        PACK_EDIT_DOMAIN,
        op,
        inverse,
    ))
}

fn outlook_draft_edit(
    cx: &mut Cx,
    pack: &AnnualAccountsPack,
    files: &EncodedStatementFiles,
    target: &ExternalRef,
) -> Result<Edit, PackError> {
    let draft = DraftMessage::new(
        MailboxTarget::new("me", None),
        format!("Annual accounts {}", pack.year),
        format!(
            "Annual accounts {} are ready for review. Attachments: {}, {}.",
            pack.year, files.spreadsheet.filename, files.deck.filename
        ),
        Vec::new(),
    );
    let entries = vec![
        action_entry(cx, "preview-outlook-draft")?,
        string_entry(cx, "mailbox", draft.target.user_id_or_me.clone())?,
        string_entry(cx, "subject", draft.subject.clone())?,
        string_entry(cx, "body", draft.body.clone())?,
        files_entry(cx, files, false)?,
        evidence_entry(cx, &pack.evidence)?,
        target_entry(cx, target)?,
    ];
    let op = cx.factory().table(entries)?;
    let inverse = discard_payload(cx, "preview-outlook-draft")?;
    Ok(Edit::new(
        DocId::new(format!("annual-accounts/{}/outlook-draft", pack.year)),
        PACK_EDIT_DOMAIN,
        op,
        inverse,
    ))
}

fn sharepoint_archive_edit(
    cx: &mut Cx,
    pack: &AnnualAccountsPack,
    files: &EncodedStatementFiles,
    target: &ExternalRef,
) -> Result<Edit, PackError> {
    let entries = vec![
        action_entry(cx, "preview-sharepoint-archive")?,
        string_entry(cx, "year", pack.year.to_string())?,
        files_entry(cx, files, false)?,
        evidence_entry(cx, &pack.evidence)?,
        target_entry(cx, target)?,
    ];
    let op = cx.factory().table(entries)?;
    let inverse = discard_payload(cx, "preview-sharepoint-archive")?;
    Ok(Edit::new(
        DocId::new(format!("annual-accounts/{}/sharepoint-archive", pack.year)),
        PACK_EDIT_DOMAIN,
        op,
        inverse,
    ))
}

fn discard_payload(cx: &mut Cx, action: &str) -> Result<Value, PackError> {
    let entries = vec![action_entry(cx, &format!("discard-{action}"))?];
    Ok(cx.factory().table(entries)?)
}

fn action_entry(cx: &mut Cx, action: &str) -> Result<(Symbol, Value), PackError> {
    Ok((
        Symbol::new("action"),
        cx.factory()
            .symbol(Symbol::qualified(PACK_SYMBOL_NAMESPACE, action))?,
    ))
}

fn string_entry(
    cx: &mut Cx,
    field: &'static str,
    value: impl Into<String>,
) -> Result<(Symbol, Value), PackError> {
    Ok((Symbol::new(field), cx.factory().string(value.into())?))
}

fn file_entry(
    cx: &mut Cx,
    file: &GeneratedFile,
    include_bytes: bool,
) -> Result<(Symbol, Value), PackError> {
    let mut entries = vec![
        string_entry(cx, "kind", file.kind.clone())?,
        string_entry(cx, "filename", file.filename.clone())?,
        string_entry(cx, "codec", file.codec_id.clone())?,
        string_entry(cx, "extension", file.extension.clone())?,
        string_entry(cx, "byte-count", file.bytes.len().to_string())?,
    ];
    if include_bytes {
        entries.push((
            Symbol::new("bytes"),
            cx.factory().bytes(file.bytes.clone())?,
        ));
    }
    Ok((Symbol::new("file"), cx.factory().table(entries)?))
}

fn files_entry(
    cx: &mut Cx,
    files: &EncodedStatementFiles,
    include_bytes: bool,
) -> Result<(Symbol, Value), PackError> {
    let values = vec![
        file_payload(cx, &files.spreadsheet, include_bytes)?,
        file_payload(cx, &files.deck, include_bytes)?,
    ];
    Ok((Symbol::new("files"), cx.factory().list(values)?))
}

fn file_payload(
    cx: &mut Cx,
    file: &GeneratedFile,
    include_bytes: bool,
) -> Result<Value, PackError> {
    Ok(file_entry(cx, file, include_bytes)?.1)
}

fn target_entry(cx: &mut Cx, target: &ExternalRef) -> Result<(Symbol, Value), PackError> {
    Ok((Symbol::new("target"), external_ref_payload(cx, target)?))
}

fn external_ref_payload(cx: &mut Cx, reference: &ExternalRef) -> Result<Value, PackError> {
    let entries = vec![
        string_entry(cx, "backend", reference.backend.clone())?,
        string_entry(cx, "external-id", reference.external_id.clone())?,
        string_entry(cx, "version", reference.version.clone().unwrap_or_default())?,
        string_entry(cx, "web-url", reference.web_url.clone().unwrap_or_default())?,
    ];
    Ok(cx.factory().table(entries)?)
}

fn evidence_entry(cx: &mut Cx, evidence: &[Evidence]) -> Result<(Symbol, Value), PackError> {
    let values = evidence
        .iter()
        .map(|item| evidence_payload(cx, item))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((Symbol::new("evidence"), cx.factory().list(values)?))
}

fn evidence_payload(cx: &mut Cx, evidence: &Evidence) -> Result<Value, PackError> {
    let entries = vec![
        string_entry(cx, "subject", evidence.subject.as_str().to_owned())?,
        (
            Symbol::new("evidence"),
            external_ref_payload(cx, &evidence.evidence)?,
        ),
        string_entry(cx, "role", evidence.predicate())?,
        string_entry(cx, "captured-at-seq", evidence.captured_at_seq.to_string())?,
        string_entry(
            cx,
            "immutable-hint",
            evidence.immutable_hint.clone().unwrap_or_default(),
        )?,
    ];
    Ok(cx.factory().table(entries)?)
}
