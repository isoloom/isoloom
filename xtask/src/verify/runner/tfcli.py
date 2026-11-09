"""Running Terraform: its CLI configuration, a module's working state, and `init`.

Providers come from a plugin cache on the cache volume. `init` first tries it as a filesystem
mirror (no network); a provider it lacks is downloaded once, then the cache serves every run.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

from common import Ctx, Done, run

CLI_CONFIG = """plugin_cache_dir = "{cache}"
plugin_cache_may_break_dependency_lock_file = true
"""
OFFLINE = """provider_installation {{
  filesystem_mirror {{
    path = "{cache}"
  }}
}}
"""
OFFLINE_RC, ONLINE_RC = Path("/tmp/terraform-offline.rc"), Path("/tmp/terraform-online.rc")


def cli_configs(ctx: Ctx) -> tuple[Path, Path]:
    """The Terraform CLI configurations: the cache as the only source, and the cache in front
    of the registry."""
    cache = ctx.cache / "terraform" / "plugins"
    cache.mkdir(parents=True, exist_ok=True)
    OFFLINE_RC.write_text(CLI_CONFIG.format(cache=cache) + OFFLINE.format(cache=cache))
    ONLINE_RC.write_text(CLI_CONFIG.format(cache=cache))
    return OFFLINE_RC, ONLINE_RC


def data_dir(module: Path) -> dict[str, str]:
    """The module's Terraform working state (TF_DATA_DIR), outside the tree: a module that
    lists the project's files (`fileset`) must not walk into another's `.terraform`."""
    name = hashlib.sha256(str(module).encode()).hexdigest()[:16]
    return {"TF_DATA_DIR": f"/tmp/terraform-data/{name}"}


def init(ctx: Ctx, module: Path, backend: bool = False) -> Done:
    """`terraform init`, from the cache alone when it can. The plugin cache isn't safe for
    concurrent writers, nor for a reader linking a package a writer is still unpacking: a
    download waits for the other inits to finish, and holds off new ones."""
    args = ["terraform", "init", "-input=false", "-no-color"] + ([] if backend else ["-backend=false"])
    with ctx.cache_lock.reading():
        done = run(args, cwd=module, env={**data_dir(module), "TF_CLI_CONFIG_FILE": str(OFFLINE_RC)})
    if done.code == 0:
        return done
    with ctx.cache_lock.writing():
        return run(args, cwd=module, env={**data_dir(module), "TF_CLI_CONFIG_FILE": str(ONLINE_RC)})
