//! The module's settings: providers, variables, the connection, the locals every VM shares, the
//! networks (an SDN zone, a VNet each) and the cloud images.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, binary, boolean, call, conditional, for_list, list, null, number, object, reference, string,
};

use super::image_url;
use crate::vm::module::{Variable, inputs_variable, terraform};
use crate::vm::versions;
use crate::vm::{controller, os, res, vms};

/// Everything before the router.
pub fn blocks(env: &Environment, m: &mut Module) {
    let windows = vms(env).any(|(_, mp)| mp.is_windows());
    let mut providers = vec![versions::PROXMOX, versions::TLS];
    if windows {
        providers.push(versions::RANDOM);
    }
    m.block(terraform(&providers));
    m.blocks(variables());
    m.blocks(super::windows::variables(env));
    if !env.spec().inputs.is_empty() {
        m.block(inputs_variable());
    }
    m.block(provider());
    m.commented(
        "The project, written to /opt/isoloom in each machine that has steps; the `isoloom` user on every VM.",
        locals(env),
    );
    m.commented(
        "The environment's networks: an SDN simple zone, a VNet per network.",
        Block::builder("resource")
            .add_label("proxmox_sdn_zone_simple")
            .add_label("env")
            .add_attribute(("id", reference("local.zone")))
            .add_attribute(("nodes", list([reference("var.node")])))
            .build(),
    );
    if controller::needed(env) {
        m.commented("The controller's own key: it runs the playbooks over SSH on every machine.", controller::key());
    }
    if windows {
        m.commented(
            "The Windows machines' administrator password (user isoloom), for WinRM.",
            crate::vm::cloud::windows_password(),
        );
    }
    let mut vnets = Vec::new();
    for (i, (net, plan)) in env.networks.iter().enumerate() {
        let id = Template::text("i").with_expr(reference("var.slot")).with_text(format!("n{i}"));
        m.commented(
            &format!("Network `{net}`: {}", plan.cidr),
            Block::builder("resource")
                .add_label("proxmox_sdn_vnet")
                .add_label(res(net))
                .add_attribute(("id", id.into_expression()))
                .add_attribute(("zone", reference("proxmox_sdn_zone_simple.env.id")))
                .build(),
        );
        vnets.push(reference(&format!("proxmox_sdn_vnet.{}", res(net))));
    }
    m.block(
        Block::builder("resource")
            .add_label("proxmox_sdn_applier")
            .add_label("env")
            .add_attribute(("depends_on", list(vnets)))
            .build(),
    );
    images(env, m);
    super::image::downloads(env, m);
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
            .describe("Uploading cloud-init snippets goes over SSH to the node")
            .block(),
        s("proxmox_ssh_private_key_file").default(string("")).block(),
        s("proxmox_ssh_address")
            .default(string(""))
            .describe("The node's SSH address, when the API reports one this machine can't reach")
            .block(),
        s("node").default(string("pve")).block(),
        s("datastore").default(string("local-lvm")).describe("Where VM disks go").block(),
        s("image_datastore")
            .default(string("local"))
            .describe("A datastore with 'iso' content, for cloud images")
            .block(),
        s("snippets_datastore")
            .default(string("local"))
            .describe("A datastore with 'snippets' content, for cloud-init")
            .block(),
        s("uplink_bridge")
            .default(string("vmbr0"))
            .describe("The bridge the router reaches the internet through")
            .block(),
        Variable::new("slot", "number")
            .default(number(1))
            .describe("1 to 99, unique per environment on this server (SDN ids are short)")
            .block(),
        s("ssh_public_key")
            .default(string(""))
            .describe("Installed for the user `isoloom` on every VM")
            .block(),
    ]
}

/// `var.<name> != ""` (or `==` when `set` is false).
fn given(name: &str, set: bool) -> Expression {
    let op = if set { BinaryOperator::NotEq } else { BinaryOperator::Eq };
    binary(reference(&format!("var.{name}")), op, string(""))
}

/// The connection: an API token, else a user and password; SSH to the node for the snippets.
pub(crate) fn provider() -> Block {
    let token = || given("proxmox_api_token", true);
    let key = || given("proxmox_ssh_private_key_file", true);
    let node = Block::builder("dynamic")
        .add_label("node")
        .add_attribute(("for_each", conditional(given("proxmox_ssh_address", false), list([]), list([number(1)]))))
        .add_block(
            Block::builder("content")
                .add_attribute(("name", reference("var.node")))
                .add_attribute(("address", reference("var.proxmox_ssh_address")))
                .build(),
        )
        .build();
    let ssh = Block::builder("ssh")
        .add_attribute(("agent", boolean(false)))
        .add_attribute(("username", reference("var.proxmox_ssh_username")))
        .add_attribute(("password", conditional(key(), null(), reference("var.proxmox_password"))))
        .add_attribute((
            "private_key",
            conditional(key(), call("file", [reference("var.proxmox_ssh_private_key_file")]), null()),
        ))
        .add_block(node)
        .build();
    Block::builder("provider")
        .add_label("proxmox")
        .add_attribute(("endpoint", reference("var.proxmox_endpoint")))
        .add_attribute(("api_token", conditional(token(), reference("var.proxmox_api_token"), null())))
        .add_attribute(("username", conditional(token(), null(), reference("var.proxmox_username"))))
        .add_attribute(("password", conditional(token(), null(), reference("var.proxmox_password"))))
        .add_attribute(("insecure", reference("var.proxmox_insecure")))
        .add_block(ssh)
        .build()
}

/// The SDN zone's id, the project's root and files, and the `isoloom` user: the operator's key
/// and, with a controller, the controller's own key too, so it can run the playbooks over SSH.
fn locals(env: &Environment) -> Block {
    let f = || reference("f");
    let file = object([
        ("path", Template::text("/opt/isoloom/").with_expr(f()).into_expression()),
        ("encoding", string("b64")),
        (
            "content",
            call(
                "filebase64",
                [Template::new()
                    .with_expr(reference("local.root"))
                    .with_text("/")
                    .with_expr(f())
                    .into_expression()],
            ),
        ),
    ]);
    let user = |keys: Expression| {
        object([
            ("name", string("isoloom")),
            ("sudo", string("ALL=(ALL) NOPASSWD:ALL")),
            ("shell", string("/bin/bash")),
            ("ssh_authorized_keys", keys),
        ])
    };
    let users = if controller::needed(env) {
        list([user(call("compact", [list([reference("var.ssh_public_key"), controller::public_key()])]))])
    } else {
        conditional(given("ssh_public_key", false), list([]), list([user(list([reference("var.ssh_public_key")]))]))
    };
    Block::builder("locals")
        .add_attribute(("zone", Template::text("iso").with_expr(reference("var.slot")).into_expression()))
        .add_attribute((
            "root",
            call(
                "abspath",
                [Template::new().with_expr(reference("path.module")).with_text("/../..").into_expression()],
            ),
        ))
        .add_attribute(("project_files", for_list("f", reference("local.project_paths"), file, None)))
        .add_attribute(("users", users))
        .build()
}

/// A cloud image per OS (the router's and the controller's Debian among them), per environment:
/// a shared file would go with whichever environment is destroyed first.
fn images(env: &Environment, m: &mut Module) {
    let mut oses: Vec<&str> = vms(env).filter(|(_, mp)| !super::image::has_own(mp)).filter_map(|(_, mp)| os(mp)).collect();
    oses.push(controller::OS);
    oses.sort_unstable();
    oses.dedup();
    for o in oses {
        let Some(url) = image_url(o) else { continue };
        let block = Block::builder("resource")
            .add_label("proxmox_download_file")
            .add_label(res(o))
            .add_attribute(("node_name", reference("var.node")))
            .add_attribute(("datastore_id", reference("var.image_datastore")))
            .add_attribute(("content_type", string("iso")))
            .add_attribute(("url", string(url)))
            .add_attribute((
                "file_name",
                Template::text("iso")
                    .with_expr(reference("var.slot"))
                    .with_text(format!("-{o}.img"))
                    .into_expression(),
            ))
            .add_attribute(("overwrite_unmanaged", boolean(true)))
            .build();
        m.commented(
            &format!("The `{o}` cloud image, per environment: a shared file would go with whichever environment is destroyed first."),
            block,
        );
    }
}
