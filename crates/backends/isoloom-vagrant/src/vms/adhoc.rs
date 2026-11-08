//! The hook for machines the runner adds to the running environment, as GNS3 adds a node to a
//! live topology: its ad-hoc attack VM (`isothrum attach` on a lab whose spec declares no
//! `access: true` machine). The runner writes a Ruby file under `<state folder>/adhoc/` calling
//! [`IsoloomAdHoc.define`](ADHOC_RB) with a name, a lab network, a free address and a box, and the
//! Vagrantfile loads every such file at the end of `Vagrant.configure` ([`loader`]). What a VM
//! needs to join each network on each provider is generated here ([`preamble`]): the network's
//! name on the hypervisor and its netmask, the environment's label, the lab machines' names for
//! the VM's hosts file, this host's default box, and what a VM running a container image needs
//! (Debian 12's box per provider, Docker's installer). On QEMU every network's hub keeps a port
//! for the runner's VM, named after it ([`ADHOC_NAME`], see [`super::qemu`]).

use std::net::Ipv4Addr;

use isoloom_manifest::access;
use isoloom_plan::Environment;
use isoloom_plan::addressing::netmask;
use isoloom_plan::lifecycle::Step;
use isoloom_render::steps;
use isoloom_spec::images;

use super::ARM_HOST;
use crate::ruby::{Ruby, Value};

/// The Ruby module (`vagrant_adhoc.rb`): `IsoloomAdHoc.define` and the loader's `files`.
pub(crate) const ADHOC_RB: &str = include_str!("../vagrant_adhoc.rb");

/// Debian 12 for a VM that runs a container image: the box Isoloom's own Debian VMs use on the
/// desktop hypervisors (VirtualBox, VMware, Parallels), with arm64 builds too.
const IMAGE_BOX: &str = "bento/debian-12";

/// Debian 12 on libvirt, where bento's box has no build (the roboxes one, as Isoloom's VMs).
const IMAGE_BOX_LIBVIRT: &str = "generic/debian12";

/// The disk of a VM running a container image on QEMU, where the cloud image's own is 10 GB: an
/// attack box unpacks to 12 GB. Sparse, as the Docker helper VM's (see [`crate::docker_vm`]).
const IMAGE_DISK_GB: u32 = 40;

/// The define name of the runner's ad-hoc attack VM: Isoloom's reserved namespace (a spec's
/// machine is never called so), the name the runner gives every attacker box it stands up itself,
/// and the key of the port each QEMU hub keeps for it.
pub(crate) const ADHOC_NAME: &str = "isoloom-access";

/// What the runner's definitions need at hand, before `Vagrant.configure`: the environment's
/// label, each lab network as a VM joins it, the lab machines' names, this host's default box,
/// Debian 12's boxes and Docker's installer for a VM running a container image, then the module.
pub(crate) fn preamble(r: &mut Ruby, env: &Environment) {
    r.comment("Machines the runner adds to the running environment (see IsoloomAdHoc below).");
    r.assign("ADHOC_LABEL", Value::str(&env.name));
    r.hash(
        "ADHOC_NETWORKS",
        env.networks.keys().map(|net| {
            let join = [
                (Value::str("name"), Value::str(&super::network_name(&env.name, net))),
                (Value::str("netmask"), Value::str(&netmask(env.spec(), net).to_string())),
            ];
            (Value::str(net), Value::hash(join))
        }),
    );
    r.assign("ADHOC_HOSTS", Value::str(&hosts(env)));
    r.assign(
        "ADHOC_BOX",
        Value::raw(format!(
            "{ARM_HOST} ? {} : {}",
            Value::str(access::DEFAULT_ARM64_BOX),
            Value::str(access::DEFAULT_BOX)
        )),
    );
    r.assign("ADHOC_IMAGE_BOX", Value::str(IMAGE_BOX));
    r.hash(
        "ADHOC_IMAGE_BOXES",
        [("libvirt", IMAGE_BOX_LIBVIRT), ("qemu", images::QEMU_DEBIAN_12)]
            .into_iter()
            .map(|(p, b)| (Value::str(p), Value::str(b))),
    );
    r.assign("ADHOC_IMAGE_DISK_GB", Value::raw(IMAGE_DISK_GB.to_string()));
    r.assign("ADHOC_DOCKER", Value::str(steps::INSTALL_DOCKER));
    r.text(ADHOC_RB);
    r.blank();
}

/// The shell that gives an added VM every lab machine's name, at its address on its first
/// network (as Isoloom's own VMs get them); empty when the lab has no addressed machine.
fn hosts(env: &Environment) -> String {
    let entries: Vec<(Ipv4Addr, Vec<String>)> = env
        .machines
        .values()
        .filter_map(|mp| mp.addresses.first().map(|(_, ip)| (*ip, mp.names.clone())))
        .collect();
    if entries.is_empty() {
        return String::new();
    }
    steps::linux(env, "", &Step::Hosts { entries }).unwrap_or_default()
}

/// At the end of `Vagrant.configure`: every definition the runner wrote, evaluated with `config`
/// in scope.
pub(crate) fn loader(r: &mut Ruby) {
    r.blank();
    r.comment("Machines the runner added to the running environment, if any (IsoloomAdHoc).");
    r.line("IsoloomAdHoc.files.each { |f| eval(File.read(f), binding, f) }");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    #[test]
    fn the_preamble_says_how_a_vm_joins_each_network_and_names_the_machines() {
        let env = environment(
            "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n  dmz: { cidr: 10.8.0.0/25 }\nmachines:\n  web: { networks: { lab: 10, dmz: 20 }, os: debian@12, aliases: [www] }\n",
        );
        let mut r = Ruby::new();
        preamble(&mut r, &env);
        let text = r.finish();
        assert!(text.contains("ADHOC_LABEL = \"t\""), "{text}");
        assert!(
            text.contains("\"lab\" => { \"name\" => \"isoloom-t-lab\", \"netmask\" => \"255.255.255.0\" }"),
            "{text}"
        );
        assert!(
            text.contains("\"dmz\" => { \"name\" => \"isoloom-t-dmz\", \"netmask\" => \"255.255.255.128\" }"),
            "{text}"
        );
        assert!(text.contains("ADHOC_HOSTS = \"printf '%s\\\\n' '10.9.0.10 web www'"), "{text}");
        assert!(
            text.contains("ADHOC_BOX = RbConfig::CONFIG[\"host_cpu\"] =~ /arm|aarch64/ ? \"cloud-image/debian-12\" : \"kalilinux/rolling\""),
            "{text}"
        );
        // A VM running a container image: Debian 12 per provider, and Docker's installer.
        assert!(text.contains("ADHOC_IMAGE_BOX = \"bento/debian-12\""), "{text}");
        assert!(
            text.contains("\"libvirt\" => \"generic/debian12\"") && text.contains("\"qemu\" => \"cloud-image/debian-12\""),
            "{text}"
        );
        assert!(text.contains("ADHOC_DOCKER = \"command -v docker >/dev/null || "), "{text}");
        assert!(text.contains("ADHOC_IMAGE_DISK_GB = 40"), "{text}");
        assert!(text.contains("module IsoloomAdHoc"), "{text}");
    }

    #[test]
    fn a_lab_with_no_addressed_machine_has_no_hosts_script() {
        let env = environment("version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines: {}\n");
        assert_eq!(hosts(&env), "");
    }
}
