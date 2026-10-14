#!/usr/bin/env python3
"""Refreshes crates/isoloom-coverage/coverage/cloud-init.txt: every key of the cloud-config
format, from cloud-init's own schema (`schema-cloud-config-v1.json`) at the pinned release. The
top-level keys, then the keys of the items Isoloom writes (`users`, `write_files`, `power_state`),
one a line.

Usage: python3 scripts/refresh-cloud-init.py   (then `cargo test`: keys with no row fail)
ISOLOOM_REFRESH=latest reads the newest release instead of the pin.
"""
import json
import os
import pathlib
import urllib.request

PIN = "26.2"
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/isoloom-coverage/coverage/cloud-init.txt"
# The nested keys listed: the items Isoloom writes, under their prefix.
NESTED = [("users.*.", "users"), ("write_files.*.", "write_files"), ("power_state.", "power_state")]


def version() -> str:
    if os.environ.get("ISOLOOM_REFRESH") != "latest":
        return PIN
    with urllib.request.urlopen("https://api.github.com/repos/canonical/cloud-init/releases/latest") as r:
        return json.load(r)["tag_name"]


def keys(defs: dict, node: dict, seen: frozenset = frozenset()) -> dict:
    """The properties a node allows, following `$ref`, `allOf`/`anyOf`/`oneOf` and array items."""
    out = {}
    ref = node.get("$ref")
    if ref and ref not in seen:
        out.update(keys(defs, defs[ref.rsplit("/", 1)[-1]], seen | {ref}))
    for combinator in ("allOf", "anyOf", "oneOf"):
        for sub in node.get(combinator, []):
            out.update(keys(defs, sub, seen))
    if isinstance(node.get("items"), dict):
        out.update(keys(defs, node["items"], seen))
    # The schema's own top-level `properties` only mark the keys as allowed (`{}`): the
    # definitions above say what each holds.
    for k, v in node.get("properties", {}).items():
        out.setdefault(k, v)
    return out


def main() -> None:
    v = version()
    url = f"https://raw.githubusercontent.com/canonical/cloud-init/{v}/cloudinit/config/schemas/schema-cloud-config-v1.json"
    with urllib.request.urlopen(url) as r:
        schema = json.load(r)
    defs = schema["$defs"]
    top = keys(defs, schema)
    lines = sorted(top)
    for prefix, key in NESTED:
        lines += sorted(prefix + k for k in keys(defs, top[key]))
    OUT.write_text(f"# Keys of cloud-init {v} schema-cloud-config-v1.json\n" + "\n".join(lines) + "\n")
    print(f"cloud-init {v}: {len(lines)} keys")


if __name__ == "__main__":
    main()
