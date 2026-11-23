//! A rendered Ansible inventory (INI), read as Ansible reads it.

use std::collections::BTreeMap;

/// The groups of inventory `text`, each with its lines (a host and its variables, or a
/// `[group:vars]` value), comments and blank lines left out, in order.
#[must_use]
pub fn ini_groups(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut group = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            name.clone_into(&mut group);
            out.entry(group.clone()).or_default();
        } else {
            out.entry(group.clone()).or_default().push(line.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inventory_is_read_as_its_groups_lines_in_order() {
        let groups = ini_groups(
            "# written by isoloom\nlocal ansible_connection=local\n\n[web]\nweb1 ansible_host=10.0.0.2\n  web2\n[web:vars]\nhttp_port=80\n[empty]\n",
        );
        let expected = [
            ("", vec!["local ansible_connection=local"]),
            ("empty", vec![]),
            ("web", vec!["web1 ansible_host=10.0.0.2", "web2"]),
            ("web:vars", vec!["http_port=80"]),
        ];
        let expected: BTreeMap<String, Vec<String>> = expected
            .into_iter()
            .map(|(g, lines)| (g.to_owned(), lines.into_iter().map(str::to_owned).collect()))
            .collect();
        assert_eq!(groups, expected);
    }
}
