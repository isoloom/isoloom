//! IPv6 on Kubernetes: the cluster picks pod addresses, so a machine's networks decide only its
//! Services' IP families. On an IPv6 network it asks for both families where the cluster has
//! them (`PreferDualStack`); on IPv6-only networks alone, for IPv6 alone.

use isoloom_plan::{Environment, MachinePlan};

use crate::objects::IpBlock;

/// A Service's `ipFamilyPolicy` and `ipFamilies`, from the machine's networks: nothing (the
/// cluster's default) when none is IPv6.
pub(crate) fn families(env: &Environment, mp: &MachinePlan) -> (Option<&'static str>, Vec<&'static str>) {
    let v6 = |n: &String| env.networks.get(n).and_then(|p| p.ipv6.as_ref());
    if !mp.addresses.keys().any(|n| v6(n).is_some()) {
        return (None, Vec::new());
    }
    if mp.addresses.keys().all(|n| v6(n).is_some_and(|v| v.only)) {
        return (Some("SingleStack"), vec!["IPv6"]);
    }
    (Some("PreferDualStack"), vec!["IPv4", "IPv6"])
}

/// The blocks "anyone" covers for a published port: every IPv4 address, and every IPv6 one
/// when the machine's Service has that family.
pub(crate) fn anyone(env: &Environment, mp: &MachinePlan) -> Vec<IpBlock> {
    let (_, fams) = families(env, mp);
    let mut out = Vec::new();
    if fams.is_empty() || fams.contains(&"IPv4") {
        out.push(IpBlock { cidr: "0.0.0.0/0".into() });
    }
    if fams.contains(&"IPv6") {
        out.push(IpBlock { cidr: "::/0".into() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    #[test]
    fn families_follow_the_networks() {
        let env = environment(
            "version: 1
name: v6
networks:
  four: { cidr: 10.1.0.0/24 }
  dual: { cidr: 10.2.0.0/24, ipv6: { cidr: \"fd00:2::/64\" } }
  six: { cidr: 10.3.0.0/24, internet: false, ipv6: { cidr: \"fd00:3::/64\", only: true } }
machines:
  a: { networks: { four: 10 }, docker: { image: nginx } }
  b: { networks: { four: 11, dual: 11 }, docker: { image: nginx } }
  c: { networks: { six: 12 }, docker: { image: nginx } }
",
        );
        let of = |m: &str| families(&env, &env.machines[m]);
        assert_eq!(of("a"), (None, vec![]));
        assert_eq!(of("b"), (Some("PreferDualStack"), vec!["IPv4", "IPv6"]));
        assert_eq!(of("c"), (Some("SingleStack"), vec!["IPv6"]));
        assert_eq!(anyone(&env, &env.machines["b"]).len(), 2);
        assert_eq!(anyone(&env, &env.machines["c"])[0].cidr, "::/0");
    }
}
