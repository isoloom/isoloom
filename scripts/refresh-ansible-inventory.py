#!/usr/bin/env python3
"""Refreshes crates/isoloom-coverage/coverage/ansible-inventory.txt: every behavioral inventory
parameter (a host variable that changes how Ansible reaches or runs on the host), from the pinned
ansible-core sources: the magic variables of `constants.py`, and the `vars:` of the core
connection, become and shell plugins and of `config/base.yml`. The WinRM connection also passes
any `ansible_winrm_<argument>` through to pywinrm's `Protocol`: those come from its signature.

Usage: python3 scripts/refresh-ansible-inventory.py   (then `cargo test`: parameters with no row
fail). ISOLOOM_REFRESH=latest reads the newest releases instead of the pins.
"""
import ast
import json
import os
import pathlib
import re
import urllib.request

ANSIBLE = "v2.21.5"
PYWINRM = "v0.5.0"
DOCUMENTED = [
    "config/base.yml",
    "plugins/connection/ssh.py",
    "plugins/connection/winrm.py",
    "plugins/connection/psrp.py",
    "plugins/connection/local.py",
    "plugins/become/sudo.py",
    "plugins/become/su.py",
    "plugins/become/runas.py",
    "plugins/doc_fragments/shell_common.py",
    "plugins/doc_fragments/shell_windows.py",
    "plugins/doc_fragments/connection_pipelining.py",
]
# Protocol arguments the WinRM plugin sets from its own options.
WINRM_OWN = {"endpoint", "username", "password", "transport"}
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/isoloom-coverage/coverage/ansible-inventory.txt"


def get(url: str) -> str:
    with urllib.request.urlopen(url) as r:
        return r.read().decode()


def latest(repo: str, pin: str) -> str:
    if os.environ.get("ISOLOOM_REFRESH") != "latest":
        return pin
    return json.loads(get(f"https://api.github.com/repos/{repo}/releases/latest"))["tag_name"]


def magic_variables(constants: str) -> set[str]:
    start = constants.index("MAGIC_VARIABLE_MAPPING = dict(")
    body = constants[start : constants.index("\n)", start)]
    return set(re.findall(r"'(ansible_[a-z0-9_]+)'", body))


def winrm_passthrough(protocol: str) -> set[str]:
    for node in ast.walk(ast.parse(protocol)):
        if isinstance(node, ast.ClassDef) and node.name == "Protocol":
            init = next(f for f in node.body if isinstance(f, ast.FunctionDef) and f.name == "__init__")
            return {f"ansible_winrm_{a.arg}" for a in init.args.args[1:] if a.arg not in WINRM_OWN}
    raise SystemExit("no Protocol class in pywinrm")


def main() -> None:
    ansible = latest("ansible/ansible", ANSIBLE)
    pywinrm = latest("diyan/pywinrm", PYWINRM)
    core = f"https://raw.githubusercontent.com/ansible/ansible/{ansible}/lib/ansible/"
    names = magic_variables(get(core + "constants.py"))
    for path in DOCUMENTED:
        names |= set(re.findall(r"name: (ansible_[a-z0-9_]+)\b", get(core + path)))
    names |= winrm_passthrough(get(f"https://raw.githubusercontent.com/diyan/pywinrm/{pywinrm}/winrm/protocol.py"))
    header = f"# Behavioral inventory parameters of ansible-core {ansible} (and pywinrm {pywinrm}'s Protocol)\n"
    OUT.write_text(header + "\n".join(sorted(names)) + "\n")
    print(f"ansible-core {ansible}: {len(names)} parameters")


if __name__ == "__main__":
    main()
