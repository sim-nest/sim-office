# sim-codec-ooxml

OOXML spreadsheet and presentation codecs for SIM office documents.

The crate reads and writes narrow `.xlsx` and `.pptx` packages for the local
sheet and deck domains. Spreadsheet numbers stay exact through explicit SIM
metadata, presentation slides carry portable block metadata, binary Office
formats are rejected, and unsupported workbook or slide features are reported
through the shared fidelity report.
