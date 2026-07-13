# sim-lib-doc-ledger

Ring-3 office bridge for ledger draft previews.

The crate resolves host-owned office draft ids into checked ledger drafts. It
builds preview edits that office surfaces can show before an operator commits a
voucher into a ledger year. Ledger types stay here and in `sim-ledger`; the
document core remains free of ledger dependencies.
