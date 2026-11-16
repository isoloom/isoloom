//! Nothing on the clouds is open to the whole internet inbound, as the security page and the
//! cloud guide say, but the one exception they name: Oracle Cloud lets ICMP's "fragmentation
//! needed" (and ICMPv6's "packet too big") in from anywhere, which path MTU discovery needs.

use isoloom_spec::model::Target;
use isoloom_targets::{Lab, generate, refusal};

use isoloom_test_support::{hcl, hcl_attribute, hcl_body, hcl_literals, hcl_text, hcl_values};

use crate::testing::{Outcome, examples};

/// The blocks that hold one inbound rule; those that hold a rule of either direction (a
/// resource's type, or a block's name) are inbound when their `direction` says so.
const INBOUND: [&str; 4] = ["ingress", "inbound", "inbound_rule", "ingress_security_rules"];
const EITHER: [&str; 3] = ["security_rule", "google_compute_firewall", "oci_core_network_security_group_security_rule"];

/// One inbound rule: every literal it holds, Azure's source prefix, and its description.
struct Rule {
    literals: Vec<String>,
    azure_sources: Vec<String>,
    description: Option<String>,
}

/// Every inbound rule of the parsed module `body`, its nested blocks searched.
fn inbound_rules(body: &hcl::Body) -> Vec<Rule> {
    let mut out = Vec::new();
    for block in body.blocks() {
        let kind = match block.identifier() {
            "resource" => block.labels().first().map(|l| l.as_str().to_owned()).unwrap_or_default(),
            other => other.to_owned(),
        };
        let direction = hcl_values(block.body(), "direction");
        let inbound = INBOUND.contains(&kind.as_str()) || (EITHER.contains(&kind.as_str()) && direction.iter().any(|d| d == "Inbound" || d == "INGRESS"));
        if inbound {
            out.push(Rule {
                literals: hcl_literals(block.body()),
                azure_sources: hcl_values(block.body(), "source_address_prefix"),
                description: hcl_attribute(block.body(), "description").map(|_| hcl_text(block.body(), "description")),
            });
        } else {
            out.extend(inbound_rules(block.body()));
        }
    }
    out
}

#[test]
fn no_cloud_rule_lets_the_whole_internet_in_but_path_mtu_discovery() -> Outcome {
    let mut rules = 0;
    for entry in std::fs::read_dir(examples())? {
        let dir = entry?.path();
        if !dir.join("isoloom.yml").is_file() {
            continue;
        }
        let lab = Lab::new(isoloom_spec::load(&dir)?);
        for target in [Target::CloudVm, Target::CloudDocker] {
            if !lab.possible().contains(&target) || refusal(&lab, target).unwrap().is_some() {
                continue;
            }
            for f in generate(&lab, target)?
                .iter()
                .filter(|f| std::path::Path::new(&f.path).extension().is_some_and(|e| e == "tf"))
            {
                for rule in inbound_rules(&hcl_body(&f.contents)) {
                    rules += 1;
                    let anywhere =
                        rule.literals.iter().any(|l| l == "0.0.0.0/0" || l == "::/0") || rule.azure_sources.iter().any(|s| s == "*" || s == "Internet");
                    let path_mtu = rule.description.as_deref() == Some("path MTU discovery") && f.path.contains("/oci/");
                    assert!(!anywhere || path_mtu, "{}: {}: {:?}", dir.display(), f.path, rule.literals);
                }
            }
        }
    }
    assert!(rules > 100, "{rules} inbound rules");
    Ok(())
}
