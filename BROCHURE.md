# sim-office

In one line: office documents as typed, inspectable SIM data without baking office suites into the kernel.

## What it gives you

`sim-office` gives SIM a common language for documents that live in spreadsheets, decks, mail, reports, local stores, and hosted office services. The first layer is deliberately small: document kind names stay open, ids stay stable, external references keep their source identity, and shape checks can ask what kind of document a value represents.

## Why you will be glad

- Office-shaped data stays portable across local files and hosted services.
- The kernel carries document values without learning office-specific enums.
- Prose document names such as article, report, and readme are reserved in the same vocabulary as sheets and decks.

## Where it fits

This repository is the office family around SIM's runtime contracts. It supplies document records and shape hooks at the core, then lets codecs, placement sites, projections, and host bridges use that shared vocabulary. The result is one office surface that can span local files, generated reports, web views, and service-backed documents.
