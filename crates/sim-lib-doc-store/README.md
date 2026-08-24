# sim-lib-doc-store

Thin relational projections for SIM office documents.

The crate stores document snapshots and projected edit rows in a local
database. Edit rows use the ledger sequence supplied by the caller; the provider does
not invent a second sequence. Undo returns the inverse edit for the latest
projected ledger entry without mutating the saved document snapshot.
