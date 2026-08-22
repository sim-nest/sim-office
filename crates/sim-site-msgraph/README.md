# sim-site-msgraph

Microsoft Graph site adapter for SIM office documents.

The crate exposes a small Graph client boundary with deterministic cassettes and
an injected `GraphPort`. Office owns request, response, authentication, and site
policy; platform composition supplies socket, DNS, TLS, timeout, and open
realization. No environment variable or ambient HTTP client can enable a live
request. Network and credential capabilities are checked before token or port
use.

The site helper registers the adapter through the shared office document site
spine, so callers see an opaque `site` export rather than a vendor-specific
kernel type. Error messages redact bearer tokens and truncate long response
bodies.
