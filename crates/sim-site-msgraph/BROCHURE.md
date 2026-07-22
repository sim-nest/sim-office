# sim-site-msgraph

In one line: Microsoft Graph documents can enter SIM through a modeled-first office site.

## What it gives you

This crate gives the office family a Microsoft Graph boundary that works with
stable recorded answers by default and requires deliberate host permission for
live service reads. It keeps the vendor connection outside the kernel while
still fitting the shared document site shape.

## Why you will be glad

- Tests can use recorded Graph answers without network accounts.
- Live access has a clear permission gate.
- Service errors stay useful without exposing bearer tokens or long payloads.

## Where it fits

This crate sits beside the document site spine. The spine supplies the loadable
site shape, and this adapter supplies the Microsoft Graph side of that shape for
office documents.
