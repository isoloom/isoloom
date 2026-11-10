//! The module's base, before the router and the machines: its providers, variables, the
//! connection to the host, the locals every VM shares, the generated credentials, and the
//! networks (`networks`, `outside`).

use crate::hcl_builder::{BinaryOperator, Block, Module, Template, binary, call, for_list, list, number, object, reference, string};
use isoloom_plan::Environment;

use super::{SERVICE_OS, default_image};
use crate::machine::credentials;
use crate::machine::windows;
use crate::module::providers;
use crate::module::variable::{self, Variable};
use crate::server::PinnedImage;
use crate::server::{self, user};

/// The highest slot: it is the fourth byte of the VMs' MACs, which VMware keeps under 0x40 for
/// addresses set by hand.
const MAX_SLOT: u64 = 63;

/// Everything before the router.
pub(super) fn blocks(env: &Environment, m: &mut Module) {
    server::head(env, m, &[providers::ESXI, providers::TLS, providers::LOCAL], variables(env), provider());
    m.commented("The project's root, and the `isoloom` user on every VM.", locals());
    credentials(env, m);
    super::networks::blocks(env, m);
    super::outside::blocks(env, m);
}

/// The OSes the module deploys, the router's and the controller's among them, sorted.
fn oses(env: &Environment) -> Vec<&str> {
    let mut all: Vec<&str> = env.vms().map(|vm| vm.os).collect();
    all.push(SERVICE_OS);
    all.sort_unstable();
    all.dedup();
    all
}

/// The module's variables, the run manifest's params and slots among them.
pub(super) fn variables(env: &Environment) -> Vec<Variable> {
    let s = |name: &str| Variable::new(name, "string");
    let mut out = vec![
        s("esxi_hostname").describe("The ESXi host's name or address"),
        s("esxi_hostport").default(string("22")).describe("Its SSH port: the provider works over SSH"),
        s("esxi_hostssl").default(string("443")),
        s("esxi_username").default(string("root")),
        s("esxi_password").sensitive(),
        s("disk_store").default(string("datastore1")).describe("Where the VMs' disks go"),
        s("uplink_port_group")
            .default(string("VM Network"))
            .describe("The port group the router reaches the internet through (DHCP)"),
        slot(),
        images(env),
        variable::ssh_public_key(false),
        s("ssh_private_key_file").describe("The private key of ssh_public_key: Terraform sets the machines up over SSH, through the router"),
    ];
    if super::outside::on_host_vswitch(env) {
        out.push(
            s("uplink_vswitch")
                .default(string("vSwitch0"))
                .describe("The host's vSwitch on its LAN: the bridged networks' port groups go on it"),
        );
    }
    out
}

/// `slot`, checked: 1 to [`MAX_SLOT`].
fn slot() -> Variable {
    let v = || reference("var.slot");
    let condition = binary(
        binary(v(), BinaryOperator::GreaterEq, number(1)),
        BinaryOperator::And,
        binary(v(), BinaryOperator::LessEq, number(MAX_SLOT)),
    );
    Variable::new("slot", "number")
        .default(number(1))
        .describe("1 to 63, unique per environment on this host: it names the vSwitches and the VMs, and makes the MACs")
        .validated(condition, "slot is 1 to 63.")
}

/// `images`: an OVA (a path or a URL) per OS. Ubuntu's come with defaults, whose SHA-256 the
/// description gives (the provider checks none); every other OS the environment runs must be
/// given, which the validation says.
fn images(env: &Environment) -> Variable {
    let oses = oses(env);
    let pinned: Vec<(&str, PinnedImage)> = oses.iter().filter_map(|o| default_image(o).map(|p| (*o, p))).collect();
    let defaults = pinned.iter().map(|(o, p)| (*o, string(p.url)));
    let needed = list(oses.iter().map(|o| string(*o)));
    let has = call("contains", [call("keys", [reference("var.images")]), reference("os")]);
    let condition = call("alltrue", [for_list("os", needed, has, None)]);
    let sums: Vec<String> = pinned.iter().map(|(o, p)| format!("{o} {}", p.checksum)).collect();
    let pins = if sums.is_empty() {
        String::new()
    } else {
        format!(" (the provider checks no hash: their SHA-256, as Ubuntu publishes them, {})", sums.join(", "))
    };
    let mut description =
        format!("An OVA per OS, a path or a URL: Ubuntu's cloud OVAs by default{pins}; for another OS, an OVA with cloud-init and open-vm-tools");
    if windows::any(env) {
        description.push_str("; for Windows, an OVA with VMware Tools and cloudbase-init (its VMware guestinfo service), sysprepped");
    }
    Variable::new("images", "string")
        .typed(call("map", [reference("string")]))
        .default(object(defaults))
        .describe(&description)
        .validated(condition, &format!("images needs an OVA for each of: {}.", oses.join(", ")))
}

/// The connection: SSH to the host, which the provider runs its commands over.
fn provider() -> Block {
    Block::builder("provider")
        .add_label("esxi")
        .add_attribute(("esxi_hostname", reference("var.esxi_hostname")))
        .add_attribute(("esxi_hostport", reference("var.esxi_hostport")))
        .add_attribute(("esxi_hostssl", reference("var.esxi_hostssl")))
        .add_attribute(("esxi_username", reference("var.esxi_username")))
        .add_attribute(("esxi_password", reference("var.esxi_password")))
        .build()
}

/// The project's root (the module is `.isoloom/esxi/`), and the `isoloom` user with the
/// operator's key (the controller adds its own when the machine is set up).
fn locals() -> Block {
    let user = user(list([reference("var.ssh_public_key")]));
    Block::builder("locals")
        .add_attribute((
            "root",
            call(
                "abspath",
                [Template::new().with_expr(reference("path.module")).with_text("/../..").into_expression()],
            ),
        ))
        .add_attribute(("users", list([user])))
        .build()
}
