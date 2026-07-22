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
