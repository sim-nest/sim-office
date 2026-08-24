//! Typed persistence rows for web captures and evidence anchors.

use rusqlite::{OptionalExtension, params};

use crate::DocStore;

/// Serialized immutable capture row. Content identity is verified by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebCaptureRow {
    /// Stable raw capture id.
    pub capture_id: String,
    /// Normalized retrieval URI.
    pub source_uri: String,
    /// Exact response bytes.
    pub body: Vec<u8>,
    /// Versioned exchange metadata.
    pub exchange_json: String,
}

/// Serialized normalized representation row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRepresentationRow {
    /// Stable representation id.
    pub representation_id: String,
    /// Referenced raw capture id.
    pub capture_id: String,
    /// Immutable normalized Unicode text.
    pub text: String,
    /// Versioned codec and fidelity metadata.
    pub metadata_json: String,
}

/// Serialized evidence anchor row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAnchorRow {
    /// Stable anchor id.
    pub anchor_id: String,
    /// Office evidence subject id.
    pub subject: String,
    /// Addressed representation id.
    pub representation_id: String,
    /// Complete versioned anchor record.
    pub record_json: String,
}

impl DocStore {
    /// Atomically saves a capture after verifying that an existing id is byte-identical.
    pub fn save_web_capture(&mut self, row: &WebCaptureRow) -> rusqlite::Result<()> {
        let tx = self.connection_mut().transaction()?;
        let prior: Option<Vec<u8>> = tx
            .query_row(
                "SELECT body FROM web_captures WHERE capture_id=?1",
                params![row.capture_id],
                |r| r.get(0),
            )
            .optional()?;
        if prior.as_ref().is_some_and(|body| body != &row.body) {
            return Err(invalid("capture id already names different bytes"));
        }
        tx.execute("INSERT OR IGNORE INTO web_captures(capture_id,source_uri,body,exchange_json) VALUES(?1,?2,?3,?4)",
            params![row.capture_id, row.source_uri, row.body, row.exchange_json])?;
        tx.commit()
    }

    /// Saves a representation only when its raw capture exists.
    pub fn save_web_representation(&mut self, row: &WebRepresentationRow) -> rusqlite::Result<()> {
        self.connection_mut().execute("INSERT INTO web_representations(representation_id,capture_id,text,metadata_json) VALUES(?1,?2,?3,?4)
            ON CONFLICT(representation_id) DO UPDATE SET capture_id=excluded.capture_id,text=excluded.text,metadata_json=excluded.metadata_json",
            params![row.representation_id,row.capture_id,row.text,row.metadata_json])?;
        Ok(())
    }

    /// Loads the immutable representation fields used to revalidate an anchor.
    pub fn load_web_representation(
        &self,
        id: &str,
    ) -> rusqlite::Result<Option<WebRepresentationRow>> {
        self.connection().query_row("SELECT capture_id,text,metadata_json FROM web_representations WHERE representation_id=?1", params![id], |r| Ok(WebRepresentationRow {
            representation_id: id.to_owned(), capture_id: r.get(0)?, text: r.get(1)?, metadata_json: r.get(2)?
        })).optional()
    }

    /// Loads the immutable capture fields used to verify representation provenance.
    pub fn load_web_capture(&self, id: &str) -> rusqlite::Result<Option<WebCaptureRow>> {
        self.connection()
            .query_row(
                "SELECT source_uri,body,exchange_json FROM web_captures WHERE capture_id=?1",
                params![id],
                |r| {
                    Ok(WebCaptureRow {
                        capture_id: id.to_owned(),
                        source_uri: r.get(0)?,
                        body: r.get(1)?,
                        exchange_json: r.get(2)?,
                    })
                },
            )
            .optional()
    }

    /// Makes a previously validated anchor visible.
    pub fn save_web_anchor(&mut self, row: &WebAnchorRow) -> rusqlite::Result<()> {
        self.connection_mut().execute("INSERT INTO web_evidence_anchors(anchor_id,subject,representation_id,record_json) VALUES(?1,?2,?3,?4)
            ON CONFLICT(anchor_id) DO UPDATE SET subject=excluded.subject,representation_id=excluded.representation_id,record_json=excluded.record_json",
            params![row.anchor_id,row.subject,row.representation_id,row.record_json])?;
        Ok(())
    }

    /// Loads an anchor row; callers must fail closed if revalidation fails.
    pub fn load_web_anchor(&self, id: &str) -> rusqlite::Result<Option<WebAnchorRow>> {
        self.connection().query_row("SELECT subject,representation_id,record_json FROM web_evidence_anchors WHERE anchor_id=?1", params![id], |r| Ok(WebAnchorRow {
            anchor_id: id.to_owned(), subject: r.get(0)?, representation_id: r.get(1)?, record_json: r.get(2)?
        })).optional()
    }
}

fn invalid(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.to_owned())
}
