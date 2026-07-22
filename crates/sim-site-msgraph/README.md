# sim-site-msgraph

Microsoft Graph site adapter for SIM office documents.

The crate exposes a small Graph client boundary with deterministic cassettes for
modeled calls and an explicit live mode for host-owned Microsoft Graph access.
Live reads require the office network and credential capabilities plus the
`SIM_OFFICE_LIVE_MS_GRAPH=1` environment gate before any token is requested or
HTTP call is attempted.

The site helper registers the adapter through the shared office document site
spine, so callers see an opaque `site` export rather than a vendor-specific
kernel type. Error messages redact bearer tokens and truncate long response
bodies.
