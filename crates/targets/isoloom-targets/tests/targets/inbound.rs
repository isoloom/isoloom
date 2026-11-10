//! Nothing on the clouds is open to the whole internet inbound, as the security page and the
//! cloud guide say, but the one exception they name: Oracle Cloud lets ICMP's "fragmentation
//! needed" (and ICMPv6's "packet too big") in from anywhere, which path MTU discovery needs.

use isoloom_spec::model::Target;
use isoloom_targets::{Lab, generate, refusal};

use crate::testing::{Outcome, examples};

/// The blocks that hold one inbound rule, by the line that opens them; those that hold a rule of
/// either direction are inbound when their body says so.
const INBOUND: [&str; 4] = ["ingress {", "inbound {", "inbound_rule {", "ingress_security_rules {"];
const EITHER: [&str; 3] = [
    "security_rule {",
    "resource \"google_compute_firewall\"",
    "resource \"oci_core_network_security_group_security_rule\"",
];

/// The block that opens on the line at `start` of `text`, to its closing brace.
fn block(text: &str, start: usize) -> &str {
    let rest = text.get(start..).unwrap_or_default();
    let mut depth = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 1 => return rest.get(..=i).unwrap_or(rest),
            '}' => depth -= 1,
            _ => {}
        }
    }
    rest
}

/// Every inbound rule of the Terraform module `text`, its whitespace folded.
fn inbound_rules(text: &str) -> Vec<String> {
    let mut at = 0;
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        let opens = line.trim();
        if INBOUND.contains(&opens) || EITHER.iter().any(|e| opens.starts_with(e)) {
            let body = block(text, at).split_whitespace().collect::<Vec<_>>().join(" ");
            let inbound = INBOUND.contains(&opens) || body.contains("direction = \"Inbound\"") || body.contains("direction = \"INGRESS\"");
            if inbound {
                out.push(body);
            }
        }
        at += line.len();
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
            if !lab.possible().contains(&target) || refusal(&lab, target).is_some() {
                continue;
            }
            for f in generate(&lab, target)?.iter().filter(|f| std::path::Path::new(&f.path).extension().is_some_and(|e| e == "tf")) {
                for rule in inbound_rules(&f.contents) {
                    rules += 1;
                    let anywhere = [
                        "\"0.0.0.0/0\"",
                        "\"::/0\"",
                        "source_address_prefix = \"*\"",
                        "source_address_prefix = \"Internet\"",
                    ]
                    .iter()
                    .any(|a| rule.contains(a));
                    let path_mtu = rule.contains("description = \"path MTU discovery\"") && f.path.contains("/oci/");
                    assert!(!anywhere || path_mtu, "{}: {}: {rule}", dir.display(), f.path);
                }
            }
        }
    }
    assert!(rules > 100, "{rules} inbound rules");
    Ok(())
}
