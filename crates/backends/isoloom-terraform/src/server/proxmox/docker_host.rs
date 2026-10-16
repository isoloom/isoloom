//! The Docker host on a Proxmox server, for `docker-vm` (`.isoloom/docker-vm/proxmox/`): one
//! Debian VM on the uplink bridge (DHCP), its address from the guest agent. Terraform makes only
//! the VM. Its cloud-init document lets the `isoloom` user in with the runner's key, installs the
//! guest agent (it reports the address), runs the constraints' prelude at every boot (the packages
//! may need the proxy), and has SSH accept the values given at launch. Everything inside the VM is
//! the run manifest's ([`manifest`]): `setup.sh` (Docker) piped over SSH, the project's archive,
//! then the Compose file started as Compose's backend says, all through the runner, so the same
//! steps run from macOS, Windows and Linux and Terraform never reaches the guest.

pub use super::manifest::docker_vm::docker_vm as manifest;

use isoloom_plan::Environment;
use isoloom_render::container::HostSize;
use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, boolean, list, number, reference, string};
use isoloom_render::run_manifest::guest::{self, GuestCompose};
use isoloom_render::{GeneratedFile, OUTPUT_DIR};
use isoloom_spec::model::Target;

use super::base::{connection, download, provider};
use super::machine::snippet;
use crate::Result;
use crate::machine::cloud_init::{self, CloudConfig};
use crate::module::providers::{self, terraform};
use crate::module::variable::Variable;
use crate::module::{self, ProjectCopy, Usage, output, resource};
use crate::server::{slotted, user};

/// The module's folder under `.isoloom/`.
pub(super) fn folder() -> String {
    format!("{}/proxmox", Target::DockerVm.id())
}

/// The Docker host's set-up, beside the module: the run manifest pipes it into `sudo sh -s`.
pub(super) const SETUP: &str = "setup.sh";

/// Where SSH learns which variables it accepts from a session (the values given at launch).
const ACCEPT_ENV_FILE: &str = "/etc/ssh/sshd_config.d/isoloom-env.conf";

/// The module and the set-up script, for Compose running in the VM as `guest` says.
///
/// # Errors
///
/// [`crate::Error::Module`] when the module can't be written.
pub fn module(env: &Environment, guest: &GuestCompose) -> Result<Vec<GeneratedFile>> {
    let folder = folder();
    let mut m = module::start(&Usage {
        apply: " -var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=<user@realm!name=secret> -var ssh_public_key=\"$(cat ~/.ssh/id_ed25519.pub)\"",
        key: None,
        after: "# Then:   setup.sh piped into `sudo sh -s` over SSH, the project in /opt/isoloom, and Compose (run.json)\n",
        ..Usage::with_key(&folder, "")
    });
    m.block(terraform(&[providers::PROXMOX]));
    m.blocks(variables().into_iter().map(Variable::block));
    m.block(provider());
    m.commented("The VM's address on the uplink, from the guest agent (not the loopback).", locals());
    m.block(download("debian", DEBIAN, "-docker-debian-12.img"));
    m.block(snippet("env", "docker", cloud_config(env, guest).document()));
    m.block(vm(env));
    m.blocks(vec![output("ip", reference("local.ip")), output("ssh_user", string(USER))]);
    let main = module::file(Target::DockerVm, &folder, m, ProjectCopy::Runner)?;
    Ok(vec![main, setup(env)])
}

/// The SSH user the runner's key lets in (it has `sudo`).
pub(super) const USER: &str = "isoloom";

/// `setup.sh`: the Docker host's set-up ([`guest::setup_script`]).
fn setup(env: &Environment) -> GeneratedFile {
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{}/{SETUP}", folder()),
        contents: guest::setup_script(env),
    }
}

/// Debian 12's cloud image.
const DEBIAN: &str = "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2";

/// The module's variables: the server, where the VM goes, its slot on the server, and the key the
/// `isoloom` user takes.
pub(super) fn variables() -> Vec<Variable> {
    let s = |name: &str| Variable::new(name, "string");
    let mut v = connection("Uploading the cloud-init snippet goes over SSH to the node");
    v.extend([
        super::base::node(),
        s("datastore").default(string("local-lvm")).describe("Where the VM's disk goes"),
        s("image_datastore")
            .default(string("local"))
            .describe("A datastore with 'iso' content, for the cloud image"),
        s("snippets_datastore")
            .default(string("local"))
            .describe("A datastore with 'snippets' content, for cloud-init"),
        s("uplink_bridge")
            .default(string("vmbr0"))
            .describe("The bridge the VM gets its address (DHCP) and the internet from"),
        Variable::new("slot", "number")
            .default(number(1))
            .describe("1 to 99, unique per environment on this server"),
        s("ssh_public_key").default(string("")).describe("Installed for the user `isoloom`"),
    ]);
    v
}

/// The VM's address on the uplink: the guest agent's that isn't the loopback.
fn locals() -> Block {
    let address = super::uplink::first_address("proxmox_virtual_environment_vm.env", &[]);
    Block::builder("locals").add_attribute(("ip", address)).build()
}

/// The cloud-init document: the `isoloom` user with the runner's key, the guest agent (it reports
/// the address), the constraints' prelude at every boot, and SSH accepting the values `guest`
/// forwards (written before `sshd` starts, and reloaded once more after).
fn cloud_config(env: &Environment, guest: &GuestCompose) -> CloudConfig {
    let mut config = CloudConfig::new(&env.name, list([user(list([reference("var.ssh_public_key")]))]));
    config.packages = vec!["qemu-guest-agent".to_owned(), "curl".to_owned()];
    config.bootcmd.extend(cloud_init::prelude(env));
    let forwarded = guest.forwarded();
    if !forwarded.is_empty() {
        let accept = format!("AcceptEnv {}\n", forwarded.join(" "));
        config.files.push(cloud_init::file(ACCEPT_ENV_FILE, Some("0644"), string(accept)));
        config.runcmd.push("systemctl reload ssh || systemctl reload sshd || true".to_owned());
    }
    config.runcmd.push("systemctl enable --now qemu-guest-agent".to_owned());
    config
}

/// `<name> { <attributes> }`.
fn settings(name: &str, attributes: Vec<(&str, Expression)>) -> Block {
    attributes.into_iter().fold(Block::builder(name), BlockBuilder::add_attribute).build()
}

fn vm(env: &Environment) -> Block {
    let size = HostSize::of(env);
    let ip_config = Block::builder("ip_config")
        .add_block(settings("ipv4", vec![("address", string("dhcp"))]))
        .build();
    resource("proxmox_virtual_environment_vm", "env")
        .add_attribute(("name", slotted(&format!("-{}", env.name))))
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("tags", list([string("isoloom"), string(env.name.as_str())])))
        .add_attribute(("on_boot", boolean(false)))
        .add_block(super::uplink::agent())
        // A missing agent must not hold a destroy: the VM is stopped, not shut down.
        .add_attribute(("stop_on_destroy", boolean(true)))
        .add_block(settings("cpu", vec![("cores", number(size.cpus.into())), ("type", string("host"))]))
        .add_block(settings("memory", vec![("dedicated", number(size.memory_mb.into()))]))
        .add_block(settings(
            "disk",
            vec![
                ("datastore_id", reference("var.datastore")),
                ("file_id", reference("proxmox_download_file.debian.id")),
                ("interface", string("virtio0")),
                ("size", number(30)),
            ],
        ))
        .add_block(settings("network_device", vec![("bridge", reference("var.uplink_bridge"))]))
        .add_block(
            Block::builder("initialization")
                .add_attribute(("datastore_id", reference("var.datastore")))
                .add_attribute(("user_data_file_id", reference("proxmox_virtual_environment_file.env.id")))
                .add_block(ip_config)
                .build(),
        )
        .add_block(settings("operating_system", vec![("type", string("l26"))]))
        .add_block(Block::builder("serial_device").build())
        .add_block(super::uplink::checked("The Docker host", &[]))
        .build()
}
