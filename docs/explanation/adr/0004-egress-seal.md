# 0004 — Seal an offline network after set-up

**Status:** Accepted (2026-10).

## Context

A network with `internet: false` must have no internet. Locally, that is enforced inside the
guest (a removed default route), which root could add back. On a cloud, machines usually need the
internet *during* set-up (to install packages), so cutting egress up front breaks provisioning —
but leaving it open after set-up means `internet: false` isn't a real guarantee. Earlier
`tf-test` runs recorded this as a known bug on every cloud; the most serious case was Azure, whose
default `AllowVnetInBound` let segmented environments reach across segments.

## Decision

**Seal after set-up.** Machines get internet only while they set themselves up. Once the
environment is ready, the runner applies a **seal** at the network level — a second Terraform
apply with the module's seal variable on:

- egress to the internet is cut for `internet: false` networks (each cloud does it its own way: a
  route table without a default route, an NSG outbound deny, EGRESS firewall rules, a firewall
  policy flip, dropped security-list egress, or a VPC-only outbound);
- the machines keep their public addresses, so the runner's SSH and the published ports still
  answer from the allowed ranges;
- segmentation is enforced against each cloud's own default (Azure NSGs deny the rest of the
  virtual network inbound after a machine's allow rules).

The run manifest carries the seal as an optional `seal` step, left out when empty so other
manifests stay byte-identical. The seal **fails closed** (a failed seal means the run is not
ready), and every later operation replays the recorded value, so a later apply can't silently undo
it. See the [run manifest reference](../../reference/run-manifest.md).

## Consequences

- `internet: false` is a real guarantee on clouds, not a guest-level courtesy that root can
  revert.
- One residual channel remains: the cloud resolver still recurses to public names. It is closed
  with a DNS allow-list where that is cheap, and documented as a known limitation otherwise.
- The same mechanism is reused for the Proxmox and ESXi routers in a server follow-up.
- A privileged container on an `internet: false` network is refused at validation, because a
  privileged container could re-add the local route the offline seal removes.

## Update: one apply (2026-11)

With no run manifest (Isoloom writes only the tools' own files), the seal is no longer a second
apply with a variable: it is part of the module, made after the machines' set-up within the one
`terraform apply`. Each module waits for every machine's set-up (`terraform_data.isoloom_ready_*`,
which reads the ready file over SSH), and the closing resources `depends_on` those waits:

- a server's router: `terraform_data.isoloom_seal` applies the sealed ruleset over SSH;
- AWS: a network ACL on the offline subnets (out to the environment's networks and
  `allowed_cidr` only);
- Azure: a security group on the offline subnets (no internet out; each machine's own group
  still filters what comes in);
- Google Cloud: the deny and allow EGRESS rules, made once the machines are set up;
- Oracle Cloud: a route table with no way to the internet, attached to the offline subnets.

Nothing made later can switch a resource the machines needed first, so a cloud whose firewalls
only allow and are made with the machine (DigitalOcean, Linode) can't seal an offline network in
one apply: those clouds refuse it.
