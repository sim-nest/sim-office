# sim-codec-mspdi

MSPDI XML exchange for SIM office Gantt documents.

The crate reads and writes Microsoft Project XML schedule files for the local
Gantt domain. Task ids, names, local dates, completion percentages, and
predecessor links map to portable schedule records, while unsupported MSPDI
fields are surfaced through the shared fidelity report.
