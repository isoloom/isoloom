"""Terraform modules: `terraform validate` and tflint, then the cloud-config documents they
build (embedded.py)."""

from __future__ import annotations

import json
from pathlib import Path

import embedded
from common import ERROR, FAIL, WARN, Ctx, Result, passed, run, tail
from tfcli import data_dir, init


def diagnostic(ctx: Ctx, module: Path, d: dict) -> Result:
    """One of `terraform validate -json`'s diagnostics."""
    where = d.get("range") or {}
    path = module / where["filename"] if where.get("filename") else module
    line = (where.get("start") or {}).get("line")
    summary = d.get("summary", "").strip()
    detail = " ".join(d.get("detail", "").split())
    message = f"{summary}: {detail}" + (f" (line {line})" if line else "")
    status = FAIL if d.get("severity") == "error" else WARN
    return Result(ctx.rel(path), "terraform", status, message, summary)


def validate(ctx: Ctx, module: Path) -> list[Result]:
    """`terraform validate` after `init -backend=false`."""
    done = init(ctx, module)
    if done.code != 0:
        return [Result(ctx.rel(module), "terraform", ERROR, "init failed: " + tail(done.both), "init")]
    done = run(["terraform", "validate", "-json", "-no-color"], cwd=module, env=data_dir(module))
    try:
        report = json.loads(done.out)
    except json.JSONDecodeError:
        return [Result(ctx.rel(module), "terraform", ERROR, tail(done.both), "validate")]
    results = [diagnostic(ctx, module, d) for d in report.get("diagnostics", [])]
    return results or [passed(ctx, module, "terraform")]


def tflint(ctx: Ctx, module: Path) -> list[Result]:
    """tflint with the AWS, Azure and Google rulesets (see the image's tflint.hcl)."""
    done = run(["tflint", "--format=json", "--no-color"], cwd=module)
    try:
        report = json.loads(done.out)
    except json.JSONDecodeError:
        return [Result(ctx.rel(module), "tflint", ERROR, tail(done.both), "run")]
    results = [
        Result(ctx.rel(module), "tflint", ERROR, " ".join(e.get("message", "").split()), "run")
        for e in report.get("errors") or []
    ]
    for issue in report.get("issues") or []:
        rule = issue.get("rule", {}).get("name", "")
        where = issue.get("range") or {}
        line = (where.get("start") or {}).get("line")
        path = module / where["filename"] if where.get("filename") else module
        message = f"{issue.get('message', '')}" + (f" (line {line})" if line else "")
        results.append(Result(ctx.rel(path), "tflint", FAIL, message, rule))
    return results or [passed(ctx, module, "tflint")]


def module(ctx: Ctx, path: Path) -> list[Result]:
    """Everything a module gets: validate, tflint, and its cloud-config documents. Terraform
    loads every provider's schema, so only a few modules run at once."""
    results = tflint(ctx, path)
    with ctx.terraform_slots:
        checked = validate(ctx, path)
        valid = not any(r.status in (FAIL, ERROR) for r in checked)
        results += checked + embedded.check(ctx, path, valid)
    return results
