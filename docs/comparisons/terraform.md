# Isoloom and Terraform or OpenTofu

[Terraform](https://www.terraform.io/) and [OpenTofu](https://opentofu.org/) describe
infrastructure as code for almost any provider, keep its state, and change it with a plan you
review. Isoloom's `proxmox`, `esxi`, `cloud-vm`, `cloud-docker` and `cloud-services` targets write
Terraform modules and run them with `terraform`, so Terraform is one of the tools Isoloom drives.

## What Terraform and OpenTofu do better

- **Any infrastructure.** Thousands of providers: DNS, databases, identity, monitoring, SaaS.
  Isoloom builds machines and networks on a few platforms, and for anything else applies a
  module of yours (`cloud-services`).
- **Long-lived production infrastructure.** State, `plan` before `apply`, imports of existing
  resources, drift detection, workspaces, remote backends with locking, policy as code. Isoloom
  environments are meant to be created, tested and torn down; a cloud module can shut its
  machines down after a while (`auto_stop_minutes`).
- **The full provider schemas.** Every argument of every resource is yours to set. Isoloom writes
  what a portable spec needs; Coverage lists every resource type of each
  provider it targets and what it does with it.
- **Modules and reuse,** a registry of them, and a language (HCL with expressions, `for_each`,
  `count`) for composing them.

## What Isoloom adds

- **One spec, several platforms.** The same environment runs on Proxmox, ESXi, six clouds (AWS,
  Azure, Google Cloud, DigitalOcean, Linode, Oracle Cloud), local VMs and, for container
  machines, Docker and Kubernetes. A Terraform configuration is written for its providers.
- **Isolation as intent.** `reach` becomes a router and firewall on Proxmox and ESXi, and
  firewall rules on the clouds, from one line; the derived checks prove it.
- **Machines that come up configured.** Isoloom generates the set-up (cloud-init, steps over SSH
  or WinRM, an Ansible controller for playbooks) and delivers secrets at run time, so the module
  isn't the end of the work.
- **Checks after apply.** `isoloom test` runs the checks from inside the environment.
- **Readable modules to keep.** The generated module is plain HCL under `.isoloom/`, with its
  variables and outputs documented ([Cloud modules](../cloud-modules.md)); run it by hand or from
  a Terraform workflow you already have.

## When to use which

- Use **Terraform or OpenTofu** for production and shared infrastructure, anything that lives for
  months, and anything beyond machines and their networks.
- Use **Isoloom** for environments you create and destroy (test labs, training ranges,
  reproductions, demos), especially when the same environment must also run locally or on
  another platform, and when you want its behavior checked.
- **Both:** `cloud-services` applies a module of yours with Isoloom's lifecycle and checks, and a
  generated module can run in your own pipeline.

`isoloom run` calls the `terraform` binary. The generated modules are ordinary HCL, but running
them with `tofu` is not tested.

## Bringing a Terraform project in

```sh
isoloom import terraform            # reads the .tf files of the current folder
isoloom import terraform infra/ --stdout
```

The files are read statically, with variables, locals, `for_each` and `count` evaluated; nothing
is planned or applied. VMs (`aws_instance`, the Azure Linux and Windows VMs,
`google_compute_instance`, the Proxmox VMs) become machines, subnets become networks, and every
other resource type is listed with what Isoloom does about it. Then:

```sh
isoloom validate && isoloom targets
```

and run the draft on the target you need, or locally first (`isoloom run vagrant`).
