#!/usr/bin/env python3
"""Refreshes crates/isoloom-coverage/coverage/vagrant/*.txt: every setting and option Vagrant and
its provider plugins accept, extracted from the pinned upstream sources (names only; the sources
aren't vendored).

- The provider and `config.vm` lists come from each config class (`attr_accessor` and setter
  methods), one setting per line.
- `communicators.txt`, `provisioners.txt` and `push.txt` merge each config class with the classes
  it inherits from, one `<kind> <option>` per line (`*` for the options every provisioner takes).
- `networks.txt` and `folders.txt` come from the documentation pages and the provider code that
  reads the options, because Vagrant has no config class for them.

Usage: python3 scripts/refresh-vagrant.py   (then `cargo test`: unclassified settings fail)
With ISOLOOM_REFRESH=latest, each repository's newest release tag replaces its pin.
"""
import json
import os
import pathlib
import re
import sys
import urllib.request

# Each repository, its pinned tag, and the pattern its release tags follow (for `latest`).
REPOS = {
    "vagrant": ("hashicorp/vagrant", "v2.4.9", r"v(\d+)\.(\d+)\.(\d+)"),
    "vmware_desktop": ("hashicorp/vagrant-vmware-desktop", "desktop-v3.0.5", r"desktop-v(\d+)\.(\d+)\.(\d+)"),
    "parallels": ("Parallels/vagrant-parallels", "v2.4.7", r"v(\d+)\.(\d+)\.(\d+)"),
    "libvirt": ("vagrant-libvirt/vagrant-libvirt", "0.12.2", r"(\d+)\.(\d+)\.(\d+)"),
    "utm": ("naveenrajm7/vagrant_utm", "v0.1.6", r"v(\d+)\.(\d+)\.(\d+)"),
    "qemu": ("ppggff/vagrant-qemu", "v0.6.3", r"v(\d+)\.(\d+)\.(\d+)"),
    "vmware_esxi": ("josenk/vagrant-vmware-esxi", "v2.5.2", r"v(\d+)\.(\d+)\.(\d+)"),
}
# One list per provider config class: (repository, path).
PROVIDERS = {
    "vm": ("vagrant", "plugins/kernel_v2/config/vm.rb"),
    "virtualbox": ("vagrant", "plugins/providers/virtualbox/config.rb"),
    "hyperv": ("vagrant", "plugins/providers/hyperv/config.rb"),
    "docker": ("vagrant", "plugins/providers/docker/config.rb"),
    "vmware_desktop": ("vmware_desktop", "lib/vagrant-vmware-desktop/config.rb"),
    "parallels": ("parallels", "lib/vagrant-parallels/config.rb"),
    "libvirt": ("libvirt", "lib/vagrant-libvirt/config.rb"),
    # Apple Silicon Macs (besides VMware Fusion and Parallels above).
    "utm": ("utm", "lib/vagrant_utm/config.rb"),
    "qemu": ("qemu", "lib/vagrant-qemu/config.rb"),
    # Standalone ESXi hosts (no vCenter).
    "vmware_esxi": ("vmware_esxi", "lib/vagrant-vmware-esxi/config.rb"),
}
SSH = ["plugins/kernel_v2/config/ssh_connect.rb", "plugins/kernel_v2/config/ssh.rb"]
ANSIBLE = "plugins/provisioners/ansible/config/"
CHEF = "plugins/provisioners/chef/config/"
CHEF_RUNNER = [CHEF + "base.rb", CHEF + "base_runner.rb"]
CONTAINER = "plugins/provisioners/container/config.rb"
# Lists of `<kind> <option>`: each kind's config class and the classes it inherits from.
GROUPS = {
    "communicators": {
        "ssh": SSH,
        "winrm": ["plugins/communicators/winrm/config.rb"],
        "winssh": SSH + ["plugins/communicators/winssh/config.rb"],
    },
    "provisioners": {
        "*": ["plugins/kernel_v2/config/vm_provisioner.rb"],
        "shell": ["plugins/provisioners/shell/config.rb"],
        "file": ["plugins/provisioners/file/config.rb"],
        "ansible": [ANSIBLE + "base.rb", ANSIBLE + "host.rb"],
        "ansible_local": [ANSIBLE + "base.rb", ANSIBLE + "guest.rb"],
        "chef_solo": CHEF_RUNNER + [CHEF + "chef_solo.rb"],
        "chef_zero": CHEF_RUNNER + [CHEF + "chef_zero.rb"],
        "chef_client": CHEF_RUNNER + [CHEF + "chef_client.rb"],
        "chef_apply": [CHEF + "base.rb", CHEF + "chef_apply.rb"],
        "container": [CONTAINER],
        "docker": [CONTAINER, "plugins/provisioners/docker/config.rb"],
        "podman": [CONTAINER, "plugins/provisioners/podman/config.rb"],
        "puppet": ["plugins/provisioners/puppet/config/puppet.rb"],
        "puppet_server": ["plugins/provisioners/puppet/config/puppet_server.rb"],
        "salt": ["plugins/provisioners/salt/config.rb"],
        "cfengine": ["plugins/provisioners/cfengine/config.rb"],
    },
    "push": {
        "atlas": ["plugins/pushes/atlas/config.rb"],
        "ftp": ["plugins/pushes/ftp/config.rb"],
        "heroku": ["plugins/pushes/heroku/config.rb"],
        "local-exec": ["plugins/pushes/local-exec/config.rb"],
    },
}
# Methods of the config classes that aren't settings.
NOT_SETTINGS = {
    "merge", "validate", "finalize!", "to_s", "get_provider_config", "get_provider_overrides",
    "initialize", "initialize_copy", "add_config", "add_provider_config", "create_trigger",
}
DOCS = "website/content/docs/"
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/isoloom-coverage/coverage/vagrant"


def latest(repo: str, pattern: str) -> str | None:
    """The repository's newest tag following its release pattern, or None when unreachable."""
    try:
        tags = json.load(urllib.request.urlopen(f"https://api.github.com/repos/{repo}/tags?per_page=100"))
    except OSError:
        return None
    versions = [(tuple(int(g) for g in m.groups()), t["name"]) for t in tags if (m := re.fullmatch(pattern, t["name"]))]
    return max(versions)[1] if versions else None


def tags() -> dict[str, str]:
    """Each repository's tag: its pin, or its newest release with ISOLOOM_REFRESH=latest."""
    out = {}
    for key, (repo, pin, pattern) in REPOS.items():
        out[key] = (os.environ.get("ISOLOOM_REFRESH") == "latest" and latest(repo, pattern)) or pin
    return out


def fetch(repo: str, tag: str, path: str) -> str:
    return urllib.request.urlopen(f"https://raw.githubusercontent.com/{repo}/{tag}/{path}").read().decode()


def settings(ruby: str) -> set[str]:
    """The settings a config class accepts: its accessors and setter-like public methods."""
    found = set()
    for line in ruby.splitlines():
        line = line.split("#", 1)[0]
        m = re.match(r"\s*attr_(?:accessor|reader)\s+(.+)", line)
        if m:
            found |= {a.strip().lstrip(":") for a in m.group(1).split(",") if a.strip().startswith(":")}
            continue
        m = re.match(r"\s*def (\w+)(=)?\(", line)
        if m and not m.group(1).startswith(("_", "validate_", "resolve_")) and m.group(1) not in NOT_SETTINGS:
            found.add(m.group(1))
    return found


def bullets(md: str, heading: str) -> set[str]:
    """The options listed as "- `name` (type) - ..." under one `## heading` of a docs page."""
    section = re.split(r"^## ", md, flags=re.M)
    body = next((s for s in section if s.startswith(heading)), "")
    return set(re.findall(r"^- `(\w+)`", body, flags=re.M))


def network_keys(md: str, kind: str) -> set[str]:
    """The options the docs' examples pass to `config.vm.network "<kind>"` (over several lines)."""
    found = set()
    lines = md.splitlines()
    for i, line in enumerate(lines):
        if f'network "{kind}"' not in line:
            continue
        stmt = line
        while stmt.rstrip().endswith((",", "[")) and i + 1 < len(lines):
            i += 1
            stmt += lines[i]
        found |= set(re.findall(r"\b(\w+):(?!:)", re.sub(r'"[^"]*"', '""', stmt)))
    return found


def option_reads(ruby: str) -> set[str]:
    """The options a provider's code reads (`options[:name]`)."""
    return set(re.findall(r"options\[:(\w+)\]", ruby))


def networks(tag: dict[str, str]) -> list[str]:
    vagrant = lambda path: fetch(REPOS["vagrant"][0], tag["vagrant"], path)
    forwarded = bullets(vagrant(DOCS + "networking/forwarded_ports.mdx"), "Options Reference")
    # `disabled` isn't in the docs: the providers skip a forwarded port that sets it.
    forwarded |= option_reads(vagrant("plugins/providers/virtualbox/util/compile_forwarded_ports.rb"))
    private = network_keys(vagrant(DOCS + "networking/private_network.mdx"), "private_network")
    # Options VirtualBox reads (any provider may read the same name), and the ones its docs
    # write with a `virtualbox__` prefix so the other providers ignore them.
    private |= option_reads(vagrant("plugins/providers/virtualbox/action/network.rb"))
    private |= set(re.findall(r"\bvirtualbox__[a-z_]+\b", vagrant(DOCS + "providers/virtualbox/networking.mdx"))) - {"virtualbox__"}
    libvirt = fetch(REPOS["libvirt"][0], tag["libvirt"], "docs/configuration.markdown")
    private |= {o for o in re.findall(r"\blibvirt__\w+", libvirt) if o != "libvirt__"}
    # Hyper-V puts a private network's adapter on the switch it names.
    if "opts[:bridge]" in vagrant("plugins/providers/hyperv/action/configure.rb"):
        private.add("bridge")
    public = network_keys(vagrant(DOCS + "networking/public_network.mdx"), "public_network")
    kinds = {"forwarded_port": forwarded, "private_network": private, "public_network": public}
    return [f"{k} {o}" for k, opts in kinds.items() for o in sorted(opts)]


def folders(tag: dict[str, str]) -> list[str]:
    vagrant = lambda path: fetch(REPOS["vagrant"][0], tag["vagrant"], path)
    disk = settings(vagrant("plugins/kernel_v2/config/disk.rb"))
    synced = bullets(vagrant(DOCS + "synced-folders/basic_usage.mdx"), "Options")
    for page, heading in [("nfs", "NFS Synced Folder Options"), ("rsync", "Options"), ("smb", "Options")]:
        synced |= bullets(vagrant(f"{DOCS}synced-folders/{page}.mdx"), heading)
    return [f"disk {o}" for o in sorted(disk)] + [f"synced_folder {o}" for o in sorted(synced)]


def write(name: str, source: str, lines: list[str]) -> None:
    (OUT / f"{name}.txt").write_text(f"# {source}\n" + "\n".join(lines) + "\n")
    print(f"{name}: {len(lines)} settings")


def main() -> None:
    tag = tags()
    OUT.mkdir(parents=True, exist_ok=True)
    for name, (key, path) in PROVIDERS.items():
        repo = REPOS[key][0]
        url = f"https://raw.githubusercontent.com/{repo}/{tag[key]}/{path}"
        write(name, f"Settings of {url}", sorted(settings(fetch(repo, tag[key], path))))
    vagrant = REPOS["vagrant"][0]
    for name, kinds in GROUPS.items():
        lines = []
        for kind, paths in kinds.items():
            found = set().union(*(settings(fetch(vagrant, tag["vagrant"], p)) for p in paths))
            lines += [f"{kind} {o}" for o in sorted(found)]
        write(name, f"Options of hashicorp/vagrant {tag['vagrant']} config classes, by kind", lines)
    write("networks", f"Network options of hashicorp/vagrant {tag['vagrant']} and vagrant-libvirt {tag['libvirt']}", networks(tag))
    write("folders", f"Disk and synced folder options of hashicorp/vagrant {tag['vagrant']}", folders(tag))


if __name__ == "__main__":
    try:
        main()
    except OSError as e:
        sys.exit(f"refresh-vagrant: {e}")
