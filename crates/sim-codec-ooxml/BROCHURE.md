# sim-codec-ooxml

In one line: Office file packages can move through SIM without hiding what the portable model cannot keep.

## What it gives you

`sim-codec-ooxml` gives the office family local file boundaries for spreadsheet
and presentation documents. It creates ordinary workbook and slide packages,
reads them back into SIM documents, and calls out styling, merged cells,
transitions, or media that do not fit the portable local models.

## Why you will be glad

- Local spreadsheets and slide decks can travel through familiar Office files.
- Exact spreadsheet values stay exact across the file boundary.
- Loss reports make unsupported workbook and presentation features visible.

## Where it fits

This crate sits between the local office domains and vendor file packages. It
keeps sheets and decks useful before live Excel, PowerPoint, SharePoint, or
LibreOffice sites enter the workflow.
