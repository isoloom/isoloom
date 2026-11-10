# Configuration

What changes Isoloom's output besides the spec: the defaults, environment variables, profiles and
the image table. `isoloom defaults` shows the defaults in effect in a project folder, and where
each value comes from.

## Defaults

The defaults hold what belongs to the person or the site rather than to the environment: the
images, the Vagrant provider, each cloud's region, and the tags every resource carries. They are read in layers, each one
overriding the one before:

| Layer | Where |
|---|---|
| Built in | `isoloom defaults --system` prints them |
| Yours | `~/.isoloom/defaults.yml` (`$ISOLOOM_HOME/defaults.yml` when `ISOLOOM_HOME` is set) |
| The project's | `isoloom.defaults.yml`, beside `isoloom.yml` |
| The profile's | the `defaults:` of the profile in use ([profiles](#profiles)) |
| The image table | the file given to `--images` ([the image table](#the-image-table)) |
| The environment | `ISOLOOM_<KEY>`, `__` standing for a dot: `ISOLOOM_CLOUD__AWS__REGION=eu-central-1` |
| The command line | `-s defaults.<key>=<value>`: `-s defaults.cloud.aws.region=eu-central-1` |

The keys:

| Key | Holds |
|---|---|
| `images` | The image table: the image each OS boots from on each target ([below](#the-image-table)) |
| `vagrant.provider` | The Vagrant provider to use |
| `cloud.<cloud>.region` | The region a cloud's modules default to (`aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci`) |
| `tags.<key>` | A tag every cloud resource and Kubernetes object carries, under the spec's own [`tags`](../spec.md#tags) (a team, a cost centre) |

An unknown key is an error. Only the `images`, `vagrant`, `cloud` and `tags` sections are read
from the environment (`ISOLOOM_TAGS__TEAM=platform`; a variable's name is read lowercased, so a
tag key with capitals or a dot is set in a file). A tag's value is text: from the environment or
a `-s` it is taken as written (`ISOLOOM_TAGS__ACCOUNT=0x1F` is `0x1F`); in a file a number or
`true`/`false` is read as its text, so quote one whose form matters (`"0x1F"`, `"1e3"`).

```sh
$ isoloom defaults
cloud.aws.region           eu-west-3                 built in
cloud.azure.region         swedencentral             built in
cloud.gcp.region           europe-west9              built in
cloud.digitalocean.region  fra1                      built in
cloud.linode.region        fr-par                    built in
cloud.oci.region           eu-paris-1                built in
```

`isoloom defaults --json` prints the same for tools.

```yaml
# ~/.isoloom/defaults.yml
cloud:
  aws: { region: eu-central-1 }
  azure: { region: westeurope }
vagrant:
  provider: libvirt
tags:
  team: platform
  cost-center: 1234
```

## Environment variables

Read by `isoloom`:

| Variable | Meaning |
|---|---|
| `ISOLOOM_HOME` | Isoloom's home folder (default `~/.isoloom`), where your `defaults.yml` lives |
| `ISOLOOM_PROFILE` | The profile to use when `--profile` isn't given |
| `ISOLOOM_IMAGES__*`, `ISOLOOM_VAGRANT__*`, `ISOLOOM_CLOUD__*`, `ISOLOOM_TAGS__*` | A defaults layer ([above](#defaults)) |

Read by the generated files when they run (a runner sets them; set them yourself when you run the
files by hand):

| Variable | Read by | Meaning |
|---|---|---|
| `ISOLOOM_PUBLISH_ADDRESS` | Compose | The host address published ports bind to (default `127.0.0.1`) |
| `ISOLOOM_PUBLISH_FIXED` | Compose, Vagrant | `1`: each published port takes its `publish` number, not a free port |
| `ISOLOOM_SECRET_<NAME>` | the generated files | A secret's value, when the spec reads it from the environment ([secrets](../spec.md#secrets)) |
| `ISOLOOM_DERIVED` | the check runners | `0` skips the checks Isoloom derives from the spec (ports, reach, internet), leaving the spec's own |
| `ISOLOOM_HYPERV_SWITCH` | Vagrant | The Hyper-V switch (default `Default Switch`) |
| `ISOLOOM_SSH_PROXY_COMMAND` | Vagrant | An SSH `ProxyCommand` for VMs on a remote ESXi host |
| `ISOLOOM_ACCESS_IMAGE`, `ISOLOOM_ACCESS_BOX` | the run manifest | The image or box a runner gives the access machine when the spec leaves it to the runner |

Other `ISOLOOM_*` names in the generated files (`ISOLOOM_HOSTPORTS`, `ISOLOOM_FILE_*`,
`ISOLOOM_HOST_FOLDER_*`, `ISOLOOM_RETRY_PAUSE`) are set by a runner from the run manifest; you
don't set them.

## Profiles

A profile holds what differs for one customer, site or use of the same environment, so the spec
stays one file. It is `profiles/<name>.yml` in the project (a kebab-case name), used with
`--profile <name>` or `ISOLOOM_PROFILE`:

| Field | Holds |
|---|---|
| `description` | What the profile is for |
| `target` | The target it runs on: the default target, and the only one allowed |
| `spec` | Values merged into the spec (before `-s`) |
| `defaults` | One more defaults layer |
| `inputs` | Launch values (the environment wins over them) |

```yaml
# profiles/customer-a.yml
description: Customer A's site, behind their proxy
target: proxmox
spec:
  constraints:
    proxy: { http: "http://proxy.customer-a.example:3128" }
defaults:
  cloud:
    aws: { region: eu-central-1 }
```

`isoloom profiles` lists the project's profiles and checks each one against the spec; it fails
when one doesn't hold.

## The image table

The image each OS boots from, per target, is built in. Your own table, given with
`--images <FILE>` (or as the `images:` of a defaults layer), replaces entries of it: a golden
image, a box from your own Vagrant server, an AMI of your account
([how-to](../guide/golden-images.md)).

```yaml
# images.yml
os:
  debian@12:
    vagrant: corp/debian-12          # a Vagrant box
    box_url: https://boxes.corp.example/debian-12.json
    proxmox: https://images.corp.example/debian-12-corp.qcow2  # or a template VM's id
    aws: ami-0123456789abcdef0
access:
  docker: registry.corp.example/tools/access:1
  vm: debian@12
```

Keys under each OS: `vagrant`, `vagrant_version`, `box_url`, `box_server_url`, `qemu`,
`hyperv`, `winrm`, `winssh`, `proxmox`, `aws`, `azure`, `gcp`, `digitalocean`, `linode`, `oci`.
`access` names the image (`docker`) or OS (`vm`) of the access machine when the spec leaves it to
the runner (`access: true`, no `os`, nothing to run). A target the table gives none of the form it
runs refuses that machine, so a spec that leaves it to the runner generates only with a table that
names it (`isoloom validate` warns when none does). The fields and their formats: `isoloom schema` and the
[spec reference](../spec.md).

## Overriding a value once

`-s` (`--set`) changes one value for one command, on the spec or the defaults:

```sh
isoloom generate -s machines.web.os=ubuntu@24.04
isoloom generate -s defaults.cloud.aws.region=eu-west-1
```
