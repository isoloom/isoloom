# Generate offline

## Generating needs no network

`isoloom validate`, `generate`, `check`, `targets`, `matrix` and the other commands read the
project folder and your defaults file, nothing else. The binary has no HTTP client; the service catalogue is embedded
in it at build time; the images and boxes it names come from its built-in tables. So generation:

- works on an air-gapped machine, or in a CI job with no egress;
- is deterministic: the same spec, profile, defaults and `isoloom` version give the same files,
  byte for byte ([determinism](../explanation/determinism.md));
- sends nothing anywhere: Isoloom has no telemetry.

Two commands start another program: `isoloom graph -o <file>.svg` runs `d2` or `dot` to draw the
diagram, and `isoloom import vagrant` runs Ruby to read a Vagrantfile.

To check it yourself, run `isoloom generate` in a sandbox that denies the network (macOS:
`sandbox-exec -p '(version 1)(allow default)(deny network*)' isoloom generate`; Linux:
`unshare -rn isoloom generate`).

## Running without internet access

Running the generated files is another matter: the tools download what the environment needs.
Where the internet is blocked, point each at an internal source:

| What is downloaded | From | Offline |
|---|---|---|
| Container images | Docker Hub and other registries | `constraints.registries` to a mirror ([corporate network](corporate-network.md)), or pre-load the images |
| OS packages in VMs and builds | Debian, Ubuntu archives, PyPI, npm | `constraints.mirrors` |
| Vagrant boxes | Vagrant Cloud | your box server or files ([golden images](golden-images.md): `box_server_url`, `box_url`) |
| Cloud and Proxmox OS images | the clouds' catalogues, the distributions' image sites | your images in the image table |
| Terraform providers | the Terraform Registry | a provider mirror: `provider_installation { network_mirror { ... } }` or `filesystem_mirror` in your Terraform CLI configuration |
| Ansible collections (VM targets) | Ansible Galaxy, pinned by `sha256` | a Galaxy mirror the controller can reach |

The spec's `internet: false` networks are different: they cut the environment's machines off the
internet once they are set up. The set-up itself still downloads, through the mirrors above.
