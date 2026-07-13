# sim-lib-doc-ledger

In one line: a careful handoff from office evidence to accounting reviews.

## What it gives you

This crate lets an office workflow review a bookkeeping entry or year-end
statement before it touches the books or leaves the local review surface. A
message, SharePoint file, task, or issue can support the entry as a reference,
while ledger checks supply exact preview data.

## Why you will be glad

- Accounting drafts are checked before they become records.
- Office evidence remains linked without copying private payloads.
- Closed-year statements turn into sheets and decks with exact totals.
- The document core stays clean; only this bridge knows about ledger drafts.

## Where it fits

This crate is the ring-3 bridge between office documents and ledger books. It
sits above the shared document core and points to ledger review data when an
office surface needs a posting or statement preview.
