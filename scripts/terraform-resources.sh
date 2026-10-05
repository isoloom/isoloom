#!/bin/sh
# Refreshes crates/isoloom-core/coverage/terraform/*.txt: the resource types each Terraform
# provider offers, from `terraform providers schema -json` (names only).
# Usage: sh scripts/terraform-resources.sh   (then `cargo test`: unclassified resources fail)
set -eu
OUT="$(cd "$(dirname "$0")/.." && pwd)/crates/isoloom-core/coverage/terraform"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
extract() { # name source version
  mkdir -p "$WORK/$1"
  printf 'terraform {\n  required_providers {\n    p = { source = "%s", version = "%s" }\n  }\n}\n' "$2" "$3" > "$WORK/$1/main.tf"
  terraform -chdir="$WORK/$1" init -input=false >/dev/null
  terraform -chdir="$WORK/$1" providers schema -json | python3 -c '
import json, sys
s = json.load(sys.stdin)
(name, p), = s["provider_schemas"].items()
print(f"# Resource types of {name} " + sys.argv[1])
print("\n".join(sorted(p["resource_schemas"])))
' "$3" > "$OUT/$1.txt"
  echo "$1: $(grep -vc '^#' "$OUT/$1.txt") resource types"
}
extract proxmox bpg/proxmox 0.115.0
extract aws hashicorp/aws 6.67.0
extract azure hashicorp/azurerm 5.8.0
extract google hashicorp/google 8.5.0
extract digitalocean digitalocean/digitalocean 2.103.0
extract linode linode/linode 4.7.0
extract oci oracle/oci 9.8.0
extract esxi josenk/esxi 1.10.3
