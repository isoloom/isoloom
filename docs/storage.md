# Storage: volumes, tmpfs and data disks

A machine keeps data in three places beside its own disk: volumes, memory-backed mounts
(`tmpfs`) and data disks (`disks`). Every target renders what it can and refuses the rest with
the reason, so `isoloom targets` shows which targets take a given spec.

## Volumes

A machine's `volumes` map a name to an absolute path. A volume declared at the top level
(`volumes:`) is shared by the machines that mount it: one named volume on Docker, one claim on
Kubernetes, and on the VM targets the owner's directory served over NFS to the others.

```yaml
volumes:
  site: { owner: writer, persistent: true }
  uploads: { external: prod-uploads }

machines:
  writer:
    volumes: { site: /srv/site }
  web:
    volumes:
      site: { path: /usr/share/nginx/html, subpath: public }
      uploads: /srv/uploads
```

| Field | What it does |
| --- | --- |
| `persistent: true` | `isoloom down` keeps the volume and the next `run` finds it again. `isoloom down --volumes` removes it. |
| `external: <name>` | An existing Docker volume or Kubernetes claim (in the environment's namespace), mounted by that name and never created or removed. |
| `subpath` (on a mount) | One folder of a shared volume instead of all of it, created when missing. The volume's owner still mounts all of it. |

`persistent` and `external` need storage that outlives the machines, so only Docker (Compose)
and Kubernetes take them. A Compose file run on a VM (`docker-vm`, `cloud-docker`) and the VM
targets refuse them, since their storage goes with the VM.

### Folders of the Proxmox server (`virtiofs`)

On Proxmox, a shared volume can be a folder of the server instead, shared into each VM that
mounts it through virtiofs (Proxmox VE 8.4 or later). No machine serves it: the VMs read and
write the server's folder directly, and the data stays on the server when the environment goes.
Every other target ignores the setting and shares the volume as usual.

```yaml
volumes:
  site: { owner: writer, virtiofs: {} }                 # the defaults
  media: { virtiofs: { mapping: lab-media, cache: never, xattr: true } }
```

| Field | What it does |
| --- | --- |
| `mapping` | An existing directory mapping of the cluster, by its id. Without it, the module creates one, `iso<slot>-<volume>`, for `<virtiofs_root>/<environment>/<volume>` on the node (`virtiofs_root` is a module variable, `/srv/isoloom` by default). |
| `cache` | How the guests cache the folder: `auto` (the default, another VM's changes show within a second), `always`, `metadata` or `never`. |
| `direct_io` | Honor `O_DIRECT`, for databases that manage their own cache. Off by default. |
| `xattr` | Pass extended attributes through. Off by default. |
| `acl` | Pass POSIX ACLs through, which implies `xattr`. Off by default. |

**On the cluster, first.** Proxmox shares a folder, it doesn't create one. With the module's own
mapping, make the folder on the node before `terraform apply`:

```sh
ssh root@pve mkdir -p /srv/isoloom/<environment>/<volume>
```

Creating the mapping needs the `Mapping.Modify` privilege on `/mapping/dir` (root, or a token
given it). With `mapping`, create it yourself instead: Datacenter, Resource Mappings, Directory
mappings, an id and the folder on each node (or `pvesh create /cluster/mapping/dir --id
lab-media --map node=pve,path=/srv/media`). The VMs need `Mapping.Use` on it.

Each VM mounts the folder by its tag (the mapping's id) from `/etc/fstab`, at boot too; a
`subpath` mounts the whole folder under `/var/lib/isoloom/volumes/` and binds its own folder at
the path. Refused: an LXC container mounting it (a container would take a bind mount, which
comes later), a Windows VM (its virtio-fs driver and WinFsp aren't installed), and more than
8 such folders on one VM. Not run on a real host.

## Memory-backed mounts

A `tmpfs` entry is a path, or a path with a size and a mode:

```yaml
tmpfs:
  - /tmp
  - { path: /run/app, size: 64m, mode: "1777" }
```

Quote the mode: YAML reads `0700` unquoted as a number. Compose writes a long-form tmpfs mount,
and a VM sets the mount up in its fstab. Kubernetes' memory volumes take a size but no mode, so
Kubernetes refuses a `mode`.

## Data disks

`disks` gives a machine data disks beside its own, by name. The name, kebab-case and at most 12
characters, is the filesystem's label.

```yaml
machines:
  db:
    vm: { os: debian@12, provision: [provision/db.sh] }
    disks:
      data: { size_gb: 50, fs: xfs, mount: /var/lib/postgresql }
      scratch: { size_gb: 10, fs: raw }
```

| Field | What it does |
| --- | --- |
| `size_gb` | The disk's size, 1 to 16384 GB. |
| `fs` | `ext4` (the Linux default), `xfs`, `ntfs` (the Windows default), or `raw` for a block device left unformatted. |
| `mount` | An absolute path on Linux and in containers, a drive letter (`E:`) on Windows. Required unless the disk is `raw`. |

On a VM the machine's set-up waits for each blank disk of its size, formats it with its name as
the label and mounts it at boot (fstab on Linux, a drive letter on Windows). A raw disk is left
as it is. The disks go with the machine: `down` removes them.

| Target | What a data disk becomes |
| --- | --- |
| Docker (Compose), `docker-vm`, `cloud-docker` | A named volume at the mount. A raw disk is refused: a container gets no block device. |
| `hybrid` | An empty disk on its VMs, as on Vagrant, and a named volume on its containers. A raw disk is refused. |
| Kubernetes | A claim of that size, mounted at the path; a raw disk is a block-mode claim. A tmpfs `mode` is refused. |
| Vagrant | An empty disk: `config.vm.disk` on VirtualBox, VMware and Hyper-V, the provider's own setting on libvirt (`storage`) and ESXi (`guest_storage`). The `qemu`, `parallels` and `utm` providers refuse it. |
| Proxmox | A `disk` block per data disk on the VM (its name as the serial), set up by cloud-init (`disk_setup`, `fs_setup`, `mounts`). An LXC container (`docker.lxc`) refuses it. |
| Clouds (`cloud-vm`) | A block volume per disk, attached to the VM. See [the cloud modules](cloud-modules.md#data-disks-disks) for each cloud's resource and smallest volume. |
| `esxi` | An `esxi_virtual_disk` per data disk (thin, on `disk_store`), attached to the VM by SCSI slot (`0:1` on, at most 14), then formatted and mounted by the machine's set-up, found by its size. Not run on a real host. |
| `hosted`, `external`, `cloud-services` | Refused: Isoloom creates no machine to attach a disk to. |
