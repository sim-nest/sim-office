# sim-codec-mspdi

In one line: project schedules can cross the Microsoft Project XML boundary with clear loss reporting.

## What it gives you

`sim-codec-mspdi` gives the office family a file exchange path for local Gantt
plans. It reads and writes the schedule pieces people need to inspect first:
task ids, names, dates, progress, and dependency links.

## Why you will be glad

- Project plans can move through common XML exchange files.
- Schedule data remains local and inspectable after import.
- Unsupported vendor fields are reported instead of silently disappearing.

## Where it fits

This crate sits between the local Gantt model and vendor project placements. It
lets Powerproject and Microsoft Project XML files participate in the office
document graph without making either vendor tool the schedule authority.
