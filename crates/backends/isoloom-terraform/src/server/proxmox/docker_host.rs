//! The Docker host on a Proxmox server, for `docker-vm` (`.isoloom/docker-vm/proxmox/main.tf`):
//! one Debian VM on the uplink bridge (DHCP), its address from the guest agent, then the same
//! set-up over SSH as the clouds (`crate::docker_host`). Its cloud-init document is the VM
//! core's, with the plan's `Prelude` step as `bootcmd` (the packages it installs may need the
//! proxy).

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_render::container::HostSize;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Structure, Template, binary, boolean, call, conditional, index, list, null, number, reference,
    string,
};
use isoloom_spec::model::Target;

use super::base::{connection, download, is_set, is_unset, provider};
use super::machine::snippet;
use crate::Result;
use crate::cloud::operator;
use crate::docker_host::{self, Access, AutoStop};
use crate::machine::cloud_init::{self, CloudConfig};
use crate::module::providers::{self, terraform};
use crate::module::variable::Variable;
use crate::module::{self, ProjectCopy, Usage, resource};
use crate::server::{slotted, user};

/// The module.
///
/// # Errors
///
/// [`crate::Error::Module`] when the module can't be written.
pub fn module(env: &Environment) -> Result<GeneratedFile> {
    let folder = format!("{}/proxmox", Target::DockerVm.id());
    let mut m = module::start(&Usage::with_key(
        &folder,
        "-var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=… ",
    ));
    m.block(terraform(&[providers::PROXMOX, providers::LOCAL]));
    m.blocks(variables());
    m.blocks(operator::host_variables(env));
    m.block(provider());
    m.commented("The VM's address on the uplink, from the guest agent (not the loopback).", locals());
    m.block(download("debian", DEBIAN, "-docker-debian-12.img"));
    m.block(snippet("env", "docker", cloud_config(env).document()));
    m.block(vm(env));
    let access = Access {
        host: reference("local.ip"),
        user: "isoloom",
        replace_on: reference("proxmox_virtual_environment_vm.env.id"),
        auto_stop: AutoStop::SetUp,
    };
    let mut setup = docker_host::setup(env, &access);
    through_node(&mut setup);
    m.commented("The environment, over SSH: the project, Docker, then the Compose file.", setup);
    m.blocks(docker_host::outputs(&access));
    module::file(Target::DockerVm, &folder, m, ProjectCopy::Archive)
}

/// Debian 12's cloud image.
const DEBIAN: &str = "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2";

fn variables() -> Vec<Block> {
    let s = |name: &str| Variable::new(name, "string");
    let mut v = connection("Uploading the cloud-init snippet goes over SSH to the node");
    v.extend([
        Variable::new("ssh_via_node", "bool")
            .default(boolean(false))
            .describe("Reach the lab VM over SSH through the node (its bridge isn't routable from here)")
            .block(),
        super::base::node(),
        s("datastore").default(string("local-lvm")).describe("Where the VM's disk goes").block(),
        s("image_datastore")
            .default(string("local"))
            .describe("A datastore with 'iso' content, for the cloud image")
            .block(),
        s("snippets_datastore")
            .default(string("local"))
            .describe("A datastore with 'snippets' content, for cloud-init")
            .block(),
        s("uplink_bridge")
            .default(string("vmbr0"))
            .describe("The bridge the VM gets its address (DHCP) and the internet from")
            .block(),
        Variable::new("slot", "number")
            .default(number(1))
            .describe("1 to 99, unique per environment on this server")
            .block(),
    ]);
    v
}

/// The project's root, and the VM's address on the uplink: the guest agent's that isn't the
/// loopback.
fn locals() -> Block {
    let addresses = super::uplink::addresses("proxmox_virtual_environment_vm.env", &[]);
    let root = call(
        "abspath",
        [Template::new().with_expr(reference("path.module")).with_text("/../../..").into_expression()],
    );
    Block::builder("locals")
        .add_attribute(("root", root))
        .add_attribute(("ip", index(addresses, 0)))
        .build()
}

/// The cloud-init document: the `isoloom` user with the operator's key, the guest agent (it
/// reports the address), and the constraints' prelude at every boot.
fn cloud_config(env: &Environment) -> CloudConfig {
    let mut config = CloudConfig::new(&env.name, list([user(list([reference("var.ssh_public_key")]))]));
    config.packages = vec!["qemu-guest-agent".to_owned(), "curl".to_owned()];
    config.bootcmd.extend(cloud_init::prelude(env));
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

/// `var.ssh_via_node && <cond> ? <yes> : null`.
fn via_node(cond: Option<Expression>, yes: Expression) -> Expression {
    let via = reference("var.ssh_via_node");
    let cond = cond.map_or(via.clone(), |c| binary(via, BinaryOperator::And, c));
    conditional(cond, yes, null())
}

/// The VM's bridge may not be routable from where Terraform runs (a host-internal NAT bridge,
/// an isolated network): with `ssh_via_node`, the set-up's connection goes through the node,
/// logged in as the snippet's upload is.
fn through_node(setup: &mut Block) {
    let node = conditional(
        is_set("proxmox_ssh_address"),
        reference("var.proxmox_ssh_address"),
        call(
            "join",
            [
                string(""),
                call(
                    "compact",
                    [call(
                        "regex",
                        [string(r"^https?://(?:\[([^]]+)\]|([^/:]+))"), reference("var.proxmox_endpoint")],
                    )],
                ),
            ],
        ),
    );
    let key = || is_set("proxmox_ssh_private_key_file");
    let bastion = [
        ("bastion_host", via_node(None, Expression::Parenthesis(Box::new(node)))),
        ("bastion_user", via_node(None, reference("var.proxmox_ssh_username"))),
        (
            "bastion_password",
            via_node(Some(is_unset("proxmox_ssh_private_key_file")), reference("var.proxmox_password")),
        ),
        (
            "bastion_private_key",
            via_node(Some(key()), call("file", [reference("var.proxmox_ssh_private_key_file")])),
        ),
    ];
    for s in &mut setup.body.0 {
        if let Structure::Block(b) = s
            && b.identifier() == "connection"
        {
            for (k, v) in bastion.clone() {
                b.body.0.push(Structure::Attribute((k, v).into()));
            }
        }
    }
}
