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
