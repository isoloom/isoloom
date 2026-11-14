# Isoloom documentation

Isoloom converts one environment spec (`isoloom.yml`) into the native files of each
infrastructure tool, and nothing else: the tools run them, by hand or through a runner such as
[Isothrum](https://github.com/Isothrum/isothrum). These pages are grouped by what you want to
do.

## Guides: learn and get things done

Start here:

- [Getting started](guide/getting-started.md): install, generate the `hello-stack` example, run
  it with Docker Compose or Isothrum, change it.
- [Verify a release](guide/verify-a-release.md): check a download's signature, checksum and
  provenance.

Per target backend (prerequisites, credentials, permissions, cost, teardown):

- [Docker Compose](guide/targets/docker.md): `docker`, `hosted`
- [Kubernetes](guide/targets/kubernetes.md): manifests, Kustomize, the Helm chart
- [Vagrant](guide/targets/vagrant.md): `vagrant`
- [Proxmox VE](guide/targets/proxmox.md): `proxmox`
- [ESXi](guide/targets/esxi.md): `esxi`
- [Clouds](guide/targets/cloud.md): `cloud-vm`, `cloud-docker`, `cloud-services` on AWS, Azure,
  Google Cloud, DigitalOcean, Linode and Oracle Cloud
- [Existing hosts](guide/targets/external.md): `external`, with Ansible

How-tos:

- [Behind a corporate proxy, mirrors, registries, NTP and a corporate CA](guide/corporate-network.md)
- [Use your own (golden) images](guide/golden-images.md)
- [Generate offline](guide/offline.md)
- [Use Isoloom in CI](guide/ci.md), with the GitHub Action
- [Commit the generated files](guide/commit-generated-files.md)
- [Keep Terraform state remotely](guide/remote-terraform-state.md)
- [Use catalogue services](guide/catalogue.md)
- [Troubleshooting and FAQ](guide/troubleshooting.md)
- [Migrate to 2.0](guide/migrating-to-2.0.md), and from 0.x [to 1.0](guide/migrating-to-1.0.md)

## Reference: look things up

- [Spec reference](spec.md): every field of `isoloom.yml` (generated from the schema)
- [Command line](reference/cli.md): every command and flag (generated from the CLI)
- [Target matrix](matrix.md): what each target does with each spec field, and the validators
  that check each target's files (generated)
- [Targets and providers](reference/targets.md)
- [Configuration](reference/configuration.md): defaults, `ISOLOOM_*` variables, profiles, the
  image table
- [Tool versions](reference/tool-versions.md): the tools each target needs, and the Terraform
  provider pins
- [Examples](reference/examples.md): the example projects under `examples/`
- [Policies](reference/policies.md): versioning, what is stable, deprecation, supported
  releases, MSRV

## Explanation: understand the design

- [Architecture](explanation/architecture.md)
- [Security considerations](explanation/security.md)
- [Determinism](explanation/determinism.md)

## Contributing

- [Contributing](../.github/CONTRIBUTING.md) and [governance](../.github/GOVERNANCE.md)
