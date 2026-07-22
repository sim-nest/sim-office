# sim-site-libreoffice

In one line: LibreOffice automation stays optional, permissioned, and outside the runtime process.

## What it gives you

`sim-site-libreoffice` gives the office family a helper-process boundary for
LibreOffice tasks such as opening a document and exporting a PDF. It keeps UNO
automation behind a small command protocol while the ordinary ODF file codec
remains the local default.

## Why you will be glad

- Live office automation is explicit and easy to deny.
- Tests can use a small fake helper instead of a desktop office install.
- Helper errors redact private local paths before they cross the boundary.

## Where it fits

This crate sits beside the document site spine and the ODF file codec. The codec
handles local files, while this site handles deliberate helper-process
placements for hosts that choose to run LibreOffice.
