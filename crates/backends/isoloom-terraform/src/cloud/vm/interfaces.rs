//! A cloud VM's interfaces after its first: the cloud attaches one per network, and the
//! machine's set-up brings each up at its address, finding it by its MAC address.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_plan::Environment;
use isoloom_render::hcl_builder::{Expression, Template};
use isoloom_spec::validate::Cidr;

/// The commands bringing up the interfaces after the first (found by MAC, `mac(network)` the
/// expression of its interface's MAC address, lowercase and colon-separated as `ip link`
/// writes it) at their addresses: at every boot, as the SSH user.
pub(crate) fn up(env: &Environment, addresses: &IndexMap<String, Ipv4Addr>, mac: &dyn Fn(&str) -> Expression) -> Vec<Template> {
    up_with(env, addresses, &|n, _| Template::new().with_expr(mac(n)))
}

/// [`up`], each MAC address given by the machine itself at boot (`mac(network, address)`, shell
/// text printing it): a cloud that attaches an interface only once the VM exists, so Terraform
/// can't hand its MAC to the VM's own set-up (Oracle Cloud).
pub(crate) fn up_found(env: &Environment, addresses: &IndexMap<String, Ipv4Addr>, mac: &dyn Fn(&str, Ipv4Addr) -> String) -> Vec<Template> {
    up_with(env, addresses, &|n, a| Template::text(mac(n, a)))
}

fn up_with(env: &Environment, addresses: &IndexMap<String, Ipv4Addr>, mac: &dyn Fn(&str, Ipv4Addr) -> Template) -> Vec<Template> {
    addresses
        .iter()
        .skip(1)
        .map(|(n, a)| {
            let len = env.networks.get(n).and_then(|p| Cidr::parse(&p.cidr)).map_or(32, |c| c.len);
            let mut t = Template::text("IF=$(ip -o link | grep -i \"");
            t.append(mac(n, *a));
            t.push_text(format!(
                "\" | awk -F': ' '{{print $2}}'); sudo ip link set \"$IF\" up && (ip -4 addr show \"$IF\" | grep -q {a}/ || sudo ip addr add {a}/{len} dev \"$IF\")"
            ));
            t
        })
        .collect()
}
