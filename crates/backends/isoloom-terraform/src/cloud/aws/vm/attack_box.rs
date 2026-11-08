//! The learner's attack box on a lab run as one VM per machine (cloud-vm): an instance of its own
//! on the lab's first network, made only while the runner asks for one (`attack_box` not empty),
//! so adding or removing the box is an apply with that variable set or cleared. It is a host of
//! the lab network, `attackbox` by name, reached like the lab's machines (SSH from
//! `allowed_cidr`), in one of two forms (`attack_box_form`):
//!
//! - `kali` (the default): the box is the instance itself, booted from Kali's own image (the
//!   official one on the AWS Marketplace) with Kali's headless toolset, as the launcher's attack
//!   box image has it. Nothing wraps it.
//! - `container`: for an account that can't run Kali's image (its Marketplace terms not
//!   accepted), a Debian instance runs `attack_box`, a container image, on its own network; the
//!   shell is `docker exec` into it.
//!
//! Cloud-init writes [`READY`] once the box is set up, [`FAILED`] with why when it couldn't be:
//! what the runner waits for.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, attr, binary, boolean, call, conditional, index, list, number, object, reference, string,
};

use crate::cloud::aws::{ami, ami_filter, egress, ingress};
use crate::cloud::instance::name_with;
use crate::cloud::vm::at_first_address;
use crate::machine::controller;
use crate::module::variable::Variable;
use crate::module::{ident, output, resource};

/// The variable the runner sets to add the box (empty: no box): the container image it runs in
/// the `container` form.
pub(crate) const VARIABLE: &str = "attack_box";

/// The variable choosing the box's form: `kali` (the instance is the box) or `container`.
pub(crate) const FORM: &str = "attack_box_form";

/// The box's resources and outputs, by Isoloom's reserved name (a spec's machine never has it).
const NAME: &str = "isoloom_access";

/// The box's container in the `container` form.
const CONTAINER: &str = "isoloom-access";

/// The files cloud-init writes about the box: set up, or not (with why).
const READY: &str = "/var/lib/isoloom/attack-box-ready";
const FAILED: &str = "/var/lib/isoloom/attack-box-failed";
const LOG: &str = "/var/lib/isoloom/attack-box.log";

/// Kali's official image on the AWS Marketplace: its name, and the product it belongs to (so a
/// look-alike listing never stands in for it).
const KALI_IMAGE: &str = "debian-kali-last-snapshot-amd64-*";
const KALI_PRODUCT: &str = "7lgvy7mt78lgoi4lant0znp5h";

/// Each form's user: Kali's image logs in as `kali`, AWS's Debian images as `admin`.
const KALI_USER: &str = "kali";
const DEBIAN_USER: &str = "admin";

/// The box's instance: 2 GB for the box and its tools, and an instance type a Free Tier account
/// may run (it refuses `t3.medium`). The disk ([`DISK_GB`]) holds the toolset (about 12 GB).
const INSTANCE_TYPE: &str = "t3.small";
const DISK_GB: u64 = 40;

/// `attack_box`: set to add the box (its container image, for the `container` form); empty for
/// none.
pub(crate) fn variable() -> Variable {
    Variable::new(VARIABLE, "string")
        .default(string(""))
        .describe("Set while the learner has an attack box on the lab's network (its container image, for the container form); empty: none.")
}

/// `attack_box_form`: `kali` (the default) or `container`.
pub(crate) fn form_variable() -> Variable {
    Variable::new(FORM, "string")
        .default(string("kali"))
        .describe("The attack box's form: kali (an instance of Kali's own image) or container (a Debian instance running attack_box's image).")
        .validated(
            call("contains", [list([string("kali"), string("container")]), reference(&format!("var.{FORM}"))]),
            "attack_box_form is kali or container.",
        )
}

/// The box's security group, image lookup, instance and outputs, on the lab's first network;
/// none for a lab with no network.
pub(super) fn write(env: &Environment, m: &mut Module) {
    let Some((_, (net, _))) = at_first_address(env).next() else { return };
    let Some(plan) = env.networks.get(net) else { return };
    let group = resource("aws_security_group", NAME)
        .add_attribute(("count", wanted()))
        .add_attribute(("name", name_with("-isoloom-access")))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
        .add_block(ingress(&format!("its network ({net})"), (0, 0), "-1", list([string(plan.cidr.as_str())])))
        .add_block(ingress("SSH from allowed_cidr", (22, 22), "tcp", list([reference("var.allowed_cidr")])))
        .add_block(egress())
        .build();
    m.commented(
        "The learner's attack box, while the runner asks for one (`attack_box`), in its form (`attack_box_form`).",
        group,
    );
    // Looked up only for a Kali box: a region without the listing still runs the lab.
    m.block(
        ami("kali", "aws-marketplace", KALI_IMAGE)
            .add_attribute(("count", conditional(kali(), wanted(), number(0))))
            .add_block(ami_filter("product-code", KALI_PRODUCT))
            .add_block(ami_filter("architecture", "x86_64"))
            .build(),
    );
    m.block(instance(&ident(net)));
    let first = |path: &str| call("try", [attr(index(reference(&format!("aws_instance.{NAME}")), 0), path), string("")]);
    m.block(output(&format!("address_{NAME}"), first("public_ip")));
    m.block(output(&format!("lab_address_{NAME}"), first("private_ip")));
    m.block(output(&format!("ssh_user_{NAME}"), conditional(kali(), string(KALI_USER), string(DEBIAN_USER))));
    m.block(output("attack_box_form", reference(&format!("var.{FORM}"))));
}

/// One while `attack_box` is set, none otherwise.
fn wanted() -> Expression {
    conditional(
        binary(reference(&format!("var.{VARIABLE}")), BinaryOperator::Eq, string("")),
        number(0),
        number(1),
    )
}

/// Whether the box is the `kali` form.
fn kali() -> Expression {
    binary(reference(&format!("var.{FORM}")), BinaryOperator::Eq, string("kali"))
}

/// The instance, on subnet `subnet`: Kali's image, or Debian's for the `container` form.
fn instance(subnet: &str) -> Block {
    let image = conditional(
        kali(),
        attr(index(reference("data.aws_ami.kali"), 0), "id"),
        reference(&format!("data.aws_ami.{}.id", ident(controller::OS))),
    );
    resource("aws_instance", NAME)
        .add_attribute(("count", wanted()))
        .add_attribute(("ami", image))
        .add_attribute(("instance_type", string(INSTANCE_TYPE)))
        .add_attribute(("user_data_base64", call("base64gzip", [conditional(kali(), kali_setup(), container_setup())])))
        .add_attribute(("subnet_id", reference(&format!("aws_subnet.{subnet}.id"))))
        .add_attribute(("associate_public_ip_address", boolean(true)))
        .add_attribute(("key_name", reference("aws_key_pair.env.key_name")))
        .add_attribute((
            "vpc_security_group_ids",
            list([attr(index(reference(&format!("aws_security_group.{NAME}")), 0), "id")]),
        ))
        .add_block(Block::builder("root_block_device").add_attribute(("volume_size", number(DISK_GB))).build())
        .add_attribute(("tags", object([("Name", name_with("-isoloom-access"))])))
        .build()
}

/// The `kali` form's set-up: the host named `attackbox`, then Kali's headless toolset (what the
/// launcher's attack box image installs), unattended; [`READY`] once done, [`FAILED`] with why.
fn kali_setup() -> Expression {
    string(format!(
        "#!/bin/sh\n# Isoloom: the learner's attack box, Kali itself. Ready: {READY}; failed: {FAILED}.\n\
mkdir -p /var/lib/isoloom\n\
hostnamectl set-hostname attackbox 2>/dev/null || hostname attackbox\n\
grep -q attackbox /etc/hosts || echo '127.0.1.1 attackbox' >> /etc/hosts\n\
if (export DEBIAN_FRONTEND=noninteractive; apt-get update && apt-get install -y -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold kali-linux-headless) > {LOG} 2>&1; \
then touch {READY}; else tail -n 20 {LOG} > {FAILED}; fi\n"
    ))
}

/// The `container` form's set-up: Docker, then `attack_box`'s image on the instance's network,
/// host-named `attackbox`; [`READY`] once it runs, [`FAILED`] with why.
fn container_setup() -> Expression {
    let head = format!(
        "#!/bin/sh\n# Isoloom: the learner's attack box, a container. Ready: {READY}; failed: {FAILED}.\nmkdir -p /var/lib/isoloom\n{}\n",
        isoloom_render::steps::INSTALL_DOCKER
    );
    Template::text(head)
        .with_text(format!(
            "if docker run -d --name {CONTAINER} --hostname attackbox --network host --restart unless-stopped '"
        ))
        .with_expr(reference(&format!("var.{VARIABLE}")))
        .with_text(format!("' sleep infinity > {LOG} 2>&1; then touch {READY}; else cp {LOG} {FAILED}; fi\n"))
        .into_expression()
}
