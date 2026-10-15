"""Just enough HCL reading to evaluate the cloud-config documents a Terraform module builds
(see embedded.py): each such attribute's expression, whole; the module's variables, for
placeholder inputs; and the module without its variables' validation rules.

The scanner knows strings, interpolations, heredocs, comments and brackets, which is what it
takes to find where an expression ends in the generated modules.
"""

from __future__ import annotations

import re
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


STRING = -1  # a quoted string on the scanner's stack; an interpolation is its bracket depth


def string_step(text: str, i: int, stack: list[int], depth: int) -> tuple[int, int]:
    """One step inside a quoted string: the next index and bracket depth. An interpolation
    pushes the depth it closes at; the closing quote pops the string."""
    if text[i] == "\\":
        return i + 2, depth
    if text.startswith(("$${", "%%{"), i):
        return i + 3, depth
    if text.startswith(("${", "%{"), i):
        stack.append(depth)
        return i + 2, depth + 1
    if text[i] == '"':
        stack.pop()
    return i + 1, depth


def skip_comment(text: str, i: int) -> int:
    """From a `#` or `//` comment at `i`, the index of the end of its line."""
    end = text.find("\n", i)
    return len(text) if end < 0 else end


def expression_end(text: str, i: int) -> int:
    """The end of the expression starting at `i`: the first newline outside any bracket,
    string, interpolation or heredoc."""
    depth = 0
    stack: list[int] = []
    while i < len(text):
        c = text[i]
        if stack and stack[-1] == STRING:
            i, depth = string_step(text, i, stack, depth)
            continue
        if c == "\n" and depth <= 0 and not stack:
            return i
        if c == '"':
            stack.append(STRING)
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if c == "}" and stack and stack[-1] == depth:
                stack.pop()
        elif text.startswith("<<", i):
            i = skip_heredoc(text, i) - 1
        elif c == "#" or text.startswith("//", i):
            i = skip_comment(text, i) - 1
        i += 1
    return len(text)


def enclosing(text: str, at: int) -> str:
    """The address of the resource (or data source) the text at `at` belongs to."""
    last = ""
    for m in RESOURCE.finditer(text, 0, at):
        last = f"{'data.' if m.group(1) == 'data' else ''}{m.group(2)}.{m.group(3)}"
    return last or "module"


def cloud_configs(text: str) -> list[Embedded]:
    """Every attribute (or object key) whose expression holds a cloud-config document: the
    innermost one, so `guestinfo = { userdata = base64encode(...) }` yields `userdata`."""
    spans: list[tuple[int, int, str]] = []
    for m in ATTRIBUTE.finditer(text):
        end = expression_end(text, m.end())
        if "#cloud-config" in text[m.end() : end]:
            spans.append((m.start(), end, m.group(1)))
    innermost = [s for s in spans if not any(o != s and s[0] < o[0] and o[1] <= s[1] for o in spans)]
    return [Embedded(enclosing(text, a), name, text[text.index("=", a) + 1 : b].strip()) for a, b, name in innermost]


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
