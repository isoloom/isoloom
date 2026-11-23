"""Scripts and Dockerfiles: shellcheck, PSScriptAnalyzer and hadolint, each over all its files at once."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from common import ERROR, FAIL, WARN, Ctx, Result, passed, run, tail
from discover import first_line

# PSScriptAnalyzer over the files listed in $args[0], one record per finding, as JSON.
ANALYZE = r"""
$ErrorActionPreference = 'Stop'
$found = foreach ($f in Get-Content -LiteralPath $args[0]) {
  try {
    Invoke-ScriptAnalyzer -Path $f | ForEach-Object {
      [pscustomobject]@{ file = $f; line = $_.Line; rule = $_.RuleName; severity = $_.Severity.ToString(); message = $_.Message }
    }
  } catch {
    [pscustomobject]@{ file = $f; line = 0; rule = 'run'; severity = 'RunError'; message = $_.Exception.Message }
  }
}
ConvertTo-Json -InputObject @($found) -Depth 3 -Compress
"""


def collect(ctx: Ctx, files: list[Path], validator: str, findings: list[Result]) -> list[Result]:
    """The findings, plus a pass for every file without one."""
    named = {r.file for r in findings}
    return findings + [passed(ctx, f, validator) for f in files if ctx.rel(f) not in named]


def shellcheck_batch(ctx: Ctx, files: list[Path], shell: str | None) -> list[Result]:
    """One shellcheck run. Every finding fails the file, as shellcheck's exit status says."""
    if not files:
        return []
    args = ["shellcheck", "--format=json1"] + ([f"--shell={shell}"] if shell else [])
    done = run(args + [str(f) for f in files])
    try:
        comments = json.loads(done.out).get("comments", [])
    except json.JSONDecodeError:
        return [Result(ctx.rel(f), "shellcheck", ERROR, tail(done.both), "run") for f in files]
    findings = [comment(ctx, c) for c in comments]
    return collect(ctx, files, "shellcheck", findings)


def comment(ctx: Ctx, c: dict) -> Result:
    """One of shellcheck's findings."""
    code = f"SC{c['code']}"
    return Result(ctx.rel(Path(c["file"])), "shellcheck", FAIL, f"{code} ({c['level']}): {c['message']} (line {c['line']})", code)


def shellcheck(ctx: Ctx, files: list[Path]) -> list[Result]:
    """shellcheck. A script without a shebang is run with `sh` by what generates it, so it is
    checked as POSIX sh rather than refused for its unknown shell."""
    named = [f for f in files if first_line(f).startswith("#!") or "shellcheck shell=" in f.read_text(errors="replace")]
    bare = [f for f in files if f not in named]
    return shellcheck_batch(ctx, named, None) + shellcheck_batch(ctx, bare, "sh")


def hadolint(ctx: Ctx, files: list[Path]) -> list[Result]:
    """hadolint. Its default threshold fails a file on anything but a style note."""
    if not files:
        return []
    done = run(["hadolint", "--format", "json", "--no-color"] + [str(f) for f in files])
    try:
        found = json.loads(done.out)
    except json.JSONDecodeError:
        return [Result(ctx.rel(f), "hadolint", ERROR, tail(done.both), "run") for f in files]
    return collect(ctx, files, "hadolint", [lint(ctx, x) for x in found])


def lint(ctx: Ctx, x: dict) -> Result:
    """One of hadolint's findings: a style note warns, anything else fails."""
    status = WARN if x["level"] == "style" else FAIL
    return Result(ctx.rel(Path(x["file"])), "hadolint", status, f"{x['code']} ({x['level']}): {x['message']} (line {x['line']})", x["code"])


def psscriptanalyzer(ctx: Ctx, files: list[Path]) -> list[Result]:
    """PSScriptAnalyzer's default rules: errors and warnings fail, information doesn't."""
    if not files:
        return []
    with tempfile.TemporaryDirectory() as tmp:
        listing, script = Path(tmp) / "files.txt", Path(tmp) / "analyze.ps1"
        listing.write_text("\n".join(str(f) for f in files) + "\n", encoding="utf-8")
        script.write_text(ANALYZE, encoding="utf-8")
        done = run(["pwsh", "-NoProfile", "-NonInteractive", "-File", str(script), str(listing)])
    try:
        found = json.loads(done.out or "[]")
    except json.JSONDecodeError:
        return [Result(ctx.rel(f), "psscriptanalyzer", ERROR, tail(done.both), "run") for f in files]
    findings = []
    for x in found:
        status = {"Information": WARN, "RunError": ERROR}.get(x["severity"], FAIL)
        message = f"{x['rule']} ({x['severity']}): {' '.join(str(x['message']).split())} (line {x['line']})"
        findings.append(Result(ctx.rel(Path(x["file"])), "psscriptanalyzer", status, message, x["rule"]))
    return collect(ctx, files, "psscriptanalyzer", findings)
