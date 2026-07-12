# sim-lib-doc-core

In one line: the small document spine that office codecs, stores, views, and sites share.

## What it gives you

`sim-lib-doc-core` gives every office layer the same basic record: what kind of document this is, which stable id names it, what runtime value carries its body, and which outside file or service record it came from. It also gives callers a shape value for a document kind, so selection and validation can stay open instead of depending on a closed list inside the kernel.

## Why you will be glad

- One document record works across files, services, and views.
- Document kinds are strings, so domains can join without changing the core.
- Embedded descriptor recipes make the crate visible to catalog and cookbook tools.

## Where it fits

This crate is ring 0 for the office family. It does not parse spreadsheets, render reports, talk to Microsoft Graph, or bridge ledgers. It names the data and shape contract those layers use, keeping the core narrow while still giving surrounding layers a real object to match against.
