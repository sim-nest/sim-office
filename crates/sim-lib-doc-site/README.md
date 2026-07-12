# sim-lib-doc-site

Document site registration and modeled realize calls for SIM office documents.

The crate registers a `DocSite` as the kernel's opaque `site` export kind and
resolves calls through one modeled/live boundary. Modeled sites return
deterministic cassette-shaped data. Live sites require their declared
capabilities before any call runs, and writes return preview edits instead of
mutating a backend directly.
