# Security policy

## Supported versions

| Version | Security fixes |
|---|---|
| 1.x | Yes: the latest 1.x release gets the fix, in a patch release |
| 0.x | No: upgrade to 1.x |

Until 1.0 is released, only the latest 0.x release is fixed. Fixes are not backported to older
minor releases: upgrading within a major version is meant to be safe.

## Reporting a vulnerability

Please don't open a public issue, pull request or discussion for a vulnerability.

Report it privately through GitHub: on the repository's **Security** tab, choose **Report a
vulnerability** (GitHub's private vulnerability reporting). The report, and the conversation
that follows, are visible only to you and the maintainers until an advisory is published.

> **Maintainers:** private vulnerability reporting is off until it is switched on in the
> repository's settings (**Settings > Code security > Private vulnerability reporting**). Until
> then the button above doesn't appear.

A useful report says:

- the Isoloom version (`isoloom --version`) and the target involved, if any;
- what an attacker can do, and what they need first (a spec they wrote, access to the host, a
  position on the network);
- the smallest spec or steps that show it;
- whether you have told anyone else, and any date you plan to publish.

## What happens next

| When | What |
|---|---|
| 3 working days | We acknowledge the report. |
| 10 working days | We confirm it or explain why we don't consider it a vulnerability, and give a first severity estimate. |
| 90 days at most | A fixed release and a published GitHub security advisory, crediting you unless you prefer not. |

We keep you informed as the fix progresses, and agree the publication date with you. If a fix
needs longer than 90 days (a change in an upstream tool, say), we tell you why and agree a new
date. A vulnerability being exploited already is fixed and announced as fast as we can.

## Scope

In scope:

- **Generated output.** Files Isoloom writes under `.isoloom/` (Compose files, Kubernetes
  manifests and the Helm chart, Vagrantfiles, Terraform modules, scripts, check runners) that
  leave an environment less isolated or less protected than its spec says. For example: a
  service reachable from a network `reach` doesn't allow, a port published beyond the host's
  loopback when the spec doesn't ask for it, a firewall rule wider than the spec's, or a secret's
  value written into a generated file.
- **The command line.** `isoloom` itself: command or code injection through a spec, a profile
  or a project file, path traversal outside the project, unsafe handling of credentials or of
  `~/.isoloom/`, and the install script (`packaging/install.sh`) and release binaries.
- **The runtime.** What Isoloom does while an environment runs: reading secrets from their
  stores and delivering them to machines, the commands it runs over SSH, WinRM or the target's
  tool, and the registry (`~/.isoloom/status.yml`).

## Out of scope

- Vulnerabilities in the tools Isoloom drives (Docker, Compose, Kubernetes, Helm, Vagrant and
  its providers, Terraform and its providers, Ansible, OpenSSH) or in the images and boxes an
  environment runs: report those upstream. If Isoloom's output turns an upstream weakness on
  where it would otherwise be off, that is in scope.
- What a spec asks for on purpose: a machine given wide `reach`, published ports, privileged
  containers, or a weak password in a project file. Isoloom builds what the spec says.
- Code the project brings: provisioning scripts, playbooks, checks, Dockerfiles, and the
  Terraform module of `cloud-services`.
- An attacker who already controls the machine running `isoloom`, or the cloud account,
  Proxmox server or ESXi host it targets.
- Environments built to be vulnerable (a training lab, say): their weaknesses are their point.
- Denial of service by a spec that asks for more resources than the host has.
- Findings from automated scanners with no demonstrated impact.
