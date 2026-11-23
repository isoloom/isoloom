# What Isoloom is, and what it is not

Isoloom is one of three products that are often used together. Each has a boundary the others
rely on. This page states Isoloom's.

## What it is

A converter. Isoloom reads one environment spec (`isoloom.yml`) and writes each target's native
files: Docker Compose, Kubernetes manifests, Vagrantfiles, Terraform modules, Ansible inventories
and playbooks. Converting is all it does.

## What it is not

- **Not a runner.** Isoloom never starts a tool, a container or a VM, and it knows nothing about
  how its files are run: by hand, from CI, or by a runner such as
  [Isothrum](https://github.com/Isothrum/isothrum).
- **Not shaped by one consumer.** The contract between the generated files and whatever runs them
  is the same for every spec: the native files, and the documented names in
  `isoloom_spec::contract` (see [Architecture](architecture.md)). No spec gets a special case for
  one runner or one app.
- **Not responsible for a spec's own inputs.** When a given spec's generated files need an
  environment variable, a Terraform variable or a secret to run, the spec's author documents it
  and whoever runs the files supplies it. Isoloom doesn't carry a value for one environment.

## Its boundary with the other products

| Product | What it is | What it may assume of Isoloom |
| --- | --- | --- |
| [Isothrum](https://github.com/Isothrum/isothrum) | A general-purpose runtime for Compose, Terraform and Vagrant projects, whoever wrote them | Nothing: it runs Isoloom's output like any other project of those formats, and depends on no Isoloom crate |
| [Cyber CTF launcher](https://github.com/Cyber-Courses/Cyber-CTF-Launcher) | A desktop app that runs capture-the-flag labs, a thin layer over Isothrum | It uses Isoloom's crates to turn a lab's spec into native files, then hands those files to Isothrum |

A change that would make Isoloom's output depend on who runs it belongs in the spec (an input
its author declares) or in the runner, not in Isoloom.
