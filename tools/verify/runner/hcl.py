"""Just enough HCL reading to evaluate the cloud-config documents a Terraform module builds
(see embedded.py): each such attribute's expression, whole; the module's variables, for
placeholder inputs; and the module without its variables' validation rules. And to find the
shell scripts a module holds (see shell_find.py): its strings, read, and a list's items.

The scanner knows strings, interpolations, heredocs, comments and brackets, which is what it
takes to find where an expression ends in the generated modules.
"""

from __future__ import annotations

import re
from collections.abc import Iterator
from dataclasses import dataclass

ATTRIBUTE = re.compile(r"^[ \t]*([A-Za-z_][\w-]*)[ \t]*=(?!=)[ \t]*", re.M)
RESOURCE = re.compile(r'^[ \t]*(resource|data)[ \t]+"([^"]+)"[ \t]+"([^"]+)"', re.M)
HEREDOC = re.compile(r"<<-?([A-Za-z_]\w*)[ \t]*\n")


@dataclass(frozen=True)
class Embedded:
    """A cloud-config expression: the resource it sits in, its attribute, and its text."""

    resource: str
    attribute: str
    expression: str


def skip_heredoc(text: str, i: int) -> int:
    """From `<<` at `i`, the index just past the heredoc's closing marker."""
    m = HEREDOC.match(text, i)
    if not m:
        return i + 2
    end = re.compile(rf"^[ \t]*{re.escape(m.group(1))}[ \t]*$", re.M).search(text, m.end())
    return end.end() if end else len(text)


# What an interpolation reads as, in a string's text: the value is known only when Terraform
# runs, and a number fits wherever shell takes a word (a path, an argument, an arithmetic term).
PLACEHOLDER = "0"
ESCAPES = {"n": "\n", "r": "\r", "t": "\t", '"': '"', "\\": "\\"}


def skip_comment(text: str, i: int) -> int:
    """From a `#` or `//` comment at `i`, the index of the end of its line."""
    end = text.find("\n", i)
    return len(text) if end < 0 else end


def skip(text: str, i: int) -> int:
    """The index just past what starts at `i`: a quoted string, a bracketed group (with all it
    holds), a heredoc or a comment; else past the one character."""
    c = text[i]
    if c == '"':
        return read_string(text, i)[1]
    if c in "([{":
        j = i + 1
        while j < len(text) and text[j] not in ")]}":
            j = skip(text, j)
        return j + 1
    if text.startswith("<<", i):
        return skip_heredoc(text, i)
    if text.startswith("/*", i):
        end = text.find("*/", i + 2)
        return len(text) if end < 0 else end + 2
    if c == "#" or text.startswith("//", i):
        return skip_comment(text, i)
    return i + 1


def escape(text: str, i: int) -> tuple[str, int]:
    """The character the escape at `i` (a backslash) stands for, and the index past it."""
    kind = text[i + 1 : i + 2]
    if kind in ("u", "U"):
        width = 4 if kind == "u" else 8
        digits = text[i + 2 : i + 2 + width]
        try:
            return chr(int(digits, 16)), i + 2 + width
        except ValueError:
            return digits, i + 2 + width
    return ESCAPES.get(kind, kind), i + 2


def read_string(text: str, i: int) -> tuple[str, int]:
    """The quoted string whose opening quote is at `i`: its text, escapes read, each
    interpolation as [`PLACEHOLDER`] and each directive (`%{ if }`) left out; and the index
    past its closing quote."""
    out: list[str] = []
    i += 1
    while i < len(text):
        c = text[i]
        if c == '"':
            return "".join(out), i + 1
        if c == "\\":
            char, i = escape(text, i)
            out.append(char)
        elif text.startswith(("$${", "%%{"), i):
            out.append(text[i + 1 : i + 3])
            i += 3
        elif text.startswith(("${", "%{"), i):
            out.append(PLACEHOLDER if c == "$" else "")
            i = skip(text, i + 1)
        else:
            out.append(c)
            i += 1
    return "".join(out), len(text)


def expression_end(text: str, i: int) -> int:
    """The end of the expression starting at `i`: the first newline outside any bracket,
    string, interpolation or heredoc."""
    while i < len(text) and text[i] != "\n":
        i = skip(text, i)
    return min(i, len(text))


def enclosing(text: str, at: int) -> str:
    """The address of the resource (or data source) the text at `at` belongs to."""
    last = ""
    for m in RESOURCE.finditer(text, 0, at):
        last = f"{'data.' if m.group(1) == 'data' else ''}{m.group(2)}.{m.group(3)}"
    return last or "module"


def config_spans(text: str) -> list[tuple[int, int, str]]:
    """Each attribute (or object key) whose expression holds a cloud-config document, the
    innermost one: where it starts and ends, and its name."""
    spans: list[tuple[int, int, str]] = []
    for m in ATTRIBUTE.finditer(text):
        end = expression_end(text, m.end())
        if "#cloud-config" in text[m.end() : end]:
            spans.append((m.start(), end, m.group(1)))
    return [s for s in spans if not any(o != s and s[0] < o[0] and o[1] <= s[1] for o in spans)]


def cloud_configs(text: str) -> list[Embedded]:
    """Every attribute (or object key) whose expression holds a cloud-config document: the
    innermost one, so `guestinfo = { userdata = base64encode(...) }` yields `userdata`."""
    return [Embedded(enclosing(text, a), name, text[text.index("=", a) + 1 : b].strip()) for a, b, name in config_spans(text)]


def cloud_config_spans(text: str) -> list[tuple[int, int]]:
    """Where the cloud-config documents' expressions are."""
    return [(a, b) for a, b, _ in config_spans(text)]


def strings(text: str) -> Iterator[tuple[int, str]]:
    """Every quoted string (not those inside another's interpolation), read: where it starts,
    and its text."""
    i = 0
    while i < len(text):
        c = text[i]
        if c == '"':
            string, end = read_string(text, i)
            yield i, string
            i = end
        elif text.startswith(("<<", "/*"), i) or c == "#" or text.startswith("//", i):
            i = skip(text, i)
        else:
            i += 1


def elements(text: str, i: int) -> list[str]:
    """The items of the list whose `[` is the first thing from `i`, each as written."""
    while i < len(text) and text[i] in " \t\n":
        i += 1
    if i >= len(text) or text[i] != "[":
        return []
    items, start, i = [], i + 1, i + 1
    while i < len(text) and text[i] != "]":
        if text[i] == ",":
            items.append(text[start:i])
            start = i + 1
            i += 1
        else:
            i = skip(text, i)
    items.append(text[start:i])
    return [x.strip() for x in items if x.strip()]


VARIABLE = re.compile(r'^variable[ \t]+"([^"]+)"[ \t]*\{', re.M)
PLACEHOLDERS = (("number", "0"), ("bool", "false"), ("list", "[]"), ("set", "[]"), ("tuple", "[]"), ("map", "{}"), ("object", "{}"))


@dataclass(frozen=True)
class Variable:
    """A module input: its name, a placeholder value of its type, and whether it has a default."""

    name: str
    placeholder: str
    has_default: bool


def variables(text: str) -> list[Variable]:
    """The module's `variable` blocks."""
    found = []
    for m in VARIABLE.finditer(text):
        block = text[m.end() : expression_end(text, m.end() - 1)]
        kind = re.search(r"^[ \t]*type[ \t]*=[ \t]*(\w+)", block, re.M)
        placeholder = next((v for t, v in PLACEHOLDERS if kind and kind.group(1) == t), '"isoloom-verify"')
        has_default = re.search(r"^[ \t]*default[ \t]*=", block, re.M) is not None
        found.append(Variable(m.group(1), placeholder, has_default))
    return found


def evaluation_override(text: str) -> str:
    """An override file for evaluating the module's expressions with no inputs given: a
    placeholder for each variable without a default."""
    return "\n".join(f'variable "{v.name}" {{\n  default = {v.placeholder}\n}}\n' for v in variables(text) if not v.has_default)


VALIDATION = re.compile(r"^[ \t]*validation[ \t]*\{", re.M)


def without_validations(text: str) -> str:
    """The module without its variables' validation rules, which check the inputs a user
    must give (an image per OS), not the documents, and which an override can't replace."""
    out, at = [], 0
    for m in VALIDATION.finditer(text):
        if m.start() < at:
            continue
        out.append(text[at : m.start()])
        at = expression_end(text, m.end() - 1)
    return "".join(out) + text[at:]
