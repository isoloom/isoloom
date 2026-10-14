# Isoloom and other tools

Isoloom describes an environment once (its machines, networks, who reaches whom, the ports they
answer on) and produces it on many targets, with the same checks proving the same behavior on
each. Most of what it produces is another tool's input: a Compose file, Kubernetes manifests, a
Vagrantfile, a Terraform module. So these pages are less "instead of" than "with, and when".

Each page says what the other tool does better, what Isoloom adds, when to use which, and how to
bring an existing project in:

- [Docker Compose](docker-compose.md)
- [Vagrant](vagrant.md)
- [Terraform and OpenTofu](terraform.md)
- [Pulumi](pulumi.md)
- [kind](kind.md)
- [Testcontainers](testcontainers.md)

What each target builds and refuses is in [Targets](../targets.md), and what Isoloom produces of
each format is in [Coverage](../COVERAGE.md). If a page here says something those two don't
back, the page is wrong: please open an issue.
