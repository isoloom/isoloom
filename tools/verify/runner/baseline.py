"""The baseline: failures known today, each with its cause, that don't fail a run.

A failure is known when the baseline has an entry with its file, validator and rule. The rule
leaves line numbers out (see common.Result), so an edit elsewhere in a file doesn't turn a known
failure into a new one. An entry nothing matches any more is reported as stale, for removal.
Each entry names its cause, and `causes` gives each cause's reason, once.
"""

from __future__ import annotations

import json
from dataclasses import asdict
from pathlib import Path

from common import ERROR, FAIL, Result

FAILING = (FAIL, ERROR)
UNEXPLAINED = ""

Key = tuple[str, str, str]


def key(entry: Result | dict) -> Key:
    """What matches a failure with its baseline entry."""
    if isinstance(entry, Result):
        return (entry.file, entry.validator, entry.rule)
    return (entry["file"], entry["validator"], entry.get("rule", ""))


def load(path: Path) -> dict:
    """The baseline file, or an empty one."""
    if not path.exists():
        return {"image": {}, "causes": {}, "failures": []}
    return json.loads(path.read_text(encoding="utf-8"))


def compare(results: list[Result], baseline: dict) -> tuple[list[Result], list[dict]]:
    """The failures the baseline doesn't know, and the entries no failure matches. An entry
    is stale only for a project this tree holds: checking another tree says nothing of it."""
    known = {key(e) for e in baseline.get("failures", [])}
    seen = {key(r) for r in results if r.status in FAILING}
    projects = {r.file.split("/", 1)[0] for r in results}
    new = [r for r in results if r.status in FAILING and key(r) not in known]
    stale = [e for e in baseline.get("failures", []) if key(e) not in seen and e["file"].split("/", 1)[0] in projects]
    return new, stale


def unexplained(baseline: dict) -> int:
    """How many causes have no reason written yet."""
    return sum(1 for reason in baseline.get("causes", {}).values() if not reason)


def updated(results: list[Result], baseline: dict, image: dict) -> dict:
    """The baseline for these results: every failure, each with its cause. A failure keeps
    the cause it had; a new one takes the cause of the same validator and rule elsewhere, or
    gets a cause of its own, `<validator>/<rule>`, whose reason is left empty to write."""
    old = baseline.get("failures", [])
    by_key = {key(e): e["cause"] for e in old if e.get("cause")}
    by_rule = {(e["validator"], e.get("rule", "")): e["cause"] for e in old if e.get("cause")}
    failures = sorted({key(r): r for r in results if r.status in FAILING}.values(), key=key)
    entries = []
    for r in failures:
        cause = by_key.get(key(r)) or by_rule.get((r.validator, r.rule)) or f"{r.validator}/{r.rule}"
        entries.append({"file": r.file, "validator": r.validator, "rule": r.rule, "cause": cause})
    reasons = baseline.get("causes", {})
    causes = {c: reasons.get(c, UNEXPLAINED) for c in sorted({e["cause"] for e in entries})}
    return {"image": image, "causes": causes, "failures": entries}


def dump(baseline: dict) -> str:
    """The baseline as written: a cause or a failure per line, so a change reads as a diff."""
    image = json.dumps(baseline.get("image", {}), ensure_ascii=False)
    causes = ",\n".join(f"    {json.dumps(c)}: {json.dumps(r, ensure_ascii=False)}" for c, r in baseline["causes"].items())
    failures = ",\n".join("    " + json.dumps(e, ensure_ascii=False) for e in baseline["failures"])
    return f'{{\n  "image": {image},\n  "causes": {{\n{causes}\n  }},\n  "failures": [\n{failures}\n  ]\n}}\n'


def line(r: Result, known: bool | None) -> str:
    """A result as a line of results.jsonl; a failure says whether the baseline knows it."""
    record = asdict(r)
    if known is not None:
        record["known"] = known
    return json.dumps(record, ensure_ascii=False)
