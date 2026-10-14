//! The environment's networks as labels and NetworkPolicies: a pod carries a label per network
//! it is on; ingress is denied by default, then open within each network and along `reach`;
//! offline pods keep their egress inside the environment (and DNS).

use isoloom_plan::Environment;

use crate::objects::{Labels, Manifest, NetworkPolicySpec, Peer, PolicyPort, Rule, Selector};

/// The label naming a machine's pods.
pub(crate) const MACHINE_LABEL: &str = "isoloom.com/machine";
/// The label naming a check's pod.
pub(crate) const CHECK_LABEL: &str = "isoloom.com/check";

/// The label of a network's members.
pub(crate) fn net_label(net: &str) -> String {
    format!("net.isoloom.com/{net}")
}

/// The members of `net`.
fn members(net: &str) -> Selector {
    Selector::of(&net_label(net), "member")
}

/// The labels a pod on these networks carries (what the network policies select): the
/// machine's name first, when it is one.
pub(crate) fn pod_labels<'a>(machine: Option<&str>, nets: impl IntoIterator<Item = &'a String>) -> Labels {
    let own = machine.map(|m| (MACHINE_LABEL.to_string(), m.to_string()));
    own.into_iter().chain(nets.into_iter().map(|n| (net_label(n), "member".to_string()))).collect()
}

/// An egress policy for the pods `selector` matches: other pods of the environment, and DNS.
pub(crate) fn offline(name: &str, selector: Selector) -> Manifest {
    let dns = |protocol| PolicyPort {
        port: 53,
        protocol: Some(protocol),
    };
    let environment = Rule {
        to: vec![Peer::pods(Selector::default())],
        ..Rule::default()
    };
    let resolver = Rule {
        to: vec![Peer {
            namespace_selector: Some(Selector::of("kubernetes.io/metadata.name", "kube-system")),
            ..Peer::default()
        }],
        ports: vec![dns("UDP"), dns("TCP")],
        ..Rule::default()
    };
    Manifest::network_policy(
        name,
        NetworkPolicySpec {
            pod_selector: selector,
            policy_types: vec!["Egress"],
            egress: vec![environment, resolver],
            ..NetworkPolicySpec::default()
        },
    )
}

/// The networks' policies: deny by default, then each network's machines reach each other,
/// then the `reach` rules.
pub(crate) fn policies(env: &Environment) -> Vec<Manifest> {
    let deny = Manifest::network_policy(
        "isoloom-default-deny",
        NetworkPolicySpec {
            policy_types: vec!["Ingress"],
            ..NetworkPolicySpec::default()
        },
    );
    let within = env.networks.keys().map(|net| {
        let rule = Rule {
            from: vec![Peer::pods(members(net))],
            ..Rule::default()
        };
        let spec = NetworkPolicySpec {
            pod_selector: members(net),
            ingress: vec![rule],
            ..NetworkPolicySpec::default()
        };
        Manifest::network_policy(&format!("net-{net}"), spec)
    });
    let mut out = vec![deny];
    out.extend(within);
    out.extend(reach(env));
    out
}

/// One policy per `reach` rule: the `from` network's members into the `to` network's, on its
/// ports (both protocols) or all of them.
fn reach(env: &Environment) -> Vec<Manifest> {
    let both = |p: &u16| {
        [
            PolicyPort {
                port: *p,
                protocol: Some("TCP"),
            },
            PolicyPort {
                port: *p,
                protocol: Some("UDP"),
            },
        ]
    };
    env.spec()
        .reach
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let rule = Rule {
                from: vec![Peer::pods(members(&r.from))],
                ports: r.ports.iter().flat_map(both).collect(),
                ..Rule::default()
            };
            let spec = NetworkPolicySpec {
                pod_selector: members(&r.to),
                ingress: vec![rule],
                ..NetworkPolicySpec::default()
            };
            // The index keeps the name unique: `reach-{from}-{to}` alone collides when a network
            // name contains a dash (reach a-b→c and a→b-c would both be `reach-a-b-c`).
            Manifest::network_policy(&format!("reach-{i}-{}-{}", r.from, r.to), spec)
        })
        .collect()
}
