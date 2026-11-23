//! The plan a caller shows: the spec, its problems, the files every target generates, and
//! each machine's addresses.

use indexmap::IndexMap;
use isoloom_spec::model;
use isoloom_targets::Lab;
use isoloom_topology::{Compiled, Topology};
use serde::{Deserialize, Serialize};

use crate::{Error, File, Result};

/// What the engine makes of a topology, as the caller reads it (JSON).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// Whether the topology compiled (`false`: `error` says why). `true` says nothing of Isoloom's
    /// verdict on the spec: read `problems` too, which hold what it finds wrong (and no file is
    /// generated then). Named so in the JSON the topology editors read.
    pub ok: bool,
    /// Why the topology can't be compiled.
    pub error: Option<String>,
    /// The spec the caller shows and the user keeps.
    pub isoloom_yml: Option<String>,
    /// What Isoloom finds wrong with the spec, in the words of `isoloom validate` (code, field,
    /// line, reason and fix).
    pub problems: Vec<String>,
    /// Every generated file, with the target's own paths (for example `.isoloom/docker/compose.yml`).
    pub files: Vec<File>,
    /// Targets the spec allows but that generate nothing, each with the reason.
    pub skipped: Vec<String>,
    /// Each machine's addresses, for the caller to show: machine → [(network, IPv4)].
    pub addresses: IndexMap<String, Vec<(String, String)>>,
}

impl Plan {
    /// The plan of a topology that gives none, for `error`.
    pub(crate) fn failed(error: &dyn std::fmt::Display) -> Plan {
        Plan {
            ok: false,
            error: Some(error.to_string()),
            isoloom_yml: None,
            problems: vec![],
            files: vec![],
            skipped: vec![],
            addresses: IndexMap::new(),
        }
    }
}

/// Turns a topology (the JSON a topology editor produces) into the spec and everything generated from it.
#[must_use]
pub(crate) fn plan(topology_json: &str) -> Plan {
    match compiled(topology_json) {
        Ok((topo, compiled)) => {
            let (yaml, spec, _) = compiled.into_parts();
            planned(&topo, yaml, spec)
        }
        Err(e) => Plan::failed(&e),
    }
}

/// The plan of a compiled topology, its spec's text `yaml` and the spec: Isoloom's problems,
/// else every target's files.
pub(crate) fn planned(topo: &Topology, yaml: String, spec: isoloom_spec::Spec) -> Plan {
    let project = Lab::new(spec);
    let problems: Vec<String> = project.problems().iter().map(ToString::to_string).collect();
    // Isoloom's generators assume a valid spec (an unattached machine has no address to give,
    // for example), so nothing is generated until validation passes.
    let (generated, skipped) = if problems.is_empty() {
        isoloom_targets::generate_all(&project)
    } else {
        (vec![], vec![])
    };
    Plan {
        addresses: addresses(&project.spec(), topo),
        ok: true,
        error: None,
        isoloom_yml: Some(yaml),
        problems,
        files: generated.into_iter().map(File::from).collect(),
        skipped: skipped.iter().map(ToString::to_string).collect(),
    }
}

/// The topology and its compiled spec.
pub(crate) fn compiled(topology_json: &str) -> Result<(Topology, Compiled)> {
    let topo: Topology = serde_json::from_str(topology_json).map_err(Error::Malformed)?;
    let compiled = isoloom_topology::compile(&topo)?;
    Ok((topo, compiled))
}

/// Every machine's IPv4 addresses, as Isoloom gives them ([`isoloom_plan::addressing::address`]).
/// Networks go by the topology's names: the network a VLAN becomes once lowered (Isoloom names
/// it, and records the LAN and VLAN id it came from) goes by the key the topology gave that VLAN.
fn addresses(spec: &model::Spec, topo: &Topology) -> IndexMap<String, Vec<(String, String)>> {
    let topology_name = |net: &String| -> Option<&str> {
        let (lan, id) = spec.networks.get(net)?.vlan.as_ref()?;
        topo.vlan_lan(lan, *id)
    };
    let address = |net: &String, octet: u8| {
        let shown = topology_name(net).unwrap_or(net).to_owned();
        (shown, isoloom_plan::addressing::address(spec, net, octet).to_string())
    };
    spec.machines
        .iter()
        .map(|(name, m)| (name.clone(), m.networks.iter().map(|(net, octet)| address(net, *octet)).collect()))
        .collect()
}
