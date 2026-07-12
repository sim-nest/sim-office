# sim-lib-doc-surface

In one line: a suite-facing document surface that turns office records into renderable panes and checked edits.

## What it gives you

This crate gives office documents a shared scene for screens, decks, tables, and embedded document panes. It makes document previews visible through the existing view stack and turns user intent into clear edit records that a host can inspect before it commits anything.

## Why you will be glad

- Office panes render as ordinary Scene values that existing hosts already know how to carry.
- Intent decoding produces explicit edit records instead of hidden UI side effects.
- The same surface works for tests, headless fixtures, and live adapters.

## Where it fits

This crate sits above the document core and below hosted suite integrations. The core owns document records and projection metadata; this surface shapes those projections into view data and translates pane actions into reversible office edits.
