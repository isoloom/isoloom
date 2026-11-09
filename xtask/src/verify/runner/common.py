"""What every validator shares: the result record, the run context and the process helper."""

from __future__ import annotations

import os
import re
import subprocess
import threading
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path

# A result's status. A file fails when its validator fails it (its own verdict, under its default
# settings); what the validator reports without failing the file is a warning.
PASS, WARN, FAIL, ERROR, SKIP = "pass", "warn", "fail", "error", "skip"

# One command's time limit: a validator that hangs is reported, not waited for.
TIMEOUT = 900

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


@dataclass(frozen=True)
class Result:
    """One line of results.jsonl.

    `rule` is what identifies the failure across runs (a check code, a resource, a summary),
    without line numbers, so the baseline survives edits elsewhere in the file.
    """

    file: str
    validator: str
    status: str
    message: str
    rule: str = ""


class SharedLock:
    """Many readers or one writer, the writer first: once it waits, new readers wait too."""

    def __init__(self) -> None:
        self.cond = threading.Condition()
        self.readers = 0
        self.writer = False
        self.waiting = 0

    @contextmanager
    def reading(self) -> Iterator[None]:
        """Held with other readers."""
        with self.cond:
            self.cond.wait_for(lambda: not self.writer and not self.waiting)
            self.readers += 1
        try:
            yield
        finally:
            with self.cond:
                self.readers -= 1
                self.cond.notify_all()

    @contextmanager
    def writing(self) -> Iterator[None]:
        """Held alone."""
        with self.cond:
            self.waiting += 1
            self.cond.wait_for(lambda: not self.writer and not self.readers)
            self.waiting -= 1
            self.writer = True
        try:
            yield
        finally:
            with self.cond:
                self.writer = False
                self.cond.notify_all()


@dataclass
class Ctx:
    """The run: the tree's root, the cache volume, and how much runs at once."""

    tree: Path
    cache: Path
    jobs: int
    terraform_slots: threading.Semaphore
    cache_lock: SharedLock = field(default_factory=SharedLock)

    def rel(self, path: Path) -> str:
        """`path` as results name it: relative to the tree, with `/` separators."""
        try:
            return path.relative_to(self.tree).as_posix()
        except ValueError:
            return path.as_posix()


@dataclass(frozen=True)
class Done:
    """A finished command: its exit code and its output, without terminal colours."""

    code: int
    out: str
    err: str

    @property
    def both(self) -> str:
        """Standard output and error together, trimmed."""
        return "\n".join(x for x in (self.out.strip(), self.err.strip()) if x)


def run(cmd: list[str], cwd: Path | None = None, env: dict[str, str] | None = None, stdin: str | None = None) -> Done:
    """Runs `cmd` to completion. A missing program or a timeout is exit code 127 or 124."""
    full_env = {**os.environ, **(env or {})}
    try:
        p = subprocess.run(
            cmd,
            cwd=cwd,
            env=full_env,
            input=stdin,
            capture_output=True,
            text=True,
            errors="replace",
            timeout=TIMEOUT,
            check=False,
        )
    except FileNotFoundError as e:
        return Done(127, "", str(e))
    except subprocess.TimeoutExpired:
        return Done(124, "", f"{cmd[0]} took longer than {TIMEOUT}s")
    return Done(p.returncode, ANSI.sub("", p.stdout), ANSI.sub("", p.stderr))


def tail(text: str, lines: int = 12) -> str:
    """The end of a long output, which is where tools put the reason."""
    kept = [line.rstrip() for line in text.strip().splitlines() if line.strip()]
    return "\n".join(kept[-lines:])


MISSING = re.compile(r"No such file|ENOENT|is missing|evalsymlink failure|no such file")


def cause(message: str, default: str) -> str:
    """A rule for a tool that has no codes: a missing file it was told to read, or `default`."""
    return "missing-file" if MISSING.search(message) else default


def passed(ctx: Ctx, path: Path, validator: str, message: str = "") -> Result:
    """A file the validator accepts."""
    return Result(ctx.rel(path), validator, PASS, message)
