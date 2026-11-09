"""The shell scripts other generated files hold, found by the markers their generators write
(see shell_embedded.py, which checks them):

- a Vagrantfile's shell provisioners: `inline: <<~'SH'` heredocs (and any other `<<~'SH'`, such
  as Hyper-V's address script) and `inline: "..."` strings, which Vagrant runs with `bash -l`;
- a Terraform module's provisioners: a `remote-exec`'s `inline` commands (one script, run with
  `/bin/sh`), unless its connection is WinRM or to Windows, and a `local-exec`'s `command`, unless
  its interpreter is another; and any string that starts with a shell's `#!` (user data, a file's
  content), outside the cloud-config documents, which are checked once Terraform has built them;
- a cloud-config document's `bootcmd` and `runcmd` (each one script, run with `/bin/sh`) and the
  `write_files` that are shell scripts (a `.sh` path, or a shell's `#!`).

Each script found also yields the shell scripts it writes with a quoted heredoc (`cat > x.sh
<<'EOF'`), checked as scripts of their own. A Terraform interpolation reads as `0` (hcl.py).
"""

from __future__ import annotations

import base64
import binascii
import gzip
import re
import shlex
from dataclasses import dataclass, replace

import hcl
import yaml

SHEBANG = re.compile(r"^#!\s*(?:/usr)?/bin/(?:env\s+)?(sh|bash|dash|ash|ksh)\b")
# shellcheck's dialects; `ash` reads as `sh`.
DIALECT = {"sh": "sh", "ash": "sh", "dash": "dash", "bash": "bash", "ksh": "ksh"}


@dataclass(frozen=True)
class Script:
    """A script: where it is in its file, the shell that runs it, and its text."""

    where: str
    shell: str
    text: str


def shell_of(text: str, default: str | None) -> str | None:
    """The shell a script's `#!` names; `default` without one; None for another interpreter."""
    if not text.startswith("#!"):
        return default
    m = SHEBANG.match(text)
    return DIALECT[m.group(1)] if m else None


# ---- Scripts inside scripts ------------------------------------------------------------------

# `... > path <<'MARK'` (or `<<-"MARK"`, `<<MARK`), then the body up to the line `MARK`.
WRITTEN = re.compile(r"""(?:>|\btee\s+(?:-a\s+)?)\s*(\S+)\s*<<(-?)\s*(['"]?)(\w+)\3[^\n]*\n""")


def written(script: Script) -> list[Script]:
    """The shell scripts `script` writes with a heredoc, each also searched in turn."""
    found = []
    for m in WRITTEN.finditer(script.text):
        path, strip, quoted, mark = m.group(1), m.group(2), m.group(3), m.group(4)
        end = re.compile(rf"^{'[ \t]*' if strip else ''}{re.escape(mark)}[ \t]*$", re.M).search(script.text, m.end())
        if not quoted or not end:
            continue
        body = script.text[m.end() : end.start()]
        if strip:
            body = re.sub(r"(?m)^\t+", "", body)
        shell = shell_of(body, "sh" if path.strip("'\"").endswith(".sh") else None)
        if shell:
            inner = Script(f"{script.where} > {path.strip(chr(39) + chr(34))}", shell, body)
            found += [inner, *written(inner)]
    return found


def with_written(scripts: list[Script]) -> list[Script]:
    """`scripts`, each followed by the scripts it writes."""
    return [s for script in scripts for s in (script, *written(script))]


# ---- Vagrantfiles ----------------------------------------------------------------------------

RUBY_HEREDOC = re.compile(r"<<([~-]?)'SH'")
RUBY_STRING = re.compile(r"""\binline:\s*(["'])""")
DEFINE = re.compile(r"""\bvm\.define\s+["']([^"']+)["']""")
NAME = re.compile(r"""\bname:\s*"([^"]*)\"""")
RUBY_ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "e": "\x1b", "s": " ", "0": "\0", "a": "\a", "b": "\b"}


def dedent(body: str) -> str:
    """A squiggly heredoc's text: the least indentation of its non-blank lines taken off."""
    lines = body.split("\n")
    indents = [len(line) - len(line.lstrip(" \t")) for line in lines if line.strip()]
    least = min(indents, default=0)
    return "\n".join(line[least:] for line in lines)


def ruby_string(text: str, i: int) -> str:
    """The Ruby string literal whose quote is at `i`, read: a double-quoted one's escapes, and
    each `#{...}` as `0` (a single-quoted one keeps all but `\\\\` and `\\'`)."""
    quote, out, i = text[i], [], i + 1
    while i < len(text) and text[i] != quote:
        c = text[i]
        if c == "\\" and quote == "'":
            nxt = text[i + 1 : i + 2]
            out.append(nxt if nxt in ("\\", "'") else c + nxt)
            i += 2
        elif c == "\\":
            nxt = text[i + 1 : i + 2]
            out.append(RUBY_ESCAPES.get(nxt, nxt))
            i += 2
        elif text.startswith("#{", i):
            out.append(hcl.PLACEHOLDER)
            i = hcl.skip(text, i + 1)
        else:
            out.append(c)
            i += 1
    return "".join(out)


def vagrant_where(text: str, at: int) -> str:
    """Where in a Vagrantfile the script at `at` is: its machine, and its provisioner's name
    (else the call the heredoc is given to)."""
    line_start = text.rfind("\n", 0, at) + 1
    line = text[line_start : text.find("\n", at)]
    machine = [m.group(1) for m in DEFINE.finditer(text, 0, at)]
    name = NAME.search(line)
    what = f'provision "{name.group(1)}"' if name else line.strip().split("(")[0].split(" ")[0]
    return f"{machine[-1]}: {what}" if machine else what


def windows_machines(text: str) -> list[tuple[int, int]]:
    """Where the Windows machines are defined (a `vm.define` block whose guest is Windows): their
    provisioners' scripts are PowerShell."""
    starts = [m.start() for m in DEFINE.finditer(text)] + [len(text)]
    blocks = zip(starts, starts[1:])
    return [(a, b) for a, b in blocks if re.search(r"\bvm\.guest\s*=\s*:windows\b", text[a:b])]


def vagrantfile(text: str) -> list[Script]:
    """A Vagrantfile's shell scripts: its `<<~'SH'` heredocs and `inline:` strings, outside the
    Windows machines."""
    windows = windows_machines(text)
    found = [s for at, s in vagrant_scripts(text) if not any(a <= at < b for a, b in windows)]
    return with_written(found)


def vagrant_scripts(text: str) -> list[tuple[int, Script]]:
    """Every `<<~'SH'` heredoc and `inline:` string, with where it starts."""
    found = []
    for m in RUBY_HEREDOC.finditer(text):
        start = text.find("\n", m.end()) + 1
        end = re.compile(r"^[ \t]*SH[ \t]*$" if m.group(1) else r"^SH$", re.M).search(text, start)
        if start <= 0 or not end:
            continue
        body = text[start : end.start()]
        body = dedent(body) if m.group(1) == "~" else body
        found.append((m.start(), Script(vagrant_where(text, m.start()), shell_of(body, "bash") or "bash", body)))
    for m in RUBY_STRING.finditer(text):
        body = ruby_string(text, m.end() - 1)
        found.append((m.start(), Script(vagrant_where(text, m.start()), shell_of(body, "bash") or "bash", body + "\n")))
    return found


# ---- Terraform modules -----------------------------------------------------------------------

PROVISIONER = re.compile(r'^[ \t]*provisioner[ \t]+"(remote-exec|local-exec)"[ \t]*\{', re.M)
WINDOWS = re.compile(r'^[ \t]*(?:type[ \t]*=[ \t]*"winrm"|target_platform[ \t]*=[ \t]*"windows")', re.M)


def attribute(block: str, name: str) -> tuple[int, int] | None:
    """The span of attribute `name`'s expression in `block` (its first one)."""
    m = re.search(rf"^[ \t]*{name}[ \t]*=(?!=)[ \t]*", block, re.M)
    return (m.end(), hcl.expression_end(block, m.end())) if m else None


def literal(expression: str) -> str | None:
    """The text of an expression that is one quoted string; None for anything else."""
    expression = expression.strip()
    if not expression.startswith('"'):
        return None
    text, end = hcl.read_string(expression, 0)
    return text if end == len(expression) else None


def remote_exec(resource: str, block: str, where: str) -> list[Script]:
    """A `remote-exec`'s `inline` commands as one `/bin/sh` script; nothing over WinRM or to
    Windows (the commands are cmd's), nor for a command that is an expression."""
    span = attribute(block, "inline")
    if not span or WINDOWS.search(block) or WINDOWS.search(resource):
        return []
    items = hcl.elements(block, span[0])
    commands = [c for c in (literal(item) for item in items) if c is not None]
    if not commands:
        return []
    text = "\n".join(commands) + "\n"
    return [Script(where, shell_of(text, "sh") or "sh", text)]


def local_exec(block: str, where: str) -> list[Script]:
    """A `local-exec`'s `command`, unless its interpreter is another than a POSIX shell."""
    span = attribute(block, "command")
    interpreter = attribute(block, "interpreter")
    if interpreter and not re.search(r'^\[\s*"(?:/bin/)?(?:sh|bash)"', block[interpreter[0] : interpreter[1]]):
        return []
    command = literal(block[span[0] : span[1]]) if span else None
    return [Script(where, "sh", command + "\n")] if command is not None else []


def resource_at(text: str, at: int) -> str:
    """The text of the resource the text at `at` is in."""
    starts = [m.start() for m in hcl.RESOURCE.finditer(text, 0, at)]
    if not starts:
        return ""
    brace = text.find("{", starts[-1])
    return text[starts[-1] : hcl.expression_end(text, brace)]


def provisioners(text: str) -> tuple[list[Script], list[tuple[int, int]]]:
    """The provisioners' scripts, and where the provisioners are."""
    found, spans = [], []
    for m in PROVISIONER.finditer(text):
        end = hcl.expression_end(text, m.end() - 1)
        block = text[m.end() : end]
        where = f"{hcl.enclosing(text, m.start())} {m.group(1)}"
        spans.append((m.start(), end))
        if m.group(1) == "remote-exec":
            found += remote_exec(resource_at(text, m.start()), block, where)
        else:
            found += local_exec(block, where)
    return found, spans


def shebang_strings(text: str, outside: list[tuple[int, int]]) -> list[Script]:
    """Every string that starts with a shell's `#!`, outside the spans given."""
    found = []
    for at, string in hcl.strings(text):
        shell = shell_of(string, None) if string.startswith("#!") else None
        if shell and not any(a <= at < b for a, b in outside):
            names = [m.group(1) for m in hcl.ATTRIBUTE.finditer(text, 0, at)]
            where = f"{hcl.enclosing(text, at)}.{names[-1] if names else 'value'}"
            found.append(Script(where, shell, string))
    return found


def module_file(text: str) -> list[Script]:
    """A Terraform file's shell scripts: its provisioners' and its `#!` strings, those inside
    a cloud-config document left to the document's own check."""
    found, spans = provisioners(text)
    found += shebang_strings(text, spans + hcl.cloud_config_spans(text))
    return with_written(found)


# ---- cloud-config documents ------------------------------------------------------------------


def command_line(command: object) -> str:
    """A `runcmd` or `bootcmd` entry as the line cloud-init writes: a string as it is, a list
    as its words quoted."""
    if isinstance(command, list):
        return shlex.join(str(word) for word in command)
    return str(command)


def decoded_content(content: str, encoding: object) -> str | None:
    """A `write_files` content as the script it holds, by its `encoding`. Plain (no encoding) is
    the content itself. A gzipped entry (`gz+b64`, `gzip`: the configuration carrier's script) is
    decoded. A plain-base64 entry (`b64`) is left alone: those are the project's own files, which
    the standalone `shellcheck` validator checks as files, so re-checking them here would only
    double-report. None when it isn't a script to check (plain base64, or bytes we can't read)."""
    enc = str(encoding or "").strip().lower().replace("application/", "")
    if not enc or enc in ("text/plain", "text"):
        return content
    tokens = enc.split("+")
    if not any(t in ("gz", "gzip", "x-gzip") for t in tokens):
        return None
    try:
        data = base64.b64decode(content, validate=True) if any(t in ("b64", "base64") for t in tokens) else content.encode("utf-8")
        return gzip.decompress(data).decode("utf-8")
    except (binascii.Error, OSError, UnicodeDecodeError):
        return None


def cloud_config(text: str, where: str = "") -> list[Script]:
    """A cloud-config document's shell: `bootcmd`, `runcmd`, and its shell `write_files`, those
    `gz+b64`/`b64`-encoded (a recipe's configure script) decoded first."""
    try:
        doc = yaml.safe_load(text)
    except yaml.YAMLError:
        return []
    if not isinstance(doc, dict):
        return []
    prefix = f"{where} " if where else ""
    found = []
    for key in ("bootcmd", "runcmd"):
        lines = [command_line(c) for c in doc.get(key) or []]
        if lines:
            found.append(Script(f"{prefix}{key}", "sh", "\n".join(lines) + "\n"))
    for f in doc.get("write_files") or []:
        if not isinstance(f, dict) or not isinstance(f.get("content"), str):
            continue
        content = decoded_content(f["content"], f.get("encoding"))
        if content is None:
            continue
        path = str(f.get("path", ""))
        shell = shell_of(content, "sh" if path.endswith(".sh") else None)
        if shell:
            found.append(Script(f"{prefix}write_files {path}", shell, content))
    return with_written(found)


def renamed(scripts: list[Script], where: str) -> list[Script]:
    """`scripts`, each said to be in `where`."""
    return [replace(s, where=f"{where} {s.where}") for s in scripts]
