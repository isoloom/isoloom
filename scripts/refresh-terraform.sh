#!/bin/sh
# Refreshes crates/isoloom-coverage/coverage/terraform/: what each Terraform provider offers, from
# `terraform providers schema -json` with the provider pinned to the version the generators
# require (crates/isoloom-terraform/src/vm/versions.rs; keep both in step).
#
#   <name>.txt          every resource type of the provider (each must be classified)
#   <name>-schema.txt   the provider's settings, and every argument and nested block of the
#                       resource types and data sources Isoloom writes (or, for the utility
#                       providers, of all of them)
#
# Usage: sh scripts/refresh-terraform.sh   (then `cargo test -p isoloom-coverage`: a new
# resource type or argument is unclassified and fails the tests until it is reviewed)
# With ISOLOOM_REFRESH=latest, each provider's newest release is read instead of its pin (the
# weekly drift job), and the files name the version read.
set -eu
OUT="$(cd "$(dirname "$0")/.." && pwd)/crates/isoloom-coverage/coverage/terraform"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cat > "$WORK/extract.py" <<'PY'
import json, sys

schema = json.load(sys.stdin)
(name, p), = schema["provider_schemas"].items()
version, out, wanted = sys.argv[1], sys.argv[2], sys.argv[3:]

def walk(block, prefix):
    """Every argument (required or optional, not computed only) and nested block, depth first."""
    rows = []
    for attr, a in sorted(block.get("attributes", {}).items()):
        if not (a.get("required") or a.get("optional")):
            continue
        rows.append(prefix + attr)
        nested = a.get("nested_type")
        if nested:
            rows += walk(nested, prefix + attr + ".")
        rows += fields(a.get("type"), prefix + attr + ".")
    for blk, b in sorted(block.get("block_types", {}).items()):
        rows.append(prefix + blk)
        rows += walk(b["block"], prefix + blk + ".")
    return rows

def fields(t, prefix):
    """The fields of an object-typed argument (or a list, set or map of objects), which a
    configuration writes like a nested block."""
    if isinstance(t, list) and t[0] in ("list", "set", "map") and isinstance(t[1], list):
        t = t[1]
    if not (isinstance(t, list) and t[0] == "object"):
        return []
    rows = []
    for name, sub in sorted(t[1].items()):
        rows.append(prefix + name)
        rows += fields(sub, prefix + name + ".")
    return rows

kinds = {"resource": p.get("resource_schemas", {}), "data": p.get("data_source_schemas", {})}
everything = wanted == ["all"]
missing = [w for w in wanted if not everything and w.split(":", 1)[1] not in kinds[w.split(":", 1)[0]]]
if missing:
    sys.exit(f"{name} {version} has no {missing}")
with open(out + ".txt", "w") as f:
    f.write(f"# Resource types of {name} {version}\n")
    f.write("".join(t + "\n" for t in sorted(kinds["resource"])))
if not wanted:
    sys.exit(0)
with open(out + "-schema.txt", "w") as f:
    f.write(f"# Settings, and arguments and nested blocks of what Isoloom writes, of {name} {version}\n")
    f.write("".join(f"provider {r}\n" for r in walk(p["provider"]["block"], "")))
    for kind in ("resource", "data"):
        for t in sorted(kinds[kind]):
            if not everything and f"{kind}:{t}" not in wanted:
                continue
            f.write(f"{kind} {t}\n")
            f.write("".join(f"{kind} {t} {r}\n" for r in walk(kinds[kind][t]["block"], "")))
PY
cat > "$WORK/selected.py" <<'PY'
import json, sys

v = json.load(sys.stdin)
print(next(iter(v["provider_selections"].values())))
PY
extract() { # name source version (kind:type ... | all)
  name=$1 source=$2 version=$3
  shift 3
  dir="$WORK/$name"
  mkdir -p "$dir"
  constraint=", version = \"$version\""
  [ "${ISOLOOM_REFRESH:-}" = latest ] && constraint=""
  if [ "$source" = terraform.io/builtin/terraform ]; then
    # Built into Terraform itself: its version is Terraform's, and a resource brings it in.
    printf 'resource "terraform_data" "p" {}\n' > "$dir/main.tf"
  else
    printf 'terraform {\n  required_providers {\n    p = { source = "%s"%s }\n  }\n}\n' "$source" "$constraint" > "$dir/main.tf"
  fi
  terraform -chdir="$dir" init -input=false >/dev/null
  # The built-in provider keeps its label: its schema follows Terraform's, whichever runs.
  [ "$version" = built-in ] || version=$(terraform -chdir="$dir" version -json | python3 "$WORK/selected.py")
  terraform -chdir="$dir" providers schema -json > "$dir/schema.json"
  python3 "$WORK/extract.py" "$version" "$OUT/$name" "$@" < "$dir/schema.json"
  echo "$name $version: $(grep -vc '^#' "$OUT/$name.txt") resource types"
}
extract proxmox bpg/proxmox 0.115.0 \
  resource:proxmox_virtual_environment_vm resource:proxmox_virtual_environment_file resource:proxmox_download_file \
  resource:proxmox_sdn_zone_simple resource:proxmox_sdn_vnet resource:proxmox_sdn_applier
extract aws hashicorp/aws 6.67.0 \
  resource:aws_instance resource:aws_vpc resource:aws_vpc_ipv4_cidr_block_association resource:aws_subnet \
  resource:aws_internet_gateway resource:aws_route_table resource:aws_route_table_association \
  resource:aws_security_group resource:aws_network_interface resource:aws_network_interface_attachment \
  resource:aws_eip resource:aws_key_pair data:aws_ami data:aws_availability_zones
extract azure hashicorp/azurerm 5.8.0 \
  resource:azurerm_resource_group resource:azurerm_virtual_network resource:azurerm_subnet \
  resource:azurerm_public_ip resource:azurerm_network_security_group resource:azurerm_network_interface \
  resource:azurerm_network_interface_security_group_association resource:azurerm_linux_virtual_machine \
  resource:azurerm_windows_virtual_machine
extract google hashicorp/google 8.5.0 \
  resource:google_project resource:google_project_service resource:google_compute_network \
  resource:google_compute_subnetwork resource:google_compute_firewall resource:google_compute_instance
extract digitalocean digitalocean/digitalocean 2.103.0 \
  resource:digitalocean_vpc resource:digitalocean_ssh_key resource:digitalocean_droplet resource:digitalocean_firewall
extract linode linode/linode 4.7.0 \
  resource:linode_instance resource:linode_firewall resource:linode_vpc resource:linode_vpc_subnet
extract oci oracle/oci 9.8.0 \
  resource:oci_core_vcn resource:oci_core_internet_gateway resource:oci_core_route_table \
  resource:oci_core_security_list resource:oci_core_subnet resource:oci_core_instance \
  resource:oci_core_network_security_group resource:oci_core_network_security_group_security_rule \
  data:oci_core_images data:oci_identity_availability_domains
extract esxi josenk/esxi 1.10.3
# The utility providers the modules use beside each platform's (keys, passwords, files, waits),
# and the ones a module may need next (cloud-init documents, HTTP checks, triggers): all of them.
extract terraform terraform.io/builtin/terraform built-in all
extract random hashicorp/random 3.9.1 all
extract tls hashicorp/tls 4.4.1 all
extract local hashicorp/local 2.9.1 all
extract time hashicorp/time 0.14.2 all
extract null hashicorp/null 3.3.2 all
extract cloudinit hashicorp/cloudinit 2.4.1 all
extract http hashicorp/http 3.6.2 all
