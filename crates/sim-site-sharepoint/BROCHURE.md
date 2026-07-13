# sim-site-sharepoint
In one line: SharePoint lists and drive folders become reviewable SIM office records.

## What it gives you

This crate turns SharePoint list rows into local sheet documents and drive
children into external references that keep their web links and ETags. A host can
read the Graph response, inspect the shape, and decide what to do before any
write is attempted.

## Why you will be glad

Project files and list metadata often live in SharePoint even when the work
happens elsewhere. This crate gives that material a calm local form, with enough
version evidence to avoid blind writes against stale files.

## Where it fits

It sits beside the Microsoft Graph adapter in `sim-office`. Use it when a SIM
workflow needs SharePoint lists, document libraries, or drive folders as office
placement data.
