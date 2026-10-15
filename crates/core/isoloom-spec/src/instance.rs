//! Instances: the same environment several times on one host. An instance number (1 to 99)
//! gives a copy its own names and its own room on the host, so two instances of a spec never
//! collide: the name gets a suffix (so Compose project, container, network, VM and namespace
//! names differ), the Docker networks move to other blocks (Docker refuses two networks on one
//! subnet), published host ports shift, and the files go to `.isoloom-<n>/` next to the
//! committed `.isoloom/`. Local VM networks are isolated by name already (VirtualBox internal
//! networks), so machines keep the addresses the spec writes.

use crate::model::{NetworkDocker, Spec};
use crate::validate::docker_cidrs;
use crate::validate::{Cidr, Cidr6};
use crate::{Error, Result};

/// The highest instance number.
pub const MAX: u8 = 99;

/// Where an instance's generated files go.
#[must_use]
pub fn output_dir(instance: Option<u8>) -> String {
    match instance {
        Some(n) => format!(".isoloom-{n}"),
        None => ".isoloom".to_owned(),
    }
}

/// The spec as instance `n` of itself (validated specs only).
///
/// # Errors
///
/// [`Error::InstanceNumber`] outside 1 to [`MAX`], [`Error::InstanceName`] when the name with its
/// suffix is too long, and [`Error::InstancePort`] or [`Error::InstanceIpv6`] when a published
/// port or an IPv6 block leaves no room to shift.
pub fn apply(spec: &Spec, n: u8) -> Result<Spec> {
    if n == 0 || n > MAX {
        return Err(Error::InstanceNumber(n));
    }
    let mut copy = spec.clone();
    copy.name = format!("{}-{n}", spec.name);
    if copy.name.len() > 55 {
        return Err(Error::InstanceName(copy.name));
    }
    // Docker blocks: the second octet moves by n (10.61.10.0/24 -> 10.63.10.0/24 for instance 2),
    // staying inside 10.0.0.0/8; networks keep their distance from each other.
    for (name, block) in docker_cidrs(spec) {
        let [_, second, third, fourth] = std::net::Ipv4Addr::from(block.base).octets();
        let shifted = Cidr {
            base: u32::from_be_bytes([10, second.wrapping_add(n), third, fourth]),
            len: block.len,
        };
        if let Some(net) = copy.networks.get_mut(&name) {
            net.docker = Some(NetworkDocker {
                cidr: format!("{}/{}", std::net::Ipv4Addr::from(shifted.base), shifted.len),
            });
        }
    }
    shift_ipv6(&mut copy, n)?;
    // Published host ports: 100 apart per instance (8080 -> 8180 for instance 1).
    for (machine, m) in &mut copy.machines {
        for port in &mut m.ports {
            if let Some(published) = port.publish {
                let shifted = u32::from(published) + 100 * u32::from(n);
                let moved = u16::try_from(shifted).map_err(|_| Error::InstancePort {
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

/// IPv6 blocks: the third group moves by n (`fd00:20::/64` -> `fd00:20:2::/64` for instance 2), on
/// every target, since Docker refuses two networks on one IPv6 subnet too.
fn shift_ipv6(copy: &mut Spec, n: u8) -> Result<()> {
    for (name, net) in &mut copy.networks {
        let Some(v6) = net.ipv6.as_mut() else { continue };
        let Some(block) = Cidr6::parse(&v6.cidr) else { continue };
        let moved = Cidr6 {
            base: block.base.wrapping_add(u128::from(n) << 80),
        };
        if Cidr6::parse(&moved.text()).is_none() {
            return Err(Error::InstanceIpv6 {
                network: name.clone(),
                cidr: v6.cidr.clone(),
                instance: n,
            });
        }
        v6.cidr = moved.text();
    }
    Ok(())
}

/// A generated file's path and contents, moved from `.isoloom/` to the instance's folder.
#[must_use]
pub fn relocate(path: &str, contents: &str, n: u8) -> (String, String) {
    let dir = output_dir(Some(n));
    let path = match path.strip_prefix(".isoloom/") {
        Some(rest) => format!("{dir}/{rest}"),
        None => path.to_owned(),
    };
    (path, contents.replace(".isoloom/", &format!("{dir}/")))
}
