# Isoloom and Vagrant

[Vagrant](https://www.vagrantup.com/) builds and manages local virtual machines from a
Vagrantfile, on VirtualBox, VMware, libvirt, Hyper-V, Parallels and more. Isoloom's `vagrant`,
`docker-vm` and `hybrid` targets write a Vagrantfile and run it with `vagrant`, so Vagrant is one
of the tools Isoloom drives.

## What Vagrant does better

- **Anything Ruby can say.** A Vagrantfile is a Ruby program: loops, conditionals, plugins, the
  full provider and provisioner settings. Isoloom writes the settings a portable spec needs;
  Coverage lists each setting and what Isoloom does with it.
- **Its ecosystem.** Many boxes, plugins for providers and for everything around them, and years
  of answers online.
- **The daily VM workflow.** `vagrant ssh`, `vagrant suspend`, snapshots, synced folders of your
  source tree: Vagrant is a comfortable place to develop inside a VM. Isoloom has `connect`,
  `exec` and a baseline snapshot, but its focus is the environment as a whole.
- **Fewer moving parts.** A Vagrantfile of yours makes the VMs you ask for. Isoloom adds a router
  VM to enforce `reach` and `tc`, and a controller VM when the spec has playbooks.

## What Isoloom adds

- **One spec for every provider and beyond.** The generated Vagrantfile carries a block for each
  provider, and the same spec runs on Proxmox, ESXi, a cloud, or (for machines with `docker:`)
  as containers and on Kubernetes. See [Targets](../targets.md).
- **Networks with rules.** A private network per spec network, a router between them that
  enforces `reach` and applies link impairment, and fixed addresses written once.
- **Checks that prove it.** `isoloom test vagrant` runs your checks and the derived ones from
  inside the environment.
- **Validation before boot.** `isoloom validate` names the exact field of a mistake, and
  `isoloom targets --host` says what this machine lacks, before minutes of `vagrant up`.
- **Secrets** delivered from a store at run time, never written into the Vagrantfile
  ([Secrets](../secrets.md)).

## When to use which

- Use **Vagrant** for one VM or a few you develop in, when you need a provider setting or plugin
  Isoloom doesn't write, or when the Vagrantfile's logic is the point.
- Use **Isoloom** for a multi-machine lab with networks and isolation rules, when the same lab
  must also run on a server or in the cloud, or when CI should prove it behaves.
- **Both:** the generated `.isoloom/vagrant/Vagrantfile` is an ordinary one: `vagrant up`,
  `vagrant ssh web` and your plugins work on it as usual.

VirtualBox is the provider run for real; the other providers' blocks are generated and not run
on a real host yet ([Targets](../targets.md#vagrant)).

## Bringing a Vagrant project in

```sh
isoloom import vagrant              # reads ./Vagrantfile
isoloom import vagrant path/to/Vagrantfile --stdout
```

The Vagrantfile runs against a stand-in `Vagrant` module, so its Ruby logic is evaluated and
nothing is created. The settings it makes become the draft (each VM a machine with `vm:`, its OS
read from the box, private networks and addresses, forwarded ports, the provider's CPUs and
memory, provisioners), and the rest is listed with why: a host-side Ansible provisioner becomes
environment-level `provision:`, a DHCP or public network needs a decision from you. Then:

```sh
isoloom validate && isoloom run vagrant && isoloom test vagrant
```

Check the OS the draft read from each box, and anything the notes say belongs in the image or a
playbook.
