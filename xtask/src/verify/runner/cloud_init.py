"""cloud-config documents: `cloud-init schema`, for files and for documents a module builds."""

from __future__ import annotations

import re
import tempfile
from pathlib import Path

from common import ERROR, FAIL, PASS, WARN, Ctx, Result, run, tail

# "<key path>: <message>, <key path>: <message>": a new entry starts where a key path does.
ENTRY = re.compile(r", (?=[\w.\-\[\]]+: )")
ERRORS = "Cloud config schema errors: "
DEPRECATIONS = "Cloud config schema deprecations: "


def entries(output: str, prefix: str) -> list[tuple[str, str]]:
    """The (key path, message) pairs of the output line that starts with `prefix`."""
    for line in output.splitlines():
        at = line.find(prefix)
        if at >= 0:
            pairs = [e.split(": ", 1) for e in ENTRY.split(line[at + len(prefix) :].strip())]
            return [(p[0], p[1] if len(p) > 1 else "") for p in pairs]
    return []


def check_file(ctx: Ctx, path: Path, where: str = "", shown: Path | None = None) -> list[Result]:
    """`cloud-init schema -c` on `path`. A document Terraform built names the module's file
    (`shown`) and where in it the document is (`where`, in the message). The rule is the key
    the schema refuses."""
    done = run(["cloud-init", "schema", "-c", str(path)])
    name = ctx.rel(shown or path)
    prefix = f"{where}: " if where else ""
    results = [Result(name, "cloud-init", FAIL, f"{prefix}{key}: {message}", key) for key, message in entries(done.both, ERRORS)]
    results += [Result(name, "cloud-init", WARN, f"{prefix}{key}: {message}", key) for key, message in entries(done.both, DEPRECATIONS)]
    if done.code != 0 and not any(r.status == FAIL for r in results):
        results.append(Result(name, "cloud-init", ERROR, prefix + tail(done.both), "schema"))
    return results or [Result(name, "cloud-init", PASS, where)]


def check_text(ctx: Ctx, shown: Path, text: str, where: str) -> list[Result]:
    """A document Terraform rendered (see terraform.embedded), checked as a file would be."""
    with tempfile.NamedTemporaryFile("w", suffix=".yaml", encoding="utf-8", delete=False) as f:
        f.write(text)
    try:
        return check_file(ctx, Path(f.name), where, shown)
    finally:
        Path(f.name).unlink()
