//! The containers' host: one more VM in the hybrid Vagrantfile, on the containers' networks with
//! promiscuous interfaces, where Docker creates a macvlan network per lab network and runs the
//! containers' Compose file.

use isoloom_plan::{Environment, MachineShape};
use isoloom_render::backend::RenderError;
use isoloom_render::{GenerateError, router};
use isoloom_spec::model::Target;
use isoloom_spec::validate::Cidr;

use super::containers::{Containers, docker_net};
use crate::ruby::{Call, Ruby, Value};
use crate::vagrantfile::{self, shell};

const HOST: &str = "isoloom-docker";

/// One lab network on the host: its interface and the macvlan network on it.
struct Nic {
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

/// The host's `config.vm.define` block, one level deep in the Vagrantfile, after a blank line.
pub(super) fn block(env: &Environment, containers: &Containers) -> Result<String, RenderError> {
    let nics = containers
        .networks
        .iter()
        .enumerate()
        .map(|(i, net)| nic(env, net, i))
        .collect::<Result<Vec<_>, _>>()?;
    let mut r = Ruby::at_depth(1);
    r.blank();
    r.comment("The containers' host: on their networks, letting Docker give each its own address there.");
    r.block(Call::new("config.vm.define").arg(Value::str(HOST)), "m", |r| define(r, env, containers, &nics));
    Ok(r.finish())
}

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
    vagrantfile::install_docker(r, "m");
    // The project, with this Compose file from Isoloom's outputs.
    let extra = Value::array([Value::str(".isoloom/hybrid/compose.yml")]);
    r.call(
        Call::new("m.vm.provision")
            .arg(Value::str("isoloom_project"))
            .kw("name", Value::str("project"))
            .kw("extra", extra),
    );
    r.heredoc(shell("m", "networks"), "inline", "SH", &networks_script(nics));
    let up = "cd /opt/isoloom && docker compose -f .isoloom/hybrid/compose.yml up -d --build --wait --wait-timeout 900";
    let inputs = (!env.spec().inputs.is_empty()).then(|| Value::raw("INPUTS"));
    r.call(shell("m", "containers").kw("inline", Value::str(up)).kw_opt("env", inputs));
}

/// Sized for every container at once plus Docker's own 1 GB (2 to 16 CPUs), every lab
/// interface promiscuous so each container answers at its own MAC address.
fn virtualbox(r: &mut Ruby, env: &Environment, nics: usize) {
    let containers = env.machines.values().filter(|m| m.shape == MachineShape::Container);
    let (cpus, mem) = containers.fold((0u32, 1024u32), |(c, m), x| (c + x.size.cpus.unwrap_or(1), m + x.size.memory_mb.unwrap_or(512)));
    r.block(Call::new("m.vm.provider").arg(Value::str("virtualbox")), "v", |r| {
        r.assign("v.name", Value::str(&format!("{} · containers", env.name)));
        r.assign("v.cpus", cpus.clamp(2, 16));
        r.assign("v.memory", mem);
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
fn nic(env: &Environment, net: &str, index: usize) -> Result<Nic, RenderError> {
    let plan = &env.networks[net];
    let invalid = || GenerateError::Unsupported {
        target: Target::Hybrid,
        what: format!("network `{net}`: `{}` isn't a block of addresses", plan.cidr),
    };
    let c = Cidr::parse(&plan.cidr).ok_or_else(invalid)?;
    let gateway = if env.router.is_some() {
        router::address(env.spec(), net)
    } else {
        c.gateway()
    };
    Ok(Nic {
        ip: c.controller().to_string(),
        mac: format!("0A15{:06X}{:02X}", name_hash(&env.name) & 0xFF_FFFF, index as u8),
        cidr: plan.cidr.clone(),
        gateway: gateway.to_string(),
        name: docker_net(env, net),
    })
}

/// Each lab interface up and promiscuous, found by its MAC address, and its macvlan network.
fn networks_script(nics: &[Nic]) -> String {
    let mut sh = String::from("set -e\n");
    for n in nics {
        let mac: Vec<String> = n
            .mac
            .to_lowercase()
            .as_bytes()
            .chunks(2)
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        sh.push_str(&format!(
            "IF=$(ip -o link | grep -i '{mac}' | awk -F': ' '{{print $2}}')\nip link set \"$IF\" up promisc on\ndocker network inspect {dn} >/dev/null 2>&1 || docker network create -d macvlan --subnet {cidr} --gateway {gw} -o parent=\"$IF\" {dn}\n",
            mac = mac.join(":"),
            dn = n.name,
            cidr = n.cidr,
            gw = n.gateway,
        ));
    }
    sh
}

/// A stable hash of the environment's name, for the Docker host's MAC addresses (FNV-1a).
fn name_hash(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    use super::{Nic, name_hash, networks_script};

    #[test]
    fn the_mac_prefix_is_stable() {
        assert_eq!(format!("{:06X}", name_hash("mixed-office") & 0xFF_FFFF), "574BB3");
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
