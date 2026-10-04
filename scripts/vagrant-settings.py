#!/usr/bin/env python3
"""Refreshes crates/isoloom-core/coverage/vagrant/*.txt: the settings each Vagrant config class
accepts, extracted from the pinned upstream sources (names only; the sources aren't vendored).

Usage: python3 scripts/vagrant-settings.py   (then `cargo test`: unclassified settings fail)
"""
import pathlib
import re
import urllib.request

SOURCES = {
    "vm": "https://raw.githubusercontent.com/hashicorp/vagrant/v2.4.9/plugins/kernel_v2/config/vm.rb",
    "virtualbox": "https://raw.githubusercontent.com/hashicorp/vagrant/v2.4.9/plugins/providers/virtualbox/config.rb",
    "hyperv": "https://raw.githubusercontent.com/hashicorp/vagrant/v2.4.9/plugins/providers/hyperv/config.rb",
    "vmware_desktop": "https://raw.githubusercontent.com/hashicorp/vagrant-vmware-desktop/desktop-v3.0.5/lib/vagrant-vmware-desktop/config.rb",
    "parallels": "https://raw.githubusercontent.com/Parallels/vagrant-parallels/v2.4.7/lib/vagrant-parallels/config.rb",
    "libvirt": "https://raw.githubusercontent.com/vagrant-libvirt/vagrant-libvirt/0.12.2/lib/vagrant-libvirt/config.rb",
    # Apple Silicon Macs (besides VMware Fusion and Parallels above).
    "utm": "https://raw.githubusercontent.com/naveenrajm7/vagrant_utm/v0.1.6/lib/vagrant_utm/config.rb",
    "qemu": "https://raw.githubusercontent.com/ppggff/vagrant-qemu/v0.6.3/lib/vagrant-qemu/config.rb",
}
# Methods of the config classes that aren't settings.
NOT_SETTINGS = {"merge", "validate", "finalize!", "to_s", "get_provider_config", "get_provider_overrides"}
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/isoloom-core/coverage/vagrant"


def settings(ruby: str) -> list[str]:
    found = []
    for line in ruby.splitlines():
        line = line.split("#", 1)[0]
        m = re.match(r"\s*attr_(?:accessor|reader)\s+(.+)", line)
        if m:
            found += [a.strip().lstrip(":") for a in m.group(1).split(",") if a.strip().startswith(":")]
            continue
        m = re.match(r"\s*def (\w+)(=)?\(", line)
        if m and not m.group(1).startswith(("_", "validate_", "resolve_")) and m.group(1) not in NOT_SETTINGS:
            found.append(m.group(1))
    return sorted(set(found))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, url in SOURCES.items():
        ruby = urllib.request.urlopen(url).read().decode()
        names = settings(ruby)
        (OUT / f"{name}.txt").write_text(f"# Settings of {url}\n" + "\n".join(names) + "\n")
        print(f"{name}: {len(names)} settings")


if __name__ == "__main__":
    main()
