//! A machine's several addresses on one network (`lan: [10, 11]`) on Oracle Cloud: each further
//! address is a secondary private IP (`oci_core_private_ip`) of the machine's VNIC on that
//! network, which the VCN then delivers to it; the instance's own set-up puts it on the
//! interface. The first VNIC is created with the instance, so its id is looked up by its
//! primary address; a VNIC attached after has its id from the attachment.

use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Block, Expression, Module, attr, index, list, reference, string};

use crate::vm::cloud::name_with;
use crate::vm::res;

/// The secondary private IPs of machine `name`, and the lookup of its first VNIC when they are on
/// it.
pub fn blocks(env: &Environment, name: &str, m: &mut Module) {
    let mp = &env.machines[name];
    let id = res(name);
    let first = mp.addresses.keys().next();
    for (net, more) in &mp.more_addresses {
        let nic = format!("{id}_{}", res(net));
        let vnic = if first == Some(net) {
            m.block(first_vnic(env, name, net, &nic));
            attr(index(reference(&format!("data.oci_core_private_ips.{nic}_first.private_ips")), 0), "vnic_id")
        } else {
            reference(&format!("oci_core_vnic_attachment.{nic}.vnic_id"))
        };
        for address in more {
            m.block(private_ip(
                &format!("{nic}_{}", address.octets()[3]),
                vnic.clone(),
                &address.to_string(),
                &format!("-{name}-{net}"),
            ));
        }
    }
}

/// The lookup of the instance's first VNIC, by its address there, once the instance exists.
fn first_vnic(env: &Environment, name: &str, net: &str, nic: &str) -> Block {
    Block::builder("data")
        .add_label("oci_core_private_ips")
        .add_label(format!("{nic}_first"))
        .add_attribute(("ip_address", string(env.machines[name].addresses[net].to_string())))
        .add_attribute(("subnet_id", reference(&format!("oci_core_subnet.{}.id", res(net)))))
        .add_attribute(("depends_on", list([reference(&format!("oci_core_instance.{}", res(name)))])))
        .build()
}

/// One secondary private IP on `vnic`, at `address`.
fn private_ip(id: &str, vnic: Expression, address: &str, display: &str) -> Block {
    Block::builder("resource")
        .add_label("oci_core_private_ip")
        .add_label(id)
        .add_attribute(("vnic_id", vnic))
        .add_attribute(("ip_address", string(address)))
        .add_attribute(("display_name", name_with(display)))
        .build()
}
