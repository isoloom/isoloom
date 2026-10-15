//! The module's base, before the router and the machines: its providers, variables, the
//! connection, the locals every VM shares, the pool, the networks (an SDN zone, a VNet each),
//! the generated credentials and the cloud images.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, binary, boolean, call, conditional, for_list, list, null, number, object, reference, string,
};

use super::image_url;
use crate::machine::{controller, vms};
use crate::module::providers;
use crate::module::variable::Variable;
use crate::module::{ident, resource};
use crate::server::{self, credentials, slotted, user};

/// The environment's pool, as a VM or a container names it.
pub(super) const POOL: &str = "proxmox_virtual_environment_pool.env.pool_id";

/// Everything before the router.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    let variables = [
        variables(),
        super::windows::variables(env),
        super::lxc::variables(env),
        super::virtiofs::variables(env),
    ]
    .concat();
    server::head(env, m, &[providers::PROXMOX, providers::TLS], variables, provider());
    m.commented(
        "The project, written to /opt/isoloom in each machine that has steps; the `isoloom` user on every VM.",
        locals(env),
    );
    m.commented(
        "The environment's VMs and containers, together in a pool of their own.",
        resource("proxmox_virtual_environment_pool", "env")
            .add_attribute(("pool_id", reference("local.zone")))
            .add_attribute(("comment", string(format!("Isoloom: {}", env.name))))
            .build(),
    );
    let mut zone = resource("proxmox_sdn_zone_simple", "env")
        .add_attribute(("id", reference("local.zone")))
        .add_attribute(("nodes", list([reference("var.node")])));
    if let Some(mtu) = super::links::zone_mtu(env) {
        zone = zone.add_attribute(("mtu", number(mtu.into())));
    }
    m.commented("The environment's networks: an SDN simple zone, a VNet per network.", zone.build());
    credentials(env, m);
    let vnets = super::links::vnets(env, m);
    m.block(resource("proxmox_sdn_applier", "env").add_attribute(("depends_on", list(vnets))).build());
    images(env, m);
    super::image::downloads(env, m);
    super::firewall::ipsets(env, m);
    super::virtiofs::mappings(env, m);
}

fn variables() -> Vec<Block> {
    let s = |name: &str| Variable::new(name, "string");
    let mut v = connection("Uploading cloud-init snippets goes over SSH to the node");
    v.extend([
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
    ]);
    v
}

/// The variables of the connection to the server: its API (a token, or a user and password),
/// and SSH to the node, which uploads the snippets (`snippets` says how many).
pub(super) fn connection(snippets: &str) -> Vec<Block> {
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
        s("proxmox_ssh_username").default(string("root")).describe(snippets).block(),
        s("proxmox_ssh_private_key_file").default(string("")).block(),
        s("proxmox_ssh_address")
            .default(string(""))
            .describe("The node's SSH address, when the API reports one this machine can't reach")
            .block(),
    ]
}

/// `var.<name> != ""`.
pub(super) fn is_set(name: &str) -> Expression {
    binary(reference(&format!("var.{name}")), BinaryOperator::NotEq, string(""))
}

/// `var.<name> == ""`.
pub(super) fn is_unset(name: &str) -> Expression {
    binary(reference(&format!("var.{name}")), BinaryOperator::Eq, string(""))
}

/// A cloud image downloaded to the node's image datastore, `proxmox_download_file.<id>`, as
/// `iso<slot><suffix>`.
pub(super) fn download(id: &str, url: &str, suffix: &str) -> Block {
    resource("proxmox_download_file", id)
        .add_attribute(("node_name", reference("var.node")))
        .add_attribute(("datastore_id", reference("var.image_datastore")))
        .add_attribute(("content_type", string("iso")))
        .add_attribute(("url", string(url)))
        .add_attribute(("file_name", slotted(suffix)))
        .add_attribute(("overwrite_unmanaged", boolean(true)))
        .build()
}

/// The connection: an API token, else a user and password; SSH to the node for the snippets.
pub(super) fn provider() -> Block {
    let token = || is_set("proxmox_api_token");
    let key = || is_set("proxmox_ssh_private_key_file");
    let node = Block::builder("dynamic")
        .add_label("node")
        .add_attribute(("for_each", conditional(is_unset("proxmox_ssh_address"), list([]), list([number(1)]))))
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
    let users = if controller::needed(env) {
        list([user(call("compact", [list([reference("var.ssh_public_key"), controller::public_key()])]))])
    } else {
        conditional(is_unset("ssh_public_key"), list([]), list([user(list([reference("var.ssh_public_key")]))]))
    };
    Block::builder("locals")
        .add_attribute(("zone", slotted("")))
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
    let own = || vms(env).filter(|(_, mp)| !super::image::has_own(mp));
    let mut oses: Vec<(&str, bool)> = own().filter_map(|(_, mp)| Some((mp.os.as_deref()?, super::arch::is_arm(mp)))).collect();
    oses.push((controller::OS, false));
    oses.sort_unstable();
    oses.dedup();
    for (o, arm) in oses {
        let url = if arm { super::arch::image_url(o) } else { image_url(o) };
        let Some(url) = url else { continue };
        let (id, file) = if arm {
            (format!("{}_arm64", ident(o)), format!("{o}-arm64"))
        } else {
            (ident(o), o.to_owned())
        };
        let block = download(&id, url, &format!("-{file}.img"));
        m.commented(
            &format!("The `{o}` cloud image, per environment: a shared file would go with whichever environment is destroyed first."),
            block,
        );
    }
}
