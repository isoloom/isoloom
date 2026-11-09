# Use your own (golden) images

By default each OS boots from a public image: a Vagrant box from Vagrant Cloud, a cloud image the
Proxmox server downloads, each cloud's official image. Many organizations require their own
hardened, scanned images instead. The image table replaces the built-in ones, per OS and per
target, without touching the spec.

## Write the table

```yaml
# images.yml
os:
  debian@12:
    vagrant: corp/debian-12                       # a box name
    box_server_url: https://boxes.corp.example    # your box server, instead of Vagrant Cloud
    proxmox: "9000"                               # a template VM's id on the node (cloned), or an image URL
    aws: ami-0123456789abcdef0                    # an AMI in the module's region
    azure: /subscriptions/.../images/debian-12-corp
    gcp: projects/corp-images/global/images/family/debian-12-corp
  windows-server@2022:
    vagrant: corp/windows-2022
    winrm: ssl                                    # the box answers WinRM over HTTPS
access:
  docker: registry.corp.example/tools/access:1    # the access machine's image, when the spec leaves it to the runner
  vm: debian@12
```

Every key and its format: [configuration](../reference/configuration.md#the-image-table).

## Use it

For one command:

```sh
isoloom generate --images images.yml
```

For every command, as a defaults layer (yours or the project's): put the same content under
`images:` in `~/.isoloom/defaults.yml` or `isoloom.defaults.yml`. `isoloom defaults` shows which
layer each image comes from.

## What your images need

- **VMs** are set up over SSH (Windows: WinRM or OpenSSH) by cloud-init and the machine's steps:
  keep cloud-init (or cloudbase-init on Windows) and, on Proxmox and ESXi, the guest agent
  (`qemu-guest-agent`, `open-vm-tools`).
- **Containers** come from the spec's images (`app.image`), the catalogue's (pinned by digest), or
  the OS's base image for a build. To pull them from your registry, mirror the public registries
  with `constraints.registries` ([corporate network](corporate-network.md)) rather than renaming
  images in the spec.
- Your image must be the OS the spec names: an image table entry for `debian@12` that boots
  Ubuntu breaks the steps written for Debian.

ESXi takes its images as a module variable (`images`, an OVA per OS) rather than from the table:
[ESXi](targets/esxi.md).
