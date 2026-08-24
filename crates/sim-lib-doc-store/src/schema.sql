PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS docs (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS edit_projection (
  seq INTEGER PRIMARY KEY,
  doc TEXT NOT NULL,
  edit TEXT NOT NULL,
  inverse TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS edit_projection_doc_seq
ON edit_projection(doc, seq DESC);

CREATE TABLE IF NOT EXISTS evidence_facts (
  subject TEXT NOT NULL,
  predicate TEXT NOT NULL,
  object TEXT NOT NULL,
  captured_at_seq INTEGER NOT NULL,
  immutable_hint TEXT,
  PRIMARY KEY(subject, predicate, object, captured_at_seq)
);

CREATE INDEX IF NOT EXISTS evidence_facts_subject_seq
ON evidence_facts(subject, captured_at_seq ASC, predicate ASC, object ASC);

CREATE TABLE IF NOT EXISTS web_captures (
  capture_id TEXT PRIMARY KEY NOT NULL,
  source_uri TEXT NOT NULL,
  body BLOB NOT NULL,
  exchange_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_representations (
  representation_id TEXT PRIMARY KEY NOT NULL,
  capture_id TEXT NOT NULL REFERENCES web_captures(capture_id) ON DELETE RESTRICT,
  text TEXT NOT NULL,
  metadata_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_evidence_anchors (
  anchor_id TEXT PRIMARY KEY NOT NULL,
  subject TEXT NOT NULL,
  representation_id TEXT NOT NULL REFERENCES web_representations(representation_id) ON DELETE RESTRICT,
  record_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS web_evidence_anchor_subject
ON web_evidence_anchors(subject, anchor_id);
