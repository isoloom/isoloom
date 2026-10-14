//! The Docker host on a Proxmox server, for `docker-vm` (`.isoloom/docker-vm/proxmox/main.tf`):
//! one Debian VM on the uplink bridge (DHCP), its address from the guest agent, then the same
//! set-up over SSH as the clouds. Its cloud-init document is the VM core's, with the plan's
//! `Prelude` step as `bootcmd` (the packages it installs may need the proxy).

use isoloom_plan::Environment;
use isoloom_render::GeneratedFile;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Module, Structure, Template, binary, boolean, call, conditional, for_list, index, list, null, number,
    object, reference, string,
};

use super::host::{self, Reach, Sizing};
use crate::vm::cloud_init::{self, CloudConfig};
use crate::vm::module::{Variable, terraform};
use crate::vm::versions;

/// `docker-vm`'s folder under `.isoloom/`.
const DIR: &str = "docker-vm";

/// The module.
pub fn render(env: &Environment) -> Result<GeneratedFile, isoloom_render::hcl_builder::Error> {
    let settings = "-var proxmox_endpoint=https://<server>:8006/ -var proxmox_api_token=… ";
    let mut m = Module::new(host::preamble(DIR, "proxmox", settings));
    m.block(terraform(&[versions::PROXMOX, versions::LOCAL]));
    m.blocks(variables());
    m.blocks(host::variables(env, false));
    m.block(crate::proxmox::module::provider());
    m.commented("The VM's address on the uplink, from the guest agent (not the loopback).", locals());
    m.block(image());
    m.block(snippet(env));
    m.block(vm(env));
    let reach = Reach {
        host: reference("local.ip"),
        user: "isoloom",
        replace_on: reference("proxmox_virtual_environment_vm.env.id"),
        auto_stop: true,
    };
    let mut setup = host::setup(env, &reach);
    through_node(&mut setup);
    m.commented("The environment, over SSH: the project, Docker, then the Compose file.", setup);
    m.blocks(host::outputs(&reach));
    host::file(&m, DIR, "proxmox")
}

fn variables() -> Vec<Block> {
    let s = |name: &str| Variable::new(name, "string");
    vec![
        s("proxmox_endpoint").describe("https://<server>:8006/").block(),
        s("proxmox_api_token")
            .default(string(""))
            .sensitive()
            .describe("user@realm!name=secret; or use proxmox_username and proxmox_password")
            .block(),
        s("proxmox_username").default(string("root@pam")).block(),
        s("proxmox_password").default(string("")).sensitive().block(),
        Variable::new("proxmox_insecure", "bool")
            .default(boolean(true))
            .describe("Accept the server's self-signed certificate")
            .block(),
        s("proxmox_ssh_username")
            .default(string("root"))
            .describe("Uploading the cloud-init snippet goes over SSH to the node")
            .block(),
        s("proxmox_ssh_private_key_file").default(string("")).block(),
        s("proxmox_ssh_address")
            .default(string(""))
            .describe("The node's SSH address, when the API reports one this machine can't reach")
            .block(),
        Variable::new("ssh_via_node", "bool")
            .default(boolean(false))
            .describe("Reach the lab VM over SSH through the node (its bridge isn't routable from here)")
            .block(),
        s("node").default(string("pve")).block(),
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
    ]
}

fn locals() -> Block {
    let addresses = call("flatten", [reference("proxmox_virtual_environment_vm.env.ipv4_addresses")]);
    let not_loopback = binary(reference("a"), BinaryOperator::NotEq, string("127.0.0.1"));
    let root = call(
        "abspath",
        [Template::new().with_expr(reference("path.module")).with_text("/../../..").into_expression()],
    );
    Block::builder("locals")
        .add_attribute(("root", root))
        .add_attribute(("ip", index(for_list("a", addresses, reference("a"), Some(not_loopback)), 0)))
        .build()
}

/// `iso<slot><suffix>`: names unique per environment on the server.
fn slotted(suffix: &str) -> Expression {
    Template::text("iso").with_expr(reference("var.slot")).with_text(suffix).into_expression()
}

fn image() -> Block {
    Block::builder("resource")
        .add_label("proxmox_download_file")
        .add_label("debian")
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("datastore_id", reference("var.image_datastore")))
        .add_attribute(("content_type", string("iso")))
        .add_attribute((
            "url",
            string("https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2"),
        ))
        .add_attribute(("file_name", slotted("-docker-debian-12.img")))
        .add_attribute(("overwrite_unmanaged", boolean(true)))
        .build()
}

/// The cloud-init snippet: the `isoloom` user with the operator's key, the guest agent (it
/// reports the address), and the constraints' prelude at every boot.
fn snippet(env: &Environment) -> Block {
    let user = object([
        ("name", string("isoloom")),
        ("sudo", string("ALL=(ALL) NOPASSWD:ALL")),
        ("shell", string("/bin/bash")),
        ("ssh_authorized_keys", list([reference("var.ssh_public_key")])),
    ]);
    let mut config = CloudConfig::new(&env.name, list([user]));
    config.packages = vec!["qemu-guest-agent".into(), "curl".into()];
    config.bootcmd.extend(cloud_init::prelude(env));
    config.runcmd.push("systemctl enable --now qemu-guest-agent".into());
    Block::builder("resource")
        .add_label("proxmox_virtual_environment_file")
        .add_label("env")
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("datastore_id", reference("var.snippets_datastore")))
        .add_attribute(("content_type", string("snippets")))
        .add_block(
            Block::builder("source_raw")
                .add_attribute(("file_name", slotted("-docker.yaml")))
                .add_attribute(("data", config.document()))
                .build(),
        )
        .build()
}

/// `<name> { <attributes> }`.
fn settings(name: &str, attributes: Vec<(&str, Expression)>) -> Block {
    attributes.into_iter().fold(Block::builder(name), BlockBuilder::add_attribute).build()
}

fn vm(env: &Environment) -> Block {
    let size = Sizing::of(env);
    let ip_config = Block::builder("ip_config")
        .add_block(settings("ipv4", vec![("address", string("dhcp"))]))
        .build();
    Block::builder("resource")
        .add_label("proxmox_virtual_environment_vm")
        .add_label("env")
        .add_attribute((
            "name",
            Template::text("iso")
                .with_expr(reference("var.slot"))
                .with_text(format!("-{}", env.name))
                .into_expression(),
        ))
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("tags", list([string("isoloom"), string(env.name.as_str())])))
        .add_attribute(("on_boot", boolean(false)))
        .add_block(settings("agent", vec![("enabled", boolean(true))]))
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
        .build()
}

/// `var.<name> != ""` (or `==` when `set` is false).
fn given(name: &str, set: bool) -> Expression {
    let op = if set { BinaryOperator::NotEq } else { BinaryOperator::Eq };
    binary(reference(&format!("var.{name}")), op, string(""))
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
        given("proxmox_ssh_address", true),
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
    let key = || given("proxmox_ssh_private_key_file", true);
    let bastion = [
        ("bastion_host", via_node(None, Expression::Parenthesis(Box::new(node)))),
        ("bastion_user", via_node(None, reference("var.proxmox_ssh_username"))),
        (
            "bastion_password",
            via_node(Some(given("proxmox_ssh_private_key_file", false)), reference("var.proxmox_password")),
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
