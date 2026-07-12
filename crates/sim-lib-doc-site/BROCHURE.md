# sim-lib-doc-site

In one line: the office bridge that makes external document places loadable without making them the frontend.

## What it gives you

This crate gives office integrations one place to register file services, helper processes, and modeled service doubles as document places. A caller can ask for data or preview a write through the same boundary, while the registered place carries its document kinds and required capabilities.

## Why you will be glad

- Vendor adapters plug into one documented boundary.
- Modeled calls give tests and recipes stable answers.
- Writes become previews first, so a live service is never changed by surprise.

## Where it fits

This crate sits next to the document core. It does not speak to Microsoft Graph, SharePoint, Dalux, or LibreOffice itself. Those adapters register through this boundary, while the document core keeps the shared records and edit payloads.
