"""The verify runner: inside the toolchain image, each generated file through its validator.

    python3 runner --tree DIR --out DIR --baseline FILE [--update]

scripts/verify.sh starts it; it is not meant to run outside the image. It writes into --out:
results.jsonl (a line per file and validator: file, validator, status, message, rule, and for
a failure whether the baseline knows it), new.jsonl (the failures it doesn't), stale.jsonl
(baseline entries nothing matched), summary.txt, and with --update, baseline.json.

Exit status: 0 when every failure is known, NEW_FAILURES when some aren't.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
from collections import Counter
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import replace
from pathlib import Path

import baseline
import cloud_init
import infra
import kubernetes
import scripts
import terraform
import tfcli
from common import ERROR, PASS, SKIP, WARN, Ctx, Result
from discover import Found, discover

NEW_FAILURES = 3
COLUMNS = (PASS, WARN, "known", "new", SKIP)

# A job: what it checks (for a runner error), and the call that checks it.
Job = tuple[Path, str, Callable[[], list[Result]]]


def chunks(files: list[Path], size: int) -> list[list[Path]]:
    """Batches, so a batch tool (shellcheck) still runs on several cores."""
    return [files[i : i + size] for i in range(0, len(files), size)]


def jobs(ctx: Ctx, found: Found) -> list[Job]:
    """One job per file, module or batch."""
    t = ctx.tree
    work: list[Job] = []
    work += [(p, "terraform", lambda p=p: terraform.module(ctx, p)) for p in found.modules]
    work += [(p, "vagrant", lambda p=p: infra.vagrant(ctx, p)) for p in found.vagrantfiles]
    work += [(p, "compose", lambda p=p: infra.compose(ctx, p)) for p in found.compose]
    work += [(t, "kubeconform", lambda: kubernetes.check(ctx, found.manifests, found.charts, found.kustomizations))]
    work += [(p, "cloud-init", lambda p=p: cloud_init.check_file(ctx, p)) for p in found.cloud_configs]
    work += [(p, "ansible-inventory", lambda p=p: infra.inventory(ctx, p)) for p in found.inventories]
    work += [(p, "ansible-lint", lambda p=p: infra.playbook(ctx, p)) for p in found.playbooks]
    work += [(c[0], "shellcheck", lambda c=c: scripts.shellcheck(ctx, c)) for c in chunks(found.shell, 40)]
    work += [(t, "hadolint", lambda: scripts.hadolint(ctx, found.dockerfiles))]
    work += [(t, "psscriptanalyzer", lambda: scripts.psscriptanalyzer(ctx, found.powershell))]
    work += [(t, "none", lambda: [Result(ctx.rel(f), "none", SKIP, "no validator reads this format") for f in found.unknown])]
    return work


def attempt(ctx: Ctx, job: Job) -> list[Result]:
    """A job's results; a bug of the runner's is an error result, not the end of the run."""
    path, validator, call = job
    try:
        return call()
    except Exception as e:  # noqa: BLE001 - reported as the job's result
        return [Result(ctx.rel(path), validator, ERROR, f"the runner failed: {e!r}", "runner")]


def execute(ctx: Ctx, work: list[Job]) -> list[Result]:
    """Runs the jobs, `ctx.jobs` at a time, with a progress line every tenth of the way."""
    results: list[Result] = []
    step = max(1, len(work) // 10)
    with ThreadPoolExecutor(max_workers=ctx.jobs) as pool:
        futures = [pool.submit(attempt, ctx, job) for job in work]
        for n, future in enumerate(as_completed(futures), 1):
            # Messages name files as the results do: from the tree, not from /work.
            results += [replace(r, message=r.message.replace(f"{ctx.tree}/", "")) for r in future.result()]
            if n % step == 0 or n == len(work):
                print(f"verify: {n}/{len(work)} jobs done", file=sys.stderr, flush=True)
    return sorted(set(results), key=lambda r: (r.file, r.validator, r.rule, r.status, r.message))


def table(results: list[Result], new: set[baseline.Key]) -> list[str]:
    """Counts per validator: passed, warned, failed and known, failed and new, skipped."""
    counts: dict[str, Counter] = {}
    for r in results:
        column = r.status if r.status not in baseline.FAILING else ("new" if baseline.key(r) in new else "known")
        counts.setdefault(r.validator, Counter())[column] += 1
    rows = [f"{'validator':<18}" + "".join(f"{c:>8}" for c in COLUMNS)]
    for validator in sorted(counts):
        rows.append(f"{validator:<18}" + "".join(f"{counts[validator][c]:>8}" for c in COLUMNS))
    return rows


def summary(found: Found, results: list[Result], new: list[Result], stale: list[dict], known: dict) -> str:
    """summary.txt: what ran, the counts, every new failure, and the baseline's state."""
    unexplained = baseline.unexplained(known)
    lines = [f"image: {os.environ.get('VERIFY_IMAGE') or 'unknown'}"]
    lines.append(f"checked: {found.checked()} files and folders; {len(found.unknown)} files no validator reads")
    lines += table(results, {baseline.key(r) for r in new}) + [""]
    lines.append(f"new failures: {len({baseline.key(r) for r in new})}")
    lines += [f"  {r.file}: {r.validator} {r.status}: {r.message.splitlines()[0] if r.message else r.rule}" for r in new]
    if stale:
        lines.append(f"stale baseline entries (no longer failing, drop them with --update-baseline): {len(stale)}")
    if unexplained:
        lines.append(f"baseline causes without a reason (tools/verify/baseline.json): {unexplained}")
    return "\n".join(lines) + "\n"


def write(out: Path, results: list[Result], new: list[Result], stale: list[dict]) -> None:
    """results.jsonl, new.jsonl and stale.jsonl."""
    new_keys = {baseline.key(r) for r in new}
    with (out / "results.jsonl").open("w", encoding="utf-8") as f:
        for r in results:
            known = (baseline.key(r) not in new_keys) if r.status in baseline.FAILING else None
            f.write(baseline.line(r, known) + "\n")
    (out / "new.jsonl").write_text("".join(baseline.line(r, False) + "\n" for r in new), encoding="utf-8")
    (out / "stale.jsonl").write_text("".join(json.dumps(e) + "\n" for e in stale), encoding="utf-8")


def main() -> int:
    """Discover, validate, compare, write."""
    parser = argparse.ArgumentParser(prog="runner")
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--update", action="store_true")
    args = parser.parse_args()
    cpus = os.cpu_count() or 2
    jobs_n = int(os.environ.get("VERIFY_JOBS") or cpus)
    slots = threading.Semaphore(int(os.environ.get("VERIFY_TERRAFORM_JOBS") or max(1, jobs_n // 2)))
    ctx = Ctx(args.tree.resolve(), Path("/cache"), jobs_n, slots)
    tfcli.cli_configs(ctx)
    found = discover(ctx.tree)
    results = execute(ctx, jobs(ctx, found))
    known = baseline.load(args.baseline)
    image = os.environ.get("VERIFY_IMAGE", "")
    if args.update:
        tag, _, image_id = image.partition(" ")
        known = baseline.updated(results, known, {"tag": tag, "id": image_id})
        (args.out / "baseline.json").write_text(baseline.dump(known), encoding="utf-8")
    new, stale = baseline.compare(results, known)
    write(args.out, results, new, stale)
    text = summary(found, results, new, stale, known)
    (args.out / "summary.txt").write_text(text, encoding="utf-8")
    return NEW_FAILURES if new else 0


if __name__ == "__main__":
    sys.exit(main())
