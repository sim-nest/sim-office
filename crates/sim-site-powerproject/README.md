# sim-site-powerproject

`sim-site-powerproject` registers Powerproject and Project for the web as gated
placements for SIM Gantt documents. Local schedules stay in the portable Gantt
model, while OLE receipts and Dataverse operation plans describe how the same
plan crosses vendor boundaries.

Live Powerproject export is available only on Windows when the
`powerproject-ole` feature and host bridge environment are both enabled. Project
for the web output is represented as Dataverse Project Schedule Service
operations so hosts can review the operation set before sending it.

## Documentation

Run the repository documentation command from the `sim-office` root:

```bash
cargo run -p xtask -- simdoc
```

The generated `docs/` tree is owned by that command.
