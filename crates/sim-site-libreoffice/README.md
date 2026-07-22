# sim-site-libreoffice

LibreOffice helper-process site for SIM office documents.

The crate registers `site/libreoffice` through the shared office document site
spine and keeps live UNO automation outside the runtime process. Live operations
require the `process-spawn` capability and `SIM_OFFICE_LIBREOFFICE_BRIDGE=1`.
The helper protocol is line-delimited JSON, so hosts can replace the helper in
tests or deployments without linking LibreOffice into the Rust crates.
