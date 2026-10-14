#!/usr/bin/env bash
# Deletes what real-target runs left in a cloud account (docs/real-targets.md, "Reaper"): until
# `isoloom reap` exists (#137), the cloud's own CLI or API, by the tags and names Isoloom's
# modules put on what they create.
#
#   scripts/real-targets/reap.sh <aws|azure|gcp|digitalocean|linode|oci> [--all] [--dry-run]
#
# What goes:
# - a resource tagged with `isoloom-expires-at` (`isoloom-expires-<unix seconds>` on DigitalOcean
#   and Linode) whose time is past;
# - a resource only named `isoloom-*` (networks, firewalls, keys; everything on Oracle Cloud,
#   whose modules don't tag) created more than REAP_MAX_AGE_HOURS hours ago (default 6);
# - with --all, every Isoloom resource in the account, whatever its age: after a failed run, in
#   an account that only CI uses (the workflow runs one job per cloud at a time).
# With --dry-run (or DRY_RUN=1), nothing is deleted; what would be is listed.
#
# Credentials and scope come from the environment, as for the cloud's CLI: AWS_REGION (default
# eu-west-3, the modules' default), ARM_CLIENT_ID, ARM_CLIENT_SECRET, ARM_TENANT_ID and
# ARM_SUBSCRIPTION_ID for Azure, `gcloud` logged in with CLOUDSDK_CORE_PROJECT for Google Cloud,
# DIGITALOCEAN_TOKEN, LINODE_TOKEN, ~/.oci/config with OCI_COMPARTMENT_ID. Exits 1 when a deletion
# failed, after trying every other one.
set -euo pipefail

cloud=${1:?usage: reap.sh <cloud> [--all] [--dry-run]}
shift
all=false
dry=${DRY_RUN:-0}
for arg in "$@"; do
  case "$arg" in
    --all) all=true ;;
    --dry-run) dry=1 ;;
    *) echo "reap.sh: unknown argument $arg" >&2; exit 2 ;;
  esac
done
now=$(date +%s)
cutoff=$((now - ${REAP_MAX_AGE_HOURS:-6} * 3600))
failures=0
reaped=0

# Runs a deletion, or only prints it on a dry run. A failure is counted, not fatal: the next
# resource still goes.
act() {
  echo "reap: $*"
  if [ "$dry" = 1 ]; then return; fi
  if "$@"; then reaped=$((reaped + 1)); else
    echo "::warning title=reaper::failed: $*"
    failures=$((failures + 1))
  fi
}

# Whether an `isoloom-expires-at` value (unix seconds, maybe empty) means the resource goes.
expired() {
  [ "$all" = true ] && return 0
  [[ "$1" =~ ^[0-9]+$ ]] && [ "$1" -le "$now" ]
}

# Whether a creation time (ISO 8601, any offset or none for UTC) means an untagged resource goes.
old() {
  [ "$all" = true ] && return 0
  local t
  t=$(python3 -c 'import sys, datetime as d
t = d.datetime.fromisoformat(sys.argv[1].replace("Z", "+00:00"))
print(int((t if t.tzinfo else t.replace(tzinfo=d.timezone.utc)).timestamp()))' "$1") || return 1
  [ "$t" -le "$cutoff" ]
}

# AWS: instances first (their volumes go with them), then each expired VPC with what's in it,
# then key pairs. Everything carries the provider's default tags.
reap_aws() {
  export AWS_REGION=${AWS_REGION:-eu-west-3}
  local tagged='Name=tag:managed-by,Values=isoloom' pick ids id exp vpc
  # shellcheck disable=SC2016 # jq's own variable, not the shell's
  pick='def tag($k): ((.Tags // []) | map(select(.Key == $k)) | first | .Value) // "";'
  ids=()
  while IFS=$'\t' read -r id exp; do
    if expired "$exp"; then ids+=("$id"); fi
  done < <(aws ec2 describe-instances --filters "$tagged" Name=instance-state-name,Values=pending,running,stopping,stopped \
    --output json | jq -r "$pick"' .Reservations[].Instances[] | [.InstanceId, tag("isoloom-expires-at")] | @tsv')
  if [ "${#ids[@]}" -gt 0 ]; then
    act aws ec2 terminate-instances --instance-ids "${ids[@]}" --output text
    [ "$dry" = 1 ] || aws ec2 wait instance-terminated --instance-ids "${ids[@]}" || true
  fi
  while IFS=$'\t' read -r vpc exp; do
    if expired "$exp"; then reap_aws_vpc "$vpc"; fi
  done < <(aws ec2 describe-vpcs --filters "$tagged" --output json | jq -r "$pick"' .Vpcs[] | [.VpcId, tag("isoloom-expires-at")] | @tsv')
  while IFS=$'\t' read -r id exp; do
    if expired "$exp"; then act aws ec2 delete-key-pair --key-pair-id "$id"; fi
  done < <(aws ec2 describe-key-pairs --filters "$tagged" --output json | jq -r "$pick"' .KeyPairs[] | [.KeyPairId, tag("isoloom-expires-at")] | @tsv')
}

# One VPC, in the order AWS accepts: security groups, subnets, route tables, gateways, the VPC.
reap_aws_vpc() {
  local vpc=$1 in="Name=vpc-id,Values=$1" id
  # shellcheck disable=SC2016 # JMESPath backticks, not a command
  for id in $(aws ec2 describe-security-groups --filters "$in" --query 'SecurityGroups[?GroupName!=`default`].GroupId' --output text); do
    act aws ec2 delete-security-group --group-id "$id"
  done
  for id in $(aws ec2 describe-subnets --filters "$in" --query 'Subnets[].SubnetId' --output text); do
    act aws ec2 delete-subnet --subnet-id "$id"
  done
  for id in $(aws ec2 describe-route-tables --filters "$in" --query 'RouteTables[?!(Associations[?Main])].RouteTableId' --output text); do
    act aws ec2 delete-route-table --route-table-id "$id"
  done
  for id in $(aws ec2 describe-internet-gateways --filters "Name=attachment.vpc-id,Values=$vpc" --query 'InternetGateways[].InternetGatewayId' --output text); do
    act aws ec2 detach-internet-gateway --internet-gateway-id "$id" --vpc-id "$vpc"
    act aws ec2 delete-internet-gateway --internet-gateway-id "$id"
  done
  act aws ec2 delete-vpc --vpc-id "$vpc"
}

# Azure's Resource Manager API, as the service principal Terraform uses (ARM_*): each
# environment is one tagged resource group, and deleting it deletes everything in it.
azure_api() {
  curl -fsS -X "$1" -H "Authorization: Bearer $azure_token" \
    "https://management.azure.com/subscriptions/${ARM_SUBSCRIPTION_ID:?ARM_SUBSCRIPTION_ID is not set}/$2"
}

reap_azure() {
  local name exp
  azure_token=$(curl -fsS "https://login.microsoftonline.com/${ARM_TENANT_ID:?ARM_TENANT_ID is not set}/oauth2/v2.0/token" \
    --data-urlencode "client_id=${ARM_CLIENT_ID:?ARM_CLIENT_ID is not set}" \
    --data-urlencode "client_secret=${ARM_CLIENT_SECRET:?ARM_CLIENT_SECRET is not set}" \
    --data-urlencode 'scope=https://management.azure.com/.default' \
    --data-urlencode 'grant_type=client_credentials' | jq -r .access_token)
  # Deleting a group is accepted at once and finishes in the background.
  while IFS=$'\t' read -r name exp; do
    if expired "$exp"; then act azure_api DELETE "resourcegroups/$name?api-version=2021-04-01"; fi
  done < <(azure_api GET "resourcegroups?api-version=2021-04-01&%24filter=tagName%20eq%20'managed-by'%20and%20tagValue%20eq%20'isoloom'" |
    jq -r '.value[] | [.name, (.tags["isoloom-expires-at"] // "")] | @tsv')
}

# Google Cloud: labelled instances (their boot disks go with them), then the `isoloom-*`
# networks with their firewall rules and subnetworks, which take no labels.
reap_gcp() {
  local name zone exp net link created id region
  while IFS=$'\t' read -r name zone exp; do
    if expired "$exp"; then act gcloud compute instances delete "$name" --zone "$zone" --quiet; fi
  done < <(gcloud compute instances list --filter='labels.managed-by=isoloom' --format=json |
    jq -r '.[] | [.name, (.zone | split("/") | last), (.labels["isoloom-expires-at"] // "")] | @tsv')
  while IFS=$'\t' read -r net link created; do
    old "$created" || continue
    for id in $(gcloud compute firewall-rules list --filter="network=$link" --format='value(name)'); do
      act gcloud compute firewall-rules delete "$id" --quiet
    done
    while IFS=$'\t' read -r id region; do
      act gcloud compute networks subnets delete "$id" --region "$region" --quiet
    done < <(gcloud compute networks subnets list --network "$net" --format=json | jq -r '.[] | [.name, (.region | split("/") | last)] | @tsv')
    act gcloud compute networks delete "$net" --quiet
  done < <(gcloud compute networks list --format=json | jq -r '.[] | select(.name | startswith("isoloom-")) | [.name, .selfLink, .creationTimestamp] | @tsv')
}

# DigitalOcean's API: droplets by tag, then the `isoloom-*` firewalls, VPCs and SSH keys.
do_api() {
  curl -fsS -X "$1" -H "Authorization: Bearer ${DIGITALOCEAN_TOKEN:?DIGITALOCEAN_TOKEN is not set}" "https://api.digitalocean.com/v2/$2"
}

reap_digitalocean() {
  local id exp kind created
  while IFS=$'\t' read -r id exp; do
    if expired "$exp"; then act do_api DELETE "droplets/$id"; fi
  done < <(do_api GET 'droplets?tag_name=isoloom&per_page=200' |
    jq -r '.droplets[] | [.id, ((.tags | map(select(startswith("isoloom-expires-"))) | first // "") | ltrimstr("isoloom-expires-"))] | @tsv')
  # The VPC frees up once its droplets are gone, which DigitalOcean does in the background.
  [ "$dry" = 1 ] || sleep 30
  for kind in firewalls vpcs account/keys; do
    while IFS=$'\t' read -r id created; do
      if old "$created"; then act do_api DELETE "$kind/$id"; fi
    done < <(do_api GET "$kind?per_page=200" |
      jq -r '(.firewalls // .vpcs // .ssh_keys)[] | select(.name | startswith("isoloom-")) | [.id, (.created_at // "1970-01-01T00:00:00Z")] | @tsv')
  done
}

# Linode's API: instances by tag, then the `isoloom-*` firewalls.
linode_api() {
  curl -fsS -X "$1" -H "Authorization: Bearer ${LINODE_TOKEN:?LINODE_TOKEN is not set}" "https://api.linode.com/v4/$2"
}

reap_linode() {
  local id exp created
  while IFS=$'\t' read -r id exp; do
    if expired "$exp"; then act linode_api DELETE "linode/instances/$id"; fi
  done < <(linode_api GET 'linode/instances?page_size=500' |
    jq -r '.data[] | select(.tags | index("isoloom")) | [.id, ((.tags | map(select(startswith("isoloom-expires-"))) | first // "") | ltrimstr("isoloom-expires-"))] | @tsv')
  while IFS=$'\t' read -r id created; do
    if old "$created"; then act linode_api DELETE "networking/firewalls/$id"; fi
  done < <(linode_api GET 'networking/firewalls?page_size=500' | jq -r '.data[] | select(.label | startswith("isoloom-")) | [.id, .created] | @tsv')
}

# Oracle Cloud: the modules don't tag, so instances and VCNs named `isoloom-*` in the CI
# compartment, by age. A VCN goes after its subnets, route tables, security lists and gateways.
reap_oci() {
  local c=${OCI_COMPARTMENT_ID:?OCI_COMPARTMENT_ID is not set} id created
  while IFS=$'\t' read -r id created; do
    if old "$created"; then
      act oci compute instance terminate --instance-id "$id" --preserve-boot-volume false --force --wait-for-state TERMINATED
    fi
  done < <(oci compute instance list -c "$c" --all | jq -r '.data[]? | select((."display-name" | startswith("isoloom-")) and ."lifecycle-state" != "TERMINATED") | [.id, ."time-created"] | @tsv')
  while IFS=$'\t' read -r id created; do
    if old "$created"; then reap_oci_vcn "$c" "$id"; fi
  done < <(oci network vcn list -c "$c" --all | jq -r '.data[]? | select((."display-name" | startswith("isoloom-")) and ."lifecycle-state" == "AVAILABLE") | [.id, ."time-created"] | @tsv')
}

reap_oci_vcn() {
  local c=$1 vcn=$2 id own='.data[]? | select(."display-name" | startswith("isoloom-")) | .id'
  for id in $(oci network subnet list -c "$c" --vcn-id "$vcn" --all | jq -r "$own"); do
    act oci network subnet delete --subnet-id "$id" --force --wait-for-state TERMINATED
  done
  for id in $(oci network route-table list -c "$c" --vcn-id "$vcn" --all | jq -r "$own"); do
    act oci network route-table delete --rt-id "$id" --force --wait-for-state TERMINATED
  done
  for id in $(oci network security-list list -c "$c" --vcn-id "$vcn" --all | jq -r "$own"); do
    act oci network security-list delete --security-list-id "$id" --force --wait-for-state TERMINATED
  done
  for id in $(oci network internet-gateway list -c "$c" --vcn-id "$vcn" --all | jq -r "$own"); do
    act oci network internet-gateway delete --ig-id "$id" --force --wait-for-state TERMINATED
  done
  act oci network vcn delete --vcn-id "$vcn" --force --wait-for-state TERMINATED
}

case "$cloud" in
  aws | azure | gcp | digitalocean | linode | oci) "reap_$cloud" ;;
  *) echo "reap.sh: unknown cloud $cloud (aws, azure, gcp, digitalocean, linode, oci)" >&2; exit 2 ;;
esac

echo "reap: $cloud: $reaped deleted, $failures failed$([ "$dry" = 1 ] && echo ' (dry run, nothing deleted)')"
[ "$failures" -eq 0 ]
