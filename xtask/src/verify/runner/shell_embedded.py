"""The shell scripts inside other generated files (shell_find.py finds them), checked by
shellcheck as `shellcheck-embedded`: each script written to a file of its own and checked with the
shell that runs it. A finding names the file the script is in, where in it (`web: provision
"routes"`, `aws_instance.web remote-exec`, `runcmd`) and the line in the script; its rule is
shellcheck's code, so the baseline survives edits elsewhere.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import shell_find
from common import ERROR, FAIL, PASS, Ctx, Result, run, tail
from shell_find import Script

VALIDATOR = "shellcheck-embedded"


def check(ctx: Ctx, shown: Path, scripts: list[Script]) -> list[Result]:
    """`scripts`, found in `shown`, through shellcheck: a result per finding, else one pass."""
    if not scripts:
        return []
    findings: list[Result] = []
    with tempfile.TemporaryDirectory(prefix="embedded-") as tmp:
        by_file = {}
        for n, script in enumerate(scripts):
            path = Path(tmp) / f"{n}.sh"
            path.write_text(script.text, encoding="utf-8")
            by_file[str(path)] = script
        for shell in sorted({s.shell for s in scripts}):
            findings += batch(ctx, shown, shell, {f: s for f, s in by_file.items() if s.shell == shell})
    return findings or [Result(ctx.rel(shown), VALIDATOR, PASS, f"{len(scripts)} scripts")]


def batch(ctx: Ctx, shown: Path, shell: str, by_file: dict[str, Script]) -> list[Result]:
    """One shellcheck run over the scripts `by_file` names, all run by `shell`; each finding
    said of `shown`."""
    done = run(["shellcheck", "--format=json1", f"--shell={shell}", *by_file])
    try:
        comments = json.loads(done.out).get("comments", [])
    except json.JSONDecodeError:
        return [Result(ctx.rel(shown), VALIDATOR, ERROR, tail(done.both), "run")]
    results = []
    for c in comments:
        where = by_file[c["file"]].where
        code = f"SC{c['code']}"
        message = f"{where}: {code} ({c['level']}): {c['message']} (script line {c['line']})"
        results.append(Result(ctx.rel(shown), VALIDATOR, FAIL, message, code))
    return results


def vagrantfile(ctx: Ctx, path: Path) -> list[Result]:
    """A Vagrantfile's shell provisioners."""
    return check(ctx, path, shell_find.vagrantfile(path.read_text(encoding="utf-8")))


def module(ctx: Ctx, path: Path) -> list[Result]:
    """A Terraform module's scripts, each of its files on its own."""
    return [r for tf in sorted(path.glob("*.tf")) for r in check(ctx, tf, shell_find.module_file(tf.read_text(encoding="utf-8")))]


def cloud_config(ctx: Ctx, path: Path) -> list[Result]:
    """A cloud-config file's shell."""
    return check(ctx, path, shell_find.cloud_config(path.read_text(encoding="utf-8")))
