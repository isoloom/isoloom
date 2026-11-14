//! What the compile tests share: drawings to start from, and the checks every compiled spec
//! passes.

use std::error::Error;

use indexmap::IndexMap;
use isoloom_spec::syntax::Octets;
use isoloom_topology::{Compiled, Endpoint, Kind, Lan, Link, Node, Topology, compile};

/// A test's outcome: a failed step is its report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn Error>>;

/// A node of `kind`, on no LAN, with no ports.
pub(crate) fn node(kind: Kind) -> Node {
    Node {
        kind,
        access: false,
        services: vec![],
        networks: IndexMap::new(),
    }
}

/// A container from `image`, kept running idle or not.
pub(crate) fn container(image: &str, idle: bool) -> Kind {
    Kind::Container {
        image: image.into(),
        idle,
        appliance: None,
        config: None,
    }
}

/// A machine of OS `os` (`debian@12`) with its provisioning steps.
pub(crate) fn vm(os: &str, provision: &[&str]) -> Outcome<Kind> {
    Ok(Kind::Vm {
        os: os.parse()?,
        provision: provision.iter().map(ToString::to_string).collect(),
    })
}

/// A LAN with block `cidr`, reaching the internet.
pub(crate) fn lan(cidr: &str) -> Lan {
    Lan {
        cidr: cidr.into(),
        internet: true,
        switch: None,
        vlan: None,
        device: None,
        tc: None,
    }
}

/// Port `port` of `node`.
pub(crate) fn ep(node: &str, port: &str) -> Endpoint {
    Endpoint {
        node: node.into(),
        port: port.into(),
    }
}

/// A cable between two ports, with no conditions.
pub(crate) fn cable(a: Endpoint, b: Endpoint) -> Link {
    Link { a, b, tc: None }
}

/// client -- router -- server, the router a container and the others VMs with a step.
pub(crate) fn three_node() -> Outcome<Topology> {
    let mut nodes = IndexMap::new();
    nodes.insert(
        "client".into(),
        Node {
            access: true,
            ..node(vm("debian@12", &["provision/x.sh"])?)
        },
    );
    nodes.insert("router".into(), node(container("frrouting/frr:v8.5.2", false)));
    nodes.insert("server".into(), node(vm("debian@12", &["provision/x.sh"])?));
    Ok(Topology {
        name: "three-node".into(),
        nodes,
        networks: IndexMap::new(),
        links: vec![
            cable(ep("client", "eth0"), ep("router", "eth0")),
            cable(ep("router", "eth1"), ep("server", "eth0")),
        ],
    })
}

/// Two LANs, office and servers, with the router on both (at .20 on office), and no cables.
pub(crate) fn two_lans() -> Outcome<Topology> {
    let mut topo = three_node()?;
    topo.links.clear();
    topo.networks.insert("office".into(), lan("10.10.1.0/24"));
    topo.networks.insert("servers".into(), lan("10.10.2.0/24"));
    join(&mut topo, "client", "office", None)?;
    join(&mut topo, "server", "servers", None)?;
    join(&mut topo, "router", "office", Some(20))?;
    join(&mut topo, "router", "servers", None)?;
    Ok(topo)
}

/// Puts `node` on `lan`, at `octet` or the next free address.
pub(crate) fn join(topo: &mut Topology, node: &str, lan: &str, octet: Option<u8>) -> Outcome {
    let n = topo.nodes.get_mut(node).ok_or_else(|| format!("no node {node}"))?;
    n.networks.insert(lan.into(), octet);
    Ok(())
}

/// The spec's text, as `isoloom.yml` gets it.
pub(crate) fn yaml(c: &Compiled) -> String {
    c.clone().into_parts().0
}

/// The spec as Isoloom reads it.
pub(crate) fn spec(c: &Compiled) -> isoloom_spec::Spec {
    c.clone().into_parts().1
}

/// The project's own files.
pub(crate) fn files(c: &Compiled) -> Vec<isoloom_topology::ProjectFile> {
    c.clone().into_parts().2
}

/// The compiled spec, which Isoloom reads and finds nothing wrong with (`isoloom validate`).
pub(crate) fn accepted(topo: &Topology) -> Outcome<Compiled> {
    let compiled = compile(topo)?;
    let project = isoloom_targets::Lab::parse(&yaml(&compiled)).map_err(|p| format!("Isoloom doesn't read it: {p:?}"))?;
    let problems: Vec<String> = project.problems().iter().map(ToString::to_string).collect();
    if !problems.is_empty() {
        return Err(format!("{}\n{}", problems.join("\n"), yaml(&compiled)).into());
    }
    Ok(compiled)
}

/// Machine `m`'s address on network `net`.
pub(crate) fn octet(c: &Compiled, m: &str, net: &str) -> Outcome<u8> {
    let s = spec(c);
    let octets = s
        .machines
        .get(m)
        .and_then(|m| m.networks.get(net))
        .ok_or_else(|| format!("{m} isn't on {net}"))?;
    match octets {
        Octets::One(o) => Ok(*o),
        Octets::Several(list) => list.first().copied().ok_or_else(|| format!("{m} has no address on {net}").into()),
    }
}
