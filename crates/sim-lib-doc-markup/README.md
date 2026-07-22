# sim-lib-doc-markup

Markup document codecs for SIM office documents.

The crate adapts the shared markup backends to the office `DocCodec` boundary.
Markdown, Typst, AsciiDoc, and LaTeX sources decode to `DocKind("article")`
documents whose body is the shared markup document value, and the same document
can encode through any implemented markup backend. Fidelity reports carry
dropped fields, preserved raw fragments, and warnings from the markup layer.
