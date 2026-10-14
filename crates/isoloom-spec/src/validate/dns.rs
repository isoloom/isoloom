//! A machine's resolver options (`dns.options`): each one resolv.conf's `options` line takes,
//! as `name` or `name:value`, so every target writes it as it is.

use super::Problems;
use crate::model::Spec;

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
