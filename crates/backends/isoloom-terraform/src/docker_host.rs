//! The Docker host: one cloud VM running the environment's containers with the `docker`
//! target's Compose file (`cloud-docker`). Every cloud's module of it shares its size
//! (`isoloom_render::container::HostSize`) and the ports it opens; it sets itself up from its
//! user data (`crate::cloud::docker::Host`): the project Terraform brings in, then
//! [`setup_body`] (Docker), then the Compose file started.

use isoloom_manifest::{Error as ManifestError, Machine, Output, OutputRole};
use isoloom_render::run_manifest;
use isoloom_render::steps::{self, CLOUD_INIT_WAIT, PUBLISHED_FROM_OUTSIDE, install_docker};

use crate::hcl_builder::{Expression, list, string};
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::Protocol;

/// The ports the containers publish over `protocol`, sorted, each once.
pub(crate) fn container_ports(env: &Environment, protocol: Protocol) -> Vec<u16> {
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
    tcp.extend(container_ports(env, Protocol::Tcp));
    tcp
}

/// The ports as strings (`["22", "8080"]`), for the firewalls that take them so.
pub(crate) fn port_strings(ports: &[u16]) -> Expression {
    list(ports.iter().map(|p| string(p.to_string())))
}

/// The outputs of a Docker host Terraform made: its address (`ip`, where SSH reaches it and the
/// published ports answer) and its SSH user (`ssh_user`), then each of `machines`' published
/// ports at that address.
///
/// # Errors
///
/// A name the manifest refuses.
pub(crate) fn docker_host_outputs(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    let mut out = vec![
        crate::module::manifest::terraform_output(
            "ip",
            OutputRole::Address,
            "The Docker host's address: SSH reaches it there, and its containers' published ports answer there.",
        )?,
        crate::module::manifest::terraform_output("ssh_user", OutputRole::Info, "The user the runner's key lets in on the Docker host.")?,
    ];
    out.extend(run_manifest::outputs_on_their_hosts(machines)?);
    Ok(out)
}

/// The Docker host's set-up as root, once the project is in, as one script for `sh -c`: once
/// cloud-init is done, the spec's constraints (proxy, mirrors, NTP) and certificate authorities,
/// then Docker from Docker's own repository unless it is there, and published ports answering
/// whatever the containers' routes ([`PUBLISHED_FROM_OUTSIDE`]). Running it again changes
/// nothing.
#[must_use]
pub(crate) fn setup_body(env: &Environment) -> String {
    let prelude = steps::linux(env, "", &isoloom_plan::Step::Prelude).unwrap_or_default();
    let trust = steps::trust_authorities(env).unwrap_or_default();
    format!("set -e\n{CLOUD_INIT_WAIT}\n{prelude}{trust}{}\n{PUBLISHED_FROM_OUTSIDE}", install_docker())
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Protocol;

    use super::*;
    use crate::cloud::docker::tests::{SPEC, env};

    #[test]
    fn published_ports_are_sorted_once_per_protocol() {
        let e = env(SPEC);
        assert_eq!(container_ports(&e, Protocol::Tcp), [8080, 15432]);
        assert_eq!(container_ports(&e, Protocol::Udp), [5353]);
        assert_eq!(tcp_ports(&e), [22, 8080, 15432]);
    }

    #[test]
    fn the_set_up_waits_for_cloud_init_then_installs_docker_as_root() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n",
            isoloom_spec::model::Target::Docker,
        );
        let body = setup_body(&env);
        // Run as root from the user data: no `sudo`.
        assert!(!body.contains("sudo"), "{body}");
        assert_eq!(
            body,
            format!(
                "set -e\ncloud-init status --wait >/dev/null 2>&1 || true\n{}\n{PUBLISHED_FROM_OUTSIDE}",
                install_docker()
            )
        );
    }
}
