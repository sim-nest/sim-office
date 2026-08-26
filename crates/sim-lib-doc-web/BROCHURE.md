# sim-lib-doc-web

In one line: Durable web-capture evidence adapters for SIM office documents.

## What it gives you

`sim-lib-doc-web` turns an offline capture into a durable office citation. Every quote remains bound to exact Unicode scalar offsets in one content-addressed representation, while the raw bytes, codec version, policy receipt, source URI, retrieval time, provider claim, and fidelity warnings remain inspectable. The same checked anchor renders as text, Markdown, Lisp, or JSON and is revalidated after restart. Changed bytes, a changed representation id, or a stale selector fail closed instead of producing a plausible-looking citation. The same representation and selector contracts now admit offline public-domain or private editions for. The contract keeps inputs, outputs, limits, and refusal cases explicit, so callers can compose the capability without acquiring unrelated host, transport, or product authority. Stable records make the result suitable for tests, inspection, and deterministic integration.

## Why you will be glad

- The public contract makes supported behavior, limits, and typed failures visible before integration.
- One owning crate prevents neighboring libraries from growing competing copies of the same policy.
- Deterministic records and checked tests keep adapters reviewable when implementations evolve.

## Where it fits

Within SIM, sim-lib-doc-web owns only the focused contract described above. Adjacent runtime libraries, platform adapters, codecs, and user surfaces can build around it while retaining their own policy. That boundary keeps the kernel small, avoids competing implementations, and lets this capability evolve without forcing unrelated components to change.
