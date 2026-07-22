# SharePoint REST fallback

Describes SharePoint REST `_api` calls as explicit fallback operations beside
Graph reads. The descriptor keeps the REST path narrow: batch entries carry an
HTTP method, URL, and optional JSON body, while live credentials and network
access remain host-gated.
