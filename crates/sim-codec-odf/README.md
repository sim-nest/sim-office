# sim-codec-odf

ODF spreadsheet and presentation codecs for SIM office documents.

The crate reads and writes `.ods` and `.odp` packages for the local sheet and
deck domains. The package writer places the ODF `mimetype` entry first and
uncompressed, sheet values keep exact SIM metadata, deck slides carry portable
block metadata, and unsupported styling is reported through the shared fidelity
report.
