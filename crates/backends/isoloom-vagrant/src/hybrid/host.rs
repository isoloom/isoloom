//! The containers' host: one more VM in the hybrid Vagrantfile, on the containers' networks with
//! promiscuous interfaces, where Docker creates a macvlan network per lab network and runs the
//! containers' Compose file.

use isoloom_plan::Environment;
use isoloom_render::container::{DOCKER_HOST as HOST, HostSize, HybridContainers as Containers, docker_net, host_environment};
use isoloom_render::router;
use isoloom_render::stable_hash;
use isoloom_spec::linux::PROJECT_DIR;
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use crate::error::{Error, Result};
use crate::ruby::{Call, Provision, Ruby, Value};
use crate::vagrantfile::{self, shell};
use crate::vms::values;

/// One lab network on the host: its interface and the macvlan network on it.
pub(super) struct Nic {
    /// The address Vagrant requires on a private network (the host takes none itself).
    ip: String,
    /// The interface's MAC address, as VirtualBox takes it (`0A15...`).
    mac: String,
    /// The network's block.
    cidr: String,
    /// The macvlan network's gateway: the router, else the network's own.
    gateway: String,
    /// The Docker (and VirtualBox internal) network's name.
    name: String,
}

/// The host's interface on each of the containers' networks.
pub(super) fn nics(env: &Environment, containers: &Containers) -> Result<Vec<Nic>> {
    containers.networks.iter().enumerate().map(|(i, net)| nic(env, net, i)).collect()
}

/// The host's `config.vm.define` block, in the Vagrantfile's configure block, after a blank line.
pub(super) fn write(r: &mut Ruby, env: &Environment, containers: &Containers, nics: &[Nic]) {
    r.blank();
    r.comment("The containers' host: on their networks, letting Docker give each its own address there.");
    r.define(HOST, true, |r| define(r, env, containers, nics));
}

/// The host's VM: its box, published ports and lab interfaces, its size, then Docker, the
/// project, the macvlan networks and the containers.
fn define(r: &mut Ruby, env: &Environment, containers: &Containers, nics: &[Nic]) {
    r.assign("m.vm.box", Value::str("bento/debian-12"));
    r.assign("m.vm.hostname", Value::str(HOST));
    for (port, protocol) in &containers.published {
        r.call(vagrantfile::forwarded_port("m", *port, *port, *protocol));
    }
    for n in nics {
        // auto_config off: the host itself takes no address there (`ip` is only what Vagrant
        // requires); its containers do.
        let call = Call::new("m.vm.network")
            .arg(Value::str("private_network"))
            .kw("ip", Value::str(&n.ip))
            .kw("auto_config", false)
            .kw("mac", Value::str(&n.mac))
            .kw("virtualbox__intnet", Value::str(&n.name));
        r.call(call);
    }
    virtualbox(r, env, nics.len());
    vagrantfile::prelude(r, env, "m");
    // The project, with the containers' Compose file, the one output of Isoloom's the host
    // reads: before Docker, so the certificate authorities it holds are trusted first.
    let rules = crate::vms::copy_rules(&[containers.file.path.as_str()]);
    r.provision(Provision::new("m", "isoloom_project").kw("name", Value::str("project")).kw("rules", rules));
    if let Some(sh) = isoloom_render::steps::trust_authorities(env) {
        r.provision(shell("m", "certificate authorities").kw("inline", Value::str(&sh)));
    }
    vagrantfile::install_docker(r, "m");
    r.provision(shell("m", "networks").heredoc("inline", "SH", &networks_script(nics)));
    let secrets = isoloom_render::secrets::host_environment(env.spec(), &isoloom_render::secrets::of_containers(env));
    let files = host_environment(env);
    let inputs = &env.spec().inputs;
    let decode = values::linux_decode(inputs);
    let up = format!("{decode}{secrets}cd {PROJECT_DIR} && {files}{}", containers.start);
    let encoded = (!inputs.is_empty()).then(|| values::encoded(inputs));
    r.provision(shell("m", "containers").kw("inline", Value::str(&up)).kw_opt("env", encoded));
}

/// Sized for every container at once plus Docker's own 1 GB (2 to 16 CPUs), every lab
/// interface promiscuous so each container answers at its own MAC address.
fn virtualbox(r: &mut Ruby, env: &Environment, nics: usize) {
    let size = HostSize::of(env);
    r.block(Call::new("m.vm.provider").arg(Value::str("virtualbox")), "v", |r| {
        r.assign("v.name", Value::str(&crate::vms::label(env, HOST)));
        r.assign("v.cpus", size.cpus);
        r.assign("v.memory", size.memory_mb);
        for i in 0..nics {
            let args = [
                Value::str("modifyvm"),
                Value::sym("id"),
                Value::str(&format!("--nicpromisc{}", i + 2)),
                Value::str("allow-all"),
            ];
            r.call(Call::new("v.customize").arg(Value::array(args)));
        }
    });
}

/// Network `net`, the host's `index`th lab network.
fn nic(env: &Environment, net: &str, index: usize) -> Result<Nic> {
    let cidr = env.networks.get(net).map(|n| n.cidr.as_str()).unwrap_or_default();
    let c = Cidr::parse(cidr).ok_or_else(|| Error::Unsupported {
        target: Target::Hybrid,
        reason: format!("network `{net}`: `{cidr}` isn't a block of addresses"),
    })?;
    let gateway = if env.router.is_some() {
        router::address(env.spec(), net)
    } else {
        c.gateway()
    };
    Ok(Nic {
        ip: c.controller().to_string(),
        // The index's low byte: the MAC's last octet.
        mac: format!("0A15{:06X}{:02X}", stable_hash::of(&env.name) & 0xFF_FFFF, index % 0x100),
        cidr: cidr.to_owned(),
        gateway: gateway.to_string(),
        name: docker_net(env, net),
    })
}

/// Each lab interface up and promiscuous, found by its MAC address, and its macvlan network.
fn networks_script(nics: &[Nic]) -> String {
    let each = nics.iter().map(|n| {
        let mac: Vec<String> = n
            .mac
            .to_lowercase()
            .as_bytes()
            .chunks(2)
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        format!(
            "IF=$(ip -o link | grep -i '{mac}' | awk -F': ' '{{print $2}}')\nip link set \"$IF\" up promisc on\ndocker network inspect {dn} >/dev/null 2>&1 || docker network create -d macvlan --subnet {cidr} --gateway {gw} -o parent=\"$IF\" {dn}\n",
            mac = mac.join(":"),
            dn = n.name,
            cidr = n.cidr,
            gw = n.gateway,
        )
    });
    std::iter::once("set -e\n".to_owned()).chain(each).collect::<Vec<_>>().concat()
}

#[cfg(test)]
mod tests {
    use super::{Nic, networks_script};

    #[test]
    fn the_mac_prefix_is_stable() {
        assert_eq!(format!("{:06X}", isoloom_render::stable_hash::of("mixed-office") & 0xFF_FFFF), "574BB3");
    }

    #[test]
    fn a_network_is_created_once_on_its_interface() {
        let n = Nic {
            ip: "10.0.0.253".into(),
            mac: "0A15ABCDEF01".into(),
            cidr: "10.0.0.0/24".into(),
            gateway: "10.0.0.1".into(),
            name: "isoloom-e-lab".into(),
        };
        let sh = networks_script(&[n]);
        assert!(sh.starts_with("set -e\nIF=$(ip -o link | grep -i '0a:15:ab:cd:ef:01'"), "{sh}");
        assert!(sh.contains("docker network inspect isoloom-e-lab >/dev/null 2>&1 || docker network create -d macvlan --subnet 10.0.0.0/24 --gateway 10.0.0.1 -o parent=\"$IF\" isoloom-e-lab\n"));
    }
}
