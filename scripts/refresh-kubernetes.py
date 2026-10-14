#!/usr/bin/env python3
"""Refreshes crates/isoloom-coverage/coverage/kubernetes/: the pinned sources of the Kubernetes,
Kustomize and Helm coverage tables, one field per line (`Definition field type`).

- openapi.txt: the object kinds of the core, apps, batch and networking groups (v1), then every
  field of every definition they reach, from the Kubernetes OpenAPI document (`status`, which
  the cluster writes, left out).
- kustomization.txt: every definition of SchemaStore's kustomization.yaml schema.
- chart.txt: Chart.yaml's fields, from SchemaStore's Chart.yaml schema.

A type is a definition's name, a JSON type, `[]` before a list's items or `{}` before a map's
values.

Usage: python3 scripts/refresh-kubernetes.py   (then `cargo test`: unclassified fields fail)
With ISOLOOM_REFRESH=latest it reads the newest Kubernetes release and SchemaStore's master.
"""
import json
import os
import pathlib
import urllib.request

# The Kubernetes version CI's kind cluster runs (helm/kind-action v1.15.1, kind v0.33.0).
KUBERNETES = "v1.37.0"
SCHEMASTORE = "8ae3e0c8c1d637a9bbb929e2ba7d77343df98465"
GROUPS = ("", "apps", "batch", "networking.k8s.io")
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/isoloom-coverage/coverage/kubernetes"


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=60) as r:
        return r.read()


def pins() -> tuple[str, str]:
    if os.environ.get("ISOLOOM_REFRESH") != "latest":
        return KUBERNETES, SCHEMASTORE
    latest = json.loads(fetch("https://api.github.com/repos/kubernetes/kubernetes/releases/latest"))["tag_name"]
    return latest, "master"


def short(ref: str) -> str:
    return ref.rsplit("/", 1)[-1].rsplit(".", 1)[-1]


def kind_of(field: dict, inline: dict, owner: str, name: str) -> str:
    """A field's type. An inline object with properties of its own becomes a definition named
    after its owner and field (`Chart` `maintainers` -> `ChartMaintainers`)."""
    if "$ref" in field:
        return short(field["$ref"])
    for combinator in ("allOf", "anyOf", "oneOf"):
        if field.get(combinator):
            return kind_of(field[combinator][0], inline, owner, name)
    if field.get("type") == "array":
        return "[]" + kind_of(field.get("items", {}), inline, owner, name)
    if isinstance(field.get("additionalProperties"), dict):
        return "{}" + kind_of(field["additionalProperties"], inline, owner, name)
    if field.get("properties"):
        own = owner + name[:1].upper() + name[1:]
        inline[own] = field
        return own
    return field.get("type", "object")


def fields(name: str, definition: dict, inline: dict) -> list[str]:
    props = definition.get("properties", {})
    return [f"{name} {f} {kind_of(spec, inline, name, f)}" for f, spec in sorted(props.items()) if f != "status"]


def openapi(version: str) -> str:
    url = f"https://raw.githubusercontent.com/kubernetes/kubernetes/{version}/api/openapi-spec/swagger.json"
    defs = json.loads(fetch(url))["definitions"]
    by_short = {short(n): n for n in defs}
    found = {}
    for name, d in defs.items():
        for gvk in d.get("x-kubernetes-group-version-kind", []):
            if gvk["version"] == "v1" and gvk["group"] in GROUPS and not name.endswith("List") and "apimachinery" not in name:
                found[gvk["kind"]] = (f"{gvk['group']}/v1" if gvk["group"] else "v1", name)
    lines = [f"# Kubernetes {version} OpenAPI (api/openapi-spec/swagger.json): kinds of the core, apps, batch and",
             "# networking groups (`kind Kind apiVersion Definition`), then every field of every definition they reach."]
    lines += [f"kind {k} {api} {short(n)}" for k, (api, n) in sorted(found.items())]
    seen, todo = set(), [n for _, n in found.values()]
    while todo:
        n = todo.pop()
        if n in seen:
            continue
        seen.add(n)
        for line in fields(short(n), defs[n], {}):
            target = by_short.get(line.split()[2].lstrip("[]{}"))
            if target:
                todo.append(target)
    for n in sorted(seen, key=short):
        lines += fields(short(n), defs[n], {})
    return "\n".join(lines) + "\n"


def schemastore(file: str, root: str, rev: str, title: str) -> str:
    url = f"https://raw.githubusercontent.com/SchemaStore/schemastore/{rev}/src/schemas/json/{file}"
    schema = json.loads(fetch(url))
    defs = dict(schema.get("definitions", {}))
    if "properties" in schema:
        defs[root] = schema
    lines = [f"# {title}: SchemaStore {file} at {rev}, every definition's fields."]
    inline: dict = {}
    for name in sorted(defs):
        lines += fields(name, defs[name], inline)
    done: set = set()
    while set(inline) - done:
        name = min(set(inline) - done)
        done.add(name)
        lines += fields(name, inline[name], inline)
    return "\n".join(lines) + "\n"


def main() -> None:
    version, rev = pins()
    OUT.mkdir(parents=True, exist_ok=True)
    outputs = {
        "openapi.txt": openapi(version),
        "kustomization.txt": schemastore("kustomization.json", "Kustomization", rev, "kustomization.yaml"),
        "chart.txt": schemastore("chart.json", "Chart", rev, "Helm's Chart.yaml"),
    }
    for name, text in outputs.items():
        (OUT / name).write_text(text)
        count = sum(1 for line in text.splitlines() if not line.startswith(("#", "kind ")))
        print(f"{name}: {count} fields")


if __name__ == "__main__":
    main()
