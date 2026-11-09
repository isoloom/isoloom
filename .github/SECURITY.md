# Security policy

## Supported versions

| Version | Security fixes |
|---|---|
| 2.x, the latest release | Yes, in a new patch or minor release |
| 1.x, the last release (1.0.9) | For three months after 2.0.0 is released |
| Anything older | No: upgrade |

Fixes are not backported to earlier minor releases: upgrading within a major version is meant to
be safe. See [supported releases](../docs/reference/policies.md#supported-releases).

## Reporting a vulnerability

Please don't open a public issue, pull request or discussion for a vulnerability. Report it
privately through GitHub: [**Report a vulnerability**](https://github.com/isoloom/isoloom/security/advisories/new)
(the repository's *Security* tab). The report, and the conversation that follows, are visible
only to you and the maintainers until an advisory is published.

A useful report says:

- the Isoloom version (`isoloom --version`) and the target involved, if any;
- what someone exploiting it can do, and what they need first (a spec they wrote, access to the
  host, a position on the network);
- the smallest spec or steps that show it;
- whether you have told anyone else, and any date you plan to publish.

Remove real secrets from what you send.

## What happens next

| Step | Target |
|---|---|
| Acknowledgement of your report | 3 working days |
| First assessment (confirmed or not, a severity estimate) | 10 working days |
| Fix released, for a confirmed critical or high severity issue | 30 days |
| Fix released, other confirmed issues | the next release, or 90 days at most |

We keep you informed as the fix progresses and agree the publication date with you. The fix ships
with a GitHub security advisory (with a CVE where it applies), crediting you unless you prefer
not. If a fix needs longer (a change in an upstream tool, say), we tell you why and agree a new
date. A vulnerability already being exploited is fixed and announced as fast as we can.

## Scope

In scope:

- **Generated output.** Files Isoloom writes under `.isoloom/` (Compose files, Kubernetes
  manifests and the Helm chart, Vagrantfiles, Terraform modules, scripts, check runners, the run
  manifest) that leave an environment less isolated or less protected than its spec says. For
  example: a service reachable from a network `reach` doesn't allow, a port published beyond the
  host's loopback (or beyond `allowed_cidr` on a cloud) when the spec doesn't ask for it, an
  `internet: false` network that reaches the internet once sealed, a firewall rule wider than the
  spec's, or a secret's value written into a generated file or the run manifest.
- **The command line.** `isoloom` itself: code execution through a spec, a profile, a defaults
  file or an imported file beyond what [the trust model](../docs/explanation/security.md#trust-model)
  says, path traversal outside the project, and unsafe handling of `~/.isoloom/`.
- **The distribution.** The release workflow and binaries, `packaging/install.sh`, the GitHub
  Action (`action.yml`), and the published crates.

## Out of scope

- Vulnerabilities in the tools that run the generated files (Docker, Compose, Kubernetes, Helm,
  Vagrant and its providers, Terraform and its providers, Ansible, OpenSSH) or in the images and
  boxes an environment runs: report those upstream. If Isoloom's output turns an upstream
  weakness on where it would otherwise be off, that is in scope.
- What a runner does with the run manifest: report it to the runner (Isothrum's policy is in
  [its repository](https://github.com/Isothrum/isothrum/security)).
- What a spec asks for on purpose: a machine given wide `reach`, published ports,
  `ISOLOOM_PUBLISH_ADDRESS=0.0.0.0`, privileged containers, a literal password in a `user` step.
  Isoloom builds what the spec says.
- The documented defaults [security considerations](../docs/explanation/security.md) describes:
  public IPs on cloud VMs, `proxmox_insecure`, plain-HTTP WinRM on private networks.
- Code the project brings: provisioning scripts, playbooks, checks, Dockerfiles, and the Terraform
  module of `cloud-services`.
- Someone who already controls the computer running `isoloom`, or the cloud account, Proxmox
  server or ESXi host it targets.
- Environments built to be vulnerable on purpose: their weaknesses are their point.
- Denial of service by a spec that asks for more resources than the host has.
- Findings from automated scanners with no demonstrated impact.

## Security model, briefly

The full model: [security considerations](../docs/explanation/security.md).

- **Isoloom converts and runs nothing.** It reads the project and writes files, opens no network
  connection and has no telemetry.
- **Secrets never land in a file.** A secret or input is a slot (a name and a destination) in the
  run manifest; its value comes from its store at run time and reaches the machine as a file, over
  SSH standard input, never in a generated file, the resolved snapshot or a command line.
- **Published ports stay local.** Compose and Vagrant publish on `127.0.0.1`; Kubernetes publishes
  as ClusterIP; a cloud VM accepts SSH, WinRM and published ports only from `allowed_cidr`, which
  has no default.
- **`internet: false` holds.** On clouds the network is sealed once the machines are set up, the
  seal fails closed, and every later apply keeps it.
- **Cloud runs expire.** Every resource is tagged with the run's expiry, the machines shut
  themselves down at that time, and a reaper destroys what is past it.
- **Signed releases.** Binaries come with checksums, a Sigstore signature, build provenance and an
  SBOM ([verify a release](../docs/guide/verify-a-release.md)); the crates forbid `unsafe` code.
