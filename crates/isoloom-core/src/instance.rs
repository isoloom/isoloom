//! Instances: the same environment several times on one host. An instance number (1 to 99)
//! gives a copy its own names and its own room on the host, so two instances of a spec never
//! collide: the name gets a suffix (so Compose project, container, network, VM and namespace
//! names differ), the Docker networks move to other blocks (Docker refuses two networks on one
//! subnet), published host ports shift, and the files go to `.isoloom-<n>/` next to the
//! committed `.isoloom/`. Local VM networks are isolated by name already (VirtualBox internal
//! networks), so machines keep the addresses the spec writes.

use crate::generate::docker_cidrs;
use crate::model::{NetworkDocker, Spec};
use crate::validate::Cidr;

/// The highest instance number.
pub const MAX: u8 = 99;

/// Where an instance's generated files go.
pub fn output_dir(instance: Option<u8>) -> String {
    match instance {
        Some(n) => format!(".isoloom-{n}"),
        None => ".isoloom".to_string(),
    }
}

/// The spec as instance `n` of itself (validated specs only).
pub fn apply(spec: &Spec, n: u8) -> Result<Spec, String> {
    if n == 0 || n > MAX {
        return Err(format!("an instance number is 1 to {MAX}"));
    }
    let mut s = spec.clone();
    s.name = format!("{}-{n}", spec.name);
    if s.name.len() > 55 {
        return Err(format!("`{}` is too long for an instance name (at most 55 characters with the suffix)", s.name));
    }
    // Docker blocks: the second octet moves by n (10.61.10.0/24 -> 10.63.10.0/24 for instance 2),
    // staying inside 10.0.0.0/8; networks keep their distance from each other.
    for (name, block) in docker_cidrs(spec) {
        let [_, b, c, d] = std::net::Ipv4Addr::from(block.base).octets();
        let shifted = Cidr {
            base: u32::from_be_bytes([10, b.wrapping_add(n), c, d]),
            len: block.len,
        };
        s.networks.get_mut(&name).expect("same networks").docker = Some(NetworkDocker {
            cidr: format!("{}/{}", std::net::Ipv4Addr::from(shifted.base), shifted.len),
        });
    }
    // Published host ports: 100 apart per instance (8080 -> 8180 for instance 1).
    for (mname, m) in s.machines.iter_mut() {
        for svc in m.services.iter_mut() {
            if let Some(p) = svc.publish {
                let shifted = u32::from(p) + 100 * u32::from(n);
                svc.publish =
                    Some(u16::try_from(shifted).map_err(|_| {
                        format!("machines.{mname}.services: published port {p} leaves no room for instance {n} (ports shift by 100 per instance)")
                    })?);
            }
        }
    }
    Ok(s)
}

/// A generated file's path and contents, moved from `.isoloom/` to the instance's folder.
pub fn relocate(path: &str, contents: &str, n: u8) -> (String, String) {
    let dir = output_dir(Some(n));
    let path = match path.strip_prefix(".isoloom/") {
        Some(rest) => format!("{dir}/{rest}"),
        None => path.to_string(),
    };
    (path, contents.replace(".isoloom/", &format!("{dir}/")))
}
