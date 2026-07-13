# sim-lib-gantt

Local Gantt schedule plans for the SIM office suite.

The crate stores exact local task dates, typed dependency links, and completion
percentages before any vendor project tool is involved. Critical path analysis
uses the shared discrete graph crate, and the SQLite store keeps local schedule
snapshots available across process restarts.
