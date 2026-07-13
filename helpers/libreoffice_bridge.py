#!/usr/bin/env python3
"""Line-delimited JSON bridge for LibreOffice UNO automation."""

from __future__ import annotations

import json
import pathlib
import sys
from typing import Any


SITE_ID = "site/libreoffice"
UNO_URL = "uno:socket,host=localhost,port=2002;urp;StarOffice.ComponentContext"


def main() -> int:
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            request = json.loads(line)
            reply = handle(request)
        except Exception as exc:  # noqa: BLE001 - helper protocol reports all failures.
            reply = {"error": str(exc)}
        sys.stdout.write(json.dumps(reply, separators=(",", ":")) + "\n")
        sys.stdout.flush()
    return 0


def handle(request: dict[str, Any]) -> dict[str, Any]:
    op = request.get("op")
    if op == "open":
        path = pathlib.Path(require_string(request, "path"))
        doc = desktop().loadComponentFromURL(path.resolve().as_uri(), "_blank", 0, ())
        if doc is None:
            raise RuntimeError(f"LibreOffice could not open {path}")
        return {
            "backend": SITE_ID,
            "external_id": f"uno:{path.resolve()}",
            "version": None,
            "web_url": None,
        }
    if op == "export_pdf":
        out = pathlib.Path(require_string(request, "out"))
        doc_ref = require_dict(request, "doc")
        source = external_ref_path(doc_ref)
        doc = desktop().loadComponentFromURL(source.resolve().as_uri(), "_blank", 0, ())
        if doc is None:
            raise RuntimeError(f"LibreOffice could not open {source}")
        props = (property_value("FilterName", "writer_pdf_Export"),)
        doc.storeToURL(out.resolve().as_uri(), props)
        doc.close(True)
        return {
            "backend": SITE_ID,
            "external_id": f"pdf:{out.resolve()}",
            "version": None,
            "web_url": None,
        }
    raise ValueError(f"unsupported UNO command {op!r}")


def desktop() -> Any:
    import uno  # type: ignore[import-not-found]

    local_ctx = uno.getComponentContext()
    resolver = local_ctx.ServiceManager.createInstanceWithContext(
        "com.sun.star.bridge.UnoUrlResolver",
        local_ctx,
    )
    ctx = resolver.resolve(UNO_URL)
    return ctx.ServiceManager.createInstanceWithContext(
        "com.sun.star.frame.Desktop",
        ctx,
    )


def property_value(name: str, value: str) -> Any:
    import uno  # type: ignore[import-not-found]

    prop = uno.createUnoStruct("com.sun.star.beans.PropertyValue")
    prop.Name = name
    prop.Value = value
    return prop


def external_ref_path(ref: dict[str, Any]) -> pathlib.Path:
    external_id = require_string(ref, "external_id")
    if external_id.startswith("uno:"):
        return pathlib.Path(external_id.removeprefix("uno:"))
    return pathlib.Path(external_id)


def require_string(source: dict[str, Any], key: str) -> str:
    value = source.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{key} must be a non-empty string")
    return value


def require_dict(source: dict[str, Any], key: str) -> dict[str, Any]:
    value = source.get(key)
    if not isinstance(value, dict):
        raise ValueError(f"{key} must be an object")
    return value


if __name__ == "__main__":
    raise SystemExit(main())
