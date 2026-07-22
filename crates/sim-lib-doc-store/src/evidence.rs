//! SQLite evidence-link storage for document audit trails.

use rusqlite::{params, types::Type};
use sim_lib_doc_core::{DocId, Evidence, ExternalRef, LinkRole};

use crate::{DocStore, codec::CodecError};

/// Attaches an evidence link as a subject/predicate/object fact row.
pub fn attach(store: &DocStore, evidence: &Evidence) -> rusqlite::Result<()> {
    let object = encode_ref(&evidence.evidence).map_err(sql_encode_error)?;
    let seq = sqlite_seq(evidence.captured_at_seq)?;
    store.connection().execute(
        "INSERT INTO evidence_facts
             (subject, predicate, object, captured_at_seq, immutable_hint)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(subject, predicate, object, captured_at_seq)
         DO UPDATE SET immutable_hint = excluded.immutable_hint",
        params![
            evidence.subject.as_str(),
            evidence.predicate(),
            object,
            seq,
            evidence.immutable_hint.as_deref(),
        ],
    )?;
    Ok(())
}

/// Returns the evidence links for `subject`, ordered by capture sequence.
pub fn evidence_for(store: &DocStore, subject: &DocId) -> rusqlite::Result<Vec<Evidence>> {
    let mut stmt = store.connection().prepare(
        "SELECT predicate, object, captured_at_seq, immutable_hint
         FROM evidence_facts
         WHERE subject = ?1
         ORDER BY captured_at_seq ASC, predicate ASC, object ASC",
    )?;
    let rows = stmt.query_map(params![subject.as_str()], |row| {
        let predicate: String = row.get(0)?;
        let object: String = row.get(1)?;
        let captured_at_seq: i64 = row.get(2)?;
        let immutable_hint: Option<String> = row.get(3)?;
        let role = LinkRole::from_predicate(&predicate).ok_or_else(|| {
            sql_decode_error(
                0,
                CodecError::new(format!("unknown evidence predicate {predicate}")),
            )
        })?;
        let evidence = decode_ref(&object).map_err(|error| sql_decode_error(1, error))?;
        Ok(Evidence::new(
            subject.clone(),
            evidence,
            role,
            u64::try_from(captured_at_seq).map_err(|_| {
                sql_decode_error(
                    2,
                    CodecError::new(format!("negative evidence sequence {captured_at_seq}")),
                )
            })?,
            immutable_hint,
        ))
    })?;

    rows.collect()
}

fn encode_ref(reference: &ExternalRef) -> Result<String, CodecError> {
    serde_json::to_string(reference).map_err(|error| CodecError::new(error.to_string()))
}

fn decode_ref(encoded: &str) -> Result<ExternalRef, CodecError> {
    serde_json::from_str(encoded).map_err(|error| CodecError::new(error.to_string()))
}

fn sqlite_seq(seq: u64) -> rusqlite::Result<i64> {
    i64::try_from(seq).map_err(|_| {
        sql_encode_error(CodecError::new(format!(
            "evidence sequence {seq} does not fit sqlite INTEGER"
        )))
    })
}

fn sql_encode_error(error: CodecError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn sql_decode_error(index: usize, error: CodecError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}
