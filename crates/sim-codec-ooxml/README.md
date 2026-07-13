# sim-codec-ooxml

OOXML spreadsheet codec for SIM office documents.

The crate reads and writes a narrow, valid `.xlsx` package for the local sheet
domain. It keeps spreadsheet numbers exact by serializing rationals as text
with explicit SIM metadata, rejects binary `.xls` files, and reports unsupported
styles or merged cells through the shared fidelity report.
