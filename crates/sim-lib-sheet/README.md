# sim-lib-sheet

Exact spreadsheet domain model for SIM office documents.

The crate represents sheets as sparse cell maps with exact rational numbers,
text, booleans, blanks, and formulas. It projects sheets into the shared office
`Doc` contract as data-position expressions, decodes them back to sheet records,
and provides an open cell-edit payload for setting one cell without adding a
closed edit enum to the office core.
