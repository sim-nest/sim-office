# sim-site-powerproject
In one line: Powerproject and Project for the web become permissioned places for SIM Gantt plans.

## What it gives you

This crate gives schedule work a vendor boundary without changing the local plan
model. It names Powerproject as a live desktop placement, names Project for the
web as a Dataverse placement, and keeps both paths tied to the same task and
dependency records.

## Why you will be glad

Teams can inspect the exact operation set before a schedule leaves SIM. The
desktop path stays behind explicit host gates, while modeled receipts let tests
prove that exported MSPDI still returns to the same Gantt surface.

## Where it fits

It uses the construction-owned MSPDI codec and the local Gantt library. Use
it when a schedule needs a Powerproject desktop export, a Project for the web
update plan, or a deterministic receipt for review.
