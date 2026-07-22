# sim-codec-odf

In one line: LibreOffice files can carry local office documents without hiding what falls outside the portable model.

## What it gives you

`sim-codec-odf` gives the office family local file boundaries for spreadsheets
and slide decks. It creates ordinary ODF packages, reads them back into SIM
documents, preserves exact sheet values, and keeps slide structure visible as
portable document content.

## Why you will be glad

- Spreadsheet and presentation files work without a running office suite.
- Exact values and slide blocks stay attached to the portable document.
- Styling that cannot travel into the local model is called out clearly.

## Where it fits

This crate sits beside the Office file codec as the LibreOffice-oriented file
boundary for the office suite. It keeps local sheets and decks useful before
helper-process or service placements enter the workflow.
