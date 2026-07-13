# sim-lib-doc-ledger

In one line: a careful handoff from office evidence to accounting drafts.

## What it gives you

This crate lets an office workflow review a bookkeeping entry before it touches
the books. A message, SharePoint file, task, or issue can support the entry as a
reference, while the actual draft is checked by the ledger layer and shown as a
preview.

## Why you will be glad

- Accounting drafts are checked before they become records.
- Office evidence remains linked without copying private payloads.
- The document core stays clean; only this bridge knows about ledger drafts.

## Where it fits

This crate is the ring-3 bridge between office documents and ledger books. It
sits above the shared document core and points to the ledger draft checker when
an office surface needs a posting preview.
