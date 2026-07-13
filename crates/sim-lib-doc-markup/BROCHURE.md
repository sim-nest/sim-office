# sim-lib-doc-markup

In one line: article files enter the office document flow through the same markup body.

## What it gives you

`sim-lib-doc-markup` lets Markdown, Typst, AsciiDoc, and LaTeX articles cross the
office file boundary as ordinary article documents. The shared markup body stays
portable, so one imported article can be exported through another supported
markup format without leaving the office document contract.

## Why you will be glad

- Article imports use the same document model as stores, sites, and views.
- Fidelity reports name preserved raw fragments, warnings, and dropped parts.
- The core office crate stays free of markup parsing dependencies.

## Where it fits

This crate is a ring-2 adapter in the office family. It depends on the markup
codec crate and the office document core, while the core remains a narrow record
and placement contract for every document domain.
