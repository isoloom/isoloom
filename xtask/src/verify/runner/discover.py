"""Which validator reads which file: by name first, then by content."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path

import yaml

# Folders that are a tool's working state, never generated output.
PRUNED = {".terraform", ".git", ".vagrant", "__pycache__", "node_modules"}
OUTPUT_DIR = re.compile(r"^\.isoloom(-\d+)?$")
COMPOSE = re.compile(r"^(docker-)?compose(\.[\w.-]+)?\.ya?ml$")
DOCKERFILE = re.compile(r"^(Dockerfile|Containerfile)([.-][\w.-]+)?$|\.(Dockerfile|Containerfile)$")
INVENTORY = re.compile(r"^(inventory|hosts)([\w.-]*)(\.ini|\.ya?ml)?$")
SHELLS = re.compile(r"^#!\s*(/usr)?/bin/(env\s+)?(sh|bash|dash|ksh|ash)\b")
KUSTOMIZATION = {"kustomization.yaml", "kustomization.yml", "Kustomization"}
PLAY_KEYS = {"hosts", "import_playbook", "ansible.builtin.import_playbook"}
CHART_FILES = {"Chart.yaml", "values.yaml", "values.schema.json", ".helmignore", "Chart.lock"}


@dataclass
class Found:
    """The tree's files, by the validator that reads them."""

    compose: list[Path] = field(default_factory=list)
    modules: list[Path] = field(default_factory=list)
    vagrantfiles: list[Path] = field(default_factory=list)
    charts: list[Path] = field(default_factory=list)
    kustomizations: list[Path] = field(default_factory=list)
    manifests: list[Path] = field(default_factory=list)
    cloud_configs: list[Path] = field(default_factory=list)
    dockerfiles: list[Path] = field(default_factory=list)
    inventories: list[Path] = field(default_factory=list)
    playbooks: list[Path] = field(default_factory=list)
    shell: list[Path] = field(default_factory=list)
    powershell: list[Path] = field(default_factory=list)
    unknown: list[Path] = field(default_factory=list)

    def checked(self) -> int:
        """How many files (or module and chart folders) a validator reads."""
        return sum(len(v) for k, v in vars(self).items() if k != "unknown")


def files(tree: Path) -> list[Path]:
    """The generated files: those under an `.isoloom/` folder when the tree has one (a project
    tree, whose other files are the spec's inputs), every file otherwise."""
    every: list[Path] = []
    for top, dirs, names in os.walk(tree):
        dirs[:] = sorted(d for d in dirs if d not in PRUNED)
        every.extend(Path(top) / n for n in sorted(names))
    scoped = [f for f in every if any(OUTPUT_DIR.match(p) for p in f.relative_to(tree).parts[:-1])]
    return scoped or every


def first_line(path: Path) -> str:
    """A file's first line, or nothing for one that can't be read as text."""
    try:
        with path.open("r", encoding="utf-8", errors="strict") as f:
            return f.readline(512)
    except (OSError, UnicodeDecodeError):
        return ""


def yaml_kind(path: Path) -> str:
    """What a YAML file holds: `manifests`, `playbooks`, `inventories`, or nothing known."""
    try:
        docs = [d for d in yaml.safe_load_all(path.read_text(encoding="utf-8")) if d is not None]
    except (OSError, UnicodeDecodeError, yaml.YAMLError):
        return ""
    if docs and all(isinstance(d, dict) and "apiVersion" in d and "kind" in d for d in docs):
        return "manifests"
    if len(docs) == 1 and isinstance(docs[0], list) and docs[0] and all(isinstance(p, dict) and PLAY_KEYS & p.keys() for p in docs[0]):
        return "playbooks"
    if len(docs) == 1 and isinstance(docs[0], dict) and isinstance(docs[0].get("all"), dict):
        return "inventories"
    return ""


def by_name(found: Found, f: Path) -> bool:
    """Files known by their name. False when the name says nothing."""
    name = f.name
    buckets = [
        (name == "Vagrantfile", found.vagrantfiles),
        (bool(COMPOSE.match(name)), found.compose),
        (bool(DOCKERFILE.search(name)), found.dockerfiles),
        (f.suffix in (".ps1", ".psm1"), found.powershell),
        (f.suffix in (".sh", ".bash"), found.shell),
        (bool(INVENTORY.match(name)) and f.suffix != ".json", found.inventories),
    ]
    for hit, bucket in buckets:
        if hit:
            bucket.append(f)
            return True
    return False


def by_content(found: Found, f: Path) -> None:
    """Files known by what they hold: cloud-config, a shebang, Kubernetes or Ansible YAML."""
    line = first_line(f)
    if line.startswith("#cloud-config"):
        found.cloud_configs.append(f)
    elif SHELLS.match(line):
        found.shell.append(f)
    elif f.suffix in (".yaml", ".yml") and (kind := yaml_kind(f)):
        getattr(found, kind).append(f)
    else:
        found.unknown.append(f)


def discover(tree: Path) -> Found:
    """Sorts the tree's generated files by validator. A Terraform module or a Helm chart is a
    folder; the YAML inside a chart is the chart's (Helm reads it), its scripts are checked."""
    found = Found()
    listed = files(tree)
    found.modules = sorted({f.parent for f in listed if f.suffix == ".tf"})
    found.charts = sorted({f.parent for f in listed if f.name == "Chart.yaml"})
    found.kustomizations = sorted({f.parent for f in listed if f.name in KUSTOMIZATION})
    for f in listed:
        if f.suffix == ".tf" or f.name in KUSTOMIZATION or f.name == ".terraform.lock.hcl" or helms(found, f):
            continue
        if not by_name(found, f):
            by_content(found, f)
    return found


def helms(found: Found, f: Path) -> bool:
    """Whether Helm reads `f` as part of a chart: its metadata, values and templates."""
    for chart in found.charts:
        if chart in f.parents:
            parts = f.relative_to(chart).parts
            return parts[0] == "templates" or parts[-1] in CHART_FILES
    return False
