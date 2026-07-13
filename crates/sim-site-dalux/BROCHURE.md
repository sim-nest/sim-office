# sim-site-dalux

In one line: Dalux project items become local SIM office records behind API identity gates.

## What it gives you

This crate gives construction project data a Dalux boundary that reads items
into local office documents and keeps live service access behind a bearer token
from an API identity. It gives hosts a small, named place to connect Dalux
without changing the document model.

## Why you will be glad

- Project item lists can be reviewed without a live Dalux account in tests.
- Note updates carry only the note text, not a broad edit payload.
- Service errors hide access tokens and long project names before they leave the adapter.

## Where it fits

It sits with the other office site adapters in `sim-office`. Use it when a SIM
workflow needs Dalux project item evidence or a narrow item note update.
