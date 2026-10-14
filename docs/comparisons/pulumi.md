# Isoloom and Pulumi

[Pulumi](https://www.pulumi.com/) describes infrastructure in general-purpose languages
(TypeScript, Python, Go, C#, Java) or YAML, and deploys it with a state, a preview and an
update, across a very wide range of providers.

## What Pulumi does better

- **A real programming language.** Functions, classes, packages, tests and your editor's tooling,
  for infrastructure with logic in it. Isoloom's spec is declarative YAML on purpose.
- **Any infrastructure,** with the providers' full schemas, and component packages to share
  patterns.
- **Long-lived infrastructure:** state, previews, a stack per stage, encrypted configuration,
  policy as code, and a service for teams.
- **Automation API.** Pulumi can be embedded in your own program to drive deployments. Isoloom
  offers [JSON output](../json.md) and a small supported library surface instead.

## What Isoloom adds

- **One spec, many platforms,** local ones included: Docker, Kubernetes, local VMs through
  Vagrant, Proxmox, ESXi and six clouds, from the same file ([Targets](../targets.md)). A Pulumi
  program targets the providers it calls.
- **Isolation and checks.** `reach` is enforced on each target (routers, NetworkPolicies,
  firewall rules) and `isoloom test` proves it from inside the environment.
- **Configured machines.** Provisioning, Ansible playbooks and secrets delivered at run time are
  part of the environment, not a separate step.
- **Output you can read and keep.** Each target's files (a Compose file, manifests, a
  Vagrantfile, Terraform modules) are written under `.isoloom/` and can be run without Isoloom.

## When to use which

- Use **Pulumi** for production infrastructure, when your team prefers code to configuration,
  or when infrastructure is part of an application's own codebase.
- Use **Isoloom** for environments you create, test and destroy, that must run in more than one
  place (a laptop and a cloud, say), and whose network behavior must be proven.

## Bringing a Pulumi project in

There is no `isoloom import` for Pulumi programs: a program's resources are only known once it
runs. Write the spec by hand from what the program creates (its VMs and their networks are the
part that maps), starting from [Getting started](../getting-started.md).

If the project still has Terraform files it came from, `isoloom import terraform` drafts a spec
from those (see [Terraform](terraform.md#bringing-a-terraform-project-in)).
