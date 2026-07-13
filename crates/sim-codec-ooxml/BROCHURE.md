# sim-codec-ooxml

In one line: Excel-compatible local files can move through SIM without turning numbers into floats.

## What it gives you

`sim-codec-ooxml` gives the office family a small `.xlsx` boundary for the exact
sheet model. It creates ordinary workbook packages, reads them back into SIM
sheet documents, and calls out styling or merge information that does not fit
the portable local model.

## Why you will be glad

- Local spreadsheets can travel through Excel-compatible files.
- Exact rational values stay exact across the file boundary.
- Loss reports make unsupported workbook features visible instead of silent.

## Where it fits

This crate sits between the exact sheet domain and vendor spreadsheet files. It
keeps the local model useful before live Excel, SharePoint, or LibreOffice sites
enter the workflow.
