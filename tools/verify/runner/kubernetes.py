"""Kubernetes: manifests, Helm charts and kustomizations, every resource checked by kubeconform.

Helm renders each chart (`helm template`) and kustomize builds each kustomization; what they
produce is validated with the plain manifests, in one kubeconform run, and each failure names
the file it comes from (the template, the kustomization). The schemas are Kubernetes'
own, at a pinned version and commit, downloaded once into the cache volume.
"""

from __future__ import annotations

import json
import re
import tempfile
from pathlib import Path

from common import ERROR, FAIL, SKIP, WARN, Ctx, Result, cause, passed, run, tail

KUBERNETES_VERSION = "1.37.1"
SCHEMAS = (
    "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/817113f21a2643311cbc3e992bb44ffe1c416c54/"
    "{{.NormalizedKubernetesVersion}}-standalone{{.StrictSuffix}}/{{.ResourceKind}}{{.KindSuffix}}.json"
)
LINT = re.compile(r"^\[(INFO|WARNING|ERROR)\] ([^:]*): (.*)$")
SOURCE = re.compile(r"^# Source: [^/]+/(.+)$", re.M)
DIGITS = re.compile(r"\d+")


def lint(ctx: Ctx, chart: Path) -> list[Result]:
    """`helm lint`: errors fail the chart; warnings and notes don't (Helm's own verdict)."""
    done = run(["helm", "lint", str(chart)])
    results = []
    for line in done.both.splitlines():
        m = LINT.match(line.strip())
        if m:
            status = FAIL if m.group(1) == "ERROR" else WARN
            where = chart / m.group(2).rstrip("/") if m.group(2) else chart
            rule = DIGITS.sub("#", m.group(3))[:120]
            results.append(Result(ctx.rel(where), "helm-lint", status, m.group(3), rule))
    if done.code != 0 and not any(r.status == FAIL for r in results):
        results.append(Result(ctx.rel(chart), "helm-lint", ERROR, tail(done.both), "lint"))
    return results or [passed(ctx, chart, "helm-lint")]


def render(ctx: Ctx, chart: Path, into: Path, origin: dict[Path, Path]) -> list[Result]:
    """`helm template`, each template's output in its own file under `into`, so kubeconform's
    findings name the template."""
    done = run(["helm", "template", "verify", str(chart)])
    if done.code != 0:
        return [Result(ctx.rel(chart / "Chart.yaml"), "helm-template", FAIL, tail(done.both), cause(done.both, "render"))]
    parts = SOURCE.split(done.out)
    for source, body in zip(parts[1::2], parts[2::2], strict=True):
        out = into / ctx.rel(chart) / source
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("a", encoding="utf-8") as f:
            f.write("---\n" + body.strip().removesuffix("---").strip() + "\n")
        origin[out] = chart / source
    return [passed(ctx, chart / "Chart.yaml", "helm-template")]


def build(ctx: Ctx, folder: Path, into: Path, origin: dict[Path, Path]) -> list[Result]:
    """`kustomize build`, its output kept for kubeconform under the kustomization's name."""
    shown = next((folder / n for n in ("kustomization.yaml", "kustomization.yml", "Kustomization") if (folder / n).exists()), folder)
    done = run(["kustomize", "build", str(folder)])
    if done.code != 0:
        # kustomize's default load restrictor refuses files outside the kustomization's folder.
        rule = "load-restrictor" if "is not in or below" in done.both else cause(done.both, "build")
        return [Result(ctx.rel(shown), "kustomize", FAIL, tail(done.both), rule)]
    out = into / ctx.rel(shown)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(done.out, encoding="utf-8")
    origin[out] = shown
    return [passed(ctx, shown, "kustomize")]


def resource(ctx: Ctx, shown: Path, r: dict) -> Result:
    """One resource kubeconform didn't accept."""
    what = f"{r.get('kind', '')}/{r.get('name', '')}".strip("/") or "document"
    errors = "; ".join(f"{e.get('path', '')}: {e.get('msg', '')}" for e in r.get("validationErrors") or [])
    message = f"{what}: {errors or r.get('msg', '')}"
    if r.get("status") == "statusInvalid":
        return Result(ctx.rel(shown), "kubeconform", FAIL, message, what)
    msg = r.get("msg", "")
    if "could not find schema" in msg:
        return Result(ctx.rel(shown), "kubeconform", SKIP, message, what)
    status = ERROR if "download" in msg or "http" in msg.lower() else FAIL
    return Result(ctx.rel(shown), "kubeconform", status, message, what)


def kubeconform(ctx: Ctx, files: list[Path], origin: dict[Path, Path]) -> list[Result]:
    """One kubeconform run over every manifest, in strict mode (unknown fields are errors)."""
    if not files:
        return []
    cache = ctx.cache / "kubeconform"
    cache.mkdir(parents=True, exist_ok=True)
    args = ["kubeconform", "-strict", "-verbose", "-output", "json", "-kubernetes-version", KUBERNETES_VERSION]
    args += ["-schema-location", SCHEMAS, "-cache", str(cache), "-n", str(ctx.jobs)]
    done = run(args + [str(f) for f in files])
    try:
        report = json.loads(done.out)
    except json.JSONDecodeError:
        return [Result(ctx.rel(origin.get(f, f)), "kubeconform", ERROR, tail(done.both), "run") for f in files]
    by_file: dict[Path, list[Result]] = {}
    for r in report.get("resources") or []:
        path = Path(r.get("filename", ""))
        shown = origin.get(path, path)
        # An empty document (between two `---`) comes back with no status at all.
        if r.get("status") in ("statusValid", "statusEmpty", ""):
            by_file.setdefault(shown, [])
        else:
            by_file.setdefault(shown, []).append(resource(ctx, shown, r))
    return [x for shown, found in sorted(by_file.items()) for x in (found or [passed(ctx, shown, "kubeconform")])]


def check(ctx: Ctx, manifests: list[Path], charts: list[Path], kustomizations: list[Path]) -> list[Result]:
    """Every Kubernetes file: charts linted and rendered, kustomizations built, then all the
    resources validated together."""
    results: list[Result] = []
    origin: dict[Path, Path] = {}
    with tempfile.TemporaryDirectory() as tmp:
        into = Path(tmp)
        for chart in charts:
            results += lint(ctx, chart) + render(ctx, chart, into / "helm", origin)
        for folder in kustomizations:
            results += build(ctx, folder, into / "kustomize", origin)
        results += kubeconform(ctx, manifests + sorted(origin), origin)
    return results
