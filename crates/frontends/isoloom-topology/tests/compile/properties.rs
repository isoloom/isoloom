//! Properties of the compiler over generated drawings: a well-formed drawing compiles to a spec
//! Isoloom accepts (`isoloom validate` finds nothing), and any drawing at all compiles or says
//! what is wrong with the drawing, never with what the compiler wrote.

use std::num::NonZeroU16;

use indexmap::IndexMap;
use isoloom_spec::appliance::is_switch;
use isoloom_spec::net::NetworkBlock;
use isoloom_spec::syntax::{OsRef, PortProtocol};
use isoloom_spec::{Appliance, Tc};
use isoloom_topology::{Endpoint, Error, Kind, Lan, Link, Node, Service, Topology, compile};
use proptest::prelude::*;
use proptest::sample::{Index, select};

use crate::testing::yaml;

const IMAGES: &[&str] = &[
    "alpine:3.20",
    "nginx:1.27-alpine",
    "redis:7-alpine",
    "frrouting/frr:v8.5.2",
    "nicolaka/netshoot:v0.13",
];
const PROTOCOLS: &[PortProtocol] = &[PortProtocol::Tcp, PortProtocol::Udp, PortProtocol::Http, PortProtocol::Https, PortProtocol::Tls];
const ROUTERS: &[Appliance] = &[
    Appliance::CiscoIol,
    Appliance::CiscoVios,
    Appliance::CiscoCsr1000v,
    Appliance::CiscoC8000v,
    Appliance::CiscoDynamips,
];

/// A link condition Isoloom takes: at least one of delay, loss and rate, jitter only with a delay.
fn tc() -> impl Strategy<Value = Tc> {
    (
        prop::option::of(1u32..500),
        prop::option::of(1u32..50),
        prop::option::of(0.0f64..100.0),
        prop::option::of((1u32..1000, select(&["kbit", "mbit", "gbit", "kbps", "mbps"][..]))),
    )
        .prop_filter("one condition at least", |(d, _, l, r)| d.is_some() || l.is_some() || r.is_some())
        .prop_map(|(delay, jitter, loss, rate)| Tc {
            jitter: delay.and(jitter).map(|j| format!("{j}ms")),
            delay: delay.map(|d| format!("{d}ms")),
            loss,
            rate: rate.map(|(n, unit)| format!("{n}{unit}")),
        })
}

/// What a drawn machine is: a container, an appliance, or an OS with its steps.
fn kind() -> impl Strategy<Value = Kind> {
    // Every spelling Isoloom lists is an OS it reads.
    let os = select(isoloom_spec::os::spellings()).prop_filter_map("an OS Isoloom reads", |os| os.parse::<OsRef>().ok());
    prop_oneof![
        (select(IMAGES), any::<bool>()).prop_map(|(image, idle)| Kind::Container {
            image: image.into(),
            idle,
            appliance: None,
            config: None,
        }),
        (select(ROUTERS), prop::option::of("[a-z ]{1,20}")).prop_map(|(a, config)| {
            Kind::Container {
                image: if a == Appliance::CiscoDynamips {
                    String::new()
                } else {
                    "vrnetlab/cisco_iol:17.12.01".into()
                },
                idle: false,
                appliance: Some(a),
                config,
            }
        }),
        (os, prop::collection::vec(any::<bool>(), 0..3)).prop_map(|(os, steps)| {
            let script = if os.is_windows() { "ps1" } else { "sh" };
            let provision = steps
                .iter()
                .enumerate()
                .map(|(i, playbook)| format!("provision/step{i}.{}", if *playbook { "yml" } else { script }))
                .collect();
            Kind::Vm { os, provision }
        }),
    ]
}

/// A machine's ports: a number listed once per transport (a TCP and a UDP port may share one).
fn services() -> impl Strategy<Value = Vec<Service>> {
    prop::collection::vec((1u16.., select(PROTOCOLS)), 0..4).prop_map(|ports| {
        let mut out: Vec<Service> = Vec::new();
        for (port, protocol) in ports {
            let udp = protocol == PortProtocol::Udp;
            if !out.iter().any(|s| s.port.get() == port && (s.protocol == PortProtocol::Udp) == udp) {
                out.push(Service {
                    port: NonZeroU16::new(port).unwrap_or(NonZeroU16::MIN),
                    protocol,
                });
            }
        }
        out
    })
}

/// A machine: what it is, its ports, the LANs it joins (by index), and whether one address is
/// pinned there.
fn machine() -> impl Strategy<Value = (Kind, Vec<Service>, Vec<Index>, bool)> {
    (kind(), services(), prop::collection::vec(any::<Index>(), 1..3), any::<bool>())
}

/// A well-formed drawing: LANs (some of them a switch's VLANs), machines on them, at most one
/// access machine, perhaps an automatic router, and cables between machines.
fn drawing() -> impl Strategy<Value = Topology> {
    (
        1usize..4,
        0usize..3,
        prop::collection::vec((any::<bool>(), prop::option::of(tc()), any::<Index>()), 5),
        prop::collection::vec(machine(), 1..7),
        prop::option::of(any::<Index>()),
        any::<bool>(),
        prop::collection::vec((any::<Index>(), any::<Index>(), prop::option::of(tc())), 0..4),
    )
        .prop_map(|(plain, vlans, lans, machines, access, router, links)| {
            let mut topo = Topology {
                name: "generated-project".into(),
                nodes: IndexMap::new(),
                networks: lan_blocks(plain, vlans, &lans),
                links: Vec::new(),
            };
            add_machines(&mut topo, machines, access);
            if router {
                let all = topo.networks.keys().map(|n| (n.clone(), None)).collect();
                topo.nodes.insert("router".into(), node(Kind::Router {}, vec![], all));
            }
            add_cables(&mut topo, links);
            settle_gateways(&mut topo);
            topo
        })
}

/// Where a plain LAN's block sits in 10.<n>.0.0/24: the whole of it, or a smaller block not at
/// the start of its last octet (whose machines' octets start past it). Each holds every machine
/// a drawing has.
const PLAIN_BLOCKS: &[&str] = &["0/24", "128/25", "64/26", "32/27", "16/28"];

/// `plain` LANs in 10.<n>.0.0/24 (each block one of [`PLAIN_BLOCKS`]), and `vlans` VLANs of
/// switch `sw1` in 10.100.<id>.0/24.
fn lan_blocks(plain: usize, vlans: usize, looks: &[(bool, Option<Tc>, Index)]) -> IndexMap<String, Lan> {
    let mut lans = IndexMap::new();
    for (i, (internet, tc, block)) in looks.iter().enumerate().take(plain + vlans) {
        let (name, lan) = if i < plain {
            let cidr = format!("10.{}.0.{}", i + 1, block.get(PLAIN_BLOCKS));
            (format!("lan{i}"), lan(&cidr, *internet, tc.clone(), None))
        } else {
            let id = u16::try_from(10 * (i - plain + 1)).unwrap_or(10);
            (format!("vlan{id}"), lan(&format!("10.100.{id}.0/24"), *internet, None, Some(id)))
        };
        lans.insert(name, lan);
    }
    lans
}

fn lan(cidr: &str, internet: bool, tc: Option<Tc>, vlan: Option<u16>) -> Lan {
    Lan {
        cidr: cidr.into(),
        internet,
        switch: vlan.map(|_| "sw1".into()),
        vlan,
        device: None,
        tc,
    }
}

fn node(kind: Kind, services: Vec<Service>, networks: IndexMap<String, Option<u8>>) -> Node {
    Node {
        kind,
        access: false,
        services,
        networks,
    }
}

/// The machines, each on the LANs its indexes pick (a pinned address is unique per machine, and
/// pinned only on the LANs whose block has it).
fn add_machines(topo: &mut Topology, machines: Vec<(Kind, Vec<Service>, Vec<Index>, bool)>, access: Option<Index>) {
    let lans: Vec<String> = topo.networks.keys().cloned().collect();
    let count = machines.len();
    for (i, (kind, services, joins, pinned)) in machines.into_iter().enumerate() {
        let octet = u8::try_from(100 + i).ok().filter(|_| pinned);
        let networks = joins
            .iter()
            .map(|j| {
                let lan = j.get(&lans);
                (lan.clone(), octet.filter(|o| topo.networks.get(lan).is_some_and(|l| offers(l, *o))))
            })
            .collect();
        // An appliance's ports are its OS's own.
        let services = if matches!(kind, Kind::Container { appliance: Some(_), .. }) {
            vec![]
        } else {
            services
        };
        topo.nodes.insert(format!("m{i}"), node(kind, services, networks));
    }
    if let Some(a) = access
        && let Some((_, n)) = topo.nodes.get_index_mut(a.index(count))
        && !matches!(n.kind, Kind::Container { appliance: Some(_), .. })
    {
        n.access = true;
    }
}

/// Whether a machine may take `octet` on `lan`.
fn offers(lan: &Lan, octet: u8) -> bool {
    NetworkBlock::parse(&lan.cidr).and_then(|block| block.host(octet)).is_some()
}

/// Cables between two different machines, each on a port of its own.
fn add_cables(topo: &mut Topology, links: Vec<(Index, Index, Option<Tc>)>) {
    let machines: Vec<String> = topo
        .nodes
        .iter()
        .filter(|(_, n)| !matches!(n.kind, Kind::Router {}))
        .map(|(k, _)| k.clone())
        .collect();
    for (k, (a, b, tc)) in links.into_iter().enumerate() {
        let (a, b) = (a.get(&machines), b.get(&machines));
        if a != b {
            let end = |node: &String| Endpoint {
                node: node.clone(),
                port: format!("eth{}", k + 1),
            };
            topo.links.push(Link { a: end(a), b: end(b), tc });
        }
    }
}

/// A Cisco router is its LAN's gateway, at the gateway's address (so none is drawn for it), and a
/// LAN routed by its gateway has no link conditions of its own (Isoloom's router applies them).
fn settle_gateways(topo: &mut Topology) {
    for n in topo.nodes.values_mut() {
        if matches!(n.kind, Kind::Container { appliance: Some(a), .. } if !is_switch(a)) {
            n.networks.values_mut().for_each(|octet| *octet = None);
        }
    }
    let routed: Vec<String> = topo
        .nodes
        .values()
        .filter(|n| matches!(n.kind, Kind::Container { appliance: Some(a), .. } if !is_switch(a)))
        .flat_map(|n| n.networks.keys().cloned())
        .collect();
    for (name, lan) in &mut topo.networks {
        if routed.contains(name) {
            lan.tc = None;
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn a_well_formed_drawing_compiles_to_a_spec_isoloom_accepts(topo in drawing()) {
        let compiled = compile(&topo).map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let project = isoloom_targets::Lab::parse(&yaml(&compiled)).map_err(|p| TestCaseError::fail(format!("{p:?}")))?;
        let problems: Vec<String> = project.problems().iter().map(ToString::to_string).collect();
        // Which target runs a drawing is Isoloom's rule (`Lab::possible`): one none runs (an
        // access VM with ports and steps beside containers only: hybrid doesn't count the access
        // machine as a mix, and a container has no command for its steps) is refused for that
        // alone, never for what the compiler wrote.
        let no_target = project.possible().is_empty()
            && matches!(problems.as_slice(), [only] if only.contains("no target is possible"));
        prop_assert!(problems.is_empty() || no_target, "{}\n{}", problems.join("\n"), yaml(&compiled));
    }

    #[test]
    fn any_drawing_compiles_or_says_what_is_wrong_with_the_drawing(topo in any_drawing()) {
        match compile(&topo) {
            Ok(compiled) => prop_assert!(isoloom_spec::parse(&yaml(&compiled)).is_ok()),
            Err(e @ (Error::Unreadable(_) | Error::PortName { .. } | Error::Yaml(_))) => {
                prop_assert!(false, "the compiler wrote what Isoloom doesn't read: {e}");
            }
            Err(_) => {}
        }
    }
}

/// A name, perhaps one Isoloom refuses.
const ANY_NAME: &str = "[a-zA-Z0-9 ._:-]{0,10}";

/// Any LANs: names, blocks and VLANs at random.
fn any_lans() -> impl Strategy<Value = IndexMap<String, Lan>> {
    let lan = (
        ANY_NAME,
        "[0-9./a-z]{0,18}",
        any::<bool>(),
        prop::option::of((ANY_NAME, any::<u16>())),
        prop::option::of(tc()),
    );
    prop::collection::vec(lan, 0..4).prop_map(|lans| {
        lans.into_iter()
            .map(|(name, cidr, internet, vlan, tc)| {
                let (switch, vlan) = vlan.map_or((None, None), |(s, id)| (Some(s), Some(id)));
                (
                    name,
                    Lan {
                        cidr,
                        internet,
                        switch,
                        vlan,
                        device: None,
                        tc,
                    },
                )
            })
            .collect()
    })
}

/// Any nodes: names, kinds, ports and LANs (known or not) at random.
fn any_nodes() -> impl Strategy<Value = IndexMap<String, Node>> {
    let joins = prop::collection::vec((ANY_NAME, any::<Option<u8>>()), 0..3);
    prop::collection::vec((ANY_NAME, kind(), services(), joins, any::<bool>()), 0..6).prop_map(|nodes| {
        nodes
            .into_iter()
            .map(|(name, kind, services, on, access)| {
                (
                    name,
                    Node {
                        kind,
                        access,
                        services,
                        networks: on.into_iter().collect(),
                    },
                )
            })
            .collect()
    })
}

/// Any drawing: names, blocks, addresses and cables picked at random, mistakes included.
fn any_drawing() -> impl Strategy<Value = Topology> {
    let end = (ANY_NAME, ANY_NAME).prop_map(|(node, port)| Endpoint { node, port });
    let links = prop::collection::vec((end.clone(), end), 0..4);
    (ANY_NAME, any_lans(), any_nodes(), links).prop_map(|(name, networks, nodes, links)| Topology {
        name,
        networks,
        nodes,
        links: links.into_iter().map(|(a, b)| Link { a, b, tc: None }).collect(),
    })
}
