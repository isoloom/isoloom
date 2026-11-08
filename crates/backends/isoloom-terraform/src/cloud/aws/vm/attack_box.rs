//! The learner's attack box on a lab run as one VM per machine (cloud-vm): an instance of its own
//! on the lab's first network, made only while the runner asks for one. `attack_box` holds the
//! container image it runs (empty: none, the default), so adding or removing the box is an apply
//! with that variable set or cleared. Cloud-init installs Docker and runs the image on the
//! instance's own network, so the box is a host of the lab network, `attackbox` by name, reached
//! like the lab's machines: SSH from `allowed_cidr`, then `docker exec`.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{
    BinaryOperator, Block, Expression, Module, Template, attr, binary, boolean, call, conditional, index, list, number, object, reference, string,
};

use crate::cloud::aws::{egress, ingress};
use crate::cloud::instance::name_with;
use crate::cloud::vm::at_first_address;
use crate::machine::controller;
use crate::module::variable::Variable;
use crate::module::{ident, output, resource};

/// The variable the runner sets to add the box: its container image (empty: no box).
pub(crate) const VARIABLE: &str = "attack_box";

/// The box's resources and outputs, by Isoloom's reserved name (a spec's machine never has it).
const NAME: &str = "isoloom_access";

/// The box's container and host name inside the instance.
const CONTAINER: &str = "isoloom-access";

/// The file cloud-init writes once the box runs, and the one it writes when it couldn't start it
/// (with why): what the runner waits for.
const READY: &str = "/var/lib/isoloom/attack-box-ready";
const FAILED: &str = "/var/lib/isoloom/attack-box-failed";

/// Its user on the Debian image (AWS's Debian images log in as `admin`).
const USER: &str = "admin";

/// The box's instance: 2 GB for the box and its tools, and an instance type a Free Tier account
/// may run (it refuses `t3.medium`). The disk ([`DISK_GB`]) holds the image (Kali's unpacks to
/// about 12 GB).
const INSTANCE_TYPE: &str = "t3.small";
const DISK_GB: u64 = 40;

/// `attack_box`: the box's image, empty for none.
pub(crate) fn variable() -> Variable {
    Variable::new(VARIABLE, "string")
        .default(string(""))
        .describe("The attack box's container image, set while the learner has one on the lab's network; empty: none.")
}

/// The box's security group, instance and outputs, on the lab's first network; none for a lab
/// with no network.
pub(super) fn write(env: &Environment, m: &mut Module) {
    let Some((_, (net, _))) = at_first_address(env).next() else { return };
    let Some(plan) = env.networks.get(net) else { return };
    let group = resource("aws_security_group", NAME)
        .add_attribute(("count", count()))
        .add_attribute(("name", name_with("-isoloom-access")))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
        .add_block(ingress(&format!("its network ({net})"), (0, 0), "-1", list([string(plan.cidr.as_str())])))
        .add_block(ingress("SSH from allowed_cidr", (22, 22), "tcp", list([reference("var.allowed_cidr")])))
        .add_block(egress())
        .build();
    m.commented("The learner's attack box, while the runner asks for one (`attack_box`: its image).", group);
    m.block(instance(&ident(net)));
    let first = |path: &str| call("try", [attr(index(reference(&format!("aws_instance.{NAME}")), 0), path), string("")]);
    m.block(output(&format!("address_{NAME}"), first("public_ip")));
    m.block(output(&format!("lab_address_{NAME}"), first("private_ip")));
    m.block(output(&format!("ssh_user_{NAME}"), string(USER)));
}

/// One of each while `attack_box` names an image, none otherwise.
fn count() -> Expression {
    conditional(
        binary(reference(&format!("var.{VARIABLE}")), BinaryOperator::Eq, string("")),
        number(0),
        number(1),
    )
}

/// The instance, on subnet `subnet`, set up by [`user_data`].
fn instance(subnet: &str) -> Block {
    resource("aws_instance", NAME)
        .add_attribute(("count", count()))
        .add_attribute(("ami", reference(&format!("data.aws_ami.{}.id", ident(controller::OS)))))
        .add_attribute(("instance_type", string(INSTANCE_TYPE)))
        .add_attribute(("user_data_base64", call("base64gzip", [user_data()])))
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

/// Cloud-init's script: Docker, then the image as a container on the instance's network, named
/// and host-named `attackbox`; [`READY`] once it runs, [`FAILED`] with why when it can't.
fn user_data() -> Expression {
    let head = format!(
        "#!/bin/sh\n# Isoloom: the learner's attack box. Ready: {READY}; failed: {FAILED}.\nmkdir -p /var/lib/isoloom\n{}\n",
        isoloom_render::steps::INSTALL_DOCKER
    );
    Template::text(head)
        .with_text(format!(
            "if docker run -d --name {CONTAINER} --hostname attackbox --network host --restart unless-stopped '"
        ))
        .with_expr(reference(&format!("var.{VARIABLE}")))
        .with_text(format!(
            "' sleep infinity > /var/lib/isoloom/attack-box.log 2>&1; then touch {READY}; else cp /var/lib/isoloom/attack-box.log {FAILED}; fi\n"
        ))
        .into_expression()
}
