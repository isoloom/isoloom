//! The spec's constraints as the environment applies them: its proxy's `no_proxy` completed with
//! what the environment reaches without the proxy, worked out once for every backend.

use std::collections::HashSet;

use isoloom_spec::model::{Constraints, Spec};
use isoloom_spec::net::docker_cidrs;

/// The constraints of `spec` (laid out for the target), the proxy's `no_proxy` being
/// [`no_proxy`]'s list; `None` when the spec has none.
#[must_use]
pub(crate) fn of(spec: &Spec) -> Option<Constraints> {
    let mut c = spec.constraints.clone()?;
    if let Some(p) = c.proxy.as_mut() {
        p.no_proxy = no_proxy(spec);
    }
    Some(c)
}

/// What is reached without the proxy: localhost, the environment's machine names and network
/// blocks (as laid out, and Docker's), then the spec's own list, each once.
fn no_proxy(spec: &Spec) -> Vec<String> {
    let local = ["localhost", "127.0.0.1", "::1"].map(str::to_owned);
    let names = spec.machines.iter().flat_map(|(name, m)| std::iter::once(name).chain(&m.aliases).cloned());
    let blocks = spec.networks.values().map(|n| n.cidr.clone());
    let docker = docker_cidrs(spec).into_iter().map(|(_, c)| c.to_string());
    let own = spec.constraints.iter().flat_map(|c| &c.proxy).flat_map(|p| p.no_proxy.iter().cloned());
    let mut seen = HashSet::new();
    local
        .into_iter()
        .chain(names)
        .chain(blocks)
        .chain(docker)
        .chain(own)
        .filter(|x| seen.insert(x.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use crate::testing;

    #[test]
    fn what_bypasses_the_proxy_is_local_then_the_environment_then_the_spec_s_own_each_once() {
        let env = testing::environment(
            "version: 1\nname: c\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, aliases: [www], os: debian@12 }\nconstraints: { proxy: { http: \"http://p:3128\", no_proxy: [.corp, web] } }\n",
            Target::Vagrant,
        );
        let no_proxy = env.constraints.and_then(|c| c.proxy).map(|p| p.no_proxy).unwrap_or_default();
        assert_eq!(no_proxy, ["localhost", "127.0.0.1", "::1", "web", "www", "10.9.0.0/24", ".corp"]);
    }
}
