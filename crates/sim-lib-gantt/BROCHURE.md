# sim-lib-gantt

In one line: local project schedules that can be checked and reopened without a vendor system.

## What it gives you

This crate gives office work a durable Gantt plan model: tasks, dates, dependency links, progress, and a local database for reopening the same plan later. It also identifies the zero-slack tasks that control the schedule, using the shared graph toolkit instead of a private dependency engine.

## Why you will be glad

- Project plans stay useful offline and in repeatable tests.
- Cyclic dependencies fail before they become misleading schedule answers.
- Vendor imports and exports have a clean local model to target.

## Where it fits

This crate sits beside the document core, local document store, and vendor site crates. It supplies the schedule object that Powerproject, Project for the web, SharePoint evidence, and ledger close work can reference without making those backends the source of truth.
