//! A machine's resolver options (`dns.options`): each one resolv.conf's `options` line takes,
//! as `name` or `name:value`, so every target writes it as it is. And the environment's private
//! zone (`dns:` at the top level): a domain every platform's DNS accepts.

use super::Problems;
use super::machines::dns_name;
use crate::model::Spec;

/// The top-level `dns`: a domain of two labels or more, and only with `private: true`.
pub(super) fn private(spec: &Spec, p: &mut Problems) {
    let Some(dns) = &spec.dns else { return };
    let Some(domain) = &dns.domain else { return };
    if !dns.private {
        p.add("dns.domain", "the domain names the private zone: set `dns.private: true`, or drop `domain`");
    }
    if !dns_name(domain) || !domain.contains('.') {
        p.add(
            "dns.domain",
            format!("`{domain}` isn't a domain of two labels or more like lab.internal (lowercase, no trailing dot)"),
        );
    }
}

/// Every machine's `dns.options`.
pub(super) fn options(spec: &Spec, p: &mut Problems) {
    for (name, m) in &spec.machines {
        let Some(dns) = &m.dns else { continue };
        for (i, o) in dns.options.iter().enumerate() {
            if !option(o) {
                p.add(
                    format!("machines.{name}.dns.options[{i}]"),
                    format!("`{o}` isn't a resolver option: write `name` or `name:value`, as resolv.conf's `options` line (ndots:2, timeout:1, rotate)"),
                );
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for (i, o) in dns.options.iter().enumerate() {
            let key = o.split(':').next().unwrap_or(o);
            if seen.contains(&key) {
                p.add(format!("machines.{name}.dns.options[{i}]"), format!("`{key}` is set twice: keep one"));
            }
            seen.push(key);
        }
    }
}

/// `name` (lowercase letters, digits and `-`, starting with a letter) or `name:value` (a
/// number).
fn option(o: &str) -> bool {
    let (name, value) = o.split_once(':').map_or((o, None), |(n, v)| (n, Some(v)));
    let name_ok = name.starts_with(|c: char| c.is_ascii_lowercase()) && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    name_ok && value.is_none_or(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::option;
    use crate::model::Spec;

    fn problems(dns: &str) -> Vec<String> {
        let yaml = format!(
            "version: 1\nname: lab\n{dns}\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }} }}\n"
        );
        let spec: Spec = crate::testing::model(&yaml).unwrap();
        crate::validate::validate(&spec).into_iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn the_private_zone_takes_a_domain_of_two_labels_or_more() {
        assert_eq!(problems("dns: { private: true }"), Vec::<String>::new());
        assert_eq!(problems("dns: { private: true, domain: lab.internal }"), Vec::<String>::new());
        let one = problems("dns: { private: true, domain: lab }");
        assert!(one.iter().any(|p| p.starts_with("dns.domain: `lab` isn't a domain")), "{one:?}");
        let upper = problems("dns: { private: true, domain: Lab.Internal }");
        assert!(upper.iter().any(|p| p.contains("lowercase")), "{upper:?}");
    }

    #[test]
    fn a_domain_needs_private_dns_on() {
        let p = problems("dns: { domain: lab.internal }");
        assert_eq!(p, ["dns.domain: the domain names the private zone: set `dns.private: true`, or drop `domain`"]);
    }

    #[test]
    fn the_domain_defaults_to_the_environment_name() {
        let spec: Spec = serde_yaml_ng::from_str("version: 1\nname: lab\ndns: { private: true }\n").unwrap();
        assert_eq!(spec.private_domain().as_deref(), Some("lab.internal"));
        let off: Spec = serde_yaml_ng::from_str("version: 1\nname: lab\n").unwrap();
        assert_eq!(off.private_domain(), None);
    }

    #[test]
    fn options_are_names_or_name_value_pairs() {
        for ok in ["ndots:2", "timeout:1", "attempts:3", "rotate", "edns0", "single-request-reopen", "trust-ad"] {
            assert!(option(ok), "{ok}");
        }
        for bad in ["", "ndots:", "ndots: 2", "NDOTS:2", "options ndots:2", "ndots:two", ":2", "2:3"] {
            assert!(!option(bad), "{bad}");
        }
    }
}
