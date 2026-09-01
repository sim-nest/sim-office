# sim-site-libreoffice

LibreOffice helper-process site for SIM office documents.

The crate registers `site/libreoffice` through the shared office document site
spine and keeps UNO mechanics outside office behavior. Composition supplies
bounded configuration, three preopened mounts, and either a `ProcessPort` or an
`IpcPort`; there is no environment gate, temporary-directory discovery, direct
spawn, or native IPC fallback. The line-delimited JSON protocol returns a
privacy-safe, ledger-ready receipt and typed missing-helper, denial, timeout,
helper-refusal, and unavailable-IPC outcomes.
