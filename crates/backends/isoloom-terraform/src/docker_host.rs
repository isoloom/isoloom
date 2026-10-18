//! The Docker host: one VM running the environment's containers with the `docker` target's
//! Compose file, on a cloud (`cloud-docker`) or a Proxmox server (`docker-vm`). Every module of
//! it shares its size (`isoloom_render::container::HostSize`) and the ports it opens; a cloud's
//! host sets itself up from its user data (`crate::cloud::docker::setup`), and the Proxmox one
//! through the docker-vm run manifest.

use isoloom_plan::{Environment, MachineShape};
use isoloom_render::hcl_builder::{Expression, list, string};
use isoloom_spec::model::Protocol;

/// The ports the containers publish over `protocol`, sorted, each once.
pub(crate) fn published(env: &Environment, protocol: Protocol) -> Vec<u16> {
    let mut ports: Vec<u16> = env
        .machines
        .values()
        .filter(|m| m.shape == MachineShape::Container)
        .flat_map(|m| &m.ports)
        .filter(|p| p.protocol == protocol)
        .map(|p| p.publish)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// SSH, then the ports the containers publish over TCP: what a firewall taking one list of TCP
/// ports opens.
pub(crate) fn tcp_ports(env: &Environment) -> Vec<u16> {
    let mut tcp = vec![22];
    tcp.extend(published(env, Protocol::Tcp));
    tcp
}

/// The ports as strings (`["22", "8080"]`), for the firewalls that take them so.
pub(crate) fn port_strings(ports: &[u16]) -> Expression {
    list(ports.iter().map(|p| string(p.to_string())))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Protocol;

    use super::*;
    use crate::cloud::docker::tests::{SPEC, env};

    #[test]
    fn published_ports_are_sorted_once_per_protocol() {
        let e = env(SPEC);
        assert_eq!(published(&e, Protocol::Tcp), [8080, 15432]);
        assert_eq!(published(&e, Protocol::Udp), [5353]);
        assert_eq!(tcp_ports(&e), [22, 8080, 15432]);
    }
}
