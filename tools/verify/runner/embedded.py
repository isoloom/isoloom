"""The cloud-config documents a Terraform module builds, evaluated by Terraform, checked by
`cloud-init schema`.

The generated modules write user data as an expression (`format("#cloud-config\\n%s",
yamlencode({...}))`), so a document only exists once Terraform evaluates it: `terraform console`
does, in a copy of the project, so nothing written for the evaluation reaches the files other
modules list while they are being checked.
"""

from __future__ import annotations

import base64
import binascii
import shutil
import tempfile
from pathlib import Path

import cloud_init
import hcl
from common import ERROR, SKIP, Ctx, Done, Result, run, tail
from discover import OUTPUT_DIR
from tfcli import data_dir, init

EVALUATED = "zz_isoloom_verify.tf"
OVERRIDE = "zz_isoloom_verify_override.tf"
UNKNOWN = "it is built from values known only after apply"
# The document, base64-encoded so the console prints it on one line, and shown even when it
# is built from a sensitive value (the console hides those).
SHOW = "base64encode(try(nonsensitive(local.isoloom_verify), local.isoloom_verify))\n"
# `.terraform/`, `.terraform.lock.hcl` and the like, left out of the copy.
TERRAFORM_FILES = shutil.ignore_patterns(".terraform*")


def decoded(out: str) -> tuple[str | None, str]:
    """The document from the console's output; else why there is none."""
    try:
        text = base64.b64decode(out.strip('"')).decode("utf-8")
    except (binascii.Error, UnicodeDecodeError) as e:
        return None, str(e)
    if not text.startswith("#cloud-config"):
        # User data handed over base64-encoded (ESXi's guestinfo.userdata).
        try:
            text = base64.b64decode(text, validate=True).decode("utf-8")
        except (binascii.Error, UnicodeDecodeError):
            return None, "the value isn't a cloud-config document with the default inputs"
    return text, ""


def evaluate(module: Path, expression: str) -> tuple[str | None, str]:
    """The value of `expression` in the module's context; else why it has none. A value
    known only after apply is no value."""
    (module / EVALUATED).write_text(f"locals {{\n  isoloom_verify = {expression}\n}}\n", encoding="utf-8")
    done = run(["terraform", "console", "-input=false", "-no-color"], cwd=module, env=data_dir(module), stdin=SHOW)
    (module / EVALUATED).unlink()
    out = done.out.strip()
    if "(known after apply)" in out:
        return None, UNKNOWN
    if done.code != 0 or not (out.startswith('"') and out.endswith('"')):
        return None, " ".join(tail(done.both, 6).replace("│", " ").split()) or "no value"
    return decoded(out)


def project_of(module: Path) -> Path:
    """The project a module belongs to (the folder holding its `.isoloom/`), whose files its
    expressions read (`${path.module}/../..`)."""
    return next((p.parent for p in module.parents if OUTPUT_DIR.match(p.name)), module)


def rendered(ctx: Ctx, copy: Path, tf: Path, e: hcl.Embedded, init_done: Done) -> list[Result]:
    """One document, named by where it is (`resource.attribute`): evaluated, then checked."""
    where = f"{e.resource}.{e.attribute}"
    if init_done.code != 0:
        status = SKIP if "doesn't validate" in init_done.err else ERROR
        return [Result(ctx.rel(tf), "cloud-init", status, f"{where}: not evaluated: {tail(init_done.both, 4)}", "evaluate")]
    text, why = evaluate(copy, e.expression)
    if text is None:
        status = SKIP if why == UNKNOWN else ERROR
        return [Result(ctx.rel(tf), "cloud-init", status, f"{where}: not evaluated: {why}", "evaluate")]
    return cloud_init.check_text(ctx, tf, text, where)


def check(ctx: Ctx, module: Path, valid: bool) -> list[Result]:
    """The module's documents, evaluated with placeholder inputs and without the variables'
    validation rules (see hcl.evaluation_override), only in a module that validates; a
    document built from values known only after apply (a key Terraform generates) is skipped."""
    sources = {tf: tf.read_text(encoding="utf-8") for tf in sorted(module.glob("*.tf"))}
    found = [(tf, e) for tf, text in sources.items() for e in hcl.cloud_configs(text)]
    if not found:
        return []
    if not valid:
        done = Done(1, "", "the module doesn't validate")
        return [r for tf, e in found for r in rendered(ctx, module, tf, e, done)]
    project = project_of(module)
    with tempfile.TemporaryDirectory(prefix="evaluate-") as tmp:
        copy = Path(tmp) / project.name / module.relative_to(project)
        # Without Terraform's own files: other modules' `terraform init` writes them while this
        # copy is made (a lock file half written, a working folder going away), and the
        # evaluation's own `init` makes the ones it needs.
        shutil.copytree(project, Path(tmp) / project.name, symlinks=True, ignore=TERRAFORM_FILES)
        (copy / OVERRIDE).write_text(hcl.evaluation_override("\n".join(sources.values())), encoding="utf-8")
        for tf, text in sources.items():
            (copy / tf.name).write_text(hcl.without_validations(text), encoding="utf-8")
        done = init(ctx, copy, backend=True)
        results = [r for tf, e in found for r in rendered(ctx, copy, tf, e, done)]
        shutil.rmtree(data_dir(copy)["TF_DATA_DIR"], ignore_errors=True)
    return results
