# sim-lib-sheet

Exact spreadsheet domain model for SIM office documents.

The crate represents sheets as sparse cell maps with exact rational numbers,
text, booleans, blanks, and formulas. It projects sheets into the shared office
`Doc` contract as data-position expressions, decodes them back to sheet records,
provides open cell-edit payloads, and maps Microsoft Graph workbook ranges
through host-provided site adapters without adding storage enums to the kernel or
office core.
