//! Instances: the same environment several times on one host. An instance number (1 to 99)
//! gives a copy its own names and its own room on the host, so two instances of a spec never
//! collide: the name gets a suffix (so Compose project, container, network, VM and namespace
//! names differ), the Docker networks move to other blocks (Docker refuses two networks on one
//! subnet), published host ports shift, and the files go to `.isoloom-<n>/` next to the
//! committed `.isoloom/`. Local VM networks are isolated by name already (VirtualBox internal
//! networks), so machines keep the addresses the spec writes.

use crate::model::{NetworkDocker, Spec};
use crate::net::Cidr;
use crate::net::Cidr6;
use crate::net::docker_cidrs;
use crate::syntax::{Code, FieldPath, Problem};
use isoloom_manifest::contract::OUTPUT_DIR;

/// Why a spec can't be made an instance of itself ([`apply`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InstanceError {
    /// An instance number outside 1 to [`MAX`].
    #[error("an instance number is 1 to {MAX}")]
    Number(u8),
    /// An environment's name too long once it carries its instance.
    #[error("`{0}` is too long for an instance name (at most {max} characters with the suffix)", max = crate::validate::NAME_MAX)]
    Name(String),
    /// A published port too high to shift for the instance.
    #[error("machines.{machine}.ports: published port {port} leaves no room for instance {instance} (ports shift by 100 per instance)")]
    Port {
        /// The machine.
        machine: String,
        /// The published port.
        port: u16,
        /// The instance.
        instance: u8,
    },
    /// A Docker network's block too high to shift for the instance: its second octet would go
    /// past 255 and wrap onto another block.
    #[error("networks.{network}.docker.cidr: {cidr} leaves no room for instance {instance} (Docker blocks move by the instance in their second octet)")]
    Docker {
        /// The network.
        network: String,
        /// Its Docker block.
        cidr: String,
        /// The instance.
        instance: u8,
    },
    /// An IPv6 block too high to shift for the instance.
    #[error("networks.{network}.ipv6.cidr: {cidr} leaves no room for instance {instance}")]
    Ipv6 {
        /// The network.
        network: String,
        /// Its IPv6 block.
        cidr: String,
        /// The instance.
        instance: u8,
    },
    /// A shifted block or port that the environment as written, or a lower instance, takes too.
    #[error("{}", .0.line())]
    Collision(Box<Problem>),
}

/// The highest instance number.
pub const MAX: u8 = 99;

/// Where an instance's generated files go.
#[must_use]
pub fn output_dir(instance: Option<u8>) -> String {
    match instance {
        Some(n) => format!("{OUTPUT_DIR}-{n}"),
        None => OUTPUT_DIR.to_owned(),
    }
}

/// The spec as instance `n` of itself (validated specs only).
///
/// # Errors
///
/// [`InstanceError::Number`] outside 1 to [`MAX`], [`InstanceError::Name`] when the name with its
/// suffix is too long, [`InstanceError::Port`], [`InstanceError::Docker`] or
/// [`InstanceError::Ipv6`] when a published port, a Docker block or an IPv6 block leaves no room
/// to shift, and [`InstanceError::Collision`] when a shifted block or port is one the environment
/// as written or a lower instance takes (the spec's own blocks or ports are closer than the shift).
pub fn apply(spec: &Spec, n: u8) -> Result<Spec, InstanceError> {
    if n == 0 || n > MAX {
        return Err(InstanceError::Number(n));
    }
    let copy = shifted(spec, n)?;
    // Instances are numbered up from the environment as written: instance n runs beside it and
    // beside the instances below n, so it takes nothing of theirs.
    for m in 0..n {
        let other = if m == 0 { spec.clone() } else { shifted(spec, m)? };
        if let Some(problem) = collision(&copy, n, &other, m) {
            return Err(InstanceError::Collision(Box::new(problem)));
        }
    }
    Ok(copy)
}

/// `spec` with its name, Docker blocks, IPv6 blocks and published ports moved for instance `n`.
fn shifted(spec: &Spec, n: u8) -> Result<Spec, InstanceError> {
    let mut copy = spec.clone();
    copy.name = format!("{}-{n}", spec.name);
    // The limit the environment's name has (a Kubernetes namespace's), its suffix included.
    if copy.name.len() > crate::validate::NAME_MAX {
        return Err(InstanceError::Name(copy.name));
    }
    // Docker blocks: the second octet moves by n (10.61.10.0/24 -> 10.63.10.0/24 for instance 2),
    // staying inside 10.0.0.0/8; networks keep their distance from each other.
    for (name, block) in docker_cidrs(spec) {
        let [_, second, third, fourth] = block.address().octets();
        // The low octets are the block's own, so the shifted one is a block too.
        let moved = second.checked_add(n).ok_or_else(|| InstanceError::Docker {
            network: name.clone(),
            cidr: block.to_string(),
            instance: n,
        })?;
        let Some(shifted) = Cidr::new(u32::from_be_bytes([10, moved, third, fourth]), block.prefix()) else {
            continue;
        };
        if let Some(net) = copy.networks.get_mut(&name) {
            net.docker = Some(NetworkDocker { cidr: shifted.to_string() });
        }
    }
    shift_ipv6(&mut copy, n)?;
    // Published host ports: 100 apart per instance (8080 -> 8180 for instance 1).
    for (machine, m) in &mut copy.machines {
        for port in &mut m.ports {
            if let Some(published) = port.publish {
                let shifted = u32::from(published) + 100 * u32::from(n);
                let moved = u16::try_from(shifted).map_err(|_| InstanceError::Port {
                    machine: machine.clone(),
                    port: published,
                    instance: n,
                })?;
                port.publish = Some(moved);
            }
        }
    }
    Ok(copy)
}

/// The first Docker block, IPv6 block or published port instance `n` (`copy`) takes that
/// `other` (the environment as written when `m` is 0, else instance `m`) takes too.
fn collision(copy: &Spec, n: u8, other: &Spec, m: u8) -> Option<Problem> {
    let theirs = if m == 0 {
        "the environment as written".to_owned()
    } else {
        format!("instance {m}")
    };
    if let Some((name, mine, taker, block)) = first_shared(docker_blocks(copy), &docker_blocks(other), Cidr::overlaps) {
        return Some(
            Problem::new(
                Code::Conflict,
                FieldPath::new("networks").key(&name).key("cidr"),
                format!("instance {n} moves this network's Docker block to {mine}, which overlaps network `{taker}`'s {block} in {theirs}: Docker refuses a network on a block another one holds"),
            )
            .fix(format!("an instance moves every Docker block by its number in the second octet: give the networks one second octet (10.61.10.0/24, 10.61.20.0/24), or second octets more than {n} apart")),
        );
    }
    if let Some((name, mine, taker, block)) = first_shared(ipv6_blocks(copy), &ipv6_blocks(other), Cidr6::overlaps) {
        return Some(
            Problem::new(
                Code::Conflict,
                FieldPath::new("networks").key(&name).key("ipv6").key("cidr"),
                format!("instance {n} moves this network's IPv6 block to {mine}, which is network `{taker}`'s {block} in {theirs}: Docker refuses a network on a block another one holds"),
            )
            .fix(format!("an instance moves every IPv6 block by its number in the third group: give the networks one third group and different second ones, or third groups more than {n} apart")),
        );
    }
    let (machine, mine, taker, _) = first_shared(published(copy), &published(other), |a, b| a == b)?;
    Some(
        Problem::new(
            Code::Conflict,
            FieldPath::new("machines").key(&machine).key("ports"),
            format!("instance {n} publishes port {mine} on the host, which machine `{taker}` publishes in {theirs}: two can't listen on one host port"),
        )
        .fix(format!(
            "an instance moves every published port by 100 times its number: publish ports that are not a multiple of 100 apart, or more than {} apart",
            100 * u32::from(n)
        )),
    )
}

/// The first of `mine` that `same` finds among `theirs`: its owner and value, then theirs.
fn first_shared<T: Copy>(mine: Vec<(String, T)>, theirs: &[(String, T)], same: impl Fn(T, T) -> bool) -> Option<(String, T, String, T)> {
    mine.into_iter().find_map(|(owner, value)| {
        let (holder, held) = theirs.iter().find(|(_, t)| same(value, *t))?;
        Some((owner, value, holder.clone(), *held))
    })
}

/// Whether network `name` is the environment's own: one outside it (`external`, `bridged`)
/// exists once, shared, and no instance makes it.
fn made(spec: &Spec, name: &str) -> bool {
    spec.networks.get(name).is_some_and(|net| !net.is_outside())
}

/// The Docker blocks of the networks the environment makes.
fn docker_blocks(spec: &Spec) -> Vec<(String, Cidr)> {
    docker_cidrs(spec).into_iter().filter(|(name, _)| made(spec, name)).collect()
}

/// The IPv6 blocks of the networks the environment makes.
fn ipv6_blocks(spec: &Spec) -> Vec<(String, Cidr6)> {
    spec.networks
        .iter()
        .filter(|(name, _)| made(spec, name))
        .filter_map(|(name, net)| Some((name.clone(), Cidr6::parse(&net.ipv6.as_ref()?.cidr)?)))
        .collect()
}

/// Every port published on the host, with its machine.
fn published(spec: &Spec) -> Vec<(String, u16)> {
    spec.machines
        .iter()
        .flat_map(|(machine, m)| m.ports.iter().filter_map(move |p| Some((machine.clone(), p.publish?))))
        .collect()
}

/// IPv6 blocks: the third group moves by n (`fd00:20::/64` -> `fd00:20:2::/64` for instance 2), on
/// every target, since Docker refuses two networks on one IPv6 subnet too.
fn shift_ipv6(copy: &mut Spec, n: u8) -> Result<(), InstanceError> {
    for (name, net) in &mut copy.networks {
        let Some(v6) = net.ipv6.as_mut() else { continue };
        let Some(block) = Cidr6::parse(&v6.cidr) else { continue };
        let moved = Cidr6::new(block.base().wrapping_add(u128::from(n) << 80)).ok_or_else(|| InstanceError::Ipv6 {
            network: name.clone(),
            cidr: v6.cidr.clone(),
            instance: n,
        })?;
        v6.cidr = moved.to_string();
    }
    Ok(())
}

/// A generated file's path and contents, moved from `.isoloom/` to the instance's folder.
#[must_use]
pub fn relocate(path: &str, contents: &str, n: u8) -> (String, String) {
    let (from, to) = (format!("{OUTPUT_DIR}/"), format!("{}/", output_dir(Some(n))));
    let path = match path.strip_prefix(&from) {
        Some(rest) => format!("{to}{rest}"),
        None => path.to_owned(),
    };
    (path, contents.replace(&from, &to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{lab, model};

    /// A web machine publishing port 80 on the host's `publish`, on the environment's network.
    fn web(publish: u16) -> Result<Spec, String> {
        model(&lab(&format!(
            "machines:\n  web:\n    docker: {{ image: nginx }}\n    networks: {{ lan: 10 }}\n    ports: [{{ port: 80, publish: {publish} }}]\n"
        )))
    }

    #[test]
    fn an_instance_writes_to_its_own_folder() {
        assert_eq!(output_dir(None), ".isoloom");
        assert_eq!(output_dir(Some(3)), ".isoloom-3");
    }

    #[test]
    fn instances_are_numbered_1_to_99() -> Result<(), String> {
        let spec = web(8080)?;
        for n in [0, 100] {
            assert!(matches!(apply(&spec, n), Err(InstanceError::Number(m)) if m == n), "instance {n}");
        }
        for n in [1, 99] {
            assert_eq!(apply(&spec, n).map(|s| s.name).ok(), Some(format!("t-{n}")), "instance {n}");
        }
        Ok(())
    }

    #[test]
    fn a_name_with_its_suffix_holds_55_characters() -> Result<(), String> {
        let mut spec = web(8080)?;
        spec.name = "a".repeat(53);
        assert_eq!(apply(&spec, 1).map(|s| s.name.len()).ok(), Some(55));
        spec.name = "a".repeat(54);
        assert!(matches!(apply(&spec, 1), Err(InstanceError::Name(name)) if name.len() == 56));
        Ok(())
    }

    #[test]
    fn docker_blocks_move_by_the_instance_in_the_second_octet() -> Result<(), String> {
        let copy = apply(&web(8080)?, 2).map_err(|e| e.to_string())?;
        assert_eq!(copy.networks["lan"].docker, Some(NetworkDocker { cidr: "10.2.0.0/24".into() }));
        assert_eq!(copy.networks["lan"].cidr, "10.0.0.0/24", "the machines keep the addresses the spec writes");
        Ok(())
    }

    #[test]
    fn a_docker_block_too_high_to_move_is_refused_not_wrapped() -> Result<(), String> {
        let spec = model(
            "version: 1\nname: t\nnetworks:\n  lan: { cidr: 10.0.0.0/24, docker: { cidr: 10.250.0.0/24 } }\nmachines:\n  web: { docker: { image: nginx }, networks: { lan: 10 } }\n",
        )?;
        let copy = apply(&spec, 5).map_err(|e| e.to_string())?;
        assert_eq!(copy.networks["lan"].docker, Some(NetworkDocker { cidr: "10.255.0.0/24".into() }));
        // Regression: 250 + 9 wrapped to 10.3.0.0/24, a block another network or instance may hold.
        assert!(matches!(
            apply(&spec, 9),
            Err(InstanceError::Docker { network, instance: 9, .. }) if network == "lan"
        ));
        Ok(())
    }

    #[test]
    fn published_ports_move_100_per_instance() -> Result<(), String> {
        let copy = apply(&web(8080)?, 3).map_err(|e| e.to_string())?;
        assert_eq!(copy.machines["web"].ports[0].publish, Some(8380));
        assert_eq!(copy.machines["web"].ports[0].port, 80, "the machine's own port stays");
        assert!(matches!(
            apply(&web(65_500)?, 1),
            Err(InstanceError::Port { machine, port: 65_500, instance: 1 }) if machine == "web"
        ));
        Ok(())
    }

    #[test]
    fn ipv6_blocks_move_by_the_instance_in_the_third_group() -> Result<(), String> {
        let spec = |cidr: &str| {
            model(&format!(
                "version: 1\nname: t\nnetworks:\n  lan: {{ cidr: 10.0.0.0/24, ipv6: {{ cidr: \"{cidr}\" }} }}\nmachines: {{}}\n"
            ))
        };
        let copy = apply(&spec("fd00:20::/64")?, 2).map_err(|e| e.to_string())?;
        assert_eq!(copy.networks["lan"].ipv6.as_ref().map(|v6| v6.cidr.as_str()), Some("fd00:20:2::/64"));
        assert!(matches!(
            apply(&spec("fdff:ffff:ffff::/64")?, 1),
            Err(InstanceError::Ipv6 { network, instance: 1, .. }) if network == "lan"
        ));
        Ok(())
    }

    /// Two networks whose Docker blocks are one apart in the second octet, and two ports 100
    /// apart: instance 1 of each lands on the other.
    fn close(lan: &str, dmz: &str, ports: (u16, u16)) -> Result<Spec, String> {
        model(&format!(
            "version: 1\nname: t\nnetworks:\n  lan: {{ cidr: {lan} }}\n  dmz: {{ cidr: {dmz} }}\nmachines:\n  web:\n    docker: {{ image: nginx }}\n    networks: {{ lan: 10 }}\n    ports: [{{ port: 80, publish: {} }}]\n  api:\n    docker: {{ image: nginx }}\n    networks: {{ dmz: 10 }}\n    ports: [{{ port: 80, publish: {} }}]\n",
            ports.0, ports.1
        ))
    }

    fn conflict(spec: &Spec, n: u8) -> Option<Problem> {
        match apply(spec, n) {
            Err(InstanceError::Collision(problem)) => Some(*problem),
            _ => None,
        }
    }

    #[test]
    fn an_instance_never_takes_a_block_of_the_environment_or_a_lower_instance() -> Result<(), String> {
        // 172.16 and 172.17 move to 10.16 and 10.17 on Docker: instance 1's lan is the base's dmz.
        let spec = close("172.16.0.0/24", "172.17.0.0/24", (8080, 9090))?;
        let p = conflict(&spec, 1).ok_or("instance 1 is refused")?;
        assert_eq!((p.code, p.path.as_str()), (Code::Conflict, "networks.lan.cidr"));
        assert!(
            p.reason.contains("10.17.0.0/24") && p.reason.contains("`dmz`") && p.reason.contains("the environment as written"),
            "{}",
            p.reason
        );
        // Blocks 3 apart: instances 1 and 2 fit, instance 3's lan is the base's dmz, instance 4's
        // is instance 1's.
        let apart = close("10.20.0.0/24", "10.23.0.0/24", (8080, 9090))?;
        for n in [1, 2] {
            assert!(apply(&apart, n).is_ok(), "instance {n}");
        }
        assert!(conflict(&apart, 3).is_some_and(|found| found.reason.contains("the environment as written")));
        assert!(conflict(&apart, 4).is_some_and(|found| found.reason.contains("instance 1")));
        // Blocks in one second octet move together and never meet.
        assert!(apply(&close("10.61.10.0/24", "10.61.20.0/24", (8080, 9090))?, MAX).is_ok());
        Ok(())
    }

    #[test]
    fn an_instance_never_publishes_a_port_of_the_environment_or_a_lower_instance() -> Result<(), String> {
        let spec = close("10.61.10.0/24", "10.61.20.0/24", (8080, 8180))?;
        let p = conflict(&spec, 1).ok_or("instance 1 is refused")?;
        assert_eq!((p.code, p.path.as_str()), (Code::Conflict, "machines.web.ports"));
        assert!(p.reason.contains("port 8180") && p.reason.contains("`api`"), "{}", p.reason);
        let apart = close("10.61.10.0/24", "10.61.20.0/24", (8080, 8380))?;
        assert!(apply(&apart, 2).is_ok());
        assert!(conflict(&apart, 4).is_some_and(|found| found.reason.contains("port 8480") && found.reason.contains("instance 1")));
        Ok(())
    }

    #[test]
    fn generated_files_and_the_paths_they_hold_move_to_the_instances_folder() {
        assert_eq!(
            relocate(".isoloom/compose.yml", "env_file: .isoloom/web.env", 2),
            (".isoloom-2/compose.yml".to_owned(), "env_file: .isoloom-2/web.env".to_owned())
        );
        assert_eq!(relocate("README.md", "plain", 2), ("README.md".to_owned(), "plain".to_owned()));
    }
}
