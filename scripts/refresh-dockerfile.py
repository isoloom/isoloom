#!/usr/bin/env python3
"""Refreshes crates/isoloom-coverage/coverage/dockerfile.txt: every Dockerfile instruction, each
instruction's flags and the parser directives, from the pinned BuildKit Dockerfile frontend.

Usage: python3 scripts/refresh-dockerfile.py   (then `cargo test`: entries with no row fail)
ISOLOOM_REFRESH=latest reads the newest `dockerfile/1.x` frontend release instead of the pin.
"""
import json
import os
import pathlib
import re
import urllib.request

PIN = "dockerfile/1.28.0"
# `RUN`'s flags are registered by hooks, one file each.
RUN_FLAGS = ["runmount", "runnetwork", "runsecurity", "rundevice"]
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/isoloom-coverage/coverage/dockerfile.txt"
FLAG = re.compile(r'flags\.Add(?:String|Bool|Strings)\("([a-z-]+)"')


def get(url: str) -> str:
    with urllib.request.urlopen(url) as r:
        return r.read().decode()


def version() -> str:
    if os.environ.get("ISOLOOM_REFRESH") != "latest":
        return PIN
    refs = json.loads(get("https://api.github.com/repos/moby/buildkit/git/matching-refs/tags/dockerfile/1."))
    stable = [r["ref"].removeprefix("refs/tags/") for r in refs if re.fullmatch(r"refs/tags/dockerfile/1\.\d+\.\d+", r["ref"])]
    return max(stable, key=lambda t: tuple(int(x) for x in t.split("/")[1].split(".")))


def main() -> None:
    v = version()
    base = f"https://raw.githubusercontent.com/moby/buildkit/{v}/frontend/dockerfile/"
    commands = get(base + "command/command.go")
    entries = {w.upper() for w in re.findall(r'^\t\w+\s+= "(\w+)"', commands, re.M)}
    parse = get(base + "instructions/parse.go")
    for func in re.finditer(r"^func parse(\w+)\(req parseRequest\).*?^}", parse, re.M | re.S):
        word = func.group(1).upper()
        entries |= {f"{word} --{f}" for f in FLAG.findall(func.group(0))}
    for hook in RUN_FLAGS:
        entries |= {f"RUN --{f}" for f in FLAG.findall(get(base + f"instructions/commands_{hook}.go"))}
    directives = get(base + "parser/directives.go")
    entries |= {f"directive {d}" for d in re.findall(r'^\tkey\w+\s+= "(\w+)"', directives, re.M)}
    unknown = {e.split(" --")[0] for e in entries if " --" in e} - entries
    if unknown:
        raise SystemExit(f"flags of unknown instructions: {unknown}")
    OUT.write_text(f"# Dockerfile instructions, flags and parser directives of BuildKit {v}\n" + "\n".join(sorted(entries)) + "\n")
    print(f"{v}: {len(entries)} entries")


if __name__ == "__main__":
    main()
