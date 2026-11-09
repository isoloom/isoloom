"""Compose files, Vagrantfiles and Ansible inventories and playbooks: one file, one run."""

from __future__ import annotations

import json
import re
import tempfile
from pathlib import Path

from common import ERROR, FAIL, WARN, Ctx, Result, cause, passed, run, tail

COMPOSE_WARNING = re.compile(r'level=warning msg="(.*)"')
ANSIBLE_WARNING = re.compile(r"^\[WARNING\]: (.*)$", re.M)
DIGITS = re.compile(r"\d+")
ANSIBLE_ENV = {"ANSIBLE_HOME": "/tmp/ansible", "ANSIBLE_LOCAL_TEMP": "/tmp/ansible/tmp"}
# An inventory source Ansible can't parse is only a warning, unless it is made an error.
INVENTORY_ENV = {**ANSIBLE_ENV, "ANSIBLE_INVENTORY_UNPARSED_FAILED": "True", "ANSIBLE_INVENTORY_ANY_UNPARSED_IS_FAILED": "True"}


def compose(ctx: Ctx, path: Path) -> list[Result]:
    """`docker compose config`: the file as Compose reads it, interpolated and checked against
    its schema. It needs no Docker daemon."""
    done = run(["docker", "compose", "-f", path.name, "config", "--quiet"], cwd=path.parent)
    warnings = [Result(ctx.rel(path), "compose", WARN, m, DIGITS.sub("#", m)[:120]) for m in COMPOSE_WARNING.findall(done.err)]
    if done.code != 0:
        return warnings + [Result(ctx.rel(path), "compose", FAIL, tail(done.both), "config")]
    return warnings + [passed(ctx, path, "compose")]


def vagrant(ctx: Ctx, path: Path) -> list[Result]:
    """`vagrant validate --ignore-provider`: the Vagrantfile loads, and its machine settings
    (not the providers') are valid. Each run has its own Vagrant home."""
    with tempfile.TemporaryDirectory() as home:
        env = {"VAGRANT_CWD": str(path.parent), "VAGRANT_HOME": home}
        done = run(["vagrant", "validate", "--ignore-provider"], cwd=path.parent, env=env)
    if done.code == 0:
        return [passed(ctx, path, "vagrant")]
    bullets = [line.strip()[2:] for line in done.both.splitlines() if line.strip().startswith("* ")]
    if not bullets:
        # A Ruby exception: its first line says what and where, the rest is Vagrant's stack.
        first = [line.strip() for line in done.both.splitlines() if line.strip() and not line.startswith("\tfrom ")]
        return [Result(ctx.rel(path), "vagrant", FAIL, "\n".join(first[:3]), cause(done.both, "validate"))]
    return [Result(ctx.rel(path), "vagrant", FAIL, b, DIGITS.sub("#", b)[:120]) for b in bullets]


def inventory(ctx: Ctx, path: Path) -> list[Result]:
    """`ansible-inventory --list`, with a source it can't parse made an error."""
    done = run(["ansible-inventory", "-i", str(path), "--list"], cwd=path.parent, env=INVENTORY_ENV)
    warnings = [Result(ctx.rel(path), "ansible-inventory", WARN, w, DIGITS.sub("#", w)[:120]) for w in ANSIBLE_WARNING.findall(done.err)]
    if done.code != 0:
        return warnings + [Result(ctx.rel(path), "ansible-inventory", FAIL, tail(done.err or done.out), "parse")]
    return warnings + [passed(ctx, path, "ansible-inventory")]


def playbook(ctx: Ctx, path: Path) -> list[Result]:
    """ansible-lint (which runs `ansible-playbook --syntax-check` first), offline: it installs
    nothing. Its warnings don't fail the file."""
    done = run(["ansible-lint", "--offline", "--nocolor", "-f", "json", path.name], cwd=path.parent, env=ANSIBLE_ENV)
    try:
        found = json.loads(done.out or "[]")
    except json.JSONDecodeError:
        return [Result(ctx.rel(path), "ansible-lint", ERROR, tail(done.both), "run")]
    results = []
    for x in found:
        where = path.parent / x.get("location", {}).get("path", path.name)
        status = WARN if x.get("severity") in ("info", "minor") else FAIL
        if x.get("check_name", "").startswith("internal-error"):
            status = ERROR
        detail = (x.get("content") or {}).get("body", "")
        message = f"{x.get('check_name')}: {x.get('description', '')}" + (f" ({detail})" if detail else "")
        results.append(Result(ctx.rel(where), "ansible-lint", status, message, x.get("check_name", "")))
    if done.code not in (0, 2) and not results:
        results.append(Result(ctx.rel(path), "ansible-lint", ERROR, tail(done.both), "run"))
    return results or [passed(ctx, path, "ansible-lint")]
