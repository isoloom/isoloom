//! `reach`: rules letting one network's machines reach another's ports.

use super::Problems;
use crate::model::Spec;

/// Each rule joins two declared, different networks, on real ports.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (i, r) in spec.reach.iter().enumerate() {
        for (field, net) in [("from", &r.from), ("to", &r.to)] {
            if !spec.networks.contains_key(net) {
                p.add(format!("reach[{i}].{field}"), format!("no network named `{net}`"));
            }
        }
        if r.from == r.to {
            p.add(format!("reach[{i}]"), "machines on the same network always reach each other; remove this rule");
        }
        if r.ports.contains(&0) {
            p.add(format!("reach[{i}].ports"), "port 0 isn't a port");
        }
    }
}
