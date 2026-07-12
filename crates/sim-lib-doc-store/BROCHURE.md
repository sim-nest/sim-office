# sim-lib-doc-store

In one line: a local office document cache that keeps edits tied to the ledger that produced them.

## What it gives you

This crate gives office work a durable local place to remember document snapshots and the edit projections that came from committed ledger entries. It is useful for offline viewing, undo previews, and tests that need repeatable document state without calling a hosted service.

## Why you will be glad

- Local document state survives process restarts.
- Undo information stays beside the ledger entry that created it.
- The cache does not become a second source of truth for document history.

## Where it fits

This crate sits beside the document core and site registration crates. The core defines documents and reversible edits, sites reach live or modeled places, and this store keeps local read projections that hosts can rebuild from ledger commits.
